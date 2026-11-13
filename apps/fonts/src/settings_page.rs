//! The settings dialog's options: installing, the page, and the catalogs.

use crate::catalog::nerd;
use crate::widgets::{self, Page};
use crate::{catalog, events, fmt, paths, prefs, window};
use gtk::glib;
use gtk::prelude::*;

pub fn build(page: &Page) {
    let p = prefs::get();

    // ----- Installing -----
    let g = page.group("Installing");
    g.note(&format!(
        "Fonts install for your user only, in <tt>{}</tt>.",
        glib::markup_escape_text(&paths::pretty(&paths::fonts_dir()))
    ));
    g.add(&widgets::segmented_row(
        "Google Fonts files",
        "Static files have one file per style and work everywhere. Variable files hold every weight in one.",
        widgets::opts(&[("static", "Static"), ("variable", "Variable"), ("both", "Both")]),
        &p.google_files,
        |v| prefs::update(|p| p.google_files = v),
    ));
    g.add(&nerd_variants_row(&p.nerd_variants));
    let (r, _) = widgets::switch_row(
        "Refresh the font cache",
        "Run fc-cache after installing or removing, so other apps see the change straight away.",
        p.refresh_cache,
        |on| prefs::update(|p| p.refresh_cache = on),
    );
    g.add(&r);

    // ----- The page -----
    let g = page.group("Preview");
    let sample = gtk::Entry::new();
    sample.set_text(&p.headline());
    sample.set_placeholder_text(Some(prefs::DEFAULT_SAMPLE));
    sample.set_width_chars(22);
    sample.connect_changed(|e| {
        prefs::update(|p| p.sample = e.text().to_string());
        events::emit(events::Change::Preferences);
    });
    g.add(&widgets::row("Heading", "The words the page's heading is set in.", Some(sample.upcast_ref())));
    let size = gtk::SpinButton::with_range(16.0, 90.0, 1.0);
    size.set_value(p.preview_size);
    size.connect_value_changed(|s| {
        prefs::update(|p| p.preview_size = s.value());
        events::emit(events::Change::Preferences);
    });
    g.add(&widgets::row(
        "Heading size",
        "In points, for the article; the interface's heading is smaller.",
        Some(size.upcast_ref()),
    ));
    let (r, _) = widgets::button_row("Clear the preview cache", "Delete downloaded preview files.", "Clear", |_| {
        let dir = paths::preview_dir();
        let size: u64 =
            std::fs::read_dir(&dir).map(|d| d.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0);
        let _ = std::fs::remove_dir_all(&dir);
        window::toast(&format!("Cleared {} of previews.", fmt::bytes(size)));
    });
    g.add(&r);

    // ----- Catalog -----
    let g = page.group("Catalog");
    let (row, _) = widgets::choice_row(
        "Sort Google Fonts by",
        "Default order for the font catalog.",
        widgets::opts(&[("popular", "Popular"), ("trending", "Trending"), ("name", "Name"), ("newest", "Newest")]),
        &p.google_sort,
        |id| {
            prefs::update(|p| p.google_sort = id);
            events::emit(events::Change::Preferences);
        },
    );
    g.add(&row);
    let (row, button) = widgets::button_row("Refresh the font lists", "", "Refresh now", |_| catalog::refresh());
    g.add(&row);
    let desc = row.first_child().and_then(|t| t.first_child()).and_then(|t| t.next_sibling()).and_downcast::<gtk::Label>();
    let label = widgets::label("", "settings-option-description");
    label.set_wrap(true);
    if let Some(text) = row.first_child().and_downcast::<gtk::Box>()
        && desc.is_none()
    {
        text.append(&label);
    }
    let refresh = {
        let (label, button) = (label.clone(), button.clone());
        move || {
            button.set_sensitive(!catalog::loading());
            let fetched = prefs::get().catalog_fetched;
            let when = if catalog::loading() {
                "Updating now…".to_string()
            } else if fetched == 0 {
                "Not fetched yet.".to_string()
            } else {
                format!("Last updated {}. They update once a day.", fmt::date(fetched))
            };
            label.set_text(&format!(
                "{} Google Fonts families and {} Nerd Fonts ({}). {when}",
                fmt::thousands(catalog::google().len()),
                catalog::nerd().len(),
                if catalog::nerd_release().is_empty() { "release unknown".into() } else { catalog::nerd_release() },
            ));
        }
    };
    refresh();
    events::subscribe(&label, move |c| {
        if c == events::Change::Catalog {
            refresh();
        }
    });
}

/// Mono, Regular and Propo, each kept or not; the Terminal page sets the same.
fn nerd_variants_row(current: &str) -> gtk::Box {
    let checks = widgets::hbox(12);
    let keep = nerd::variants(current);
    let mut all: Vec<(&'static str, gtk::CheckButton)> = Vec::new();
    for (id, title) in [("mono", "Mono"), ("regular", "Regular"), ("propo", "Propo")] {
        let c = gtk::CheckButton::with_label(title);
        c.set_active(keep.contains(&id));
        checks.append(&c);
        all.push((id, c));
    }
    let all = std::rc::Rc::new(all);
    // Set while the checks follow the settings, which isn't a choice.
    let syncing = std::rc::Rc::new(std::cell::Cell::new(false));
    for (_, c) in all.iter() {
        let (all, syncing) = (all.clone(), syncing.clone());
        c.connect_toggled(move |c| {
            if syncing.get() {
                return;
            }
            let picked: Vec<&str> = all.iter().filter(|(_, c)| c.is_active()).map(|(id, _)| *id).collect();
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
    let row = widgets::row(
        "Nerd Fonts variants",
        "Mono has single-width icons, for terminals. Regular suits editors and bars, Propo interfaces and documents.",
        Some(checks.upcast_ref()),
    );
    let all2 = all.clone();
    events::subscribe(&row, move |c| {
        if c == events::Change::Preferences {
            let keep = nerd::variants(&prefs::get().nerd_variants);
            syncing.set(true);
            for (id, c) in all2.iter() {
                c.set_active(keep.contains(id));
            }
            syncing.set(false);
        }
    });
    row
}
