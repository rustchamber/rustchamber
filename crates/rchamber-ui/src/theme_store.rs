use std::sync::OnceLock;

use eframe::egui::Color32;
use serde::Deserialize;

use crate::theme::{self, Palette};

pub const UI_SCALE_MIN: f32 = 0.85;
pub const UI_SCALE_MAX: f32 = 1.25;
pub const SPACING_SCALE_MIN: f32 = 0.9;
pub const SPACING_SCALE_MAX: f32 = 1.15;
pub const CODE_SIZE_MIN: f32 = 9.0;
pub const CODE_SIZE_MAX: f32 = 22.0;
pub const TERMINAL_SIZE_MIN: f32 = 9.0;
pub const TERMINAL_SIZE_MAX: f32 = 24.0;
pub const DEFAULT_CODE_SIZE: f32 = 13.0;
pub const DEFAULT_TERMINAL_SIZE: f32 = 14.0;

pub const UI_WEIGHTS: [(&str, &str); 3] = [
    ("normal", "Normal"),
    ("balanced", "Balanced"),
    ("strong", "Strong"),
];

pub const UI_FONTS: [(&str, &str); 3] = [
    ("inter", "Inter"),
    ("mono", "JetBrains Mono"),
    ("serif", "Instrument Serif"),
];

pub const CODE_FONTS: [(&str, &str); 3] = [
    ("jetbrains-mono", "JetBrains Mono"),
    ("inter", "Inter"),
    ("serif", "Instrument Serif"),
];

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    pub dark_theme: String,
    pub light_theme: String,
    pub brightness: f32,
    pub ui_scale: f32,
    pub ui_weight: String,
    pub ui_font: String,
    pub code_font: String,
    pub code_size: f32,
    pub terminal_size: f32,
    pub spacing_scale: f32,
    pub animated: bool,
    pub language: String,
    pub time_format: String,
    pub week_start: String,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            dark_theme: "rustchamber-dark".into(),
            light_theme: "rustchamber-light".into(),
            brightness: 0.0,
            ui_scale: 1.0,
            ui_weight: "normal".into(),
            ui_font: "inter".into(),
            code_font: "jetbrains-mono".into(),
            code_size: DEFAULT_CODE_SIZE,
            terminal_size: DEFAULT_TERMINAL_SIZE,
            spacing_scale: 1.0,
            animated: true,
            language: "en".into(),
            time_format: "auto".into(),
            week_start: "auto".into(),
        }
    }
}

impl ThemeSettings {
    pub fn is_dark(&self) -> bool {
        self.brightness < 0.5
    }

    pub fn active_theme(&self) -> &str {
        if self.is_dark() {
            &self.dark_theme
        } else {
            &self.light_theme
        }
    }

    pub fn zoom(&self) -> f32 {
        self.ui_scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
    }

    pub fn spacing(&self) -> f32 {
        self.spacing_scale
            .clamp(SPACING_SCALE_MIN, SPACING_SCALE_MAX)
    }

    pub fn code_scale(&self) -> f32 {
        (self.code_size.clamp(CODE_SIZE_MIN, CODE_SIZE_MAX) / DEFAULT_CODE_SIZE).clamp(0.6, 2.0)
    }

    pub fn terminal_scale(&self) -> f32 {
        (self
            .terminal_size
            .clamp(TERMINAL_SIZE_MIN, TERMINAL_SIZE_MAX)
            / DEFAULT_TERMINAL_SIZE)
            .clamp(0.6, 2.0)
    }

    pub fn weight(&self) -> &str {
        match self.ui_weight.as_str() {
            "balanced" | "strong" => self.ui_weight.as_str(),
            _ => "normal",
        }
    }

    pub fn normalize(&mut self) {
        self.brightness = self.brightness.clamp(0.0, 1.0);
        self.ui_scale = self.zoom();
        self.spacing_scale = self.spacing();
        self.code_size = self.code_size.clamp(CODE_SIZE_MIN, CODE_SIZE_MAX);
        self.terminal_size = self
            .terminal_size
            .clamp(TERMINAL_SIZE_MIN, TERMINAL_SIZE_MAX);
        if theme_palette(&self.dark_theme, true).is_none() {
            self.dark_theme = ThemeSettings::default().dark_theme;
        }
        if theme_palette(&self.light_theme, false).is_none() {
            self.light_theme = ThemeSettings::default().light_theme;
        }
    }
}

