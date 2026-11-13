//! The list beside the page: Google Fonts, Nerd Fonts and This computer, each name
//! drawn in itself. Pointing at a font tries it in the slot being chosen; picking
//! one sets it there. This computer holds updates, the fonts installed with Fonts
//! (Remove), and every other font on the system.

use crate::catalog::{self, FontEntry, PreviewFile, Source};
use crate::installed::Record;
use crate::slots::{self, Slot};
use crate::{cmd, events, fmt, install, installed, names, paths, prefs, system, typeset, widgets};
use gtk::prelude::*;
use gtk::{gio, glib, pango};
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Google,
    Nerd,
    Computer,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Google, Tab::Nerd, Tab::Computer];

    pub fn id(self) -> &'static str {
        match self {
            Tab::Google => "google",
            Tab::Nerd => "nerd",
            Tab::Computer => "computer",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::Google => "Google",
            Tab::Nerd => "Nerd Fonts",
            Tab::Computer => "This computer",
        }
    }

    /// A tab by its id, or by a page Fonts had before (installed, system).
    pub fn from_id(id: &str) -> Option<Tab> {
        match id {
            "google" => Some(Tab::Google),
            "nerd" => Some(Tab::Nerd),
            "computer" | "installed" | "system" => Some(Tab::Computer),
            _ => None,
        }
    }
}

const GOOGLE_SORTS: &[(&str, &str)] = &[("popular", "Popular"), ("trending", "Trending"), ("name", "Name"), ("newest", "Newest")];
const GOOGLE_CATEGORIES: &[(&str, &str)] = &[
    ("", "All categories"),
    ("Sans serif", "Sans serif"),
    ("Serif", "Serif"),
    ("Display", "Display"),
    ("Handwriting", "Handwriting"),
    ("Monospace", "Monospace"),
];
const NERD_CATEGORIES: &[(&str, &str)] = &[("", "All kinds"), ("Monospace", "Monospace"), ("Proportional", "Proportional")];
/// A few Nerd Fonts icons beside each Nerd Font: folder, branch, terminal, Rust, bolt, arrow.
const GLYPHS: &str = "\u{f07b} \u{e725} \u{f489} \u{e7a8} \u{f0e7} \u{e0b0}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadAction {
    None,
    UpdateAll,
    RemoveAll,
    Reload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Catalog,
    Update,
    Installed,
    System,
}

enum Item {
    Header { title: String, count: usize, action: HeadAction },
    Font { entry: Box<FontEntry>, kind: Kind },
}

fn item_of(obj: &glib::Object) -> Option<std::cell::Ref<'_, Item>> {
    obj.downcast_ref::<glib::BoxedAnyObject>().map(|b| b.borrow::<Item>())
}

fn font_of(obj: &glib::Object) -> Option<FontEntry> {
    match &*item_of(obj)? {
        Item::Font { entry, .. } => Some((**entry).clone()),
        Item::Header { .. } => None,
    }
}

fn compare(sort: &str, a: &FontEntry, b: &FontEntry) -> Ordering {
    let name = || a.family.to_lowercase().cmp(&b.family.to_lowercase());
    let rank = |x: u32| if x == 0 { u32::MAX } else { x };
    match sort {
        "popular" => rank(a.popularity).cmp(&rank(b.popularity)).then_with(name),
        "trending" => rank(a.trending).cmp(&rank(b.trending)).then_with(name),
        "newest" => b.added.cmp(&a.added).then_with(name),
        _ => name(),
    }
}

/// An installed font as a row: the catalog's entry when it has one, previewed from
/// the installed files, showing the installed version.
fn installed_entry(r: &Record) -> FontEntry {
    let mut e = catalog::find(r.source, &r.id).unwrap_or_else(|| install::stub(r));
    let font = r.files.iter().find(|f| {
        let f = f.to_ascii_lowercase();
        f.ends_with(".ttf") || f.ends_with(".otf")
    });
    let regular = r.files.iter().find(|f| f.contains("-Regular.")).or(font);
    if let Some(f) = regular {
        e.preview = PreviewFile::Local(r.dir.join(f));
    }
    e.version = r.version.clone();
    e
}

/// A font file dropped on the window (or picked), which Fonts keeps apart from its installs.
fn added_by_you(e: &FontEntry) -> bool {
    matches!(&e.preview, PreviewFile::Local(p) if p.starts_with(paths::fonts_dir().join("local")))
}

