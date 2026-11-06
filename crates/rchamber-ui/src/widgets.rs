use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Id, Label, Layout, Margin, Pos2, Rect, Response,
    RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::icons::{self, Icon};
use crate::motion;
use crate::theme::{self, Palette};

pub const HIT_XS: f32 = 24.0;
pub const HIT_SM: f32 = 28.0;
pub const HIT_MD: f32 = 32.0;
pub const HIT_LG: f32 = 36.0;
pub const TOOLTIP_DELAY: f32 = 0.300;
pub const TOOLTIP_DELAY_FAST: f32 = 0.150;
pub const TOOLTIP_DELAY_ROW: f32 = 0.400;
pub const CHIP_HEIGHT: f32 = 28.0;
pub const CHIP_RADIUS: u8 = 8;
pub const TAB_HEIGHT: f32 = 28.0;
pub const TAB_TRACK_HEIGHT: f32 = 28.0;
pub const TAB_PILL_HEIGHT: f32 = 24.0;
pub const TAB_PILL_FORM_HEIGHT: f32 = 28.0;
pub const TAB_TRACK_PAD: f32 = 2.0;
pub const TAB_UNDERLINE: f32 = 3.0;
pub const MENU_ITEM_HEIGHT: f32 = 28.0;
pub const MENU_RADIUS: u8 = 9;
pub const INPUT_HEIGHT: f32 = 32.0;
pub const INPUT_COMPACT_HEIGHT: f32 = 28.0;
pub const TEXTAREA_MIN_HEIGHT: f32 = 82.0;
pub const TEXTAREA_FOOTER_HEIGHT: f32 = 20.0;
pub const CHECKBOX_SIZE: f32 = 14.0;
pub const CHECKBOX_ICON: f32 = 10.0;
pub const SWITCH_WIDTH: f32 = 36.0;
pub const SWITCH_HEIGHT: f32 = 20.0;
pub const SWITCH_SM_WIDTH: f32 = 28.0;
pub const SWITCH_SM_HEIGHT: f32 = 16.0;
pub const SEGMENT_HEIGHT: f32 = 28.0;
pub const SEGMENT_FORM_HEIGHT: f32 = 32.0;
pub const SELECT_HEIGHT: f32 = 36.0;
pub const SELECT_ROW_HEIGHT: f32 = 32.0;
pub const SELECT_SEARCH_ROW_HEIGHT: f32 = 40.0;
pub const SELECT_SEARCH_MAX_HEIGHT: f32 = 420.0;
pub const SLIDER_HEIGHT: f32 = 24.0;
pub const DIALOG_MAX_WIDTH: f32 = 512.0;
pub const DIALOG_PAD: f32 = 20.0;
pub const DIALOG_GAP: f32 = 16.0;
pub const DIALOG_HEADER_GAP: f32 = 8.0;
pub const DIALOG_FOOTER_GAP: f32 = 8.0;
pub const DIALOG_CLOSE_SIZE: f32 = 28.0;
pub const SKELETON_BAR_HEIGHT: f32 = 10.0;
pub const SKELETON_PULSE_SECONDS: f32 = 1.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Ghost,
    Outline,
    Icon,
    Subtle,
    Neutral,
    Destructive,
    Info,
    Chip,
    Secondary,
    Link,
    Success,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonSize {
    Xs,
    Sm,
    Md,
    Lg,
}

impl ButtonSize {
    pub fn height(self) -> f32 {
        match self {
            ButtonSize::Xs => HIT_XS,
            ButtonSize::Sm => HIT_SM,
            ButtonSize::Md => HIT_MD,
            ButtonSize::Lg => HIT_LG,
        }
    }

    pub fn padding(self) -> f32 {
        match self {
            ButtonSize::Xs => 8.0,
            ButtonSize::Sm => 10.0,
            ButtonSize::Md => 12.0,
            ButtonSize::Lg => 16.0,
        }
    }

    pub fn padding_with_icon(self) -> f32 {
        match self {
            ButtonSize::Xs => 6.0,
            ButtonSize::Sm => 8.0,
            ButtonSize::Md => 10.0,
            ButtonSize::Lg => 14.0,
        }
    }

    pub fn gap(self) -> f32 {
        match self {
            ButtonSize::Xs => 4.0,
            ButtonSize::Sm => 6.0,
            _ => 8.0,
        }
    }

    pub fn font_size(self) -> f32 {
        match self {
            ButtonSize::Xs => theme::TEXT_MICRO,
            _ => theme::TEXT_UI_LABEL,
        }
    }

    pub fn line_height(self) -> f32 {
        match self {
            ButtonSize::Xs => theme::LINE_TIGHT,
            _ => theme::LINE_LABEL,
        }
    }

    pub fn icon_size(self) -> f32 {
        16.0
    }

