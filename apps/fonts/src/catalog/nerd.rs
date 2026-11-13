//! Nerd Fonts: the font list the project keeps in its repository, and the
//! archives from its latest release.

use super::{FontEntry, PreviewFile, Source};
use anyhow::{Context, Result};
use serde::Deserialize;

pub const FONTS_URL: &str = "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/master/bin/scripts/lib/fonts.json";
pub const RELEASE_URL: &str = "https://api.github.com/repos/ryanoasis/nerd-fonts/releases/latest";
const UNPATCHED: &str = "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/master/src/unpatched-fonts/";

#[derive(Deserialize)]
struct List {
    fonts: Vec<Font>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Font {
    unpatched_name: String,
    patched_name: String,
    folder_name: String,
    #[serde(default)]
    license_id: String,
    #[serde(default)]
    is_monospaced: bool,
    #[serde(default)]
    description: String,
    #[serde(default)]
    image_preview_font_source: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

pub fn parse_release(text: &str) -> Result<String> {
    let r: Release = serde_json::from_str(text).context("unexpected Nerd Fonts release")?;
    Ok(r.tag_name)
}

pub fn parse(text: &str, release: &str) -> Result<Vec<FontEntry>> {
    let list: List = serde_json::from_str(text).context("unexpected Nerd Fonts list")?;
    let mut out: Vec<FontEntry> = list
        .fonts
        .into_iter()
        .map(|f| {
            let family = format!("{} Nerd Font", f.patched_name);
            let category = if f.is_monospaced { "Monospace" } else { "Proportional" }.to_string();
            let src = &f.image_preview_font_source;
            let preview = if src.ends_with(".ttf") || src.ends_with(".otf") {
                PreviewFile::Url(format!("{UNPATCHED}{src}"))
            } else {
                PreviewFile::Url(String::new())
            };
            let haystack =
                format!("{} {} {} {} {}", family, f.unpatched_name, f.folder_name, f.description, category).to_lowercase();
            FontEntry {
                source: Source::Nerd,
                id: f.folder_name,
                family,
                category,
                // Regular, Mono and Propo, each with their styles; known after install.
                styles: 0,
                detail: format!("{}. Based on {}.", f.description.trim_end_matches('.'), f.unpatched_name),
                license: f.license_id,
                version: release.to_string(),
                popularity: 0,
                trending: 0,
                added: String::new(),
                variable: false,
                preview,
                haystack,
            }
        })
        .collect();
    out.sort_by_key(|e| e.family.to_lowercase());
    Ok(out)
}

pub fn archive_url(release: &str, folder: &str) -> String {
    format!("https://github.com/ryanoasis/nerd-fonts/releases/download/{release}/{folder}.tar.xz")
}

/// Which variant a patched file is: `regular`, `mono` or `propo`.
pub fn variant(file: &str) -> &'static str {
    if file.contains("NerdFontMono") || file.contains(" NFM") {
        "mono"
    } else if file.contains("NerdFontPropo") || file.contains(" NFP") {
        "propo"
    } else {
        "regular"
    }
}

/// Every variant, in the order they're offered.
pub const VARIANTS: [&str; 3] = ["mono", "regular", "propo"];

/// The variants a preference keeps: a comma list, or an older `all`, `mono` or
/// `regular` (regular and Propo). Never none.
pub fn variants(pref: &str) -> Vec<&'static str> {
    let picked: Vec<&'static str> = match pref {
        "all" | "" => VARIANTS.to_vec(),
        "regular" => vec!["regular", "propo"],
        _ => VARIANTS.iter().copied().filter(|v| pref.split(',').any(|p| p.trim() == *v)).collect(),
    };
    if picked.is_empty() { VARIANTS.to_vec() } else { picked }
}

/// Whether to keep a file for the chosen variants.
pub fn keep(file: &str, which: &str) -> bool {
    variants(which).contains(&variant(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list() {
        let text = r#"{"fonts":[
            {"unpatchedName":"Hack","patchedName":"Hack","folderName":"Hack","licenseId":"MIT","isMonospaced":true,
             "description":"Dotted zero, short descenders","imagePreviewFontSource":"Hack/Regular/Hack-Regular.ttf"},
            {"unpatchedName":"Symbols Only","patchedName":"Symbols","folderName":"NerdFontsSymbolsOnly","licenseId":"MIT",
             "isMonospaced":true,"description":"Just the icons","imagePreviewFontSource":"NerdFontsSymbolsOnly/Blank.sfd"}
        ]}"#;
        let list = parse(text, "v3.5.1").unwrap();
        assert_eq!(list[0].family, "Hack Nerd Font");
        assert_eq!(list[0].version, "v3.5.1");
        assert!(matches!(&list[0].preview, PreviewFile::Url(u) if u.ends_with("Hack/Regular/Hack-Regular.ttf")));
        assert!(matches!(&list[1].preview, PreviewFile::Url(u) if u.is_empty()));
        assert_eq!(parse_release(r#"{"tag_name":"v3.5.1","assets":[]}"#).unwrap(), "v3.5.1");
    }

    #[test]
    fn sorts_variants() {
        assert_eq!(variant("HackNerdFont-Regular.ttf"), "regular");
        assert_eq!(variant("HackNerdFontMono-Bold.ttf"), "mono");
        assert_eq!(variant("HackNerdFontPropo-Italic.ttf"), "propo");
        assert!(keep("HackNerdFontPropo-Italic.ttf", "regular"));
        assert!(!keep("HackNerdFontMono-Bold.ttf", "regular"));
        assert!(!keep("HackNerdFont-Regular.ttf", "mono"));
        assert!(keep("HackNerdFont-Regular.ttf", "all"));
        assert!(keep("HackNerdFontMono-Bold.ttf", "mono,regular"));
        assert!(!keep("HackNerdFontPropo-Bold.ttf", "mono,regular"));
        assert_eq!(variants("propo"), vec!["propo"]);
        assert_eq!(variants("nonsense"), VARIANTS.to_vec());
    }
}
