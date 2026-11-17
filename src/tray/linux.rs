//! StatusNotifierItem tray (KDE, GNOME with the AppIndicator extension, XFCE, Cinnamon, …).

use std::sync::Arc;

use ksni::TrayMethods;
use ksni::menu::{CheckmarkItem, StandardItem};

use super::{ICON_SIZE, icon_rgba, open_config_dir, toggle_autostart};
use crate::app::App;
use crate::log;

struct Tray {
    app: Arc<App>,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "ts6rpc".into()
    }

    fn title(&self) -> String {
        "ts6rpc".into()
    }

    // Left click opens the menu too.
    const MENU_ON_ACTIVATE: bool = true;

    fn category(&self) -> ksni::Category {
        ksni::Category::Communications
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let mut data = icon_rgba(self.app.is_enabled());
        for px in data.as_chunks_mut::<4>().0 {
            px.rotate_right(1); // RGBA -> ARGB
        }
        vec![ksni::Icon { width: ICON_SIZE as i32, height: ICON_SIZE as i32, data }]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "TeamSpeak Discord Rich Presence".into(),
            description: self.app.status_line(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            StandardItem { label: self.app.status_line(), enabled: false, ..Default::default() }.into(),
            ksni::MenuItem::Separator,
            CheckmarkItem {
                label: "Show in Discord".into(),
                checked: self.app.is_enabled(),
                activate: Box::new(|t: &mut Self| t.app.set_enabled(!t.app.is_enabled())),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Start on login".into(),
                checked: crate::autostart::is_enabled(),
                activate: Box::new(|_: &mut Self| toggle_autostart()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open config folder".into(),
                icon_name: "folder-open".into(),
                activate: Box::new(|_: &mut Self| open_config_dir()),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|t: &mut Self| t.app.request_quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Shows the tray icon and keeps it up to date. Runs until the app quits.
pub async fn run(app: Arc<App>) {
    // Also works if the panel starts after us (e.g. on login): the icon appears once it's ready.
    let tray = Tray { app: app.clone() };
    let handle = match tray.assume_sni_available(true).spawn().await {
        Ok(h) => h,
        Err(e) => {
            log!("Tray: not available ({e:?})");
            return std::future::pending().await;
        }
    };
    let mut enabled = app.enabled.subscribe();
    let mut status = app.status.subscribe();
    loop {
        tokio::select! {
            r = enabled.changed() => if r.is_err() { break },
            r = status.changed() => if r.is_err() { break },
        }
        // ksni re-reads the properties and signals what changed.
        handle.update(|_| {}).await;
    }
    std::future::pending().await
}
