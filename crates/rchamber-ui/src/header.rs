use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Response, RichText, Sense, Stroke, TextStyle, Ui,
    Vec2, WidgetText,
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

pub const HEADER_HEIGHT: f32 = 40.0;

const TITLE_SIZE: f32 = 13.5;
const MENU_W: f32 = 20.0;
const MENU_H: f32 = 18.0;
const MENU_GAP: f32 = 5.0;
const CHIP_GAP: f32 = 6.0;
const BUTTON: f32 = 24.0;
const RING: f32 = 16.0;
const ACTION_GAP: f32 = 12.0;
const TAB_HEIGHT: f32 = 26.0;
const TAB_PAD: f32 = 9.0;
const TAB_GAP: f32 = 4.0;
const TAB_FONT: f32 = 12.5;
const DOT: f32 = 5.0;
const TITLE_ROOM_MIN: f32 = 96.0;

struct Reading {
    fraction: Option<f32>,
    used: f64,
    limit: f64,
    cost: f64,
}

struct TabSpec {
    label: &'static str,
    surface: Surface,
    count: usize,
}

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let leading = chrome::header_leading(ui, app.ui.sidebar_exposure);
    let row = ui.max_rect();
    ui.painter().hline(
        row.x_range(),
        row.bottom() - theme::BORDER * 0.5,
        Stroke::new(theme::BORDER, p.border),
    );

    let reading = reading(app);
    let session = app.model.active.as_ref().map(|chat| {
        (
            chat.session.id.clone(),
            chat.title().to_string(),
            chat.awaiting_permission,
        )
    });
    let left = row.left() + leading;
    let approval = session.as_ref().is_some_and(|(_, _, pending)| *pending);

    let mut cluster_left = row.right();
    let mut tabs: Vec<(Rect, TabSpec, bool)> = Vec::new();
    if session.is_some() {
        let specs = [
            TabSpec {
                label: "Chat",
                surface: Surface::Session,
                count: 0,
            },
            TabSpec {
                label: "Changes",
                surface: Surface::Changes,
                count: app
                    .model
                    .active
                    .as_ref()
                    .map(|chat| chat.diff.len())
                    .unwrap_or(0),
            },
            TabSpec {
                label: "Terminal",
                surface: Surface::Terminal,
                count: 0,
            },
        ];
        let widths: Vec<f32> = specs
            .iter()
            .map(|spec| tab_width(ui, spec, app.ui.surface == spec.surface))
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + TAB_GAP * (specs.len() - 1) as f32;
        let mut x = row.right() - total;
        for (spec, width) in specs.into_iter().zip(widths) {
            let rect = Rect::from_min_size(
                Pos2::new(x, row.center().y - TAB_HEIGHT * 0.5),
                Vec2::new(width, TAB_HEIGHT),
            );
            tabs.push((rect, spec, app.ui.context_open));
            x += width + TAB_GAP;
        }
        cluster_left = row.right() - total;
    } else if row.width() > 760.0 {
        let chips = [
            (
                format!("{} Command palette", widgets::shortcut_label("K")),
                true,
            ),
            (
                format!("{} Open project", widgets::shortcut_label("O")),
                false,
            ),
        ];
        let mut x = row.right();
        for (label, palette) in chips.iter().rev() {
            let width = tag_width(ui, label);
            let rect = Rect::from_min_size(
                Pos2::new(x - width, row.center().y - theme::TAG_HEIGHT * 0.5),
                Vec2::new(width, theme::TAG_HEIGHT),
            );
            paint_tag(ui, rect, label, p.fg_muted, p.tag_bg);
            let hit = ui.interact(
                rect,
                egui::Id::unique(("header-home-chip", label.as_str())),
                Sense::click(),
            );
            widgets::pointer(ui, &hit);
            if hit.clicked() {
                if *palette {
                    app.ui.palette_open = true;
                } else {
                    app.open_add_project();
                }
            }
            x = rect.left() - CHIP_GAP;
        }
        cluster_left = x + CHIP_GAP;
    }

    let usage_room = reading
        .as_ref()
        .map(|value| usage_width(ui, &p, value) + ACTION_GAP)
        .unwrap_or(0.0);
    let project_button = app
        .model
        .workspace
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || app
            .model
            .active
            .as_ref()
            .and_then(|chat| chat.session.directory())
            .is_some();
    let extras = usage_room + if project_button { BUTTON } else { 0.0 } + ACTION_GAP;
    let show_extras = cluster_left - extras > left + TITLE_ROOM_MIN;
    if show_extras {
        let mut cursor = cluster_left;
        if project_button {
            let project_rect = Rect::from_center_size(
                Pos2::new(cursor - BUTTON * 0.5, row.center().y),
                Vec2::splat(BUTTON),
            );
            cursor -= BUTTON + ACTION_GAP;
            let project = ui.interact(
                project_rect,
                egui::Id::unique("header-project-controls"),
                Sense::click(),
            );
            widgets::pointer(ui, &project);
            paint_icon(ui, &p, Icon::ArrowUpRight, project_rect, false, &project);
            if project.on_hover_text("Reveal project folder").clicked() {
                let directory = app.workspace_directory();
                open_directory(&directory);
            }
        }
        if let Some(reading) = reading.as_ref() {
            let width = usage_width(ui, &p, reading);
            let rect = Rect::from_min_max(
                Pos2::new(cursor - width, row.center().y - RING * 0.5),
                Pos2::new(cursor, row.center().y + RING * 0.5),
            );
            usage(ui, app, &p, rect, reading);
            cursor -= width + ACTION_GAP;
        }
        cluster_left = cursor;
    }

    let title = session
        .as_ref()
        .map(|(_, title, _)| title.as_str())
        .unwrap_or("Welcome to RustChamber");
    let mut chip_room = 0.0;
    if let Some((id, _, _)) = session.as_ref() {
        chip_room += CHIP_GAP + tag_width(ui, &format!("Session #{}", short_id(id)));
        if approval {
            chip_room += CHIP_GAP + tag_width(ui, "Awaiting tool approval");
        }
        chip_room += MENU_GAP + MENU_W + CHIP_GAP;
    } else {
        chip_room += CHIP_GAP + tag_width(ui, "Local-first workspace");
    }
    let room = (cluster_left - left - ACTION_GAP * 1.5 - chip_room).clamp(40.0, 520.0);
    let title_galley = elide(ui, title, theme::font_semibold(TITLE_SIZE), p.fg, room);
    let title_end = left + title_galley.size().x;

    let mut x = title_end;
    let mut chips: Vec<(Rect, String, bool)> = Vec::new();
    if let Some((id, _, _)) = session.as_ref() {
        let label = format!("Session #{}", short_id(id));
        let width = tag_width(ui, &label);
        let rect = Rect::from_min_size(
            Pos2::new(x + CHIP_GAP, row.center().y - theme::TAG_HEIGHT * 0.5),
            Vec2::new(width, theme::TAG_HEIGHT),
        );
        x = rect.right();
        chips.push((rect, label, false));
        if approval {
            let label = "Awaiting tool approval".to_string();
            let width = tag_width(ui, &label);
            let rect = Rect::from_min_size(
                Pos2::new(x + CHIP_GAP, row.center().y - theme::TAG_HEIGHT * 0.5),
                Vec2::new(width, theme::TAG_HEIGHT),
            );
            x = rect.right();
            chips.push((rect, label, true));
        }
    } else {
        let label = "Local-first workspace".to_string();
        let width = tag_width(ui, &label);
        let rect = Rect::from_min_size(
            Pos2::new(x + CHIP_GAP, row.center().y - theme::TAG_HEIGHT * 0.5),
            Vec2::new(width, theme::TAG_HEIGHT),
        );
        x = rect.right();
        chips.push((rect, label, false));
    }

    let menu_rect = Rect::from_center_size(
        Pos2::new(x + MENU_GAP + MENU_W * 0.5, row.center().y),
        Vec2::new(MENU_W, MENU_H),
    );
    let title_block = Rect::from_min_max(
        Pos2::new(left - 2.0, row.top()),
        Pos2::new(
            if session.is_some() {
                menu_rect.right()
            } else {
                x
            },
            row.bottom(),
        ),
    );
    chrome::header_drag(ui, cluster_left - 4.0, title_block);
    ui.painter().galley(
        Pos2::new(left, row.center().y - title_galley.size().y * 0.5),
        title_galley,
        p.fg,
    );
    for (rect, label, warning) in &chips {
        if rect.right() > cluster_left - 4.0 {
            continue;
        }
        if *warning {
            paint_tag(ui, *rect, label, p.warning_text, p.warning_soft);
        } else {
            paint_tag(ui, *rect, label, p.fg_muted, p.tag_bg);
        }
    }

    if let Some((id, title, _)) = session.as_ref()
        && menu_rect.right() <= cluster_left - 4.0
    {
        let trigger = ui.interact(
            menu_rect,
            egui::Id::unique(("header-session-menu", id)),
            Sense::click(),
        );
        widgets::pointer(ui, &trigger);
        paint_menu_trigger(ui, &p, menu_rect, id, trigger.hovered(), app.ui.header_menu);
        session_menu(
            ui,
            app,
            &p,
            MenuTarget {
                id,
                title,
                open: app.ui.header_menu,
            },
            trigger,
        );
    }

    for (rect, spec, opened) in tabs {
        let active = opened && app.ui.surface == spec.surface;
        let response = paint_tab(ui, &p, rect, &spec, active);
        let clicked = response
            .on_hover_text(match spec.surface {
                Surface::Session => "Conversation".to_string(),
                Surface::Changes => "Review working changes".to_string(),
                _ => "Built-in terminal".to_string(),
            })
            .clicked();
        if clicked
            && select_surface(&mut app.ui, spec.surface)
            && spec.surface == Surface::Changes
            && let Some(id) = app.model.active_id().map(str::to_string)
        {
            app.backend.send(Action::LoadDiff(id));
        }
    }
}

