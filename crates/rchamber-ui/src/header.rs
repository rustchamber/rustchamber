use eframe::egui::{
    self, Color32, Pos2, Rect, Response, RichText, Sense, Stroke, TextStyle, Ui, Vec2, WidgetText,
};
use rchamber_api::models::Message;
use rchamber_app::format::money;
use rchamber_app::{Action, ToastKind};

use crate::app::{RustChamber, hover_amount, menu_nav_focus, menu_popup};
use crate::chrome;
use crate::icons::{self, Icon};
use crate::surfaces::{PanelZone, Surface};
use crate::theme::{self, Palette};
use crate::widgets;

pub const HEADER_HEIGHT: f32 = 48.0;

const BLOCK_PAD: f32 = 4.0;
const TITLE_SIZE: f32 = 14.0;
const MENU_W: f32 = 24.0;
const MENU_H: f32 = 18.0;
const MENU_GAP: f32 = 2.0;
const CLUSTER_GAP: f32 = 8.0;
const BUTTON: f32 = 24.0;
const RING: f32 = 16.0;
const ACTION_GAP: f32 = 12.0;

struct Reading {
    fraction: Option<f32>,
    used: f64,
    limit: f64,
    cost: f64,
}

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let leading = chrome::header_leading(ui, app.ui.sidebar_exposure);
    let row = ui.max_rect();

    let reading = reading(app);
    let cluster = cluster_width(ui, &p, reading.as_ref());
    let left = row.left() + leading + BLOCK_PAD;
    let right = (row.right() - cluster - ACTION_GAP).max(left + 48.0);
    crate::chrome::header_drag(ui, right);

    let session = app.model.active.as_ref().map(|chat| {
        let project = app
            .model
            .project_for_session(&chat.session)
            .map(|project| project.display_name().to_string())
            .or_else(|| {
                chat.session.directory().and_then(|directory| {
                    std::path::Path::new(directory)
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                })
            });
        (chat.session.id.clone(), chat.title().to_string(), project)
    });
    let title = session
        .as_ref()
        .map(|(_, title, _)| title.as_str())
        .unwrap_or("New session");
    let menu_open = app.ui.header_menu;

    let menu_room = if session.is_some() {
        MENU_W + MENU_GAP
    } else {
        0.0
    };
    let title_room = (right - left - menu_room).max(40.0);
    let subtitle = session
        .as_ref()
        .and_then(|(_, _, project)| project.as_deref());
    let title_center = if subtitle.is_some() {
        row.center().y - 7.0
    } else {
        row.center().y
    };
    let title_galley = elide(ui, title, theme::font(TITLE_SIZE), p.fg, title_room);
    let title_end = left + title_galley.size().x;
    ui.painter().galley(
        Pos2::new(left, title_center - title_galley.size().y * 0.5),
        title_galley,
        p.fg,
    );
    if let Some(project) = subtitle {
        let subtitle_galley = elide(
            ui,
            project,
            theme::font(theme::TEXT_MICRO),
            p.fg_muted,
            title_room,
        );
        ui.painter().galley(
            Pos2::new(left, row.center().y + 9.0 - subtitle_galley.size().y * 0.5),
            subtitle_galley,
            p.fg_muted,
        );
    }

    if let Some((id, title, _)) = session.as_ref() {
        let rect = Rect::from_center_size(
            Pos2::new(title_end + MENU_GAP + MENU_W * 0.5, title_center),
            Vec2::new(MENU_W, MENU_H),
        );
        let trigger = ui.interact(
            rect,
            egui::Id::unique(("header-session-menu", id)),
            Sense::click(),
        );
        crate::widgets::pointer(ui, &trigger);
        paint_menu_trigger(ui, &p, rect, id, trigger.hovered(), menu_open);
        session_menu(
            ui,
            app,
            &p,
            MenuTarget {
                id,
                title,
                open: menu_open,
            },
            trigger,
        );
    }

    paint_actions(ui, app, &p, row, reading.as_ref());
}

struct MenuTarget<'a> {
    id: &'a str,
    title: &'a str,
    open: bool,
}