    pub fn radius(self) -> u8 {
        match self {
            ButtonSize::Xs => theme::radius::SM,
            ButtonSize::Lg => theme::radius::LG,
            _ => theme::radius::MD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabVariant {
    Underline,
    Pill,
}

#[derive(Clone, Debug)]
pub struct TabSpec<'a> {
    pub label: &'a str,
    pub icon: Option<Icon>,
    pub closable: bool,
}

impl<'a> TabSpec<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            icon: None,
            closable: false,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn closable(mut self) -> Self {
        self.closable = true;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabEvent {
    Selected(usize),
    Closed(usize),
    Reordered(usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchSize {
    Default,
    Sm,
}

struct ButtonColors {
    base: Color32,
    hover: Color32,
    press: Color32,
    fg: Color32,
    fg_hover: Color32,
    border: Color32,
    border_hover: Color32,
    border_width: f32,
    raised: bool,
    underline: bool,
}

fn button_colors(p: &Palette, variant: ButtonVariant) -> ButtonColors {
    let tint = |color: Color32, light: [f32; 4], dark: [u8; 4], fg: Color32| {
        let (base, hover, press, border) = if p.dark {
            (
                p.alpha(color, dark[0]),
                p.alpha(color, dark[1]),
                p.alpha(color, dark[2]),
                p.alpha(color, dark[3]),
            )
        } else {
            (
                p.mix(p.bg, color, light[0]),
                p.mix(p.bg, color, light[1]),
                p.mix(p.bg, color, light[2]),
                p.alpha(color, (light[3] * 255.0) as u8),
            )
        };
        ButtonColors {
            base,
            hover,
            press,
            fg,
            fg_hover: fg,
            border,
            border_hover: border,
            border_width: theme::BORDER,
            raised: true,
            underline: false,
        }
    };
    match variant {
        ButtonVariant::Primary => tint(
            p.primary,
            [0.10, 0.16, 0.22, 0.12],
            [41, 56, 77, 51],
            p.primary_text,
        ),
        ButtonVariant::Destructive => tint(
            p.red,
            [0.07, 0.11, 0.16, 0.09],
            [23, 36, 51, 36],
            p.error_text,
        ),
        ButtonVariant::Info => tint(
            p.blue,
            [0.04, 0.07, 0.10, 0.08],
            [18, 26, 36, 31],
            p.info_text,
        ),
        ButtonVariant::Success => tint(
            p.green,
            [0.10, 0.16, 0.22, 0.14],
            [34, 52, 70, 42],
            p.success_text,
        ),
        ButtonVariant::Neutral => ButtonColors {
            base: p.bg_soft,
            hover: p.over(p.bg_soft, p.hover),
            press: p.over(p.bg_soft, p.active),
            fg: p.fg,
            fg_hover: p.fg,
            border: p.raised_edge(),
            border_hover: p.raised_edge(),
            border_width: theme::HAIRLINE,
            raised: true,
            underline: false,
        },
        ButtonVariant::Outline => ButtonColors {
            base: if p.dark { p.card } else { p.bg },
            hover: p.over(if p.dark { p.card } else { p.bg }, p.hover),
            press: p.over(if p.dark { p.card } else { p.bg }, p.active),
            fg: p.fg,
            fg_hover: p.fg,
            border: p.raised_edge(),
            border_hover: p.raised_edge(),
            border_width: theme::HAIRLINE,
            raised: true,
            underline: false,
        },
        ButtonVariant::Secondary => ButtonColors {
            base: p.bg_soft,
            hover: p.over(p.bg_soft, p.hover),
            press: p.over(p.bg_soft, p.active),
            fg: p.fg,
            fg_hover: p.fg,
            border: Color32::TRANSPARENT,
            border_hover: Color32::TRANSPARENT,
            border_width: 0.0,
            raised: false,
            underline: false,
        },
        ButtonVariant::Chip => ButtonColors {
            base: p.wash(p.fg, 13),
            hover: p.hover,
            press: p.selection,
            fg: p.fg,
            fg_hover: p.fg,
            border: Color32::TRANSPARENT,
            border_hover: Color32::TRANSPARENT,
            border_width: 0.0,
            raised: false,
            underline: false,
        },
        ButtonVariant::Link => ButtonColors {
            base: Color32::TRANSPARENT,
            hover: Color32::TRANSPARENT,
            press: Color32::TRANSPARENT,
            fg: p.primary_text,
            fg_hover: p.primary_text,
            border: Color32::TRANSPARENT,
            border_hover: Color32::TRANSPARENT,
            border_width: 0.0,
            raised: false,
            underline: true,
        },
        ButtonVariant::Ghost => ButtonColors {
            base: Color32::TRANSPARENT,
            hover: p.hover,
            press: p.hover,
            fg: p.fg,
            fg_hover: p.fg,
            border: Color32::TRANSPARENT,
            border_hover: Color32::TRANSPARENT,
            border_width: 0.0,
            raised: false,
            underline: false,
        },
        ButtonVariant::Icon => ButtonColors {
            base: Color32::TRANSPARENT,
            hover: p.hover,
            press: p.active,
            fg: p.fg_muted,
            fg_hover: p.fg,
            border: Color32::TRANSPARENT,
            border_hover: Color32::TRANSPARENT,
            border_width: 0.0,
            raised: false,
            underline: false,
        },
        ButtonVariant::Subtle => ButtonColors {
            base: p.subtle,
            hover: p.over(p.subtle, p.hover),
            press: p.over(p.subtle, p.active),
            fg: p.fg_muted,
            fg_hover: p.fg,
            border: Color32::TRANSPARENT,
            border_hover: Color32::TRANSPARENT,
            border_width: 0.0,
            raised: false,
            underline: false,
        },
    }
}

fn dim(color: Color32) -> Color32 {
    theme::scale_alpha(color, 0.5)
}

fn paint_button_shadow(ui: &Ui, rect: Rect, corner: u8, p: &Palette) {
    let alpha = if p.dark { 70 } else { 22 };
    ui.painter().rect_filled(
        rect.translate(Vec2::new(0.0, 1.0)).expand(0.5),
        CornerRadius::same(corner.saturating_add(1)),
        Color32::from_black_alpha(alpha),
    );
}

fn paint_raised(ui: &Ui, rect: Rect, corner: u8, p: &Palette, pressed: bool, raised_edge: bool) {
    if pressed {
        ui.painter().rect_stroke(
            rect.shrink(0.25),
            CornerRadius::same(corner),
            Stroke::new(theme::HAIRLINE, p.raised_edge()),
            StrokeKind::Inside,
        );
        return;
    }
    if raised_edge {
        ui.painter().rect_stroke(
            rect.shrink(0.25),
            CornerRadius::same(corner),
            Stroke::new(theme::HAIRLINE, p.raised_edge()),
            StrokeKind::Inside,
        );
    }
    if let Some(highlight) = p.raised_highlight() {
        let inset = corner as f32 + 1.0;
        ui.painter().hline(
            (rect.left() + inset)..=(rect.right() - inset),
            rect.top() + theme::HAIRLINE,
            Stroke::new(theme::HAIRLINE, highlight),
        );
    }
}

pub fn focus_ring(_ui: &Ui, _rect: Rect, _corner: u8, _p: &Palette) {}

pub fn tooltip(response: Response, text: impl Into<egui::WidgetText>) -> Response {
    let text: egui::WidgetText = text.into();
    response.on_hover_ui(move |ui| {
        ui.set_max_width(ui.spacing().tooltip_width);
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, theme::font_meta());
        ui.add(Label::new(text));
    })
}

pub fn tooltip_now(response: Response, text: impl Into<egui::WidgetText>) -> Response {
    let text: egui::WidgetText = text.into();
    response.on_hover_ui_at_pointer(move |ui| {
        ui.set_max_width(ui.spacing().tooltip_width);
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, theme::font_meta());
        ui.add(Label::new(text));
    })
}

pub fn set_tooltip_delay(ctx: &egui::Context, delay: f32) {
    let mut style = (*ctx.global_style()).clone();
    style.interaction.tooltip_delay = delay;
    ctx.set_global_style(style);
}

pub fn tooltip_frame(p: &Palette) -> egui::Frame {
    theme::tooltip_frame(p)
}

pub fn button(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    variant: ButtonVariant,
    size: ButtonSize,
    p: &Palette,
) -> Response {
    button_ex(ui, label, icon, variant, size, true, p)
}

pub fn button_ex(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    variant: ButtonVariant,
    size: ButtonSize,
    enabled: bool,
    p: &Palette,
) -> Response {
    let font = if size == ButtonSize::Xs {
        theme::font(size.font_size())
    } else {
        theme::font_medium(size.font_size())
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let icon_only = variant == ButtonVariant::Icon && label.trim().is_empty() && icon.is_some();
    let icon_space = match (icon, icon_only) {
        (None, _) => 0.0,
        (Some(_), true) => 0.0,
        (Some(_), false) => size.icon_size() + size.gap(),
    };
    let pad = if icon.is_some() {
        size.padding_with_icon()
    } else {
        size.padding()
    };
    let width = if icon_only {
        size.height()
    } else {
        (galley.size().x + icon_space + pad * 2.0).max(size.height())
    };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, size.height()),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered() && enabled);
    let press_t = motion::Animated::from_id(response.id.with("press"))
        .with_duration(motion::FAST)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.is_pointer_button_down_on() && enabled);
    let focus_t = motion::Animated::from_id(response.id.with("focus"))
        .with_duration(motion::FAST)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.has_focus());

    let colors = button_colors(p, variant);
    let base = if enabled {
        colors.base
    } else {
        dim(colors.base)
    };
    let hover = if enabled {
        colors.hover
    } else {
        dim(colors.hover)
    };
    let press = if enabled {
        colors.press
    } else {
        dim(colors.press)
    };
    let hovered_fill = p.mix(base, hover, hover_t.max(focus_t));
    let fill = p.mix(hovered_fill, press, press_t);
    let fg = if enabled {
        p.mix(
            colors.fg,
            colors.fg_hover,
            hover_t.max(press_t).max(focus_t),
        )
    } else {
        p.disabled
    };
    let border = if enabled {
        p.mix(colors.border, colors.border_hover, hover_t.max(press_t))
    } else {
        dim(colors.border)
    };
    let corner = size.radius();
    if colors.raised {
        paint_button_shadow(ui, rect, corner, p);
    }
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(corner), fill);
    }
    if colors.raised {
        paint_raised(
            ui,
            rect,
            corner,
            p,
            press_t > 0.5,
            colors.border_width <= theme::HAIRLINE,
        );
    }
    if colors.border_width > theme::HAIRLINE && border.a() > 0 {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(corner),
            Stroke::new(colors.border_width, border),
            StrokeKind::Inside,
        );
    }
    let mut x = if icon_only {
        rect.center().x
    } else {
        rect.left() + pad
    };
    if let Some(icon) = icon {
        let icon_center_x = if icon_only {
            x
        } else {
            x + size.icon_size() / 2.0
        };
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(
                Pos2::new(icon_center_x, rect.center().y),
                Vec2::splat(size.icon_size()),
            ),
            fg,
        );
        x += icon_space;
    }
    let text_pos = Pos2::new(x, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_pos, galley.clone(), fg);
    if colors.underline && hover_t > 0.01 {
        let y = text_pos.y + galley.size().y + 1.0;
        ui.painter().hline(
            x..=(x + galley.size().x),
            y,
            Stroke::new(theme::BORDER, theme::scale_alpha(fg, hover_t)),
        );
    }
    response
}

pub fn button_tip(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    variant: ButtonVariant,
    size: ButtonSize,
    tip: &str,
    p: &Palette,
) -> Response {
    let response = button(ui, label, icon, variant, size, p);
    tooltip(response, tip)
}

pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, tint: Color32) -> Response {
    let hit = (size + 8.0).max(HIT_XS);
    icon_button_hit(ui, icon, size, hit, tint)
}

pub fn pointer(ui: &Ui, response: &Response) {
    if response.hovered() && ui.is_enabled() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

pub fn icon_button_hit(ui: &mut Ui, icon: Icon, size: f32, hit: f32, tint: Color32) -> Response {
    let p = theme::current_palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(hit), Sense::click());
    let enabled = ui.is_enabled();
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered() && enabled);
    let press_t = motion::Animated::from_id(response.id.with("press"))
        .with_duration(motion::FAST)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.is_pointer_button_down_on() && enabled);
    let focus_t = motion::Animated::from_id(response.id.with("focus"))
        .with_duration(motion::FAST)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.has_focus());
    let fill_t = hover_t.max(press_t).max(focus_t);
    if fill_t > 0.001 {
        let color = if press_t > hover_t { p.active } else { p.hover };
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            theme::scale_alpha(color, fill_t),
        );
    }
    let fg = if !enabled {
        p.disabled
    } else {
        p.mix(tint, p.fg, hover_t.max(focus_t))
    };
    let inner = Rect::from_center_size(rect.center(), Vec2::splat(size));
    icons::paint(ui, icon, inner, fg);
    pointer(ui, &response);
    response
}

