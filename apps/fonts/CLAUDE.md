# Fonts — notes for working on this repo

- GTK4 (gtk4-rs 0.11) + Rust. No libadwaita. Nyx Design 2.0 (`use_design_2`, see
  `STYLE.md`) on `nyx_ui::frame`, with no sidebar: the header shows the app, the search
  across both catalogs and this computer (Ctrl+K) in the middle, and updates, the
  install readout and Settings at the end. Every colour is a `@theme_*` token; the
  slots' colours are the theme's swatches as `@sw_violet`, `@sw_teal`, `@sw_amber`.
- The page is the pairing sheet: `picker.rs` (the list: Google, Nerd Fonts, This
  computer, or search results, in a `ListView` of section headings and font rows) beside
  `sheet.rs` (the Heading, Body and Code slots and the Article, Interface and Terminal
  pages). `slots.rs` holds each slot's font (`prefs.slot_*`, `source:id`), the slot being
  chosen, and the font pointed at in the list ("trying"). Picking a row sets it in the
  slot; the list follows the slot's font with its selection.
- Names in the list are drawn in themselves (`names.rs`): Google cuts a file to the
  name's letters (`fonts.googleapis.com/css2?family=…&text=…`), other fonts use their
  preview file; four workers, first come first served, a row takes its request back
  when it shows another font. `typeset::set` puts a label in a `preview::Loaded` font.
- Catalogs (`catalog/`): Google `fonts.google.com/metadata/fonts` (popularity/trending
  are ranks, lower = more popular) and the download manifest
  `fonts.google.com/download/list?family=` (strip `)]}'`). Nerd Fonts: `fonts.json` in the
  repo + the latest release tag; archives are `<folderName>.tar.xz`. Raw JSON is cached
  in `~/.cache/nyx-fonts/` and refetched after 24 h (`prefs.catalog_fetched`).
- `events.rs` is the change bus (Catalog, Installed, Jobs); subscribers live as long as
  their widget. The status column updates bound labels in place (no row rebuilds).
- `install.rs`: one worker thread, jobs in order, results over async-channel. Installs
  stage in a hidden `.Family.part-PID` folder beside the target and swap it in.
  `installed.rs` records files in `~/.local/state/nyx-fonts/installed.toml`; only
  paths under `~/.local/share/fonts/nyx-fonts` are ever deleted.
- Previews (`preview.rs`): one file per family in `~/.cache/nyx-fonts/preview/`, drawn
  through its own `FcConfig` + `PangoCairoFcFontMap` set with `widget.set_font_map`,
  plus a fallback file: the Nerd icons for the code slot (a catalog preview is the
  unpatched font), the system sans for names a font can't draw. Labels ask for the
  preview's family by name, or fontconfig may pick the fallback.
- Control (`src/control.rs`): `nyx_core::control` on the app's bus name, for Nyx Assistant: `fonts.installed`, `search`, `install` (never removes).
- Checks: `cargo clippy --all-targets -- -D warnings`, `cargo test`. The network
  install test: `cargo test -- --ignored installs_and_removes` (scratch XDG dirs).
  Visual check: run a test copy with `NYX_FONTS_APP_ID=io.github.design_nexus.nyx.FontsTest`
  (a separate instance) and scratch XDG dirs on a private headless output, never on the
  desktop, and capture it with `grim -o`.
