//! Previews: one font file per family, downloaded to the cache and drawn
//! through a font map of its own, so nothing has to be installed to see it and
//! the window's own fonts are left alone.

use crate::catalog::{FontEntry, PreviewFile, Source, google};
use crate::{http, paths};
use anyhow::{Context, Result, bail};
use gtk::glib::translate::from_glib_full;
use gtk::pango;
use gtk::pango::prelude::*;
use std::ffi::{CString, c_void};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const MAX_FONT: u64 = 64 * 1024 * 1024;

fn cache_name(entry: &FontEntry, ext: &str) -> PathBuf {
    let safe: String = entry.id.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    let src = match entry.source {
        Source::Google => "google",
        Source::Nerd => "nerd",
        Source::System => "system",
    };
    paths::preview_dir().join(format!("{src}-{safe}-{}.{ext}", entry.version.replace(['/', ' '], "_")))
}

fn ext_of(url: &str) -> &str {
    if url.to_ascii_lowercase().ends_with(".otf") { "otf" } else { "ttf" }
}

fn save(url: &str, dest: &Path) -> Result<()> {
    let bytes = http::get_bytes(url, MAX_FONT)?;
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// The font file to preview, fetched if needed (blocking; run it off the UI thread).
pub fn fetch(entry: &FontEntry) -> Result<PathBuf> {
    // An installed font previews from its own files.
    match &entry.preview {
        PreviewFile::Local(p) => {
            if p.is_file() {
                return Ok(p.clone());
            }
            bail!("{} is missing", paths::pretty(p));
        }
        PreviewFile::Url(url) => {
            if url.is_empty() {
                bail!("there's no preview for this font");
            }
            let dest = cache_name(entry, ext_of(url));
            if !dest.is_file() {
                save(url, &dest)?;
            }
            Ok(dest)
        }
        PreviewFile::Google => {
            for ext in ["ttf", "otf"] {
                let dest = cache_name(entry, ext);
                if dest.is_file() {
                    return Ok(dest);
                }
            }
            let manifest = google::manifest(&entry.family)?;
            let file = google::preview_file(&manifest.file_refs).context("Google lists no font files for it")?;
            let dest = cache_name(entry, ext_of(&file.filename));
            save(&file.url, &dest)?;
            Ok(dest)
        }
    }
}

/// A file that draws only `text`, for showing a family's name in itself: Google cuts
/// one to the name's letters, a few kilobytes. Other fonts use their preview file.
pub fn fetch_name(entry: &FontEntry) -> Result<PathBuf> {
    if entry.source != Source::Google {
        return fetch(entry);
    }
    let dest = paths::preview_dir().join(format!(
        "name-{}-{}.ttf",
        entry.id.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect::<String>(),
        entry.version.replace(['/', ' '], "_")
    ));
    if dest.is_file() {
        return Ok(dest);
    }
    // The ellipsis and dots for names cut short.
    let text = format!("{}….", entry.family);
    let url = format!(
        "https://fonts.googleapis.com/css2?family={}&text={}",
        http::encode(&entry.family).replace("%20", "+"),
        http::encode(&text)
    );
    let subset = http::get_text(&url).ok().and_then(|css| {
        let start = css.find("url(")? + 4;
        let end = start + css[start..].find(')')?;
        Some(css[start..end].to_string())
    });
    match subset {
        Some(file) => {
            save(&file, &dest)?;
            Ok(dest)
        }
        // A family without a regular weight: its whole preview file.
        None => fetch(entry),
    }
}

/// What a font map falls back to for characters its font lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fallback {
    /// Nothing: text is all in the font, or missing.
    None,
    /// The Nerd Fonts icons (a catalog preview is the unpatched font).
    Icons,
    /// The system's sans serif, for a name the font can't draw (emoji, symbols, scripts).
    Text,
}

fn find_file(pattern: &str, accept: impl Fn(&str, &str) -> (bool, bool, bool)) -> Option<PathBuf> {
    let out = std::process::Command::new("fc-list")
        .args(["--format", "%{family[0]}\t%{style[0]}\t%{file}\n", pattern])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let fonts: Vec<(&str, &str, &str)> = text
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            Some((p.next()?, p.next()?, p.next()?))
        })
        .collect();
    fonts.iter().min_by_key(|(family, style, _)| accept(family, style)).map(|(_, _, file)| PathBuf::from(file))
}

