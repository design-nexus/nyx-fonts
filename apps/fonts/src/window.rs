//! The main window: the header (the app; the search across both catalogs in the
//! middle; updates, what's installing, and settings at the end), and in the page the
//! list of fonts beside the sheet they're set on.

use crate::picker::{Picker, Tab};
use crate::sheet::Sheet;
use crate::slots::{self, Slot};
use crate::{catalog, cmd, events, fmt, install, installed, paths, prefs, settings_dialog, system, theme, widgets};
use gtk::prelude::*;
use gtk::{gdk, glib};
use nyx_ui::chrome;
use std::cell::RefCell;
use std::rc::Rc;

struct Ui {
    window: gtk::ApplicationWindow,
    picker: Picker,
    sheet: Sheet,
    search: gtk::SearchEntry,
    overlay: gtk::Overlay,
    header: chrome::AppHeader,
    /// Catalog loading and installs under way, at the header's end.
    readout: gtk::Label,
    narrow: bool,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
}

fn ui() -> Option<Rc<RefCell<Ui>>> {
    UI.with(|u| u.borrow().clone())
}

pub fn present(app: &gtk::Application, section: Option<&str>) {
    if let Some(ui) = ui() {
        let window = ui.borrow().window.clone();
        if let Some(s) = section {
            open_section(s);
        }
        window.present();
        return;
    }
    theme::install();
    build(app);
    let start = section.map(String::from).unwrap_or_else(|| nyx_theme::start_page("google", &prefs::get().last_section));
    let picker = ui().map(|u| u.borrow().picker.clone());
    if let Some(p) = picker {
        p.show_tab(Tab::from_id(&start).unwrap_or(Tab::Google));
    }
    if start == "settings" {
        settings_dialog::open();
    }
    if let Some(window) = window() {
        window.present();
    }
    if prefs::take_broken() {
        toast("Your settings file couldn't be read, so defaults are in use. The old file is kept as settings.toml.bak.");
    }
}

/// A list tab, or Settings (`--section`).
fn open_section(id: &str) {
    if id == "settings" {
        settings_dialog::open();
    } else if let (Some(tab), Some(ui)) = (Tab::from_id(id), ui()) {
        let picker = ui.borrow().picker.clone();
        picker.show_tab(tab);
    }
}

fn build(app: &gtk::Application) {
    let window =
        gtk::ApplicationWindow::builder().application(app).title("Fonts").default_width(1280).default_height(800).build();
    window.add_css_class("fonts-window");
    // No client-side titlebar: Hyprland manages the window.
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    nyx_ui::keys::install(&window, nyx_ui::keys::Common::default());
    window.set_icon_name(Some("io.github.design_nexus.nyx.Fonts"));

    // ----- Header -----
    let header = chrome::app_header(&window, "font-x-generic-symbolic", "Fonts");
    let search = search_entry();
    header.center.append(&search);
    header.end.set_spacing(6);
    let readout = widgets::label("", "header-status");
    readout.set_ellipsize(gtk::pango::EllipsizeMode::End);
    readout.set_max_width_chars(36);
    header.end.append(&readout);
    header.end.append(&updates_button());
    header.end.append(&chrome::settings_button(settings_dialog::open));

    // ----- The list and the sheet -----
    let picker = Picker::new();
    let sheet = Sheet::new();
    let content = widgets::hbox(0);
    // The list keeps its set width: the wrapping lines in it would ask for more.
    let column = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&picker.root)
        .build();
    content.append(&column);
    content.append(&nyx_ui::frame::pane_line(gtk::Orientation::Vertical));
    content.append(&sheet.root);

    let frame = nyx_ui::frame::AppFrame::new(&header.root);
    frame.set_flush(true);
    frame.set_page(&content);
    let overlay = frame.install(&window);
    accept_font_drops(&window);
    install_keys(&window, &search);

    // A narrow window (a tiled half-screen) gets a narrower list and a shorter header.
    let apply_width = |w: &gtk::ApplicationWindow| {
        let width = if w.width() > 0 { w.width() } else { w.default_width() };
        settings_dialog::fit(w);
        set_narrow(width > 0 && width < 980);
    };
    window.connect_default_width_notify(move |w| apply_width(w));
    window.connect_realize(move |w| apply_width(w));
    // Tiled windows are resized by the compositor without touching the default
    // size: an invisible layer over the whole window reports each real size
    // change, and the layout follows on the next frame.
    let probe = gtk::DrawingArea::new();
    probe.set_can_target(false);
    probe.set_can_focus(false);
    overlay.add_overlay(&probe);
    overlay.set_measure_overlay(&probe, false);
    let w2 = window.clone();
    probe.connect_resize(move |_, _, _| {
        let w2 = w2.clone();
        glib::idle_add_local_once(move || apply_width(&w2));
    });

    let r = readout.clone();
    let refresh = move || {
        let text = install::status().or_else(|| catalog::loading().then(|| "Updating the font lists…".to_string()));
        r.set_visible(text.is_some() && !narrow());
        r.set_text(text.as_deref().unwrap_or(""));
    };
    refresh();
    events::subscribe(&readout, move |_| refresh());

    let ui = Ui { window, picker, sheet, search, overlay, header, readout, narrow: false };
    UI.with(|u| *u.borrow_mut() = Some(Rc::new(RefCell::new(ui))));
    system::refresh();
}

