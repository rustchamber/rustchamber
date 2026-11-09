pub mod files;
pub mod git;
pub mod terminal;
pub mod viewers;

use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, Layout, Margin, Pos2, Rect, Response, RichText,
    ScrollArea, Sense, Stroke, Ui, Vec2,
};

use crate::icons::{self, Icon};
use crate::motion;
use crate::theme::{self, Palette};
use crate::widgets;

pub const PANEL_HEADER_HEIGHT: f32 = 40.0;
pub const PANEL_ACTION: f32 = 28.0;
pub const PANEL_ACTIONS_GAP: f32 = 4.0;
pub const PANEL_INSET: f32 = 6.0;
pub const TAB_HEIGHT: f32 = PANEL_HEADER_HEIGHT;
pub const SECTION_GAP: f32 = 20.0;
pub const HAIRLINE: f32 = 0.5;

const TAB_SIDE_PAD: f32 = 12.0;
const TAB_ICON: f32 = 16.0;
const TAB_LABEL_GAP: f32 = 6.0;
const TAB_CLOSE: f32 = 20.0;
const TAB_CLOSE_GAP: f32 = 4.0;
const TAB_MAX_WIDTH: f32 = 224.0;
const MODE_ICON: f32 = 14.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelZone {
    #[default]
    Right,
    Bottom,
}

impl PanelZone {
    pub fn other(self) -> Self {
        match self {
            PanelZone::Right => PanelZone::Bottom,
            PanelZone::Bottom => PanelZone::Right,
        }
    }

    pub fn move_label(self) -> &'static str {
        match self.other() {
            PanelZone::Right => "Move to right",
            PanelZone::Bottom => "Move to bottom",
        }
    }

    pub fn move_icon(self) -> Icon {
        match self.other() {
            PanelZone::Right => Icon::PanelRight,
            PanelZone::Bottom => Icon::ArrowDown,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TabDrag {
    pub label: String,
    pub active: bool,
    pub outside: bool,
    pub released: Option<Pos2>,
}

fn zone_id() -> Id {
    Id::unique("panel-current-zone")
}

fn zone_request_id() -> Id {
    Id::unique("panel-zone-request")
}

fn close_request_id() -> Id {
    Id::unique("panel-close-request")
}

fn tab_drag_id() -> Id {
    Id::unique("panel-tab-drag")
}

pub fn set_panel_zone(ctx: &egui::Context, zone: PanelZone) {
    ctx.data_mut(|data| data.insert_temp(zone_id(), zone));
}

pub fn panel_zone(ctx: &egui::Context) -> PanelZone {
    ctx.data(|data| data.get_temp::<PanelZone>(zone_id()))
        .unwrap_or_default()
}

pub fn request_zone(ctx: &egui::Context, zone: PanelZone) {
    ctx.data_mut(|data| data.insert_temp(zone_request_id(), zone));
}

pub fn take_zone_request(ctx: &egui::Context) -> Option<PanelZone> {
    ctx.data_mut(|data| data.remove_temp::<PanelZone>(zone_request_id()))
}

pub fn request_close(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(close_request_id(), true));
}

pub fn take_close_request(ctx: &egui::Context) -> bool {
    ctx.data_mut(|data| data.remove_temp::<bool>(close_request_id()))
        .unwrap_or(false)
}

pub fn set_tab_drag(ctx: &egui::Context, drag: Option<TabDrag>) {
    ctx.data_mut(|data| {
        if let Some(drag) = drag {
            data.insert_temp(tab_drag_id(), drag);
        } else {
            data.remove::<TabDrag>(tab_drag_id());
        }
    });
}

pub fn tab_drag(ctx: &egui::Context) -> Option<TabDrag> {
    ctx.data(|data| data.get_temp::<TabDrag>(tab_drag_id()))
}

pub fn clear_tab_drag(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<TabDrag>(tab_drag_id()));
}

pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from >= items.len() || to >= items.len() || from == to {
        return;
    }
    let item = items.remove(from);
    items.insert(to, item);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Surface {
    #[default]
    Session,
    Context,
    Changes,
    Git,
    Files,
    Terminal,
    Extension,
}

impl Surface {
    pub const ALL: [Surface; 6] = [
        Surface::Session,
        Surface::Context,
        Surface::Changes,
        Surface::Git,
        Surface::Files,
        Surface::Terminal,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Surface::Session => "Session",
            Surface::Context => "Context",
            Surface::Changes => "Changes",
            Surface::Git => "Git",
            Surface::Files => "Files",
            Surface::Terminal => "Terminal",
            Surface::Extension => "Extension",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Surface::Session => Icon::Layers,
            Surface::Context => Icon::Circle,
            Surface::Changes => Icon::Diff,
            Surface::Git => Icon::Branch,
            Surface::Files => Icon::File,
            Surface::Terminal => Icon::Terminal,
            Surface::Extension => Icon::Puzzle,
        }
    }
}

