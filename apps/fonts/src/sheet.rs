//! The page the fonts are set on: the Heading, Body and Code slots across the top,
//! and an article, an app's interface or a terminal set in them. The slot being
//! chosen is outlined in its colour, and shows the font pointed at in the list.

use crate::catalog::Source;
use crate::preview::{self, Loaded};
use crate::slots::{self, Slot};
use crate::{catalog, cmd, events, install, prefs, typeset, widgets};
use gtk::prelude::*;
use gtk::{glib, pango};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const PAGES: &[(&str, &str)] = &[("article", "Article"), ("interface", "Interface"), ("terminal", "Terminal")];

/// A label's size: fixed, smaller in a narrow window, or the heading's own.
#[derive(Clone, Copy)]
enum Px {
    Fixed(f64),
    Narrow(f64, f64),
    /// The heading size from Settings, times this.
    Headline(f64),
}

/// Labels set in a slot's font.
type SetIn = Rc<RefCell<Vec<(Slot, gtk::Label, Px)>>>;

struct Zone {
    slot: Slot,
    frame: gtk::Box,
    tag: gtk::Label,
}

struct Card {
    slot: Slot,
    button: gtk::Button,
    check: gtk::Image,
    chevron: gtk::Image,
}

struct Inner {
    set_in: SetIn,
    zones: Vec<Zone>,
    cards: Vec<Card>,
    bar: gtk::Box,
    spacer: gtk::Box,
    bar_end: gtk::Box,
    second_row: gtk::Box,
    pages: gtk::Stack,
    page_buttons: Vec<(&'static str, gtk::ToggleButton)>,
    install: gtk::Button,
    install_label: gtk::Label,
    headlines: Vec<gtk::Label>,
    set_line: gtk::Label,
    columns: gtk::Box,
    code_line: gtk::Label,
    variants: gtk::Box,
    variants_title: gtk::Label,
    variant_checks: Vec<(&'static str, gtk::CheckButton)>,
    /// Set while the checks follow the settings, which isn't a choice.
    syncing: Cell<bool>,
    /// The font each slot is drawn in now, and what it was loaded for.
    fonts: RefCell<[Option<Loaded>; 3]>,
    wanted: RefCell<[Option<String>; 3]>,
    generation: [Cell<u64>; 3],
    narrow: Cell<bool>,
}

#[derive(Clone)]
pub struct Sheet {
    pub root: gtk::Box,
    inner: Rc<Inner>,
}

fn idx(s: Slot) -> usize {
    Slot::ALL.iter().position(|x| *x == s).unwrap_or(0)
}

/// A label set in a slot's font.
fn fonted(set_in: &SetIn, slot: Slot, text: &str, class: &str, px: Px) -> gtk::Label {
    let l = typeset::label(text, class);
    set_in.borrow_mut().push((slot, l.clone(), px));
    l
}

fn wrapped(l: gtk::Label) -> gtk::Label {
    l.set_wrap(true);
    l.set_wrap_mode(pango::WrapMode::WordChar);
    l
}

/// A block the slot's colour outlines while it's being chosen, with a tag on its edge.
fn zone(slot: Slot, child: &impl IsA<gtk::Widget>, zones: &mut Vec<Zone>) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.add_css_class("zone-wrap");
    let frame = widgets::vbox(0);
    frame.add_css_class("zone");
    frame.add_css_class(&format!("slot-{}", slot.id()));
    frame.append(child);
    overlay.set_child(Some(&frame));
    let tag = widgets::label("", "zone-tag");
    tag.add_css_class(&format!("slot-{}", slot.id()));
    tag.set_halign(gtk::Align::End);
    tag.set_valign(gtk::Align::Start);
    tag.set_margin_end(8);
    tag.set_visible(false);
    overlay.add_overlay(&tag);
    zones.push(Zone { slot, frame, tag });
    overlay
}

fn dots() -> gtk::Box {
    let b = widgets::hbox(6);
    b.set_valign(gtk::Align::Center);
    for _ in 0..3 {
        let d = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        d.add_css_class("win-dot");
        b.append(&d);
    }
    b
}

fn swatch(name: &str) -> String {
    nyx_theme::swatches().into_iter().find(|(n, _)| n == name).map(|(_, c)| c).unwrap_or_else(|| nyx_theme::palette().accent)
}

impl Sheet {
    pub fn new() -> Sheet {
        let root = widgets::vbox(0);
        root.add_css_class("sheet");
        root.set_hexpand(true);
        let set_in: SetIn = Rc::default();
        let mut zones = Vec::new();

        // ----- The slots, the page switch, Install -----
        let top = widgets::vbox(8);
        top.add_css_class("slot-bar");
        let bar = widgets::hbox(8);
        let mut cards = Vec::new();
        for slot in Slot::ALL {
            let button = gtk::Button::new();
            button.add_css_class("slot-card");
            button.add_css_class(&format!("slot-{}", slot.id()));
            button.set_tooltip_text(Some(&format!("Choose the {} font from the list", slot.title().to_lowercase())));
            let content = widgets::hbox(0);
            let stripe = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            stripe.add_css_class("slot-stripe");
            content.append(&stripe);
            let grid = gtk::Grid::new();
            grid.set_column_spacing(8);
            grid.set_hexpand(true);
            let key = widgets::label(&slot.title().to_uppercase(), "slot-key");
            key.set_hexpand(true);
            grid.attach(&key, 0, 0, 1, 1);
            let name = fonted(&set_in, slot, "", "slot-name", Px::Narrow(15.0, 13.0));
            name.set_ellipsize(pango::EllipsizeMode::End);
            name.set_width_chars(6);
            name.set_max_width_chars(28);
            grid.attach(&name, 0, 1, 1, 1);
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.add_css_class("slot-ok");
            check.set_halign(gtk::Align::End);
            check.set_tooltip_text(Some("Installed"));
            grid.attach(&check, 1, 0, 1, 1);
            let chevron = gtk::Image::from_icon_name("pan-down-symbolic");
            chevron.add_css_class("slot-chevron");
            chevron.set_halign(gtk::Align::End);
            grid.attach(&chevron, 1, 1, 1, 1);
            content.append(&grid);
            button.set_child(Some(&content));
            button.connect_clicked(move |_| slots::set_active(slot));
            bar.append(&button);
            cards.push(Card { slot, button, check, chevron });
        }
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        bar.append(&spacer);
        let bar_end = widgets::hbox(10);
        let seg = widgets::hbox(0);
        seg.add_css_class("segmented");
        let mut page_buttons = Vec::new();
        let mut first: Option<gtk::ToggleButton> = None;
        let current = prefs::get().page;
        for (id, title) in PAGES {
            let b = gtk::ToggleButton::with_label(title);
            b.add_css_class("segment");
            if let Some(f) = &first {
                b.set_group(Some(f));
            } else {
                first = Some(b.clone());
            }
            b.set_active(current == *id);
            seg.append(&b);
            page_buttons.push((*id, b));
        }
        bar_end.append(&seg);
        let install = gtk::Button::new();
        install.add_css_class("suggested-action");
        let ic = widgets::hbox(8);
        ic.append(&gtk::Image::from_icon_name("folder-download-symbolic"));
        let install_label = gtk::Label::new(None);
        ic.append(&install_label);
        install.set_child(Some(&ic));
        install.set_tooltip_text(Some("Install every font on the page that isn't installed"));
        install.connect_clicked(|_| {
            for e in slots::to_install() {
                install::install(&e);
            }
        });
        bar_end.append(&install);
        bar.append(&bar_end);
        // The row may be narrower than the slots and the page switch for a moment, while
        // a window shrinks: it doesn't hold the window wider than it was made.
        let bar_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(true)
            .propagate_natural_height(true)
            .child(&bar)
            .build();
        top.append(&bar_scroll);
        let second_row = widgets::hbox(8);
        second_row.set_visible(false);
        top.append(&second_row);
        root.append(&top);

        // ----- The pages -----
        let pages = gtk::Stack::new();
        pages.set_transition_type(gtk::StackTransitionType::Crossfade);
        pages.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 140 });
        let mut headlines = Vec::new();

