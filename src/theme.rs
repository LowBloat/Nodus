//! Visual theme — palette, fonts, and egui style configuration.
//!
//! Commit 1 of the UI overhaul extracts these concerns out of `app.rs`. The
//! public surface is intentionally identical to what existed before; later
//! commits add dark mode, layout constants, and theme persistence.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    epaint::text::VariationCoords,
};

// --- Color palette (light mode, the only mode today) ----------------------------

pub const APP_BG: Color32 = Color32::from_rgb(244, 247, 251);
pub const PAPER: Color32 = Color32::from_rgb(255, 255, 255);
pub const SIDEBAR: Color32 = Color32::from_rgb(234, 240, 246);
pub const INK: Color32 = Color32::from_rgb(24, 34, 48);
pub const MUTED: Color32 = Color32::from_rgb(98, 108, 129);
pub const BORDER: Color32 = Color32::from_rgb(220, 227, 236);
pub const ACCENT: Color32 = Color32::from_rgb(50, 103, 227);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(39, 86, 199);
pub const SUCCESS: Color32 = Color32::from_rgb(35, 122, 87);
pub const WARNING: Color32 = Color32::from_rgb(181, 71, 8);
pub const SOFT_BLUE: Color32 = Color32::from_rgb(232, 239, 255);
pub const SOFT_GREEN: Color32 = Color32::from_rgb(232, 246, 239);
pub const SOFT_WARNING: Color32 = Color32::from_rgb(255, 243, 230);

// --- Font family names (kept private; install_fonts and helpers use them) ------

const INTER_MEDIUM: &str = "inter-medium";
const INTER_SEMIBOLD: &str = "inter-semibold";
const SOURCE_SERIF: &str = "source-serif";
const SOURCE_SERIF_SEMIBOLD: &str = "source-serif-semibold";

// --- Palette struct ------------------------------------------------------------

/// Colors for one visual theme. Today only `light()` is meaningful; a `dark()`
/// variant will be added when the user-facing theme toggle lands.
///
/// Some fields are not yet consumed by `apply` itself — they are read directly
/// by `app.rs` render code (status pills, sync panel, etc.). The `dead_code`
/// allow acknowledges this without scattering it across the codebase.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub sidebar: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub soft_blue: Color32,
    pub soft_green: Color32,
    pub soft_warning: Color32,
    /// Marker so callers (and tests) can know which palette they got.
    pub dark: bool,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            bg: APP_BG,
            surface: PAPER,
            sidebar: SIDEBAR,
            ink: INK,
            muted: MUTED,
            border: BORDER,
            accent: ACCENT,
            accent_hover: ACCENT_HOVER,
            success: SUCCESS,
            warning: WARNING,
            soft_blue: SOFT_BLUE,
            soft_green: SOFT_GREEN,
            soft_warning: SOFT_WARNING,
            dark: false,
        }
    }
}

// --- Style configuration --------------------------------------------------------

/// Apply a palette to egui's `Visuals` and `TextStyle` slots. Mirrors the
/// original `configure_style` behavior; no visual change at this commit.
pub fn apply(ctx: &egui::Context, palette: &Palette) {
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.animation_time = 0.14;
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = palette.bg;
    style.visuals.window_fill = palette.surface;
    style.visuals.extreme_bg_color = palette.surface;
    style.visuals.faint_bg_color = palette.bg;
    style.visuals.code_bg_color = Color32::from_rgb(238, 242, 247);
    style.visuals.widgets.noninteractive.bg_fill = palette.surface;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(247, 249, 252);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    style.visuals.widgets.hovered.bg_fill = palette.soft_blue;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(171, 194, 244));
    style.visuals.widgets.active.bg_fill = palette.accent_hover;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    style.visuals.selection.bg_fill = Color32::from_rgb(196, 214, 255);
    style.visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    style.visuals.hyperlink_color = palette.accent;
    style.visuals.override_text_color = Some(palette.ink);
    style.text_styles.insert(TextStyle::Body, ui_regular(14.0));
    style.text_styles.insert(TextStyle::Button, ui_medium(13.5));
    style.text_styles.insert(TextStyle::Heading, ui_semibold(22.0));
    style.text_styles.insert(TextStyle::Small, ui_regular(11.5));
    ctx.set_theme(egui::Theme::Light);
    ctx.set_style_of(egui::Theme::Light, style);
}

// --- Font installation ---------------------------------------------------------

/// Load Inter (UI) and Source Serif 4 (reading) variable fonts. Must be called
/// once at app startup before the first frame.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let inter = include_bytes!("../assets/fonts/InterVariable.ttf");
    let source_serif = include_bytes!("../assets/fonts/SourceSerif4Variable-Roman.ttf");
    let variable = |bytes: &'static [u8], weight: f32| {
        let mut data = FontData::from_static(bytes);
        data.tweak.coords = VariationCoords::new([(b"wght", weight)]);
        Arc::new(data)
    };

    fonts
        .font_data
        .insert("inter".to_owned(), variable(inter, 400.0));
    fonts
        .font_data
        .insert(INTER_MEDIUM.to_owned(), variable(inter, 500.0));
    fonts
        .font_data
        .insert(INTER_SEMIBOLD.to_owned(), variable(inter, 620.0));
    fonts
        .font_data
        .insert(SOURCE_SERIF.to_owned(), variable(source_serif, 400.0));
    fonts.font_data.insert(
        SOURCE_SERIF_SEMIBOLD.to_owned(),
        variable(source_serif, 620.0),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".to_owned());
    let proportional_fallbacks = fonts.families[&FontFamily::Proportional].clone();
    for name in [INTER_MEDIUM, INTER_SEMIBOLD] {
        let mut family = vec![name.to_owned()];
        family.extend(proportional_fallbacks.iter().skip(1).cloned());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    for name in [SOURCE_SERIF, SOURCE_SERIF_SEMIBOLD] {
        let mut family = vec![name.to_owned()];
        family.extend(proportional_fallbacks.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    ctx.set_fonts(fonts);
}

// --- Font helpers --------------------------------------------------------------

pub fn ui_regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn ui_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(INTER_MEDIUM.into()))
}

pub fn ui_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(INTER_SEMIBOLD.into()))
}

pub fn serif_regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SOURCE_SERIF.into()))
}

pub fn serif_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SOURCE_SERIF_SEMIBOLD.into()))
}