pub fn edge(p: &Palette) -> Color32 {
    p.alpha(p.border, 134)
}

pub fn edge_muted(p: &Palette) -> Color32 {
    p.alpha(p.border, 76)
}

#[derive(Clone, Debug)]
pub struct PanelTab {
    pub label: String,
    pub icon: Icon,
    pub active: bool,
    pub closable: bool,
}

impl PanelTab {
    pub fn new(label: impl Into<String>, icon: Icon) -> Self {
        Self {
            label: label.into(),
            icon,
            active: false,
            closable: false,
        }
    }

    pub fn active(mut self, value: bool) -> Self {
        self.active = value;
        self
    }

    pub fn closable(mut self, value: bool) -> Self {
        self.closable = value;
        self
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PanelHeader {
    pub select: Option<usize>,
    pub close: Option<usize>,
    pub double: Option<usize>,
    pub reorder: Option<(usize, usize)>,
}

pub fn panel_header(
    ui: &mut Ui,
    p: &Palette,
    tabs: &[PanelTab],
    toolbar: impl FnOnce(&mut Ui),
    actions: impl FnOnce(&mut Ui),
) -> PanelHeader {
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, PANEL_HEADER_HEIGHT), Sense::hover());
    ui.painter().hline(
        row.x_range(),
        row.bottom() - theme::BORDER * 0.5,
        Stroke::new(theme::BORDER, p.border),
    );

    let mut result = PanelHeader::default();
    let inner = Rect::from_min_max(
        Pos2::new(row.left() + PANEL_INSET, row.top()),
        Pos2::new(row.right() - PANEL_INSET, row.bottom()),
    );

    let action_width = inner.width().clamp(0.0, 300.0);
    let mut action_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                Pos2::new(inner.right() - action_width, inner.top()),
                inner.max,
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    action_ui.spacing_mut().item_spacing.x = PANEL_ACTIONS_GAP;
    actions(&mut action_ui);
    let actions_left = action_ui
        .min_rect()
        .left()
        .min(inner.right())
        .max(inner.left());

    let is_strip = tabs.len() > 1 || tabs.first().is_some_and(|tab| tab.closable);
    let mut content_right = inner.left();
    if is_strip {
        let available = (actions_left - row.left() - 8.0).max(0.0);
        if available > 24.0 {
            let strip_rect = Rect::from_min_max(
                Pos2::new(row.left(), row.top()),
                Pos2::new(row.left() + available, row.bottom()),
            );
            let content_width: f32 = tabs.iter().map(|tab| measure_tab(ui, p, tab)).sum();
            let mut strip_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(strip_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            let indicator_id = strip_ui.scope_id().with("indicator");
            ScrollArea::horizontal()
                .id_salt(("panel-tabs", ui.scope_id()))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, true])
                .show(&mut strip_ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.horizontal(|ui| {
                        panel_tabs(ui, p, tabs, strip_rect, indicator_id, &mut result);
                    });
                });
            content_right = (strip_rect.left() + content_width).min(strip_rect.right()) + 8.0;
        }
    } else if let Some(tab) = tabs.first() {
        content_right = paint_mode_label(ui, p, tab, row, actions_left - 4.0);
    }

    if content_right < actions_left - 8.0 {
        let toolbar_rect = Rect::from_min_max(
            Pos2::new(content_right, inner.top()),
            Pos2::new(actions_left - 4.0, inner.bottom()),
        );
        let mut toolbar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(toolbar_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        toolbar_ui.spacing_mut().item_spacing.x = 8.0;
        toolbar(&mut toolbar_ui);
    }

    result
}

fn paint_mode_label(ui: &mut Ui, p: &Palette, tab: &PanelTab, row: Rect, right: f32) -> f32 {
    let left = row.left() + TAB_SIDE_PAD;
    icons::paint(
        ui,
        tab.icon,
        Rect::from_center_size(
            Pos2::new(left + MODE_ICON * 0.5, row.center().y),
            Vec2::splat(MODE_ICON),
        ),
        p.fg,
    );
    let text_left = left + MODE_ICON + TAB_LABEL_GAP;
    let width = (right - text_left - TAB_SIDE_PAD).max(24.0);
    let galley = truncated_galley(
        ui,
        tab.label.clone(),
        theme::font(theme::TEXT_UI_LABEL),
        p.fg,
        width,
    );
    ui.painter().galley(
        Pos2::new(text_left, row.center().y - galley.size().y * 0.5),
        galley.clone(),
        p.fg,
    );
    (text_left + galley.size().x + TAB_SIDE_PAD).min(right)
}

