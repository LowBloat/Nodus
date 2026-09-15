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

// --- Dark mode palette --------------------------------------------------------

pub const DARK_BG: Color32 = Color32::from_rgb(26, 29, 36);
pub const DARK_SURFACE: Color32 = Color32::from_rgb(35, 39, 48);
pub const DARK_SIDEBAR: Color32 = Color32::from_rgb(31, 34, 41);
pub const DARK_INK: Color32 = Color32::from_rgb(230, 233, 239);
pub const DARK_MUTED: Color32 = Color32::from_rgb(139, 148, 166);
pub const DARK_BORDER: Color32 = Color32::from_rgb(46, 51, 64);
pub const DARK_ACCENT: Color32 = Color32::from_rgb(111, 147, 255);
pub const DARK_ACCENT_HOVER: Color32 = Color32::from_rgb(138, 170, 255);
pub const DARK_SUCCESS: Color32 = Color32::from_rgb(92, 199, 143);
pub const DARK_WARNING: Color32 = Color32::from_rgb(255, 154, 77);
pub const DARK_SOFT_BLUE: Color32 = Color32::from_rgb(31, 42, 63);
pub const DARK_SOFT_GREEN: Color32 = Color32::from_rgb(31, 53, 40);
pub const DARK_SOFT_WARNING: Color32 = Color32::from_rgb(61, 40, 24);

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

    pub fn dark() -> Self {
        Self {
            bg: DARK_BG,
            surface: DARK_SURFACE,
            sidebar: DARK_SIDEBAR,
            ink: DARK_INK,
            muted: DARK_MUTED,
            border: DARK_BORDER,
            accent: DARK_ACCENT,
            accent_hover: DARK_ACCENT_HOVER,
            success: DARK_SUCCESS,
            warning: DARK_WARNING,
            soft_blue: DARK_SOFT_BLUE,
            soft_green: DARK_SOFT_GREEN,
            soft_warning: DARK_SOFT_WARNING,
            dark: true,
        }
    }
}

/// Resolve a `ThemeMode` to a concrete palette by consulting the OS preference
/// when the mode is `System`.
#[allow(dead_code)]
pub fn current_palette(ctx: &egui::Context, mode: crate::config::ThemeMode) -> Palette {
    use crate::config::ThemeMode as M;
    match mode {
        M::Light => Palette::light(),
        M::Dark => Palette::dark(),
        M::System => match ctx.system_theme() {
            Some(egui::Theme::Dark) => Palette::dark(),
            _ => Palette::light(),
        },
    }
}

// --- Style configuration --------------------------------------------------------

/// Apply a palette to egui's `Visuals` and `TextStyle` slots. Sets BOTH the
/// light and dark styles so the OS-following `System` mode can flip without
/// recomputing; the visible `set_theme` matches `palette.dark`.
pub fn apply(ctx: &egui::Context, palette: &Palette) {
    let mut light = (*ctx.style_of(egui::Theme::Light)).clone();
    configure_style(&mut light, &Palette::light());
    ctx.set_style_of(egui::Theme::Light, light);

    let mut dark = (*ctx.style_of(egui::Theme::Dark)).clone();
    configure_style(&mut dark, &Palette::dark());
    ctx.set_style_of(egui::Theme::Dark, dark);

    ctx.set_theme(if palette.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
}

fn configure_style(style: &mut egui::Style, palette: &Palette) {
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.animation_time = 0.14;
    style.visuals = if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    style.visuals.panel_fill = palette.bg;
    style.visuals.window_fill = palette.surface;
    style.visuals.extreme_bg_color = palette.surface;
    style.visuals.faint_bg_color = palette.bg;
    style.visuals.code_bg_color = if palette.dark {
        Color32::from_rgb(20, 24, 32)
    } else {
        Color32::from_rgb(238, 242, 247)
    };
    style.visuals.widgets.noninteractive.bg_fill = palette.surface;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    style.visuals.widgets.inactive.bg_fill = if palette.dark {
        palette.bg
    } else {
        Color32::from_rgb(247, 249, 252)
    };
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    style.visuals.widgets.hovered.bg_fill = palette.soft_blue;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.active.bg_fill = palette.accent_hover;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    style.visuals.selection.bg_fill = if palette.dark {
        palette.soft_blue
    } else {
        Color32::from_rgb(196, 214, 255)
    };
    style.visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    style.visuals.hyperlink_color = palette.accent;
    style.visuals.override_text_color = Some(palette.ink);
    style.text_styles.insert(TextStyle::Body, ui_regular(14.0));
    style.text_styles.insert(TextStyle::Button, ui_medium(13.5));
    style.text_styles.insert(TextStyle::Heading, ui_semibold(22.0));
    style.text_styles.insert(TextStyle::Small, ui_regular(11.5));
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

// --- Layout constants ---------------------------------------------------------

/// Magic-number layout values, consolidated so future tweaks live in one place.
pub mod layout {
    /// Inner padding inside the left sidebar (per side).
    pub const SIDEBAR_MARGIN: f32 = 18.0;
    /// Default height of a sidebar note row.
    pub const SIDEBAR_ROW_HEIGHT: f32 = 38.0;
    /// Minimum width for buttons inside the sidebar (avoids 0-width collapse).
    pub const SIDEBAR_BUTTON_WIDTH_MIN: f32 = 180.0;
    /// Allowed drag range for the sidebar.
    pub const SIDEBAR_MIN_WIDTH: f32 = 200.0;
    pub const SIDEBAR_MAX_WIDTH: f32 = 440.0;
    /// Animation duration (seconds) for sidebar collapse/expand.
    pub const SIDEBAR_ANIMATION_TIME: f32 = 0.18;

    /// Inner padding inside the right sync panel (per side).
    pub const SYNC_MARGIN: f32 = 20.0;
    /// Default width of the sync panel when expanded.
    pub const SYNC_WIDTH: f32 = 292.0;
    /// Animation duration for sync panel collapse/expand.
    pub const SYNC_ANIMATION_TIME: f32 = 0.18;

    /// Max width of the centered "paper" inside the editor.
    pub const PAPER_MAX_WIDTH: f32 = 860.0;
    /// Vertical padding around the editor paper card.
    pub const PAPER_VERTICAL_MARGIN: f32 = 22.0;
    /// Horizontal padding around the editor paper card.
    pub const PAPER_HORIZONTAL_MARGIN: f32 = 28.0;
    /// Inner padding inside the paper card (symmetric x).
    pub const PAPER_INNER_PADDING: f32 = 44.0;
    /// Fixed width of the Save button in the editor header.
    pub const SAVE_BUTTON_WIDTH: f32 = 126.0;
}
