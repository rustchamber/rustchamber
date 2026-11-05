use eframe::egui::{self, Align, Color32, Layout, Rect, Sense, Stroke, Ui, Vec2};
use rchamber_app::Action;

use crate::app::{RustChamber, hover_amount};
use crate::icons::{self, Icon};
use crate::panel_config::{self, RailPanel};
use crate::theme::Palette;

const RAIL_PAD: f32 = 8.0;
const RAIL_BUTTON: f32 = 28.0;
const RAIL_GLYPH: f32 = 17.0;
const RAIL_GAP: f32 = 8.0;
const BADGE: f32 = 6.0;

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let changes = app
        .model
        .active
        .as_ref()
        .map(|chat| chat.diff.len())
        .unwrap_or(0);
    let mut config = panel_config::load(ui.ctx());
    let visible = config.visible_panels();

    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), ui.available_height()),
        Layout::top_down(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.add_space(RAIL_PAD);
            let context = context_ring(ui, app, &p);
            let tip = context_tooltip(app);
            let _ = context.on_hover_text(tip);
            ui.add_space(RAIL_GAP);
            for panel in &visible {
                let surface = panel.surface();
                let active = app.ui.surface == surface && app.ui.context_open;
                let badge = *panel == RailPanel::Changes && changes > 0;
                let response = rail_button(ui, &p, panel.icon(), active, badge)
                    .on_hover_text(format!("{}\n{}", panel.label(), panel.description()));
                if response.clicked() {
                    activate(app, *panel);
                }
                ui.add_space(RAIL_GAP);
            }
            let configure = panel_config::configure_button(ui, &p);
            if configure.on_hover_text("Configure panels").clicked() {
                panel_config::open_dialog(ui.ctx());
            }
            ui.add_space(RAIL_GAP);
        },
    );

    if panel_config::dialog_open(ui.ctx()) {
        panel_config::show_dialog(ui.ctx(), &mut config, &p);
    }
}

fn context_ring(ui: &mut Ui, app: &mut RustChamber, p: &Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(RAIL_BUTTON), Sense::click());
    crate::widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    let active = app.ui.context_open
        && app.ui.surface == crate::surfaces::Surface::Context
        && app.ui.zone == crate::surfaces::PanelZone::Right;
    let color = if active {
        p.primary
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, hovered)
    };
    let center = rect.center();
    let radius = 6.5;
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.6, p.alpha(color, 70)));
    if let Some(fraction) = context_fraction(app) {
        let start = -std::f32::consts::FRAC_PI_2;
        let sweep = std::f32::consts::TAU * fraction.clamp(0.0, 1.0);
        let steps = 32;
        let points: Vec<egui::Pos2> = (0..=steps)
            .map(|index| {
                let angle = start + sweep * index as f32 / steps as f32;
                egui::Pos2::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                )
            })
            .collect();
        if points.len() > 1 {
            ui.painter().add(egui::Shape::line(
                points,
                Stroke::new(1.6, context_tone(p, fraction)),
            ));
        }
    }
    if response.clicked() {
        if active {
            app.ui.context_open = false;
        } else {
            app.ui.surface = crate::surfaces::Surface::Context;
            app.ui.zone = crate::surfaces::PanelZone::Right;
            app.ui.context_open = true;
        }
    }
    response
}

fn context_fraction(app: &RustChamber) -> Option<f32> {
    let chat = app.model.active.as_ref()?;
    let tokens = chat
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            rchamber_api::models::Message::Assistant(assistant) => assistant.tokens.as_ref(),
            _ => None,
        })?;
    let limit = app
        .model
        .current_model()
        .and_then(|model| model.context_limit())
        .unwrap_or(200_000) as f64;
    Some((tokens.context_used() / limit.max(1.0)) as f32)
}

fn context_tone(p: &Palette, fraction: f32) -> Color32 {
    let percent = (fraction * 100.0).round();
    if percent >= 80.0 {
        p.red
    } else if percent >= 50.0 {
        p.coral
    } else {
        p.green
    }
}

fn context_tooltip(app: &RustChamber) -> String {
    let Some(chat) = app.model.active.as_ref() else {
        return "Context".into();
    };
    let used = chat
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            rchamber_api::models::Message::Assistant(assistant) => assistant
                .tokens
                .as_ref()
                .map(|tokens| tokens.context_used()),
            _ => None,
        })
        .unwrap_or(0.0);
    let limit = app
        .model
        .current_model()
        .and_then(|model| model.context_limit())
        .unwrap_or(200_000) as f64;
    format!(
        "Context\nSession context and token usage\nUsed tokens: {}\nContext limit: {}",
        rchamber_app::format::compact_number(used),
        rchamber_app::format::compact_number(limit)
    )
}

fn activate(app: &mut RustChamber, panel: RailPanel) {
    let surface = panel.surface();
    if app.ui.surface == surface && app.ui.context_open {
        app.ui.context_open = false;
        return;
    }
    app.ui.surface = surface;
    app.ui.context_open = true;
    if surface == crate::surfaces::Surface::Changes
        && let Some(id) = app.model.active_id().map(str::to_string)
    {
        app.backend.send(Action::LoadDiff(id));
    }
}

fn rail_button(ui: &mut Ui, p: &Palette, icon: Icon, active: bool, badge: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(RAIL_BUTTON), Sense::click());
    crate::widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    let color = if active {
        p.primary
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, hovered)
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(RAIL_GLYPH)),
        color,
    );
    if badge {
        ui.painter().circle_filled(
            egui::Pos2::new(
                rect.right() - 4.0 - BADGE * 0.5,
                rect.top() + 4.0 + BADGE * 0.5,
            ),
            BADGE * 0.5,
            p.blue,
        );
    }
    response
}
