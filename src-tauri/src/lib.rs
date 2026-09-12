use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_shell::ShellExt;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use uuid::Uuid;

const CODEXBAR_VERSION: &str = "0.59.0";
const REFRESH_INTERVAL_SECONDS: u64 = 60;
const COST_REFRESH_INTERVAL_SECONDS: u64 = 10 * 60;
const MACOS_TRAY_ITEM_WIDTH: f64 = 50.0;
const STALE_AFTER_SECONDS: u64 = 180;
const RESET_CONFIRMATION: &str = "RESET_ONE_CREDIT";
const EXCHANGE_RATE_CACHE_SECONDS: u64 = 6 * 60 * 60;
const ECB_DAILY_RATES_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct UsageWindow {
    id: String,
    label: String,
    used_percent: f64,
    remaining_percent: f64,
    resets_at: Option<String>,
    window_minutes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct UsageBreakdown {
    label: String,
    tokens: u64,
    estimated_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct CostSummary {
    recent_date: Option<String>,
    recent_tokens: Option<u64>,
    recent_cost_usd: Option<f64>,
    last_30_days_tokens: Option<u64>,
    last_30_days_cost_usd: Option<f64>,
    estimated: bool,
    #[serde(default)]
    history_days: Option<u64>,
    #[serde(default)]
    projects: Vec<UsageBreakdown>,
    #[serde(default)]
    models: Vec<UsageBreakdown>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ResetCredits {
    available_count: u64,
    nearest_expiry: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ExchangeRate {
    krw_per_usd: f64,
    reference_date: String,
    fetched_at_epoch_seconds: u64,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ProviderSnapshot {
    id: String,
    name: String,
    source: String,
    windows: Vec<UsageWindow>,
    cost: Option<CostSummary>,
    reset_credits: Option<ResetCredits>,
    last_success_at_epoch_seconds: u64,
    stale: bool,
    error: Option<String>,
}

impl ProviderSnapshot {
    fn empty(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: provider_name(id).to_string(),
            source: "local".to_string(),
            windows: Vec::new(),
            cost: None,
            reset_credits: None,
            last_success_at_epoch_seconds: 0,
            stale: true,
            error: Some("첫 사용량을 읽는 중입니다.".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    generated_at_epoch_seconds: u64,
    last_attempt_at_epoch_seconds: u64,
    #[serde(default)]
    last_cost_scan_at_epoch_seconds: u64,
    stale_after_seconds: u64,
    refresh_interval_seconds: u64,
    sidecar_version: String,
    providers: Vec<ProviderSnapshot>,
    #[serde(default)]
    exchange_rate: Option<ExchangeRate>,
    global_error: Option<String>,
}

impl Default for AppSnapshot {
    fn default() -> Self {
        Self {
            generated_at_epoch_seconds: 0,
            last_attempt_at_epoch_seconds: 0,
            last_cost_scan_at_epoch_seconds: 0,
            stale_after_seconds: STALE_AFTER_SECONDS,
            refresh_interval_seconds: REFRESH_INTERVAL_SECONDS,
            sidecar_version: CODEXBAR_VERSION.to_string(),
            providers: vec![
                ProviderSnapshot::empty("codex"),
                ProviderSnapshot::empty("claude"),
            ],
            exchange_rate: None,
            global_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResetResult {
    outcome: String,
    message: String,
    reused_pending_attempt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetAttempt {
    idempotency_key: String,
    status: String,
    outcome: Option<String>,
    created_at_epoch_seconds: u64,
}

struct RuntimeState {
    snapshot: Mutex<AppSnapshot>,
    refresh_guard: tokio::sync::Mutex<()>,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            snapshot: Mutex::new(AppSnapshot::default()),
            refresh_guard: tokio::sync::Mutex::new(()),
        }
    }
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn provider_name(id: &str) -> &'static str {
    match id {
        "codex" => "Codex",
        "claude" => "Claude",
        _ => "AI",
    }
}

fn friendly_window_label(id: &str, title: Option<&str>, minutes: Option<u64>) -> String {
    match id {
        "codex-spark" => "Spark 5시간".to_string(),
        "codex-spark-weekly" => "Spark 주간".to_string(),
        "claude-weekly-scoped-fable" => "Fable 주간".to_string(),
        _ => {
            if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
                return title.to_string();
            }
            match minutes {
                Some(300) => "5시간 한도".to_string(),
                Some(10_080) => "주간 한도".to_string(),
                Some(value) if value % 1_440 == 0 => format!("{}일 한도", value / 1_440),
                Some(value) if value % 60 == 0 => format!("{}시간 한도", value / 60),
                _ => "사용 한도".to_string(),
            }
        }
    }
}

fn parse_window(id: &str, title: Option<&str>, value: &Value) -> Option<UsageWindow> {
    let used = value.get("usedPercent")?.as_f64()?.clamp(0.0, 100.0);
    let minutes = value.get("windowMinutes").and_then(Value::as_u64);
    Some(UsageWindow {
        id: id.to_string(),
        label: friendly_window_label(id, title, minutes),
        used_percent: used,
        remaining_percent: (100.0 - used).clamp(0.0, 100.0),
        resets_at: value
            .get("resetsAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        window_minutes: minutes,
    })
}

fn parse_usage(raw: &str) -> Result<HashMap<String, ProviderSnapshot>, String> {
    let rows: Value = serde_json::from_str(raw)
        .map_err(|_| "보조 프로그램의 사용량 응답을 읽지 못했습니다.".to_string())?;
    let rows = rows
        .as_array()
        .ok_or_else(|| "보조 프로그램의 사용량 형식이 예상과 다릅니다.".to_string())?;

    let mut providers = HashMap::new();
    for row in rows {
        let Some(id) = row.get("provider").and_then(Value::as_str) else {
            continue;
        };
        if id != "codex" && id != "claude" {
            continue;
        }
        let null_usage = Value::Null;
        let usage = row.get("usage").unwrap_or(&null_usage);
        let mut windows = Vec::new();
        if let Some(value) = usage.get("primary")
            && let Some(window) = parse_window(&format!("{id}-primary"), None, value)
        {
            windows.push(window);
        }
        if let Some(value) = usage.get("secondary")
            && let Some(window) = parse_window(&format!("{id}-secondary"), None, value)
        {
            windows.push(window);
        }
        if let Some(extra_windows) = usage.get("extraRateWindows").and_then(Value::as_array) {
            for extra in extra_windows {
                let extra_id = extra
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("extra-window");
                let title = extra.get("title").and_then(Value::as_str);
                if let Some(window_value) = extra.get("window")
                    && let Some(window) = parse_window(extra_id, title, window_value)
                {
                    windows.push(window);
                }
            }
        }
        windows.sort_by_key(|window| window.window_minutes.unwrap_or(u64::MAX));

        let reset_credits = usage.get("codexResetCredits").and_then(|reset| {
            let available_count = reset.get("availableCount")?.as_u64()?;
            let nearest = reset
                .get("credits")
                .and_then(Value::as_array)
                .and_then(|credits| {
                    credits
                        .iter()
                        .filter(|credit| {
                            credit.get("status").and_then(Value::as_str) == Some("available")
                        })
                        .min_by_key(|credit| {
                            credit
                                .get("expires_at")
                                .or_else(|| credit.get("expiresAt"))
                                .and_then(Value::as_str)
                                .unwrap_or("9999")
                        })
                });
            Some(ResetCredits {
                available_count,
                nearest_expiry: nearest
                    .and_then(|credit| credit.get("expires_at").or_else(|| credit.get("expiresAt")))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                title: nearest
                    .and_then(|credit| credit.get("title"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        });

        let error = if row.get("error").is_some_and(|value| !value.is_null()) {
            Some("로그인 또는 사용량 연결을 확인해 주세요.".to_string())
        } else if windows.is_empty() {
            Some("표시할 사용 한도를 받지 못했습니다.".to_string())
        } else {
            None
        };

        providers.insert(
            id.to_string(),
            ProviderSnapshot {
                id: id.to_string(),
                name: provider_name(id).to_string(),
                source: row
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("local")
                    .to_string(),
                windows,
                cost: None,
                reset_credits,
                last_success_at_epoch_seconds: 0,
                stale: error.is_some(),
                error,
            },
        );
    }
    Ok(providers)
}

fn safe_project_label(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return "분류되지 않은 작업".to_string();
    }
    if trimmed.starts_with('/') {
        return Path::new(trimmed)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("분류되지 않은 작업")
            .to_string();
    }
    trimmed.to_string()
}

fn aggregate_breakdowns<'a, I>(
    rows: I,
    label_key: &str,
    token_key: &str,
    cost_key: &str,
    project_labels: bool,
) -> Vec<UsageBreakdown>
where
    I: IntoIterator<Item = &'a Value>,
{
    let mut totals: HashMap<String, (u64, f64, bool)> = HashMap::new();
    for row in rows {
        let Some(raw_label) = row.get(label_key).and_then(Value::as_str) else {
            continue;
        };
        let label = if project_labels {
            safe_project_label(raw_label)
        } else if raw_label.trim().is_empty() {
            "알 수 없는 모델".to_string()
        } else {
            raw_label.trim().to_string()
        };
        let tokens = row.get(token_key).and_then(Value::as_u64).unwrap_or(0);
        let cost = row.get(cost_key).and_then(Value::as_f64);
        if tokens == 0 && cost.unwrap_or(0.0) == 0.0 {
            continue;
        }
        let entry = totals.entry(label).or_insert((0, 0.0, false));
        entry.0 = entry.0.saturating_add(tokens);
        if let Some(cost) = cost {
            entry.1 += cost;
            entry.2 = true;
        }
    }

    let mut breakdowns: Vec<_> = totals
        .into_iter()
        .map(|(label, (tokens, cost, has_cost))| UsageBreakdown {
            label,
            tokens,
            estimated_cost_usd: has_cost.then_some(cost),
        })
        .collect();
    breakdowns.sort_by(|left, right| right.tokens.cmp(&left.tokens));
    breakdowns.truncate(50);
    breakdowns
}

fn parse_cost(raw: &str) -> Result<HashMap<String, CostSummary>, String> {
    let rows: Value = serde_json::from_str(raw)
        .map_err(|_| "보조 프로그램의 토큰 비용 응답을 읽지 못했습니다.".to_string())?;
    let rows = rows
        .as_array()
        .ok_or_else(|| "보조 프로그램의 토큰 비용 형식이 예상과 다릅니다.".to_string())?;
    let mut costs = HashMap::new();

    for row in rows {
        let Some(id) = row.get("provider").and_then(Value::as_str) else {
            continue;
        };
        if id != "codex" && id != "claude" {
            continue;
        }
        if row.get("error").is_some_and(|value| !value.is_null()) {
            continue;
        }
        let recent = row.get("daily").and_then(Value::as_array).and_then(|days| {
            days.iter()
                .max_by_key(|day| day.get("date").and_then(Value::as_str))
        });
        let projects = aggregate_breakdowns(
            row.get("projects")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
            "name",
            "totalTokens",
            "totalCost",
            true,
        );
        let top_level_models: Vec<&Value> = row
            .get("modelBreakdowns")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .collect();
        let daily_models: Vec<&Value> = row
            .get("daily")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|day| day.get("modelBreakdowns").and_then(Value::as_array))
            .flatten()
            .collect();
        let models = if top_level_models.is_empty() {
            aggregate_breakdowns(daily_models, "modelName", "totalTokens", "cost", false)
        } else {
            aggregate_breakdowns(top_level_models, "modelName", "totalTokens", "cost", false)
        };

        costs.insert(
            id.to_string(),
            CostSummary {
                recent_date: recent
                    .and_then(|day| day.get("date"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                recent_tokens: recent
                    .and_then(|day| day.get("totalTokens"))
                    .and_then(Value::as_u64)
                    .or_else(|| row.get("sessionTokens").and_then(Value::as_u64)),
                recent_cost_usd: recent
                    .and_then(|day| day.get("totalCost"))
                    .and_then(Value::as_f64)
                    .or_else(|| row.get("sessionCostUSD").and_then(Value::as_f64)),
                last_30_days_tokens: row.get("last30DaysTokens").and_then(Value::as_u64),
                last_30_days_cost_usd: row.get("last30DaysCostUSD").and_then(Value::as_f64),
                estimated: true,
                history_days: row.get("historyDays").and_then(Value::as_u64),
                projects,
                models,
            },
        );
    }
    Ok(costs)
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "쌀먹 데이터 폴더를 찾지 못했습니다.".to_string())?;
    fs::create_dir_all(&path).map_err(|_| "쌀먹 데이터 폴더를 만들지 못했습니다.".to_string())?;
    Ok(path)
}

fn snapshot_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("last-snapshot.json"))
}

fn reset_attempt_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("reset-attempt.json"))
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| "로컬 상태를 정리하지 못했습니다.".to_string())?;
    fs::write(&temporary, bytes)
        .map_err(|_| "로컬 상태를 임시 저장하지 못했습니다.".to_string())?;
    fs::rename(&temporary, path).map_err(|_| "로컬 상태 저장을 마무리하지 못했습니다.".to_string())
}

fn load_cached_snapshot(app: &AppHandle) -> Option<AppSnapshot> {
    let path = snapshot_path(app).ok()?;
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

async fn run_codexbar(app: &AppHandle, args: &[&str]) -> Result<String, String> {
    let output = app
        .shell()
        .sidecar("codexbar")
        .map_err(|_| "포함된 사용량 보조 프로그램을 찾지 못했습니다.".to_string())?
        .args(args)
        .output()
        .await
        .map_err(|_| "사용량 보조 프로그램을 실행하지 못했습니다.".to_string())?;

    if !output.status.success() {
        return Err("사용량 보조 프로그램이 정상적으로 끝나지 않았습니다.".to_string());
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "사용량 보조 프로그램의 문자 형식을 읽지 못했습니다.".to_string())
}

fn extract_ecb_rate(xml: &str, currency: &str) -> Option<f64> {
    for quote in ['\'', '"'] {
        let marker = format!("currency={quote}{currency}{quote} rate={quote}");
        if let Some(marker_start) = xml.find(&marker) {
            let value = &xml[marker_start + marker.len()..];
            if let Some(end) = value.find(quote)
                && let Ok(rate) = value[..end].parse::<f64>()
            {
                return Some(rate);
            }
        }
    }
    None
}

fn extract_ecb_reference_date(xml: &str) -> Option<String> {
    for quote in ['\'', '"'] {
        let marker = format!("time={quote}");
        if let Some(marker_start) = xml.find(&marker) {
            let value = &xml[marker_start + marker.len()..];
            if let Some(end) = value.find(quote) {
                let date = &value[..end];
                if date.len() == 10 {
                    return Some(date.to_string());
                }
            }
        }
    }
    None
}

fn parse_ecb_exchange_rate(xml: &str, fetched_at: u64) -> Result<ExchangeRate, String> {
    let usd_per_eur = extract_ecb_rate(xml, "USD")
        .ok_or_else(|| "달러 기준 환율을 읽지 못했습니다.".to_string())?;
    let krw_per_eur = extract_ecb_rate(xml, "KRW")
        .ok_or_else(|| "원화 기준 환율을 읽지 못했습니다.".to_string())?;
    let krw_per_usd = krw_per_eur / usd_per_eur;
    if !(500.0..=3_000.0).contains(&krw_per_usd) {
        return Err("원화 환율이 안전 범위를 벗어났습니다.".to_string());
    }
    Ok(ExchangeRate {
        krw_per_usd,
        reference_date: extract_ecb_reference_date(xml)
            .unwrap_or_else(|| "기준일 미상".to_string()),
        fetched_at_epoch_seconds: fetched_at,
        source: "유럽중앙은행".to_string(),
    })
}

async fn fetch_exchange_rate(previous: Option<&ExchangeRate>, now: u64) -> Option<ExchangeRate> {
    if let Some(previous) = previous
        && now.saturating_sub(previous.fetched_at_epoch_seconds) < EXCHANGE_RATE_CACHE_SECONDS
    {
        return Some(previous.clone());
    }

    let request = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "10",
            "--user-agent",
            "ssalmeok/0.1",
            ECB_DAILY_RATES_URL,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let output = tokio::time::timeout(Duration::from_secs(12), request)
        .await
        .ok()
        .and_then(Result::ok);

    output
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|xml| parse_ecb_exchange_rate(&xml, now).ok())
        .or_else(|| previous.cloned())
}

fn previous_provider<'a>(snapshot: &'a AppSnapshot, id: &str) -> Option<&'a ProviderSnapshot> {
    snapshot.providers.iter().find(|provider| provider.id == id)
}

fn merge_provider_snapshot(
    id: &str,
    previous: Option<&ProviderSnapshot>,
    fresh: Option<&ProviderSnapshot>,
    fresh_cost: Option<&CostSummary>,
    usage_error: Option<String>,
    cost_error: Option<String>,
    attempted_at: u64,
) -> ProviderSnapshot {
    let fresh_was_present = fresh.is_some();
    let mut provider = fresh
        .cloned()
        .unwrap_or_else(|| ProviderSnapshot::empty(id));
    let usage_succeeded = provider.error.is_none() && !provider.windows.is_empty();

    if usage_succeeded {
        provider.last_success_at_epoch_seconds = attempted_at;
        provider.stale = false;
    } else if let Some(old) = previous {
        provider.windows = old.windows.clone();
        provider.reset_credits = old.reset_credits.clone();
        provider.last_success_at_epoch_seconds = old.last_success_at_epoch_seconds;
        provider.stale = true;
    }

    if let Some(cost) = fresh_cost {
        provider.cost = Some(cost.clone());
    } else if let Some(old) = previous {
        provider.cost = old.cost.clone();
        if provider.error.is_none() {
            provider.error = Some("토큰 비용 갱신이 늦어지고 있습니다.".to_string());
        }
    }

    if !usage_succeeded && !fresh_was_present {
        provider.error =
            Some(usage_error.unwrap_or_else(|| "사용량을 새로 읽지 못했습니다.".to_string()));
    } else if !usage_succeeded && provider.error.is_none() {
        provider.error = usage_error;
    }
    if fresh_cost.is_none() && provider.error.is_none() {
        provider.error =
            Some(cost_error.unwrap_or_else(|| "토큰 비용을 읽지 못했습니다.".to_string()));
    }
    if provider.last_success_at_epoch_seconds > 0
        && attempted_at.saturating_sub(provider.last_success_at_epoch_seconds) > STALE_AFTER_SECONDS
    {
        provider.stale = true;
    }
    provider
}

fn should_scan_cost(previous: &AppSnapshot, attempted_at: u64, force_refresh: bool) -> bool {
    force_refresh
        || previous.last_cost_scan_at_epoch_seconds == 0
        || attempted_at.saturating_sub(previous.last_cost_scan_at_epoch_seconds)
            >= COST_REFRESH_INTERVAL_SECONDS
}

async fn build_snapshot(
    app: &AppHandle,
    previous: &AppSnapshot,
    force_cost_refresh: bool,
) -> AppSnapshot {
    let attempted_at = now_epoch_seconds();
    let should_scan_cost = should_scan_cost(previous, attempted_at, force_cost_refresh);
    let codex_usage_future = run_codexbar(
        app,
        &[
            "usage",
            "--format",
            "json",
            "--provider",
            "codex",
            "--source",
            "oauth",
            "--no-color",
        ],
    );
    let claude_usage_future = run_codexbar(
        app,
        &[
            "usage",
            "--format",
            "json",
            "--provider",
            "claude",
            "--source",
            "cli",
            "--no-color",
        ],
    );
    let cost_future = async {
        if should_scan_cost {
            Some(
                run_codexbar(
                    app,
                    &[
                        "cost",
                        "--format",
                        "json",
                        "--provider",
                        "both",
                        "--no-color",
                    ],
                )
                .await,
            )
        } else {
            None
        }
    };
    let exchange_rate_future = fetch_exchange_rate(previous.exchange_rate.as_ref(), attempted_at);
    let (codex_usage_result, claude_usage_result, cost_result, exchange_rate) = tokio::join!(
        codex_usage_future,
        claude_usage_future,
        cost_future,
        exchange_rate_future
    );

    let parsed_codex_usage = codex_usage_result
        .as_ref()
        .map_err(|error| error.clone())
        .and_then(|raw| parse_usage(raw));
    let parsed_claude_usage = claude_usage_result
        .as_ref()
        .map_err(|error| error.clone())
        .and_then(|raw| parse_usage(raw));
    let parsed_cost = cost_result.map(|result| {
        result
            .as_ref()
            .map_err(|error| error.clone())
            .and_then(|raw| parse_cost(raw))
    });

    let codex_usage_error = parsed_codex_usage.as_ref().err().cloned();
    let claude_usage_error = parsed_claude_usage.as_ref().err().cloned();
    let cost_error = parsed_cost
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .cloned();

    let mut usage_map = parsed_codex_usage.ok().unwrap_or_default();
    usage_map.extend(parsed_claude_usage.ok().unwrap_or_default());
    let cost_map = match parsed_cost {
        Some(Ok(costs)) => costs,
        Some(Err(_)) => HashMap::new(),
        None => previous
            .providers
            .iter()
            .filter_map(|provider| {
                provider
                    .cost
                    .as_ref()
                    .map(|cost| (provider.id.clone(), cost.clone()))
            })
            .collect(),
    };
    let mut providers = Vec::new();

    for id in ["codex", "claude"] {
        let usage_error = match id {
            "codex" => codex_usage_error.clone(),
            "claude" => claude_usage_error.clone(),
            _ => None,
        };
        providers.push(merge_provider_snapshot(
            id,
            previous_provider(previous, id),
            usage_map.get(id),
            cost_map.get(id),
            usage_error,
            cost_error.clone(),
            attempted_at,
        ));
    }

    let all_unavailable = providers
        .iter()
        .all(|provider| provider.last_success_at_epoch_seconds == 0);
    let global_error = if all_unavailable {
        codex_usage_error.or(claude_usage_error).or(cost_error)
    } else {
        None
    };

    AppSnapshot {
        generated_at_epoch_seconds: attempted_at,
        last_attempt_at_epoch_seconds: attempted_at,
        last_cost_scan_at_epoch_seconds: if should_scan_cost {
            attempted_at
        } else {
            previous.last_cost_scan_at_epoch_seconds
        },
        stale_after_seconds: STALE_AFTER_SECONDS,
        refresh_interval_seconds: REFRESH_INTERVAL_SECONDS,
        sidecar_version: CODEXBAR_VERSION.to_string(),
        providers,
        exchange_rate,
        global_error,
    }
}

fn min_remaining(provider: &ProviderSnapshot) -> Option<f64> {
    provider
        .windows
        .iter()
        .map(|window| window.remaining_percent)
        .reduce(f64::min)
}

fn tray_title(provider: Option<&ProviderSnapshot>) -> String {
    match provider.and_then(min_remaining) {
        Some(value) => format!("{:.0}", value),
        None => "--".to_string(),
    }
}

fn format_integer(value: u64) -> String {
    let digits = value.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(character);
    }
    formatted
}

fn format_reset_countdown(resets_at: Option<&str>) -> String {
    let Some(resets_at) = resets_at else {
        return "확인 불가".to_string();
    };
    let Ok(reset) = DateTime::parse_from_rfc3339(resets_at) else {
        return "확인 불가".to_string();
    };
    let seconds = (reset.with_timezone(&Utc) - Utc::now()).num_seconds();
    if seconds <= 0 {
        return "초기화 확인 중".to_string();
    }
    let total_minutes = (seconds + 59) / 60;
    let days = total_minutes / 1_440;
    let hours = (total_minutes % 1_440) / 60;
    let minutes = total_minutes % 60;
    if days > 0 {
        format!("{days}일 {hours}시간")
    } else if hours > 0 {
        format!("{hours}시간 {minutes}분")
    } else {
        format!("{minutes}분")
    }
}

fn preferred_window<'a>(
    provider: &'a ProviderSnapshot,
    preferred_id: &str,
    fallback_minutes: u64,
) -> Option<&'a UsageWindow> {
    provider
        .windows
        .iter()
        .find(|window| window.id == preferred_id)
        .or_else(|| {
            provider
                .windows
                .iter()
                .find(|window| window.window_minutes == Some(fallback_minutes))
        })
}

fn limit_menu_text(label: &str, window: Option<&UsageWindow>) -> String {
    window
        .map(|window| format!("{label}  ·  {:.0}% 남음", window.remaining_percent))
        .unwrap_or_else(|| format!("{label}  ·  확인 불가"))
}

fn reset_menu_text(window: Option<&UsageWindow>) -> String {
    format!(
        "초기화까지  ·  {}",
        format_reset_countdown(window.and_then(|window| window.resets_at.as_deref()))
    )
}

fn token_menu_text(provider: Option<&ProviderSnapshot>) -> String {
    provider
        .and_then(|provider| provider.cost.as_ref())
        .and_then(|cost| cost.recent_tokens)
        .map(|tokens| format!("하루 사용 토큰  ·  {}", format_integer(tokens)))
        .unwrap_or_else(|| "하루 사용 토큰  ·  확인 중".to_string())
}

fn krw_menu_text(
    provider: Option<&ProviderSnapshot>,
    exchange_rate: Option<&ExchangeRate>,
) -> String {
    let converted = provider
        .and_then(|provider| provider.cost.as_ref())
        .and_then(|cost| cost.recent_cost_usd)
        .zip(exchange_rate)
        .map(|(cost, rate)| (cost * rate.krw_per_usd).round());
    match converted {
        Some(value) if value.is_finite() && value >= 0.0 => format!(
            "한화 환산  ·  약 ₩{} (정가 기준)",
            format_integer(value as u64)
        ),
        _ => "한화 환산  ·  환율 확인 중".to_string(),
    }
}

fn info_menu_item(
    app: &AppHandle,
    id: &str,
    text: String,
) -> tauri::Result<tauri::menu::MenuItem<tauri::Wry>> {
    MenuItemBuilder::with_id(id, text).enabled(false).build(app)
}

fn build_combined_tray_menu(
    app: &AppHandle,
    snapshot: Option<&AppSnapshot>,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let codex = snapshot.and_then(|snapshot| provider_by_id(snapshot, "codex"));
    let claude = snapshot.and_then(|snapshot| provider_by_id(snapshot, "claude"));
    let exchange_rate = snapshot.and_then(|snapshot| snapshot.exchange_rate.as_ref());
    let codex_weekly =
        codex.and_then(|provider| preferred_window(provider, "codex-secondary", 10_080));
    let claude_five_hour =
        claude.and_then(|provider| preferred_window(provider, "claude-primary", 300));
    let claude_weekly =
        claude.and_then(|provider| preferred_window(provider, "claude-secondary", 10_080));

    let codex_header = info_menu_item(
        app,
        "codex-header",
        codex
            .and_then(min_remaining)
            .map(|remaining| format!("Codex  ·  {remaining:.0}% 남음"))
            .unwrap_or_else(|| "Codex  ·  읽는 중".to_string()),
    )?;
    let codex_weekly_item = info_menu_item(
        app,
        "codex-weekly",
        limit_menu_text("주간 한도", codex_weekly),
    )?;
    let codex_weekly_reset =
        info_menu_item(app, "codex-weekly-reset", reset_menu_text(codex_weekly))?;
    let codex_tokens = info_menu_item(app, "codex-tokens", token_menu_text(codex))?;
    let codex_krw = info_menu_item(app, "codex-krw", krw_menu_text(codex, exchange_rate))?;
    let codex_reset_credits = info_menu_item(
        app,
        "codex-reset-credits",
        codex
            .and_then(|provider| provider.reset_credits.as_ref())
            .map(|credits| format!("리셋권  ·  {}장", credits.available_count))
            .unwrap_or_else(|| "리셋권  ·  확인 중".to_string()),
    )?;

    let claude_header = info_menu_item(
        app,
        "claude-header",
        claude
            .and_then(min_remaining)
            .map(|remaining| format!("Claude  ·  {remaining:.0}% 남음"))
            .unwrap_or_else(|| "Claude  ·  읽는 중".to_string()),
    )?;
    let claude_five_hour_item = info_menu_item(
        app,
        "claude-five-hour",
        limit_menu_text("5시간 한도", claude_five_hour),
    )?;
    let claude_five_hour_reset = info_menu_item(
        app,
        "claude-five-hour-reset",
        reset_menu_text(claude_five_hour),
    )?;
    let claude_weekly_item = info_menu_item(
        app,
        "claude-weekly",
        limit_menu_text("주간 한도", claude_weekly),
    )?;
    let claude_weekly_reset =
        info_menu_item(app, "claude-weekly-reset", reset_menu_text(claude_weekly))?;
    let claude_tokens = info_menu_item(app, "claude-tokens", token_menu_text(claude))?;
    let claude_krw = info_menu_item(app, "claude-krw", krw_menu_text(claude, exchange_rate))?;
    let cost_note = info_menu_item(
        app,
        "cost-note",
        "※ 실제 청구액이 아닌 개발자용 정가 환산".to_string(),
    )?;

    MenuBuilder::with_id(app, "usage-context-menu")
        .items(&[
            &codex_header,
            &codex_weekly_item,
            &codex_weekly_reset,
            &codex_tokens,
            &codex_krw,
            &codex_reset_credits,
        ])
        .separator()
        .items(&[
            &claude_header,
            &claude_five_hour_item,
            &claude_five_hour_reset,
            &claude_weekly_item,
            &claude_weekly_reset,
            &claude_tokens,
            &claude_krw,
            &cost_note,
        ])
        .separator()
        .text("usage-open", "쌀먹 열기")
        .text("usage-refresh", "지금 갱신")
        .separator()
        .text("usage-quit", "종료")
        .build()
}

fn most_constrained_provider(snapshot: &AppSnapshot) -> Option<&ProviderSnapshot> {
    snapshot
        .providers
        .iter()
        .filter_map(|provider| min_remaining(provider).map(|remaining| (provider, remaining)))
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(provider, _)| provider)
}