fn reading(app: &RustChamber) -> Option<Reading> {
    let chat = app.model.active.as_ref()?;
    let tokens = chat
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::Assistant(assistant) => assistant.tokens.as_ref(),
            _ => None,
        })?;
    let limit = app
        .model
        .current_model()
        .and_then(|model| model.context_limit())
        .unwrap_or(200_000) as f64;
    let used = tokens.context_used();
    let fraction = (used / limit.max(1.0)) as f32;
    Some(Reading {
        fraction: Some(fraction),
        used,
        limit,
        cost: chat.session.cost,
    })
}

fn format_tokens(tokens: f64) -> String {
    if tokens >= 1_000_000.0 {
        format!("{:.1}M", tokens / 1_000_000.0)
    } else if tokens >= 1_000.0 {
        format!("{:.1}K", tokens / 1_000.0)
    } else {
        format!("{}", tokens.round() as i64)
    }
}

fn percent_label(reading: &Reading) -> String {
    match reading.fraction {
        Some(fraction) => format!("{:.1}%", (fraction * 100.0).min(999.0)),
        None => "—".into(),
    }
}

fn tone(p: &Palette, percent: Option<f32>) -> Color32 {
    match percent {
        Some(percent) if percent >= 80.0 => p.red,
        Some(percent) if percent >= 50.0 => p.coral,
        _ => p.green,
    }
}

fn usage_width(ui: &Ui, p: &Palette, reading: &Reading) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        percent_label(reading),
        theme::font_medium(theme::TEXT_META),
        p.fg_muted,
    );
    4.0 + RING + 6.0 + galley.size().x + 4.0
}

fn cluster_width(ui: &Ui, p: &Palette, reading: Option<&Reading>) -> f32 {
    let mut width = BUTTON + CLUSTER_GAP + BUTTON;
    if let Some(reading) = reading {
        width += CLUSTER_GAP + usage_width(ui, p, reading);
    }
    width
}

fn paint_actions(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    row: Rect,
    reading: Option<&Reading>,
) {
    let center_y = row.center().y;
    let mut right = row.right();

    let toggle_rect = Rect::from_center_size(
        Pos2::new(right - BUTTON * 0.5, center_y),
        Vec2::splat(BUTTON),
    );
    right -= BUTTON + CLUSTER_GAP;
    let toggle = ui.interact(
        toggle_rect,
        egui::Id::unique("header-work-status"),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &toggle);
    paint_icon(
        ui,
        p,
        Icon::Layers,
        toggle_rect,
        app.ui.context_open && app.ui.surface == Surface::Session,
        &toggle,
    );
    if toggle.on_hover_text("Session").clicked() {
        open_session_panel(&mut app.ui);
    }

    let project_rect = Rect::from_center_size(
        Pos2::new(right - BUTTON * 0.5, center_y),
        Vec2::splat(BUTTON),
    );
    right -= BUTTON + CLUSTER_GAP;
    let project = ui.interact(
        project_rect,
        egui::Id::unique("header-project-controls"),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &project);
    paint_icon(ui, p, Icon::ArrowUpRight, project_rect, false, &project);
    if project.on_hover_text("Reveal project folder").clicked() {
        let directory = app.workspace_directory();
        open_directory(&directory);
    }

    if let Some(reading) = reading {
        let width = usage_width(ui, p, reading);
        let rect = Rect::from_min_max(
            Pos2::new(right - width, center_y - BUTTON * 0.5),
            Pos2::new(right, center_y + BUTTON * 0.5),
        );
        usage(ui, app, p, rect, reading);
    }
}

fn open_session_panel(ui: &mut crate::app::UiState) {
    let open = ui.context_open && ui.surface == Surface::Session && ui.zone == PanelZone::Right;
    ui.surface = Surface::Session;
    ui.zone = PanelZone::Right;
    ui.context_open = !open;
}

fn open_directory(directory: &str) {
    let _ = crate::widgets::reveal_in_finder(std::path::Path::new(directory));
}

