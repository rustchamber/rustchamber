use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, Id, Layout, Pos2, Rect, Response, Sense,
    Stroke, Ui, Vec2,
};

use crate::app::hover_amount;
use crate::icons::Icon;
use crate::surfaces::Surface;
use crate::theme::{self, Palette};
use crate::widgets;

pub const DIALOG_TITLE: &str = "Rail panels";
pub const DIALOG_DESCRIPTION: &str = "Choose which panels the rail shows. Hidden panels keep their data and stay reachable from the command palette.";
pub const DIALOG_WIDTH: f32 = 448.0;
pub const ROW_HEIGHT: f32 = 44.0;
pub const ROW_PADDING: f32 = 12.0;
pub const ROW_RADIUS: u8 = 6;
pub const CONFIGURE_BUTTON: f32 = 28.0;

const ROW_GAP: f32 = 2.0;
const HANDLE_WIDTH: f32 = 18.0;
const HANDLE_DOT: f32 = 1.6;
const HANDLE_GAP: f32 = 10.0;
const HEADER_GAP: f32 = 6.0;
const CLOSE_GAP: f32 = 12.0;
const DROP_LINE: f32 = 2.0;

const SLIDERS_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="21" y1="4" x2="14" y2="4"/><line x1="10" y1="4" x2="3" y2="4"/><line x1="21" y1="12" x2="12" y2="12"/><line x1="8" y1="12" x2="3" y2="12"/><line x1="21" y1="20" x2="16" y2="20"/><line x1="12" y1="20" x2="3" y2="20"/><line x1="14" y1="2" x2="14" y2="6"/><line x1="8" y1="10" x2="8" y2="14"/><line x1="16" y1="18" x2="16" y2="22"/></svg>"##;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RailPanel {
    Files,
    Git,
    Changes,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelGroup {
    Changes,
    Tools,
}

impl RailPanel {
    pub const ALL: [RailPanel; 4] = [
        RailPanel::Git,
        RailPanel::Changes,
        RailPanel::Files,
        RailPanel::Terminal,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RailPanel::Files => "Files",
            RailPanel::Git => "Git",
            RailPanel::Changes => "Changes",
            RailPanel::Terminal => "Terminal",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            RailPanel::Files => "Edit project files",
            RailPanel::Git => "Commits, branches, and pull requests",
            RailPanel::Changes => "Review working changes",
            RailPanel::Terminal => "Built-in terminal",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            RailPanel::Files => Icon::File,
            RailPanel::Git => Icon::Branch,
            RailPanel::Changes => Icon::Diff,
            RailPanel::Terminal => Icon::Terminal,
        }
    }

    pub fn surface(self) -> Surface {
        match self {
            RailPanel::Files => Surface::Files,
            RailPanel::Git => Surface::Git,
            RailPanel::Changes => Surface::Changes,
            RailPanel::Terminal => Surface::Terminal,
        }
    }

    pub fn group(self) -> PanelGroup {
        match self {
            RailPanel::Git | RailPanel::Changes => PanelGroup::Changes,
            RailPanel::Files | RailPanel::Terminal => PanelGroup::Tools,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PanelConfig {
    order: Vec<RailPanel>,
    hidden: Vec<RailPanel>,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            order: RailPanel::ALL.to_vec(),
            hidden: Vec::new(),
        }
    }
}

impl PanelConfig {
    pub fn sanitized(mut self) -> Self {
        let mut order = Vec::with_capacity(RailPanel::ALL.len());
        for panel in self.order.drain(..) {
            if !order.contains(&panel) {
                order.push(panel);
            }
        }
        for panel in RailPanel::ALL {
            if !order.contains(&panel) {
                order.push(panel);
            }
        }
        let mut hidden = Vec::new();
        for panel in self.hidden {
            if order.contains(&panel) && !hidden.contains(&panel) {
                hidden.push(panel);
            }
        }
        self.order = order;
        self.hidden = hidden;
        self
    }

    pub fn is_visible(&self, panel: RailPanel) -> bool {
        !self.hidden.contains(&panel)
    }

    pub fn set_visible(&mut self, panel: RailPanel, visible: bool) {
        if visible {
            self.hidden.retain(|hidden| *hidden != panel);
        } else if !self.hidden.contains(&panel) {
            self.hidden.push(panel);
        }
    }

    pub fn toggle(&mut self, panel: RailPanel) {
        let visible = self.is_visible(panel);
        self.set_visible(panel, !visible);
    }