        // The article.
        let paper = widgets::vbox(0);
        paper.add_css_class("paper");
        let kicker = fonted(&set_in, Slot::Body, "HARBOUR NOTES · ISSUE 14", "kicker", Px::Fixed(11.0));
        paper.append(&kicker);
        let headline = wrapped(fonted(&set_in, Slot::Heading, "", "headline", Px::Headline(1.0)));
        headlines.push(headline.clone());
        paper.append(&zone(Slot::Heading, &headline, &mut zones));
        let standfirst = wrapped(fonted(
            &set_in,
            Slot::Body,
            "Stalls run from the lighthouse to the old ferry steps, with bread, cheese and late apples from the valley farms.",
            "standfirst",
            Px::Narrow(18.0, 14.0),
        ));
        let byline = fonted(&set_in, Slot::Body, "By Mara Quill · Saturday 14 November · 4 min read", "byline", Px::Fixed(12.0));
        let columns = widgets::hbox(24);
        columns.set_homogeneous(true);
        columns.add_css_class("columns");
        for text in [
            "The first boats come in before six, and by eight the quay smells of coffee and smoke. Bring a bag: the first hundred visitors get a free cup, and the bakery promises it won't run out of rye this time.",
            "Parking fills fast, so the ferry runs every twenty minutes on market days. Dogs are welcome on a lead, and the tide table is posted by the harbour office.",
        ] {
            let p = wrapped(fonted(&set_in, Slot::Body, text, "paragraph", Px::Narrow(14.5, 13.5)));
            p.set_valign(gtk::Align::Start);
            columns.append(&p);
        }
        let body = widgets::vbox(0);
        body.append(&standfirst);
        body.append(&byline);
        body.append(&columns);
        paper.append(&zone(Slot::Body, &body, &mut zones));
        let quote =
            wrapped(fonted(&set_in, Slot::Heading, "“We sold out of apples by ten, twice.”", "quote", Px::Narrow(22.0, 18.0)));
        paper.append(&quote);
        let code_line = fonted(&set_in, Slot::Code, "", "code-line", Px::Fixed(12.5));
        code_line.set_ellipsize(pango::EllipsizeMode::End);
        paper.append(&zone(Slot::Code, &code_line, &mut zones));
        let set_line = widgets::label("", "set-line");
        set_line.set_wrap(true);
        paper.append(&set_line);
        pages.add_named(&desk(&paper, 660), Some("article"));