fn usage(ui: &mut Ui, app: &mut RustChamber, p: &Palette, rect: Rect, reading: &Reading) {
    let response = ui.interact(
        rect,
        egui::Id::unique("header-context-usage"),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id, response.hovered());
    let open = app.ui.context_open
        && app.ui.surface == Surface::Context
        && app.ui.zone == crate::surfaces::PanelZone::Right;
    let color = if open {
        p.fg
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, hovered)
    };
    let center = Pos2::new(rect.left() + 4.0 + RING * 0.5, rect.center().y);
    let radius = (RING - 2.0) * 0.5;
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.8, p.alpha(color, 77)));
    if let Some(fraction) = reading.fraction {
        let start = -std::f32::consts::FRAC_PI_2;
        let sweep = std::f32::consts::TAU * fraction.clamp(0.0, 1.0);
        let steps = 36;
        let points: Vec<Pos2> = (0..=steps)
            .map(|index| {
                let angle = start + sweep * index as f32 / steps as f32;
                Pos2::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                )
            })
            .collect();
        if points.len() > 1 {
            ui.painter().add(egui::Shape::line(
                points,
                Stroke::new(1.8, tone(p, Some((fraction * 100.0).round()))),
            ));
        }
    }
    let galley = ui.painter().layout_no_wrap(
        percent_label(reading),
        theme::font_medium(theme::TEXT_META),
        color,
    );
    ui.painter().galley(
        Pos2::new(
            rect.left() + 4.0 + RING + 6.0,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    let mut tip = String::new();
    if reading.fraction.is_none() {
        tip.push_str("Context compacted. Usage updates after the next response.");
        tip.push('\n');
    } else {
        tip.push_str(&format!("Used tokens: {}", format_tokens(reading.used)));
        tip.push('\n');
    }
    tip.push_str(&format!("Context limit: {}", format_tokens(reading.limit)));
    if reading.cost > 0.0 {
        tip.push('\n');
        tip.push_str(&format!("Cost: {}", money(reading.cost)));
    }
    if !open {
        tip = format!("Open context overview\n{tip}");
    }
    if response.on_hover_text(tip).clicked() {
        if open {
            app.ui.context_open = false;
        } else {
            app.ui.surface = Surface::Context;
            app.ui.zone = crate::surfaces::PanelZone::Right;
            app.ui.context_open = true;
        }
    }
}

fn paint_icon(ui: &Ui, p: &Palette, icon: Icon, rect: Rect, active: bool, response: &Response) {
    let amount = hover_amount(ui.ctx(), response.id.with("paint"), response.hovered());
    let color = if active {
        p.fg
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, amount)
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
        color,
    );
}

fn elide(
    ui: &Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    WidgetText::from(RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        width,
        TextStyle::Body,
    )
}

fn paint_menu_trigger(ui: &mut Ui, p: &Palette, rect: Rect, id: &str, hovered: bool, open: bool) {
    let amount = hover_amount(
        ui.ctx(),
        egui::Id::unique(("header-menu-hover", id)),
        hovered,
    );
    let color = if open {
        p.fg
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, amount)
    };
    icons::paint(
        ui,
        Icon::More,
        Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
        color,
    );
}

