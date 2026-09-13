use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, DeclaredClass, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSCellImagePosition, NSEvent, NSImage, NSMenu, NSMenuItem,
    NSStatusBar, NSStatusItem, NSVariableStatusItemLength, NSView,
};
use objc2_foundation::{MainThreadMarker, NSData, NSSize, NSString};
use tauri::AppHandle;

use super::{
    AppSnapshot, format_reset_countdown, krw_menu_text, limit_menu_text, min_remaining,
    most_constrained_provider, preferred_window, provider_by_id, token_menu_text,
};

const AUTOSAVE_NAME: &str = "com.godju.ssalmeok.native.v1";
const ICON_SIZE: f64 = 18.0;

const CODEX_ICON: &[u8] = include_bytes!("../icons/tray-codex.png");
const CLAUDE_ICON: &[u8] = include_bytes!("../icons/tray-claude.png");

thread_local! {
    static APP_HANDLE: RefCell<Option<AppHandle>> = const { RefCell::new(None) };
    static TRAY: RefCell<Option<NativeTray>> = const { RefCell::new(None) };
}

#[derive(Debug)]
struct NativeTrayTargetIvars {
    status_item: Retained<NSStatusItem>,
    menu: RefCell<Option<Retained<NSMenu>>>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[name = "SsalmeokNativeTrayTarget"]
    #[thread_kind = MainThreadOnly]
    #[ivars = NativeTrayTargetIvars]
    struct NativeTrayTarget;

    impl NativeTrayTarget {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            self.highlight(true);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            self.highlight(false);
            if let Some(app) = current_app_handle() {
                super::toggle_main_window(&app);
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.highlight(true);
            let menu = self
                .ivars()
                .menu
                .borrow()
                .as_ref()
                .map(Retained::clone);
            if let Some(menu) = menu {
                NSMenu::popUpContextMenu_withEvent_forView(&menu, event, self);
            }
            self.highlight(false);
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, _event: &NSEvent) {
            self.highlight(false);
        }

        #[unsafe(method(openApp:))]
        fn open_app(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = current_app_handle() {
                super::show_main_window(&app);
            }
        }

        #[unsafe(method(refreshUsage:))]
        fn refresh_usage(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = current_app_handle() {
                tauri::async_runtime::spawn(async move {
                    let _ = super::refresh_snapshot_internal(&app, true).await;
                });
            }
        }

        #[unsafe(method(quitApp:))]
        fn quit_app(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = current_app_handle() {
                app.exit(0);
            }
        }
    }
);

impl NativeTrayTarget {
    fn new(
        mtm: MainThreadMarker,
        status_item: Retained<NSStatusItem>,
    ) -> Retained<NativeTrayTarget> {
        let frame = status_item
            .button(mtm)
            .expect("status item button must exist")
            .bounds();
        let this = mtm.alloc().set_ivars(NativeTrayTargetIvars {
            status_item,
            menu: RefCell::new(None),
        });
        let target: Retained<NativeTrayTarget> =
            unsafe { msg_send![super(this), initWithFrame: frame] };
        target.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        target
    }

    fn highlight(&self, highlighted: bool) {
        let mtm = MainThreadMarker::from(self);
        if let Some(button) = self.ivars().status_item.button(mtm) {
            button.highlight(highlighted);
        }
    }

    fn replace_menu(&self, menu: Retained<NSMenu>) {
        self.ivars().menu.replace(Some(menu));
    }
}

struct NativeTray {
    status_item: Retained<NSStatusItem>,
    target: Retained<NativeTrayTarget>,
    codex_icon: Retained<NSImage>,
    claude_icon: Retained<NSImage>,
}

impl NativeTray {
    fn new(mtm: MainThreadMarker, snapshot: Option<&AppSnapshot>) -> Result<Self, &'static str> {
        let codex_icon = decode_icon(CODEX_ICON).ok_or("Codex 트레이 아이콘을 읽지 못했습니다.")?;
        let claude_icon =
            decode_icon(CLAUDE_ICON).ok_or("Claude 트레이 아이콘을 읽지 못했습니다.")?;