        // An app's interface.
        let app = widgets::vbox(0);
        app.add_css_class("ui-window");
        let titlebar = widgets::hbox(10);
        titlebar.add_css_class("ui-titlebar");
        titlebar.append(&dots());
        titlebar.append(&fonted(&set_in, Slot::Body, "Harbour Market", "ui-title", Px::Fixed(12.0)));
        app.append(&titlebar);
        let ui_body = widgets::hbox(0);
        let side = widgets::vbox(2);
        side.add_css_class("ui-side");
        for (i, item) in ["Stalls", "Map", "Tides", "Settings"].iter().enumerate() {
            let l = fonted(&set_in, Slot::Body, item, "ui-nav", Px::Fixed(13.0));
            if i == 0 {
                l.add_css_class("active");
            }
            side.append(&l);
        }
        ui_body.append(&side);
        let main = widgets::vbox(0);
        main.add_css_class("ui-main");
        main.set_hexpand(true);
        let ui_head = wrapped(fonted(&set_in, Slot::Heading, "Saturday stalls", "ui-heading", Px::Headline(0.56)));
        headlines.push(ui_head.clone());
        main.append(&zone(Slot::Heading, &ui_head, &mut zones));
        let ui_text = widgets::vbox(10);
        ui_text.append(&wrapped(fonted(
            &set_in,
            Slot::Body,
            "46 stalls from the lighthouse to the old ferry steps. Book a pitch before Thursday noon.",
            "ui-desc",
            Px::Fixed(14.0),
        )));
        let buttons = widgets::hbox(8);
        for (text, class) in [("Book a stall", "primary"), ("Directions", "")] {
            let b = widgets::hbox(0);
            b.add_css_class("ui-button");
            if !class.is_empty() {
                b.add_css_class(class);
            }
            b.append(&fonted(&set_in, Slot::Body, text, "", Px::Fixed(13.0)));
            buttons.append(&b);
        }
        ui_text.append(&buttons);
        let table = gtk::Grid::new();
        table.add_css_class("ui-table");
        table.set_column_spacing(18);
        table.set_row_spacing(8);
        for (r, row) in [
            ["Bread and rye", "Stall 4", "06:30"],
            ["Valley apples", "Stall 12", "08:00"],
            ["Harbour coffee", "Stall 1", "05:45"],
        ]
        .iter()
        .enumerate()
        {
            for (c, cell) in row.iter().enumerate() {
                let l = fonted(&set_in, Slot::Body, cell, if c == 0 { "" } else { "dim" }, Px::Fixed(13.5));
                l.set_hexpand(c == 0);
                table.attach(&l, c as i32, r as i32, 1, 1);
            }
        }
        ui_text.append(&table);
        main.append(&zone(Slot::Body, &ui_text, &mut zones));
        let field = widgets::vbox(4);
        field.append(&fonted(&set_in, Slot::Body, "stalls.toml", "ui-field-name", Px::Fixed(11.0)));
        let ui_code = fonted(&set_in, Slot::Code, "open = \"08:00\"   # tide at 11:40 => 46 stalls", "ui-field", Px::Fixed(12.5));
        ui_code.set_ellipsize(pango::EllipsizeMode::End);
        field.append(&ui_code);
        main.append(&zone(Slot::Code, &field, &mut zones));
        ui_body.append(&main);
        app.append(&ui_body);
        pages.add_named(&desk(&app, 700), Some("interface"));