pub fn subtle_icon_button(ui: &mut Ui, icon: Icon, size: f32, p: &Palette) -> Response {
    icon_button(ui, icon, size, p.fg_muted)
}

pub fn primary_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Primary, ButtonSize::Md, p)
}

pub fn destructive_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(
        ui,
        label,
        icon,
        ButtonVariant::Destructive,
        ButtonSize::Md,
        p,
    )
}

pub fn info_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Info, ButtonSize::Md, p)
}

pub fn neutral_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Neutral, ButtonSize::Md, p)
}

pub fn secondary_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Secondary, ButtonSize::Md, p)
}

pub fn chip_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Chip, ButtonSize::Md, p)
}

pub fn link_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Link, ButtonSize::Md, p)
}

pub fn ghost_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Ghost, ButtonSize::Sm, p)
}

pub fn outline_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Outline, ButtonSize::Sm, p)
}

pub fn subtle_button(ui: &mut Ui, label: &str, icon: Option<Icon>, p: &Palette) -> Response {
    button(ui, label, icon, ButtonVariant::Subtle, ButtonSize::Sm, p)
}

pub fn chip(ui: &mut Ui, label: &str, icon: Option<Icon>, selected: bool, p: &Palette) -> Response {
    chip_ex(ui, label, icon, selected, true, p)
}

pub fn chip_ex(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    selected: bool,
    enabled: bool,
    p: &Palette,
) -> Response {
    let font = theme::font_medium(theme::TEXT_UI_LABEL);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let icon_space = if icon.is_some() { 14.0 + 6.0 } else { 0.0 };
    let width = galley.size().x + icon_space + 20.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, CHIP_HEIGHT),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered() && enabled);
    let focus_t = motion::Animated::from_id(response.id.with("focus"))
        .with_duration(motion::FAST)
        .bool(ui.ctx(), response.has_focus());
    let rest = p.wash(p.fg, 13);
    let hovered = p.over(rest, p.hover);
    let base = p.mix(rest, hovered, hover_t.max(focus_t));
    let fill = if selected { p.selection } else { base };
    let fill = if enabled { fill } else { dim(fill) };
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(CHIP_RADIUS), fill);
    }
    let text = if !enabled {
        p.disabled
    } else if selected {
        p.selection_fg
    } else {
        p.fg
    };
    let mut x = rect.left() + 10.0;
    if let Some(icon) = icon {
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(Pos2::new(x + 7.0, rect.center().y), Vec2::splat(14.0)),
            text,
        );
        x += icon_space;
    }
    ui.painter().galley(
        Pos2::new(x, rect.center().y - galley.size().y / 2.0),
        galley,
        text,
    );
    response
}

pub fn pill(ui: &mut Ui, label: &str, fg: Color32, bg: Color32, icon: Option<Icon>) -> Response {
    let p = theme::current_palette(ui.ctx());
    let font = theme::font_medium(theme::TEXT_META);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let icon_space = if icon.is_some() { 14.0 + 4.0 } else { 0.0 };
    let width = galley.size().x + icon_space + 20.0;
    let height = CHIP_HEIGHT;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::PILL), bg);
    if hover_t > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::PILL),
            p.fade(p.hover, hover_t),
        );
    }
    let mut x = rect.left() + 10.0;
    if let Some(icon) = icon {
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(Pos2::new(x + 7.0, rect.center().y), Vec2::splat(14.0)),
            fg,
        );
        x += icon_space;
    }
    ui.painter().galley(
        Pos2::new(x, rect.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );
    response
}

pub fn tab(ui: &mut Ui, label: &str, selected: bool, p: &Palette) -> Response {
    let font = theme::font(theme::TEXT_MICRO);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let width = galley.size().x + 24.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, TAB_HEIGHT), Sense::click());
    let fg = if selected || response.hovered() {
        p.fg
    } else {
        p.fg_muted
    };
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.bottom() - 1.0),
                Vec2::new(rect.width(), TAB_UNDERLINE),
            ),
            CornerRadius {
                nw: 2,
                ne: 2,
                sw: 0,
                se: 0,
            },
            p.primary,
        );
    }
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        fg,
    );
    response
}

pub fn tab_pill(ui: &mut Ui, label: &str, selected: bool, p: &Palette) -> Response {
    let font = theme::font_medium(theme::TEXT_MARKDOWN);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let (outer, response) = ui.allocate_exact_size(
        Vec2::new(
            galley.size().x + 24.0 + TAB_TRACK_PAD * 2.0,
            TAB_TRACK_HEIGHT,
        ),
        Sense::click(),
    );
    let rect = Rect::from_min_size(
        Pos2::new(outer.left() + TAB_TRACK_PAD, outer.top() + TAB_TRACK_PAD),
        Vec2::new(outer.width() - TAB_TRACK_PAD * 2.0, TAB_PILL_HEIGHT),
    );
    if selected {
        paint_pill_indicator(ui, rect, p);
    }
    let fg = if selected || response.hovered() {
        p.fg
    } else {
        p.fg_muted
    };
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        fg,
    );
    response
}

pub fn tabs(ui: &mut Ui, labels: &[&str], selected: &mut usize, p: &Palette) -> bool {
    tabs_ex(ui, labels, selected, TabVariant::Underline, p)
}

pub fn tabs_pill(ui: &mut Ui, labels: &[&str], selected: &mut usize, p: &Palette) -> bool {
    tabs_ex(ui, labels, selected, TabVariant::Pill, p)
}

pub fn tabs_ex(
    ui: &mut Ui,
    labels: &[&str],
    selected: &mut usize,
    variant: TabVariant,
    p: &Palette,
) -> bool {
    let specs: Vec<TabSpec> = labels.iter().map(|label| TabSpec::new(label)).collect();
    let id = ui.next_auto_id();
    let height = match variant {
        TabVariant::Pill => TAB_TRACK_HEIGHT,
        TabVariant::Underline => TAB_HEIGHT,
    };
    match tabs_strip_at(ui, id, &specs, *selected, variant, height, p) {
        Some(TabEvent::Selected(index)) if index != *selected => {
            *selected = index;
            true
        }
        _ => false,
    }
}

pub fn tabs_strip(
    ui: &mut Ui,
    tabs: &[TabSpec],
    active: usize,
    variant: TabVariant,
    p: &Palette,
) -> Option<TabEvent> {
    let id = ui.next_auto_id();
    let height = match variant {
        TabVariant::Pill => TAB_TRACK_HEIGHT,
        TabVariant::Underline => TAB_HEIGHT,
    };
    tabs_strip_at(ui, id, tabs, active, variant, height, p)
}