fn measure_tab(ui: &Ui, p: &Palette, tab: &PanelTab) -> f32 {
    let text = ui
        .painter()
        .layout_no_wrap(tab.label.clone(), theme::font(theme::TEXT_MICRO), p.fg);
    let close = if tab.closable {
        TAB_CLOSE + TAB_CLOSE_GAP
    } else {
        0.0
    };
    (text.size().x + TAB_SIDE_PAD * 2.0 + TAB_ICON + TAB_LABEL_GAP)
        .clamp(TAB_SIDE_PAD * 2.0 + TAB_ICON + TAB_LABEL_GAP, TAB_MAX_WIDTH)
        + close
}

fn tab_drop_target(rects: &[Rect], pointer_x: f32, from: usize) -> Option<usize> {
    if rects.is_empty() || from >= rects.len() {
        return None;
    }
    let slot = rects
        .iter()
        .take_while(|rect| pointer_x > rect.center().x)
        .count();
    let target = if slot > from { slot - 1 } else { slot };
    (target < rects.len() && target != from).then_some(target)
}

fn panel_tabs(
    ui: &mut Ui,
    p: &Palette,
    tabs: &[PanelTab],
    strip: Rect,
    indicator_id: Id,
    result: &mut PanelHeader,
) {
    let mut rects = Vec::with_capacity(tabs.len());
    let mut hits = Vec::with_capacity(tabs.len());
    for (index, tab) in tabs.iter().enumerate() {
        let width = measure_tab(ui, p, tab);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, TAB_HEIGHT), Sense::hover());
        let hit = ui.interact(
            rect,
            Id::unique(("panel-tab", ui.scope_id(), index)),
            Sense::click_and_drag(),
        );
        if tab.active
            && !strip.contains_rect(rect)
            && !ui.ctx().input(|input| input.pointer.any_down())
        {
            ui.scroll_to_rect(rect, Some(Align::Center));
        }
        rects.push(rect);
        hits.push(hit);
    }

    let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
    let mut dragged: Option<usize> = None;
    for (index, hit) in hits.iter().enumerate() {
        if hit.dragged_by(egui::PointerButton::Primary) {
            dragged = Some(index);
        }
    }
    let drag_offset = dragged
        .and_then(|index| hits[index].total_drag_delta())
        .unwrap_or_default();
    let strip_hit = Rect::from_min_max(
        Pos2::new(strip.left(), strip.top() - 16.0),
        Pos2::new(strip.right(), strip.bottom() + 16.0),
    );
    let slot = match (dragged, pointer) {
        (Some(_), Some(pos)) if strip_hit.contains(pos) => Some(
            rects
                .iter()
                .take_while(|rect| pos.x > rect.center().x)
                .count(),
        ),
        _ => None,
    };

    let pointer_pos = ui.ctx().pointer_hover_pos();
    let close_hovers: Vec<bool> = tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            if !tab.closable {
                return false;
            }
            pointer_pos.is_some_and(|pos| tab_close_rect(rects[index]).contains(pos))
        })
        .collect();

    if let Some(active) = tabs.iter().position(|tab| tab.active) {
        let target = rects[active];
        let x = motion::Motion::from_id(indicator_id.with("x"))
            .with_duration(motion::TAB)
            .with_easing(motion::Easing::Decelerate)
            .value(ui.ctx(), target.left());
        let width = motion::Motion::from_id(indicator_id.with("w"))
            .with_duration(motion::TAB_SIZE)
            .with_easing(motion::Easing::Decelerate)
            .value(ui.ctx(), target.width());
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(x, strip.bottom() - widgets::TAB_UNDERLINE),
                Vec2::new(width, widgets::TAB_UNDERLINE),
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

    for (index, tab) in tabs.iter().enumerate() {
        if dragged == Some(index) {
            continue;
        }
        let hover = motion::hover_fade(
            ui,
            hits[index].id.with("hover"),
            (hits[index].hovered() || close_hovers[index]) && !tab.active,
        );
        paint_panel_tab(ui, p, tab, rects[index], hover, 1.0);
    }
    if let Some(index) = dragged {
        paint_panel_tab(
            ui,
            p,
            &tabs[index],
            rects[index].translate(Vec2::new(drag_offset.x, 0.0)),
            0.0,
            0.5,
        );
    }

    if let (Some(_), Some(slot)) = (dragged, slot) {
        let center_y = rects
            .first()
            .map_or(strip.center().y, |rect| rect.center().y);
        let x = match rects.get(slot) {
            Some(rect) => rect.left() - 1.0,
            None => rects.last().map_or(strip.left(), |rect| rect.right() + 1.0),
        };
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(x - 1.0, center_y - 9.0), Vec2::new(2.0, 18.0)),
            CornerRadius::same(1),
            p.primary,
        );
    }

    let mut close_clicked: Option<usize> = None;
    let mut menu_move: Option<PanelZone> = None;
    let mut menu_close: Option<usize> = None;
    let mut menu_close_panel = false;
    let zone = panel_zone(ui.ctx());

    for (index, tab) in tabs.iter().enumerate() {
        if tab.closable && dragged.is_none() {
            let rect = rects[index];
            let close_rect = tab_close_rect(rect);
            let visible = tab.active || hits[index].hovered() || close_hovers[index];
            if visible {
                let close_response = ui.interact(
                    close_rect,
                    Id::unique(("panel-tab-close", ui.scope_id(), index)),
                    Sense::click(),
                );
                if close_response.hovered() {
                    ui.painter().rect_filled(
                        close_rect,
                        CornerRadius::same(theme::radius::SM),
                        p.hover,
                    );
                }
                icons::paint(
                    ui,
                    Icon::Close,
                    Rect::from_center_size(close_rect.center(), Vec2::splat(12.0)),
                    if close_response.hovered() {
                        p.fg
                    } else {
                        p.fg_muted
                    },
                );
                if close_response.clicked() {
                    close_clicked = Some(index);
                }
            }
        }

        let hit = hits[index].clone();
        if hit.clicked() && close_clicked != Some(index) {
            result.select = Some(index);
        }
        if hit.middle_clicked() && tab.closable {
            close_clicked = Some(index);
        }
        if hit.double_clicked() {
            result.double = Some(index);
        }
        if hit.hovered() || dragged == Some(index) {
            ui.ctx().set_cursor_icon(if dragged == Some(index) {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::PointingHand
            });
        }
        if !hit.dragged() && !hit.drag_stopped() {
            let _ = hit.clone().on_hover_text(if tab.closable {
                format!("{} — middle-click or the close button closes it", tab.label)
            } else {
                tab.label.clone()
            });
        }

        if hit.drag_started_by(egui::PointerButton::Primary)
            || hit.dragged_by(egui::PointerButton::Primary)
        {
            set_tab_drag(
                ui.ctx(),
                Some(TabDrag {
                    label: tab.label.clone(),
                    active: true,
                    outside: pointer.is_none_or(|pos| !strip_hit.contains(pos)),
                    released: None,
                }),
            );
        } else if hit.drag_stopped_by(egui::PointerButton::Primary) {
            if let Some(pos) = pointer
                && strip_hit.contains(pos)
                && let Some(target) = tab_drop_target(&rects, pos.x, index)
            {
                result.reorder = Some((index, target));
            }
            set_tab_drag(
                ui.ctx(),
                Some(TabDrag {
                    label: tab.label.clone(),
                    active: false,
                    outside: pointer.is_none_or(|pos| !strip_hit.contains(pos)),
                    released: pointer,
                }),
            );
        }

        hit.context_menu(|ui| {
            ui.set_min_width(196.0);
            let close = if tab.closable {
                widgets::menu_item(ui, "Close", Some(Icon::Close), None, p)
            } else {
                widgets::menu_item(ui, "Close panel", Some(Icon::Close), None, p)
            };
            if close.clicked() {
                if tab.closable {
                    menu_close = Some(index);
                } else {
                    menu_close_panel = true;
                }
                ui.close();
            }
            widgets::menu_separator(ui, p);
            if widgets::menu_item(ui, zone.move_label(), Some(zone.move_icon()), None, p).clicked()
            {
                menu_move = Some(zone.other());
                ui.close();
            }
        });
    }

    if menu_close_panel {
        request_close(ui.ctx());
    }
    if let Some(zone) = menu_move {
        request_zone(ui.ctx(), zone);
    }
    if let Some(index) = close_clicked.or(menu_close) {
        result.close = Some(index);
    }
}

