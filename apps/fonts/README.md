# Fonts

A font manager for [Omarchy](https://omarchy.org). Search every family on
[Google Fonts](https://fonts.google.com) and every [Nerd Font](https://www.nerdfonts.com),
choose them by setting a real page with them, and install them with one click. It takes
its colors from your Omarchy theme and fits a half-screen tile.

## What it does

- **A pairing sheet:** the page has three slots, Heading, Body and Code. Pick a slot,
  point at a font in the list and the page changes; click it to keep it. The page can
  be an article, an app's interface or a terminal, where Nerd Font icons and ligatures
  show. Install every font on the page in one go. Nothing is installed to preview a
  font: one file is downloaded to a cache, and each name in the list is drawn in itself.
- **Google Fonts:** all ~1,950 families, each name in its own font. Show one category
  (sans serif, serif, display, handwriting, monospace), and sort by popularity,
  trending, name or newest.
- **Nerd Fonts:** every font from the latest
  [Nerd Fonts release](https://github.com/ryanoasis/nerd-fonts/releases), patched with
  thousands of icons for terminals and editors.
- **Install for your user:** fonts go to `~/.local/share/fonts/nyx-fonts`, no sudo
  needed, and the font cache is refreshed so other apps see them right away. Choose
  static or variable files for Google Fonts, and which Nerd Font variants to keep
  (Mono for terminals, regular and Propo).
- **This computer:** updates first (update one font or all of them), then everything
  installed with Fonts, with its size and Remove, then every other font on your computer,
  from the system or added by you. Drop .ttf or .otf files on the window to add them.
- **Search** Google Fonts, Nerd Fonts and this computer at once (<kbd>Ctrl</kbd>+<kbd>K</kbd>).
- The font lists are cached and refreshed once a day, so browsing works offline.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/design-nexus/nyx-fonts/main/install.sh | bash -s -- fonts
```

This installs GTK 4 if it's missing, builds with Cargo, and installs `nyx-fonts` to
`~/.local/bin` along with a launcher entry.

To remove it, run the same line with `uninstall.sh` in place of `install.sh`. Fonts you
installed stay installed; add `-s -- --purge` to also remove them and the app's settings.

## Usage

```
nyx-fonts [OPTIONS]
```

| Option | What |
| --- | --- |
| `--section ID` | Open a list: `google`, `nerd`, `computer` (or `installed`, `system`), or `settings` |
| `--search TEXT` | Search both catalogs and this computer |
| `--toggle` | Close the window if it's open, otherwise open it |

| Key | What |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>K</kbd> or <kbd>Ctrl</kbd>+<kbd>F</kbd> | Search |
| <kbd>Esc</kbd> | Clear the search |
| <kbd>Ctrl</kbd>+<kbd>1</kbd>–<kbd>3</kbd> | Google, Nerd Fonts or This computer |
| <kbd>Alt</kbd>+<kbd>1</kbd>–<kbd>3</kbd> | Choose the heading, body or code font |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Settings |
| <kbd>F1</kbd> or <kbd>?</kbd> | Show the shortcuts |
| <kbd>Ctrl</kbd>+<kbd>Q</kbd> | Close |

## Files

| Path | What |
| --- | --- |
| `~/.local/share/fonts/nyx-fonts/` | Installed fonts, one folder per family |
| `~/.local/state/nyx-fonts/installed.toml` | What was installed, from where, and which version |
| `~/.config/nyx-fonts/settings.toml` | Preferences: install options, the page's fonts, theme |
| `~/.config/nyx-fonts/themes/*.toml` | Your own themes |
| `~/.cache/nyx-fonts/` | The font lists and preview files |

## License

MIT. Fonts you install come with their own licenses (mostly the SIL Open Font
License), saved next to the font files.