pub fn tabs_strip_at(
    ui: &mut Ui,
    id: Id,
    tabs: &[TabSpec],
    active: usize,
    variant: TabVariant,
    height: f32,
    p: &Palette,
) -> Option<TabEvent> {
    if tabs.is_empty() {
        return None;
    }
    let pill = variant == TabVariant::Pill;
    let button_height = if pill {
        height - TAB_TRACK_PAD * 2.0
    } else {
        height
    };
    let font = if pill {
        theme::font_medium(theme::TEXT_MARKDOWN)
    } else {
        theme::font(theme::TEXT_MICRO)
    };
    let side_pad = 12.0;
    let label_gap = 6.0;
    let icon_box = 16.0;
    let close_width = 20.0;

    let mut widths = Vec::with_capacity(tabs.len());
    for spec in tabs {
        let galley =
            ui.painter()
                .layout_no_wrap(spec.label.to_string(), font.clone(), Color32::PLACEHOLDER);
        let mut width = galley.size().x + side_pad * 2.0;
        if spec.icon.is_some() {
            width += icon_box + label_gap;
        }
        if spec.closable {
            width += close_width;
        }
        widths.push(width.max(button_height));
    }
    let gap = if pill { 2.0 } else { 0.0 };
    let track_pad = if pill { TAB_TRACK_PAD } else { 0.0 };
    let total: f32 = widths.iter().sum::<f32>() + gap * (tabs.len() as f32 - 1.0);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(total + track_pad * 2.0, height), Sense::hover());

    if pill {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::LG),
            p.wash(p.fg, 10),
        );
        ui.painter().hline(
            rect.left() + theme::radius::LG as f32..=rect.right() - theme::radius::LG as f32,
            rect.top() + theme::HAIRLINE,
            Stroke::new(
                theme::HAIRLINE,
                Color32::from_black_alpha(if p.dark { 100 } else { 12 }),
            ),
        );
    }

    let mut x = rect.left() + track_pad;
    let mut tab_rects = Vec::with_capacity(tabs.len());
    for width in &widths {
        tab_rects.push(Rect::from_min_size(
            Pos2::new(x, rect.top() + track_pad),
            Vec2::new(*width, button_height),
        ));
        x += width + gap;
    }

    let mut hovered = vec![false; tabs.len()];
    let mut focused = vec![false; tabs.len()];
    let mut close_hovered = vec![false; tabs.len()];
    let mut pressed: Option<usize> = None;
    let mut dragged: Option<usize> = None;
    let mut events: Vec<TabEvent> = Vec::new();
    for (index, tab_rect) in tab_rects.iter().enumerate() {
        let tab_response = ui.interact(*tab_rect, id.with(("tab", index)), Sense::click_and_drag());
        hovered[index] = tab_response.hovered();
        focused[index] = tab_response.has_focus();
        if tab_response.is_pointer_button_down_on() && !tab_response.dragged() {
            pressed = Some(index);
        }
        if tab_response.dragged() {
            dragged = Some(index);
        }
        if tab_response.clicked() {
            events.push(TabEvent::Selected(index));
        }
        if tabs[index].closable {
            let close_rect = Rect::from_center_size(
                Pos2::new(tab_rect.right() - 12.0, tab_rect.center().y),
                Vec2::splat(close_width),
            );
            let close_response = ui.interact(close_rect, id.with(("close", index)), Sense::click());
            close_hovered[index] = close_response.hovered();
            if close_response.clicked() {
                events.push(TabEvent::Closed(index));
            }
        }
        if tab_response.drag_stopped()
            && let Some(position) = ui.ctx().pointer_interact_pos()
            && let Some(over) = tab_rects
                .iter()
                .position(|candidate| candidate.contains(position))
            && over != index
        {
            events.push(TabEvent::Reordered(index, over));
        }
    }

    let target = active.min(tab_rects.len() - 1);
    let nudge = match pressed {
        Some(pressed_index) if pressed_index != target => {
            if pressed_index > target {
                motion::TAB_NUDGE
            } else {
                -motion::TAB_NUDGE
            }
        }
        _ => 0.0,
    };
    let target_rect = tab_rects[target];
    let indicator_x = motion::Motion::from_id(id.with("indicator-x"))
        .with_duration(motion::TAB)
        .with_easing(motion::Easing::Decelerate)
        .value(ui.ctx(), target_rect.left() + nudge);
    let indicator_width = motion::Motion::from_id(id.with("indicator-w"))
        .with_duration(motion::TAB_SIZE)
        .with_easing(motion::Easing::Decelerate)
        .value(ui.ctx(), target_rect.width());
    let indicator_height = motion::Motion::from_id(id.with("indicator-h"))
        .with_duration(motion::TAB_SIZE)
        .with_easing(motion::Easing::Decelerate)
        .value(ui.ctx(), target_rect.height());

    if pill {
        paint_pill_indicator(
            ui,
            Rect::from_min_size(
                Pos2::new(indicator_x, target_rect.top()),
                Vec2::new(indicator_width, indicator_height),
            ),
            p,
        );
    } else {
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(indicator_x, rect.bottom() - 1.0),
                Vec2::new(indicator_width, TAB_UNDERLINE),
            ),
            CornerRadius {
                nw: 2,
                ne: 2,
                sw: 0,
                se: 0,
            },
            p.primary,
        );
    }

    for (index, spec) in tabs.iter().enumerate() {
        let tab_rect = tab_rects[index];
        if focused[index] {
            ui.painter()
                .rect_filled(tab_rect, CornerRadius::same(theme::radius::MD), p.hover);
        }
        let is_active = index == active;
        let opacity = if dragged == Some(index) { 0.5 } else { 1.0 };
        let fg = if is_active || hovered[index] {
            p.fg
        } else {
            p.fg_muted
        };
        let galley =
            ui.painter()
                .layout_no_wrap(spec.label.to_string(), font.clone(), Color32::PLACEHOLDER);
        let mut content_width = galley.size().x;
        if spec.icon.is_some() {
            content_width += icon_box + label_gap;
        }
        if spec.closable {
            content_width += close_width;
        }
        let mut content_x = tab_rect.center().x - content_width / 2.0;
        let show_close = spec.closable
            && (if pill {
                hovered[index] || close_hovered[index]
            } else {
                is_active || hovered[index] || close_hovered[index]
            });
        if let Some(icon) = spec.icon {
            let icon_rect = Rect::from_center_size(
                Pos2::new(content_x + icon_box / 2.0, tab_rect.center().y),
                Vec2::splat(icon_box),
            );
            if pill && show_close {
                icons::paint(
                    ui,
                    Icon::Close,
                    Rect::from_center_size(icon_rect.center(), Vec2::splat(14.0)),
                    theme::scale_alpha(p.fg_muted, opacity),
                );
            } else {
                let tint = if !pill && is_active { p.primary } else { fg };
                icons::paint(ui, icon, icon_rect, theme::scale_alpha(tint, opacity));
            }
            content_x += icon_box + label_gap;
        }
        ui.painter().galley(
            Pos2::new(content_x, tab_rect.center().y - galley.size().y / 2.0),
            galley,
            theme::scale_alpha(fg, opacity),
        );
        if spec.closable && (!pill || spec.icon.is_none()) && show_close {
            let close_rect = Rect::from_center_size(
                Pos2::new(tab_rect.right() - 12.0, tab_rect.center().y),
                Vec2::splat(close_width),
            );
            icons::paint(
                ui,
                Icon::Close,
                Rect::from_center_size(close_rect.center(), Vec2::splat(12.0)),
                if close_hovered[index] || hovered[index] {
                    theme::scale_alpha(p.fg, opacity)
                } else {
                    theme::scale_alpha(p.fg_muted, opacity)
                },
            );
        }
    }

    events.into_iter().next()
}

fn paint_pill_indicator(ui: &Ui, rect: Rect, p: &Palette) {
    ui.painter().rect_filled(
        rect.translate(Vec2::new(0.0, 1.0)).expand(0.5),
        CornerRadius::same(theme::radius::MD + 1),
        Color32::from_black_alpha(if p.dark { 80 } else { 18 }),
    );
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::radius::MD),
        if p.dark { p.card } else { p.bg },
    );
    ui.painter().rect_stroke(
        rect.shrink(0.25),
        CornerRadius::same(theme::radius::MD),
        Stroke::new(theme::HAIRLINE, p.raised_edge()),
        StrokeKind::Inside,
    );
    if let Some(highlight) = p.raised_highlight() {
        ui.painter().hline(
            rect.left() + theme::radius::MD as f32..=rect.right() - theme::radius::MD as f32,
            rect.top() + theme::HAIRLINE,
            Stroke::new(theme::HAIRLINE, highlight),
        );
    }
}

pub fn segmented(ui: &mut Ui, labels: &[&str], selected: &mut usize, p: &Palette) -> bool {
    segmented_ex(ui, labels, selected, SEGMENT_HEIGHT, p)
}

pub fn segmented_ex(
    ui: &mut Ui,
    labels: &[&str],
    selected: &mut usize,
    height: f32,
    p: &Palette,
) -> bool {
    segmented_impl(ui, labels, selected, height, None, p)
}

pub fn segmented_fixed(
    ui: &mut Ui,
    labels: &[&str],
    selected: &mut usize,
    height: f32,
    width: f32,
    p: &Palette,
) -> bool {
    segmented_impl(ui, labels, selected, height, Some(width), p)
}

fn segmented_impl(
    ui: &mut Ui,
    labels: &[&str],
    selected: &mut usize,
    height: f32,
    fixed_width: Option<f32>,
    p: &Palette,
) -> bool {
    if labels.is_empty() {
        return false;
    }
    let id = ui.next_auto_id();
    let button_height = height - TAB_TRACK_PAD * 2.0;
    let font = theme::font_medium(theme::TEXT_MARKDOWN);
    let side_pad = 12.0;
    let widths: Vec<f32> = if let Some(width) = fixed_width {
        let inner =
            (width - TAB_TRACK_PAD * 2.0 - 2.0 * (labels.len().saturating_sub(1)) as f32).max(0.0);
        vec![inner / labels.len() as f32; labels.len()]
    } else {
        labels
            .iter()
            .map(|label| {
                let galley = ui.painter().layout_no_wrap(
                    label.to_string(),
                    font.clone(),
                    Color32::PLACEHOLDER,
                );
                (galley.size().x + side_pad * 2.0).max(button_height)
            })
            .collect()
    };
    let total = fixed_width.unwrap_or_else(|| {
        widths.iter().sum::<f32>() + 2.0 * (labels.len() as f32 - 1.0) + TAB_TRACK_PAD * 2.0
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total, height), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::radius::LG),
        p.wash(p.fg, 10),
    );
    ui.painter().hline(
        rect.left() + theme::radius::LG as f32..=rect.right() - theme::radius::LG as f32,
        rect.top() + theme::HAIRLINE,
        Stroke::new(
            theme::HAIRLINE,
            Color32::from_black_alpha(if p.dark { 100 } else { 12 }),
        ),
    );
    let mut x = rect.left() + TAB_TRACK_PAD;
    let mut rects = Vec::with_capacity(labels.len());
    for width in &widths {
        rects.push(Rect::from_min_size(
            Pos2::new(x, rect.top() + TAB_TRACK_PAD),
            Vec2::new(*width, button_height),
        ));
        x += width + 2.0;
    }
    let target = (*selected).min(labels.len() - 1);
    let target_rect = rects[target];
    let indicator_x = motion::Motion::from_id(id.with("seg-x"))
        .with_duration(motion::TAB)
        .with_easing(motion::Easing::Decelerate)
        .value(ui.ctx(), target_rect.left());
    let indicator_width = motion::Motion::from_id(id.with("seg-w"))
        .with_duration(motion::TAB_SIZE)
        .with_easing(motion::Easing::Decelerate)
        .value(ui.ctx(), target_rect.width());
    paint_pill_indicator(
        ui,
        Rect::from_min_size(
            Pos2::new(indicator_x, target_rect.top()),
            Vec2::new(indicator_width, target_rect.height()),
        ),
        p,
    );
    let mut changed = false;
    for (index, label) in labels.iter().enumerate() {
        let response = ui.interact(rects[index], id.with(("seg", index)), Sense::click());
        if response.clicked() && index != *selected {
            *selected = index;
            changed = true;
        }
        let galley =
            ui.painter()
                .layout_no_wrap((*label).to_string(), font.clone(), Color32::PLACEHOLDER);
        let color = if index == target || response.hovered() {
            p.fg
        } else {
            p.fg_muted
        };
        ui.painter().galley(
            Pos2::new(
                rects[index].center().x - galley.size().x / 2.0,
                rects[index].center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );
    }
    changed
}