fn tab_close_rect(tab_rect: Rect) -> Rect {
    Rect::from_center_size(
        Pos2::new(
            tab_rect.right() - TAB_CLOSE_GAP - TAB_CLOSE * 0.5,
            tab_rect.center().y,
        ),
        Vec2::splat(TAB_CLOSE),
    )
}

fn paint_panel_tab(ui: &Ui, p: &Palette, tab: &PanelTab, rect: Rect, hover: f32, opacity: f32) {
    let close_reserve = if tab.closable {
        TAB_CLOSE + TAB_CLOSE_GAP
    } else {
        0.0
    };
    let color = theme::scale_alpha(
        if tab.active {
            p.fg
        } else {
            p.mix(p.fg_muted, p.fg, hover)
        },
        opacity,
    );
    let icon_color = theme::scale_alpha(
        if tab.active {
            p.primary
        } else {
            p.mix(p.fg_muted, p.fg, hover)
        },
        opacity,
    );
    icons::paint(
        ui,
        tab.icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + TAB_SIDE_PAD + TAB_ICON * 0.5, rect.center().y),
            Vec2::splat(TAB_ICON),
        ),
        icon_color,
    );
    let text_left = rect.left() + TAB_SIDE_PAD + TAB_ICON + TAB_LABEL_GAP;
    let text_width = (rect.right() - close_reserve - TAB_SIDE_PAD - text_left).max(8.0);
    let galley = truncated_galley(
        ui,
        tab.label.clone(),
        theme::font(theme::TEXT_MICRO),
        color,
        text_width,
    );
    ui.painter().galley(
        Pos2::new(text_left, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
}

pub fn hairline(ui: &mut Ui, p: &Palette) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        Stroke::new(HAIRLINE, edge_muted(p)),
    );
    ui.add_space(HAIRLINE);
}

