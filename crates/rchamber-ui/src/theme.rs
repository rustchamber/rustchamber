use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle, Visuals,
};
use egui::epaint::text::{FontTweak, VariationCoords};

use crate::theme_store::ThemeSettings;

static FONT_SCALE: AtomicU32 = AtomicU32::new(1.0f32.to_bits());

pub const INTER: &str = "inter";
pub const INTER_MEDIUM: &str = "inter-medium";
pub const INTER_SEMI: &str = "inter-semibold";
pub const INTER_BOLD: &str = "inter-bold";
pub const MONO: &str = "jetbrains-mono";
pub const SERIF: &str = "instrument-serif";
pub const SERIF_ITALIC: &str = "instrument-serif-italic";

pub const HAIRLINE: f32 = 0.5;
pub const BORDER: f32 = 1.0;

pub const EDGE_MUTED_ALPHA: f32 = 0.40;
pub const EDGE_ALPHA: f32 = 0.70;

pub const INK_LUMINANCE_THRESHOLD: f32 = 0.18;
pub const INK_CONTRAST: f32 = 4.5;

pub mod radius {
    pub const XS: u8 = 4;
    pub const SM: u8 = 5;
    pub const TAG: u8 = 6;
    pub const INLINE_CODE: u8 = 6;
    pub const MD: u8 = 7;
    pub const DEMO: u8 = 7;
    pub const MENU_ITEM: u8 = 8;
    pub const TABLE: u8 = 8;
    pub const LG: u8 = 9;
    pub const BUTTON: u8 = 9;
    pub const PANEL: u8 = 10;
    pub const XL: u8 = 12;
    pub const CODE_BLOCK: u8 = 16;
    pub const PILL: u8 = 255;
}

pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_5: f32 = 20.0;
pub const SPACE_6: f32 = 24.0;
pub const SPACE_8: f32 = 32.0;

pub const TEXT_MARKDOWN: f32 = 14.0;
pub const TEXT_CODE: f32 = 12.0;
pub const TEXT_CODE_HEADER: f32 = 13.0;
pub const TEXT_UI_HEADER: f32 = 14.0;
pub const TEXT_UI_SECTION: f32 = 15.0;
pub const TEXT_UI_LABEL: f32 = 13.5;
pub const TEXT_BUTTON: f32 = 12.5;
pub const TEXT_META: f32 = 12.5;
pub const TEXT_MICRO: f32 = 12.0;
pub const TEXT_TAG: f32 = 11.0;
pub const TEXT_PAGE_TITLE: f32 = 17.0;

pub const BUTTON_HEIGHT: f32 = 26.0;
pub const ICON_BUTTON: f32 = 24.0;
pub const TAG_HEIGHT: f32 = 18.0;
pub const KEYCAP_HEIGHT: f32 = 18.0;

pub const LINE_PROSE: f32 = 23.0;
pub const LINE_USER: f32 = 26.0;
pub const LINE_LABEL: f32 = 20.0;
pub const LINE_HEADER: f32 = 20.0;
pub const LINE_SECTION: f32 = 22.0;
pub const LINE_META: f32 = 18.0;
pub const LINE_TIGHT: f32 = 17.0;
pub const LINE_TAG: f32 = 17.0;
pub const LINE_CODE: f32 = 23.0;
pub const LINE_CODE_ROW: f32 = 22.0;
pub const LINE_HEADING: f32 = 26.0;
pub const LINE_HEADING_1: f32 = 26.0;
pub const LINE_HEADING_2: f32 = 23.4;
pub const LINE_HEADING_3: f32 = 20.8;
pub const LINE_HEADING_4: f32 = 18.2;

pub const PARAGRAPH_GAP: f32 = 10.4;
pub const LIST_GAP: f32 = 4.0;
pub const LIST_INDENT: f32 = 24.0;
pub const HEADING_TOP: f32 = 20.0;
pub const HEADING_BOTTOM: f32 = 8.0;
pub const QUOTE_PAD: f32 = 12.8;
pub const INLINE_CODE_PAD_X: f32 = 5.6;
pub const INLINE_CODE_PAD_Y: f32 = 1.6;
pub const CODE_HEADER_HEIGHT: f32 = 32.0;
pub const CODE_HEADER_PAD_X: f32 = 12.0;
pub const CODE_HEADER_PAD_Y: f32 = 6.0;
pub const CODE_PAD_X: f32 = 12.0;
pub const CODE_PAD_Y: f32 = 10.0;

pub const GLASS_POPOVER: f32 = 0.50;
pub const GLASS_POPOVER_DARK: f32 = 0.52;
pub const GLASS_TOOLTIP: f32 = 0.62;
pub const GLASS_TOOLTIP_DARK: f32 = 0.64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

