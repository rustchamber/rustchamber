use eframe::egui::{
    self, Color32, Pos2, Rangef, Rect, Response, Sense, TextStyle, Ui, Vec2, WidgetText,
};

use crate::app::header::HEADER_HEIGHT;
use crate::app::{RustChamber, hover_amount};
use crate::icons::{self, Icon};
use crate::theme::{self, Palette};

pub const TRAFFIC_LIGHTS_INSET: f32 = 96.0;
pub const TRAFFIC_LIGHTS_LEFT: f32 = 16.0;
pub const TITLEBAR_MARGIN: f32 = 12.0;
pub const CONTROLS_SIZE: f32 = 24.0;
pub const CONTROLS_GAP: f32 = 10.0;
pub const NEW_SESSION_LABEL: &str = "New session";
pub const GLYPH: f32 = 16.0;

const CLUSTER_GAP: f32 = 8.0;
const LABEL_PAD_START: f32 = 4.0;
const LABEL_PAD_END: f32 = 8.0;
const END_PAD: f32 = 8.0;

pub fn native_chrome() -> bool {
    cfg!(target_os = "macos")
}

fn start_inset(ctx: &egui::Context) -> f32 {
    let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    if native_chrome() && !fullscreen {
        TRAFFIC_LIGHTS_INSET
    } else {
        TITLEBAR_MARGIN
    }
}

pub fn new_session_width(ctx: &egui::Context) -> f32 {
    let galley = ctx.fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            NEW_SESSION_LABEL.to_string(),
            theme::font_medium(theme::TEXT_UI_LABEL),
            Color32::WHITE,
        )
    });
    LABEL_PAD_START + GLYPH + CONTROLS_GAP + galley.size().x + LABEL_PAD_END
}

pub fn cluster_width(ctx: &egui::Context, sidebar_open: bool) -> f32 {
    let new = if sidebar_open {
        new_session_width(ctx)
    } else {
        CONTROLS_SIZE
    };
    CONTROLS_SIZE + CLUSTER_GAP + new
}

pub fn header_leading(ui: &Ui, exposure: f32) -> f32 {
    let start = start_inset(ui.ctx());
    let closed = (start + cluster_width(ui.ctx(), false) + END_PAD - ui.max_rect().left()).max(0.0);
    closed * (1.0 - exposure.clamp(0.0, 1.0))
}

pub fn sidebar_strip(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), HEADER_HEIGHT),
        Sense::hover(),
    );
    let start = start_inset(ui.ctx()) + cluster_width(ui.ctx(), true) + END_PAD;
    if rect.width() <= start + 4.0 {
        return;
    }
    let drag = Rect::from_min_max(Pos2::new(rect.left() + start, rect.top()), rect.max);
    let response = ui.interact(
        drag,
        ui.scope_id().with("sidebar-strip"),
        Sense::click_and_drag(),
    );
    drag_window(ui, &response);
}

pub fn header_drag(ui: &mut Ui, right_limit: f32) {
    let rect = ui.max_rect();
    let rect = Rect::from_min_max(
        rect.min,
        egui::Pos2::new(
            right_limit.clamp(rect.left() + 1.0, rect.right()),
            rect.bottom(),
        ),
    );
    let response = ui.interact(
        rect,
        ui.scope_id().with("titlebar-drag"),
        Sense::click_and_drag(),
    );
    drag_window(ui, &response);
}

pub fn titlebar_controls(ctx: &egui::Context, app: &mut RustChamber, p: &Palette) {
    let x = start_inset(ctx);
    let y = (HEADER_HEIGHT * 0.5 - CONTROLS_SIZE * 0.5).round();
    let open = !app.ui.sidebar_collapsed;
    let cap = if open {
        (app.settings.sidebar_width.clamp(264.0, 500.0) - x - END_PAD).max(CONTROLS_SIZE)
    } else {
        f32::INFINITY
    };
    let mut toggle = false;
    let mut create = false;
    egui::Area::new(egui::Id::unique("titlebar-controls"))
        .fixed_pos(egui::pos2(x, y))
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(CLUSTER_GAP, 0.0);
            ui.horizontal(|ui| {
                let button = control(ui, p, Icon::Sidebar, None, open, CONTROLS_SIZE);
                toggle = button.on_hover_text("Toggle sidebar (⌘B)").clicked();
                let room = (cap - CONTROLS_SIZE - CLUSTER_GAP).max(CONTROLS_SIZE);
                let label = open.then_some(NEW_SESSION_LABEL);
                let button = control(ui, p, Icon::Plus, label, false, room);
                create = if open {
                    button.clicked()
                } else {
                    button.on_hover_text("New session (⌘N)").clicked()
                };
            });
        });
    if toggle {
        app.ui.sidebar_collapsed = !app.ui.sidebar_collapsed;
        app.ui.sidebar_folded_by_zone = false;
    }
    if create {
        app.create_session(None);
    }
}

fn control(
    ui: &mut Ui,
    p: &Palette,
    icon: Icon,
    label: Option<&str>,
    active: bool,
    max_width: f32,
) -> Response {
    let width = if label.is_some() {
        new_session_width(ui.ctx()).min(max_width)
    } else {
        CONTROLS_SIZE.min(max_width)
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, CONTROLS_SIZE), Sense::click());
    let hovered = hover_amount(ui.ctx(), response.id, response.hovered());
    let color = if active {
        p.fg
    } else {
        p.fg_muted.lerp_to_gamma(p.fg, hovered)
    };
    let icon_center = if label.is_some() {
        Pos2::new(rect.left() + LABEL_PAD_START + GLYPH * 0.5, rect.center().y)
    } else {
        rect.center()
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(icon_center, Vec2::splat(GLYPH)),
        color,
    );
    if let Some(text) = label {
        let room = (rect.width() - LABEL_PAD_START - GLYPH - CONTROLS_GAP - LABEL_PAD_END).max(0.0);
        if room > 1.0 {
            let galley = WidgetText::from(
                egui::RichText::new(text)
                    .font(theme::font_medium(theme::TEXT_UI_LABEL))
                    .color(color),
            )
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                room,
                TextStyle::Button,
            );
            let left = rect.left() + LABEL_PAD_START + GLYPH + CONTROLS_GAP;
            ui.painter().galley(
                Pos2::new(left, rect.center().y - galley.size().y * 0.5),
                galley,
                color,
            );
        }
    }
    response
}

pub fn place_traffic_lights(frame: &eframe::Frame, ctx: &egui::Context) {
    if !native_chrome() {
        return;
    }
    frame.set_traffic_lights_position(ctx, Rangef::new(0.0, HEADER_HEIGHT), TRAFFIC_LIGHTS_LEFT);
}

fn drag_window(ui: &Ui, response: &egui::Response) {
    if response.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if response.double_clicked_by(egui::PointerButton::Primary) {
        let maximized = ui
            .ctx()
            .input(|input| input.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}
