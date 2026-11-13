use eframe::egui::{
    self, Align, CornerRadius, Layout, Pos2, Rect, Response, ScrollArea, Sense, Stroke, Ui, Vec2,
};
use rchamber_app::Action;
use rchamber_app::workspace::paths_match;
use rchamber_app::ProjectEntry;

use crate::app::{RustChamber, hover_amount};
use crate::icons::{self, Icon};
use crate::panel_config::{self, RailPanel};
use crate::surfaces::Surface;
use crate::theme::{self, Palette};
use crate::widgets;

const RAIL_PAD: f32 = 10.0;
const ITEM: f32 = 28.0;
const ITEM_GAP: f32 = 8.0;
const BOTTOM_GAP: f32 = 6.0;
const GLYPH: f32 = 16.0;
const MONOGRAM: f32 = 11.5;
const BUSY_DOT: f32 = 7.0;
const MARK: f32 = 2.0;

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let edge = ui.max_rect();
    ui.painter().vline(
        edge.left() + theme::BORDER * 0.5,
        edge.y_range(),
        Stroke::new(theme::BORDER, p.border),
    );

    let projects: Vec<ProjectEntry> = app
        .model
        .workspace_projects()
        .into_iter()
        .cloned()
        .collect();
    let active_id = active_project(app);
    let running: Vec<String> = projects
        .iter()
        .filter(|project| project_running(app, &project.id))
        .map(|project| project.id.clone())
        .collect();
    let mut config = panel_config::load(ui.ctx());
    let visible_panels = config.visible_panels();
    let mut dragged = ui.data(|data| data.get_temp::<RailPanel>(surface_drag_id()));
    let mut rows: Vec<(RailPanel, Rect)> = Vec::new();
    let mut reorder: Option<(usize, usize)> = None;
    let bottom_items = 4.0 + if app.updates.available().is_some() { 1.0 } else { 0.0 };
    let bottom_room = RAIL_PAD + bottom_items * ITEM + bottom_items * BOTTOM_GAP;

    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), ui.available_height()),
        Layout::top_down(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.add_space(RAIL_PAD);
            let room = (ui.available_height() - bottom_room).max(ITEM);
            ScrollArea::vertical()
                .id_salt("rail-projects")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, false])
                .max_height(room)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        if projects.is_empty() {
                            empty_item(ui, &p, app);
                            ui.add_space(ITEM_GAP);
                        }
                        for project in &projects {
                            let active = active_id.as_deref() == Some(project.id.as_str());
                            let busy = running.iter().any(|id| id == &project.id);
                            let response = project_item(
                                ui,
                                &p,
                                &monogram(project.display_name()),
                                active,
                                busy,
                                edge.left(),
                            )
                            .on_hover_text(format!(
                                "{}\n{}",
                                project.display_name(),
                                project.canonical
                            ));
                            if response.clicked() {
                                app.select_project(&project.id);
                            }
                            ui.add_space(ITEM_GAP);
                        }

                        let (divider, _) =
                            ui.allocate_exact_size(Vec2::new(20.0, 1.0), Sense::hover());
                        ui.painter().hline(
                            divider.x_range(),
                            divider.center().y,
                            Stroke::new(theme::HAIRLINE, p.border),
                        );
                        ui.add_space(ITEM_GAP);

                        let context_active =
                            app.ui.context_open && app.ui.surface == Surface::Context;
                        let context = surface_item(
                            ui,
                            &p,
                            Icon::ChartBar,
                            context_active,
                            edge.left(),
                            false,
                        )
                        .on_hover_text("Context");
                        if context.clicked() {
                            if context_active {
                                app.ui.context_open = false;
                            } else {
                                app.ui.surface = Surface::Context;
                                app.ui.zone = crate::surfaces::PanelZone::Right;
                                app.ui.context_open = true;
                            }
                        }
                        ui.add_space(ITEM_GAP);
                        for panel in &visible_panels {
                            let active =
                                app.ui.context_open && app.ui.surface == panel.surface();
                            let response = surface_item(
                                ui,
                                &p,
                                panel.icon(),
                                active,
                                edge.left(),
                                dragged == Some(*panel),
                            )
                            .on_hover_text(panel.label());
                            rows.push((*panel, response.rect));
                            if response.drag_started() {
                                dragged = Some(*panel);
                                ui.data_mut(|data| {
                                    data.insert_temp(surface_drag_id(), *panel);
                                });
                            }
                            if response.dragged() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                            }
                            if response.clicked() {
                                activate(app, *panel);
                            }
                            ui.add_space(ITEM_GAP);
                        }

                        if let Some(panel) = dragged
                            && let Some(pointer) =
                                ui.ctx().input(|input| input.pointer.interact_pos())
                        {
                            let inside =
                                rows.first().zip(rows.last()).is_some_and(|(first, last)| {
                                    pointer.y >= first.1.top()
                                        && pointer.y <= last.1.bottom()
                                });
                            if inside
                                && let Some(from) =
                                    rows.iter().position(|row| row.0 == panel)
                                && let Some(target) =
                                    rail_drop_target(&rows, from, pointer.y)
                            {
                                let row = rows[target].1;
                                let y = if target < from {
                                    row.top() - ITEM_GAP * 0.5
                                } else {
                                    row.bottom() + ITEM_GAP * 0.5
                                };
                                paint_rail_drop(ui, &p, row, y);
                                if ui.ctx().input(|input| input.pointer.any_released()) {
                                    reorder = Some((from, target));
                                }
                            }
                        }
                    });
                });

            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.add_space(RAIL_PAD);
                let settings = icon_item(ui, &p, Icon::Settings, app.ui.show_settings)
                    .on_hover_text(format!(
                        "Settings ({})",
                        widgets::shortcut_label(",")
                    ));
                if settings.clicked() {
                    app.open_settings();
                }
                ui.add_space(BOTTOM_GAP);
                extensions_menu(ui, app, &p);
                if app.updates.available().is_some() {
                    ui.add_space(BOTTOM_GAP);
                    crate::update::sidebar_badge(ui, app, &p);
                }
                ui.add_space(BOTTOM_GAP);
                let configure = icon_item(ui, &p, Icon::SlidersHorizontal, false)
                    .on_hover_text("Configure panels…");
                if configure.clicked() {
                    panel_config::open_dialog(ui.ctx());
                }
            });
        },
    );

    if let Some((from, to)) = reorder {
        config.move_to(from, to);
        panel_config::save(ui.ctx(), &config);
    }
    if dragged.is_some() && !ui.ctx().input(|input| input.pointer.any_down()) {
        ui.data_mut(|data| data.remove::<RailPanel>(surface_drag_id()));
    }

    if panel_config::dialog_open(ui.ctx()) {
        let mut config = config;
        panel_config::show_dialog(ui.ctx(), &mut config, &p);
    }
}