    pub fn show_all(&mut self) {
        self.hidden.clear();
    }

    pub fn visible_panels(&self) -> Vec<RailPanel> {
        self.order
            .iter()
            .copied()
            .filter(|panel| self.is_visible(*panel))
            .collect()
    }

    pub fn move_to(&mut self, from: usize, to: usize) {
        if from >= self.order.len() || to >= self.order.len() || from == to {
            return;
        }
        let panel = self.order.remove(from);
        self.order.insert(to, panel);
    }
}

pub fn config_id() -> Id {
    Id::unique("rail/panels/configuration.v2")
}

fn dialog_id() -> Id {
    Id::unique("rail/panels/dialog")
}

fn open_id() -> Id {
    Id::unique("rail/panels/dialog-open")
}

fn drag_id() -> Id {
    Id::unique("rail/panels/drag")
}

pub fn load(ctx: &egui::Context) -> PanelConfig {
    ctx.data_mut(|data| data.get_persisted::<PanelConfig>(config_id()))
        .unwrap_or_default()
        .sanitized()
}

pub fn save(ctx: &egui::Context, config: &PanelConfig) {
    ctx.data_mut(|data| data.insert_persisted(config_id(), config.clone()));
}

pub fn dialog_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(open_id()))
        .unwrap_or(false)
}

pub fn open_dialog(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(open_id(), true));
    ctx.request_repaint();
}

pub fn close_dialog(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(open_id(), false));
    ctx.data_mut(|data| data.remove::<RailPanel>(drag_id()));
}

pub fn configure_button(ui: &mut Ui, p: &Palette) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    let hover = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    paint_sliders(
        ui,
        Rect::from_center_size(rect.center(), Vec2::splat(15.0)),
        p.fg_muted.lerp_to_gamma(p.fg, hover),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response
}