pub fn section_label(ui: &mut Ui, text: &str, p: &Palette) {
    ui.label(
        RichText::new(text)
            .font(theme::font_semibold(14.0))
            .color(p.fg),
    );
}

pub fn context_label(ui: &mut Ui, text: &str, p: &Palette) {
    ui.label(
        RichText::new(text)
            .font(theme::font_semibold(12.0))
            .color(p.fg),
    );
}

pub fn section_label_at(ui: &Ui, rect: Rect, text: &str, p: &Palette) {
    ui.painter().text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        text,
        theme::font_semibold(13.5),
        p.fg,
    );
}

pub fn panel_button(ui: &mut Ui, p: &Palette, icon: Icon, tip: &str) -> Response {
    icon_action(ui, p, icon, PANEL_ACTION, 14.0, tip)
}

pub fn panel_action(ui: &mut Ui, p: &Palette, icon: Icon, enabled: bool, tip: &str) -> Response {
    if !enabled {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(PANEL_ACTION), Sense::hover());
        if ui.is_rect_visible(rect) {
            icons::paint(
                ui,
                icon,
                Rect::from_center_size(rect.center(), Vec2::splat(14.0)),
                p.disabled,
            );
        }
        return response.on_hover_text(tip);
    }
    icon_action(ui, p, icon, PANEL_ACTION, 14.0, tip)
}

pub fn mini_icon_button(ui: &mut Ui, p: &Palette, icon: Icon, tip: &str) -> Response {
    icon_action(ui, p, icon, 22.0, 12.0, tip)
}

fn icon_action(ui: &mut Ui, p: &Palette, icon: Icon, size: f32, glyph: f32, tip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let press = motion::Animated::from_id(response.id.with("press"))
        .with_duration(0.09)
        .bool(ui.ctx(), response.is_pointer_button_down_on());
    let hover = motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    let radius = CornerRadius::same(theme::radius::MD);
    if press > 0.001 {
        ui.painter()
            .rect_filled(rect, radius, p.active.gamma_multiply(press));
    } else if hover > 0.001 {
        ui.painter()
            .rect_filled(rect, radius, p.hover.gamma_multiply(hover));
    }
    let tint = p.mix(p.fg_muted, p.fg, hover.max(press));
    if ui.is_rect_visible(rect) {
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(rect.center(), Vec2::splat(glyph)),
            tint,
        );
    }
    crate::widgets::pointer(ui, &response);
    response.on_hover_text(tip)
}

pub fn refresh_button(ui: &mut Ui, p: &Palette, loading: bool, tip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(PANEL_ACTION), Sense::click());
    let hover = motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if hover > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.hover.gamma_multiply(hover),
        );
    }
    if loading {
        widgets::paint_spinner(ui, rect.center(), 14.0, p.primary);
        ui.ctx().request_repaint_after(Duration::from_millis(40));
    } else if ui.is_rect_visible(rect) {
        icons::paint(
            ui,
            Icon::Refresh,
            Rect::from_center_size(rect.center(), Vec2::splat(14.0)),
            p.mix(p.fg_muted, p.fg, hover),
        );
    }
    response.on_hover_text(tip)
}

pub fn close_button(ui: &mut Ui, p: &Palette) -> Response {
    panel_button(ui, p, Icon::Close, "Close panel")
}