fn ellipsis_galley(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap.max_width = width.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

pub fn select(
    ui: &mut Ui,
    selected: &mut String,
    options: &[(String, String)],
    placeholder: &str,
    p: &Palette,
) -> bool {
    select_impl(ui, selected, options, placeholder, None, p)
}

pub fn searchable_select(
    ui: &mut Ui,
    selected: &mut String,
    options: &[(String, String)],
    placeholder: &str,
    search_hint: &str,
    p: &Palette,
) -> bool {
    select_impl(ui, selected, options, placeholder, Some(search_hint), p)
}

fn select_impl(
    ui: &mut Ui,
    selected: &mut String,
    options: &[(String, String)],
    placeholder: &str,
    search_hint: Option<&str>,
    p: &Palette,
) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, SELECT_HEIGHT), Sense::click());
    let id = response.id;
    let popup_id = id.with("popover");
    let query_id = popup_id.with("query");
    let search_id = popup_id.with("search");
    let mut open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(popup_id))
        .unwrap_or(false);
    let _was_open = open;
    let mut just_opened = false;
    if response.clicked() {
        open = !open;
        if open {
            ui.ctx()
                .data_mut(|data| data.insert_temp(query_id, String::new()));
            just_opened = true;
        }
    }
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered());
    let mut fill = p.field_fill();
    if hover_t > 0.001 {
        fill = p.over(fill, p.fade(p.hover, hover_t));
    }
    let corner = CornerRadius::same(theme::radius::MD);
    ui.painter().rect_filled(rect, corner, fill);
    let (highlight_width, highlight) = p.field_highlight();
    ui.painter().hline(
        (rect.left() + theme::radius::MD as f32)..=(rect.right() - theme::radius::MD as f32),
        rect.top() + highlight_width * 0.5,
        Stroke::new(highlight_width, highlight),
    );
    let (edge_width, edge) = if response.has_focus() {
        (theme::BORDER, p.field_edge_focus())
    } else {
        (theme::HAIRLINE, p.field_edge())
    };
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(edge_width, edge),
        StrokeKind::Inside,
    );
    let current = options.iter().find(|(option_id, _)| option_id == selected);
    let (text, color) = match current {
        Some((_, label)) => (label.as_str(), p.fg),
        None => (placeholder, p.fg_muted),
    };
    let font = theme::font(theme::TEXT_UI_LABEL);
    let galley = ellipsis_galley(ui, text, font, color, rect.width() - 44.0);
    ui.painter().galley(
        Pos2::new(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    icons::paint(
        ui,
        Icon::ChevronDown,
        Rect::from_center_size(
            Pos2::new(rect.right() - 16.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        if response.hovered() { p.fg } else { p.fg_muted },
    );
    pointer(ui, &response);
    let popup_width = width.max(240.0);
    let searchable = search_hint.is_some();
    let row_height = if searchable {
        SELECT_SEARCH_ROW_HEIGHT
    } else {
        SELECT_ROW_HEIGHT
    };
    let max_height = if searchable {
        SELECT_SEARCH_MAX_HEIGHT
            .min((ui.ctx().content_rect().height() * 0.46).clamp(220.0, SELECT_SEARCH_MAX_HEIGHT))
    } else {
        320.0
    };
    let mut chosen: Option<String> = None;
    let mut query = ui
        .ctx()
        .data(|data| data.get_temp::<String>(query_id))
        .unwrap_or_default();
    egui::Popup::from_response(&response)
        .id(popup_id)
        .open_bool(&mut open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .gap(4.0)
        .width(popup_width)
        .frame(theme::popover_frame(p))
        .show(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let body_width = (popup_width - 12.0).max(1.0);
            ui.set_width(body_width);
            if searchable {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    let field_width = (body_width - 12.0).max(1.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(field_width, INPUT_HEIGHT),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_width(field_width);
                            search_field(
                                ui,
                                &mut query,
                                search_hint.unwrap_or_default(),
                                p,
                                search_id,
                            );
                        },
                    );
                });
                if just_opened {
                    ui.memory_mut(|memory| memory.request_focus(search_id));
                }
                ui.add_space(6.0);
            } else {
                ui.add_space(4.0);
            }
            let needle = query.trim().to_lowercase();
            let filtered: Vec<&(String, String)> = options
                .iter()
                .filter(|(_, label)| needle.is_empty() || label.to_lowercase().contains(&needle))
                .collect();
            if filtered.is_empty() {
                let (empty, _) =
                    ui.allocate_exact_size(Vec2::new(body_width, 52.0), Sense::hover());
                ui.painter().text(
                    empty.center(),
                    egui::Align2::CENTER_CENTER,
                    "No matches",
                    theme::font(theme::TEXT_META),
                    p.fg_muted,
                );
            } else {
                let mut scroll = ScrollArea::vertical()
                    .id_salt(popup_id.with("list"))
                    .max_height(max_height)
                    .auto_shrink([false, true]);
                if just_opened
                    && let Some(index) = filtered
                        .iter()
                        .position(|(option_id, _)| option_id == selected)
                {
                    scroll = scroll.vertical_scroll_offset(
                        (index as f32 * row_height - row_height * 3.0).max(0.0),
                    );
                }
                scroll.show(ui, |ui| {
                    ui.set_width(body_width);
                    for (option_id, label) in filtered {
                        let (row, row_response) = ui
                            .allocate_exact_size(Vec2::new(body_width, row_height), Sense::click());
                        let active = option_id == selected;
                        if active {
                            ui.painter().rect_filled(
                                row,
                                CornerRadius::same(theme::radius::MD),
                                p.selection,
                            );
                        } else if row_response.hovered() {
                            ui.painter().rect_filled(
                                row,
                                CornerRadius::same(theme::radius::MD),
                                p.hover,
                            );
                        }
                        let text_color = if active { p.selection_fg } else { p.fg };
                        let text_width = row.width() - 36.0;
                        let galley = ellipsis_galley(
                            ui,
                            label,
                            theme::font(theme::TEXT_UI_LABEL),
                            text_color,
                            text_width,
                        );
                        ui.painter().galley(
                            Pos2::new(row.left() + 12.0, row.center().y - galley.size().y / 2.0),
                            galley,
                            text_color,
                        );
                        if active {
                            icons::paint(
                                ui,
                                Icon::Check,
                                Rect::from_center_size(
                                    Pos2::new(row.right() - 18.0, row.center().y),
                                    Vec2::splat(14.0),
                                ),
                                p.selection_fg,
                            );
                        }
                        pointer(ui, &row_response);
                        if row_response.clicked() {
                            chosen = Some(option_id.clone());
                        }
                    }
                });
            }
            ui.add_space(6.0);
        });
    ui.ctx().data_mut(|data| data.insert_temp(query_id, query));
    if let Some(option) = chosen {
        if *selected != option {
            *selected = option;
            ui.ctx().data_mut(|data| data.insert_temp(popup_id, false));
            return true;
        }
        open = false;
    }
    ui.ctx().data_mut(|data| data.insert_temp(popup_id, open));
    false
}

