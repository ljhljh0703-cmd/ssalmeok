use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandEvent;

use super::{
    AppSnapshot, RuntimeState, format_reset_countdown, krw_menu_text, limit_menu_text,
    min_remaining, most_constrained_provider, preferred_window, provider_by_id,
    refresh_snapshot_internal, show_main_window, toggle_main_window, token_menu_text,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayPayload {
    icon: String,
    title: String,
    tooltip: String,
    codex_lines: Vec<String>,
    claude_lines: Vec<String>,
    cost_note: String,
}

pub(super) fn start(app: &AppHandle, snapshot: &AppSnapshot) -> Result<(), String> {
    let state = app.state::<RuntimeState>();
    let mut child_slot = state
        .menubar_child
        .lock()
        .map_err(|_| "메뉴 막대 도우미 상태를 열지 못했습니다.".to_string())?;
    if child_slot.is_some() {
        return Ok(());
    }

    let codex_icon = resolve_icon(app, "tray-codex.png")?;
    let claude_icon = resolve_icon(app, "tray-claude.png")?;
    let (mut events, mut child) = app
        .shell()
        .sidecar("ssalmeok-menubar")
        .map_err(|_| "AppKit 메뉴 막대 도우미를 찾지 못했습니다.".to_string())?
        .args([
            codex_icon.to_string_lossy().to_string(),
            claude_icon.to_string_lossy().to_string(),
        ])
        .spawn()
        .map_err(|_| "AppKit 메뉴 막대 도우미를 실행하지 못했습니다.".to_string())?;

    let line = payload_line(snapshot)?;
    if child.write(&line).is_err() {
        let _ = child.kill();
        return Err("메뉴 막대 도우미에 첫 상태를 보내지 못했습니다.".to_string());
    }
    *child_slot = Some(child);
    drop(child_slot);

    let event_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    handle_action(&event_app, String::from_utf8_lossy(&bytes).trim());
                }
                CommandEvent::Stderr(bytes) => {
                    let message = String::from_utf8_lossy(&bytes);
                    eprintln!("menubar helper: {}", message.trim());
                }
                CommandEvent::Error(error) => {
                    eprintln!("menubar helper stream error: {error}");
                }
                CommandEvent::Terminated(_) => break,
                _ => {}
            }
        }

        if let Ok(mut slot) = event_app.state::<RuntimeState>().menubar_child.lock() {
            slot.take();
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let snapshot = event_app
            .state::<RuntimeState>()
            .snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default();
        if let Err(error) = start(&event_app, &snapshot) {
            eprintln!("failed to restart the menubar helper: {error}");
        }
    });

    Ok(())
}

pub(super) fn update(app: &AppHandle, snapshot: &AppSnapshot) {
    let line = match payload_line(snapshot) {
        Ok(line) => line,
        Err(error) => {
            eprintln!("failed to encode menubar state: {error}");
            return;
        }
    };

    let state = app.state::<RuntimeState>();
    let mut child_slot = match state.menubar_child.lock() {
        Ok(slot) => slot,
        Err(_) => return,
    };
    if let Some(child) = child_slot.as_mut() {
        if child.write(&line).is_err() {
            eprintln!("failed to update the menubar helper");
        }
        return;
    }
    drop(child_slot);
    if let Err(error) = start(app, snapshot) {
        eprintln!("failed to start the menubar helper: {error}");
    }
}

fn handle_action(app: &AppHandle, action: &str) {
    match action {
        "toggle" => {
            let target = app.clone();
            let _ = app.run_on_main_thread(move || toggle_main_window(&target));
        }
        "open" => {
            let target = app.clone();
            let _ = app.run_on_main_thread(move || show_main_window(&target));
        }
        "refresh" => {
            let target = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = refresh_snapshot_internal(&target, true).await;
            });
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

fn resolve_icon(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|_| "앱 리소스 폴더를 찾지 못했습니다.".to_string())?
        .join("icons")
        .join(name);
    if bundled.is_file() {
        return Ok(bundled);
    }

    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("icons")
        .join(name);
    if source.is_file() {
        return Ok(source);
    }
    Err(format!("서비스 아이콘을 찾지 못했습니다: {name}"))
}