fn short_source(s: Source) -> &'static str {
    match s {
        Source::Google => "Google",
        Source::Nerd => "Nerd Fonts",
        Source::System => "System",
    }
}

struct Inner {
    tab: Cell<Tab>,
    query: RefCell<String>,
    google_category: RefCell<String>,
    nerd_category: RefCell<String>,
    store: gio::ListStore,
    selection: gtk::SingleSelection,
    view: gtk::ListView,
    /// Set while the list moves its own selection, which isn't a pick.
    guard: Cell<bool>,
    /// The slot and font the selection last followed.
    synced: RefCell<(Slot, (Source, String))>,
    tabs: Vec<(Tab, gtk::ToggleButton)>,
    filter_row: gtk::Box,
    count: gtk::Label,
    google_cat: gtk::DropDown,
    nerd_cat: gtk::DropDown,
    sort: gtk::DropDown,
    for_line: gtk::Box,
    for_label: gtk::Label,
    stack: gtk::Stack,
    empty: gtk::Box,
}

#[derive(Clone)]
pub struct Picker {
    pub root: gtk::Box,
    inner: Rc<Inner>,
}

impl Picker {
    pub fn new() -> Picker {
        let root = widgets::vbox(0);
        root.add_css_class("picker");
        root.set_size_request(330, -1);
        root.set_hexpand(false);

        // The three tabs, across the top.
        let seg = widgets::hbox(0);
        seg.add_css_class("segmented");
        seg.add_css_class("picker-tabs");
        let mut tabs = Vec::new();
        let mut first: Option<gtk::ToggleButton> = None;
        for t in Tab::ALL {
            let b = gtk::ToggleButton::with_label(t.title());
            b.add_css_class("segment");
            b.set_hexpand(true);
            if let Some(f) = &first {
                b.set_group(Some(f));
            } else {
                first = Some(b.clone());
            }
            seg.append(&b);
            tabs.push((t, b));
        }
        root.append(&seg);

        // The count, and the category and order.
        let filter_row = widgets::hbox(6);
        filter_row.add_css_class("picker-filter");
        let count = widgets::label("", "picker-count");
        count.set_hexpand(true);
        count.set_ellipsize(pango::EllipsizeMode::End);
        filter_row.append(&count);
        let google_cat = widgets::dropdown(&widgets::opts(GOOGLE_CATEGORIES), "");
        google_cat.set_tooltip_text(Some("Category"));
        let nerd_cat = widgets::dropdown(&widgets::opts(NERD_CATEGORIES), "");
        nerd_cat.set_tooltip_text(Some("Kind"));
        let sort = widgets::dropdown(&widgets::opts(GOOGLE_SORTS), &prefs::get().google_sort);
        sort.set_tooltip_text(Some("Order"));
        for d in [&google_cat, &nerd_cat, &sort] {
            d.add_css_class("picker-dropdown");
            filter_row.append(d);
        }
        root.append(&filter_row);

        // Which slot the list is choosing for.
        let for_line = widgets::hbox(6);
        for_line.add_css_class("for-line");
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("dot");
        dot.set_valign(gtk::Align::Center);
        for_line.append(&dot);
        let for_label = gtk::Label::new(None);
        for_label.set_xalign(0.0);
        for_label.set_wrap(true);
        for_label.set_hexpand(true);
        for_line.append(&for_label);
        root.append(&for_line);

        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);
        let view = gtk::ListView::new(Some(selection.clone()), Some(factory()));
        view.add_css_class("pick-list");
        let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&view).build();
        let stack = gtk::Stack::new();
        stack.add_css_class("picker-body");
        stack.set_vexpand(true);
        stack.add_named(&scroll, Some("list"));
        let loading = widgets::vbox(10);
        loading.set_valign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_size_request(28, 28);
        loading.append(&spinner);
        let l = widgets::label("Loading the font lists…", "dim");
        l.set_xalign(0.5);
        loading.append(&l);
        stack.add_named(&loading, Some("loading"));
        let empty = widgets::vbox(0);
        empty.set_vexpand(true);
        stack.add_named(&empty, Some("empty"));
        root.append(&stack);

        let foot = gtk::Button::new();
        foot.add_css_class("drop-foot");
        let fc = widgets::hbox(7);
        fc.set_halign(gtk::Align::Center);
        fc.append(&gtk::Image::from_icon_name("folder-download-symbolic"));
        let fl = gtk::Label::new(Some("Drop .ttf or .otf files here to install them"));
        fl.set_ellipsize(pango::EllipsizeMode::End);
        fc.append(&fl);
        foot.set_child(Some(&fc));
        foot.set_tooltip_text(Some("Or click to choose font files"));
        foot.connect_clicked(|_| choose_files());
        root.append(&foot);

        let inner = Rc::new(Inner {
            tab: Cell::new(Tab::Google),
            query: RefCell::default(),
            google_category: RefCell::default(),
            nerd_category: RefCell::default(),
            store,
            selection,
            view,
            guard: Cell::new(false),
            synced: RefCell::new((slots::active(), slots::key(slots::active()))),
            tabs,
            filter_row,
            count,
            google_cat,
            nerd_cat,
            sort,
            for_line,
            for_label,
            stack,
            empty,
        });
        let p = Picker { root, inner };
        p.connect();
        p
    }

    fn connect(&self) {
        let i = &self.inner;
        for (t, b) in &i.tabs {
            let (p, t) = (self.clone(), *t);
            b.connect_toggled(move |b| {
                if b.is_active() && p.inner.tab.get() != t {
                    p.inner.tab.set(t);
                    prefs::update(|pr| pr.last_section = t.id().to_string());
                    crate::window::tab_shown(t);
                    p.rebuild(true);
                }
            });
        }
        let p = self.clone();
        i.google_cat.connect_selected_notify(move |d| {
            let c = GOOGLE_CATEGORIES.get(d.selected() as usize).map(|(id, _)| id.to_string()).unwrap_or_default();
            *p.inner.google_category.borrow_mut() = c;
            p.rebuild(true);
        });
        let p = self.clone();
        i.nerd_cat.connect_selected_notify(move |d| {
            let c = NERD_CATEGORIES.get(d.selected() as usize).map(|(id, _)| id.to_string()).unwrap_or_default();
            *p.inner.nerd_category.borrow_mut() = c;
            p.rebuild(true);
        });
        let p = self.clone();
        i.sort.connect_selected_notify(move |d| {
            if let Some((id, _)) = GOOGLE_SORTS.get(d.selected() as usize)
                && prefs::get().google_sort != *id
            {
                prefs::update(|pr| pr.google_sort = id.to_string());
                p.rebuild(true);
            }
        });
        // Picking a row sets it in the slot.
        let p = self.clone();
        i.selection.connect_selected_item_notify(move |s| {
            if p.inner.guard.get() {
                return;
            }
            if let Some(e) = s.selected_item().and_then(|o| font_of(&o)) {
                *p.inner.synced.borrow_mut() = (slots::active(), e.key());
                slots::choose(&e);
            }
        });
        // A list filled before it was on screen scrolls to the slot's font once it is.
        let p = self.clone();
        i.view.connect_map(move |_| {
            let p = p.clone();
            glib::idle_add_local_once(move || {
                let n = p.inner.selection.selected();
                if n != gtk::INVALID_LIST_POSITION && p.inner.tab.get() != Tab::Computer {
                    p.inner.view.scroll_to(n, gtk::ListScrollFlags::NONE, None);
                }
            });
        });
        // Leaving the list stops trying.
        let motion = gtk::EventControllerMotion::new();
        motion.connect_leave(|_| slots::try_font(None));
        i.view.add_controller(motion);

        let p = self.clone();
        events::subscribe(&self.root, move |c| match c {
            // The first fill (the lists just read) goes to the slot's font.
            events::Change::Catalog | events::Change::Installed | events::Change::System => {
                p.rebuild(p.inner.store.n_items() == 0)
            }
            events::Change::Slots => {
                p.update_for_line();
                let now = (slots::active(), slots::key(slots::active()));
                if *p.inner.synced.borrow() != now {
                    p.sync_selection(true);
                }
            }
            events::Change::Preferences => {
                let sort = prefs::get().google_sort;
                let at = GOOGLE_SORTS.iter().position(|(id, _)| *id == sort).unwrap_or(0) as u32;
                if p.inner.sort.selected() != at {
                    p.inner.sort.set_selected(at);
                }
            }
            events::Change::Jobs => {}
        });
    }

    /// Show a tab.
    pub fn show_tab(&self, tab: Tab) {
        if let Some((_, b)) = self.inner.tabs.iter().find(|(t, _)| *t == tab) {
            if b.is_active() {
                self.rebuild(true);
            } else {
                b.set_active(true);
            }
        }
    }

    /// Search both catalogs and this computer ("" goes back to the tab).
    pub fn set_query(&self, q: &str) {
        if *self.inner.query.borrow() == q {
            return;
        }
        *self.inner.query.borrow_mut() = q.to_string();
        self.rebuild(true);
    }

    pub fn focus_list(&self) {
        self.inner.view.grab_focus();
    }

    pub fn set_narrow(&self, narrow: bool) {
        self.root.set_size_request(if narrow { 300 } else { 330 }, -1);
    }

    fn items(&self) -> Vec<Item> {
        let i = &self.inner;
        let q = i.query.borrow().clone();
        let font = |entry: &FontEntry, kind| Item::Font { entry: Box::new(entry.clone()), kind };
        let header = |title: &str, count, action| Item::Header { title: title.to_string(), count, action };
        if !q.trim().is_empty() {
            let terms = catalog::terms(&q);
            let mut google: Vec<FontEntry> = catalog::google().iter().filter(|e| e.matches(&terms)).cloned().collect();
            google.sort_by(|a, b| compare("popular", a, b));
            let nerd: Vec<FontEntry> = catalog::nerd().iter().filter(|e| e.matches(&terms)).cloned().collect();
            let local: Vec<FontEntry> = system::cached().iter().filter(|e| e.matches(&terms)).cloned().collect();
            let mut out = Vec::new();
            for (title, list, kind) in [
                ("Google Fonts", google, Kind::Catalog),
                ("Nerd Fonts", nerd, Kind::Catalog),
                ("This computer", local, Kind::System),
            ] {
                if !list.is_empty() {
                    out.push(header(title, list.len(), HeadAction::None));
                    out.extend(list.iter().map(|e| font(e, kind)));
                }
            }
            return out;
        }
        match i.tab.get() {
            Tab::Google => {
                let cat = i.google_category.borrow().clone();
                let sort = prefs::get().google_sort;
                let mut list: Vec<FontEntry> =
                    catalog::google().iter().filter(|e| cat.is_empty() || e.category == cat).cloned().collect();
                list.sort_by(|a, b| compare(&sort, a, b));
                list.iter().map(|e| font(e, Kind::Catalog)).collect()
            }
            Tab::Nerd => {
                let cat = i.nerd_category.borrow().clone();
                catalog::nerd().iter().filter(|e| cat.is_empty() || e.category == cat).map(|e| font(e, Kind::Catalog)).collect()
            }
            Tab::Computer => {
                let all = installed::all();
                let (old, current): (Vec<Record>, Vec<Record>) = all.into_iter().partition(Record::outdated);
                let mut out = Vec::new();
                if !old.is_empty() {
                    out.push(header("Updates", old.len(), HeadAction::UpdateAll));
                    out.extend(old.iter().map(|r| font(&installed_entry(r), Kind::Update)));
                }
                if !current.is_empty() {
                    out.push(header("Installed with Fonts", current.len(), HeadAction::RemoveAll));
                    out.extend(current.iter().map(|r| font(&installed_entry(r), Kind::Installed)));
                }
                let others: Vec<FontEntry> =
                    system::cached().iter().filter(|e| e.category != "Installed here" || added_by_you(e)).cloned().collect();
                out.push(header("System and yours", others.len(), HeadAction::Reload));
                out.extend(others.iter().map(|e| font(e, Kind::System)));
                out
            }
        }
    }

    /// Fill the list for the tab, the filters or the search; `jump` scrolls to the
    /// slot's font (a new tab or filter) rather than keeping the place.
    fn rebuild(&self, jump: bool) {
        let i = &self.inner;
        let items = self.items();
        let fonts = items.iter().filter(|it| matches!(it, Item::Font { .. })).count();
        let objs: Vec<glib::BoxedAnyObject> = items.into_iter().map(glib::BoxedAnyObject::new).collect();
        i.guard.set(true);
        i.store.splice(0, i.store.n_items(), &objs);
        i.guard.set(false);
        self.sync_selection(jump);

        let tab = i.tab.get();
        let searching = !i.query.borrow().trim().is_empty();
        i.google_cat.set_visible(!searching && tab == Tab::Google);
        i.sort.set_visible(!searching && tab == Tab::Google);
        i.nerd_cat.set_visible(!searching && tab == Tab::Nerd);
        i.filter_row.set_visible(searching || tab != Tab::Computer);
        i.count.set_text(&if searching {
            fmt::count(fonts, "result", "results")
        } else if tab == Tab::Nerd {
            format!("{} families, icons included", fmt::thousands(fonts))
        } else {
            fmt::thousands(fonts)
        });
        self.update_for_line();

        // A spinner while the lists load, a message when there's nothing.
        let show_empty = |title: &str, desc: &str, retry: bool| {
            while let Some(c) = i.empty.first_child() {
                i.empty.remove(&c);
            }
            let action: widgets::Action<'_> = if retry { Some(("Try again", Box::new(catalog::refresh))) } else { None };
            i.empty.append(&widgets::empty_state("font-x-generic-symbolic", title, desc, action));
            i.stack.set_visible_child_name("empty");
        };
        let catalog_tab = searching || tab != Tab::Computer;
        if i.store.n_items() > 0 {
            i.stack.set_visible_child_name("list");
        } else if catalog_tab && (!catalog::loaded() || (catalog::loading() && catalog::google().is_empty())) {
            i.stack.set_visible_child_name("loading");
        } else if let Some(e) = catalog::error().filter(|_| catalog_tab && catalog::google().is_empty()) {
            show_empty("Couldn't load the font lists", &glib::markup_escape_text(&e), true);
        } else if searching {
            let q = i.query.borrow().trim().to_string();
            show_empty("Nothing found", &format!("No fonts match “{}”.", glib::markup_escape_text(&q)), false);
        } else {
            show_empty("No fonts", "No fonts in this category.", false);
        }
    }

    fn update_for_line(&self) {
        let i = &self.inner;
        let slot = slots::active();
        let what = match slot {
            Slot::Heading => "Choosing the <b>heading</b>: point at a font to try it",
            Slot::Body => "Choosing the <b>body</b>: point at a font to try it",
            Slot::Code => "Choosing the <b>code</b> font",
        };
        i.for_label.set_markup(what);
        for s in Slot::ALL {
            i.for_line.remove_css_class(&format!("slot-{}", s.id()));
        }
        i.for_line.add_css_class(&format!("slot-{}", slot.id()));
        let searching = !i.query.borrow().trim().is_empty();
        i.for_line.set_visible(searching || i.tab.get() != Tab::Computer);
    }

    /// Select the row of the slot's font, if the list has it.
    fn sync_selection(&self, scroll: bool) {
        let i = &self.inner;
        let slot = slots::active();
        let key = slots::key(slot);
        *i.synced.borrow_mut() = (slot, key.clone());
        let found = (0..i.store.n_items()).find(|n| i.store.item(*n).and_then(|o| font_of(&o)).is_some_and(|e| e.key() == key));
        i.guard.set(true);
        match found {
            Some(n) => {
                i.selection.set_selected(n);
                // This computer opens at its top, on the updates.
                let top = i.tab.get() == Tab::Computer && i.query.borrow().trim().is_empty();
                if scroll {
                    i.view.scroll_to(if top { 0 } else { n }, gtk::ListScrollFlags::NONE, None);
                }
            }
            None => {
                i.selection.set_selected(gtk::INVALID_LIST_POSITION);
                if scroll && i.store.n_items() > 0 {
                    i.view.scroll_to(0, gtk::ListScrollFlags::NONE, None);
                }
            }
        }
        i.guard.set(false);
    }
}