        // A terminal, with the Nerd Font variants and the font at terminal sizes.
        let term_page = widgets::vbox(0);
        let term = widgets::vbox(0);
        term.add_css_class("terminal");
        let tbar = widgets::hbox(6);
        tbar.add_css_class("term-bar");
        tbar.append(&dots());
        tbar.append(&widgets::label("kitty: ~/harbour", "term-title"));
        term.append(&tbar);
        let tbody = widgets::vbox(6);
        tbody.add_css_class("term-body");
        for line in TERMINAL {
            let l = fonted(&set_in, Slot::Code, "", "term-line", Px::Fixed(14.0));
            l.set_markup(line);
            l.set_ellipsize(pango::EllipsizeMode::End);
            tbody.append(&l);
        }
        term.append(&tbody);
        term_page.append(&term);

        let variants = widgets::vbox(0);
        variants.add_css_class("card");
        variants.add_css_class("term-card");
        let vh = widgets::hbox(10);
        vh.add_css_class("term-card-head");
        let variants_title = widgets::label("", "term-card-title");
        vh.append(&variants_title);
        let vsub = widgets::label("Which Nerd Font variants to keep", "dim");
        vsub.set_ellipsize(pango::EllipsizeMode::End);
        vh.append(&vsub);
        variants.append(&vh);
        let mut variant_checks = Vec::new();
        for (id, title, desc) in VARIANTS {
            let check = gtk::CheckButton::new();
            check.add_css_class("variant");
            let text = widgets::vbox(1);
            text.append(&widgets::label(title, "variant-title"));
            text.append(&widgets::label(desc, "variant-desc"));
            check.set_child(Some(&text));
            variants.append(&check);
            variant_checks.push((*id, check));
        }
        term_page.append(&variants);

        let sizes = widgets::vbox(0);
        sizes.add_css_class("card");
        sizes.add_css_class("term-card");
        let sh = widgets::label("At terminal sizes", "term-card-title");
        sh.add_css_class("term-card-head");
        sizes.append(&sh);
        for px in [11.0, 13.0, 16.0] {
            let row = widgets::hbox(12);
            row.add_css_class("size-row");
            let l = widgets::label(&format!("{px:.0} px"), "size-px");
            l.set_width_chars(5);
            row.append(&l);
            let sample = fonted(
                &set_in,
                Slot::Code,
                "\u{f07b} src  \u{e725} main  stalls >= 40 && tide != 0.0 => open",
                "size-sample",
                Px::Fixed(px),
            );
            sample.set_ellipsize(pango::EllipsizeMode::End);
            row.append(&sample);
            sizes.append(&row);
        }
        term_page.append(&sizes);
        pages.add_named(&desk(&term_page, 700), Some("terminal"));
        pages.set_visible_child_name(if PAGES.iter().any(|(id, _)| *id == current) { &current } else { "article" });
        root.append(&pages);