fn update_trays(app: &AppHandle, snapshot: &AppSnapshot) {
    let Some(tray) = app.tray_by_id("usage-tray") else {
        return;
    };
    let selected = most_constrained_provider(snapshot);
    let icon = if selected.is_some_and(|provider| provider.id == "claude") {
        tauri::include_image!("./icons/tray-claude.png")
    } else {
        tauri::include_image!("./icons/tray-codex.png")
    };
    let tooltip = match (
        provider_by_id(snapshot, "codex").and_then(min_remaining),
        provider_by_id(snapshot, "claude").and_then(min_remaining),
    ) {
        (Some(codex), Some(claude)) => {
            format!("Codex {codex:.0}% · Claude {claude:.0}% 남음")
        }
        _ => "Codex · Claude 남은 사용량을 읽는 중".to_string(),
    };

    let _ = tray.set_icon_with_as_template(Some(icon), false);
    let _ = tray.set_title(Some(tray_title(selected)));
    let _ = tray.set_tooltip(Some(tooltip));
    if let Ok(menu) = build_combined_tray_menu(app, Some(snapshot)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn provider_by_id<'a>(snapshot: &'a AppSnapshot, id: &str) -> Option<&'a ProviderSnapshot> {
    snapshot.providers.iter().find(|provider| provider.id == id)
}