pub fn slider(ui: &mut Ui, value: &mut f32, min: f32, max: f32, p: &Palette) -> Response {
    let width = ui.available_width();
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(width, SLIDER_HEIGHT), Sense::click_and_drag());
    let track = Rect::from_center_size(
        rect.center(),
        Vec2::new((rect.width() - 12.0).max(1.0), 4.0),
    );
    if (response.dragged() || response.clicked())
        && let Some(position) = ui.ctx().pointer_interact_pos()
    {
        let t = ((position.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let next = min + t * (max - min);
        if (next - *value).abs() > 0.0001 {
            *value = next;
            response.mark_changed();
        }
    }
    let current = (*value).clamp(min, max);
    let t = ((current - min) / (max - min).max(f32::EPSILON)).clamp(0.0, 1.0);
    let knob = Pos2::new(track.left() + track.width() * t, track.center().y);
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered() || response.dragged());
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), p.wash(p.fg, 18));
    if knob.x > track.left() {
        ui.painter().rect_filled(
            Rect::from_min_max(track.min, Pos2::new(knob.x, track.bottom())),
            CornerRadius::same(2),
            p.primary,
        );
    }
    let radius = 6.5 + hover_t;
    ui.painter().circle_filled(knob, radius + 1.5, p.bg);
    ui.painter().circle_filled(knob, radius, p.primary);
    ui.painter()
        .circle_filled(knob, radius * 0.36, p.on_primary);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn switch(ui: &mut Ui, on: &mut bool, p: &Palette) -> Response {
    switch_ex(ui, on, SwitchSize::Default, true, p)
}

pub fn switch_ex(
    ui: &mut Ui,
    on: &mut bool,
    size: SwitchSize,
    enabled: bool,
    p: &Palette,
) -> Response {
    let (width, height) = match size {
        SwitchSize::Default => (SWITCH_WIDTH, SWITCH_HEIGHT),
        SwitchSize::Sm => (SWITCH_SM_WIDTH, SWITCH_SM_HEIGHT),
    };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, height),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    if enabled && response.clicked() {
        *on = !*on;
    }
    let thumb_size = height - 4.0;
    let travel = width - 4.0 - thumb_size;
    let t = motion::Animated::from_id(response.id.with("on"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::Standard)
        .bool(ui.ctx(), *on);
    let track = if enabled {
        p.mix(p.border, p.primary, t)
    } else {
        dim(p.mix(p.border, p.primary, t))
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same((height * 0.5) as u8), track);
    if response.has_focus() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((height * 0.5) as u8),
            p.fade(p.hover, 0.6),
        );
    }
    let thumb_center = Pos2::new(
        rect.left() + 2.0 + thumb_size / 2.0 + travel * t,
        rect.center().y,
    );
    let thumb_color = if !enabled {
        p.disabled
    } else {
        p.mix(p.fg_muted, p.on_primary, t)
    };
    ui.painter()
        .circle_filled(thumb_center, thumb_size / 2.0, thumb_color);
    response
}

pub fn checkbox(ui: &mut Ui, checked: &mut bool, p: &Palette) -> Response {
    checkbox_ex(ui, checked, false, true, p)
}

pub fn checkbox_ex(
    ui: &mut Ui,
    checked: &mut bool,
    indeterminate: bool,
    enabled: bool,
    p: &Palette,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(CHECKBOX_SIZE),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    if enabled && response.clicked() {
        *checked = !*checked;
    }
    let on = *checked || indeterminate;
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(0.2)
        .with_easing(motion::Easing::EaseOut)
        .bool(
            ui.ctx(),
            (response.hovered() || response.has_focus()) && enabled,
        );
    let border = if on {
        p.mix(p.border, p.fg, 0.6)
    } else {
        p.mix(p.border, p.fg, 0.4)
    };
    let border = if enabled { border } else { dim(border) };
    if hover_t > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::XS),
            p.fade(p.hover, hover_t),
        );
    }
    ui.painter().rect_stroke(
        rect.shrink(0.5),
        CornerRadius::same(theme::radius::XS),
        Stroke::new(theme::BORDER, border),
        StrokeKind::Inside,
    );
    if on {
        let color = if enabled { p.fg } else { p.disabled };
        if indeterminate {
            let bar = Rect::from_center_size(rect.center(), Vec2::new(CHECKBOX_ICON, 2.0));
            ui.painter().rect_filled(bar, CornerRadius::same(1), color);
        } else {
            icons::paint(
                ui,
                Icon::Check,
                Rect::from_center_size(rect.center(), Vec2::splat(CHECKBOX_ICON)),
                color,
            );
        }
    }
    response
}

pub fn status_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.0, color);
}

pub fn spinner(ui: &mut Ui, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_spinner(ui, rect.center(), size, color);
}

pub fn paint_spinner(ui: &Ui, center: Pos2, size: f32, color: Color32) {
    motion::paint_spinner(ui, center, size, color);
}

pub fn busy_dots(ui: &mut Ui, p: &Palette) -> Response {
    busy_dots_sized(ui, theme::font(theme::TEXT_MARKDOWN), p.body)
}

pub fn busy_dots_sized(ui: &mut Ui, font: FontId, color: Color32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap("...".to_string(), font.clone(), Color32::PLACEHOLDER);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    let phase = motion::busy_dots(ui.ctx());
    let mut x = rect.left();
    for shown in phase {
        let dot = ui.painter().layout_no_wrap(
            ".".to_string(),
            font.clone(),
            if shown { color } else { Color32::TRANSPARENT },
        );
        let width = dot.size().x;
        ui.painter().galley(
            Pos2::new(x, rect.top()),
            dot,
            if shown { color } else { Color32::TRANSPARENT },
        );
        x += width;
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(1000));
    response
}

pub fn busy_label(ui: &mut Ui, label: &str, p: &Palette) -> Response {
    let font = theme::font(theme::TEXT_MARKDOWN);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), p.body);
    let dots = ui
        .painter()
        .layout_no_wrap(" ...".to_string(), font.clone(), Color32::PLACEHOLDER);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + dots.size().x, galley.size().y),
        Sense::hover(),
    );
    ui.painter().galley(rect.min, galley.clone(), p.body);
    let phase = motion::busy_dots(ui.ctx());
    let mut x = rect.left() + galley.size().x + dots.size().x / 3.0;
    for shown in phase {
        let dot = ui.painter().layout_no_wrap(
            ".".to_string(),
            font.clone(),
            if shown { p.body } else { Color32::TRANSPARENT },
        );
        let width = dot.size().x;
        ui.painter().galley(
            Pos2::new(x, rect.top()),
            dot,
            if shown { p.body } else { Color32::TRANSPARENT },
        );
        x += width;
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(1000));
    response
}

pub fn skeleton_bar(ui: &mut Ui, width: f32, p: &Palette) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, SKELETON_BAR_HEIGHT), Sense::hover());
    let pulse = motion::pulse(ui.ctx(), response.id, SKELETON_PULSE_SECONDS);
    let fill = p.over(p.subtle, p.fade(p.hover, pulse));
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::XS), fill);
    if !motion::reduced_motion(ui.ctx()) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    }
    response
}

pub fn refresh_action(ui: &mut Ui, spinning: bool, size: f32, p: &Palette) -> Response {
    let icon = if spinning { Icon::Clock } else { Icon::Refresh };
    let response = icon_button(ui, icon, size, p.fg_muted);
    if spinning {
        let center = response.rect.center();
        motion::paint_spinner(ui, center, size + 4.0, p.fg_muted);
    }
    response
}

pub fn kbd(ui: &mut Ui, keys: &str, p: &Palette) {
    let font = theme::font_medium(11.0);
    let galley = ui
        .painter()
        .layout_no_wrap(keys.to_string(), font, p.fg_muted);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(galley.size().x + 10.0, 20.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::XS), p.hover);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::radius::XS),
        Stroke::new(theme::HAIRLINE, p.edge_muted()),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        p.fg_muted,
    );
}

pub fn section_label(ui: &mut Ui, text: &str, p: &Palette) {
    ui.label(
        RichText::new(text)
            .font(theme::font_medium(theme::TEXT_META))
            .color(p.fg_muted),
    );
}

pub fn input_frame(p: &Palette, focused: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(p.field_fill())
        .stroke(Stroke::new(
            if focused {
                theme::BORDER
            } else {
                theme::HAIRLINE
            },
            if focused {
                p.field_edge_focus()
            } else {
                p.field_edge()
            },
        ))
        .corner_radius(theme::radius::MD)
        .inner_margin(Margin::symmetric(10, 5))
}

pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str, p: &Palette, id: Id) -> Response {
    text_field_ex(ui, value, hint, false, true, p, id)
}