/// "2 updates": fonts installed with Fonts that have newer files. Opens This computer.
fn updates_button() -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("updates-button");
    b.set_valign(gtk::Align::Center);
    let c = widgets::hbox(6);
    c.append(&gtk::Image::from_icon_name("view-refresh-symbolic"));
    let label = gtk::Label::new(None);
    c.append(&label);
    b.set_child(Some(&c));
    b.set_tooltip_text(Some("Show the fonts with newer files"));
    b.connect_clicked(|_| {
        let Some(ui) = ui() else { return };
        let (search, picker) = (ui.borrow().search.clone(), ui.borrow().picker.clone());
        search.set_text("");
        picker.show_tab(Tab::Computer);
    });
    let refresh = {
        let (b, label) = (b.clone(), label.clone());
        move || {
            let n = installed::outdated().len();
            b.set_visible(n > 0);
            label.set_text(&fmt::count(n, "update", "updates"));
        }
    };
    refresh();
    events::subscribe(&b, move |c| {
        if matches!(c, events::Change::Catalog | events::Change::Installed) {
            refresh();
        }
    });
    b
}

/// The search's width in characters, in a full window and a narrow one.
const SEARCH_WIDE: i32 = 32;
const SEARCH_NARROW: i32 = 18;

/// The search across both catalogs, always in the header (Ctrl+K).
fn search_entry() -> gtk::SearchEntry {
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search Google Fonts and Nerd Fonts"));
    search.add_css_class("bar-search");
    search.set_width_chars(SEARCH_WIDE);
    search.set_tooltip_text(Some("Search (Ctrl+K)"));
    search.update_property(&[gtk::accessible::Property::Label("Search Google Fonts and Nerd Fonts")]);
    search.connect_search_changed(|e| {
        let picker = ui().map(|u| u.borrow().picker.clone());
        if let Some(p) = picker {
            p.set_query(e.text().trim());
        }
    });
    search.connect_activate(|_| {
        let picker = ui().map(|u| u.borrow().picker.clone());
        if let Some(p) = picker {
            p.focus_list();
        }
    });
    search.connect_stop_search(|e| e.set_text(""));
    search
}