pub fn expand_button(ui: &mut Ui, p: &Palette, expanded: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(PANEL_ACTION), Sense::click());
    let press = motion::Animated::from_id(response.id.with("press"))
        .with_duration(0.09)
        .bool(ui.ctx(), response.is_pointer_button_down_on());
    let hover = motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    let radius = CornerRadius::same(theme::radius::MD);
    if press > 0.001 {
        ui.painter()
            .rect_filled(rect, radius, p.active.gamma_multiply(press));
    } else if hover > 0.001 {
        ui.painter()
            .rect_filled(rect, radius, p.hover.gamma_multiply(hover));
    }
    if ui.is_rect_visible(rect) {
        paint_fullscreen(
            ui,
            rect,
            expanded,
            p.mix(p.fg_muted, p.fg, hover.max(press)),
        );
    }
    response.on_hover_text(if expanded {
        "Collapse panel"
    } else {
        "Expand panel"
    })
}

fn paint_fullscreen(ui: &Ui, rect: Rect, expanded: bool, color: Color32) {
    let (extent, arm) = if expanded { (9.5, 3.2) } else { (14.0, 4.8) };
    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(extent));
    let (l, r, t, b) = (glyph.left(), glyph.right(), glyph.top(), glyph.bottom());
    let corners = [
        [
            Pos2::new(l, t + arm),
            Pos2::new(l, t),
            Pos2::new(l + arm, t),
        ],
        [
            Pos2::new(r - arm, t),
            Pos2::new(r, t),
            Pos2::new(r, t + arm),
        ],
        [
            Pos2::new(l, b - arm),
            Pos2::new(l, b),
            Pos2::new(l + arm, b),
        ],
        [
            Pos2::new(r - arm, b),
            Pos2::new(r, b),
            Pos2::new(r, b - arm),
        ],
    ];
    let stroke = Stroke::new(1.4, color);
    for path in corners {
        ui.painter().add(egui::Shape::line(path.to_vec(), stroke));
    }
}

pub fn status_letter(ui: &Ui, rect: Rect, _p: &Palette, letter: &str, color: Color32) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        theme::font_semibold(12.0),
        color,
    );
}

pub fn empty_state(ui: &mut Ui, p: &Palette, icon: Icon, title: &str, hint: &str) {
    let top = (ui.available_height() * 0.3).clamp(24.0, 180.0);
    ui.add_space(top);
    ui.vertical_centered(|ui| {
        let (mark, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        ui.painter().rect_filled(
            mark,
            CornerRadius::same(theme::radius::XL),
            p.wash(p.fg, if p.dark { 8 } else { 10 }),
        );
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(mark.center(), Vec2::splat(20.0)),
            p.fg_muted,
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new(title)
                .font(theme::font_semibold(13.5))
                .color(p.fg),
        );
        if !hint.is_empty() {
            ui.add_space(4.0);
            let width = (ui.available_width() - 48.0).clamp(120.0, 320.0);
            ui.allocate_ui_with_layout(
                Vec2::new(width, 0.0),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(hint)
                                .font(theme::font(12.5))
                                .color(p.fg_muted),
                        )
                        .wrap(),
                    );
                },
            );
        }
    });
}

pub fn inline_empty(ui: &mut Ui, p: &Palette, label: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new(label)
                .font(theme::font(12.5))
                .color(p.fg_muted),
        );
    });
}

pub fn error_banner(ui: &mut Ui, p: &Palette, message: &str, retry: &str) -> bool {
    let mut clicked = false;
    egui::Frame::new()
        .fill(p.status_bg(p.red))
        .corner_radius(theme::radius::XL)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::icon(ui, Icon::Warning, 14.0, p.status_text(p.red));
                let width = (ui.available_width() - 70.0).max(40.0);
                let galley = truncated_galley_width(
                    ui,
                    message.to_string(),
                    theme::font(12.5),
                    p.status_text(p.red),
                    width,
                    true,
                );
                let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
                ui.painter().galley(rect.min, galley, p.status_text(p.red));
                response.on_hover_text(message);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::ghost_button(ui, retry, Some(Icon::Refresh), p).clicked() {
                        clicked = true;
                    }
                });
            });
        });
    clicked
}

pub fn loading_state(ui: &mut Ui, p: &Palette, label: &str) {
    ui.add_space(18.0);
    ui.vertical_centered(|ui| {
        widgets::spinner(ui, 16.0, p.primary);
        ui.add_space(8.0);
        ui.label(
            RichText::new(label)
                .font(theme::font(12.5))
                .color(p.fg_muted),
        );
    });
    ui.ctx().request_repaint_after(Duration::from_millis(90));
}

pub fn divider(ui: &mut Ui, p: &Palette) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        Stroke::new(HAIRLINE, edge_muted(p)),
    );
    ui.add_space(1.0);
}

pub fn truncated_galley(
    ui: &Ui,
    text: String,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    truncated_galley_width(ui, text, font, color, width, false)
}