        let status_item =
            NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        status_item.setAutosaveName(Some(&NSString::from_str(AUTOSAVE_NAME)));
        status_item.setVisible(true);

        let button = status_item
            .button(mtm)
            .ok_or("macOS 메뉴 막대 버튼을 만들지 못했습니다.")?;
        button.setImagePosition(NSCellImagePosition::ImageLeft);

        let target = NativeTrayTarget::new(mtm, status_item.retain());
        button.addSubview(&target);

        let tray = Self {
            status_item,
            target,
            codex_icon,
            claude_icon,
        };
        tray.apply_snapshot(snapshot, mtm);
        Ok(tray)
    }

    fn apply_snapshot(&self, snapshot: Option<&AppSnapshot>, mtm: MainThreadMarker) {
        let Some(button) = self.status_item.button(mtm) else {
            return;
        };

        let (provider_id, remaining) = snapshot
            .and_then(most_constrained_provider)
            .and_then(|provider| {
                min_remaining(provider).map(|remaining| (provider.id.as_str(), remaining))
            })
            .unwrap_or(("codex", f64::NAN));
        let icon = if provider_id == "claude" {
            &self.claude_icon
        } else {
            &self.codex_icon
        };
        let title = if remaining.is_finite() {
            format!("{remaining:.0}")
        } else {
            "…".to_string()
        };
        button.setImage(Some(icon));
        button.setTitle(&NSString::from_str(&title));
        button.setToolTip(Some(&NSString::from_str(&tray_tooltip(snapshot))));

        self.target.setFrame(button.bounds());
        self.target
            .replace_menu(build_menu(snapshot, &self.target, mtm));
    }
}

pub(super) fn create(app: AppHandle, snapshot: Option<&AppSnapshot>) -> Result<(), &'static str> {
    let mtm = MainThreadMarker::new().ok_or("메인 스레드에서 트레이를 만들지 못했습니다.")?;
    APP_HANDLE.with(|slot| slot.replace(Some(app)));
    TRAY.with(|slot| {
        if slot.borrow().is_some() {
            return Ok(());
        }
        slot.replace(Some(NativeTray::new(mtm, snapshot)?));
        Ok(())
    })
}

pub(super) fn update(snapshot: &AppSnapshot) {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("native tray update skipped outside the main thread");
        return;
    };
    TRAY.with(|slot| {
        if let Some(tray) = slot.borrow().as_ref() {
            tray.apply_snapshot(Some(snapshot), mtm);
        }
    });
}

fn current_app_handle() -> Option<AppHandle> {
    APP_HANDLE.with(|slot| slot.borrow().clone())
}

fn decode_icon(bytes: &[u8]) -> Option<Retained<NSImage>> {
    let data = NSData::from_vec(bytes.to_vec());
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    image.setSize(NSSize::new(ICON_SIZE, ICON_SIZE));
    image.setTemplate(false);
    Some(image)
}

fn tray_tooltip(snapshot: Option<&AppSnapshot>) -> String {
    let codex = snapshot
        .and_then(|snapshot| provider_by_id(snapshot, "codex"))
        .and_then(min_remaining);
    let claude = snapshot
        .and_then(|snapshot| provider_by_id(snapshot, "claude"))
        .and_then(min_remaining);
    match (codex, claude) {
        (Some(codex), Some(claude)) => format!("Codex {codex:.0}% · Claude {claude:.0}% 남음"),
        _ => "Codex · Claude 남은 사용량을 읽는 중".to_string(),
    }
}