fn active_project(app: &RustChamber) -> Option<String> {
    let stored = app
        .model
        .workspace_store
        .active_project_id
        .clone()
        .filter(|id| app.model.project_by_id(id).is_some());
    stored
        .or_else(|| {
            app.model.workspace.as_deref().and_then(|directory| {
                app.model
                    .workspace_projects()
                    .into_iter()
                    .find(|project| paths_match(&project.canonical, directory))
                    .map(|project| project.id.clone())
            })
        })
        .or_else(|| {
            app.model
                .active
                .as_ref()
                .and_then(|chat| app.model.project_for_session(&chat.session))
                .map(|project| project.id.clone())
        })
        .or_else(|| {
            app.model
                .workspace_projects()
                .first()
                .map(|project| project.id.clone())
        })
}

fn session_running(app: &RustChamber, session: &rchamber_api::models::Session) -> bool {
    if let Some(chat) = app
        .model
        .active
        .as_ref()
        .filter(|chat| chat.session.id == session.id)
    {
        return chat.busy;
    }
    match (session.time.busy, session.time.idle) {
        (Some(busy), Some(idle)) => busy > idle,
        (Some(_), None) => true,
        _ => false,
    }
}

fn project_running(app: &RustChamber, project_id: &str) -> bool {
    app.model.sessions.iter().any(|session| {
        session.time.archived.is_none()
            && app
                .model
                .project_for_session(session)
                .is_some_and(|project| project.id == project_id)
            && session_running(app, session)
    })
}