pub fn show_dialog(ctx: &egui::Context, config: &mut PanelConfig, p: &Palette) {
    if !dialog_open(ctx) {
        return;
    }
    widgets::dialog_backdrop(ctx, p, 1.0);

    let mut close = false;
    let mut toggle = None;
    let mut reorder = None;
    let mut show_all = false;
    let mut dragged = ctx.data(|data| data.get_temp::<RailPanel>(drag_id()));
    let mut rows: Vec<(RailPanel, Rect)> = Vec::new();

    let modal = egui::Modal::new(dialog_id())
        .frame(theme::dialog_frame(p))
        .backdrop_color(Color32::TRANSPARENT)
        .show(ctx, |ui| {
            ui.set_width(DIALOG_WIDTH);
            let content_width =
                (ui.available_width() - widgets::DIALOG_CLOSE_SIZE - CLOSE_GAP * 2.0).max(120.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = CLOSE_GAP;
                ui.vertical(|ui| {
                    ui.set_width(content_width);
                    widgets::dialog_title(ui, DIALOG_TITLE, p);
                    ui.add_space(HEADER_GAP);
                    widgets::dialog_description(ui, DIALOG_DESCRIPTION, p);
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if widgets::dialog_close(ui, p).clicked() {
                        close = true;
                    }
                });
            });

            ui.add_space(widgets::DIALOG_GAP);
            ui.spacing_mut().item_spacing.y = ROW_GAP;

            for (index, panel) in config.order.iter().copied().enumerate() {
                let group_start = index > 0 && config.order[index - 1].group() != panel.group();
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), ROW_HEIGHT),
                    Sense::hover(),
                );
                rows.push((panel, rect));
                if group_start {
                    ui.painter().hline(
                        (rect.left() + ROW_PADDING)..=(rect.right() - ROW_PADDING),
                        rect.top() - ROW_GAP * 0.5,
                        Stroke::new(theme::HAIRLINE, p.edge_muted()),
                    );
                }
                if ui.rect_contains_pointer(rect) {
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(ROW_RADIUS), p.hover);
                }
                let inner = Rect::from_min_max(
                    Pos2::new(rect.left() + ROW_PADDING, rect.top()),
                    Pos2::new(rect.right() - ROW_PADDING, rect.bottom()),
                );
                let mut row_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                row_ui.spacing_mut().item_spacing.x = HANDLE_GAP;

                let (handle_rect, handle) = row_ui.allocate_exact_size(
                    Vec2::new(HANDLE_WIDTH, ROW_HEIGHT),
                    Sense::click_and_drag(),
                );
                paint_handle(
                    &row_ui,
                    p,
                    handle_rect,
                    dragged == Some(panel),
                    handle.hovered(),
                );
                if handle.hovered() {
                    row_ui.ctx().set_cursor_icon(CursorIcon::Grab);
                }
                if handle.dragged() {
                    row_ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                }
                if handle.drag_started() {
                    dragged = Some(panel);
                    ctx.data_mut(|data| data.insert_temp(drag_id(), panel));
                }

                let mut checked = config.is_visible(panel);
                if widgets::checkbox(&mut row_ui, &mut checked, p).clicked() {
                    toggle = Some(panel);
                }

                let label_rect = Rect::from_min_max(
                    row_ui.cursor().min,
                    Pos2::new(row_ui.max_rect().right(), row_ui.max_rect().bottom()),
                );
                let label = row_ui.interact(
                    label_rect,
                    row_ui.scope_id().with(("panel-label", index)),
                    Sense::click(),
                );
                let color = if checked { p.fg } else { p.fg_muted };
                let galley = row_ui.painter().layout_no_wrap(
                    panel.label().to_string(),
                    theme::font(theme::TEXT_UI_LABEL),
                    color,
                );
                row_ui.painter().galley(
                    Pos2::new(
                        label_rect.left(),
                        label_rect.center().y - galley.size().y * 0.5,
                    ),
                    galley,
                    color,
                );
                if label.clicked() {
                    toggle = Some(panel);
                }
                if label.hovered() {
                    row_ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
            }

            if let Some(panel) = dragged
                && let Some(pointer) = ctx.input(|input| input.pointer.interact_pos())
            {
                let inside = rows.first().zip(rows.last()).is_some_and(|(first, last)| {
                    pointer.y >= first.1.top() && pointer.y <= last.1.bottom()
                });
                if inside
                    && let Some(from) = rows.iter().position(|row| row.0 == panel)
                    && let Some(target) = drop_target(&rows, from, pointer.y)
                {
                    let row = rows[target].1;
                    let y = if target < from {
                        row.top() - ROW_GAP * 0.5
                    } else {
                        row.bottom() + ROW_GAP * 0.5
                    };
                    paint_drop_line(ui, p, row, y);
                    if ctx.input(|input| input.pointer.any_released()) {
                        reorder = Some((from, target));
                    }
                }
            }

            if !config.hidden.is_empty() {
                ui.add_space(8.0);
                theme::divider(ui, p);
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::link_button(ui, "Show all", None, p).clicked() {
                        show_all = true;
                    }
                });
            }
        });

    if modal.should_close() {
        close = true;
    }

    let mut changed = false;
    if let Some(panel) = toggle {
        config.toggle(panel);
        changed = true;
    }
    if let Some((from, to)) = reorder {
        config.move_to(from, to);
        changed = true;
    }
    if show_all {
        config.show_all();
        changed = true;
    }
    if changed {
        save(ctx, config);
    }
    if dragged.is_some() && !ctx.input(|input| input.pointer.any_down()) {
        ctx.data_mut(|data| data.remove::<RailPanel>(drag_id()));
    }
    if close {
        close_dialog(ctx);
    }
}

fn drop_target(rows: &[(RailPanel, Rect)], from: usize, pointer_y: f32) -> Option<usize> {
    if rows.is_empty() || from >= rows.len() {
        return None;
    }
    let slot = rows
        .iter()
        .take_while(|row| pointer_y > row.1.center().y)
        .count();
    let target = if slot > from { slot - 1 } else { slot }.min(rows.len() - 1);
    (target != from).then_some(target)
}

fn paint_sliders(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::from_bytes(
        "bytes://rustchamber/configure-panels.svg",
        SLIDERS_SVG.as_bytes(),
    )
    .tint(color)
    .paint_at(ui, rect);
}

fn paint_handle(ui: &Ui, p: &Palette, rect: Rect, active: bool, hovered: bool) {
    let color = if active || hovered { p.fg } else { p.fg_muted };
    let center = rect.center();
    for row in -1..=1 {
        for column in -1..=1 {
            if column == 0 {
                continue;
            }
            ui.painter().circle_filled(
                Pos2::new(center.x + column as f32 * 4.0, center.y + row as f32 * 5.0),
                HANDLE_DOT,
                color,
            );
        }
    }
}