pub fn text_field_ex(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    invalid: bool,
    enabled: bool,
    p: &Palette,
    id: Id,
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, INPUT_HEIGHT),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    let focused = ui.memory(|memory| memory.has_focus(id));
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered() && !focused && enabled);
    let mut fill = p.field_fill();
    if hover_t > 0.001 {
        fill = p.over(fill, p.fade(p.hover, hover_t));
    }
    if !enabled {
        fill = dim(fill);
    }
    let corner = CornerRadius::same(theme::radius::MD);
    ui.painter().rect_filled(rect, corner, fill);
    let (highlight_width, highlight) = p.field_highlight();
    ui.painter().hline(
        (rect.left() + theme::radius::MD as f32)..=(rect.right() - theme::radius::MD as f32),
        rect.top() + highlight_width * 0.5,
        Stroke::new(highlight_width, highlight),
    );
    let (edge_width, edge) = if !enabled {
        (theme::HAIRLINE, dim(p.field_edge()))
    } else if invalid {
        (theme::BORDER, p.red)
    } else if focused {
        (theme::BORDER, p.field_edge_focus())
    } else {
        (theme::HAIRLINE, p.field_edge())
    };
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(edge_width, edge),
        StrokeKind::Inside,
    );
    let text_rect = rect.shrink2(Vec2::new(10.0, 4.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let edit = egui::TextEdit::singleline(value)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(Margin::ZERO)
        .hint_text(
            RichText::new(hint)
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg_muted),
        )
        .text_color(if enabled { p.fg } else { p.disabled })
        .font(theme::font(theme::TEXT_UI_LABEL))
        .desired_width(f32::INFINITY);
    let edit_response = child.add(edit);
    if enabled && response.clicked() {
        edit_response.request_focus();
    }
    if response.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    edit_response
}

pub fn search_field(ui: &mut Ui, query: &mut String, hint: &str, p: &Palette, id: Id) -> bool {
    let height = INPUT_HEIGHT;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let focused = ui.memory(|memory| memory.has_focus(id));
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered() && !focused);
    let mut fill = p.field_fill();
    if hover_t > 0.001 {
        fill = p.over(fill, p.fade(p.hover, hover_t));
    }
    let corner = CornerRadius::same(theme::radius::MD);
    ui.painter().rect_filled(rect, corner, fill);
    let (highlight_width, highlight) = p.field_highlight();
    ui.painter().hline(
        (rect.left() + theme::radius::MD as f32)..=(rect.right() - theme::radius::MD as f32),
        rect.top() + highlight_width * 0.5,
        Stroke::new(highlight_width, highlight),
    );
    let (edge_width, edge) = if focused {
        (theme::BORDER, p.field_edge_focus())
    } else {
        (theme::HAIRLINE, p.field_edge())
    };
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(edge_width, edge),
        StrokeKind::Inside,
    );
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 19.0, rect.center().y),
        Vec2::splat(16.0),
    );
    icons::paint(ui, Icon::Search, icon_rect, p.fg_muted);
    let clear_space = if query.is_empty() { 0.0 } else { 24.0 };
    let text_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 37.0, rect.top() + 5.0),
        Pos2::new(rect.right() - 12.0 - clear_space, rect.bottom() - 5.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let edit = egui::TextEdit::singleline(query)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(Margin::ZERO)
        .hint_text(
            RichText::new(hint)
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg_muted),
        )
        .text_color(p.fg)
        .font(theme::font(theme::TEXT_UI_LABEL))
        .desired_width(f32::INFINITY);
    let edit_response = child.add(edit);
    if response.clicked() {
        edit_response.request_focus();
    }
    if !query.is_empty() && response.hovered() {
        let clear = Rect::from_center_size(
            Pos2::new(rect.right() - 16.0, rect.center().y),
            Vec2::splat(20.0),
        );
        let clear_response = ui.interact(clear, id.with("clear"), Sense::click());
        icons::paint(
            ui,
            Icon::Close,
            Rect::from_center_size(clear.center(), Vec2::splat(14.0)),
            if clear_response.hovered() {
                p.fg
            } else {
                p.fg_muted
            },
        );
        if clear_response.clicked() {
            query.clear();
        }
    }
    edit_response.changed()
}

pub fn textarea(ui: &mut Ui, value: &mut String, hint: &str, p: &Palette, id: Id) -> Response {
    textarea_ex(ui, value, hint, true, p, id)
}

pub fn textarea_ex(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    enabled: bool,
    p: &Palette,
    id: Id,
) -> Response {
    let width = ui.available_width();
    let font = theme::font(theme::TEXT_UI_LABEL);
    let inner_width = (width - 22.0).max(16.0);
    let mut job = egui::text::LayoutJob::default();
    let measure = if value.is_empty() {
        hint
    } else {
        value.as_str()
    };
    job.append(
        measure,
        0.0,
        egui::TextFormat {
            font_id: font.clone(),
            color: p.fg,
            line_height: Some(theme::LINE_LABEL),
            ..Default::default()
        },
    );
    job.wrap.max_width = inner_width;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let content_height = (galley.size().y + 8.0).max(TEXTAREA_MIN_HEIGHT);
    let stored = ui
        .memory(|memory| memory.data.get_temp::<f32>(id.with("height")))
        .unwrap_or(0.0);
    let base_height = content_height + TEXTAREA_FOOTER_HEIGHT + 8.0 + 10.0;
    let height = stored.max(base_height);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, height),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    let focused = ui.memory(|memory| memory.has_focus(id));
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::HOVER)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered() && !focused && enabled);
    let mut fill = p.field_fill();
    if hover_t > 0.001 {
        fill = p.over(fill, p.fade(p.hover, hover_t));
    }
    if !enabled {
        fill = dim(fill);
    }
    let corner = CornerRadius::same(theme::radius::XL);
    ui.painter().rect_filled(rect, corner, fill);
    let (edge_width, edge) = if !enabled {
        (theme::HAIRLINE, dim(p.field_edge()))
    } else if focused {
        (theme::BORDER, p.field_edge_focus())
    } else {
        (theme::HAIRLINE, p.field_edge())
    };
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(edge_width, edge),
        StrokeKind::Inside,
    );
    let (highlight_width, highlight) = p.field_highlight();
    ui.painter().hline(
        (rect.left() + theme::radius::XL as f32)..=(rect.right() - theme::radius::XL as f32),
        rect.top() + highlight_width * 0.5,
        Stroke::new(highlight_width, highlight),
    );
    let footer_top = rect.bottom() - 10.0 - TEXTAREA_FOOTER_HEIGHT;
    let text_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 12.0, rect.top() + 10.0),
        Pos2::new(rect.right() - 10.0, footer_top - 8.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    let hint_color = if (response.hovered() || focused) && enabled {
        p.fade(p.fg, 0.8)
    } else {
        p.fg_muted
    };
    let edit = egui::TextEdit::multiline(value)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(Margin::ZERO)
        .hint_text(RichText::new(hint).font(font.clone()).color(hint_color))
        .text_color(if enabled { p.fg } else { p.disabled })
        .font(font)
        .desired_width(f32::INFINITY)
        .desired_rows(3);
    let edit_response = child.add(edit);
    if enabled && response.clicked() {
        edit_response.request_focus();
    }
    let handle_rect = Rect::from_min_max(
        Pos2::new(rect.right() - 38.0, rect.bottom() - 38.0),
        Pos2::new(rect.right(), rect.bottom()),
    );
    if enabled {
        let handle = ui.interact(handle_rect, id.with("resize"), Sense::drag());
        if handle.hovered() || handle.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if handle.dragged() {
            let next = (height + handle.drag_delta().y).max(base_height);
            ui.memory_mut(|memory| memory.data.insert_temp(id.with("height"), next));
            ui.ctx().request_repaint();
        }
    }
    let glyph = Rect::from_center_size(
        Pos2::new(rect.right() - 10.0, rect.bottom() - 10.0),
        Vec2::splat(12.0),
    );
    let stroke = Stroke::new(theme::BORDER, p.fg_muted);
    ui.painter().line_segment(
        [
            Pos2::new(glyph.left() + 1.0, glyph.bottom() - 1.0),
            Pos2::new(glyph.right() - 3.0, glyph.top() + 3.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            Pos2::new(glyph.left() + 5.0, glyph.bottom() - 1.0),
            Pos2::new(glyph.right() - 1.0, glyph.top() + 5.0),
        ],
        stroke,
    );
    response
}

pub fn menu_item(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    shortcut: Option<&str>,
    p: &Palette,
) -> Response {
    menu_item_ex(ui, label, icon, shortcut, false, true, p)
}

pub fn menu_item_ex(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    shortcut: Option<&str>,
    selected: bool,
    enabled: bool,
    p: &Palette,
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, MENU_ITEM_HEIGHT),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    let highlighted = enabled && (response.hovered() || response.has_focus());
    let fill = if selected {
        p.selection
    } else if highlighted {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(MENU_RADIUS), fill);
    }
    let text_color = if !enabled {
        p.disabled
    } else if selected {
        p.selection_fg
    } else {
        p.fg
    };
    let mut x = rect.left() + 8.0;
    if let Some(icon) = icon {
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(Pos2::new(x + 7.0, rect.center().y), Vec2::splat(14.0)),
            if enabled { p.fg_muted } else { p.disabled },
        );
        x += 14.0 + 8.0;
    }
    ui.painter().text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        theme::font(theme::TEXT_UI_LABEL),
        text_color,
    );
    if let Some(shortcut) = shortcut {
        ui.painter().text(
            Pos2::new(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shortcut,
            theme::font_mono(11.0),
            if enabled { p.fg_muted } else { p.disabled },
        );
    } else if selected {
        icons::paint(
            ui,
            Icon::Check,
            Rect::from_center_size(
                Pos2::new(rect.right() - 15.0, rect.center().y),
                Vec2::splat(14.0),
            ),
            p.fg_muted,
        );
    }
    response
}