/// The file a fallback draws from, found once (blocking the first time).
fn fallback_file(f: Fallback) -> Option<PathBuf> {
    static ICONS: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    static TEXT: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    match f {
        Fallback::None => None,
        Fallback::Icons => ICONS
            .get_or_init(|| {
                find_file(":charset=e0b0 f07b e725", |family, style| {
                    (!family.starts_with("Symbols Nerd Font"), !family.contains("Mono"), style != "Regular")
                })
            })
            .clone(),
        Fallback::Text => TEXT
            .get_or_init(|| {
                let out =
                    std::process::Command::new("fc-match").args(["-f", "%{file}", "sans-serif:style=Regular"]).output().ok()?;
                let file = String::from_utf8_lossy(&out.stdout).trim().to_string();
                (!file.is_empty()).then(|| PathBuf::from(file))
            })
            .clone(),
    }
}

/// Find the fallback fonts now, off the UI thread, so the first preview doesn't wait.
pub fn warm_fallbacks() {
    std::thread::spawn(|| {
        fallback_file(Fallback::Icons);
        fallback_file(Fallback::Text);
    });
}

/// A font map that draws one file, and the family to ask it for.
pub type Loaded = (pango::FontMap, String);

thread_local! {
    static MAPS: std::cell::RefCell<std::collections::HashMap<(PathBuf, Fallback), Loaded>> = Default::default();
}

/// The font map for `file`, made once, with characters it lacks from `fallback`. Ask
/// for the returned family by name, or fontconfig may pick the fallback for everything.
pub fn load(file: &Path, fallback: Fallback) -> Option<Loaded> {
    let key = (file.to_path_buf(), fallback);
    if let Some(l) = MAPS.with(|m| m.borrow().get(&key).cloned()) {
        return Some(l);
    }
    let single = font_map(&[file])?;
    let family = single.list_families().first().map(|f| f.name().to_string()).unwrap_or_default();
    let map = match fallback_file(fallback).filter(|f| f != file) {
        Some(extra) => font_map(&[file, &extra]).unwrap_or(single),
        None => single,
    };
    let loaded = (map, family);
    MAPS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 400 {
            m.clear();
        }
        m.insert(key, loaded.clone());
    });
    Some(loaded)
}

/// Drop cached previews older than a month.
pub fn prune() {
    let Ok(entries) = std::fs::read_dir(paths::preview_dir()) else { return };
    let month = std::time::Duration::from_secs(30 * 24 * 3600);
    for e in entries.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > month);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcConfigCreate() -> *mut c_void;
    fn FcConfigAppFontAddFile(config: *mut c_void, file: *const u8) -> i32;
    fn FcConfigDestroy(config: *mut c_void);
}

#[link(name = "pangoft2-1.0")]
unsafe extern "C" {
    fn pango_fc_font_map_set_config(fontmap: *mut c_void, config: *mut c_void);
}

#[link(name = "pangocairo-1.0")]
unsafe extern "C" {
    fn pango_cairo_font_map_new_for_font_type(font_type: i32) -> *mut pango::ffi::PangoFontMap;
}

const CAIRO_FONT_TYPE_FT: i32 = 1;

/// A font map that holds just these files. With one, every family asked for falls
/// back to it, so text drawn with it is always in the previewed font.
fn font_map(files: &[&Path]) -> Option<pango::FontMap> {
    let paths: Vec<CString> = files.iter().filter_map(|f| CString::new(f.as_os_str().as_bytes()).ok()).collect();
    if paths.len() != files.len() {
        return None;
    }
    // SAFETY: plain fontconfig and pango calls. The font map takes its own
    // reference to the config, so ours is released straight after.
    unsafe {
        let config = FcConfigCreate();
        if config.is_null() {
            return None;
        }
        for path in &paths {
            if FcConfigAppFontAddFile(config, path.as_ptr().cast()) == 0 {
                FcConfigDestroy(config);
                return None;
            }
        }
        let map = pango_cairo_font_map_new_for_font_type(CAIRO_FONT_TYPE_FT);
        if map.is_null() {
            FcConfigDestroy(config);
            return None;
        }
        pango_fc_font_map_set_config(map.cast(), config);
        FcConfigDestroy(config);
        Some(from_glib_full(map))
    }
}