fn monogram(name: &str) -> String {
    let mut initials = String::new();
    for part in name.split(|character: char| !character.is_alphanumeric()) {
        if part.is_empty() {
            continue;
        }
        for (index, character) in part.chars().enumerate() {
            if index == 0 || character.is_uppercase() {
                initials.push(character);
                if initials.chars().count() == 2 {
                    return initials.to_uppercase();
                }
            }
        }
    }
    let leading: String = name
        .chars()
        .filter(|character| character.is_alphanumeric())
        .take(2)
        .collect();
    if leading.is_empty() {
        "·".into()
    } else {
        leading.to_uppercase()
    }
}

fn project_item(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    active: bool,
    busy: bool,
    rail_left: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ITEM), Sense::click());
    widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    let corner = CornerRadius::same(theme::radius::MD);
    if active {
        ui.painter().rect_filled(rect, corner, p.active);
        ui.painter().rect_stroke(
            rect,
            corner,
            Stroke::new(theme::BORDER, p.border),
            egui::StrokeKind::Inside,
        );
        ui.painter().rect_filled(
            Rect::from_center_size(
                Pos2::new(rail_left + theme::BORDER + MARK * 0.5, rect.center().y),
                Vec2::new(MARK, 16.0),
            ),
            CornerRadius::same(1),
            p.primary,
        );
    } else if hovered > 0.01 {
        ui.painter()
            .rect_filled(rect, corner, p.fade(p.hover, hovered));
    }
    let color = if active {
        p.fg
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, hovered)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::font_mono(MONOGRAM), color);
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    if busy {
        let center = Pos2::new(rect.right() - 3.0, rect.top() + 3.0);
        ui.painter()
            .circle_filled(center, BUSY_DOT * 0.5 + 1.5, p.bg_soft);
        ui.painter()
            .circle_filled(center, BUSY_DOT * 0.5, p.primary);
    }
    response
}

fn empty_item(ui: &mut Ui, p: &Palette, app: &mut RustChamber) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ITEM), Sense::click());
    widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    let corner = CornerRadius::same(theme::radius::MD);
    if hovered > 0.01 {
        ui.painter()
            .rect_filled(rect, corner, p.fade(p.hover, hovered));
    }
    crate::logo::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
        p.fg_muted.lerp_to_gamma(p.fg, hovered),
    );
    if response
        .on_hover_text("Add a project")
        .clicked()
    {
        app.open_add_project();
    }
}

fn surface_drag_id() -> egui::Id {
    egui::Id::unique("rail/surface-drag")
}

fn rail_drop_target(rows: &[(RailPanel, Rect)], from: usize, pointer_y: f32) -> Option<usize> {
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

fn paint_rail_drop(ui: &Ui, p: &Palette, row: Rect, y: f32) {
    let left = row.left() - 2.0;
    let right = row.right() + 2.0;
    ui.painter().line_segment(
        [Pos2::new(left, y), Pos2::new(right, y)],
        Stroke::new(2.0, p.primary),
    );
    ui.painter()
        .circle_filled(Pos2::new(left, y), 2.0, p.primary);
    ui.painter()
        .circle_filled(Pos2::new(right, y), 2.0, p.primary);
}

fn surface_item(
    ui: &mut Ui,
    p: &Palette,
    icon: Icon,
    active: bool,
    rail_left: f32,
    dragging: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ITEM), Sense::click_and_drag());
    widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    let corner = CornerRadius::same(theme::radius::MD);
    if active {
        ui.painter().rect_filled(rect, corner, p.active);
        ui.painter().rect_filled(
            Rect::from_center_size(
                Pos2::new(rail_left + theme::BORDER + MARK * 0.5, rect.center().y),
                Vec2::new(MARK, 16.0),
            ),
            CornerRadius::same(1),
            p.primary,
        );
    } else if hovered > 0.01 {
        ui.painter()
            .rect_filled(rect, corner, p.fade(p.hover, hovered));
    }
    if dragging {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            CornerRadius::same(theme::radius::MD + 1),
            Stroke::new(theme::BORDER, p.accent_border),
            egui::StrokeKind::Outside,
        );
    }
    let color = if active {
        p.primary
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, hovered)
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(GLYPH)),
        color,
    );
    response
}

fn icon_item(ui: &mut Ui, p: &Palette, icon: Icon, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ITEM), Sense::click());
    widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    let corner = CornerRadius::same(theme::radius::MD);
    if active {
        ui.painter().rect_filled(rect, corner, p.active);
    } else if hovered > 0.01 {
        ui.painter()
            .rect_filled(rect, corner, p.fade(p.hover, hovered));
    }
    let color = if active {
        p.fg
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, hovered)
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(GLYPH)),
        color,
    );
    response
}