pub fn menu_item_destructive(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    shortcut: Option<&str>,
    enabled: bool,
    p: &Palette,
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, MENU_ITEM_HEIGHT),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let enabled = enabled && ui.is_enabled();
    let highlighted = enabled && (response.hovered() || response.has_focus());
    if highlighted {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(MENU_RADIUS),
            if p.dark {
                p.wash(p.red, 51)
            } else {
                p.wash(p.red, 26)
            },
        );
    }
    let text_color = if enabled { p.red } else { p.disabled };
    let mut x = rect.left() + 8.0;
    if let Some(icon) = icon {
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(Pos2::new(x + 7.0, rect.center().y), Vec2::splat(14.0)),
            text_color,
        );
        x += 14.0 + 8.0;
    }
    ui.painter().text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        theme::font(theme::TEXT_UI_LABEL),
        text_color,
    );
    if let Some(shortcut) = shortcut {
        ui.painter().text(
            Pos2::new(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shortcut,
            theme::font_mono(11.0),
            if enabled { p.fg_muted } else { p.disabled },
        );
    }
    response
}

pub fn menu_separator(ui: &mut Ui, p: &Palette) {
    let rect = ui.available_rect_before_wrap();
    ui.add_space(2.0);
    let y = rect.top() + 2.0;
    ui.painter().hline(
        (rect.left() - 4.0)..=(rect.right() + 4.0),
        y,
        Stroke::new(theme::HAIRLINE, p.edge_color()),
    );
    ui.allocate_space(Vec2::new(rect.width(), 1.0));
    ui.add_space(2.0);
}

pub fn copy_button(ui: &mut Ui, text: &str, p: &Palette) -> Response {
    let id = ui.next_auto_id();
    let now = ui.input(|input| input.time);
    let until = ui
        .memory(|memory| memory.data.get_temp::<f64>(id.with("copied")))
        .unwrap_or(0.0);
    let copied = now < until;
    let icon = if copied { Icon::Check } else { Icon::Copy };
    let tint = if copied { p.green } else { p.fg_muted };
    let response = icon_button_hit(ui, icon, 14.0, HIT_XS, tint);
    if response.clicked() {
        ui.ctx().copy_text(text.to_string());
        ui.memory_mut(|memory| memory.data.insert_temp(id.with("copied"), now + 1.4));
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1400));
    }
    response
}

pub fn truncated(ui: &mut Ui, text: &str, font: FontId, color: Color32, max_width: f32) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
}

pub fn hline(ui: &mut Ui, p: &Palette, inset: f32) {
    let rect = ui.available_rect_before_wrap();
    let y = rect.top();
    ui.painter().hline(
        (rect.left() + inset)..=(rect.right() - inset),
        y,
        Stroke::new(theme::HAIRLINE, p.edge_muted()),
    );
    ui.add_space(theme::BORDER);
}

pub fn centered_text(ui: &mut Ui, text: &str, font: FontId, color: Color32) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(text).font(font).color(color));
    });
}

pub fn dialog_backdrop(ctx: &egui::Context, p: &Palette, t: f32) {
    let rect = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        Id::unique("dialog-backdrop"),
    ))
    .rect_filled(rect, CornerRadius::ZERO, p.fade(p.scrim(), t));
}

pub fn dialog_scale_rect(rect: Rect, t: f32) -> Rect {
    let scale = 0.98 + 0.02 * t.clamp(0.0, 1.0);
    Rect::from_center_size(rect.center(), rect.size() * scale)
}

pub fn dialog_frame(p: &Palette) -> egui::Frame {
    theme::dialog_frame(p)
}

pub fn dialog_header<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = DIALOG_HEADER_GAP;
        add(ui)
    })
    .inner
}

pub fn dialog_title(ui: &mut Ui, text: &str, p: &Palette) {
    ui.label(
        RichText::new(text)
            .font(theme::font_semibold(theme::TEXT_UI_HEADER))
            .color(p.fg),
    );
}

pub fn dialog_description(ui: &mut Ui, text: &str, p: &Palette) {
    ui.label(
        RichText::new(text)
            .font(theme::font(theme::TEXT_META))
            .color(p.fg_muted),
    );
}

pub fn dialog_footer<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = DIALOG_FOOTER_GAP;
        add(ui)
    })
    .inner
}

pub fn dialog_close(ui: &mut Ui, p: &Palette) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(DIALOG_CLOSE_SIZE), Sense::click());
    let hover_t = motion::Animated::from_id(response.id.with("hover"))
        .with_duration(motion::FAST)
        .with_easing(motion::Easing::EaseOut)
        .bool(ui.ctx(), response.hovered() || response.has_focus());
    let fg = p.mix(p.fg_muted, p.fg, hover_t);
    icons::paint(
        ui,
        Icon::Close,
        Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
        theme::scale_alpha(fg, 0.7 + 0.3 * hover_t),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_sizes_match_the_reference_ladder() {
        let p = Palette::light();
        egui::__run_test_ui(|ui| {
            for (size, height) in [
                (ButtonSize::Xs, 24.0),
                (ButtonSize::Sm, 28.0),
                (ButtonSize::Md, 32.0),
                (ButtonSize::Lg, 36.0),
            ] {
                let response = button(
                    ui,
                    "Save",
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    size,
                    &p,
                );
                assert!((response.rect.height() - height).abs() < 0.01);
            }
        });
    }

    #[test]
    fn primitives_render_at_reference_metrics() {
        let p = Palette::light();
        egui::__run_test_ui(|ui| {
            ui.set_width(600.0);
            let field = text_field(ui, &mut String::new(), "Hint", &p, Id::unique("field"));
            assert!(field.rect.height() <= INPUT_HEIGHT);
            let area = textarea(ui, &mut String::new(), "Hint", &p, Id::unique("area"));
            assert!(area.rect.height() >= TEXTAREA_MIN_HEIGHT);
            let mut on = false;
            assert!((switch(ui, &mut on, &p).rect.height() - SWITCH_HEIGHT).abs() < 0.01);
            let mut checked = false;
            assert!((checkbox(ui, &mut checked, &p).rect.height() - CHECKBOX_SIZE).abs() < 0.01);
            let mut selected = 0usize;
            segmented(ui, &["One", "Two"], &mut selected, &p);
            segmented_fixed(
                ui,
                &["One", "Two", "Three"],
                &mut selected,
                SEGMENT_HEIGHT,
                240.0,
                &p,
            );
            let select_options = [
                ("a".to_string(), "Alpha".to_string()),
                ("b".to_string(), "Beta".to_string()),
            ];
            let mut choice = "a".to_string();
            assert!(!select(ui, &mut choice, &select_options, "Pick", &p));
            assert_eq!(choice, "a");
            let mut value = 0.5;
            let response = slider(ui, &mut value, 0.0, 1.0, &p);
            assert!((response.rect.height() - SLIDER_HEIGHT).abs() < 0.01);
            let specs = [
                TabSpec::new("File").icon(Icon::File).closable(),
                TabSpec::new("Changes"),
            ];
            tabs_strip(ui, &specs, 0, TabVariant::Pill, &p);
            tabs_strip(ui, &specs, 0, TabVariant::Underline, &p);
            chip(ui, "Filter", Some(Icon::Search), true, &p);
            pill(ui, "status", p.fg, p.hover, None);
            menu_item(ui, "Open", Some(Icon::Folder), Some("⌘O"), &p);
            menu_item_destructive(ui, "Delete", Some(Icon::Trash), None, true, &p);
            menu_separator(ui, &p);
            busy_dots(ui, &p);
            busy_label(ui, "Working", &p);
            spinner(ui, 16.0, p.primary);
            kbd(ui, "⌘K", &p);
            assert_eq!(dialog_header(ui, |ui| dialog_title(ui, "Title", &p)), ());
            dialog_description(ui, "Body", &p);
            dialog_footer(ui, |ui| {
                let _ = dialog_close(ui, &p);
            });
        });
    }

    #[test]
    fn skeleton_bar_renders_at_text_height() {
        let p = Palette::light();
        egui::__run_test_ui(|ui| {
            let response = skeleton_bar(ui, 120.0, &p);
            assert!((response.rect.height() - SKELETON_BAR_HEIGHT).abs() < 0.01);
            assert!((response.rect.width() - 120.0).abs() < 0.01);
        });
    }
}
