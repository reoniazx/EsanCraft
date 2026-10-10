//! Fonts: egui's defaults and bundled Thai fallbacks, plus the operating system's own UI fonts
//! loaded at runtime when present (never redistributed). The fallbacks also work on Web/WASM.

use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

/// (path, ttc index) candidates for regular and bold UI text, by platform.
const REGULAR: &[(&str, u32)] = &[
    ("/System/Library/Fonts/HelveticaNeue.ttc", 0),
    ("/System/Library/Fonts/Helvetica.ttc", 0),
    ("C:\\Windows\\Fonts\\segoeui.ttf", 0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0),
    ("/usr/share/fonts/TTF/DejaVuSans.ttf", 0),
    ("/usr/share/fonts/dejavu/DejaVuSans.ttf", 0),
    ("/usr/local/share/fonts/dejavu/DejaVuSans.ttf", 0),
    ("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf", 0),
];
const BOLD: &[(&str, u32)] = &[
    ("/System/Library/Fonts/HelveticaNeue.ttc", 1),
    ("/System/Library/Fonts/Helvetica.ttc", 1),
    ("C:\\Windows\\Fonts\\segoeuib.ttf", 0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 0),
    ("/usr/share/fonts/TTF/DejaVuSans-Bold.ttf", 0),
    ("/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf", 0),
    ("/usr/local/share/fonts/dejavu/DejaVuSans-Bold.ttf", 0),
    ("/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf", 0),
];

/// Asks fontconfig for the desktop's UI font. Only TrueType/OpenType results are used: egui cannot
/// read Type 1 or bitmap fonts, which `fc-match` may return on minimal systems.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn load_from_fontconfig(bold: bool) -> Option<FontData> {
    let pattern = if bold { "sans-serif:style=Bold" } else { "sans-serif" };
    let output = std::process::Command::new("fc-match").args(["--format=%{file}\\n%{index}\\n", pattern]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut lines = stdout.lines();
    let path = lines.next()?.trim();
    // The upper 16 bits carry a variable font's named instance; the face index is the lower 16.
    let index = lines.next()?.trim().parse::<u32>().ok()? & 0xFFFF;
    let bytes = std::fs::read(path).ok()?;
    let sfnt = matches!(bytes.get(..4), Some(b"\0\x01\0\0" | b"OTTO" | b"true" | b"ttcf"));
    if !sfnt {
        return None;
    }
    let mut font = FontData::from_owned(bytes);
    font.index = index;
    Some(font)
}

#[cfg(not(target_arch = "wasm32"))]
fn load(cands: &[(&str, u32)], bold: bool) -> Option<FontData> {
    if std::env::var_os("SOUNDCRAFT_NO_SYSTEM_FONTS").is_some() {
        return None;
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    if let Some(font) = load_from_fontconfig(bold) {
        return Some(font);
    }
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let _ = bold;
    for (p, idx) in cands {
        if let Ok(bytes) = std::fs::read(p) {
            let mut fd = FontData::from_owned(bytes);
            fd.index = *idx;
            return Some(fd);
        }
    }
    None
}

#[cfg(target_arch = "wasm32")]
fn load(_: &[(&str, u32)], _: bool) -> Option<FontData> {
    None
}

/// Always available, including on machines without Thai fonts or filesystem access.
fn bundled_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto-sans-thai-regular".into(),
        Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/noto-sans-thai/NotoSansThai-Regular.ttf"))),
    );
    fonts.font_data.insert(
        "noto-sans-thai-bold".into(),
        Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/noto-sans-thai/NotoSansThai-Bold.ttf"))),
    );
    let mut bold_family = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    bold_family.push("noto-sans-thai-bold".into());
    fonts.families.insert(FontFamily::Name("bold".into()), bold_family);
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("noto-sans-thai-regular".into());
    }
    fonts
}

pub fn definitions() -> FontDefinitions {
    let mut fonts = bundled_definitions();
    let bold_key = FontFamily::Name("bold".into());
    let mut bold_family = fonts.families.get(&bold_key).cloned().unwrap_or_default();
    if let Some(reg) = load(REGULAR, false) {
        fonts.font_data.insert("system-regular".into(), Arc::new(reg));
        if let Some(f) = fonts.families.get_mut(&FontFamily::Proportional) {
            f.insert(0, "system-regular".into());
        }
    }
    if let Some(b) = load(BOLD, true) {
        fonts.font_data.insert("system-bold".into(), Arc::new(b));
        bold_family.insert(0, "system-bold".into());
    } else if fonts.font_data.contains_key("system-regular") {
        bold_family.insert(0, "system-regular".into());
    }
    fonts.families.insert(bold_key, bold_family);
    fonts
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, FontId};

    #[test]
    fn bundled_fonts_cover_thai_in_every_ui_family() {
        let ctx = egui::Context::default();
        ctx.set_fonts(bundled_definitions());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx().fonts_mut(|fonts| {
                for family in [FontFamily::Proportional, FontFamily::Monospace, FontFamily::Name("bold".into())] {
                    let id = FontId::new(12.0, family);
                    let replacement = fonts.layout_no_wrap("\u{10FFFF}".into(), id.clone(), Color32::WHITE);
                    let missing_uv: Vec<_> = replacement.rows.iter().flat_map(|row| row.row.glyphs.iter().map(|g| g.uv_rect)).collect();
                    // All assigned Thai characters: consonants, vowels, tone marks, digits and punctuation.
                    // Compare rendered glyphs: egui's has_glyph can report a false negative for
                    // characters supplied by the same face as its replacement glyph (e.g. ฿ in Hack).
                    for code in (0x0E01..=0x0E3A).chain(0x0E3F..=0x0E5B) {
                        if let Some(ch) = char::from_u32(code) {
                            let galley = fonts.layout_no_wrap(ch.to_string(), id.clone(), Color32::WHITE);
                            let uv: Vec<_> = galley.rows.iter().flat_map(|row| row.row.glyphs.iter().map(|g| g.uv_rect)).collect();
                            assert_ne!(uv, missing_uv, "missing Thai character U+{code:04X} in {:?}", id.family);
                        }
                    }
                    let base = fonts.layout_no_wrap("ก".into(), id.clone(), Color32::WHITE);
                    let marked = fonts.layout_no_wrap("กี่".into(), id.clone(), Color32::WHITE);
                    assert!((base.size().x - marked.size().x).abs() < 0.1, "Thai marks must attach without adding advance width");
                    let mixed = fonts.layout_no_wrap("ร้องนำ - น้ำเสียง ๑๒๓.wav".into(), id, Color32::WHITE);
                    assert!(mixed.size().x.is_finite() && mixed.size().x > 0.0);
                }
            });
        });
        output.textures_delta.clear();
    }
}