#[derive(Clone, Debug)]
struct ThemeDoc {
    id: String,
    name: String,
    variant: String,
    colors: Colors,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Metadata {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    variant: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Colors {
    primary: PrimaryColors,
    surface: SurfaceColors,
    interactive: InteractiveColors,
    status: StatusColors,
    chat: ChatColors,
    markdown: MarkdownColors,
    tools: ToolColors,
    syntax: SyntaxColors,
    pr: PrColors,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PrimaryColors {
    base: String,
    hover: String,
    active: String,
    muted: String,
    foreground: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SurfaceColors {
    background: String,
    foreground: String,
    muted: String,
    muted_foreground: String,
    elevated: String,
    subtle: String,
    overlay: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct InteractiveColors {
    border: String,
    selection: String,
    selection_foreground: String,
    border_hover: String,
    border_focus: String,
    focus: String,
    focus_ring: String,
    hover: String,
    active: String,
    cursor: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StatusColors {
    error: String,
    warning: String,
    success: String,
    info: String,
    error_foreground: String,
    warning_foreground: String,
    success_foreground: String,
    info_foreground: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ChatColors {
    user_message_background: String,
    divider: String,
    background: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct MarkdownColors {
    link: String,
    link_hover: String,
    inline_code: String,
    inline_code_background: String,
    blockquote: String,
    blockquote_border: String,
    list_marker: String,
    bold: String,
    italic: String,
    hr: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ToolColors {
    border: String,
    icon: String,
    title: String,
    description: String,
    edit: EditColors,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct EditColors {
    added_background: String,
    removed_background: String,
    modified_background: String,
    line_number: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SyntaxColors {
    base: SyntaxBaseColors,
    tokens: SyntaxTokenColors,
    highlights: SyntaxHighlightColors,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SyntaxBaseColors {
    background: String,
    comment: String,
    keyword: String,
    string: String,
    number: String,
    function: String,
    variable: String,
    #[serde(rename = "type")]
    type_name: String,
    operator: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SyntaxTokenColors {
    class_name: String,
    punctuation: String,
    variable_property: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SyntaxHighlightColors {
    diff_added: String,
    diff_removed: String,
    diff_modified: String,
    diff_modified_background: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PrColors {
    open: String,
    blocked: String,
    merged: String,
    closed: String,
}

#[derive(Deserialize)]
struct ThemeFile {
    metadata: Metadata,
    colors: Colors,
}

const THEME_SOURCES: &[&str] = &[
    include_str!("../assets/themes/aura-dark.json"),
    include_str!("../assets/themes/aura-light.json"),
    include_str!("../assets/themes/ayu-dark.json"),
    include_str!("../assets/themes/ayu-light.json"),
    include_str!("../assets/themes/carbonfox-dark.json"),
    include_str!("../assets/themes/carbonfox-light.json"),
    include_str!("../assets/themes/catppuccin-dark.json"),
    include_str!("../assets/themes/catppuccin-light.json"),
    include_str!("../assets/themes/cursor-dark.json"),
    include_str!("../assets/themes/cursor-light.json"),
    include_str!("../assets/themes/dracula-dark.json"),
    include_str!("../assets/themes/dracula-light.json"),
    include_str!("../assets/themes/everforest-dark.json"),
    include_str!("../assets/themes/everforest-light.json"),
    include_str!("../assets/themes/flexoki-dark.json"),
    include_str!("../assets/themes/flexoki-light.json"),
    include_str!("../assets/themes/github-dark.json"),
    include_str!("../assets/themes/github-light.json"),
    include_str!("../assets/themes/gruvbox-dark.json"),
    include_str!("../assets/themes/gruvbox-light.json"),
    include_str!("../assets/themes/jetbrains-dark.json"),
    include_str!("../assets/themes/jetbrains-light.json"),
    include_str!("../assets/themes/kanagawa-dark.json"),
    include_str!("../assets/themes/kanagawa-light.json"),
    include_str!("../assets/themes/material-dark.json"),
    include_str!("../assets/themes/material-light.json"),
    include_str!("../assets/themes/mono-dark.json"),
    include_str!("../assets/themes/mono-light.json"),
    include_str!("../assets/themes/mono-plus-dark.json"),
    include_str!("../assets/themes/mono-plus-light.json"),
    include_str!("../assets/themes/monokai-dark.json"),
    include_str!("../assets/themes/monokai-light.json"),
    include_str!("../assets/themes/nightowl-dark.json"),
    include_str!("../assets/themes/nightowl-light.json"),
    include_str!("../assets/themes/nord-dark.json"),
    include_str!("../assets/themes/nord-light.json"),
    include_str!("../assets/themes/onedarkpro-dark.json"),
    include_str!("../assets/themes/onedarkpro-light.json"),
    include_str!("../assets/themes/rustchamber-dark.json"),
    include_str!("../assets/themes/rustchamber-light.json"),
    include_str!("../assets/themes/opencode-dark.json"),
    include_str!("../assets/themes/opencode-light.json"),
    include_str!("../assets/themes/osaka-jade-refined-dark.json"),
    include_str!("../assets/themes/osaka-jade-refined-light.json"),
    include_str!("../assets/themes/rosepine-dark.json"),
    include_str!("../assets/themes/rosepine-light.json"),
    include_str!("../assets/themes/solarized-dark.json"),
    include_str!("../assets/themes/solarized-light.json"),
    include_str!("../assets/themes/tokyonight-dark.json"),
    include_str!("../assets/themes/tokyonight-light.json"),
    include_str!("../assets/themes/vercel-dark.json"),
    include_str!("../assets/themes/vercel-light.json"),
    include_str!("../assets/themes/vesper-dark.json"),
    include_str!("../assets/themes/vesper-light.json"),
    include_str!("../assets/themes/vitesse-dark-dark.json"),
    include_str!("../assets/themes/vitesse-light-light.json"),
];

fn catalog() -> &'static [ThemeDoc] {
    static CATALOG: OnceLock<Vec<ThemeDoc>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        THEME_SOURCES
            .iter()
            .filter_map(|source| {
                let file: ThemeFile = serde_json::from_str(source).ok()?;
                Some(ThemeDoc {
                    id: file.metadata.id,
                    name: file.metadata.name,
                    variant: file.metadata.variant,
                    colors: file.colors,
                })
            })
            .collect()
    })
}

pub fn theme_count() -> usize {
    catalog().len()
}

pub fn theme_options(dark: bool) -> Vec<(String, String)> {
    let variant = if dark { "dark" } else { "light" };
    let mut options: Vec<(String, String)> = catalog()
        .iter()
        .filter(|theme| theme.variant == variant && !theme.id.is_empty())
        .map(|theme| (theme.id.clone(), theme.name.clone()))
        .collect();
    options.sort_by(|left, right| {
        left.1
            .to_lowercase()
            .cmp(&right.1.to_lowercase())
            .then_with(|| left.0.cmp(&right.0))
    });
    options
}

pub fn theme_name(id: &str) -> Option<String> {
    catalog()
        .iter()
        .find(|theme| theme.id == id)
        .map(|theme| theme.name.clone())
}

pub fn theme_palette(id: &str, dark: bool) -> Option<Palette> {
    let variant = if dark { "dark" } else { "light" };
    catalog()
        .iter()
        .find(|theme| theme.id == id && theme.variant == variant)
        .map(build_palette)
}

pub fn palette_for(settings: &ThemeSettings) -> Palette {
    let dark = theme_palette(&settings.dark_theme, true).unwrap_or_else(Palette::dark);
    let light = theme_palette(&settings.light_theme, false).unwrap_or_else(Palette::light);
    dark.lerp(&light, settings.brightness)
}

fn build_palette(theme: &ThemeDoc) -> Palette {
    let dark = theme.variant == "dark";
    let base = if dark {
        Palette::dark()
    } else {
        Palette::light()
    };
    let c = &theme.colors;

    let bg = pick(&c.surface.background, base.bg);
    let fg = pick(&c.surface.foreground, base.fg);
    let fg_muted = pick(&c.surface.muted_foreground, base.fg_muted);
    let card = pick(
        &c.surface.elevated,
        mix(bg, fg, if dark { 0.05 } else { 0.03 }),
    );
    let bg_soft = pick(&c.surface.muted, mix(bg, card, 0.35));
    let subtle = pick(&c.surface.subtle, mix(bg, card, 0.5));
    let fg_faint = mix(fg_muted, bg, 0.30);

    let primary = pick(&c.primary.base, base.primary);
    let primary_hover = pick(&c.primary.hover, mix(primary, tint_target(dark), 0.12));
    let primary_active = pick(&c.primary.active, mix(primary, tint_target(dark), 0.22));
    let on_primary = if c.primary.foreground.trim().is_empty() {
        best_on(primary)
    } else {
        pick(&c.primary.foreground, best_on(primary))
    };
    let primary_text = readable_on(bg, primary, dark);

    let border = pick(&c.interactive.border, base.border);
    let border_hover = pick(&c.interactive.border_hover, base.border_hover);
    let border_focus = pick(
        &c.interactive.border_focus,
        pick(&c.interactive.focus, primary),
    );
    let focus_ring = pick(
        &c.interactive.focus_ring,
        Color32::from_rgba_unmultiplied(primary.r(), primary.g(), primary.b(), 0x55),
    );
    let selection = pick(&c.interactive.selection, base.selection);
    let selection_fg = pick(&c.interactive.selection_foreground, fg);

    let red = pick(&c.status.error, base.red);
    let coral = pick(&c.status.warning, base.coral);
    let green = pick(&c.status.success, base.green);
    let blue = pick(&c.status.info, base.blue);
    let error_text = readable_on(bg, red, dark);
    let status_error_text = readable_on(bg, red, dark);
    let status_warning_text = readable_on(bg, coral, dark);
    let status_success_text = readable_on(bg, green, dark);
    let status_info_text = readable_on(bg, blue, dark);
    let warning_text = status_warning_text;
    let success_text = status_success_text;
    let info_text = status_info_text;

    let purple = pick(
        &c.pr.merged,
        pick(&c.pr.closed, pick(&c.syntax.tokens.class_name, base.purple)),
    );
    let code_bg = pick(&c.syntax.base.background, base.code_bg);
    let user_bg = pick(
        &c.chat.user_message_background,
        mix(bg, primary, if dark { 0.14 } else { 0.10 }),
    );
    let hr = pick(&c.markdown.hr, pick(&c.markdown.blockquote_border, base.hr));

    let scrollbar = theme::fade(fg_muted, 0.18);
    let scrollbar_hover = theme::fade(fg_muted, 0.36);

    let diff_added = diff_color(
        &c.syntax.highlights.diff_added,
        &c.tools.edit.added_background,
        code_bg,
        base.diff_added,
    );
    let diff_removed = diff_color(
        &c.syntax.highlights.diff_removed,
        &c.tools.edit.removed_background,
        code_bg,
        base.diff_removed,
    );
    let diff_modified = diff_color(
        &c.syntax.highlights.diff_modified,
        &c.tools.edit.modified_background,
        code_bg,
        base.diff_modified,
    );

    Palette {
        dark,
        bg,
        bg_soft,
        card,
        popover: card,
        hover: pick(&c.interactive.hover, base.hover),
        active: pick(&c.interactive.active, base.active),
        border,
        border_soft: theme::fade(border, theme::EDGE_MUTED_ALPHA),
        fg,
        fg_muted,
        fg_faint,
        primary,
        on_primary,
        primary_text,
        red,
        error_text,
        green,
        blue,
        coral,
        purple,
        code_bg,
        user_bg,
        selection,
        shadow: base.shadow,
        subtle,
        overlay: pick(&c.surface.overlay, base.overlay),
        border_hover,
        border_focus,
        focus_ring,
        selection_fg,
        disabled: theme::fade(fg, 0.5),
        body: theme::fade(fg, 0.9),
        primary_hover,
        primary_active,
        markdown_link: pick(&c.markdown.link, base.markdown_link),
        markdown_link_hover: pick(&c.markdown.link_hover, base.markdown_link_hover),
        inline_code: pick(&c.markdown.inline_code, base.inline_code),
        inline_code_bg: pick(&c.markdown.inline_code_background, base.inline_code_bg),
        blockquote: pick(&c.markdown.blockquote, base.blockquote),
        blockquote_border: pick(&c.markdown.blockquote_border, base.blockquote_border),
        list_marker: pick(&c.markdown.list_marker, base.list_marker),
        bold: pick(&c.markdown.bold, base.bold),
        italic: pick(&c.markdown.italic, base.italic),
        hr,
        scrollbar,
        scrollbar_hover,
        info_text,
        success_text,
        warning_text,
        status_error_text,
        status_warning_text,
        status_success_text,
        status_info_text,
        diff_added,
        diff_removed,
        diff_modified,
        tool_border: pick(&c.tools.border, base.tool_border),
        tool_icon: pick(&c.tools.icon, base.tool_icon),
        tool_title: pick(&c.tools.title, base.tool_title),
        tool_description: pick(&c.tools.description, base.tool_description),
        chat_divider: pick(&c.chat.divider, hr),
        elevation_ring: base.elevation_ring,
    }
}

fn tint_target(dark: bool) -> Color32 {
    if dark { Color32::WHITE } else { Color32::BLACK }
}

fn diff_color(
    highlight: &str,
    edit_background: &str,
    surface: Color32,
    fallback: Color32,
) -> Color32 {
    if let Some(color) = color_hex(highlight) {
        return color;
    }
    if let Some(color) = color_hex(edit_background) {
        return surface.blend(color);
    }
    fallback
}

fn best_on(color: Color32) -> Color32 {
    if theme::contrast_ratio(Color32::WHITE, color) >= theme::contrast_ratio(Color32::BLACK, color)
    {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

fn readable_on(bg: Color32, color: Color32, dark: bool) -> Color32 {
    if theme::contrast_ratio(color, bg) >= 4.5 {
        return color;
    }
    let target = tint_target(dark);
    let mut low = 0.0;
    let mut high = 1.0;
    let mut best = target;
    for _ in 0..12 {
        let mid = (low + high) * 0.5;
        let candidate = mix(color, target, mid);
        if theme::contrast_ratio(candidate, bg) >= 4.6 {
            best = candidate;
            high = mid;
        } else {
            low = mid;
        }
    }
    best
}

fn pick(value: &str, fallback: Color32) -> Color32 {
    color_hex(value).unwrap_or(fallback)
}

fn mix(left: Color32, right: Color32, t: f32) -> Color32 {
    let [lr, lg, lb, la] = left.to_srgba_unmultiplied();
    let [rr, rg, rb, ra] = right.to_srgba_unmultiplied();
    let t = t.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        channel(lr, rr),
        channel(lg, rg),
        channel(lb, rb),
        channel(la, ra),
    )
}

pub fn color_hex(value: &str) -> Option<Color32> {
    let hex = value.trim().trim_start_matches('#');
    let channel = |start: usize| u8::from_str_radix(hex.get(start..start + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Color32::from_rgb(channel(0)?, channel(2)?, channel(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(
            channel(0)?,
            channel(2)?,
            channel(4)?,
            channel(6)?,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_the_house_theme_in_both_variants() {
        let dark = theme_options(true);
        let light = theme_options(false);
        assert!(
            dark.iter()
                .any(|(id, name)| id == "rustchamber-dark" && name == "RustChamber")
        );
        assert!(
            light
                .iter()
                .any(|(id, name)| id == "rustchamber-light" && name == "RustChamber")
        );
        assert!(theme_count() >= 56);
        assert!(dark.len() >= 28);
        assert!(light.len() >= 28);
        assert!(dark.windows(2).all(|pair| pair[0].1 <= pair[1].1));
        assert!(light.windows(2).all(|pair| pair[0].1 <= pair[1].1));
    }

    #[test]
    fn theme_palette_follows_the_variant() {
        let dark = theme_palette("rustchamber-dark", true).unwrap();
        assert!(dark.dark);
        assert_eq!(dark.bg, Color32::from_rgb(0x12, 0x0f, 0x0e));
        assert_eq!(dark.primary, Color32::from_rgb(0xda, 0x7c, 0x47));
        let light = theme_palette("rustchamber-light", false).unwrap();
        assert!(!light.dark);
        assert_eq!(light.bg, Color32::from_rgb(0xfd, 0xfc, 0xfa));
        assert!(theme_palette("rustchamber-light", true).is_none());
        assert!(theme_palette("rustchamber-dark", false).is_none());
    }

    #[test]
    fn every_catalog_entry_builds_a_palette() {
        for dark in [true, false] {
            for (id, _) in theme_options(dark) {
                let palette = theme_palette(&id, dark).unwrap_or_else(|| panic!("{id}"));
                assert_eq!(palette.dark, dark, "{id}");
                assert_ne!(palette.bg, palette.fg, "{id}");
                assert!(theme::contrast_ratio(palette.fg, palette.bg) >= 2.0, "{id}");
            }
        }
    }

    #[test]
    fn pair_selection_resolves_both_sides() {
        let settings = ThemeSettings::default();
        let dark = palette_for(&ThemeSettings {
            brightness: 0.0,
            ..settings.clone()
        });
        let light = palette_for(&ThemeSettings {
            brightness: 1.0,
            ..settings
        });
        assert!(dark.dark);
        assert!(!light.dark);
        assert_eq!(dark.bg, theme_palette("rustchamber-dark", true).unwrap().bg);
        assert_eq!(
            light.bg,
            theme_palette("rustchamber-light", false).unwrap().bg
        );
    }

    #[test]
    fn defaults_are_sane() {
        let settings = ThemeSettings::default();
        assert_eq!(settings.brightness, 0.0);
        assert!(settings.is_dark());
        assert_eq!(settings.ui_scale, 1.0);
        assert_eq!(settings.spacing_scale, 1.0);
        assert_eq!(settings.code_size, 13.0);
        assert_eq!(settings.terminal_size, 14.0);
        assert!(settings.animated);
        assert_eq!(settings.language, "en");
        assert_eq!(settings.time_format, "auto");
        assert_eq!(settings.week_start, "auto");
        assert_eq!(settings.weight(), "normal");
        assert!(theme_palette(&settings.dark_theme, true).is_some());
        assert!(theme_palette(&settings.light_theme, false).is_some());
        let restored: ThemeSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn brightness_blends_between_the_two_themes() {
        let settings = ThemeSettings::default();
        let blender = ThemeSettings {
            brightness: 0.5,
            dark_theme: "nord-dark".into(),
            light_theme: "nord-light".into(),
            ..settings
        };
        let dark = theme_palette("nord-dark", true).unwrap();
        let light = theme_palette("nord-light", false).unwrap();
        let middle = palette_for(&blender);
        assert!(!middle.dark);
        for (value, low, high) in [
            (middle.bg, dark.bg, light.bg),
            (middle.card, dark.card, light.card),
            (middle.primary, dark.primary, light.primary),
        ] {
            let [r, g, b, _] = value.to_srgba_unmultiplied();
            let low = low.to_srgba_unmultiplied();
            let high = high.to_srgba_unmultiplied();
            for (index, channel) in [r, g, b].into_iter().enumerate() {
                assert!(channel >= low[index].min(high[index]));
                assert!(channel <= low[index].max(high[index]));
            }
        }
    }

    #[test]
    fn malformed_colors_fall_back() {
        assert!(color_hex("#12").is_none());
        assert_eq!(color_hex("#DA7C47"), Some(Color32::from_rgb(218, 124, 71)));
        assert_eq!(
            color_hex("#c8c6c52b"),
            Some(Color32::from_rgba_unmultiplied(200, 198, 197, 43))
        );
    }

    #[test]
    fn normalization_keeps_settings_in_range() {
        let mut settings = ThemeSettings {
            brightness: 3.0,
            ui_scale: 9.0,
            spacing_scale: 0.1,
            code_size: 1.0,
            terminal_size: 99.0,
            dark_theme: "missing-dark".into(),
            light_theme: "missing-light".into(),
            ..ThemeSettings::default()
        };
        settings.normalize();
        assert_eq!(settings.brightness, 1.0);
        assert_eq!(settings.ui_scale, UI_SCALE_MAX);
        assert_eq!(settings.spacing_scale, SPACING_SCALE_MIN);
        assert_eq!(settings.code_size, CODE_SIZE_MIN);
        assert_eq!(settings.terminal_size, TERMINAL_SIZE_MAX);
        assert_eq!(settings.dark_theme, "rustchamber-dark");
        assert_eq!(settings.light_theme, "rustchamber-light");
    }
}