impl ThemeMode {
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => system_dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
            ThemeMode::System => "System",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub bg_soft: Color32,
    pub card: Color32,
    pub popover: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub border: Color32,
    pub border_soft: Color32,
    pub fg: Color32,
    pub fg_muted: Color32,
    pub fg_faint: Color32,
    pub primary: Color32,
    pub on_primary: Color32,
    pub primary_text: Color32,
    pub red: Color32,
    pub error_text: Color32,
    pub green: Color32,
    pub blue: Color32,
    pub coral: Color32,
    pub purple: Color32,
    pub code_bg: Color32,
    pub user_bg: Color32,
    pub selection: Color32,
    pub shadow: Color32,
    pub subtle: Color32,
    pub overlay: Color32,
    pub border_hover: Color32,
    pub border_focus: Color32,
    pub focus_ring: Color32,
    pub selection_fg: Color32,
    pub disabled: Color32,
    pub body: Color32,
    pub primary_hover: Color32,
    pub primary_active: Color32,
    pub markdown_link: Color32,
    pub markdown_link_hover: Color32,
    pub inline_code: Color32,
    pub inline_code_bg: Color32,
    pub blockquote: Color32,
    pub blockquote_border: Color32,
    pub list_marker: Color32,
    pub bold: Color32,
    pub italic: Color32,
    pub hr: Color32,
    pub scrollbar: Color32,
    pub scrollbar_hover: Color32,
    pub info_text: Color32,
    pub success_text: Color32,
    pub warning_text: Color32,
    pub status_error_text: Color32,
    pub status_warning_text: Color32,
    pub status_success_text: Color32,
    pub status_info_text: Color32,
    pub diff_added: Color32,
    pub diff_removed: Color32,
    pub diff_modified: Color32,
    pub tool_border: Color32,
    pub tool_icon: Color32,
    pub tool_title: Color32,
    pub tool_description: Color32,
    pub chat_divider: Color32,
    pub elevation_ring: Color32,
    pub accent_soft: Color32,
    pub accent_border: Color32,
    pub warning_soft: Color32,
    pub warning_border: Color32,
    pub error_soft: Color32,
    pub error_border: Color32,
    pub success_soft: Color32,
    pub success_border: Color32,
    pub info_soft: Color32,
    pub info_border: Color32,
    pub tag_bg: Color32,
    pub diff_added_bg: Color32,
    pub diff_added_emph: Color32,
    pub diff_removed_bg: Color32,
    pub diff_removed_emph: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        let mut palette = Self {
            dark: true,
            bg: hex(0x12, 0x0F, 0x0E),
            bg_soft: hex(0x0E, 0x0C, 0x0B),
            card: hex(0x1B, 0x19, 0x18),
            popover: hex(0x1B, 0x19, 0x18),
            hover: Color32::from_rgba_unmultiplied(255, 255, 255, 15),
            active: Color32::from_rgba_unmultiplied(255, 255, 255, 26),
            border: Color32::from_rgba_unmultiplied(0x24, 0x23, 0x23, 0xBF),
            border_soft: Color32::TRANSPARENT,
            fg: hex(0xDB, 0xD7, 0xCC),
            fg_muted: hex(0x9E, 0x9A, 0x90),
            fg_faint: hex(0x7C, 0x79, 0x70),
            primary: hex(0xDA, 0x7C, 0x47),
            on_primary: hex(0x12, 0x0F, 0x0E),
            primary_text: hex(0xE2, 0x99, 0x6F),
            red: hex(0xDA, 0x5B, 0x4A),
            error_text: hex(0xE1, 0x77, 0x68),
            green: hex(0x76, 0xAD, 0x4F),
            blue: hex(0x47, 0x9F, 0xE6),
            coral: hex(0xC8, 0x84, 0x1B),
            purple: hex(0x99, 0x88, 0xE5),
            code_bg: hex(0x10, 0x0E, 0x0D),
            user_bg: hex(0x2D, 0x1C, 0x12),
            selection: Color32::from_rgba_unmultiplied(0xC8, 0xC6, 0xC5, 0x2B),
            shadow: Color32::from_black_alpha(120),
            subtle: hex(0x0E, 0x0C, 0x0B),
            overlay: Color32::from_black_alpha(0x9E),
            border_hover: hex(0x50, 0x4E, 0x4C),
            border_focus: hex(0xDA, 0x7C, 0x47),
            focus_ring: Color32::from_rgba_unmultiplied(0xDA, 0x7C, 0x47, 0x66),
            selection_fg: hex(0xDB, 0xD7, 0xCC),
            disabled: Color32::from_rgba_unmultiplied(0xDB, 0xD7, 0xCC, 0x6B),
            body: hex(0xC5, 0xC1, 0xB6),
            primary_hover: hex(0xEB, 0x8C, 0x57),
            primary_active: hex(0xFD, 0x9B, 0x66),
            markdown_link: hex(0x5D, 0x99, 0xA9),
            markdown_link_hover: hex(0x6B, 0xA7, 0xB8),
            inline_code: hex(0x76, 0xAD, 0x4F),
            inline_code_bg: hex(0x10, 0x0E, 0x0D),
            blockquote: hex(0x94, 0x91, 0x87),
            blockquote_border: hex(0x30, 0x2E, 0x2B),
            list_marker: Color32::from_rgba_unmultiplied(0x4D, 0x93, 0x4E, 0x99),
            bold: hex(0xDF, 0xDC, 0xD2),
            italic: hex(0xDB, 0xD7, 0xCC),
            hr: hex(0x30, 0x2E, 0x2B),
            scrollbar: Color32::from_rgba_unmultiplied(0x87, 0x7F, 0x73, 0x2E),
            scrollbar_hover: Color32::from_rgba_unmultiplied(0x87, 0x7F, 0x73, 0x5C),
            info_text: hex(0x47, 0x9F, 0xE6),
            success_text: hex(0x76, 0xAD, 0x4F),
            warning_text: hex(0xE1, 0xAD, 0x57),
            status_error_text: hex(0xDB, 0x60, 0x4F),
            status_warning_text: hex(0xC8, 0x84, 0x1B),
            status_success_text: hex(0x76, 0xAD, 0x4F),
            status_info_text: hex(0x47, 0x9F, 0xE6),
            diff_added: hex(0x76, 0xAD, 0x4F),
            diff_removed: hex(0xE1, 0x77, 0x68),
            diff_modified: hex(0x5D, 0x99, 0xA9),
            tool_border: Color32::from_rgba_unmultiplied(0x30, 0x2E, 0x2B, 0x73),
            tool_icon: hex(0x94, 0x91, 0x87),
            tool_title: hex(0xDB, 0xD7, 0xCC),
            tool_description: hex(0x94, 0x91, 0x87),
            chat_divider: hex(0x30, 0x2E, 0x2B),
            elevation_ring: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            accent_soft: Color32::from_rgba_unmultiplied(0xDA, 0x7C, 0x47, 38),
            accent_border: Color32::from_rgba_unmultiplied(0xDA, 0x7C, 0x47, 97),
            warning_soft: Color32::from_rgba_unmultiplied(0xC8, 0x84, 0x1B, 38),
            warning_border: Color32::from_rgba_unmultiplied(0xC8, 0x84, 0x1B, 87),
            error_soft: Color32::from_rgba_unmultiplied(0xDA, 0x5B, 0x4A, 38),
            error_border: Color32::from_rgba_unmultiplied(0xDA, 0x5B, 0x4A, 87),
            success_soft: Color32::from_rgba_unmultiplied(0x76, 0xAD, 0x4F, 36),
            success_border: Color32::from_rgba_unmultiplied(0x76, 0xAD, 0x4F, 82),
            info_soft: Color32::from_rgba_unmultiplied(0x47, 0x9F, 0xE6, 36),
            info_border: Color32::from_rgba_unmultiplied(0x47, 0x9F, 0xE6, 87),
            tag_bg: Color32::from_rgba_unmultiplied(0xDB, 0xD7, 0xCC, 18),
            diff_added_bg: Color32::from_rgba_unmultiplied(0x76, 0xAD, 0x4F, 36),
            diff_added_emph: Color32::from_rgba_unmultiplied(0x76, 0xAD, 0x4F, 71),
            diff_removed_bg: Color32::from_rgba_unmultiplied(0xDA, 0x5B, 0x4A, 38),
            diff_removed_emph: Color32::from_rgba_unmultiplied(0xDA, 0x5B, 0x4A, 77),
        };
        palette.border_soft = palette.edge_muted();
        palette
    }

    pub fn light() -> Self {
        let mut palette = Self {
            dark: false,
            bg: hex(0xFD, 0xFC, 0xFA),
            bg_soft: hex(0xF7, 0xF6, 0xF4),
            card: hex(0xF3, 0xF2, 0xF0),
            popover: hex(0xF3, 0xF2, 0xF0),
            hover: Color32::from_rgba_unmultiplied(0, 0, 0, 10),
            active: Color32::from_rgba_unmultiplied(0, 0, 0, 20),
            border: Color32::from_rgba_unmultiplied(0xE5, 0xE1, 0xDE, 0xBF),
            border_soft: Color32::TRANSPARENT,
            fg: hex(0x44, 0x44, 0x3D),
            fg_muted: hex(0x69, 0x69, 0x60),
            fg_faint: hex(0x8C, 0x8B, 0x82),
            primary: hex(0xB3, 0x50, 0x17),
            on_primary: hex(0xFD, 0xFC, 0xFA),
            primary_text: hex(0x93, 0x41, 0x13),
            red: hex(0xB7, 0x49, 0x3F),
            error_text: hex(0xA0, 0x40, 0x37),
            green: hex(0x51, 0x77, 0x34),
            blue: hex(0x2C, 0x6E, 0xBD),
            coral: hex(0x88, 0x69, 0x14),
            purple: hex(0x79, 0x5C, 0xBC),
            code_bg: hex(0xFA, 0xF9, 0xF7),
            user_bg: hex(0xF3, 0xE9, 0xE3),
            selection: Color32::from_rgba_unmultiplied(0xA9, 0x99, 0x8F, 0x2B),
            shadow: Color32::from_black_alpha(28),
            subtle: hex(0xF7, 0xF6, 0xF4),
            overlay: Color32::from_rgba_unmultiplied(0x39, 0x39, 0x34, 0x47),
            border_hover: hex(0xCB, 0xC7, 0xC2),
            border_focus: hex(0xB3, 0x50, 0x17),
            focus_ring: Color32::from_rgba_unmultiplied(0xB3, 0x50, 0x17, 0x59),
            selection_fg: hex(0x44, 0x44, 0x3D),
            disabled: Color32::from_rgba_unmultiplied(0x44, 0x44, 0x3D, 0x73),
            body: hex(0x57, 0x57, 0x4F),
            primary_hover: hex(0x9A, 0x43, 0x10),
            primary_active: hex(0x85, 0x39, 0x0C),
            markdown_link: hex(0x2D, 0x7D, 0x8C),
            markdown_link_hover: hex(0x29, 0x6A, 0xA3),
            inline_code: hex(0x00, 0x6A, 0x2C),
            inline_code_bg: hex(0xFA, 0xF9, 0xF7),
            blockquote: hex(0x6B, 0x6B, 0x63),
            blockquote_border: hex(0xD8, 0xD5, 0xD0),
            list_marker: Color32::from_rgba_unmultiplied(0x1E, 0x75, 0x4F, 0x99),
            bold: hex(0x3D, 0x3D, 0x37),
            italic: hex(0x44, 0x44, 0x3D),
            hr: hex(0xD8, 0xD5, 0xD0),
            scrollbar: Color32::from_rgba_unmultiplied(0x40, 0x2E, 0x25, 0x2E),
            scrollbar_hover: Color32::from_rgba_unmultiplied(0x40, 0x2E, 0x25, 0x5C),
            info_text: hex(0x27, 0x61, 0xA7),
            success_text: hex(0x51, 0x77, 0x34),
            warning_text: hex(0x78, 0x5C, 0x12),
            status_error_text: hex(0xAE, 0x46, 0x3C),
            status_warning_text: hex(0x82, 0x64, 0x13),
            status_success_text: hex(0x4E, 0x72, 0x32),
            status_info_text: hex(0x2A, 0x68, 0xB4),
            diff_added: hex(0x2C, 0x75, 0x1E),
            diff_removed: hex(0xAB, 0x59, 0x59),
            diff_modified: hex(0x2D, 0x7D, 0x8C),
            tool_border: Color32::from_rgba_unmultiplied(0xD8, 0xD5, 0xD0, 0x73),
            tool_icon: hex(0x69, 0x69, 0x60),
            tool_title: hex(0x44, 0x44, 0x3D),
            tool_description: hex(0x69, 0x69, 0x60),
            chat_divider: hex(0xD8, 0xD5, 0xD0),
            elevation_ring: Color32::from_rgba_unmultiplied(0, 0, 0, 36),
            accent_soft: Color32::from_rgba_unmultiplied(0xB3, 0x50, 0x17, 28),
            accent_border: Color32::from_rgba_unmultiplied(0xB3, 0x50, 0x17, 82),
            warning_soft: Color32::from_rgba_unmultiplied(0x88, 0x69, 0x14, 31),
            warning_border: Color32::from_rgba_unmultiplied(0x88, 0x69, 0x14, 77),
            error_soft: Color32::from_rgba_unmultiplied(0xB7, 0x49, 0x3F, 26),
            error_border: Color32::from_rgba_unmultiplied(0xB7, 0x49, 0x3F, 71),
            success_soft: Color32::from_rgba_unmultiplied(0x51, 0x77, 0x34, 28),
            success_border: Color32::from_rgba_unmultiplied(0x51, 0x77, 0x34, 71),
            info_soft: Color32::from_rgba_unmultiplied(0x2C, 0x6E, 0xBD, 26),
            info_border: Color32::from_rgba_unmultiplied(0x2C, 0x6E, 0xBD, 71),
            tag_bg: Color32::from_rgba_unmultiplied(0x44, 0x44, 0x3D, 15),
            diff_added_bg: Color32::from_rgba_unmultiplied(0x51, 0x77, 0x34, 26),
            diff_added_emph: Color32::from_rgba_unmultiplied(0x51, 0x77, 0x34, 56),
            diff_removed_bg: Color32::from_rgba_unmultiplied(0xB7, 0x49, 0x3F, 26),
            diff_removed_emph: Color32::from_rgba_unmultiplied(0xB7, 0x49, 0x3F, 56),
        };
        palette.border_soft = palette.edge_muted();
        palette
    }

    pub fn from_mode(mode: ThemeMode, system_dark: bool) -> Self {
        if mode.is_dark(system_dark) {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn lerp(&self, other: &Palette, t: f32) -> Palette {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.0 {
            return self.clone();
        }
        if t >= 1.0 {
            return other.clone();
        }
        let blend = |a: Color32, b: Color32| blend_color(a, b, t);
        let bg = blend(self.bg, other.bg);
        let dark_source = if self.dark { self } else { other };
        let light_source = if self.dark { other } else { self };
        let ink = if luminance(bg) < INK_LUMINANCE_THRESHOLD {
            dark_source
        } else {
            light_source
        };
        let mut palette = Palette {
            dark: t < 0.5,
            bg,
            bg_soft: blend(self.bg_soft, other.bg_soft),
            card: blend(self.card, other.card),
            popover: blend(self.popover, other.popover),
            hover: blend(self.hover, other.hover),
            active: blend(self.active, other.active),
            border: blend(self.border, other.border),
            border_soft: blend(self.border_soft, other.border_soft),
            fg: ink.fg,
            fg_muted: ink.fg_muted,
            fg_faint: ink.fg_faint,
            primary: blend(self.primary, other.primary),
            on_primary: ink.on_primary,
            primary_text: ink.primary_text,
            red: blend(self.red, other.red),
            error_text: ink.error_text,
            green: blend(self.green, other.green),
            blue: blend(self.blue, other.blue),
            coral: blend(self.coral, other.coral),
            purple: blend(self.purple, other.purple),
            code_bg: blend(self.code_bg, other.code_bg),
            user_bg: blend(self.user_bg, other.user_bg),
            selection: blend(self.selection, other.selection),
            shadow: blend(self.shadow, other.shadow),
            subtle: blend(self.subtle, other.subtle),
            overlay: blend(self.overlay, other.overlay),
            border_hover: blend(self.border_hover, other.border_hover),
            border_focus: blend(self.border_focus, other.border_focus),
            focus_ring: blend(self.focus_ring, other.focus_ring),
            selection_fg: ink.selection_fg,
            disabled: ink.disabled,
            body: ink.body,
            primary_hover: blend(self.primary_hover, other.primary_hover),
            primary_active: blend(self.primary_active, other.primary_active),
            markdown_link: ink.markdown_link,
            markdown_link_hover: ink.markdown_link_hover,
            inline_code: ink.inline_code,
            inline_code_bg: blend(self.inline_code_bg, other.inline_code_bg),
            blockquote: ink.blockquote,
            blockquote_border: blend(self.blockquote_border, other.blockquote_border),
            list_marker: ink.list_marker,
            bold: ink.bold,
            italic: ink.italic,
            hr: blend(self.hr, other.hr),
            scrollbar: blend(self.scrollbar, other.scrollbar),
            scrollbar_hover: blend(self.scrollbar_hover, other.scrollbar_hover),
            info_text: ink.info_text,
            success_text: ink.success_text,
            warning_text: ink.warning_text,
            status_error_text: ink.status_error_text,
            status_warning_text: ink.status_warning_text,
            status_success_text: ink.status_success_text,
            status_info_text: ink.status_info_text,
            diff_added: blend(self.diff_added, other.diff_added),
            diff_removed: blend(self.diff_removed, other.diff_removed),
            diff_modified: blend(self.diff_modified, other.diff_modified),
            tool_border: blend(self.tool_border, other.tool_border),
            tool_icon: ink.tool_icon,
            tool_title: ink.tool_title,
            tool_description: ink.tool_description,
            chat_divider: blend(self.chat_divider, other.chat_divider),
            elevation_ring: blend(self.elevation_ring, other.elevation_ring),
            accent_soft: blend(self.accent_soft, other.accent_soft),
            accent_border: blend(self.accent_border, other.accent_border),
            warning_soft: blend(self.warning_soft, other.warning_soft),
            warning_border: blend(self.warning_border, other.warning_border),
            error_soft: blend(self.error_soft, other.error_soft),
            error_border: blend(self.error_border, other.error_border),
            success_soft: blend(self.success_soft, other.success_soft),
            success_border: blend(self.success_border, other.success_border),
            info_soft: blend(self.info_soft, other.info_soft),
            info_border: blend(self.info_border, other.info_border),
            tag_bg: blend(self.tag_bg, other.tag_bg),
            diff_added_bg: blend(self.diff_added_bg, other.diff_added_bg),
            diff_added_emph: blend(self.diff_added_emph, other.diff_added_emph),
            diff_removed_bg: blend(self.diff_removed_bg, other.diff_removed_bg),
            diff_removed_emph: blend(self.diff_removed_emph, other.diff_removed_emph),
        };
        palette.fg = readable_ink(palette.fg, palette.bg, INK_CONTRAST + 0.1);
        palette.body = readable_ink(palette.body, palette.bg, INK_CONTRAST);
        palette.bold = readable_ink(palette.bold, palette.bg, INK_CONTRAST + 0.1);
        palette.italic = readable_ink(palette.italic, palette.bg, INK_CONTRAST);
        palette.tool_title = readable_ink(palette.tool_title, palette.bg, INK_CONTRAST);
        palette.primary_text = readable_ink(palette.primary_text, palette.bg, INK_CONTRAST);
        palette.error_text = readable_ink(palette.error_text, palette.bg, INK_CONTRAST);
        palette.info_text = readable_ink(palette.info_text, palette.bg, INK_CONTRAST);
        palette.success_text = readable_ink(palette.success_text, palette.bg, INK_CONTRAST);
        palette.warning_text = readable_ink(palette.warning_text, palette.bg, INK_CONTRAST);
        palette.status_error_text =
            readable_ink(palette.status_error_text, palette.bg, INK_CONTRAST);
        palette.status_warning_text =
            readable_ink(palette.status_warning_text, palette.bg, INK_CONTRAST);
        palette.status_success_text =
            readable_ink(palette.status_success_text, palette.bg, INK_CONTRAST);
        palette.status_info_text = readable_ink(palette.status_info_text, palette.bg, INK_CONTRAST);
        palette.markdown_link = readable_ink(palette.markdown_link, palette.bg, INK_CONTRAST);
        palette.markdown_link_hover =
            readable_ink(palette.markdown_link_hover, palette.bg, INK_CONTRAST);
        palette.inline_code = readable_ink(palette.inline_code, palette.bg, INK_CONTRAST);
        palette.fg_muted = readable_ink(palette.fg_muted, palette.bg, 3.0);
        palette.tool_description = readable_ink(palette.tool_description, palette.bg, 3.0);
        palette.tool_icon = readable_ink(palette.tool_icon, palette.bg, 3.0);
        palette.blockquote = readable_ink(palette.blockquote, palette.bg, 3.0);
        palette.list_marker = readable_ink(palette.list_marker, palette.bg, 2.5);
        palette.fg_faint = readable_ink(palette.fg_faint, palette.bg, 2.0);
        palette.disabled = readable_ink(palette.disabled, palette.bg, 1.8);
        let selection_surface = palette.over(palette.bg, palette.selection);
        palette.selection_fg = readable_ink(palette.selection_fg, selection_surface, INK_CONTRAST);
        palette.on_primary = readable_ink(palette.on_primary, palette.primary, INK_CONTRAST);
        palette.diff_added = readable_ink(palette.diff_added, palette.bg, INK_CONTRAST);
        palette.diff_removed = readable_ink(palette.diff_removed, palette.bg, INK_CONTRAST);
        palette.diff_modified = readable_ink(palette.diff_modified, palette.bg, INK_CONTRAST);
        palette
    }

    pub fn alpha(&self, color: Color32, alpha: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
    }

    pub fn fade(&self, color: Color32, t: f32) -> Color32 {
        let [r, g, b, a] = color.to_srgba_unmultiplied();
        Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * t.clamp(0.0, 1.0)).round() as u8)
    }

    pub fn tint(&self, color: Color32, amount: f32) -> Color32 {
        self.mix(self.card, color, amount)
    }

    pub fn status_color(&self, label: &str) -> Color32 {
        match label {
            "done" | "succeeded" | "completed" => self.green,
            "failed" | "error" => self.red,
            "working" | "running" => self.primary,
            "waiting" | "permission" => self.coral,
            _ => self.fg_muted,
        }
    }

    pub fn mix(&self, a: Color32, b: Color32, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        Color32::from_rgba_premultiplied(
            lerp(a.r() as f32, b.r() as f32, t) as u8,
            lerp(a.g() as f32, b.g() as f32, t) as u8,
            lerp(a.b() as f32, b.b() as f32, t) as u8,
            lerp(a.a() as f32, b.a() as f32, t) as u8,
        )
    }

    pub fn over(&self, base: Color32, top: Color32) -> Color32 {
        base.blend(top)
    }

    pub fn wash(&self, color: Color32, alpha: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
    }

    pub fn scale_alpha(&self, color: Color32, factor: f32) -> Color32 {
        scale_alpha(color, factor)
    }

    pub fn status_bg(&self, color: Color32) -> Color32 {
        self.wash(color, 32)
    }

    pub fn status_border(&self, color: Color32) -> Color32 {
        self.wash(color, 80)
    }

    pub fn with_accent(mut self, accent: Color32) -> Self {
        let target = if self.dark {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        self.primary = accent;
        self.primary_hover = self.mix(accent, target, 0.12);
        self.primary_active = self.mix(accent, target, 0.22);
        self.primary_text = self.readable_text(accent);
        self.border_focus = accent;
        self.focus_ring = self.fade(accent, if self.dark { 0.40 } else { 0.35 });
        self.accent_soft = self.fade(accent, if self.dark { 0.15 } else { 0.11 });
        self.accent_border = self.fade(accent, if self.dark { 0.38 } else { 0.32 });
        self
    }

    pub fn status_text(&self, color: Color32) -> Color32 {
        if color == self.red {
            self.status_error_text
        } else if color == self.coral {
            self.status_warning_text
        } else if color == self.green {
            self.status_success_text
        } else if color == self.blue {
            self.status_info_text
        } else if color == self.primary {
            self.primary_text
        } else {
            self.readable_text(color)
        }
    }

    pub fn readable_text(&self, color: Color32) -> Color32 {
        if contrast_ratio(color, self.bg) >= 4.5 {
            return color;
        }
        let target = if self.dark {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        let mut low = 0.0;
        let mut high = 1.0;
        let mut best = target;
        for _ in 0..12 {
            let mid = (low + high) * 0.5;
            let candidate = self.mix(color, target, mid);
            if contrast_ratio(candidate, self.bg) >= 4.6 {
                best = candidate;
                high = mid;
            } else {
                low = mid;
            }
        }
        best
    }

    pub fn body_fg(&self) -> Color32 {
        self.body
    }

    pub fn muted_fg(&self) -> Color32 {
        self.fg_muted
    }

    pub fn disabled_fg(&self) -> Color32 {
        self.disabled
    }

    pub fn hover_fill(&self) -> Color32 {
        self.hover
    }

    pub fn press_fill(&self) -> Color32 {
        self.active
    }

    pub fn selected_fill(&self) -> Color32 {
        self.selection
    }

    pub fn focus_fill(&self) -> Color32 {
        self.hover
    }

    pub fn edge_muted(&self) -> Color32 {
        self.fade(self.border, EDGE_MUTED_ALPHA)
    }

    pub fn edge_color(&self) -> Color32 {
        self.fade(self.border, EDGE_ALPHA)
    }

    pub fn edge_strong(&self) -> Color32 {
        self.border
    }

    pub fn raised_edge(&self) -> Color32 {
        self.fade(self.fg, if self.dark { 0.14 } else { 0.16 })
    }

    pub fn field_edge(&self) -> Color32 {
        self.fade(self.fg, 0.16)
    }

    pub fn field_edge_focus(&self) -> Color32 {
        self.fade(self.fg, 0.26)
    }

    pub fn raised_highlight(&self) -> Option<Color32> {
        if self.dark {
            Some(self.wash(self.fg, 20))
        } else {
            None
        }
    }

    pub fn field_highlight(&self) -> (f32, Color32) {
        if self.dark {
            (HAIRLINE, self.wash(self.fg, 20))
        } else {
            (BORDER, Color32::from_black_alpha(10))
        }
    }

    pub fn tile_tone(&self) -> Color32 {
        self.wash(self.fg, if self.dark { 8 } else { 10 })
    }

    pub fn tile_inset(&self) -> Color32 {
        self.wash(self.fg, if self.dark { 11 } else { 15 })
    }

    pub fn tile_hover(&self) -> Color32 {
        self.wash(self.fg, if self.dark { 14 } else { 18 })
    }

    pub fn glass_popover_fill(&self) -> Color32 {
        let opacity = if self.dark {
            GLASS_POPOVER_DARK
        } else {
            GLASS_POPOVER
        };
        self.over(self.bg, self.fade(self.card, opacity))
    }

    pub fn glass_tooltip_fill(&self) -> Color32 {
        let opacity = if self.dark {
            GLASS_TOOLTIP_DARK
        } else {
            GLASS_TOOLTIP
        };
        self.over(self.bg, self.fade(self.card, opacity))
    }

    pub fn scrim(&self) -> Color32 {
        self.overlay
    }

    pub fn field_fill(&self) -> Color32 {
        if self.dark {
            self.mix(self.card, self.bg, 0.5)
        } else {
            self.card
        }
    }

    pub fn hairline(&self) -> Stroke {
        Stroke::new(HAIRLINE, self.edge_muted())
    }

    pub fn edge(&self) -> Stroke {
        Stroke::new(BORDER, self.edge_color())
    }

    pub fn strong_edge(&self) -> Stroke {
        Stroke::new(BORDER, self.edge_strong())
    }

    pub fn focus_edge(&self) -> Stroke {
        Stroke::new(BORDER, self.field_edge_focus())
    }

    pub fn raised_fill(&self) -> Color32 {
        self.card
    }

    pub fn code_inline_bg(&self) -> Color32 {
        self.fade(self.inline_code_bg, 0.5)
    }

    pub fn inline_code_chip(&self) -> Color32 {
        self.over(self.code_inline_bg(), self.wash(self.fg, 10))
    }

    pub fn modal_shadow(&self) -> egui::epaint::Shadow {
        egui::epaint::Shadow {
            offset: [0, 24],
            blur: 64,
            spread: 0,
            color: Color32::from_black_alpha(if self.dark { 170 } else { 96 }),
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn blend_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    let channel = |left: u8, right: u8| lerp(left as f32, right as f32, t).round() as u8;
    Color32::from_rgba_unmultiplied(
        channel(ar, br),
        channel(ag, bg),
        channel(ab, bb),
        channel(aa, ba),
    )
}

fn readable_ink(color: Color32, bg: Color32, minimum: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let (tr, tg, tb) = if luminance(bg) < INK_LUMINANCE_THRESHOLD {
        (255, 255, 255)
    } else {
        (0, 0, 0)
    };
    let sample = |amount: f32| {
        Color32::from_rgba_unmultiplied(
            lerp(r as f32, tr as f32, amount).round() as u8,
            lerp(g as f32, tg as f32, amount).round() as u8,
            lerp(b as f32, tb as f32, amount).round() as u8,
            a,
        )
    };
    let ratio = |value: Color32| contrast_ratio(bg.blend(value), bg);
    if ratio(color) >= minimum {
        return color;
    }
    let mut low = 0.0;
    let mut high = 1.0;
    let mut best = sample(1.0);
    for _ in 0..16 {
        let mid = (low + high) * 0.5;
        let candidate = sample(mid);
        if ratio(candidate) >= minimum {
            best = candidate;
            high = mid;
        } else {
            low = mid;
        }
    }
    best
}

fn channel_luminance(value: u8) -> f32 {
    let c = value as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn luminance(color: Color32) -> f32 {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    0.2126 * channel_luminance(r) + 0.7152 * channel_luminance(g) + 0.0722 * channel_luminance(b)
}

pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (lighter, darker) = {
        let la = luminance(a);
        let lb = luminance(b);
        if la >= lb { (la, lb) } else { (lb, la) }
    };
    (lighter + 0.05) / (darker + 0.05)
}

const fn hex(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn scale_alpha(color: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * factor.clamp(0.0, 1.0)).round() as u8)
}

pub fn fade(color: Color32, t: f32) -> Color32 {
    scale_alpha(color, t)
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let inter = FontData::from_static(include_bytes!("../assets/fonts/InterVariable.ttf"));
    let mono = FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    let serif = FontData::from_static(include_bytes!(
        "../assets/fonts/InstrumentSerif-Regular.ttf"
    ));
    let serif_italic =
        FontData::from_static(include_bytes!("../assets/fonts/InstrumentSerif-Italic.ttf"));

    let family = |name: &str, weight: f32, data: &FontData| {
        let mut data = data.clone();
        data.tweak = FontTweak {
            coords: VariationCoords::new([(b"wght", weight)]),
            ..Default::default()
        };
        (name.to_string(), data)
    };

    let faces = [
        family(INTER, 410.0, &inter),
        family(INTER_MEDIUM, 500.0, &inter),
        family(INTER_SEMI, 600.0, &inter),
        family(INTER_BOLD, 700.0, &inter),
        family(MONO, 420.0, &mono),
    ];
    for (name, data) in faces {
        fonts.font_data.insert(name, Arc::new(data));
    }
    fonts.font_data.insert(SERIF.into(), Arc::new(serif));
    fonts
        .font_data
        .insert(SERIF_ITALIC.into(), Arc::new(serif_italic));

    let families: [(FontFamily, &str); 5] = [
        (FontFamily::Proportional, INTER),
        (FontFamily::Name(INTER_MEDIUM.into()), INTER_MEDIUM),
        (FontFamily::Name(INTER_SEMI.into()), INTER_SEMI),
        (FontFamily::Name(INTER_BOLD.into()), INTER_BOLD),
        (FontFamily::Monospace, MONO),
    ];
    for (family, name) in families {
        let mut chain = vec![name.to_string()];
        chain.extend(fallbacks.iter().cloned());
        fonts.families.insert(family, chain);
    }
    for (family, name) in [
        (FontFamily::Name(SERIF.into()), SERIF),
        (FontFamily::Name(SERIF_ITALIC.into()), SERIF_ITALIC),
    ] {
        fonts
            .families
            .insert(family, vec![name.to_string(), INTER.to_string()]);
    }

    ctx.set_fonts(fonts.clone());
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::unique("theme.fonts"), fonts);
        data.insert_temp(
            egui::Id::unique("theme.font-signature"),
            format!("{INTER}|{MONO}"),
        );
    });
}

