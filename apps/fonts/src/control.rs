//! Fonts' tools on the control interface (`nyx_core::control`), for Nyx Assistant: what's
//! installed, search Google Fonts and Nerd Fonts, and install one. Nothing is removed.

use crate::catalog::{self, FontEntry, Source};
use crate::{install, installed, system};
use nyx_core::control::text;
use serde_json::{Value, json};

/// Fonts' description for the control interface (API 2): its actions, the `nyx`
/// command's `fonts` namespace, and D-Bus activation so a call starts it (with its window,
/// which holds what the actions work on).
pub fn description(app_id: &str) -> Value {
    let mut tools = tools();
    for t in tools.as_array_mut().into_iter().flatten() {
        let risk = match t["name"].as_str().unwrap_or_default() {
            "fonts.installed" => "read_only",
            "fonts.search" => "read_only",
            "fonts.install" => "external",
            _ => "writes",
        };
        t["x-nyx"] = json!({"risk": risk});
        // A single required argument can be given bare: `nyx APP VERB WORDS`.
        if let Some([one]) = t["inputSchema"]["required"].as_array().map(Vec::as_slice) {
            t["x-nyx"]["positional"] = json!([one]);
        }
    }
    json!({
        "tools": tools,
        "nyx": {"api": 2, "app": app_id, "name": "Fonts", "icon": "io.github.design_nexus.nyx.Fonts", "namespaces": ["fonts"],
                "aliases": [], "bus_name": app_id, "activation": "dbus", "default": "fonts.search"}
    })
}

pub fn tools() -> Value {
    json!([
        {"name": "fonts.installed", "description": "The font families on this computer, and the ones Fonts installed.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}}}},
        {"name": "fonts.search", "description": "Search Google Fonts and Nerd Fonts by name or kind (serif, monospace…).", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}, "source": {"enum": ["google", "nerd"]}}, "required": ["query"]}},
        {"name": "fonts.install", "description": "Download and install a Google Font or Nerd Font family (by name) for this user.", "inputSchema": {"type": "object", "properties": {"family": {"type": "string"}, "source": {"enum": ["google", "nerd"]}}, "required": ["family"]}},
    ])
}

fn source(args: &Value) -> Option<Source> {
    match args["source"].as_str() {
        Some("google") => Some(Source::Google),
        Some("nerd") => Some(Source::Nerd),
        _ => None,
    }
}

/// The catalogs, once loaded (loading them starts on the first call).
fn catalogs(only: Option<Source>) -> Result<Vec<FontEntry>, String> {
    if !catalog::loaded() {
        if !catalog::loading() {
            catalog::start();
        }
        return Err(catalog::error().unwrap_or_else(|| "the font catalogs are still loading; try again in a moment".into()));
    }
    let mut all: Vec<FontEntry> = Vec::new();
    if only != Some(Source::Nerd) {
        all.extend(catalog::google().iter().cloned());
    }
    if only != Some(Source::Google) {
        all.extend(catalog::nerd().iter().cloned());
    }
    Ok(all)
}

fn entry(e: &FontEntry) -> Value {
    json!({
        "family": e.family, "source": e.source.label(), "category": e.category, "styles": e.styles,
        "installed": installed::get(e.source, &e.id).is_some(),
    })
}

pub fn handle(name: &str, args: Value) -> Result<Value, String> {
    match name {
        "fonts.installed" => {
            let terms = catalog::terms(args["query"].as_str().unwrap_or(""));
            let mut families: Vec<String> = system::list().into_iter().filter(|e| e.matches(&terms)).map(|e| e.family).collect();
            families.sort_unstable_by_key(|f| f.to_lowercase());
            families.dedup();
            let ours: Vec<String> = installed::all().into_iter().map(|r| r.family).collect();
            Ok(
                json!({"families": families.len(), "names": families.iter().take(200).collect::<Vec<_>>(), "installed_by_fonts": ours}),
            )
        }
        "fonts.search" => {
            let terms = catalog::terms(&text(&args, "query")?);
            let mut found: Vec<FontEntry> = catalogs(source(&args))?.into_iter().filter(|e| e.matches(&terms)).collect();
            // Google's most popular first (lower rank); Nerd Fonts after.
            found.sort_by_key(|e| if e.popularity == 0 { u32::MAX } else { e.popularity });
            Ok(json!({"fonts": found.iter().take(15).map(entry).collect::<Vec<_>>()}))
        }
        "fonts.install" => {
            let family = text(&args, "family")?;
            let want = family.trim().to_lowercase();
            let all = catalogs(source(&args))?;
            let e = all
                .iter()
                .find(|e| e.family.to_lowercase() == want || e.id.to_lowercase() == want)
                .ok_or_else(|| format!("no font called {family} in Google Fonts or Nerd Fonts"))?;
            if installed::get(e.source, &e.id).is_some() {
                return Ok(json!({"already_installed": entry(e)}));
            }
            install::install(e);
            Ok(json!({"installing": entry(e), "note": "it downloads in the background; fonts.installed shows it once done"}))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}