fn truncated_galley_width(
    ui: &Ui,
    text: String,
    font: egui::FontId,
    color: Color32,
    width: f32,
    wrap: bool,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text, font, color, width.max(12.0));
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(12.0),
        max_rows: if wrap { 3 } else { 1 },
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_item_reorders_within_bounds() {
        let mut items = vec!["a", "b", "c", "d"];
        move_item(&mut items, 0, 2);
        assert_eq!(items, vec!["b", "c", "a", "d"]);
        move_item(&mut items, 3, 0);
        assert_eq!(items, vec!["d", "b", "c", "a"]);
        move_item(&mut items, 1, 1);
        assert_eq!(items, vec!["d", "b", "c", "a"]);
        move_item(&mut items, 9, 0);
        assert_eq!(items, vec!["d", "b", "c", "a"]);
    }

    #[test]
    fn panel_zone_moves_name_the_other_zone() {
        assert_eq!(PanelZone::Right.move_label(), "Move to bottom");
        assert_eq!(PanelZone::Bottom.move_label(), "Move to right");
        assert_eq!(PanelZone::Right.other(), PanelZone::Bottom);
        assert_eq!(PanelZone::Bottom.other(), PanelZone::Right);
    }

    #[test]
    fn tab_drop_target_uses_the_insertion_slot() {
        let rects: Vec<Rect> = (0..3)
            .map(|index| {
                Rect::from_min_size(Pos2::new(index as f32 * 100.0, 0.0), Vec2::new(90.0, 28.0))
            })
            .collect();
        assert_eq!(tab_drop_target(&rects, 10.0, 1), Some(0));
        assert_eq!(tab_drop_target(&rects, 10.0, 0), None);
        assert_eq!(tab_drop_target(&rects, 150.0, 0), Some(1));
        assert_eq!(tab_drop_target(&rects, 250.0, 0), Some(2));
        assert_eq!(tab_drop_target(&rects, 245.0, 2), None);
        assert_eq!(tab_drop_target(&rects, 120.0, 2), Some(1));
    }

    #[test]
    fn panel_tabs_fill_the_header_with_the_underline_anatomy() {
        assert_eq!(TAB_HEIGHT, PANEL_HEADER_HEIGHT);
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut widths = None;

        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            widths = Some((
                measure_tab(ui, &p, &PanelTab::new("Alpha", Icon::File)),
                measure_tab(ui, &p, &PanelTab::new("Alpha", Icon::File).closable(true)),
                measure_tab(ui, &p, &PanelTab::new("x".repeat(80), Icon::File)),
            ));
        });
        output.textures_delta.clear();

        let (plain, closable, wide) = widths.unwrap();
        let chrome = TAB_SIDE_PAD * 2.0 + TAB_ICON + TAB_LABEL_GAP;
        assert!(plain >= chrome && plain < chrome + 60.0);
        assert_eq!(closable - plain, TAB_CLOSE + TAB_CLOSE_GAP);
        assert_eq!(wide, TAB_MAX_WIDTH);
    }

    #[test]
    fn a_single_surface_is_named_not_tabbed() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let tabs = [PanelTab::new("Context", Icon::Layers)];
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut selected = None;
        let mut closed = None;

        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut result = None;
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    result = Some(panel_header(ui, &p, &tabs, |_| {}, |_| {}));
                });
            });
            output.textures_delta.clear();
            if let Some(result) = result {
                selected = result.select.or(selected);
                closed = result.close.or(closed);
            }
        };

        let point = Pos2::new(30.0, 20.0);
        frame(vec![egui::Event::PointerMoved(point)]);
        frame(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);

        assert_eq!(selected, None);
        assert_eq!(closed, None);
    }

    #[test]
    fn the_header_expand_toggle_uses_the_panel_action_cluster() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let tabs = [PanelTab::new("Context", Icon::Layers)];
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut toggled = false;

        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    panel_header(
                        ui,
                        &p,
                        &tabs,
                        |_| {},
                        |ui| {
                            let _ = close_button(ui, &p);
                            if expand_button(ui, &p, toggled).clicked() {
                                toggled = true;
                            }
                        },
                    );
                });
            });
            output.textures_delta.clear();
        };

        let point = Pos2::new(
            800.0 - PANEL_INSET - PANEL_ACTION * 1.5 - PANEL_ACTIONS_GAP,
            20.0,
        );
        frame(vec![egui::Event::PointerMoved(point)]);
        frame(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);

        assert!(toggled);
    }

    #[test]
    fn the_strip_marks_the_active_tab_with_a_rule_bottom_underline() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let tabs = [
            PanelTab::new("Alpha", Icon::File)
                .active(true)
                .closable(true),
            PanelTab::new("Beta", Icon::File).closable(true),
        ];
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel_header(ui, &p, &tabs, |_| {}, |_| {});
            });
        });
        output.textures_delta.clear();

        let mut shapes = Vec::new();
        for clipped in &output.shapes {
            flatten_shape(&clipped.shape, &mut shapes);
        }
        let rule = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::LineSegment { points, stroke }
                    if stroke.color == p.border && points[0].y == points[1].y =>
                {
                    Some(points[0].y)
                }
                _ => None,
            })
            .expect("header rule");
        let bar = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Rect(rect)
                    if rect.fill == p.primary
                        && (rect.rect.height() - widgets::TAB_UNDERLINE).abs() < 0.01 =>
                {
                    Some(rect.clone())
                }
                _ => None,
            })
            .expect("underline indicator");
        let label = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Text(text)
                    if text.galley.job.sections.first().is_some_and(|section| {
                        (section.format.font_id.size - theme::TEXT_MICRO).abs() < 0.01
                    }) =>
                {
                    Some(text)
                }
                _ => None,
            })
            .expect("micro label");

        let header_bottom = rule + theme::BORDER * 0.5;
        assert!((bar.rect.bottom() - header_bottom).abs() < 0.01);
        assert!((bar.rect.top() - (header_bottom - widgets::TAB_UNDERLINE)).abs() < 0.01);
        assert_eq!(bar.corner_radius.nw, 2);
        assert!(bar.rect.width() > label.galley.size().x + TAB_ICON);
        let label_center = label.pos.y + label.galley.size().y * 0.5;
        assert!(
            label_center > header_bottom - PANEL_HEADER_HEIGHT && label_center < bar.rect.top()
        );
    }

    fn flatten_shape(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(items) => {
                for item in items {
                    flatten_shape(item, out);
                }
            }
            other => out.push(other.clone()),
        }
    }

    #[test]
    fn the_tab_close_glyph_closes_without_selecting() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let tabs = [
            PanelTab::new("Alpha", Icon::File)
                .active(true)
                .closable(true),
            PanelTab::new("Beta", Icon::File).closable(true),
        ];
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut selected = None;
        let mut closed = None;

        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut result = None;
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    result = Some(panel_header(ui, &p, &tabs, |_| {}, |_| {}));
                });
            });
            output.textures_delta.clear();
            if let Some(result) = result {
                selected = result.select.or(selected);
                closed = result.close.or(closed);
            }
        };

        let point = Pos2::new(96.0, 28.0);
        frame(vec![egui::Event::PointerMoved(point)]);
        frame(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);

        assert_eq!(closed, Some(0));
        assert_eq!(selected, None);
    }

    #[test]
    fn dragging_a_panel_tab_reorders_it() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let tabs = [
            PanelTab::new("Alpha", Icon::File).active(true),
            PanelTab::new("Beta", Icon::File),
        ];
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut reorder = None;

        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut result = None;
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    result = Some(panel_header(ui, &p, &tabs, |_| {}, |_| {}));
                });
            });
            output.textures_delta.clear();
            if let Some(result) = result {
                reorder = result.reorder.or(reorder);
            }
        };

        let press = Pos2::new(30.0, 20.0);
        let release = Pos2::new(400.0, 20.0);
        frame(vec![]);
        frame(vec![egui::Event::PointerMoved(press)]);
        frame(vec![egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerMoved(Pos2::new(120.0, 20.0))]);
        frame(vec![egui::Event::PointerMoved(release)]);
        frame(vec![egui::Event::PointerButton {
            pos: release,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);

        assert_eq!(reorder, Some((0, 1)));
    }

    #[test]
    fn panel_tab_click_selects_and_middle_click_closes() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let tabs = [
            PanelTab::new("Alpha", Icon::File)
                .active(true)
                .closable(true),
            PanelTab::new("Beta", Icon::File).closable(true),
        ];
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut selected = None;
        let mut closed = None;

        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut result = None;
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    result = Some(panel_header(ui, &p, &tabs, |_| {}, |_| {}));
                });
            });
            output.textures_delta.clear();
            if let Some(result) = result {
                selected = result.select.or(selected);
                closed = result.close.or(closed);
            }
        };

        let point = Pos2::new(30.0, 20.0);
        frame(vec![egui::Event::PointerMoved(point)]);
        for button in [egui::PointerButton::Primary, egui::PointerButton::Middle] {
            frame(vec![egui::Event::PointerButton {
                pos: point,
                button,
                pressed: true,
                modifiers: Default::default(),
            }]);
            frame(vec![egui::Event::PointerButton {
                pos: point,
                button,
                pressed: false,
                modifiers: Default::default(),
            }]);
        }

        assert_eq!(selected, Some(0));
        assert_eq!(closed, Some(0));
    }
}