        let inner = Rc::new(Inner {
            set_in,
            zones,
            cards,
            bar,
            spacer,
            bar_end,
            second_row,
            pages,
            page_buttons,
            install,
            install_label,
            headlines,
            set_line,
            columns,
            code_line,
            variants,
            variants_title,
            variant_checks,
            syncing: Cell::new(false),
            fonts: RefCell::default(),
            wanted: RefCell::default(),
            generation: Default::default(),
            narrow: Cell::new(false),
        });
        let sheet = Sheet { root, inner };
        sheet.connect();
        sheet.refresh_texts();
        sheet.refresh_variants();
        sheet.refresh_slots();
        sheet.refresh_install();
        sheet.paint_code();
        sheet
    }

    fn connect(&self) {
        let i = &self.inner;
        for (id, b) in &i.page_buttons {
            let (pages, id) = (i.pages.clone(), *id);
            b.connect_toggled(move |b| {
                if b.is_active() {
                    pages.set_visible_child_name(id);
                    prefs::update(|p| p.page = id.to_string());
                }
            });
        }
        for (id, check) in &i.variant_checks {
            check.set_active(catalog::nerd::variants(&prefs::get().nerd_variants).contains(id));
            let s = self.clone();
            check.connect_toggled(move |c| {
                if s.inner.syncing.get() {
                    return;
                }
                let picked: Vec<&str> = s.inner.variant_checks.iter().filter(|(_, c)| c.is_active()).map(|(id, _)| *id).collect();
                // Keeping nothing would install nothing.
                if picked.is_empty() {
                    c.set_active(true);
                    return;
                }
                let value = picked.join(",");
                if prefs::get().nerd_variants != value {
                    prefs::update(|p| p.nerd_variants = value);
                    events::emit(events::Change::Preferences);
                }
            });
        }
        let s = self.clone();
        events::subscribe(&self.root, move |c| match c {
            events::Change::Slots => s.refresh_slots(),
            events::Change::Catalog | events::Change::System => {
                slots::reresolve();
                s.refresh_slots();
                s.refresh_install();
            }
            events::Change::Installed => {
                slots::reresolve();
                s.refresh_slots();
                s.refresh_install();
            }
            events::Change::Jobs => s.refresh_install(),
            events::Change::Preferences => {
                s.refresh_texts();
                s.refresh_variants();
                s.apply_sizes();
            }
        });
        let s = self.clone();
        nyx_theme::subscribe(&self.root, move || s.paint_code());
    }

    pub fn set_narrow(&self, narrow: bool) {
        let i = &self.inner;
        if i.narrow.replace(narrow) == narrow {
            return;
        }
        if narrow {
            self.root.add_css_class("narrow");
            i.bar.remove(&i.bar_end);
            i.second_row.append(&i.bar_end);
        } else {
            self.root.remove_css_class("narrow");
            i.second_row.remove(&i.bar_end);
            i.bar.append(&i.bar_end);
        }
        i.second_row.set_visible(narrow);
        i.spacer.set_visible(!narrow);
        for c in &i.cards {
            c.button.set_hexpand(narrow);
            c.check.set_visible(!narrow && slots::chosen(c.slot).is_some_and(|e| slots::is_installed(&e)));
            c.chevron.set_visible(!narrow);
        }
        i.columns.set_orientation(if narrow { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal });
        i.columns.set_spacing(if narrow { 10 } else { 24 });
        self.apply_sizes();
    }

    fn refresh_texts(&self) {
        let headline = prefs::get().headline();
        for l in &self.inner.headlines {
            if l.has_css_class("headline") {
                l.set_text(&headline);
            }
        }
    }

    fn refresh_variants(&self) {
        let keep = catalog::nerd::variants(&prefs::get().nerd_variants);
        self.inner.syncing.set(true);
        for (id, c) in &self.inner.variant_checks {
            c.set_active(keep.contains(id));
        }
        self.inner.syncing.set(false);
    }

    /// The code line in the theme's colours.
    fn paint_code(&self) {
        self.inner.code_line.set_markup(&format!(
            "<span foreground=\"{}\">market</span>.open(<span foreground=\"{}\">\"08:00\"</span>) <span fgalpha=\"60%\">// 46 stalls, tide at 11:40</span>",
            swatch("blue"),
            swatch("green")
        ));
    }

    /// The slot cards, the outlined block, the set line and the fonts, after a slot
    /// or a font changed.
    fn refresh_slots(&self) {
        let i = &self.inner;
        let active = slots::active();
        for c in &i.cards {
            if c.slot == active {
                c.button.add_css_class("on");
            } else {
                c.button.remove_css_class("on");
            }
            let installed = slots::shown(c.slot).is_some_and(|e| slots::is_installed(&e));
            c.check.set_visible(installed && !i.narrow.get());
        }
        for (slot, l, _) in i.set_in.borrow().iter() {
            if l.has_css_class("slot-name") {
                let name = slots::name(*slot);
                l.set_text(name.strip_suffix(" Nerd Font").unwrap_or(&name));
                l.set_tooltip_text(Some(&name));
            }
        }
        for z in &i.zones {
            let on = z.slot == active;
            if on {
                z.frame.add_css_class("on");
            } else {
                z.frame.remove_css_class("on");
            }
            let shown = slots::shown(z.slot);
            let trying = shown.as_ref().is_some_and(|e| !slots::is_installed(e) || slots::trying().is_some());
            z.tag.set_text(&format!("{} · {}{}", z.slot.title(), if trying { "trying " } else { "" }, slots::name(z.slot)));
            z.tag.set_visible(on);
        }
        let [h, b, c] = Slot::ALL.map(slots::name);
        i.set_line.set_text(&format!("Set in {h}, {b} and {c}"));
        let code = slots::shown(Slot::Code);
        let nerd = code.as_ref().is_some_and(|e| e.source == Source::Nerd);
        i.variants.set_visible(nerd);
        i.variants_title.set_text(&format!("Install {} as", slots::name(Slot::Code)));
        for slot in Slot::ALL {
            self.load(slot);
        }
    }

    fn refresh_install(&self) {
        let i = &self.inner;
        let todo = slots::to_install();
        let busy = todo.iter().any(|e| install::busy(&e.key()).is_some());
        i.install.set_visible(!todo.is_empty());
        i.install.set_sensitive(!busy);
        i.install_label.set_text(&if busy { "Installing…".to_string() } else { format!("Install {}", todo.len()) });
        for c in &i.cards {
            let installed = slots::shown(c.slot).is_some_and(|e| slots::is_installed(&e));
            c.check.set_visible(installed && !i.narrow.get());
        }
    }

    /// Fetch the font a slot shows and set the slot's text in it. Fonts pointed at
    /// in passing wait a moment, so sweeping down the list doesn't fetch each one.
    fn load(&self, slot: Slot) {
        let i = &self.inner;
        let n = idx(slot);
        let entry = slots::shown(slot);
        let want = entry.as_ref().map(|e| format!("{:?}|{}|{}|{:?}", e.source, e.id, e.version, e.preview));
        if i.wanted.borrow()[n] == want {
            return;
        }
        i.wanted.borrow_mut()[n] = want;
        let generation = i.generation[n].get() + 1;
        i.generation[n].set(generation);
        let Some(entry) = entry else {
            i.fonts.borrow_mut()[n] = None;
            self.apply(slot);
            return;
        };
        let delay = if slots::trying().is_some() && slot == slots::active() { 140 } else { 0 };
        let s = self.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(delay), move || {
            if s.inner.generation[n].get() != generation {
                return;
            }
            let s2 = s.clone();
            cmd::background(
                move || preview::fetch(&entry).ok(),
                move |file| {
                    let i = &s2.inner;
                    if i.generation[n].get() != generation {
                        return;
                    }
                    i.fonts.borrow_mut()[n] = file.and_then(|f| {
                        preview::load(&f, if slot == Slot::Code { preview::Fallback::Icons } else { preview::Fallback::None })
                    });
                    s2.apply(slot);
                },
            );
        });
    }

    fn px(&self, px: Px) -> f64 {
        let narrow = self.inner.narrow.get();
        match px {
            Px::Fixed(v) => v,
            Px::Narrow(wide, small) => {
                if narrow {
                    small
                } else {
                    wide
                }
            }
            Px::Headline(f) => {
                let base = prefs::get().preview_size.clamp(12.0, 120.0) * 4.0 / 3.0;
                (if narrow { base * 0.6 } else { base }) * f
            }
        }
    }

    fn apply(&self, slot: Slot) {
        let font = self.inner.fonts.borrow()[idx(slot)].clone();
        for (s, l, px) in self.inner.set_in.borrow().iter() {
            if *s == slot {
                typeset::set(l, font.as_ref(), self.px(*px));
            }
        }
    }

    fn apply_sizes(&self) {
        for slot in Slot::ALL {
            self.apply(slot);
        }
    }
}

