//! The main window: the header (the app and the page, the sidebar toggle; the search
//! across both catalogs in the middle; what's installed or installing, and settings at
//! the end), the navigation sidebar and a stack of pages built the first time they're
//! shown.

use crate::sections::{self, Section};
use crate::widgets;
use crate::{catalog, cmd, events, fmt, install, installed, paths, prefs, settings_dialog, theme};
use gtk::prelude::*;
use nyx_ui::chrome;
use gtk::{gdk, glib};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

struct Ui {
    window: gtk::ApplicationWindow,
    stack: gtk::Stack,
    nav: gtk::Box,
    nav_items: HashMap<String, gtk::Button>,
    search: gtk::SearchEntry,
    pages: HashMap<String, gtk::Widget>,
    sections: Vec<Section>,
    current: String,
    /// Where Esc in the search goes back to.
    before_search: String,
    /// Pages visited, for Back.
    history: Vec<String>,
    overlay: gtk::Overlay,
    header: chrome::AppHeader,
    /// The catalogs and installs, at the header's end.
    readout: gtk::Label,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
    static NARROW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn ui() -> Option<Rc<RefCell<Ui>>> {
    UI.with(|u| u.borrow().clone())
}

pub fn present(app: &gtk::Application, section: Option<&str>) {
    if let Some(ui) = ui() {
        let window = ui.borrow().window.clone();
        if let Some(s) = section {
            navigate(s);
        }
        window.present();
        return;
    }
    theme::install();
    install_icons();
    build(app);
    let start = section.map(String::from).unwrap_or_else(|| nyx_theme::start_page("google", &prefs::get().last_section));
    navigate(&start);
    if let Some(ui) = ui() {
        ui.borrow().window.present();
    }
    if prefs::take_broken() {
        toast("Your settings file couldn't be read, so defaults are in use. The old file is kept as settings.toml.bak.");
    }
}

/// Our own symbolic icons, for things the icon theme has no glyph for.
/// They're written to the cache once and added to the icon search path.
fn install_icons() {
    const ICONS: &[(&str, &str)] = &[
        ("fonts-google-symbolic.svg", include_str!("../data/icons/fonts-google-symbolic.svg")),
        ("fonts-nerd-symbolic.svg", include_str!("../data/icons/fonts-nerd-symbolic.svg")),
        ("fonts-installed-symbolic.svg", include_str!("../data/icons/fonts-installed-symbolic.svg")),
        ("fonts-system-symbolic.svg", include_str!("../data/icons/fonts-system-symbolic.svg")),
    ];
    let dir = crate::paths::cache_dir().join("icons");
    for (name, svg) in ICONS {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = crate::cmd::atomic_write(&path, svg);
        }
    }
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

fn nav_button(icon: &str, title: &str, tooltip: &str) -> (gtk::Button, gtk::Label, gtk::Label) {
    let button = gtk::Button::new();
    button.add_css_class("nav-item");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.append(&gtk::Image::from_icon_name(icon));
    let l = widgets::label(title, "nav-label");
    l.set_hexpand(true);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&l);
    let count = widgets::label("", "nav-count");
    count.add_css_class("compact-hide");
    content.append(&count);
    button.set_child(Some(&content));
    button.set_tooltip_text(Some(tooltip));
    (button, l, count)
}

/// The counts beside the sidebar's pages.
fn nav_counts() -> Vec<(&'static str, String)> {
    let n = |x: usize| if x == 0 { String::new() } else { fmt::thousands(x) };
    let outdated = installed::outdated().len();
    vec![
        ("google", n(catalog::google().len())),
        ("nerd", n(catalog::nerd().len())),
        ("installed", if outdated > 0 { format!("{} ↑", installed::all().len()) } else { n(installed::all().len()) }),
    ]
}

