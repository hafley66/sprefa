//! Menu bar tray for the daemon. macOS-only in v1; Windows + Linux are
//! cfg-gated for Phase 2 follow-ups.
//!
//! The tray owns the platform main thread (CFRunLoop on mac, message pump on
//! windows, GLib loop on linux). `daemon::run_daemon` moves the accept loop
//! off-main so the main thread can block on `run_tray`. The Quit menu item
//! mirrors the RPC shutdown path: set the flag, clean up, exit.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;

use crate::daemon::{shutdown_cleanup, Daemon};

#[cfg(target_os = "macos")]
pub fn run_tray(d: Arc<Daemon>) -> Result<()> {
    use tao::event::Event;
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::TrayIconBuilder;
    // Accessory mode: status item top-right, no Dock icon, no cmd-tab entry.
    // Equivalent to LSUIElement=true in an Info.plist but works unbundled.
    let mut event_loop = EventLoop::new();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    let root_label = d
        .root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| d.root.to_string_lossy().into_owned());

    let header = MenuItem::new(format!("sprefa — {root_label}"), false, None);
    let status = MenuItem::new(
        format!("Tick #{}", d.tick_count.load(Ordering::Relaxed)),
        false,
        None,
    );
    let program = MenuItem::new(
        format!("Program: {}", d.program_display),
        false,
        None,
    );
    let quit = MenuItem::new("Quit sprefa daemon", true, None);

    let menu = Menu::new();
    menu.append(&header)
        .map_err(|e| anyhow::anyhow!("menu header: {e}"))?;
    menu.append(&status)
        .map_err(|e| anyhow::anyhow!("menu status: {e}"))?;
    menu.append(&program)
        .map_err(|e| anyhow::anyhow!("menu program: {e}"))?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| anyhow::anyhow!("menu sep: {e}"))?;
    menu.append(&quit)
        .map_err(|e| anyhow::anyhow!("menu quit: {e}"))?;

    let icon = make_tray_icon();

    let _tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_tooltip(format!("sprefa daemon — {}", d.root.display()))
        .build()
        .map_err(|e| anyhow::anyhow!("tray init: {e}"))?;

    let menu_rx = MenuEvent::receiver();
    let quit_id = quit.id().clone();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::MainEventsCleared = event {
            if let Ok(m) = menu_rx.try_recv() {
                if m.id == quit_id {
                    d.shutdown_requested.store(true, Ordering::Relaxed);
                    shutdown_cleanup(&d);
                    std::process::exit(0);
                }
            }
        }
    });
}

fn make_tray_icon() -> tray_icon::Icon {
    const SIZE: u32 = 32;
    let half = SIZE as f32 / 2.0;
    let outer_radius = half - 1.0;
    let inner_radius = half - 5.0;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - half + 0.5;
            let dy = y as f32 - half + 0.5;
            let d = (dx * dx + dy * dy).sqrt();
            if d <= outer_radius && d >= inner_radius {
                let edge = (d - inner_radius).min(outer_radius - d);
                let a = if edge < 1.5 {
                    ((edge / 1.5) * 255.0) as u8
                } else {
                    255
                };
                rgba.extend_from_slice(&[0x3b, 0x82, 0xf6, a]);
            } else if d <= inner_radius {
                rgba.extend_from_slice(&[0x3b, 0x82, 0xf6, 0xff]);
            } else {
                rgba.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("icon from rgba")
}

#[cfg(not(target_os = "macos"))]
pub fn run_tray(_d: Arc<Daemon>) -> Result<()> {
    anyhow::bail!("tray not implemented on this platform (Phase 2 v1 is mac-only)")
}