fn payload_line(snapshot: &AppSnapshot) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec(&build_payload(snapshot))
        .map_err(|_| "메뉴 막대 상태를 만들지 못했습니다.".to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn build_payload(snapshot: &AppSnapshot) -> TrayPayload {
    let selected = most_constrained_provider(snapshot);
    let icon = if selected.is_some_and(|provider| provider.id == "claude") {
        "claude"
    } else {
        "codex"
    };
    let title = selected
        .and_then(min_remaining)
        .map(|remaining| format!("{remaining:.0}"))
        .unwrap_or_else(|| "…".to_string());
    let tooltip = match (
        provider_by_id(snapshot, "codex").and_then(min_remaining),
        provider_by_id(snapshot, "claude").and_then(min_remaining),
    ) {
        (Some(codex), Some(claude)) => {
            format!("Codex {codex:.0}% · Claude {claude:.0}% 남음")
        }
        _ => "Codex · Claude 남은 사용량을 읽는 중".to_string(),
    };

    let codex = provider_by_id(snapshot, "codex");
    let claude = provider_by_id(snapshot, "claude");
    let exchange_rate = snapshot.exchange_rate.as_ref();
    let codex_weekly =
        codex.and_then(|provider| preferred_window(provider, "codex-secondary", 10_080));
    let claude_five_hour =
        claude.and_then(|provider| preferred_window(provider, "claude-primary", 300));
    let claude_weekly =
        claude.and_then(|provider| preferred_window(provider, "claude-secondary", 10_080));

    TrayPayload {
        icon: icon.to_string(),
        title,
        tooltip,
        codex_lines: vec![
            codex
                .and_then(min_remaining)
                .map(|remaining| format!("Codex  ·  {remaining:.0}% 남음"))
                .unwrap_or_else(|| "Codex  ·  읽는 중".to_string()),
            limit_menu_text("주간 한도", codex_weekly),
            format!(
                "초기화까지  ·  {}",
                format_reset_countdown(codex_weekly.and_then(|window| window.resets_at.as_deref()))
            ),
            token_menu_text(codex),
            krw_menu_text(codex, exchange_rate),
            codex
                .and_then(|provider| provider.reset_credits.as_ref())
                .map(|credits| format!("리셋권  ·  {}장", credits.available_count))
                .unwrap_or_else(|| "리셋권  ·  확인 중".to_string()),
        ],
        claude_lines: vec![
            claude
                .and_then(min_remaining)
                .map(|remaining| format!("Claude  ·  {remaining:.0}% 남음"))
                .unwrap_or_else(|| "Claude  ·  읽는 중".to_string()),
            limit_menu_text("5시간 한도", claude_five_hour),
            format!(
                "5시간 초기화까지  ·  {}",
                format_reset_countdown(
                    claude_five_hour.and_then(|window| window.resets_at.as_deref())
                )
            ),
            limit_menu_text("주간 한도", claude_weekly),
            format!(
                "주간 초기화까지  ·  {}",
                format_reset_countdown(
                    claude_weekly.and_then(|window| window.resets_at.as_deref())
                )
            ),
            token_menu_text(claude),
            krw_menu_text(claude, exchange_rate),
        ],
        cost_note: "※ 실제 청구액이 아닌 개발자용 정가 환산".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_contract_matches_the_swift_helper() {
        let line = payload_line(&AppSnapshot::default()).expect("payload should serialize");
        assert_eq!(line.last(), Some(&b'\n'));
        let payload: serde_json::Value =
            serde_json::from_slice(&line).expect("payload should be valid JSON");
        for key in [
            "icon",
            "title",
            "tooltip",
            "codexLines",
            "claudeLines",
            "costNote",
        ] {
            assert!(payload.get(key).is_some(), "missing helper field: {key}");
        }
    }
}