fn build(app: &gtk::Application) {
    let window =
        gtk::ApplicationWindow::builder().application(app).title("Fonts").default_width(1280).default_height(800).build();
    window.add_css_class("fonts-window");
    // No client-side titlebar: Hyprland manages the window.
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    nyx_ui::keys::install(&window, nyx_ui::keys::Common::default());
    window.set_icon_name(Some("io.github.design_nexus.nyx.Fonts"));

    let sections = sections::all();

    // ----- Sidebar -----
    let nav = gtk::Box::new(gtk::Orientation::Vertical, 0);
    nav.add_css_class("settings-navigation");
    nav.set_hexpand(false);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut nav_items = HashMap::new();
    let mut counts: Vec<(String, gtk::Label)> = Vec::new();
    let mut last_group = "";
    for s in sections.iter().filter(|s| s.nav) {
        if s.group != last_group {
            let first = last_group.is_empty();
            let g = widgets::label(&s.group.to_uppercase(), "nav-group");
            if first {
                g.add_css_class("first");
            }
            // The first group in the app's accent, the others from the theme's [sections.fonts].
            let section = if first { nyx_ui::sidebar::ACCENT_SECTION.to_string() } else { s.group.to_lowercase() };
            nyx_ui::sidebar::set_section(&g, &section);
            list.append(&g);
            last_group = s.group;
        }
        let (button, label, count) = nav_button(s.icon, s.title, s.description);
        label.add_css_class("compact-hide");
        let id = s.id;
        button.connect_clicked(move |_| navigate(id));
        list.append(&button);
        nav_items.insert(s.id.to_string(), button);
        counts.push((s.id.to_string(), count));
    }
    let refresh_counts = move || {
        let values = nav_counts();
        for (id, label) in &counts {
            let text = values.iter().find(|(i, _)| i == id).map(|(_, t)| t.clone()).unwrap_or_default();
            label.set_text(&text);
        }
    };
    refresh_counts();
    events::subscribe(&list, move |c| {
        if matches!(c, events::Change::Catalog | events::Change::Installed) {
            refresh_counts();
        }
    });
    let nav_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        // Scrolls with wheel, trackpad and keyboard; no visible scrollbar.
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .child(&list)
        .build();
    nav.append(&nav_scroll);

    // ----- Content -----
    let stack = gtk::Stack::new();
    stack.add_css_class("settings-content");
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 160 });

    // ----- Header -----
    // The app and the page, the sidebar toggle; the search across both catalogs in the
    // middle; what's installed or installing, and Settings at the end.
    let header = chrome::app_header(&window, "font-x-generic-symbolic", "Fonts");
    let collapse = widgets::bar_button("sidebar-show-symbolic", "Collapse the sidebar (Ctrl+B)");
    collapse.connect_clicked(|_| toggle_sidebar());
    header.start.append(&collapse);
    let search = search_entry();
    header.center.append(&search);
    header.end.set_spacing(4);
    let readout = library_readout();
    header.end.append(&readout);
    header.end.append(&chrome::settings_button(settings_dialog::open));

    let frame = nyx_ui::frame::AppFrame::new(&header.root);
    frame.set_sidebar(&nav);
    frame.set_page(&stack);
    let overlay = frame.install(&window);
    accept_font_drops(&window);

    install_keys(&window, &search);

    // Narrow windows (a tiled half-screen) get an icon-only sidebar.
    let apply_width = {
        let nav = nav.clone();
        move |w: &gtk::ApplicationWindow| {
            let width = if w.width() > 0 { w.width() } else { w.default_width() };
            let narrow = width > 0 && width < 980;
            settings_dialog::fit(w);
            if narrow == NARROW.with(|n| n.get()) && nav.has_css_class("sized") {
                return;
            }
            nav.add_css_class("sized");
            set_narrow(narrow);
            apply_compact(&nav, narrow || prefs::get().sidebar_collapsed);
        }
    };
    let aw = apply_width.clone();
    window.connect_default_width_notify(move |w| aw(w));
    let aw = apply_width.clone();
    window.connect_realize(move |w| aw(w));
    // Tiled windows are resized by the compositor without touching the default
    // size: an invisible layer over the whole window reports each real size
    // change, and the layout follows on the next frame.
    let probe = gtk::DrawingArea::new();
    probe.set_can_target(false);
    probe.set_can_focus(false);
    overlay.add_overlay(&probe);
    overlay.set_measure_overlay(&probe, false);
    {
        let (aw, w2) = (apply_width.clone(), window.clone());
        probe.connect_resize(move |_, _, _| {
            let (aw, w2) = (aw.clone(), w2.clone());
            // After this layout pass, when the window's width is the new one.
            glib::idle_add_local_once(move || aw(&w2));
        });
    }
    // A slow fallback, in case a resize slips by.
    let w2 = window.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(1500), move || {
        apply_width(&w2);
        glib::ControlFlow::Continue
    });

    let ui = Ui {
        window,
        stack,
        nav,
        nav_items,
        search,
        pages: HashMap::new(),
        sections,
        current: String::new(),
        before_search: String::new(),
        history: Vec::new(),
        overlay,
        header,
        readout,
    };
    UI.with(|u| *u.borrow_mut() = Some(Rc::new(RefCell::new(ui))));
}

