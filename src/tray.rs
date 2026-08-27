//! System tray: icon, menu, event handlers, and the UI commands they emit.

use std::sync::mpsc::Sender;

use anyhow::{Result, anyhow};
use eframe::egui;
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

const MENU_SHOW: &str = "paretowatch_show";
const MENU_REFRESH: &str = "paretowatch_refresh";
const MENU_MUTE_HOUR: &str = "paretowatch_mute_hour";
const MENU_QUIT: &str = "paretowatch_quit";

#[derive(Debug)]
pub(crate) enum UiCommand {
    Toggle,
    Show,
    Refresh,
    MuteOneHour,
    Quit,
}

pub(crate) fn install_tray_event_handlers(ctx: egui::Context, ui_tx: Sender<UiCommand>) {
    let tx = ui_tx.clone();
    let repaint = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |event| match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
        | TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            let _ = tx.send(UiCommand::Toggle);
            repaint.request_repaint();
        }
        _ => {}
    }));

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id = event.id.0.as_str();
        let cmd = match id {
            MENU_SHOW => Some(UiCommand::Show),
            MENU_REFRESH => Some(UiCommand::Refresh),
            MENU_MUTE_HOUR => Some(UiCommand::MuteOneHour),
            MENU_QUIT => Some(UiCommand::Quit),
            _ => None,
        };
        if let Some(cmd) = cmd {
            let _ = ui_tx.send(cmd);
            ctx.request_repaint();
        }
    }));
}

pub(crate) fn create_tray() -> Result<TrayIcon> {
    let menu = Menu::new();
    let show = MenuItem::with_id(MENU_SHOW, "Show ParetoWatch", true, None);
    let refresh = MenuItem::with_id(MENU_REFRESH, "Refresh now", true, None);
    let mute_hour = MenuItem::with_id(MENU_MUTE_HOUR, "Mute alerts for 1 hour", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::with_id(MENU_QUIT, "Quit", true, None);
    menu.append_items(&[&show, &refresh, &mute_hour, &separator, &quit])?;

    let tray = TrayIconBuilder::new()
        .with_tooltip("ParetoWatch")
        .with_icon(make_tray_icon()?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?;
    Ok(tray)
}

fn make_tray_icon() -> Result<tray_icon::Icon> {
    const W: u32 = 32;
    const H: u32 = 32;
    let mut rgba = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            let cx = x as f32 - 15.5;
            let cy = y as f32 - 15.5;
            let r2 = cx * cx + cy * cy;
            if r2 < 220.0 {
                rgba[i] = 28;
                rgba[i + 1] = 170;
                rgba[i + 2] = 220;
                rgba[i + 3] = 255;
            }
            // small rising frontier mark
            if ((8..=11).contains(&x) && (20..=23).contains(&y))
                || ((14..=17).contains(&x) && (14..=17).contains(&y))
                || ((20..=23).contains(&x) && (8..=11).contains(&y))
            {
                rgba[i] = 255;
                rgba[i + 1] = 255;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, W, H).map_err(|e| anyhow!("tray icon: {e}"))
}

#[cfg(target_os = "linux")]
pub(crate) fn spawn_linux_tray() {
    std::thread::spawn(|| {
        if let Err(err) = gtk::init() {
            eprintln!("ParetoWatch tray: GTK init failed: {err}");
            return;
        }
        match create_tray() {
            Ok(_tray) => gtk::main(),
            Err(err) => eprintln!("ParetoWatch tray creation failed: {err:#}"),
        }
    });
}