fn short_id(id: &str) -> String {
    let trimmed = id.trim_start_matches("ses_");
    trimmed.chars().take(4).collect()
}

fn tab_width(ui: &Ui, spec: &TabSpec, active: bool) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        spec.label.to_string(),
        theme::font_medium(TAB_FONT),
        Color32::PLACEHOLDER,
    );
    let dot = if active { DOT + 5.0 } else { 0.0 };
    let count = if spec.count > 0 {
        let galley = ui.painter().layout_no_wrap(
            spec.count.to_string(),
            theme::font_mono(theme::TEXT_TAG),
            Color32::PLACEHOLDER,
        );
        galley.size().x + 10.0 + 6.0
    } else {
        0.0
    };
    TAB_PAD * 2.0 + dot + galley.size().x + count
}

fn paint_tab(ui: &mut Ui, p: &Palette, rect: Rect, spec: &TabSpec, active: bool) -> Response {
    let response = ui.interact(
        rect,
        egui::Id::unique(("header-tab", spec.label)),
        Sense::click(),
    );
    widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    let corner = CornerRadius::same(theme::radius::SM);
    if active {
        ui.painter().rect_filled(rect, corner, p.card);
        ui.painter().rect_stroke(
            rect,
            corner,
            Stroke::new(theme::BORDER, p.border),
            egui::StrokeKind::Inside,
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
    let mut x = rect.left() + TAB_PAD;
    if active {
        ui.painter().circle_filled(
            Pos2::new(x + DOT * 0.5, rect.center().y),
            DOT * 0.5,
            p.primary,
        );
        x += DOT + 5.0;
    }
    let galley =
        ui.painter()
            .layout_no_wrap(spec.label.to_string(), theme::font_medium(TAB_FONT), color);
    ui.painter().galley(
        Pos2::new(x, rect.center().y - galley.size().y * 0.5),
        galley.clone(),
        color,
    );
    x += galley.size().x;
    if spec.count > 0 {
        let count = spec.count.to_string();
        let galley = ui.painter().layout_no_wrap(
            count.clone(),
            theme::font_mono(theme::TEXT_TAG),
            p.fg_muted,
        );
        let chip = Rect::from_min_size(
            Pos2::new(x + 6.0, rect.center().y - theme::TAG_HEIGHT * 0.5),
            Vec2::new(galley.size().x + 10.0, theme::TAG_HEIGHT),
        );
        ui.painter()
            .rect_filled(chip, CornerRadius::same(theme::radius::XS), p.tag_bg);
        ui.painter().galley(
            Pos2::new(
                chip.center().x - galley.size().x * 0.5,
                chip.center().y - galley.size().y * 0.5,
            ),
            galley,
            p.fg_muted,
        );
    }
    response
}

fn tag_width(ui: &Ui, label: &str) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        theme::font_mono(theme::TEXT_TAG),
        Color32::PLACEHOLDER,
    );
    galley.size().x + 12.0
}