pub fn font_scale() -> f32 {
    f32::from_bits(FONT_SCALE.load(Ordering::Relaxed))
}

fn set_font_scale(value: f32) {
    FONT_SCALE.store(value.to_bits(), Ordering::Relaxed);
}

fn text_styles() -> std::collections::BTreeMap<TextStyle, FontId> {
    [
        (TextStyle::Heading, font_semibold(20.0)),
        (TextStyle::Body, font(TEXT_MARKDOWN)),
        (TextStyle::Monospace, font_mono(TEXT_CODE)),
        (TextStyle::Button, font_medium(TEXT_UI_LABEL)),
        (TextStyle::Small, font(TEXT_MICRO)),
    ]
    .into()
}

pub fn apply_typography(ctx: &egui::Context, settings: &ThemeSettings) {
    set_font_scale(settings.code_scale());
    ctx.set_zoom_factor(settings.zoom());
    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing *= settings.spacing();
    style.animation_time = if settings.animated {
        crate::motion::FAST
    } else {
        0.0
    };
    style.text_styles = text_styles();
    ctx.set_global_style(style);
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::unique("settings.reduce_motion"),
            !settings.animated,
        );
    });
    apply_font_families(ctx, settings);
}

fn apply_font_families(ctx: &egui::Context, settings: &ThemeSettings) {
    let weight = match settings.weight() {
        "strong" => INTER_SEMI,
        "balanced" => INTER_MEDIUM,
        _ => INTER,
    };
    let ui_face = match settings.ui_font.as_str() {
        "mono" | "jetbrains-mono" => MONO,
        "serif" | "instrument-serif" => SERIF,
        _ => weight,
    };
    let code_face = match settings.code_font.as_str() {
        "inter" | "sans" => INTER,
        "serif" | "instrument-serif" => SERIF,
        _ => MONO,
    };
    let signature = format!("{ui_face}|{code_face}");
    let current =
        ctx.data(|data| data.get_temp::<String>(egui::Id::unique("theme.font-signature")));
    if current.as_deref() == Some(signature.as_str()) {
        return;
    }
    let Some(base) =
        ctx.data(|data| data.get_temp::<FontDefinitions>(egui::Id::unique("theme.fonts")))
    else {
        return;
    };
    let ui_fallback: Vec<String> = base
        .families
        .get(&FontFamily::Proportional)
        .map(|chain| {
            chain
                .iter()
                .filter(|name| name.as_str() != INTER)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mono_fallback: Vec<String> = base
        .families
        .get(&FontFamily::Monospace)
        .map(|chain| {
            chain
                .iter()
                .filter(|name| name.as_str() != MONO)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let chain = |primary: &str, fallbacks: &[String]| {
        let mut chain = vec![primary.to_string()];
        chain.extend(
            fallbacks
                .iter()
                .filter(|name| name.as_str() != primary)
                .cloned(),
        );
        chain
    };
    let mut fonts = base;
    fonts
        .families
        .insert(FontFamily::Proportional, chain(ui_face, &ui_fallback));
    fonts
        .families
        .insert(FontFamily::Monospace, chain(code_face, &mono_fallback));
    ctx.set_fonts(fonts);
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::unique("theme.font-signature"), signature);
    });
}

pub fn accent_color(value: &str) -> Option<Color32> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let channel = |start: usize| u8::from_str_radix(&hex[start..start + 2], 16).ok();
    Some(Color32::from_rgb(channel(0)?, channel(2)?, channel(4)?))
}

