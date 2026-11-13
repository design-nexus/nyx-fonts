//! This app's own preferences (`~/.config/nyx-fonts/settings.toml`).

pub use nyx_theme::ThemeMode;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub mode: ThemeMode,
    pub theme: String,
    pub reduce_motion: bool,
    /// From the sidebar Fonts had before the pairing sheet; kept so old files still read.
    pub sidebar_collapsed: bool,
    pub glow: bool,
    /// The list's tab shown last: google, nerd or computer.
    pub last_section: String,
    /// Which Google Fonts files to install: static, variable or both.
    pub google_files: String,
    /// Which Nerd Font variants to keep: a comma list of mono, regular and propo
    /// (older files say all, mono, or regular for regular and Propo).
    pub nerd_variants: String,
    /// Run fc-cache after installing or removing.
    pub refresh_cache: bool,
    /// The page's heading.
    pub sample: String,
    /// The heading's size in points.
    pub preview_size: f64,
    /// How the Google Fonts list is sorted: popular, trending, name or newest.
    pub google_sort: String,
    /// Unix time the catalogs were last fetched.
    pub catalog_fetched: i64,
    /// The fonts set on the page, as `source:id` (`google:Playfair Display`).
    pub slot_heading: String,
    pub slot_body: String,
    pub slot_code: String,
    /// The slot the list fills: heading, body or code.
    pub active_slot: String,
    /// The page the fonts are set on: article, interface or terminal.
    pub page: String,
}

pub const DEFAULT_SAMPLE: &str = "The weekend market returns to the harbour";
/// The sample before the pairing sheet, read as "not changed".
const OLD_SAMPLE: &str = "The quick brown fox jumps over the lazy dog";

impl Default for Prefs {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Omarchy,
            theme: "tokyo-night".into(),
            reduce_motion: false,
            sidebar_collapsed: false,
            glow: true,
            last_section: "google".into(),
            google_files: "static".into(),
            nerd_variants: "mono,regular,propo".into(),
            refresh_cache: true,
            sample: DEFAULT_SAMPLE.into(),
            preview_size: 38.0,
            google_sort: "popular".into(),
            catalog_fetched: 0,
            slot_heading: "google:Playfair Display".into(),
            slot_body: "google:Source Serif 4".into(),
            slot_code: "nerd:JetBrainsMono".into(),
            active_slot: "heading".into(),
            page: "article".into(),
        }
    }
}

impl Prefs {
    /// The heading the page shows.
    pub fn headline(&self) -> String {
        let s = self.sample.trim();
        if s.is_empty() || s == OLD_SAMPLE { DEFAULT_SAMPLE.to_string() } else { s.to_string() }
    }
}

nyx_core::prefs_store!(Prefs);