/// One row's widgets: a section heading, or a font.
struct Row {
    heading: gtk::Box,
    sec_title: gtk::Label,
    sec_count: gtk::Label,
    sec_action: gtk::Button,
    action: Cell<HeadAction>,
    font: gtk::Box,
    name: gtk::Label,
    meta: gtk::Label,
    glyphs: gtk::Label,
    tags: Vec<(Slot, gtk::Label)>,
    end: gtk::Box,
    /// What `end` shows, so progress doesn't rebuild its buttons.
    end_state: RefCell<String>,
    /// The font the name is drawn in.
    named: Rc<RefCell<Option<names::Key>>>,
}

fn factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, obj| {
        let Some(item) = obj.downcast_ref::<gtk::ListItem>() else { return };
        let root = widgets::vbox(0);

        let heading = widgets::hbox(6);
        heading.add_css_class("pick-sec");
        let sec_title = widgets::label("", "pick-sec-title");
        let sec_count = widgets::label("", "pick-sec-count");
        sec_count.set_hexpand(true);
        let sec_action = gtk::Button::new();
        sec_action.add_css_class("small");
        heading.append(&sec_title);
        heading.append(&sec_count);
        heading.append(&sec_action);
        root.append(&heading);

        let font = widgets::hbox(8);
        font.add_css_class("pick-row");
        let text = widgets::vbox(1);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let name = typeset::label("", "pick-name");
        name.set_ellipsize(pango::EllipsizeMode::End);
        text.append(&name);
        let meta_row = widgets::hbox(8);
        let meta = widgets::label("", "pick-meta");
        meta.set_ellipsize(pango::EllipsizeMode::End);
        meta_row.append(&meta);
        let glyphs = widgets::label(GLYPHS, "pick-glyphs");
        meta_row.append(&glyphs);
        text.append(&meta_row);
        font.append(&text);
        let mut tags = Vec::new();
        for s in Slot::ALL {
            let t = widgets::label(s.title(), "slot-tag");
            t.add_css_class(&format!("slot-{}", s.id()));
            t.set_valign(gtk::Align::Center);
            font.append(&t);
            tags.push((s, t));
        }
        let end = widgets::hbox(4);
        end.set_valign(gtk::Align::Center);
        font.append(&end);
        root.append(&font);
        item.set_child(Some(&root));

        let row = Rc::new(Row {
            heading,
            sec_title,
            sec_count,
            sec_action,
            action: Cell::new(HeadAction::None),
            font,
            name,
            meta,
            glyphs,
            tags,
            end,
            end_state: RefCell::default(),
            named: Rc::default(),
        });

        let r = row.clone();
        row.sec_action.connect_clicked(move |_| match r.action.get() {
            HeadAction::UpdateAll => install::update_all(),
            HeadAction::RemoveAll => {
                if r.sec_action.has_css_class("armed") {
                    install::remove_all();
                } else {
                    arm(&r.sec_action, "Click again to remove all", "Remove all");
                }
            }
            HeadAction::Reload => system::refresh(),
            HeadAction::None => {}
        });

        // Pointing at a font tries it.
        let motion = gtk::EventControllerMotion::new();
        let weak = item.downgrade();
        motion.connect_enter(move |_, _, _| {
            if let Some(e) = weak.upgrade().and_then(|i| i.item()).and_then(|o| font_of(&o)) {
                slots::try_font(Some(&e));
            }
        });
        row.font.add_controller(motion);

        let (r, weak) = (row.clone(), item.downgrade());
        let refresh = move || {
            if let Some(item) = weak.upgrade() {
                refresh_row(&r, &item);
            }
        };
        let f = refresh.clone();
        item.connect_item_notify(move |_| f());
        let f = refresh.clone();
        item.connect_selected_notify(move |_| f());
        events::subscribe(&root, move |c| {
            if matches!(c, events::Change::Installed | events::Change::Jobs | events::Change::Slots) {
                refresh();
            }
        });
    });
    factory
}

