//! The window's colors (ui/styles/base.css .proj-theme): the same OKLCH lightness and chroma per
//! token, at its swatch's hue. Light mode: the pill is light.

use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The swatch the window wears (its header's project, or All projects'), as it last said; MAX until it has.
pub static THEME: AtomicUsize = AtomicUsize::new(usize::MAX);
/// Your own Color's hue and strength (the window's swatch OWN), as the window last said.
pub const OWN_SWATCH: usize = 10;
static OWN: std::sync::Mutex<(f64, f64)> = std::sync::Mutex::new((0.0, 0.0));

/// Your own Color's hue and strength now, as the window says it.
pub fn own_hue(h: f64, k: f64) {
    *crate::locking::lock(&OWN, "talk own hue") = (h, k);
}

/// Hue and strength per swatch (Moss, 0, is Cue's own).
const HUE: [(f64, f64); 10] = [(0.0, 0.0), (107.0, 0.3), (301.0, 0.57), (87.0, 0.75), (67.0, 0.37), (157.0, 0.33), (194.0, 0.48), (276.0, 0.64), (353.0, 0.44), (271.0, 0.1)];

/// (bg, onBg, sel, onSel): the window's, so the pill matches it; before the window has said, `project`'s.
pub fn of(snap: &Value, project: &str) -> (String, String, String, String) {
    let i = match THEME.load(Ordering::Relaxed) {
        usize::MAX => swatch(snap, project),
        i => i,
    };
    let (h, k) = match i {
        OWN_SWATCH => *crate::locking::lock(&OWN, "talk own hue"),
        i if i > 0 && i < HUE.len() => HUE[i],
        _ => return ("#3F4A22".into(), "#FFFFFF".into(), "#D2DE96".into(), "#1E2410".into()),
    };
    (oklch(0.385, 0.072 * k, h), "#FFFFFF".into(), oklch(0.875, 0.098 * k, h), oklch(0.25, 0.04 * k, h))
}

/// The swatch you picked for it in Cue, else one from its name (as the window does). Project colors off (the
/// default): every project wears the window's Color (All projects').
fn swatch(snap: &Value, project: &str) -> usize {
    if snap["settings"]["appearance"]["project"] != true {
        return snap["settings"]["project"]["all"].as_u64().map_or(0, |c| c as usize);
    }
    if let Some(c) = snap["settings"]["project"]["colors"][project].as_u64() {
        return c as usize;
    }
    let mut h: u32 = 0;
    for ch in project.encode_utf16() {
        h = (h * 31 + ch as u32) % 9973;
    }
    (h % 10) as usize
}

/// OKLCH to #rrggbb, each channel clipped into sRGB.
pub fn oklch(l: f64, c: f64, h: f64) -> String {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s_ = (l - 0.0894841775 * a - 1.291485548 * b).powi(3);
    let lin = [
        4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
        -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
        -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
    ];
    let ch = |v: f64| {
        let g = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (g.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    format!("#{:02x}{:02x}{:02x}", ch(lin[0]), ch(lin[1]), ch(lin[2]))
}

#[cfg(test)]
mod tests {
    #[test]
    fn moss_is_cues_own_and_a_swatch_gets_its_hue() {
        let snap = serde_json::json!({ "settings": { "appearance": { "project": true }, "project": { "colors": { "cue": 0, "billing": 6 } } } });
        assert_eq!(super::of(&snap, "cue").2, "#D2DE96");
        let teal = super::of(&snap, "billing");
        assert_ne!(teal.2, "#D2DE96");
        assert!(teal.0.starts_with('#') && teal.0.len() == 7, "{teal:?}");
    }
}