fn build_menu(
    snapshot: Option<&AppSnapshot>,
    target: &NativeTrayTarget,
    mtm: MainThreadMarker,
) -> Retained<NSMenu> {
    let codex = snapshot.and_then(|snapshot| provider_by_id(snapshot, "codex"));
    let claude = snapshot.and_then(|snapshot| provider_by_id(snapshot, "claude"));
    let exchange_rate = snapshot.and_then(|snapshot| snapshot.exchange_rate.as_ref());
    let codex_weekly =
        codex.and_then(|provider| preferred_window(provider, "codex-secondary", 10_080));
    let claude_five_hour =
        claude.and_then(|provider| preferred_window(provider, "claude-primary", 300));
    let claude_weekly =
        claude.and_then(|provider| preferred_window(provider, "claude-secondary", 10_080));

    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);

    add_info(
        &menu,
        codex
            .and_then(min_remaining)
            .map(|remaining| format!("Codex  ·  {remaining:.0}% 남음"))
            .unwrap_or_else(|| "Codex  ·  읽는 중".to_string()),
        false,
        mtm,
    );
    add_info(&menu, limit_menu_text("주간 한도", codex_weekly), true, mtm);
    add_info(
        &menu,
        format!(
            "초기화까지  ·  {}",
            format_reset_countdown(codex_weekly.and_then(|window| window.resets_at.as_deref()))
        ),
        true,
        mtm,
    );
    add_info(&menu, token_menu_text(codex), true, mtm);
    add_info(&menu, krw_menu_text(codex, exchange_rate), true, mtm);
    add_info(
        &menu,
        codex
            .and_then(|provider| provider.reset_credits.as_ref())
            .map(|credits| format!("리셋권  ·  {}장", credits.available_count))
            .unwrap_or_else(|| "리셋권  ·  확인 중".to_string()),
        true,
        mtm,
    );

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    add_info(
        &menu,
        claude
            .and_then(min_remaining)
            .map(|remaining| format!("Claude  ·  {remaining:.0}% 남음"))
            .unwrap_or_else(|| "Claude  ·  읽는 중".to_string()),
        false,
        mtm,
    );
    add_info(
        &menu,
        limit_menu_text("5시간 한도", claude_five_hour),
        true,
        mtm,
    );
    add_info(
        &menu,
        format!(
            "5시간 초기화까지  ·  {}",
            format_reset_countdown(claude_five_hour.and_then(|window| window.resets_at.as_deref()))
        ),
        true,
        mtm,
    );
    add_info(
        &menu,
        limit_menu_text("주간 한도", claude_weekly),
        true,
        mtm,
    );
    add_info(
        &menu,
        format!(
            "주간 초기화까지  ·  {}",
            format_reset_countdown(claude_weekly.and_then(|window| window.resets_at.as_deref()))
        ),
        true,
        mtm,
    );
    add_info(&menu, token_menu_text(claude), true, mtm);
    add_info(&menu, krw_menu_text(claude, exchange_rate), true, mtm);
    add_info(
        &menu,
        "※ 실제 청구액이 아닌 개발자용 정가 환산".to_string(),
        false,
        mtm,
    );

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    add_action(&menu, "쌀먹 열기", sel!(openApp:), target, mtm);
    add_action(&menu, "지금 갱신", sel!(refreshUsage:), target, mtm);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    add_action(&menu, "종료", sel!(quitApp:), target, mtm);
    menu
}

fn add_info(menu: &NSMenu, title: String, indented: bool, mtm: MainThreadMarker) {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(&title),
            None,
            &NSString::new(),
        )
    };
    item.setEnabled(false);
    if indented {
        item.setIndentationLevel(1);
    }
    menu.addItem(&item);
}

fn add_action(
    menu: &NSMenu,
    title: &str,
    action: objc2::runtime::Sel,
    target: &NativeTrayTarget,
    mtm: MainThreadMarker,
) {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(title),
            Some(action),
            &NSString::new(),
        )
    };
    unsafe { item.setTarget(Some(target)) };
    item.setEnabled(true);
    menu.addItem(&item);
}