/// A page on the desk: centred, at most `max` wide, scrolling.
fn desk(child: &impl IsA<gtk::Widget>, max: i32) -> gtk::ScrolledWindow {
    let clamp = nyx_ui::clamp::Clamp::new(child, max);
    clamp.add_css_class("desk-page");
    clamp.set_valign(gtk::Align::Start);
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .css_classes(["desk"])
        .child(&clamp)
        .build()
}

const VARIANTS: &[(&str, &str, &str)] = &[
    ("mono", "Mono", "Icons one cell wide, for terminals"),
    ("regular", "Regular", "Larger icons, for editors and bars"),
    ("propo", "Propo", "Proportional, for interfaces"),
];

/// The terminal's lines: a prompt with Nerd Fonts icons, ls, and code with ligatures.
const TERMINAL: &[&str] = &[
    "<span background=\"#7aa2f7\" foreground=\"#14151b\"> \u{f015} ~/harbour </span><span foreground=\"#7aa2f7\" background=\"#9ece6a\">\u{e0b0}</span><span background=\"#9ece6a\" foreground=\"#14151b\"> \u{e725} main </span><span foreground=\"#9ece6a\">\u{e0b0}</span> ls",
    "<span foreground=\"#7aa2f7\">\u{f07b} src</span>  <span foreground=\"#7aa2f7\">\u{f07b} assets</span>  <span foreground=\"#ff9e64\">\u{e7a8} main.rs</span>  <span foreground=\"#9ece6a\">\u{f121} index.html</span>  \u{f013} config.toml",
    "<span background=\"#7aa2f7\" foreground=\"#14151b\"> \u{f015} ~/harbour </span><span foreground=\"#7aa2f7\" background=\"#9ece6a\">\u{e0b0}</span><span background=\"#9ece6a\" foreground=\"#14151b\"> \u{e725} main </span><span foreground=\"#9ece6a\">\u{e0b0}</span> cat src/tide.rs",
    "<span foreground=\"#bb9af7\">fn</span> <span foreground=\"#7aa2f7\">is_open</span>(stalls: <span foreground=\"#2ac3de\">u32</span>, tide: <span foreground=\"#2ac3de\">f32</span>) -&gt; <span foreground=\"#2ac3de\">bool</span> {",
    "    stalls &gt;= <span foreground=\"#ff9e64\">40</span> &amp;&amp; tide != <span foreground=\"#ff9e64\">0.0</span> <span foreground=\"#5c6080\">// =&gt; true on market days</span>",
    "}",
    "<span background=\"#7aa2f7\" foreground=\"#14151b\"> \u{f015} ~/harbour </span><span foreground=\"#7aa2f7\" background=\"#9ece6a\">\u{e0b0}</span><span background=\"#9ece6a\" foreground=\"#14151b\"> \u{e725} main </span><span foreground=\"#9ece6a\">\u{e0b0}</span> <span background=\"#c9cbd8\"> </span>",
];