fn remaining_for_alert(provider: &ProviderSnapshot) -> Option<f64> {
    min_remaining(provider)
}

fn send_notification(app: &AppHandle, body: &str) {
    let _ = app.notification().builder().title("쌀먹").body(body).show();
}

fn maybe_notify(app: &AppHandle, previous: &AppSnapshot, current: &AppSnapshot) {
    for id in ["codex", "claude"] {
        let Some(old) = provider_by_id(previous, id) else {
            continue;
        };
        let Some(new) = provider_by_id(current, id) else {
            continue;
        };
        if old.last_success_at_epoch_seconds == 0 {
            continue;
        }

        if old.error.is_none() && new.error.is_some() {
            send_notification(
                app,
                &format!("{} 사용량을 새로 읽지 못했습니다.", provider_name(id)),
            );
        }

        if let (Some(old_remaining), Some(new_remaining)) =
            (remaining_for_alert(old), remaining_for_alert(new))
        {
            for threshold in [20.0, 5.0, 0.0] {
                if old_remaining > threshold && new_remaining <= threshold {
                    let body = if threshold == 0.0 {
                        format!("{} 사용 한도를 모두 썼습니다.", provider_name(id))
                    } else {
                        format!(
                            "{} 남은 한도가 {:.0}%입니다.",
                            provider_name(id),
                            new_remaining
                        )
                    };
                    send_notification(app, &body);
                    break;
                }
            }
        }

        for old_window in &old.windows {
            let Some(new_window) = new.windows.iter().find(|window| window.id == old_window.id)
            else {
                continue;
            };
            if old_window.resets_at != new_window.resets_at
                && old_window.used_percent - new_window.used_percent >= 5.0
            {
                send_notification(
                    app,
                    &format!(
                        "{}의 {}가 초기화됐습니다.",
                        provider_name(id),
                        new_window.label
                    ),
                );
            }
        }
    }
}