fn extensions_menu(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let entries: Vec<(String, String, Icon, i64)> = app
        .model
        .extensions
        .installed
        .iter()
        .filter(|extension| extension.is_active())
        .filter_map(|extension| {
            let panel = extension.manifest.contributes.panel.as_ref()?;
            Some((
                extension.manifest.id.clone(),
                panel
                    .title
                    .clone()
                    .unwrap_or_else(|| extension.manifest.name.clone()),
                icons::extension_icon(extension.manifest.icon.as_deref()),
                app.model
                    .extensions
                    .badges
                    .get(&extension.manifest.id)
                    .copied()
                    .unwrap_or(0),
            ))
        })
        .collect();
    let active = app.ui.surface == Surface::Extension && app.ui.context_open;
    let trigger = icon_item(ui, p, Icon::Puzzle, active).on_hover_text("Extensions");
    if entries.is_empty() {
        if trigger.clicked() {
            app.model
                .toast(rchamber_app::ToastKind::Info, "No extensions installed");
        }
        return;
    }
    let id = egui::Id::unique("rail-extensions-menu");
    let mut open = ui
        .data(|data| data.get_temp::<bool>(id))
        .unwrap_or(false);
    let mut items: Vec<Response> = Vec::new();
    let mut chosen: Option<String> = None;
    crate::app::menu_popup(ui.ctx(), &trigger, id, &mut open, p, 6.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(212.0);
        for (id, title, icon, badge) in &entries {
            let selected = app.ui.extension_panel.as_deref() == Some(id.as_str());
            let label = if *badge > 0 {
                format!("{title}  ({badge})")
            } else {
                title.clone()
            };
            let response = crate::widgets::menu_item_ex(
                ui,
                &label,
                Some(*icon),
                None,
                selected,
                true,
                p,
            );
            if response.clicked() {
                chosen = Some(id.clone());
            }
            items.push(response);
        }
    });
    crate::app::menu_nav_focus(ui.ctx(), id, open, &items);
    if let Some(id) = chosen {
        activate_extension(app, &id);
        open = false;
    }
    if trigger.clicked() {
        open = !open;
    }
    ui.data_mut(|data| data.insert_temp(id, open));
}

fn activate(app: &mut RustChamber, panel: RailPanel) {
    let surface = panel.surface();
    if app.ui.surface == surface && app.ui.context_open {
        app.ui.context_open = false;
        return;
    }
    app.ui.surface = surface;
    app.ui.zone = crate::surfaces::PanelZone::Right;
    app.ui.context_open = true;
    if surface == Surface::Changes
        && let Some(id) = app.model.active_id().map(str::to_string)
    {
        app.backend.send(Action::LoadDiff(id));
    }
}

fn activate_extension(app: &mut RustChamber, id: &str) {
    if app.ui.surface == Surface::Extension
        && app.ui.extension_panel.as_deref() == Some(id)
        && app.ui.context_open
    {
        app.ui.context_open = false;
        return;
    }
    app.ui.surface = Surface::Extension;
    app.ui.extension_panel = Some(id.to_string());
    app.ui.zone = crate::surfaces::PanelZone::Right;
    app.ui.context_open = true;
    app.backend.send(Action::ExtensionCall {
        id: id.to_string(),
        call_id: "panel".into(),
        method: "panel".into(),
        args: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(panel: RailPanel, top: f32) -> (RailPanel, Rect) {
        (
            panel,
            Rect::from_min_size(Pos2::new(0.0, top), Vec2::new(ITEM, ITEM)),
        )
    }

    #[test]
    fn drop_target_moves_below_the_hovered_center() {
        let rows = [
            row(RailPanel::Git, 0.0),
            row(RailPanel::Changes, 36.0),
            row(RailPanel::Files, 72.0),
            row(RailPanel::Terminal, 108.0),
        ];
        assert_eq!(rail_drop_target(&rows, 0, 130.0), Some(3));
        assert_eq!(rail_drop_target(&rows, 3, 10.0), Some(0));
        assert_eq!(rail_drop_target(&rows, 1, 40.0), None);
        assert_eq!(rail_drop_target(&[], 0, 0.0), None);
    }
}
