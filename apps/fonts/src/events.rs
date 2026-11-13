//! Change notifications from the catalogs, the installed list and the install
//! queue to whatever shows them. A subscriber lives as long as its widget.

use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The font lists were loaded, refreshed or started loading.
    Catalog,
    Preferences,
    /// A font was installed, updated or removed.
    Installed,
    /// The install queue moved on (progress, a job started or finished).
    Jobs,
    /// A slot's font, the slot being chosen, or the font being tried changed.
    Slots,
    /// The fonts on this computer were read again.
    System,
}

type Callback = Rc<dyn Fn(Change)>;

thread_local! {
    static SUBSCRIBERS: RefCell<Vec<(glib::WeakRef<gtk::Widget>, Callback)>> = const { RefCell::new(Vec::new()) };
}

pub fn subscribe(widget: &impl IsA<gtk::Widget>, f: impl Fn(Change) + 'static) {
    let weak = widget.upcast_ref::<gtk::Widget>().downgrade();
    SUBSCRIBERS.with(|s| s.borrow_mut().push((weak, Rc::new(f))));
}

pub fn emit(change: Change) {
    // Copy the list first: a callback may subscribe (a page being built).
    let live: Vec<Callback> = SUBSCRIBERS.with(|s| {
        let mut s = s.borrow_mut();
        s.retain(|(w, _)| w.upgrade().is_some());
        s.iter().map(|(_, f)| f.clone()).collect()
    });
    for f in live {
        f(change);
    }
}