async fn refresh_snapshot_internal(app: &AppHandle, force_cost_refresh: bool) -> AppSnapshot {
    let state = app.state::<RuntimeState>();
    let _guard = state.refresh_guard.lock().await;
    let _ = app.emit("snapshot-refreshing", true);

    let previous = state
        .snapshot
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_default();
    let current = build_snapshot(app, &previous, force_cost_refresh).await;
    maybe_notify(app, &previous, &current);

    if let Ok(path) = snapshot_path(app) {
        let _ = write_json_atomic(&path, &current);
    }
    if let Ok(mut snapshot) = state.snapshot.lock() {
        *snapshot = current.clone();
    }
    update_trays(app, &current);
    let _ = app.emit("snapshot-updated", &current);
    let _ = app.emit("snapshot-refreshing", false);
    current
}

#[tauri::command]
fn get_snapshot(state: State<'_, RuntimeState>) -> AppSnapshot {
    state
        .snapshot
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_default()
}

#[tauri::command]
async fn refresh_snapshot(app: AppHandle) -> AppSnapshot {
    refresh_snapshot_internal(&app, true).await
}

fn find_codex_executable(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("SSALMEOK_CODEX_PATH").map(PathBuf::from)
        && path.is_file()
    {
        return Ok(path);
    }

    let mut candidates = Vec::new();
    if let Ok(home) = app.path().home_dir() {
        candidates.push(home.join(".local/bin/codex"));
        candidates.push(home.join(".cargo/bin/codex"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/codex"));
    candidates.push(PathBuf::from("/usr/local/bin/codex"));

    if let Some(path_value) = env::var_os("PATH") {
        for directory in env::split_paths(&path_value) {
            candidates.push(directory.join("codex"));
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "Codex 명령줄 도구를 찾지 못했습니다.".to_string())
}

async fn send_rpc_line(stdin: &mut tokio::process::ChildStdin, value: Value) -> Result<(), String> {
    let mut line =
        serde_json::to_vec(&value).map_err(|_| "Codex 요청을 만들지 못했습니다.".to_string())?;
    line.push(b'\n');
    stdin
        .write_all(&line)
        .await
        .map_err(|_| "Codex에 요청을 보내지 못했습니다.".to_string())?;
    stdin
        .flush()
        .await
        .map_err(|_| "Codex 요청 전송을 마치지 못했습니다.".to_string())
}

async fn wait_rpc_response<R: AsyncBufRead + Unpin>(
    lines: &mut Lines<R>,
    expected_id: i64,
    wait_seconds: u64,
) -> Result<Value, String> {
    let future = async {
        loop {
            let next = lines
                .next_line()
                .await
                .map_err(|_| "Codex 응답을 읽지 못했습니다.".to_string())?;
            let Some(line) = next else {
                return Err("Codex 연결이 예상보다 일찍 끝났습니다.".to_string());
            };
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.get("id").and_then(Value::as_i64) == Some(expected_id) {
                if let Some(error) = value.get("error") {
                    let message = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Codex가 요청을 거절했습니다.");
                    return Err(message.to_string());
                }
                return Ok(value);
            }
        }
    };

    tokio::time::timeout(Duration::from_secs(wait_seconds), future)
        .await
        .map_err(|_| "Codex 응답 대기 시간이 지났습니다.".to_string())?
}

fn read_or_create_reset_attempt(app: &AppHandle) -> Result<(ResetAttempt, bool), String> {
    let path = reset_attempt_path(app)?;
    if let Ok(bytes) = fs::read(&path)
        && let Ok(attempt) = serde_json::from_slice::<ResetAttempt>(&bytes)
        && attempt.status == "pending"
    {
        return Ok((attempt, true));
    }

    let attempt = ResetAttempt {
        idempotency_key: Uuid::new_v4().to_string(),
        status: "pending".to_string(),
        outcome: None,
        created_at_epoch_seconds: now_epoch_seconds(),
    };
    write_json_atomic(&path, &attempt)?;
    Ok((attempt, false))
}

fn finish_reset_attempt(app: &AppHandle, attempt: &mut ResetAttempt, outcome: &str) {
    attempt.status = "completed".to_string();
    attempt.outcome = Some(outcome.to_string());
    if let Ok(path) = reset_attempt_path(app) {
        let _ = write_json_atomic(&path, attempt);
    }
}

async fn perform_codex_reset(app: &AppHandle) -> Result<ResetResult, String> {
    let codex = find_codex_executable(app)?;
    let (mut attempt, reused_pending_attempt) = read_or_create_reset_attempt(app)?;

    let mut child = tokio::process::Command::new(codex)
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "Codex 공식 연결을 시작하지 못했습니다.".to_string())?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Codex 요청 통로를 열지 못했습니다.".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Codex 응답 통로를 열지 못했습니다.".to_string())?;
    if let Some(mut stderr) = child.stderr.take() {
        tauri::async_runtime::spawn(async move {
            let mut discarded = Vec::new();
            let _ = stderr.read_to_end(&mut discarded).await;
        });
    }
    let mut lines = BufReader::new(stdout).lines();

    send_rpc_line(
        &mut stdin,
        json!({
            "method": "initialize",
            "id": 0,
            "params": {
                "clientInfo": {
                    "name": "ssalmeok",
                    "title": "쌀먹",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
    )
    .await?;
    wait_rpc_response(&mut lines, 0, 15).await?;
    send_rpc_line(&mut stdin, json!({"method": "initialized", "params": {}})).await?;
    send_rpc_line(
        &mut stdin,
        json!({"method": "account/rateLimits/read", "id": 1}),
    )
    .await?;

    let read_response = wait_rpc_response(&mut lines, 1, 20).await?;
    let null_reset_credits = Value::Null;
    let reset_credits = read_response
        .pointer("/result/rateLimitResetCredits")
        .unwrap_or(&null_reset_credits);
    let available = reset_credits
        .get("availableCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if available == 0 {
        finish_reset_attempt(app, &mut attempt, "noCredit");
        let _ = child.kill().await;
        return Ok(ResetResult {
            outcome: "noCredit".to_string(),
            message: "사용 가능한 Codex 초기화권이 없습니다.".to_string(),
            reused_pending_attempt,
        });
    }

    let credit_id = reset_credits
        .get("credits")
        .and_then(Value::as_array)
        .and_then(|credits| {
            credits
                .iter()
                .find(|credit| credit.get("status").and_then(Value::as_str) == Some("available"))
        })
        .and_then(|credit| credit.get("id"))
        .and_then(Value::as_str);
    let mut params = json!({"idempotencyKey": attempt.idempotency_key.clone()});
    if let Some(credit_id) = credit_id {
        params["creditId"] = Value::String(credit_id.to_string());
    }
    send_rpc_line(
        &mut stdin,
        json!({
            "method": "account/rateLimitResetCredit/consume",
            "id": 2,
            "params": params
        }),
    )
    .await?;

    let consume_response = wait_rpc_response(&mut lines, 2, 25).await?;
    let outcome = consume_response
        .pointer("/result/outcome")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    send_rpc_line(
        &mut stdin,
        json!({"method": "account/rateLimits/read", "id": 3}),
    )
    .await?;
    let _ = wait_rpc_response(&mut lines, 3, 20).await;
    let _ = child.kill().await;

    let message = match outcome.as_str() {
        "reset" => "Codex 초기화권 1장을 사용했습니다.",
        "alreadyRedeemed" => "같은 초기화 요청이 이미 처리됐습니다.",
        "nothingToReset" => "현재 초기화할 수 있는 한도 창이 없습니다.",
        "noCredit" => "사용 가능한 Codex 초기화권이 없습니다.",
        _ => "초기화 결과를 확인하지 못했습니다. 같은 요청값으로 다시 확인합니다.",
    }
    .to_string();

    if outcome != "unknown" {
        finish_reset_attempt(app, &mut attempt, &outcome);
    }

    Ok(ResetResult {
        outcome,
        message,
        reused_pending_attempt,
    })
}

#[tauri::command]
async fn consume_codex_reset_credit(
    app: AppHandle,
    confirmation: String,
) -> Result<ResetResult, String> {
    if confirmation != RESET_CONFIRMATION {
        return Err("초기화 확인 문구가 일치하지 않습니다.".to_string());
    }
    let result = perform_codex_reset(&app).await?;
    let app_for_refresh = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = refresh_snapshot_internal(&app_for_refresh, false).await;
    });
    Ok(result)
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    match WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("쌀먹")
        .inner_size(520.0, 760.0)
        .min_inner_size(460.0, 640.0)
        .resizable(true)
        .maximizable(false)
        .fullscreen(false)
        .center()
        .visible(true)
        .build()
    {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(error) => {
            eprintln!("failed to create the main window: {error}");
        }
    }
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.destroy();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    } else {
        show_main_window(app);
    }
}

fn create_trays(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = build_combined_tray_menu(app.handle(), None)?;
    let tray = TrayIconBuilder::with_id("usage-tray")
        .icon(tauri::include_image!("./icons/tray-codex.png"))
        .icon_as_template(false)
        .title("--")
        .tooltip("Codex · Claude 남은 사용량을 읽는 중")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    #[cfg(target_os = "macos")]
    tray.with_inner_tray_icon(|inner| {
        if let Some(status_item) = inner.ns_status_item() {
            status_item.setLength(MACOS_TRAY_ITEM_WIDTH);
            status_item.setVisible(true);
        }
    })?;

    app.on_menu_event(|app, event| match event.id().as_ref() {
        "usage-open" => show_main_window(app),
        "usage-refresh" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = refresh_snapshot_internal(&app, true).await;
            });
        }
        "usage-quit" => app.exit(0),
        _ => {}
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState::new())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let cached_snapshot = load_cached_snapshot(app.handle());
            if let Some(cached) = cached_snapshot.as_ref()
                && let Ok(mut snapshot) = app.state::<RuntimeState>().snapshot.lock()
            {
                *snapshot = cached.clone();
            }
            if !app.autolaunch().is_enabled().unwrap_or(false) {
                let _ = app.autolaunch().enable();
            }

            create_trays(app)?;
            if let Some(cached) = cached_snapshot.as_ref() {
                update_trays(app.handle(), cached);
            }
            let launched_hidden = env::args().any(|argument| argument == "--hidden");
            if !launched_hidden {
                show_main_window(app.handle());
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut refresh_interval =
                    tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL_SECONDS));
                refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    refresh_interval.tick().await;
                    let _ = refresh_snapshot_internal(&app_handle, false).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.destroy();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            refresh_snapshot,
            consume_codex_reset_credit,
            quit_app
        ])
        .build(tauri::generate_context!())
        .expect("쌀먹을 실행하지 못했습니다.")
        .run(|_, event| {
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usage_without_exposing_account_identity() {
        let raw = r#"[
          {
            "provider":"codex",
            "source":"oauth",
            "usage":{
              "accountEmail":"private@example.com",
              "secondary":{"windowMinutes":10080,"usedPercent":69,"resetsAt":"2026-09-15T08:47:07Z"},
              "extraRateWindows":[{"id":"codex-spark","title":"Codex Spark 5-hour","window":{"windowMinutes":300,"usedPercent":11}}],
              "codexResetCredits":{"availableCount":1,"credits":[{"id":"secret-id","status":"available","expires_at":"2026-10-05T04:20:00Z","title":"전체 재설정"}]}
            }
          }
        ]"#;
        let parsed = parse_usage(raw).expect("usage should parse");
        let codex = parsed.get("codex").expect("codex row");
        assert_eq!(codex.windows.len(), 2);
        assert_eq!(codex.windows[0].label, "Spark 5시간");
        assert_eq!(codex.reset_credits.as_ref().unwrap().available_count, 1);

        let public_json = serde_json::to_string(codex).unwrap();
        assert!(!public_json.contains("private@example.com"));
        assert!(!public_json.contains("secret-id"));
    }

    #[test]
    fn parses_recent_and_thirty_day_costs() {
        let raw = r#"[
          {
            "provider":"claude",
            "sessionTokens":50,
            "sessionCostUSD":0.5,
            "last30DaysTokens":900,
            "last30DaysCostUSD":9.0,
            "historyDays":30,
            "daily":[{"date":"2026-09-12","totalTokens":60,"totalCost":0.6,"modelBreakdowns":[{"modelName":"claude-test","totalTokens":60,"cost":0.6}]}],
            "projects":[{"name":"sample-work","totalTokens":60,"totalCost":0.6}]
          }
        ]"#;
        let parsed = parse_cost(raw).expect("cost should parse");
        let claude = parsed.get("claude").unwrap();
        assert_eq!(claude.recent_tokens, Some(60));
        assert_eq!(claude.last_30_days_tokens, Some(900));
        assert_eq!(claude.history_days, Some(30));
        assert_eq!(claude.projects[0].label, "sample-work");
        assert_eq!(claude.models[0].label, "claude-test");
        assert!(claude.estimated);
    }

    #[test]
    fn menu_title_uses_least_remaining_window() {
        let mut provider = ProviderSnapshot::empty("codex");
        provider.windows = vec![
            UsageWindow {
                id: "five-hour".into(),
                label: "5시간".into(),
                used_percent: 12.0,
                remaining_percent: 88.0,
                resets_at: None,
                window_minutes: Some(300),
            },
            UsageWindow {
                id: "weekly".into(),
                label: "주간".into(),
                used_percent: 69.0,
                remaining_percent: 31.0,
                resets_at: None,
                window_minutes: Some(10_080),
            },
        ];
        assert_eq!(tray_title(Some(&provider)), "31");
    }

    #[test]
    fn tray_selects_the_provider_with_least_remaining_usage() {
        let mut codex = ProviderSnapshot::empty("codex");
        codex.windows = vec![UsageWindow {
            id: "codex-weekly".into(),
            label: "주간".into(),
            used_percent: 25.0,
            remaining_percent: 75.0,
            resets_at: None,
            window_minutes: Some(10_080),
        }];
        let mut claude = ProviderSnapshot::empty("claude");
        claude.windows = vec![UsageWindow {
            id: "claude-five-hour".into(),
            label: "5시간".into(),
            used_percent: 70.0,
            remaining_percent: 30.0,
            resets_at: None,
            window_minutes: Some(300),
        }];
        let snapshot = AppSnapshot {
            providers: vec![codex, claude],
            ..AppSnapshot::default()
        };

        assert_eq!(most_constrained_provider(&snapshot).unwrap().id, "claude");
    }

    #[test]
    fn keeps_last_good_values_when_refresh_fails() {
        let mut previous = ProviderSnapshot::empty("claude");
        previous.windows = vec![UsageWindow {
            id: "claude-primary".into(),
            label: "5시간 한도".into(),
            used_percent: 31.0,
            remaining_percent: 69.0,
            resets_at: Some("2026-09-12T09:00:00Z".into()),
            window_minutes: Some(300),
        }];
        previous.cost = Some(CostSummary {
            recent_date: Some("2026-09-12".into()),
            recent_tokens: Some(100),
            recent_cost_usd: Some(1.0),
            last_30_days_tokens: Some(900),
            last_30_days_cost_usd: Some(9.0),
            estimated: true,
            history_days: Some(30),
            projects: Vec::new(),
            models: Vec::new(),
        });
        previous.last_success_at_epoch_seconds = 1_000;
        previous.stale = false;
        previous.error = None;

        let merged = merge_provider_snapshot(
            "claude",
            Some(&previous),
            None,
            None,
            Some("오프라인".into()),
            Some("비용 조회 실패".into()),
            1_060,
        );

        assert_eq!(merged.windows, previous.windows);
        assert_eq!(merged.cost, previous.cost);
        assert_eq!(merged.last_success_at_epoch_seconds, 1_000);
        assert!(merged.stale);
        assert_eq!(merged.error.as_deref(), Some("오프라인"));
    }

    #[test]
    fn parses_ecb_reference_rate_and_formats_won() {
        let xml = r#"<Cube><Cube time='2026-09-11'><Cube currency='USD' rate='1.1592'/><Cube currency='KRW' rate='1556.56'/></Cube></Cube>"#;
        let exchange = parse_ecb_exchange_rate(xml, 1_000).expect("exchange rate should parse");
        assert_eq!(exchange.reference_date, "2026-09-11");
        assert!((exchange.krw_per_usd - 1_342.788).abs() < 0.1);

        let mut provider = ProviderSnapshot::empty("claude");
        provider.cost = Some(CostSummary {
            recent_date: Some("2026-09-12".into()),
            recent_tokens: Some(12_345_678),
            recent_cost_usd: Some(1.0),
            last_30_days_tokens: None,
            last_30_days_cost_usd: None,
            estimated: true,
            history_days: Some(30),
            projects: Vec::new(),
            models: Vec::new(),
        });
        assert_eq!(
            krw_menu_text(Some(&provider), Some(&exchange)),
            "한화 환산  ·  약 ₩1,343 (정가 기준)"
        );
        assert_eq!(
            token_menu_text(Some(&provider)),
            "하루 사용 토큰  ·  12,345,678"
        );
    }

    #[test]
    fn tray_details_prefer_base_windows_over_scoped_windows() {
        let mut provider = ProviderSnapshot::empty("claude");
        provider.windows = vec![
            UsageWindow {
                id: "claude-weekly-scoped-fable".into(),
                label: "Fable 주간".into(),
                used_percent: 80.0,
                remaining_percent: 20.0,
                resets_at: None,
                window_minutes: Some(10_080),
            },
            UsageWindow {
                id: "claude-secondary".into(),
                label: "주간 한도".into(),
                used_percent: 7.0,
                remaining_percent: 93.0,
                resets_at: None,
                window_minutes: Some(10_080),
            },
        ];

        let selected = preferred_window(&provider, "claude-secondary", 10_080).unwrap();
        assert_eq!(selected.id, "claude-secondary");
        assert_eq!(
            limit_menu_text("주간 한도", Some(selected)),
            "주간 한도  ·  93% 남음"
        );
    }

    #[test]
    fn throttles_cost_history_scans_but_allows_manual_refresh() {
        let mut previous = AppSnapshot::default();
        assert!(should_scan_cost(&previous, 1_000, false));

        previous.last_cost_scan_at_epoch_seconds = 1_000;
        assert!(!should_scan_cost(
            &previous,
            1_000 + COST_REFRESH_INTERVAL_SECONDS - 1,
            false
        ));
        assert!(should_scan_cost(
            &previous,
            1_000 + COST_REFRESH_INTERVAL_SECONDS,
            false
        ));
        assert!(should_scan_cost(&previous, 1_001, true));
    }
}
