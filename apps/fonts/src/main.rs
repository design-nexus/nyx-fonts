//! Fonts — browse, preview and install Google Fonts and Nerd Fonts.

mod catalog;
mod cmd;
mod control;
mod events;
mod fmt;
mod http;
mod install;
mod installed;
mod names;
mod paths;
mod picker;
mod prefs;
mod preview;
mod settings_dialog;
mod settings_page;
mod sheet;
mod slots;
mod system;
mod theme;
mod typeset;
mod widgets;
mod window;

use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::Cell;

pub const APP_ID: &str = "io.github.design_nexus.nyx.Fonts";

const USAGE: &str = "Usage: nyx-fonts [OPTIONS]\n\
\n\
  --section ID    open (or switch the open window) to a list: google, nerd, computer\n\
                  (installed and system too), or settings\n\
  --search TEXT   search both catalogs\n\
  --toggle        close the window if it's open, otherwise open it (for a keybinding)\n";

thread_local! {
    static STARTED: Cell<bool> = const { Cell::new(false) };
}

fn start() {
    if STARTED.with(|s| s.replace(true)) {
        return;
    }
    catalog::start();
    preview::warm_fallbacks();
    std::thread::spawn(preview::prune);
}

fn main() -> glib::ExitCode {
    nyx_core::app::init(nyx_core::app::App { slug: "nyx-fonts", name: "Fonts" });
    nyx_core::control::describe_if_asked(&control::description(
        &std::env::var("NYX_FONTS_APP_ID").unwrap_or_else(|_| APP_ID.to_string()),
    ));
    nyx_core::migrate::from(&["nexus-fonts"]);
    installed::move_old_fonts();
    let mut args: Vec<String> = std::env::args().collect();
    // Started by D-Bus for an action: GLib leaves this flag to apps that handle their own
    // command line.
    let service = args.iter().any(|a| a == "--gapplication-service");
    args.retain(|a| a != "--gapplication-service");
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return glib::ExitCode::SUCCESS;
    }

    // GTK's Vulkan renderer enumerates every GPU at startup, which wakes a
    // sleeping discrete GPU on hybrid laptops. GL renders only on the one in use.
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: still single-threaded; nothing else reads the environment yet.
        unsafe { std::env::set_var("GSK_RENDERER", "gl") };
    }

    // Developer aid: a different id gives a separate instance (a test copy on a virtual
    // monitor), so it doesn't hand its arguments to the copy that's open.
    let id = std::env::var("NYX_FONTS_APP_ID").unwrap_or_else(|_| APP_ID.to_string());
    let app = gtk::Application::builder()
        .application_id(id)
        .flags(if service {
            gio::ApplicationFlags::HANDLES_COMMAND_LINE | gio::ApplicationFlags::IS_SERVICE
        } else {
            gio::ApplicationFlags::HANDLES_COMMAND_LINE
        })
        .build();
    // Nyx Assistant searches and installs fonts through this (primary instance only).
    // The nyx command, Glimpse and Nyx Assistant reach it through this (primary instance
    // only); a call to a closed app opens its window, which holds what the actions use.
    let described = control::description(&app.application_id().map(|s| s.to_string()).unwrap_or_default());
    app.connect_startup(move |app| {
        let handle_app = app.clone();
        nyx_core::control::serve(app, described.clone(), move |name, args| {
            if handle_app.active_window().is_none() {
                window::present(&handle_app, None);
            }
            control::handle(name, args)
        })
    });
    app.connect_command_line(|app, cl| {
        let argv: Vec<String> = cl.arguments().iter().map(|a| a.to_string_lossy().to_string()).collect();
        let has = |flag: &str| argv.iter().any(|a| a == flag);
        let value_of = |flag: &str| argv.iter().position(|a| a == flag).and_then(|i| argv.get(i + 1)).cloned();
        if has("--toggle")
            && let Some(w) = window::window()
            && w.is_visible()
        {
            w.close();
            return glib::ExitCode::SUCCESS;
        }
        start();
        window::present(app, value_of("--section").as_deref());
        if let Some(q) = value_of("--search") {
            window::search(&q);
        }
        glib::ExitCode::SUCCESS
    });
    app.connect_shutdown(|_| prefs::flush());
    app.run_with_args(&args)
}