/// A button's first click arms it; the second acts. It disarms after a few seconds.
fn arm(b: &gtk::Button, armed: &str, idle: &'static str) {
    b.add_css_class("armed");
    b.set_label(armed);
    let b2 = b.clone();
    glib::timeout_add_local_once(std::time::Duration::from_secs(4), move || {
        b2.remove_css_class("armed");
        b2.set_label(idle);
    });
}

fn refresh_row(row: &Row, item: &gtk::ListItem) {
    let Some(obj) = item.item() else {
        row.heading.set_visible(false);
        row.font.set_visible(false);
        return;
    };
    let Some(it) = item_of(&obj) else { return };
    match &*it {
        Item::Header { title, count, action } => {
            item.set_selectable(false);
            item.set_activatable(false);
            item.set_focusable(false);
            row.heading.set_visible(true);
            row.font.set_visible(false);
            row.sec_title.set_text(&title.to_uppercase());
            row.sec_count.set_text(&fmt::thousands(*count));
            row.sec_action.set_visible(*action != HeadAction::None);
            if row.action.get() != *action {
                row.action.set(*action);
                row.sec_action.remove_css_class("armed");
                row.sec_action.remove_css_class("suggested-action");
                row.sec_action.remove_css_class("destructive-text");
                match action {
                    HeadAction::UpdateAll => {
                        row.sec_action.set_child(Some(&labeled("view-refresh-symbolic", "Update all")));
                        row.sec_action.add_css_class("suggested-action");
                        row.sec_action.set_tooltip_text(Some("Fetch the newer files of every font listed here"));
                    }
                    HeadAction::RemoveAll => {
                        row.sec_action.set_label("Remove all");
                        row.sec_action.add_css_class("destructive-text");
                        row.sec_action.set_tooltip_text(Some("Delete every font installed with this app"));
                    }
                    HeadAction::Reload => {
                        row.sec_action.set_icon_name("view-refresh-symbolic");
                        row.sec_action.set_tooltip_text(Some("Read the font list again"));
                    }
                    HeadAction::None => {}
                }
            }
        }
        Item::Font { entry, kind } => {
            item.set_selectable(true);
            item.set_activatable(true);
            item.set_focusable(true);
            row.heading.set_visible(false);
            row.font.set_visible(true);
            show_font(row, entry, *kind, item.is_selected());
        }
    }
}