fn paint_drop_line(ui: &Ui, p: &Palette, row: Rect, y: f32) {
    let left = row.left() + ROW_PADDING;
    let right = row.right() - ROW_PADDING;
    ui.painter().line_segment(
        [Pos2::new(left, y), Pos2::new(right, y)],
        Stroke::new(DROP_LINE, p.primary),
    );
    ui.painter()
        .circle_filled(Pos2::new(left, y), DROP_LINE, p.primary);
    ui.painter()
        .circle_filled(Pos2::new(right, y), DROP_LINE, p.primary);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_configuration_keeps_every_panel_visible() {
        let config = PanelConfig::default();
        assert_eq!(config.order, RailPanel::ALL.to_vec());
        assert_eq!(config.visible_panels(), RailPanel::ALL.to_vec());
        assert!(config.hidden.is_empty());
    }

    #[test]
    fn hidden_panels_keep_their_order_slot() {
        let mut config = PanelConfig::default();
        config.set_visible(RailPanel::Git, false);
        assert!(!config.is_visible(RailPanel::Git));
        assert!(!config.visible_panels().contains(&RailPanel::Git));
        assert_eq!(config.order[0], RailPanel::Git);
        config.set_visible(RailPanel::Git, true);
        assert_eq!(config.visible_panels(), RailPanel::ALL.to_vec());
    }

    #[test]
    fn dragging_reorders_in_both_directions() {
        let mut config = PanelConfig::default();
        config.move_to(0, 3);
        assert_eq!(config.order[3], RailPanel::Git);
        assert_eq!(config.order[0], RailPanel::Changes);
        config.move_to(3, 0);
        assert_eq!(config.order, RailPanel::ALL.to_vec());
        config.move_to(9, 0);
        assert_eq!(config.order, RailPanel::ALL.to_vec());
    }

    #[test]
    fn sanitize_removes_duplicates_and_appends_missing_panels() {
        let mut config = PanelConfig::default();
        config.order.truncate(3);
        config.order.push(RailPanel::Git);
        config.hidden = vec![RailPanel::Git, RailPanel::Git, RailPanel::Terminal];
        let config = config.sanitized();
        assert_eq!(config.order.len(), RailPanel::ALL.len());
        assert_eq!(
            config
                .order
                .iter()
                .filter(|panel| **panel == RailPanel::Git)
                .count(),
            1
        );
        assert_eq!(config.hidden, vec![RailPanel::Git, RailPanel::Terminal]);
    }

    #[test]
    fn configuration_round_trips_through_egui_memory() {
        let ctx = egui::Context::default();
        let mut config = PanelConfig::default();
        config.move_to(0, 2);
        config.set_visible(RailPanel::Terminal, false);
        save(&ctx, &config);
        assert_eq!(load(&ctx), config);
    }

    #[test]
    fn show_all_restores_hidden_panels() {
        let mut config = PanelConfig::default();
        config.set_visible(RailPanel::Git, false);
        config.set_visible(RailPanel::Files, false);
        config.show_all();
        assert_eq!(config.visible_panels(), RailPanel::ALL.to_vec());
    }

    #[test]
    fn dialog_copy_and_memory_keys_are_stable() {
        assert_eq!(DIALOG_TITLE, "Rail panels");
        assert_eq!(
            DIALOG_DESCRIPTION,
            "Choose which panels the rail shows. Hidden panels keep their data and stay reachable from the command palette."
        );
        assert_eq!(config_id(), Id::unique("rail/panels/configuration.v2"));
    }

    #[test]
    fn drop_target_uses_the_insertion_slot() {
        let rows: Vec<(RailPanel, Rect)> = (0..4)
            .map(|index| {
                (
                    RailPanel::ALL[index],
                    Rect::from_min_size(
                        Pos2::new(0.0, index as f32 * 46.0),
                        Vec2::new(300.0, ROW_HEIGHT),
                    ),
                )
            })
            .collect();
        assert_eq!(drop_target(&rows, 0, 80.0), Some(1));
        assert_eq!(drop_target(&rows, 3, 10.0), Some(0));
        assert_eq!(drop_target(&rows, 1, 200.0), Some(3));
        assert_eq!(drop_target(&rows, 2, 22.0), Some(0));
        assert_eq!(drop_target(&rows, 1, 70.0), None);
        assert_eq!(drop_target(&rows, 9, 70.0), None);
    }

    #[test]
    fn panels_map_to_the_surfaces_the_app_has() {
        assert_eq!(RailPanel::Files.surface(), Surface::Files);
        assert_eq!(RailPanel::Git.surface(), Surface::Git);
        assert_eq!(RailPanel::Changes.surface(), Surface::Changes);
        assert_eq!(RailPanel::Terminal.surface(), Surface::Terminal);
    }
}