pub fn apply_density(ctx: &egui::Context, compact: bool) {
    let mut style = (*ctx.global_style()).clone();
    if compact {
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.interact_size = egui::vec2(30.0, 26.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
    } else {
        style.spacing.item_spacing = egui::vec2(SPACE_2, 6.0);
        style.spacing.interact_size = egui::vec2(32.0, 28.0);
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
    }
    ctx.set_global_style(style);
}

pub fn apply(ctx: &egui::Context, palette: &Palette) {
    ctx.set_theme(if palette.dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    let mut style = (*ctx.global_style()).clone();
    style.visuals = visuals(palette);
    style.spacing.item_spacing = egui::vec2(SPACE_2, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    style.spacing.interact_size = egui::vec2(32.0, 28.0);
    style.spacing.window_margin = Margin::same(20);
    style.spacing.indent = 12.0;
    style.spacing.menu_margin = Margin::symmetric(12, 6);
    style.spacing.combo_width = 180.0;
    style.spacing.scroll.bar_width = 10.0;
    style.spacing.scroll.floating_width = 4.0;
    style.spacing.scroll.floating_allocated_width = 4.0;
    style.spacing.scroll.bar_inner_margin = 3.0;
    style.spacing.scroll.floating = true;
    style.interaction.tooltip_delay = 0.30;
    style.interaction.tooltip_grace_time = 0.15;
    style.interaction.selectable_labels = true;
    style.animation_time = crate::motion::FAST;
    style.visuals.weak_text_color = Some(palette.blockquote);
    style.text_styles = text_styles();
    ctx.set_global_style(style);
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::unique("theme.palette"), palette.clone());
    });
}