fn set_narrow(narrow: bool) {
    let Some(ui) = ui() else { return };
    if ui.borrow().narrow == narrow && ui.borrow().picker.root.has_css_class("sized") {
        return;
    }
    ui.borrow_mut().narrow = narrow;
    let u = ui.borrow();
    u.picker.root.add_css_class("sized");
    u.picker.set_narrow(narrow);
    u.sheet.set_narrow(narrow);
    u.search.set_width_chars(if narrow { SEARCH_NARROW } else { SEARCH_WIDE });
    u.search.set_placeholder_text(Some(if narrow { "Search fonts" } else { "Search Google Fonts and Nerd Fonts" }));
    u.readout.set_visible(!narrow && !u.readout.text().is_empty());
    // The app's name goes in a narrow window; its icon stays.
    if let Some(title) = find_class(u.header.root.upcast_ref(), "app-title") {
        title.set_visible(!narrow);
    }
}

fn find_class(w: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    if w.has_css_class(class) {
        return Some(w.clone());
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(found) = find_class(&c, class) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

pub fn narrow() -> bool {
    ui().is_some_and(|u| u.borrow().narrow)
}

/// The window title follows the list's tab.
pub fn tab_shown(tab: Tab) {
    if let Some(ui) = ui() {
        chrome::set_title(&ui.borrow().window, "Fonts", tab.title());
    }
}

fn install_keys(window: &gtk::ApplicationWindow, search: &gtk::SearchEntry) {
    // Capture phase: these work wherever focus is.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let s2 = search.clone();
    let w2 = window.clone();
    keys.connect_key_pressed(move |_, key, _, mods| {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let alt = mods.contains(gdk::ModifierType::ALT_MASK);
        let typing = gtk::prelude::GtkWindowExt::focus(&w2).is_some_and(|f| {
            f.is::<gtk::Text>() || f.ancestor(gtk::Entry::static_type()).is_some() || f.is::<gtk::SearchEntry>()
        });
        let done = glib::Propagation::Stop;
        if settings_dialog::is_open() {
            return match key {
                gdk::Key::Escape => {
                    settings_dialog::escape();
                    done
                }
                gdk::Key::f if ctrl => {
                    settings_dialog::focus_search();
                    done
                }
                gdk::Key::q | gdk::Key::w if ctrl => {
                    w2.close();
                    done
                }
                _ => glib::Propagation::Proceed,
            };
        }
        let digit = key.to_unicode().and_then(|c| c.to_digit(10)).filter(|d| (1..=3).contains(d)).map(|d| d as usize - 1);
        match key {
            gdk::Key::f | gdk::Key::k if ctrl => {
                s2.grab_focus();
                done
            }
            gdk::Key::F1 => {
                show_shortcuts();
                done
            }
            gdk::Key::question if !ctrl && !typing => {
                show_shortcuts();
                done
            }
            gdk::Key::comma if ctrl => {
                settings_dialog::open();
                done
            }
            gdk::Key::q | gdk::Key::w if ctrl => {
                w2.close();
                done
            }
            _ if ctrl && !alt && digit.is_some() => {
                if let (Some(d), Some(ui)) = (digit, ui()) {
                    let picker = ui.borrow().picker.clone();
                    s2.set_text("");
                    picker.show_tab(Tab::ALL[d]);
                }
                done
            }
            _ if alt && !ctrl && digit.is_some() => {
                if let Some(d) = digit {
                    slots::set_active(Slot::ALL[d]);
                }
                done
            }
            gdk::Key::Escape if s2.has_focus() || !s2.text().is_empty() => {
                s2.set_text("");
                gtk::prelude::GtkWindowExt::set_focus(&w2, None::<&gtk::Widget>);
                done
            }
            _ => glib::Propagation::Proceed,
        }
    });
    window.add_controller(keys);
}

/// Every keyboard shortcut, for Settings and the F1 list.
pub const SHORTCUTS: &[(&[&str], &str)] = &[
    (&["Ctrl", "K"], "Search Google Fonts, Nerd Fonts and this computer"),
    (&["Esc"], "Clear the search"),
    (&["Ctrl", "1–3"], "Google, Nerd Fonts or This computer"),
    (&["Alt", "1–3"], "Choose the heading, body or code font"),
    (&["Ctrl", ","], "Settings"),
    (&["F1"], "Show these shortcuts (or ?)"),
    (&["Ctrl", "Q"], "Close"),
];

/// The shortcuts in a dialog.
pub fn show_shortcuts() {
    let (dialog, card) = widgets::dialog("Keyboard shortcuts", 520);
    let list = widgets::vbox(0);
    list.add_css_class("group-list");
    for (keys, what) in SHORTCUTS {
        list.append(&widgets::row(what, "", Some(widgets::key_caps(keys).upcast_ref())));
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        // Scrolls without a scrollbar, which would cover the key caps.
        .vscrollbar_policy(gtk::PolicyType::External)
        .propagate_natural_height(true)
        .max_content_height(560)
        .child(&list)
        .build();
    card.append(&scroll);
    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    let d = dialog.clone();
    close.connect_clicked(move |_| d.close());
    card.append(&close);
    dialog.present();
}

/// Show a short message at the bottom of the window.
pub fn toast(message: &str) {
    let Some(ui) = ui() else {
        eprintln!("nyx-fonts: {message}");
        return;
    };
    let overlay = ui.borrow().overlay.clone();
    if let Some(old) = TOAST.with(|t| t.borrow_mut().take())
        && old.parent().is_some()
    {
        overlay.remove_overlay(&old);
    }
    let label = gtk::Label::new(Some(message));
    label.set_wrap(true);
    label.set_max_width_chars(70);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bx.add_css_class("toast");
    bx.append(&label);
    bx.set_halign(gtk::Align::Center);
    bx.set_valign(gtk::Align::End);
    overlay.add_overlay(&bx);
    TOAST.with(|t| *t.borrow_mut() = Some(bx.clone()));
    glib::timeout_add_local_once(std::time::Duration::from_millis(3500), move || {
        if bx.parent().is_some() {
            overlay.remove_overlay(&bx);
        }
    });
}

thread_local! {
    /// The toast showing now; a new one replaces it.
    static TOAST: RefCell<Option<gtk::Box>> = const { RefCell::new(None) };
}

/// The layer over the window, for toasts and the settings dialog.
pub fn overlay() -> Option<gtk::Overlay> {
    ui().map(|u| u.borrow().overlay.clone())
}

pub fn window() -> Option<gtk::ApplicationWindow> {
    ui().map(|u| u.borrow().window.clone())
}

/// Search both catalogs from outside (`--search`).
pub fn search(query: &str) {
    let Some(ui) = ui() else { return };
    let entry = ui.borrow().search.clone();
    entry.set_text(query);
}

/// Install font files for this user: copied into
/// `~/.local/share/fonts/nyx-fonts/local`, then the font cache is refreshed.
pub fn install_files(files: Vec<std::path::PathBuf>) {
    let fonts: Vec<std::path::PathBuf> = files
        .into_iter()
        .filter(|p| {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            matches!(ext.as_str(), "ttf" | "otf" | "ttc")
        })
        .collect();
    if fonts.is_empty() {
        toast("Drop .ttf, .otf or .ttc files to install them.");
        return;
    }
    let dir = paths::fonts_dir().join("local");
    cmd::background(
        move || -> std::io::Result<usize> {
            std::fs::create_dir_all(&dir)?;
            for f in &fonts {
                if let Some(name) = f.file_name() {
                    std::fs::copy(f, dir.join(name))?;
                }
            }
            let _ = std::process::Command::new("fc-cache").arg("-f").arg(&dir).status();
            Ok(fonts.len())
        },
        |result| match result {
            Ok(n) => {
                toast(&format!("Installed {}. Find them under This computer.", fmt::count(n, "font file", "font files")));
                system::refresh();
            }
            Err(e) => toast(&format!("Couldn't install the fonts: {e}")),
        },
    );
}

fn accept_font_drops(window: &gtk::ApplicationWindow) {
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    drop.connect_drop(|_, value, _, _| {
        let Ok(list) = value.get::<gdk::FileList>() else { return false };
        install_files(list.files().iter().filter_map(|f| f.path()).collect());
        true
    });
    window.add_controller(drop);
}
