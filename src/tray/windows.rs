//! Windows notification area icon. Must run on the main thread (it needs a message loop).

use std::sync::Arc;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, MSG, SetTimer, TranslateMessage,
};

use super::{ICON_SIZE, icon_rgba, open_config_dir, toggle_autostart};
use crate::app::App;
use crate::log;

fn icon(enabled: bool) -> Option<Icon> {
    Icon::from_rgba(icon_rgba(enabled), ICON_SIZE, ICON_SIZE).ok()
}

/// Tooltips are limited to 127 characters.
fn tooltip(app: &App) -> String {
    let s = format!("ts6rpc\n{}", app.status_line());
    if s.chars().count() > 127 { s.chars().take(126).chain(Some('…')).collect() } else { s }
}

/// Runs the tray and its message loop until `done()` returns true.
pub fn run(app: Arc<App>, done: impl Fn() -> bool) {
    let status = MenuItem::new(app.status_line(), false, None);
    let enabled = CheckMenuItem::new("Show in Discord", true, app.is_enabled(), None);
    let autostart = CheckMenuItem::new("Start on login", true, crate::autostart::is_enabled(), None);
    let open = MenuItem::new("Open config folder", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[
        &status,
        &PredefinedMenuItem::separator(),
        &enabled,
        &autostart,
        &open,
        &PredefinedMenuItem::separator(),
        &quit,
    ]);

    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(true)
        .with_tooltip(tooltip(&app));
    if let Some(i) = icon(app.is_enabled()) {
        builder = builder.with_icon(i);
    }
    let tray = match builder.build() {
        Ok(t) => Some(t),
        Err(e) => {
            log!("Tray: not available ({e})");
            None
        }
    };

    // A thread timer wakes the loop regularly to pick up status changes and exit requests.
    let timer = unsafe { SetTimer(std::ptr::null_mut(), 0, 250, None) };
    let mut shown = (app.is_enabled(), app.status_line());
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while !done() && unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == enabled.id() {
                app.set_enabled(enabled.is_checked());
            } else if ev.id == autostart.id() {
                toggle_autostart();
                autostart.set_checked(crate::autostart::is_enabled());
            } else if ev.id == open.id() {
                open_config_dir();
            } else if ev.id == quit.id() {
                app.request_quit();
            }
        }
        let now = (app.is_enabled(), app.status_line());
        if now != shown {
            enabled.set_checked(now.0);
            status.set_text(&now.1);
            if let Some(t) = &tray {
                if now.0 != shown.0 {
                    let _ = t.set_icon(icon(now.0));
                }
                let _ = t.set_tooltip(Some(tooltip(&app)));
            }
            shown = now;
        }
    }
    unsafe { KillTimer(std::ptr::null_mut(), timer) };
}