/// The search's width in characters, in a full window and a narrow one.
const SEARCH_WIDE: i32 = 30;
const SEARCH_NARROW: i32 = 12;

/// The search across both catalogs, always in the header (Ctrl+K).
fn search_entry() -> gtk::SearchEntry {
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search both catalogs  (Ctrl+K)"));
    search.add_css_class("bar-search");
    search.set_width_chars(SEARCH_WIDE);
    search.update_property(&[gtk::accessible::Property::Label("Search both catalogs")]);
    search.connect_search_changed(|e| on_search(&e.text()));
    search.connect_activate(|_| sections::search::focus_results());
    search.connect_stop_search(|e| e.set_text(""));
    search
}

/// What's in the catalogs and installed, or the install under way.
fn library_readout() -> gtk::Label {
    let readout = widgets::label("", "header-status");
    readout.set_ellipsize(gtk::pango::EllipsizeMode::End);
    readout.set_margin_end(6);
    let refresh = {
        let readout = readout.clone();
        move || {
            let text = if let Some(job) = install::status() {
                job
            } else if catalog::loading() {
                "Updating the font lists…".to_string()
            } else {
                let mut parts = Vec::new();
                if catalog::loaded() {
                    parts.push(format!("{} Google", fmt::thousands(catalog::google().len())));
                    parts.push(format!("{} Nerd", catalog::nerd().len()));
                }
                parts.push(format!("{} installed", installed::all().len()));
                parts.join(" · ")
            };
            readout.set_text(&text);
        }
    };
    refresh();
    events::subscribe(&readout, move |_| refresh());
    readout
}

/// The sidebar shows only icons: hide the labels (everything marked
/// `compact-hide`), show what's marked `compact-show`, centre the icons and the toggle.
fn apply_compact(nav: &gtk::Box, compact: bool) {
    if compact {
        nav.add_css_class("compact");
    } else {
        nav.remove_css_class("compact");
    }
    set_compact_hidden(nav, compact);
}

pub fn toggle_sidebar() {
    prefs::update(|p| p.sidebar_collapsed = !p.sidebar_collapsed);
    let Some(ui) = ui() else { return };
    let nav = ui.borrow().nav.clone();
    apply_compact(&nav, NARROW.with(|n| n.get()) || prefs::get().sidebar_collapsed);
}