pub fn current_palette(ctx: &egui::Context) -> Palette {
    ctx.data(|data| data.get_temp::<Palette>(egui::Id::unique("theme.palette")))
        .unwrap_or_else(Palette::dark)
}

fn visuals(p: &Palette) -> Visuals {
    let mut visuals = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.glass_popover_fill();
    visuals.extreme_bg_color = p.code_bg;
    visuals.faint_bg_color = p.hover;
    visuals.code_bg_color = p.code_bg;
    visuals.window_stroke = Stroke::new(HAIRLINE, p.edge_strong());
    visuals.window_corner_radius = CornerRadius::same(radius::XL);
    visuals.menu_corner_radius = CornerRadius::same(radius::XL);
    visuals.selection.bg_fill = p.selection;
    visuals.selection.stroke = Stroke::new(BORDER, p.selection_fg);
    visuals.hyperlink_color = p.markdown_link;
    visuals.warn_fg_color = p.coral;
    visuals.error_fg_color = p.red;
    visuals.text_cursor.stroke = Stroke::new(BORDER, p.primary);
    visuals.striped = false;
    visuals.slider_trailing_fill = true;
    visuals.window_shadow = p.modal_shadow();
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 120 } else { 56 }),
    };

    let corner = CornerRadius::same(radius::BUTTON);
    visuals.widgets.noninteractive.bg_fill = p.bg;
    visuals.widgets.noninteractive.weak_bg_fill = p.bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(HAIRLINE, p.edge_muted());
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(BORDER, p.fg);
    visuals.widgets.noninteractive.corner_radius = corner;

    visuals.widgets.inactive.bg_fill = p.scrollbar;
    visuals.widgets.inactive.weak_bg_fill = p.card;
    visuals.widgets.inactive.bg_stroke = Stroke::new(HAIRLINE, p.edge_muted());
    visuals.widgets.inactive.fg_stroke = Stroke::new(BORDER, p.fg_muted);
    visuals.widgets.inactive.corner_radius = corner;

    visuals.widgets.hovered.bg_fill = p.scrollbar_hover;
    visuals.widgets.hovered.weak_bg_fill = p.hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(HAIRLINE, p.edge_color());
    visuals.widgets.hovered.fg_stroke = Stroke::new(BORDER, p.fg);
    visuals.widgets.hovered.corner_radius = corner;

    visuals.widgets.active.bg_fill = p.active;
    visuals.widgets.active.weak_bg_fill = p.active;
    visuals.widgets.active.bg_stroke = Stroke::new(HAIRLINE, p.edge_color());
    visuals.widgets.active.fg_stroke = Stroke::new(BORDER, p.fg);
    visuals.widgets.active.corner_radius = corner;

    visuals.widgets.open.bg_fill = p.glass_popover_fill();
    visuals.widgets.open.weak_bg_fill = p.glass_popover_fill();
    visuals.widgets.open.bg_stroke = Stroke::new(HAIRLINE, p.edge_color());
    visuals.widgets.open.fg_stroke = Stroke::new(BORDER, p.fg);
    visuals.widgets.open.corner_radius = CornerRadius::same(radius::XL);

    visuals.widgets.noninteractive.expansion = 0.0;
    visuals
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn font_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(INTER_MEDIUM.into()))
}

