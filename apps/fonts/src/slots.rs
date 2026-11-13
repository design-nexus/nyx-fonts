//! The page's three slots, Heading, Body and Code: the font set in each, the slot
//! the list is choosing for, and the font being tried there (pointed at in the list).

use crate::catalog::{self, FontEntry, Source};
use crate::{events, install, installed, prefs, system};
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Heading,
    Body,
    Code,
}

impl Slot {
    pub const ALL: [Slot; 3] = [Slot::Heading, Slot::Body, Slot::Code];

    pub fn id(self) -> &'static str {
        match self {
            Slot::Heading => "heading",
            Slot::Body => "body",
            Slot::Code => "code",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Slot::Heading => "Heading",
            Slot::Body => "Body",
            Slot::Code => "Code",
        }
    }

    pub fn from_id(id: &str) -> Slot {
        match id {
            "body" => Slot::Body,
            "code" => Slot::Code,
            _ => Slot::Heading,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Default)]
struct State {
    /// The font in each slot, once it's been found in a catalog or on the system.
    chosen: [Option<FontEntry>; 3],
    trying: Option<FontEntry>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn source_id(s: Source) -> &'static str {
    match s {
        Source::Google => "google",
        Source::Nerd => "nerd",
        Source::System => "system",
    }
}

fn stored(slot: Slot) -> String {
    let p = prefs::get();
    match slot {
        Slot::Heading => p.slot_heading,
        Slot::Body => p.slot_body,
        Slot::Code => p.slot_code,
    }
}

/// The slot's font as `(source, id)`, from the settings.
pub fn key(slot: Slot) -> (Source, String) {
    let s = stored(slot);
    let (src, id) = s.split_once(':').unwrap_or(("google", &s));
    let source = match src {
        "nerd" => Source::Nerd,
        "system" => Source::System,
        _ => Source::Google,
    };
    (source, id.to_string())
}

fn resolve(source: Source, id: &str) -> Option<FontEntry> {
    match source {
        Source::System => system::cached().iter().find(|e| e.id == id).cloned(),
        _ => catalog::find(source, id).or_else(|| installed::get(source, id).map(|r| install::stub(&r))),
    }
}

pub fn active() -> Slot {
    Slot::from_id(&prefs::get().active_slot)
}

pub fn set_active(slot: Slot) {
    if active() == slot {
        return;
    }
    prefs::update(|p| p.active_slot = slot.id().to_string());
    STATE.with(|s| s.borrow_mut().trying = None);
    events::emit(events::Change::Slots);
}

/// The font set in a slot, when it's known yet.
pub fn chosen(slot: Slot) -> Option<FontEntry> {
    if let Some(e) = STATE.with(|s| s.borrow().chosen[slot.index()].clone()) {
        return Some(e);
    }
    let (source, id) = key(slot);
    let e = resolve(source, &id)?;
    STATE.with(|s| s.borrow_mut().chosen[slot.index()] = Some(e.clone()));
    Some(e)
}

/// What the page shows in a slot: the font being tried there, or the one set.
pub fn shown(slot: Slot) -> Option<FontEntry> {
    if slot == active()
        && let Some(e) = STATE.with(|s| s.borrow().trying.clone())
    {
        return Some(e);
    }
    chosen(slot)
}

pub fn trying() -> Option<FontEntry> {
    STATE.with(|s| s.borrow().trying.clone())
}

/// The family to name for a slot, even before the catalogs are read.
pub fn name(slot: Slot) -> String {
    match shown(slot) {
        Some(e) => e.family,
        None => {
            let (source, id) = key(slot);
            if source == Source::Nerd { format!("{id} Nerd Font") } else { id }
        }
    }
}

/// Set the font for the slot being chosen.
pub fn choose(entry: &FontEntry) {
    let slot = active();
    let value = format!("{}:{}", source_id(entry.source), entry.id);
    let same = stored(slot) == value;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.chosen[slot.index()] = Some(entry.clone());
        s.trying = None;
    });
    if !same {
        prefs::update(|p| match slot {
            Slot::Heading => p.slot_heading = value,
            Slot::Body => p.slot_body = value,
            Slot::Code => p.slot_code = value,
        });
    }
    events::emit(events::Change::Slots);
}

/// Try a font in the slot being chosen, or stop trying (`None`).
pub fn try_font(entry: Option<&FontEntry>) {
    let changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let same = match (&s.trying, entry) {
            (Some(a), Some(b)) => a.key() == b.key(),
            (None, None) => true,
            _ => false,
        };
        if !same {
            s.trying = entry.cloned();
        }
        !same
    });
    if changed {
        events::emit(events::Change::Slots);
    }
}

/// The slots a font is set in.
pub fn slots_of(key: &(Source, String)) -> Vec<Slot> {
    Slot::ALL.into_iter().filter(|s| self::key(*s) == *key).collect()
}

/// The fonts set on the page that can be installed and aren't yet.
pub fn to_install() -> Vec<FontEntry> {
    let mut out: Vec<FontEntry> = Vec::new();
    for slot in Slot::ALL {
        if let Some(e) = chosen(slot)
            && e.source != Source::System
            && installed::get(e.source, &e.id).is_none()
            && !out.iter().any(|o| o.key() == e.key())
        {
            out.push(e);
        }
    }
    out
}

/// Whether a font is on this computer.
pub fn is_installed(e: &FontEntry) -> bool {
    e.source == Source::System || installed::get(e.source, &e.id).is_some()
}

/// Look the slots' fonts up again, after the catalogs or the system list are read.
pub fn reresolve() {
    STATE.with(|s| s.borrow_mut().chosen = Default::default());
}