fn set_compact_hidden(root: &gtk::Box, compact: bool) {
    fn walk(w: &gtk::Widget, compact: bool) {
        if w.has_css_class("compact-hide") {
            w.set_visible(!compact);
        }
        if w.has_css_class("compact-show") {
            w.set_visible(compact);
        }
        // Icon-only: centre the icon in its pill, and the toggle in the column.
        if w.has_css_class("nav-item")
            && let Some(content) = w.downcast_ref::<gtk::Button>().and_then(|b| b.child())
        {
            content.set_halign(if compact { gtk::Align::Center } else { gtk::Align::Fill });
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            walk(&c, compact);
            child = c.next_sibling();
        }
    }
    walk(root.upcast_ref(), compact);
}

/// The visible page's own filter field, when it has one.
fn page_filter() -> Option<gtk::SearchEntry> {
    fn find(w: &gtk::Widget) -> Option<gtk::SearchEntry> {
        if w.has_css_class("page-search") && w.is_mapped() {
            return w.downcast_ref::<gtk::SearchEntry>().cloned();
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(e) = find(&c) {
                return Some(e);
            }
            child = c.next_sibling();
        }
        None
    }
    let ui = ui()?;
    let page = {
        let u = ui.borrow();
        u.pages.get(&u.current).cloned()
    }?;
    find(&page)
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
        match key {
            // A page with its own filter gets Ctrl+F; Ctrl+K always searches both catalogs.
            gdk::Key::f if ctrl => {
                if let Some(f) = page_filter() {
                    f.grab_focus();
                } else {
                    s2.grab_focus();
                }
                done
            }
            gdk::Key::k if ctrl => {
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
            gdk::Key::b if ctrl => {
                toggle_sidebar();
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
            gdk::Key::Left if alt => {
                back();
                done
            }
            _ if ctrl && !alt && key.to_unicode().and_then(|c| c.to_digit(10)).is_some_and(|d| d >= 1) => {
                let n = key.to_unicode().and_then(|c| c.to_digit(10)).unwrap_or(1) as usize;
                let id = ui().and_then(|u| u.borrow().sections.iter().filter(|s| s.nav).nth(n - 1).map(|s| s.id));
                if let Some(id) = id {
                    navigate(id);
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

    // The mouse's back button.
    let mouse_back = gtk::GestureClick::new();
    mouse_back.set_button(8);
    mouse_back.set_propagation_phase(gtk::PropagationPhase::Capture);
    mouse_back.connect_pressed(|g, _, _, _| {
        g.set_state(gtk::EventSequenceState::Claimed);
        back();
    });
    window.add_controller(mouse_back);
}

/// Every keyboard shortcut, for Settings and the F1 list.
pub const SHORTCUTS: &[(&[&str], &str)] = &[
    (&["Ctrl", "F"], "Filter this page"),
    (&["Ctrl", "K"], "Search both catalogs"),
    (&["Esc"], "Clear the search"),
    (&["Ctrl", "1–4"], "Go to a page in the sidebar"),
    (&["Alt", "←"], "Back"),
    (&["Ctrl", "B"], "Collapse or expand the sidebar"),
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

/// Back to the page before.
pub fn back() {
    let Some(ui) = ui() else { return };
    let prev = ui.borrow_mut().history.pop();
    if let Some(id) = prev {
        go(&id, false);
    }
}

fn on_search(text: &str) {
    let Some(ui) = ui() else { return };
    let q = text.trim().to_string();
    if q.is_empty() {
        let back = std::mem::take(&mut ui.borrow_mut().before_search);
        if ui.borrow().current == "search" {
            navigate(if back.is_empty() { "google" } else { &back });
        }
        return;
    }
    if ui.borrow().current != "search" {
        let cur = ui.borrow().current.clone();
        ui.borrow_mut().before_search = cur;
    }
    navigate("search");
    sections::search::set_query(&q);
}

/// Narrow windows (a tiled half-screen) mark their pages, and get a shorter search and
/// no library counts in the header.
fn set_narrow(narrow: bool) {
    NARROW.with(|n| n.set(narrow));
    let Some(ui) = ui() else { return };
    let u = ui.borrow();
    for page in u.pages.values() {
        mark_page(page, narrow);
    }
    u.search.set_width_chars(if narrow { SEARCH_NARROW } else { SEARCH_WIDE });
    u.readout.set_visible(!narrow);
}

pub fn narrow() -> bool {
    NARROW.with(|n| n.get())
}

fn mark_page(page: &gtk::Widget, narrow: bool) {
    let body = page
        .downcast_ref::<gtk::ScrolledWindow>()
        .and_then(|s| s.child())
        .and_then(|v| v.first_child())
        .unwrap_or_else(|| page.clone());
    if narrow {
        body.add_css_class("narrow");
    } else {
        body.remove_css_class("narrow");
    }
}

fn ensure_built(id: &str) -> bool {
    let Some(ui) = ui() else { return false };
    if ui.borrow().pages.contains_key(id) {
        return true;
    }
    let section = {
        let u = ui.borrow();
        u.sections.iter().find(|s| s.id == id && s.id != "settings").map(|s| (s.id, s.build, s.fill))
    };
    let Some((sid, build, fill)) = section else { return false };
    let page = widgets::page(sid);
    if fill {
        page.fill();
    }
    build(&page);
    let page: gtk::Widget = page.root.upcast();
    mark_page(&page, narrow());
    let stack = ui.borrow().stack.clone();
    stack.add_named(&page, Some(id));
    ui.borrow_mut().pages.insert(id.to_string(), page);
    true
}

pub fn navigate(id: &str) {
    go(id, true);
}

fn go(id: &str, record: bool) {
    let Some(ui) = ui() else { return };
    // Settings is a card over the window, not a page.
    if id == "settings" {
        settings_dialog::open();
        if !ui.borrow().current.is_empty() {
            return;
        }
    }
    let id = if ensure_built(id) { id.to_string() } else { "google".to_string() };
    if !ensure_built(&id) {
        return;
    }
    let mut u = ui.borrow_mut();
    if record && u.current != id && !u.current.is_empty() && u.current != "search" {
        let prev = u.current.clone();
        u.history.push(prev);
        if u.history.len() > 50 {
            u.history.remove(0);
        }
    }
    if let Some(prev) = u.nav_items.get(&u.current) {
        prev.remove_css_class("active");
    }
    if let Some(b) = u.nav_items.get(&id) {
        b.add_css_class("active");
    }
    u.stack.set_visible_child_name(&id);
    u.current = id.clone();
    let title = page_title(&u.sections, &id);
    chrome::set_title(&u.window, "Fonts", &title);
    u.header.set_subtitle(&title);
    let search = u.search.clone();
    drop(u);
    if id != "search" {
        if !search.text().is_empty() {
            ui.borrow_mut().before_search.clear();
            search.set_text("");
        }
        prefs::update(|p| p.last_section = id);
    }
}

fn page_title(sections: &[Section], id: &str) -> String {
    sections.iter().find(|s| s.id == id).map(|s| s.title.to_string()).unwrap_or_default()
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

/// Font files dropped on the window are installed for this user: copied into
/// `~/.local/share/fonts/nyx-fonts/local`, then the font cache is refreshed.
fn accept_font_drops(window: &gtk::ApplicationWindow) {
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    drop.connect_drop(|_, value, _, _| {
        let Ok(list) = value.get::<gdk::FileList>() else { return false };
        let fonts: Vec<std::path::PathBuf> = list
            .files()
            .iter()
            .filter_map(|f| f.path())
            .filter(|p| {
                let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                matches!(ext.as_str(), "ttf" | "otf" | "ttc")
            })
            .collect();
        if fonts.is_empty() {
            toast("Drop .ttf, .otf or .ttc files to install them.");
            return false;
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
                Ok(n) => toast(&format!("Installed {}. Find them in System fonts, under User.", fmt::count(n, "font file", "font files"))),
                Err(e) => toast(&format!("Couldn't install the fonts: {e}")),
            },
        );
        true
    });
    window.add_controller(drop);
}

