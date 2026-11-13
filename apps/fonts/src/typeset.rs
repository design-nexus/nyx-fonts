//! Setting a label in a previewed font, at a size in pixels.

use crate::preview::Loaded;
use gtk::pango;
use gtk::prelude::*;

/// Draw `label` in `font` (or the window's font while it's loading) at `px`.
pub fn set(label: &gtk::Label, font: Option<&Loaded>, px: f64) {
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrSize::new_size_absolute((px * pango::SCALE as f64) as i32));
    match font {
        Some((map, family)) => {
            label.set_font_map(Some(map));
            if !family.is_empty() {
                attrs.insert(pango::AttrString::new_family(family));
            }
        }
        None => label.set_font_map(None::<&pango::FontMap>),
    }
    label.set_attributes(Some(&attrs));
}

/// A label for text drawn in a previewed font.
pub fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("in-font");
    if !class.is_empty() {
        l.add_css_class(class);
    }
    l.set_xalign(0.0);
    l
}
