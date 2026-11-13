//! Theming: the shared Nyx theme engine, given this app's stylesheet.

pub use nyx_theme::*;

use crate::prefs;

/// Install the stylesheet and the current palette, and follow Omarchy live.
pub fn install() {
    nyx_theme::use_design_2();
    nyx_theme::install(Style {
        window_class: "fonts-window",
        base_skip: &[],
        css: include_str!("style.css"),
        no_glow_css: "\n.toast { box-shadow: 0 0 0 1px alpha(@theme_accent, 0.6); }\n",
        extra_colors: Some(swatch_colors),
        prefs: || {
            let p = prefs::get();
            ThemePrefs { mode: p.mode, theme: p.theme, glow: p.glow, reduce_motion: p.reduce_motion }
        },
        set_theme: Some(|mode, theme| {
            prefs::update(|p| {
                p.mode = mode;
                p.theme = theme;
            })
        }),
    });
}

/// The theme's named swatches as GTK colours (`@sw_violet`), for the slots' colours.
fn swatch_colors(p: &Palette) -> String {
    let swatches = nyx_theme::swatches();
    let get = |name: &str| swatches.iter().find(|(n, _)| n == name).map(|(_, c)| c.clone()).unwrap_or_else(|| p.accent.clone());
    let mut css = String::new();
    for name in ["blue", "violet", "teal", "amber", "green"] {
        css.push_str(&format!("@define-color sw_{name} {};\n", get(name)));
    }
    css
}