fn paint_tag(ui: &Ui, rect: Rect, label: &str, fg: Color32, bg: Color32) {
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::XS), bg);
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), theme::font_mono(theme::TEXT_TAG), fg);
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        fg,
    );
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

fn select_surface(ui: &mut crate::app::UiState, surface: Surface) -> bool {
    if !ui.context_open {
        ui.zone = PanelZone::Right;
    }
    let changed = ui.surface != surface;
    ui.surface = surface;
    ui.context_open = true;
    changed
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
    widgets::pointer(ui, &response);
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
        .circle_stroke(center, radius, Stroke::new(1.6, p.alpha(color, 77)));
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
                Stroke::new(1.6, tone(p, Some((fraction * 100.0).round()))),
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
    if hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::SM),
            p.fade(p.hover, amount),
        );
    }
    let color = if open {
        p.fg
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, amount)
    };
    icons::paint(
        ui,
        Icon::More,
        Rect::from_center_size(rect.center(), Vec2::splat(14.0)),
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
    fn stage_tab_opens_the_panel_on_the_right() {
        let mut ui = crate::app::UiState {
            context_open: false,
            zone: PanelZone::Bottom,
            ..crate::app::UiState::default()
        };
        assert!(select_surface(&mut ui, Surface::Changes));
        assert!(ui.context_open);
        assert_eq!(ui.surface, Surface::Changes);
        assert_eq!(ui.zone, PanelZone::Right);
    }

    #[test]
    fn stage_tab_keeps_an_open_panel_in_place() {
        let mut ui = crate::app::UiState {
            context_open: true,
            zone: PanelZone::Bottom,
            ..crate::app::UiState::default()
        };
        select_surface(&mut ui, Surface::Terminal);
        assert!(ui.context_open);
        assert_eq!(ui.surface, Surface::Terminal);
        assert_eq!(ui.zone, PanelZone::Bottom);
    }

    #[test]
    fn stage_tab_stays_open_when_it_is_already_active() {
        let mut ui = crate::app::UiState {
            context_open: true,
            zone: PanelZone::Right,
            surface: Surface::Session,
            ..crate::app::UiState::default()
        };
        assert!(!select_surface(&mut ui, Surface::Session));
        assert!(ui.context_open);
        assert_eq!(ui.surface, Surface::Session);
    }

    #[test]
    fn short_session_ids_are_compact() {
        assert_eq!(short_id("ses_2f1a3b4c"), "2f1a");
        assert_eq!(short_id(""), "");
    }
}
