//! Every font family on the system, from `fc-list`.

use crate::catalog::{FontEntry, PreviewFile, Source};
use crate::{cmd, events, paths};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

thread_local! {
    static CACHED: RefCell<Rc<Vec<FontEntry>>> = RefCell::new(Rc::default());
    static READING: Cell<bool> = const { Cell::new(false) };
}

/// The families as last read (empty until the first read finishes).
pub fn cached() -> Rc<Vec<FontEntry>> {
    CACHED.with(|c| c.borrow().clone())
}

/// Read the families again in the background, then tell everyone.
pub fn refresh() {
    if READING.with(|r| r.replace(true)) {
        return;
    }
    cmd::background(list, |list| {
        READING.with(|r| r.set(false));
        CACHED.with(|c| *c.borrow_mut() = Rc::new(list));
        events::emit(events::Change::System);
    });
}

/// Read the families (blocking; run it off the UI thread).
pub fn list() -> Vec<FontEntry> {
    let out = Command::new("fc-list").arg("--format").arg("%{family[0]}\t%{style[0]}\t%{file}\n").output();
    match out {
        Ok(o) if o.status.success() => parse(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

pub fn parse(text: &str) -> Vec<FontEntry> {
    let ours = paths::fonts_dir();
    let user = [paths::user_fonts_dir(), paths::home().join(".fonts")];
    // family → (styles, files)
    let mut families: BTreeMap<String, (Vec<String>, Vec<PathBuf>)> = BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(family), Some(style), Some(file)) = (parts.next(), parts.next(), parts.next()) else { continue };
        if family.is_empty() || file.is_empty() {
            continue;
        }
        let e = families.entry(family.to_string()).or_default();
        if !e.0.iter().any(|s| s == style) {
            e.0.push(style.to_string());
        }
        e.1.push(PathBuf::from(file));
    }
    let mut out: Vec<FontEntry> = families
        .into_iter()
        .map(|(family, (styles, mut files))| {
            files.sort();
            // Preview the regular style when there is one.
            let regular = files
                .iter()
                .find(|f| f.file_stem().is_some_and(|s| s.to_string_lossy().ends_with("-Regular")))
                .or_else(|| files.first())
                .cloned()
                .unwrap_or_default();
            let category = if regular.starts_with(&ours) {
                "Installed here"
            } else if user.iter().any(|u| regular.starts_with(u)) {
                "User"
            } else {
                "System"
            };
            let folder = regular.parent().map(paths::pretty).unwrap_or_default();
            let haystack = format!("{family} {category} {folder} {}", styles.join(" ")).to_lowercase();
            FontEntry {
                source: Source::System,
                id: family.clone(),
                family,
                category: category.to_string(),
                styles: styles.len(),
                detail: folder,
                license: String::new(),
                version: String::new(),
                popularity: 0,
                trending: 0,
                added: String::new(),
                variable: false,
                preview: PreviewFile::Local(regular),
                haystack,
            }
        })
        .collect();
    out.sort_by_key(|e| e.family.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_by_family() {
        let text = "Hack\tBold\t/usr/share/fonts/Hack-Bold.ttf\n\
                    Hack\tRegular\t/usr/share/fonts/Hack-Regular.ttf\n\
                    Inter\tRegular\t/usr/share/fonts/Inter.ttf\n\
                    \tRegular\t/x.ttf\n";
        let list = parse(text);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].family, "Hack");
        assert_eq!(list[0].styles, 2);
        assert_eq!(list[0].category, "System");
        assert_eq!(list[0].preview, PreviewFile::Local("/usr/share/fonts/Hack-Regular.ttf".into()));
    }
}