pub fn font_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(INTER_SEMI.into()))
}

pub fn font_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(INTER_BOLD.into()))
}

pub fn font_mono(size: f32) -> FontId {
    FontId::new(size * font_scale(), FontFamily::Monospace)
}

pub fn font_serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SERIF.into()))
}

pub fn font_serif_italic(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SERIF_ITALIC.into()))
}

pub fn font_prose() -> FontId {
    font(TEXT_MARKDOWN)
}

pub fn font_label() -> FontId {
    font(TEXT_UI_LABEL)
}

pub fn font_label_medium() -> FontId {
    font_medium(TEXT_UI_LABEL)
}

pub fn font_meta() -> FontId {
    font(TEXT_META)
}

pub fn font_meta_medium() -> FontId {
    font_medium(TEXT_META)
}

pub fn font_micro() -> FontId {
    font(TEXT_MICRO)
}

pub fn font_micro_medium() -> FontId {
    font_medium(TEXT_MICRO)
}

pub fn font_tag() -> FontId {
    font_mono(TEXT_TAG)
}

pub fn font_code() -> FontId {
    font_mono(TEXT_CODE)
}

pub fn heading_line_height(level: usize) -> f32 {
    match level {
        1 => LINE_HEADING_1,
        2 => LINE_HEADING_2,
        3 => LINE_HEADING_3,
        _ => LINE_HEADING_4,
    }
}

pub fn font_heading(level: usize) -> FontId {
    match level {
        1 => font_semibold(20.0),
        2 => font_semibold(18.0),
        3 => font_semibold(16.0),
        _ => font_semibold(TEXT_MARKDOWN),
    }
}

pub fn row_height(ui: &egui::Ui, font: &FontId) -> f32 {
    ui.ctx().fonts_mut(|fonts| fonts.row_height(font))
}

pub fn extra_line_spacing(ui: &egui::Ui, font: &FontId, target: f32) -> f32 {
    (target - row_height(ui, font)).max(0.0)
}

pub fn hairline_stroke(p: &Palette) -> Stroke {
    Stroke::new(HAIRLINE, p.edge_muted())
}

pub fn edge_stroke(p: &Palette) -> Stroke {
    Stroke::new(BORDER, p.edge_color())
}

pub fn strong_edge_stroke(p: &Palette) -> Stroke {
    Stroke::new(BORDER, p.edge_strong())
}

pub fn focus_ring_stroke(p: &Palette) -> Stroke {
    Stroke::new(BORDER, p.field_edge_focus())
}

pub fn card_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(HAIRLINE, p.elevation_ring))
        .corner_radius(radius::XL)
        .inner_margin(Margin::same(12))
}

pub fn panel_card_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(HAIRLINE, p.elevation_ring))
        .corner_radius(radius::PANEL)
}

pub fn popover_frame(p: &Palette) -> egui::Frame {
    popover_frame_t(p, 1.0)
}

pub fn popover_frame_t(p: &Palette, t: f32) -> egui::Frame {
    egui::Frame::new()
        .fill(p.fade(p.glass_popover_fill(), t))
        .stroke(Stroke::new(HAIRLINE, p.fade(p.elevation_ring, t)))
        .corner_radius(radius::XL)
        .inner_margin(Margin::same(6))
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: p.fade(p.shadow, t),
        })
}

pub fn tooltip_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.glass_tooltip_fill())
        .stroke(Stroke::new(BORDER, p.edge_color()))
        .corner_radius(radius::XL)
        .inner_margin(Margin::symmetric(12, 6))
        .shadow(egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: p.fade(p.shadow, 0.8),
        })
}

pub fn dialog_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.bg)
        .corner_radius(radius::XL)
        .inner_margin(Margin::same(20))
        .shadow(p.modal_shadow())
}