fn session_menu(
    ui: &Ui,
    app: &mut RustChamber,
    p: &Palette,
    target: MenuTarget,
    trigger: Response,
) {
    let id = egui::Id::unique(("header-session-menu-popup", target.id));
    let mut open = target.open;
    let mut items: Vec<Response> = Vec::new();
    menu_popup(ui.ctx(), &trigger, id, &mut open, p, 4.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(236.0);
        let response = widgets::menu_item(ui, "Rename", Some(Icon::Pencil), None, p);
        if response.clicked() {
            app.ui.rename = Some((target.id.to_string(), target.title.to_string()));
            app.ui.header_menu = false;
        }
        items.push(response);
        let response = widgets::menu_item_ex(
            ui,
            "Rename with AI",
            Some(Icon::Spark),
            None,
            false,
            false,
            p,
        );
        let response = response.on_hover_text("No small model configured");
        items.push(response);
        let response = widgets::menu_item(ui, "Copy session ID", Some(Icon::Copy), None, p);
        if response.clicked() {
            ui.ctx().copy_text(target.id.to_string());
            app.model.toast(ToastKind::Success, "Session ID copied");
            app.ui.header_menu = false;
        }
        items.push(response);
        ui.add_space(2.0);
        widgets::hline(ui, p, 4.0);
        ui.add_space(2.0);
        let response = widgets::menu_item(ui, "Export Markdown", Some(Icon::Download), None, p);
        if response.clicked() {
            if let Some(session) = app
                .model
                .sessions
                .iter()
                .find(|session| session.id == target.id)
                .cloned()
            {
                crate::sidebar::export_markdown(app, &session);
            }
            app.ui.header_menu = false;
        }
        items.push(response);
        let response = widgets::menu_item_ex(
            ui,
            "Open in Mini Chat",
            Some(Icon::MessageSquare),
            None,
            false,
            false,
            p,
        );
        let response = response.on_hover_text("Mini Chat is not available yet");
        items.push(response);
        let response = widgets::menu_item_ex(
            ui,
            "Move to a new worktree",
            Some(Icon::Branch),
            None,
            false,
            false,
            p,
        );
        let response = response.on_hover_text("Worktrees are not available yet");
        items.push(response);
        let response = widgets::menu_item(ui, "Move session…", Some(Icon::FolderInput), None, p);
        if response.clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                let directory = folder.to_string_lossy().to_string();
                app.backend.send(Action::MoveSession {
                    id: target.id.to_string(),
                    directory,
                });
                app.model.toast(ToastKind::Info, "Moving session…");
            }
            app.ui.header_menu = false;
        }
        items.push(response);
        ui.add_space(2.0);
        widgets::hline(ui, p, 4.0);
        ui.add_space(2.0);
        let response = widgets::menu_item(ui, "Archive", Some(Icon::Archive), None, p);
        if response.clicked() {
            app.backend.send(Action::ArchiveSession {
                id: target.id.to_string(),
                archived: true,
            });
            app.model.clear_active();
            app.ui.header_menu = false;
        }
        items.push(response);
        let response =
            widgets::menu_item_destructive(ui, "Delete", Some(Icon::Trash), None, true, p);
        if response.clicked() {
            app.ui.confirm_delete = Some(target.id.to_string());
            app.ui.header_menu = false;
        }
        items.push(response);
    });
    menu_nav_focus(ui.ctx(), id, open, &items);
    if trigger.clicked() {
        app.ui.header_menu = !app.ui.header_menu;
    } else if !open {
        app.ui.header_menu = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_session_action_opens_the_session_panel_on_the_right() {
        let mut ui = crate::app::UiState {
            surface: Surface::Files,
            zone: PanelZone::Bottom,
            context_open: false,
            ..crate::app::UiState::default()
        };
        open_session_panel(&mut ui);
        assert!(ui.context_open);
        assert_eq!(ui.surface, Surface::Session);
        assert_eq!(ui.zone, PanelZone::Right);
    }

    #[test]
    fn header_session_action_switches_an_open_panel_to_the_session_surface() {
        let mut ui = crate::app::UiState {
            surface: Surface::Git,
            zone: PanelZone::Bottom,
            context_open: true,
            ..crate::app::UiState::default()
        };
        open_session_panel(&mut ui);
        assert!(ui.context_open);
        assert_eq!(ui.surface, Surface::Session);
        assert_eq!(ui.zone, PanelZone::Right);
    }

    #[test]
    fn header_session_action_closes_the_session_panel_when_it_is_already_shown() {
        let mut ui = crate::app::UiState {
            surface: Surface::Session,
            zone: PanelZone::Right,
            context_open: true,
            ..crate::app::UiState::default()
        };
        open_session_panel(&mut ui);
        assert!(!ui.context_open);
        assert_eq!(ui.surface, Surface::Session);
        assert_eq!(ui.zone, PanelZone::Right);
    }
}