fn labeled(icon: &str, text: &str) -> gtk::Box {
    let c = widgets::hbox(6);
    c.append(&gtk::Image::from_icon_name(icon));
    c.append(&gtk::Label::new(Some(text)));
    c
}

fn show_font(row: &Row, e: &FontEntry, kind: Kind, selected: bool) {
    // Every Nerd Font is called "… Nerd Font"; the icons beside it say so.
    let short = if e.source == Source::Nerd { e.family.strip_suffix(" Nerd Font") } else { None };
    row.name.set_text(short.unwrap_or(&e.family));
    let mut about = vec![e.source.label().to_string()];
    if e.variable {
        about.push("Variable".into());
    }
    if !e.license.is_empty() {
        about.push(e.license.clone());
    }
    let mut tip = format!("{}\n{}", e.family, about.join(" · "));
    if !e.detail.is_empty() {
        tip.push('\n');
        tip.push_str(&e.detail);
    }
    row.font.set_tooltip_text(Some(&tip));
    let named = Some(names::key(e));
    if *row.named.borrow() != named {
        if let Some(old) = row.named.replace(named.clone()) {
            names::forget(&old);
        }
        typeset::set(&row.name, None, 21.0);
        let (label, now) = (row.name.downgrade(), Rc::downgrade(&row.named));
        names::load(e, move |font| {
            // The row may show another font by now.
            if let (Some(label), Some(now)) = (label.upgrade(), now.upgrade())
                && *now.borrow() == named
            {
                typeset::set(&label, font.as_ref(), 21.0);
            }
        });
    }

    let record = installed::get(e.source, &e.id);
    row.meta.set_use_markup(kind == Kind::Update);
    match kind {
        Kind::Update => {
            let latest = catalog::find(e.source, &e.id).map(|c| c.version).unwrap_or_default();
            // The year once, when both are in it.
            let same_year = e.version.get(..4).is_some_and(|y| latest.starts_with(y));
            row.meta.set_markup(&format!(
                "{} · {} → <span weight=\"bold\" foreground=\"{}\">{}</span>",
                short_source(e.source),
                glib::markup_escape_text(&fmt::version(&e.version, !same_year)),
                nyx_theme::look().warning,
                glib::markup_escape_text(&fmt::version(&latest, true))
            ));
        }
        Kind::Installed => {
            let size = record.as_ref().map(|r| fmt::bytes(r.bytes)).unwrap_or_default();
            row.meta.set_text(&format!("{} · {size}", short_source(e.source)));
        }
        Kind::System => row.meta.set_text(if e.category == "User" || added_by_you(e) { "Added by you" } else { "System" }),
        Kind::Catalog => {
            let mut parts = Vec::new();
            if !e.category.is_empty() {
                parts.push(e.category.clone());
            }
            if e.styles > 0 {
                parts.push(fmt::count(e.styles, "style", "styles"));
            }
            row.meta.set_text(&parts.join(" · "));
        }
    }
    row.glyphs.set_visible(e.source == Source::Nerd && kind == Kind::Catalog);

    let in_slots = slots::slots_of(&e.key());
    for (s, t) in &row.tags {
        t.set_visible(in_slots.contains(s) && kind != Kind::Update);
    }

    // Install, Update, Remove or Open folder, and any progress.
    let busy = install::busy(&e.key());
    let outdated = record.as_ref().is_some_and(|r| r.outdated());
    let state = format!("{:?}|{kind:?}|{selected}|{}|{}|{outdated}", e.key(), busy.is_some(), record.is_some());
    if let Some(text) = &busy
        && let Some(l) = row.end.first_child().and_downcast::<gtk::Label>()
        && l.has_css_class("pick-busy")
    {
        l.set_text(text);
        *row.end_state.borrow_mut() = state;
        return;
    }
    if *row.end_state.borrow() == state {
        return;
    }
    *row.end_state.borrow_mut() = state;
    while let Some(c) = row.end.first_child() {
        row.end.remove(&c);
    }
    if let Some(text) = busy {
        let l = widgets::label(&text, "pick-busy");
        l.set_ellipsize(pango::EllipsizeMode::End);
        l.set_max_width_chars(16);
        row.end.append(&l);
        return;
    }
    let small = |b: &gtk::Button| {
        b.add_css_class("small");
        b.clone()
    };
    match (kind, record) {
        (Kind::System, _) => {
            if selected
                && let PreviewFile::Local(p) = &e.preview
                && let Some(dir) = p.parent()
            {
                let open = widgets::icon_button("folder-open-symbolic", "Open the font's folder");
                let dir = dir.to_path_buf();
                open.connect_clicked(move |_| cmd::spawn(&["xdg-open", &dir.to_string_lossy()]));
                row.end.append(&small(&open));
            }
        }
        (Kind::Update, Some(r)) => {
            let update = widgets::labeled_button("view-refresh-symbolic", "Update");
            update.set_tooltip_text(Some(&format!("{} → newer files", r.version)));
            update.connect_clicked(move |_| install::update(&r));
            row.end.append(&small(&update));
        }
        (_, Some(r)) => {
            if selected {
                let open = widgets::icon_button("folder-open-symbolic", "Open the font's folder");
                let dir = r.dir.clone();
                open.connect_clicked(move |_| cmd::spawn(&["xdg-open", &dir.to_string_lossy()]));
                row.end.append(&small(&open));
                let r2 = r.clone();
                let remove = widgets::two_click("Remove", "Remove?", move || install::remove(&r2));
                remove.remove_css_class("destructive-action");
                remove.add_css_class("destructive-text");
                remove.set_tooltip_text(Some("Delete this font's files (click twice)"));
                row.end.append(&small(&remove));
                if outdated {
                    let update = widgets::labeled_button("view-refresh-symbolic", "Update");
                    update.connect_clicked(move |_| install::update(&r));
                    row.end.append(&small(&update));
                }
            } else if outdated {
                row.end.append(&widgets::label("Update", "pick-update"));
            } else {
                let ok = gtk::Image::from_icon_name("object-select-symbolic");
                ok.add_css_class("pick-ok");
                ok.set_tooltip_text(Some("Installed"));
                row.end.append(&ok);
            }
        }
        (_, None) => {
            if selected && e.source != Source::System {
                let b = widgets::labeled_button("folder-download-symbolic", "Install");
                b.add_css_class("suggested-action");
                b.set_tooltip_text(Some("Install for your user, in ~/.local/share/fonts"));
                let e2 = e.clone();
                b.connect_clicked(move |_| install::install(&e2));
                row.end.append(&small(&b));
            }
        }
    }
}

/// Pick font files to install, as dropping them on the window does.
fn choose_files() {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Fonts"));
    for s in ["ttf", "otf", "ttc"] {
        filter.add_suffix(s);
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder().title("Install fonts").modal(true).filters(&filters).build();
    dialog.open_multiple(crate::window::window().as_ref(), gio::Cancellable::NONE, |res| {
        let Ok(list) = res else { return };
        let files: Vec<std::path::PathBuf> =
            (0..list.n_items()).filter_map(|n| list.item(n).and_downcast::<gio::File>()).filter_map(|f| f.path()).collect();
        crate::window::install_files(files);
    });
}