pub fn divider(ui: &mut egui::Ui, p: &Palette) {
    let rect = ui.available_rect_before_wrap();
    let y = rect.top() + 0.5;
    ui.painter().hline(
        rect.left()..=rect.right(),
        y,
        Stroke::new(HAIRLINE, p.edge_muted()),
    );
    ui.add_space(BORDER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_code_surface_stays_quiet() {
        let light = Palette::light();
        let dark = Palette::dark();
        let light_straight = light.code_inline_bg().to_srgba_unmultiplied();
        let dark_straight = dark.code_inline_bg().to_srgba_unmultiplied();
        assert!(light_straight[0] > 230);
        assert!(light_straight[1] > 230);
        assert_eq!(light_straight[3], 128);
        assert!(dark_straight[0] < 40);
        assert_eq!(dark_straight[3], 128);
    }

    #[test]
    fn premultiplied_blends_keep_alpha() {
        let p = Palette::light();
        let mut hover = p.over(p.bg, p.hover);
        assert_eq!(hover.a(), 255);
        assert!(hover.r() < p.bg.r());
        hover = p.over(p.bg, p.selection);
        assert_eq!(hover.a(), 255);
        let faded = scale_alpha(p.focus_ring, 0.5);
        assert_eq!(faded.a(), (p.focus_ring.a() as f32 * 0.5).round() as u8);
    }

    #[test]
    fn fade_multiplies_existing_alpha() {
        let p = Palette::light();
        assert_eq!(
            p.fade(p.border, 0.5).a(),
            (p.border.a() as f32 * 0.5).round() as u8
        );
        assert_eq!(p.fade(p.bg, 0.5).a(), 128);
        assert_eq!(
            p.edge_muted().a(),
            (p.border.a() as f32 * 0.40).round() as u8
        );
        assert_eq!(
            p.edge_color().a(),
            (p.border.a() as f32 * 0.70).round() as u8
        );
    }

    #[test]
    fn soft_edges_match_the_muted_reference_edge() {
        let light = Palette::light();
        let dark = Palette::dark();
        assert_eq!(light.border_soft, light.edge_muted());
        assert_eq!(dark.border_soft, dark.edge_muted());
        assert_eq!(light.border_soft.a(), 76);
        assert_eq!(dark.border_soft.a(), 76);
    }

    #[test]
    fn control_highlights_follow_the_theme() {
        let light = Palette::light();
        let dark = Palette::dark();
        assert!(light.raised_highlight().is_none());
        assert!(dark.raised_highlight().is_some());
        assert_eq!(light.field_highlight().0, BORDER);
        assert_eq!(dark.field_highlight().0, HAIRLINE);
    }

    #[test]
    fn token_scale_matches_reference() {
        assert_eq!(HAIRLINE, 0.5);
        assert_eq!(BORDER, 1.0);
        assert_eq!(radius::XS, 4);
        assert_eq!(radius::SM, 5);
        assert_eq!(radius::INLINE_CODE, 6);
        assert_eq!(radius::MD, 7);
        assert_eq!(radius::TABLE, 8);
        assert_eq!(radius::LG, 9);
        assert_eq!(radius::XL, 12);
        assert_eq!(radius::CODE_BLOCK, 16);
        assert_eq!(TEXT_MARKDOWN, 14.0);
        assert_eq!(TEXT_CODE, 12.0);
        assert_eq!(TEXT_UI_LABEL, 13.5);
        assert_eq!(TEXT_META, 12.5);
        assert_eq!(LINE_PROSE, 23.0);
        assert_eq!(PARAGRAPH_GAP, 10.4);
        assert_eq!(QUOTE_PAD, 12.8);
        assert_eq!(CODE_HEADER_HEIGHT, 32.0);
    }

    #[test]
    fn dense_app_scale_matches_the_tokens_doc() {
        assert_eq!(TEXT_PAGE_TITLE, 17.0);
        assert_eq!(TEXT_UI_SECTION, 15.0);
        assert_eq!(TEXT_UI_HEADER, 14.0);
        assert_eq!(TEXT_UI_LABEL, 13.5);
        assert_eq!(TEXT_BUTTON, 12.5);
        assert_eq!(TEXT_CODE_HEADER, 13.0);
        assert_eq!(TEXT_META, 12.5);
        assert_eq!(TEXT_CODE, 12.0);
        assert_eq!(TEXT_TAG, 11.0);
        assert_eq!(LINE_USER, 26.0);
        assert_eq!(LINE_LABEL, 20.0);
        assert_eq!(LINE_META, 18.0);
        assert_eq!(LINE_TAG, 17.0);
        assert_eq!(LINE_CODE_ROW, 22.0);
        assert_eq!(BUTTON_HEIGHT, 26.0);
        assert_eq!(ICON_BUTTON, 24.0);
        assert_eq!(SPACE_1, 4.0);
        assert_eq!(SPACE_2, 8.0);
        assert_eq!(SPACE_3, 12.0);
        assert_eq!(SPACE_4, 16.0);
        assert_eq!(SPACE_5, 20.0);
        assert_eq!(SPACE_6, 24.0);
        assert_eq!(SPACE_8, 32.0);
        assert_eq!(radius::TAG, 6);
        assert_eq!(radius::DEMO, 7);
        assert_eq!(radius::MENU_ITEM, 8);
        assert_eq!(radius::BUTTON, 9);
        assert_eq!(radius::PANEL, 10);
    }

    #[test]
    fn diff_and_action_pairs_match_the_reference() {
        let light = Palette::light();
        let dark = Palette::dark();
        assert_eq!(light.on_primary, Color32::from_rgb(0xFD, 0xFC, 0xFA));
        assert_eq!(dark.on_primary, Color32::from_rgb(0x12, 0x0F, 0x0E));
        assert_eq!(light.diff_added, Color32::from_rgb(0x2C, 0x75, 0x1E));
        assert_eq!(dark.diff_added, Color32::from_rgb(0x76, 0xAD, 0x4F));
        assert_eq!(light.diff_removed, Color32::from_rgb(0xAB, 0x59, 0x59));
        assert_eq!(dark.diff_removed, Color32::from_rgb(0xE1, 0x77, 0x68));
        assert_eq!(light.diff_modified, Color32::from_rgb(0x2D, 0x7D, 0x8C));
        assert_eq!(dark.diff_modified, Color32::from_rgb(0x5D, 0x99, 0xA9));
        assert_eq!(light.error_text, Color32::from_rgb(0xA0, 0x40, 0x37));
        assert_eq!(dark.error_text, Color32::from_rgb(0xE1, 0x77, 0x68));
        assert_eq!(light.tag_bg.a(), 15);
        assert_eq!(dark.tag_bg.a(), 18);
        for p in [&light, &dark] {
            for (name, ink) in [
                ("diff_added", p.diff_added),
                ("diff_removed", p.diff_removed),
                ("diff_modified", p.diff_modified),
            ] {
                for ground in [p.bg, p.code_bg] {
                    let ratio = contrast_ratio(ink, ground);
                    assert!(
                        ratio >= INK_CONTRAST,
                        "{name} on {ground:?} scores {ratio:.2}:1"
                    );
                }
            }
        }
    }

    #[test]
    fn scrim_uses_the_overlay_alpha() {
        let light = Palette::light();
        let dark = Palette::dark();
        assert_eq!(light.scrim(), light.overlay);
        assert_eq!(dark.scrim(), dark.overlay);
        assert_eq!(light.overlay.a(), 0x47);
        assert_eq!(dark.overlay.a(), 0x9E);
    }

    #[test]
    fn derived_status_text_is_readable() {
        let light = Palette::light();
        let dark = Palette::dark();
        for p in [&light, &dark] {
            for color in [p.red, p.green, p.blue, p.coral, p.primary] {
                assert!(contrast_ratio(p.status_text(color), p.bg) >= 4.4);
            }
        }
    }

    #[test]
    fn lerp_endpoints_return_the_source_palettes() {
        let dark = Palette::dark();
        let light = Palette::light();
        let start = dark.lerp(&light, 0.0);
        let end = dark.lerp(&light, 1.0);
        assert!(start.dark);
        assert!(!end.dark);
        assert_eq!(start.bg, dark.bg);
        assert_eq!(start.fg, dark.fg);
        assert_eq!(start.primary, dark.primary);
        assert_eq!(start.tool_border, dark.tool_border);
        assert_eq!(end.bg, light.bg);
        assert_eq!(end.fg, light.fg);
        assert_eq!(end.primary, light.primary);
        assert_eq!(end.elevation_ring, light.elevation_ring);
    }

    #[test]
    fn lerp_midpoint_sits_between_the_endpoints() {
        let dark = Palette::dark();
        let light = Palette::light();
        let middle = dark.lerp(&light, 0.5);
        assert!(!middle.dark);
        assert_eq!(dark.lerp(&light, 0.25).fg, dark.fg);
        assert_eq!(dark.lerp(&light, 0.75).fg, light.fg);
        let pairs = [
            (dark.bg, light.bg, middle.bg),
            (dark.card, light.card, middle.card),
            (dark.primary, light.primary, middle.primary),
            (dark.selection, light.selection, middle.selection),
        ];
        for (low, high, value) in pairs {
            let low = low.to_srgba_unmultiplied();
            let high = high.to_srgba_unmultiplied();
            let value = value.to_srgba_unmultiplied();
            for channel in 0..4 {
                assert!(value[channel] >= low[channel].min(high[channel]));
                assert!(value[channel] <= low[channel].max(high[channel]));
            }
        }
    }

    #[test]
    fn blended_ink_stays_readable_across_brightness() {
        use crate::theme_store::{ThemeSettings, palette_for};

        for (dark, light) in [
            ("rustchamber-dark", "rustchamber-light"),
            ("nord-dark", "nord-light"),
        ] {
            let base = ThemeSettings {
                dark_theme: dark.into(),
                light_theme: light.into(),
                ..ThemeSettings::default()
            };
            for t in [0.25, 0.5, 0.75] {
                let palette = palette_for(&ThemeSettings {
                    brightness: t,
                    ..base.clone()
                });
                let ratio = contrast_ratio(palette.fg, palette.bg);
                assert!(ratio >= 4.5, "{dark} at brightness {t}: {ratio:.2}");
            }
        }
    }

    #[test]
    fn selection_tokens_match_the_reference() {
        assert_eq!(
            Palette::dark().selection,
            Color32::from_rgba_unmultiplied(0xC8, 0xC6, 0xC5, 0x2B)
        );
        assert_eq!(
            Palette::light().selection,
            Color32::from_rgba_unmultiplied(0xA9, 0x99, 0x8F, 0x2B)
        );
        assert_eq!(
            Palette::dark().selection_fg,
            Color32::from_rgb(0xDB, 0xD7, 0xCC)
        );
        assert_eq!(
            Palette::light().selection_fg,
            Color32::from_rgb(0x44, 0x44, 0x3D)
        );
    }

    #[test]
    fn selection_visuals_keep_text_legible() {
        for p in [Palette::dark(), Palette::light()] {
            let visuals = visuals(&p);
            assert_eq!(visuals.selection.bg_fill, p.selection);
            assert_eq!(visuals.selection.stroke.color, p.selection_fg);
            assert!(visuals.selection.stroke.color.a() > 0);
        }
    }

    #[test]
    fn selected_text_stays_readable_on_every_surface() {
        for p in [Palette::dark(), Palette::light()] {
            for surface in [
                p.bg,
                p.bg_soft,
                p.card,
                p.code_bg,
                p.user_bg,
                p.glass_popover_fill(),
            ] {
                let fill = p.over(surface, p.selection);
                let ratio = contrast_ratio(p.selection_fg, fill);
                assert!(
                    ratio >= 4.5,
                    "selected text on {fill:?} scores {ratio:.2}:1"
                );
            }
        }
    }

    #[test]
    fn reading_pairs_meet_wcag_aa_on_every_ground() {
        for p in [Palette::light(), Palette::dark()] {
            let grounds = [p.bg, p.card, p.subtle, p.code_bg];
            let inks = [
                ("fg", p.fg),
                ("body", p.body),
                ("fg_muted", p.fg_muted),
                ("primary_text", p.primary_text),
                ("error_text", p.error_text),
                ("warning_text", p.warning_text),
                ("info_text", p.info_text),
                ("status_error_text", p.status_error_text),
                ("status_warning_text", p.status_warning_text),
                ("status_success_text", p.status_success_text),
                ("status_info_text", p.status_info_text),
            ];
            for ground in grounds {
                for (name, ink) in inks {
                    let ratio = contrast_ratio(ink, ground);
                    assert!(
                        ratio >= INK_CONTRAST,
                        "{name} on {ground:?} scores {ratio:.2}:1 in {} mode",
                        if p.dark { "dark" } else { "light" }
                    );
                }
            }
            let on_primary = contrast_ratio(p.on_primary, p.primary);
            assert!(
                on_primary >= INK_CONTRAST,
                "on_primary scores {on_primary:.2}:1 in {} mode",
                if p.dark { "dark" } else { "light" }
            );
        }
    }

    #[test]
    fn semantic_soft_tokens_are_paired() {
        for p in [Palette::light(), Palette::dark()] {
            for (name, color) in [
                ("accent_soft", p.accent_soft),
                ("accent_border", p.accent_border),
                ("warning_soft", p.warning_soft),
                ("warning_border", p.warning_border),
                ("error_soft", p.error_soft),
                ("error_border", p.error_border),
                ("success_soft", p.success_soft),
                ("success_border", p.success_border),
                ("info_soft", p.info_soft),
                ("info_border", p.info_border),
                ("tag_bg", p.tag_bg),
                ("diff_added_bg", p.diff_added_bg),
                ("diff_added_emph", p.diff_added_emph),
                ("diff_removed_bg", p.diff_removed_bg),
                ("diff_removed_emph", p.diff_removed_emph),
            ] {
                assert!(color.a() > 0, "{name} is transparent");
            }
            assert!(p.accent_border.a() > p.accent_soft.a());
            assert!(p.diff_added_emph.a() > p.diff_added_bg.a());
            assert!(p.diff_removed_emph.a() > p.diff_removed_bg.a());
        }
    }

    #[test]
    fn primary_action_pairs_accent_with_on_primary_text() {
        for p in [Palette::light(), Palette::dark()] {
            assert!(contrast_ratio(p.on_primary, p.primary) >= INK_CONTRAST);
            assert!(contrast_ratio(p.on_primary, p.primary_hover) >= INK_CONTRAST - 0.25);
            assert!(contrast_ratio(p.on_primary, p.primary_active) >= INK_CONTRAST - 0.25);
        }
    }

    #[test]
    fn accent_override_repaints_focus_and_soft_tokens() {
        let accent = Color32::from_rgb(0xDA, 0x7C, 0x47);
        for p in [Palette::light(), Palette::dark()] {
            let p = p.with_accent(accent);
            assert_eq!(p.primary, accent);
            assert_eq!(p.border_focus, accent);
            for token in [p.accent_soft, p.accent_border, p.focus_ring] {
                let [r, g, b, _] = token.to_srgba_unmultiplied();
                assert!(r.abs_diff(accent.r()) <= 4);
                assert!(g.abs_diff(accent.g()) <= 4);
                assert!(b.abs_diff(accent.b()) <= 4);
            }
        }
    }

    #[test]
    fn popover_frame_keeps_the_glass_surface() {
        for p in [Palette::dark(), Palette::light()] {
            let frame = popover_frame_t(&p, 1.0);
            assert_eq!(
                frame.inner_margin,
                Margin {
                    left: 6,
                    right: 6,
                    top: 6,
                    bottom: 6,
                }
            );
            assert_eq!(frame.corner_radius, CornerRadius::same(radius::XL));
            assert_eq!(frame.stroke, Stroke::new(HAIRLINE, p.elevation_ring));
            assert_eq!(
                popover_frame_t(&p, 0.5).stroke.color.a(),
                (p.elevation_ring.a() as f32 * 0.5).round() as u8
            );
            assert_eq!(frame.fill, p.glass_popover_fill());
            assert_eq!(frame.fill.a(), 255);
            assert_ne!(frame.fill, p.bg);
            assert_eq!(frame.shadow.offset, [0, 8]);
            assert_eq!(frame.shadow.blur, 24);
            assert_eq!(frame.shadow.spread, 0);
            assert_eq!(frame.shadow.color, p.shadow);
            assert_eq!(popover_frame_t(&p, 0.5).fill.a(), 128);
            let base = popover_frame(&p);
            assert_eq!(base.inner_margin, frame.inner_margin);
            assert_eq!(base.fill, frame.fill);
        }
    }
}
