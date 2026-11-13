use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::epaint::{Vertex, WHITE_UV};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Margin, Mesh, Pos2, Rect,
    Response, RichText, ScrollArea, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};
use rchamber_api::models::{
    AssistantMessage, Attachment, Message, ModelRef, Part, Provider, RetryInfo, ToolPart,
    ToolState, UserMessage,
};
use rchamber_app::format::{time_label, tool_detail, tool_label};
use rchamber_app::{Action, now_ms};
use rchamber_ext::ToolRenderer;

use crate::app::RustChamber;
use crate::composer;
use crate::icons::{self, Icon};
use crate::markdown::Markdown;
use crate::motion;
use crate::theme::{self, Palette};

const COLUMN_WIDTH: f32 = 760.0;
const USER_BUBBLE_MAX: f32 = 0.85;
const USER_PAD_X: f32 = 14.0;
const USER_PAD_Y: f32 = 10.0;
const TEXT_USER: f32 = theme::TEXT_MARKDOWN;
const ACTIONS_ROW: f32 = 22.0;
const ACTION_SIZE: f32 = 22.0;
const ACTION_ICON: f32 = 12.0;
const ROW_HEIGHT: f32 = 28.0;
const ROW_HEIGHT_NARROW: f32 = 28.0;
const ROW_WIDE_MIN: f32 = 640.0;
const ROW_ICON: f32 = 14.0;
const ROW_GAP: f32 = 6.0;
const ROW_PAD_END: f32 = 8.0;
const ROW_TITLE: f32 = 12.5;
const ROW_TEXT: f32 = 12.5;
const FOLD_TITLE: f32 = theme::TEXT_UI_LABEL;
const FOLD_HEIGHT: f32 = 28.0;
const FOLD_ROWS: usize = 7;
const DISCLOSURE: f32 = 0.2;
const REASONING_ROW: f32 = 26.0;
const REASONING_INDENT: f32 = 12.0;
const REASONING_BAR: f32 = 2.0;
const REASONING_RISE: f32 = 4.0;
const REASONING_RISE_TIME: f32 = 0.180;
const REASONING_MAX: f32 = 320.0;
const OUTPUT_MAX: f32 = 320.0;
const DIFF_LINE: f32 = 22.0;
const EDGE_FADE: f32 = 48.0;
const COMPOSER_FADE: f32 = 64.0;
const REARM: f32 = 40.0;
const PILL_HEIGHT: f32 = 28.0;
const PILL_GAP: f32 = 8.0;
const PILL_LABEL_MAX: f32 = 240.0;
const PILL_LABEL_PAD_END: f32 = 8.0;
const COMPOSER_BOTTOM_PAD: f32 = 16.0;
const STATUS_MIN_DISPLAY: f64 = 1.2;
const STATUS_MARK: f32 = 16.0;
const STATUS_MARK_INITIAL: f32 = 10.0;
const STATUS_PAD_START: f32 = 6.0;
const STATUS_PAD_END: f32 = 8.0;
const STATUS_GAP: f32 = 8.0;
const SESSION_SWITCH_HOLD: f64 = 0.4;
const TAIL_GAP: f32 = 80.0;

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let Some(session_id) = app.model.active_id().map(str::to_string) else {
        crate::home::show(ui, app);
        return;
    };
    let p = app.palette.clone();
    let area_rect = ui.max_rect();
    app.md.scale = app.settings.appearance.chat_scale;
    set_tool_renderers(ui.ctx(), tool_renderers(app));
    set_message_actions(ui.ctx(), message_actions(app));
    let host = block_host(app, &session_id);
    crate::blocks::set_host(ui.ctx(), host);

    let outcome = egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(p.bg))
        .show(ui, |ui| transcript(ui, app, &p, &session_id, area_rect))
        .inner;

    composer::floating(ui, app, Some(session_id.clone()), false, area_rect);
    app.md.scale = 1.0;

    let dock = dock_metrics(ui.ctx(), &session_id, area_rect);
    scroll_pill(ui.ctx(), app, &p, &session_id, dock, outcome.away);
    status_chip(ui.ctx(), app, &p, &session_id, dock, outcome.away);
}

#[derive(Clone)]
struct MessageActionSlot {
    actions: Vec<(String, String, String)>,
    sender: rchamber_app::ActionSink,
}

fn message_actions(app: &RustChamber) -> MessageActionSlot {
    let actions = app
        .model
        .extensions
        .installed
        .iter()
        .filter(|extension| extension.is_active())
        .flat_map(|extension| {
            let id = extension.manifest.id.clone();
            extension
                .manifest
                .contributes
                .actions
                .iter()
                .filter(|action| {
                    action.target == "message"
                        && (action.roles.is_empty()
                            || action.roles.iter().any(|role| role == "assistant"))
                })
                .map(move |action| (id.clone(), action.id.clone(), action.label.clone()))
        })
        .collect();
    MessageActionSlot {
        actions,
        sender: app.backend.sink(),
    }
}

fn set_message_actions(ctx: &egui::Context, slot: MessageActionSlot) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique("message-actions"), slot));
}

fn assistant_text(assistant: &AssistantMessage) -> String {
    let mut text = String::new();
    for part in &assistant.content {
        let Part::Text(part) = part else {
            continue;
        };
        if part.text.trim().is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&part.text);
    }
    text
}

fn block_host(app: &RustChamber, session_id: &str) -> crate::blocks::BlockHost {
    use std::sync::Arc;
    let actions = app.backend.actions.clone();
    let send_actions = actions.clone();
    let send_session = session_id.to_string();
    let tree_blocks = app.model.extensions.blocks.clone();
    let tree_actions = actions.clone();
    crate::blocks::BlockHost {
        submit: Some(Arc::new(move |response: rchamber_ext::BlockResponse| {
            let text = response.summary();
            if text.trim().is_empty() {
                return;
            }
            let _ = send_actions.send(rchamber_app::Action::Send {
                session_id: send_session.clone(),
                text,
                files: Vec::new(),
                delivery: "steer".into(),
            });
        })),
        request: Some(Arc::new(move |name: &str, args: &[String]| {
            let _ = tree_actions.send(rchamber_app::Action::ExtensionBlock {
                name: name.to_string(),
                args: args.to_vec(),
            });
        })),
        tree: Some(Arc::new(move |name: &str, args: &[String]| {
            let key = format!("{name}:{}", args.join("\u{1f}"));
            tree_blocks.get(&key).cloned()
        })),
    }
}

#[derive(Clone, Copy, Default)]
struct Outcome {
    away: bool,
}

#[derive(Clone)]
struct Timeline {
    session_id: String,
    messages: Vec<Message>,
    has_more: bool,
    error: Option<String>,
    busy: bool,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct TimelineKey {
    session: String,
    len: usize,
    last: Option<String>,
    has_more: bool,
    error: usize,
    busy: bool,
}

#[derive(Clone, Default)]
struct TimelineCache {
    session: String,
    key: TimelineKey,
    timeline: Option<Arc<Timeline>>,
    switch_at: f64,
    refreshed_at: f64,
    waited: bool,
    reveal_at: Option<f64>,
}

impl TimelineCache {
    fn capture(&mut self, chat: &rchamber_app::state::Chat, now: f64) {
        let key = TimelineKey {
            session: chat.session.id.clone(),
            len: chat.messages.len(),
            last: chat
                .messages
                .last()
                .and_then(|message| message.id().map(str::to_string)),
            has_more: chat.has_more,
            error: chat.error.as_ref().map_or(0, String::len),
            busy: chat.busy,
        };
        if self.key == key && !(chat.busy && now - self.refreshed_at >= 0.25) {
            return;
        }
        self.refreshed_at = now;
        self.timeline = Some(Arc::new(Timeline {
            session_id: chat.session.id.clone(),
            messages: chat.messages.clone(),
            has_more: chat.has_more,
            error: chat.error.clone(),
            busy: chat.busy,
        }));
        self.key = key;
    }

    fn held(&self, session_id: &str, loading: bool, now: f64) -> Option<Arc<Timeline>> {
        hold_previous(
            loading,
            self.waited,
            now - self.switch_at,
            self.timeline.as_deref(),
            session_id,
        )
        .then(|| self.timeline.clone())
        .flatten()
    }

    fn store(&self, ui: &Ui, id: Id) {
        ui.memory_mut(|memory| memory.data.insert_temp(id, self.clone()));
    }
}

fn hold_previous(
    loading: bool,
    waited: bool,
    age: f64,
    timeline: Option<&Timeline>,
    session_id: &str,
) -> bool {
    loading
        && waited
        && age < SESSION_SWITCH_HOLD
        && timeline.is_some_and(|timeline| {
            timeline.session_id != session_id && !timeline.messages.is_empty()
        })
}

fn reveal_alpha(elapsed: f64, reduced: bool) -> f32 {
    if reduced {
        return 1.0;
    }
    motion::EASE_OUT.apply((elapsed / motion::REVEAL as f64) as f32)
}

#[derive(Clone, Copy)]
struct Dock {
    left: f32,
    top: f32,
}

fn dock_metrics(ctx: &egui::Context, session_id: &str, area: Rect) -> Dock {
    let reserved = composer::reserved_height(ctx, session_id, false);
    let (left, _) = column_geometry(area.width(), wide_layout(ctx));
    Dock {
        left: area.left() + left,
        top: (composer_top(area.bottom(), reserved) - PILL_GAP).max(area.top()),
    }
}

fn composer_top(area_bottom: f32, reserved: f32) -> f32 {
    area_bottom - COMPOSER_BOTTOM_PAD - reserved
}

fn pill_origin(dock: Dock) -> Pos2 {
    Pos2::new(dock.left, dock.top - PILL_HEIGHT)
}

fn transcript(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    session_id: &str,
    area_rect: Rect,
) -> Outcome {
    let Some(chat) = &app.model.active else {
        return Outcome::default();
    };
    let loading = chat.loading;
    let now = ui.input(|input| input.time);
    let cache_id = Id::unique("chat-timeline");
    let mut cache = ui
        .memory(|memory| memory.data.get_temp::<TimelineCache>(cache_id))
        .unwrap_or_default();
    if cache.session != session_id {
        cache.session = session_id.to_string();
        cache.switch_at = now;
        cache.waited = loading;
        cache.reveal_at = None;
    }
    if !loading {
        cache.capture(chat, now);
    }
    let held = cache.held(session_id, loading, now);

    if held.is_none() && loading {
        cache.store(ui, cache_id);
        loading_rows(ui, p);
        return Outcome::default();
    }

    let pin_id = Id::unique(("chat-pin", session_id));
    let follow_id = Id::unique(("chat-follow", session_id));
    let offset_id = Id::unique(("chat-offset", session_id));

    let (has_more, requested, busy) = match &app.model.active {
        Some(chat) => (chat.has_more, chat.scroll_to_bottom, chat.busy),
        None => return Outcome::default(),
    };
    let pinned =
        requested || ui.memory(|memory| memory.data.get_temp::<bool>(pin_id).unwrap_or(false));
    let mut following =
        pinned || ui.memory(|memory| memory.data.get_temp::<bool>(follow_id).unwrap_or(true));
    let previous_offset = ui
        .memory(|memory| memory.data.get_temp::<f32>(offset_id))
        .unwrap_or(-1.0);

    let mut load_older = false;
    let mut open_file: Option<String> = None;
    let mut approvals: Vec<Action> = Vec::new();
    let mut md = std::mem::take(&mut app.md);

    if let Some(timeline) = &held {
        ui.ctx().request_repaint_after(Duration::from_secs_f64(
            (SESSION_SWITCH_HOLD - (now - cache.switch_at)).max(0.01),
        ));
        held_timeline(ui, p, &mut md, timeline, session_id, area_rect);
        app.md = md;
        cache.store(ui, cache_id);
        return Outcome::default();
    }

    let alpha = if cache.waited {
        let start = *cache.reveal_at.get_or_insert(now);
        let value = reveal_alpha(now - start, motion::reduced_motion(ui.ctx()));
        if value >= 1.0 {
            cache.waited = false;
            cache.reveal_at = None;
        }
        value
    } else {
        1.0
    };
    if alpha < 1.0 {
        ui.ctx().request_repaint();
    }

    let output = {
        let model = &app.model;
        let scroll = ScrollArea::vertical()
            .id_salt(("chat-scroll", session_id))
            .auto_shrink([false, false])
            .stick_to_bottom(following);
        scroll.show(ui, |ui| {
            if alpha < 0.999 {
                ui.set_opacity(alpha);
            }
            centered_column(ui, |ui| {
                list_header(ui, p, has_more, &mut load_older);
                let Some(chat) = &model.active else {
                    return;
                };
                let messages = visible_messages(&chat.messages);
                if messages.is_empty() {
                    empty_state(ui, p);
                } else {
                    let turns = build_turns(&messages);
                    let retired = retired_turns(&turns);
                    for (index, turn) in turns.iter().enumerate() {
                        turn_view(ui, p, &mut md, turn, retired[index], &mut open_file);
                    }
                }
                if let Some(error) = &chat.error {
                    ui.add_space(12.0);
                    error_card(ui, p, &mut md, Some("Run failed"), error);
                }
                let pending = &chat.pending;
                for (index, request) in pending.iter().take(2).enumerate() {
                    ui.add_space(12.0);
                    approval_card(ui, p, request, index, pending.len(), &mut approvals);
                }
                tail_space(ui, session_id, area_rect, busy);
                if pinned {
                    ui.scroll_to_cursor_animation(
                        Some(Align::BOTTOM),
                        egui::style::ScrollAnimation::none(),
                    );
                }
            });
        })
    };

    app.md = md;

    for action in approvals {
        app.backend.send(action);
    }

    let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
    let offset = output.state.clamped_offset().y;
    let distance = (max_offset - offset).max(0.0);

    following = follow_next(pinned, distance, previous_offset, offset, following);
    if pinned {
        ui.memory_mut(|memory| {
            memory.data.insert_temp(pin_id, false);
        });
        if let Some(chat) = &mut app.model.active {
            chat.scroll_to_bottom = false;
        }
    }

    ui.memory_mut(|memory| {
        memory.data.insert_temp(follow_id, following);
        memory.data.insert_temp(offset_id, offset);
    });

    let transparent = Color32::from_rgba_unmultiplied(p.bg.r(), p.bg.g(), p.bg.b(), 0);
    let viewport = output.inner_rect;
    if offset > 0.5 && viewport.height() > EDGE_FADE {
        fade_band(
            ui.painter(),
            Rect::from_min_size(
                Pos2::new(area_rect.left(), viewport.top()),
                Vec2::new(area_rect.width(), EDGE_FADE),
            ),
            p.bg,
            transparent,
        );
    }
    let reserved = composer::reserved_height(ui.ctx(), session_id, false);
    let composer_top = composer_top(area_rect.bottom(), reserved);
    let fade_top = (composer_top - COMPOSER_FADE).max(viewport.top());
    if composer_top > viewport.top() {
        fade_band(
            ui.painter(),
            Rect::from_min_max(
                Pos2::new(area_rect.left(), fade_top),
                Pos2::new(area_rect.right(), composer_top),
            ),
            transparent,
            p.bg,
        );
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(area_rect.left(), composer_top),
                Pos2::new(area_rect.right(), area_rect.bottom()),
            ),
            CornerRadius::ZERO,
            p.bg,
        );
    }

    if load_older {
        app.backend.send(Action::LoadOlder(session_id.to_string()));
    }
    if let Some(path) = open_file {
        app.ui.context_open = true;
        app.ui.surface = crate::surfaces::Surface::Files;
        let files = &mut app.model.files;
        if !files.open_tabs.iter().any(|tab| tab == &path) {
            files.open_tabs.push(path.clone());
        }
        files.active_tab = Some(path.clone());
        app.backend.send(Action::LoadFile(path));
    }

    cache.store(ui, cache_id);

    Outcome {
        away: !following && distance > REARM,
    }
}

fn held_timeline(
    ui: &mut Ui,
    p: &Palette,
    md: &mut Markdown,
    timeline: &Timeline,
    session_id: &str,
    area_rect: Rect,
) {
    let mut load_older = false;
    let mut open_file: Option<String> = None;
    let scroll = ScrollArea::vertical()
        .id_salt(("chat-scroll", timeline.session_id.as_str()))
        .auto_shrink([false, false])
        .stick_to_bottom(true);
    scroll.show(ui, |ui| {
        centered_column(ui, |ui| {
            list_header(ui, p, timeline.has_more, &mut load_older);
            let messages = visible_messages(&timeline.messages);
            if messages.is_empty() {
                empty_state(ui, p);
            } else {
                let turns = build_turns(&messages);
                let retired = retired_turns(&turns);
                for (index, turn) in turns.iter().enumerate() {
                    turn_view(ui, p, md, turn, retired[index], &mut open_file);
                }
            }
            if let Some(error) = &timeline.error {
                ui.add_space(12.0);
                error_card(ui, p, md, Some("Run failed"), error);
            }
            tail_space(ui, session_id, area_rect, timeline.busy);
        });
    });
}

fn follow_next(pinned: bool, distance: f32, previous: f32, offset: f32, following: bool) -> bool {
    if pinned {
        return true;
    }
    if distance <= REARM {
        return true;
    }
    if previous >= 0.0 && offset < previous - 0.5 {
        return false;
    }
    following
}

fn tail_space(ui: &mut Ui, session_id: &str, area_rect: Rect, busy: bool) {
    let ctx = ui.ctx().clone();
    let current = composer::reserved_height(&ctx, session_id, false);
    let resting_id = Id::unique(("composer-resting", session_id));
    let resting = ctx
        .memory(|memory| memory.data.get_temp::<f32>(resting_id))
        .map(|resting| resting.min(current))
        .unwrap_or(current);
    ctx.memory_mut(|memory| {
        memory.data.insert_temp(resting_id, resting);
    });
    let inset = if busy {
        (resting + TAIL_GAP).max(current + TAIL_GAP)
    } else {
        resting + TAIL_GAP
    };
    let tail = inset.max(area_rect.height() * 0.10).max(48.0);
    ui.add_space(tail);
}

fn empty_state(ui: &mut Ui, p: &Palette) {
    ui.add_space(44.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "No messages yet",
        theme::font(theme::TEXT_META),
        p.fg_muted,
    );
    ui.add_space(44.0);
}

fn loading_rows(ui: &mut Ui, p: &Palette) {
    let alpha = motion::waiting_alpha(ui.ctx(), Id::unique("chat-skeleton"));
    centered_column(ui, |ui| {
        ui.add_space(16.0);
        for index in 0..3 {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 168.0), Sense::hover());
            let painter = ui.painter();
            let mut y = rect.top() + 12.0;
            for row in 0..3 - (index % 2) {
                let bar =
                    Rect::from_min_size(Pos2::new(rect.left() + 16.0, y), Vec2::new(14.0, 14.0));
                painter.circle_filled(bar.center(), 7.0, skeleton(p, 0.9 * alpha));
                let title_w = 96.0 - row as f32 * 12.0;
                fill_bar(
                    painter,
                    Rect::from_min_size(
                        Pos2::new(bar.right() + 8.0, y + 1.0),
                        Vec2::new(title_w, 12.0),
                    ),
                    skeleton(p, alpha),
                );
                fill_bar(
                    painter,
                    Rect::from_min_size(
                        Pos2::new(bar.right() + 16.0 + title_w, y + 1.0),
                        Vec2::new(160.0 - row as f32 * 24.0, 12.0),
                    ),
                    skeleton(p, 0.7 * alpha),
                );
                y += 22.0;
            }
            y += 8.0;
            for line in 0..3 {
                let width = rect.width() * [0.92, 0.78, 0.64][line];
                fill_bar(
                    painter,
                    Rect::from_min_size(Pos2::new(rect.left() + 16.0, y), Vec2::new(width, 12.0)),
                    skeleton(p, 0.55 * alpha),
                );
                y += 22.0;
            }
            ui.add_space(16.0);
        }
        ui.add_space(40.0);
    });
}

fn skeleton(p: &Palette, alpha: f32) -> Color32 {
    p.alpha(p.fg, (26.0 * alpha).clamp(0.0, 255.0) as u8)
}

fn fill_bar(painter: &egui::Painter, rect: Rect, color: Color32) {
    painter.rect_filled(rect, CornerRadius::same(theme::radius::XS), color);
}

fn list_header(ui: &mut Ui, p: &Palette, has_more: bool, load: &mut bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::hover());
    if !has_more {
        return;
    }
    let label = "Load older messages";
    let font = theme::font_medium(theme::TEXT_UI_LABEL);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, p.fg);
    let width = galley.size().x + 20.0;
    let button = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.top() + 12.0 + 14.0),
        Vec2::new(width, 28.0),
    );
    let response = ui.interact(button, Id::unique("chat-load-older"), Sense::click());
    crate::widgets::pointer(ui, &response);
    let hover = response.hovered();
    ui.painter().rect_filled(
        button,
        CornerRadius::same(theme::radius::MD),
        if hover {
            p.over(p.card, p.hover)
        } else {
            p.card
        },
    );
    ui.painter().rect_stroke(
        button,
        CornerRadius::same(theme::radius::MD),
        Stroke::new(theme::HAIRLINE, p.border_soft),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        Pos2::new(
            button.center().x - galley.size().x * 0.5,
            button.center().y - galley.size().y * 0.5,
        ),
        galley,
        if hover { p.fg } else { p.fg_muted },
    );
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
    }
    if response.clicked() {
        *load = true;
    }
}

fn wide_layout(ctx: &egui::Context) -> bool {
    ctx.content_rect().width() >= 1024.0
}

fn row_height(viewport_width: f32) -> f32 {
    if viewport_width >= ROW_WIDE_MIN {
        ROW_HEIGHT
    } else {
        ROW_HEIGHT_NARROW
    }
}

fn row_height_of(ui: &Ui) -> f32 {
    row_height(ui.ctx().content_rect().width())
}

fn inline_pad(available: f32, wide: bool) -> f32 {
    if wide {
        (available * 0.03).clamp(16.0, 24.0)
    } else {
        (available * 0.025).clamp(12.0, 16.0)
    }
}

fn column_geometry(available: f32, wide: bool) -> (f32, f32) {
    let available = available.max(0.0);
    let pad = inline_pad(available, wide).min(available * 0.5);
    let inner = (available - 2.0 * pad).min(COLUMN_WIDTH).max(0.0);
    let left = (available - inner) * 0.5;
    (left, inner)
}

fn centered_column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let origin = ui.cursor().min;
    let width = ui.available_width().max(0.0);
    let (left, inner) = column_geometry(width, wide_layout(ui.ctx()));
    let child_rect = Rect::from_min_size(
        Pos2::new(origin.x + left, origin.y),
        Vec2::new(inner, 1.0e6),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(child_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.spacing_mut().item_spacing = Vec2::ZERO;
    add(&mut child);
    let used = child.min_rect().height();
    ui.advance_cursor_after_rect(Rect::from_min_size(child_rect.min, Vec2::new(inner, used)));
}

struct Turn<'a> {
    key: &'a str,
    user: Option<&'a UserMessage>,
    steps: Vec<&'a AssistantMessage>,
    notices: Vec<&'a Message>,
}

fn build_turns<'a>(messages: &[&'a Message]) -> Vec<Turn<'a>> {
    let mut turns: Vec<Turn<'a>> = Vec::new();
    for message in messages {
        match message {
            Message::User(user) => turns.push(Turn {
                key: user.id.as_str(),
                user: Some(user.as_ref()),
                steps: Vec::new(),
                notices: Vec::new(),
            }),
            Message::Assistant(assistant) => match turns.last_mut() {
                Some(turn) if turn.user.is_some() => turn.steps.push(assistant.as_ref()),
                _ => turns.push(Turn {
                    key: assistant.id.as_str(),
                    user: None,
                    steps: vec![assistant.as_ref()],
                    notices: Vec::new(),
                }),
            },
            other => match turns.last_mut() {
                Some(turn) if turn.user.is_some() && turn.steps.is_empty() => {
                    turn.notices.push(other)
                }
                _ => turns.push(Turn {
                    key: other.id().unwrap_or("notice"),
                    user: None,
                    steps: Vec::new(),
                    notices: vec![other],
                }),
            },
        }
    }
    turns
}

fn retired_turns(turns: &[Turn<'_>]) -> Vec<bool> {
    let mut retired = vec![false; turns.len()];
    let mut seen = false;
    for index in (0..turns.len()).rev() {
        retired[index] = seen;
        seen = seen || !turns[index].steps.is_empty();
    }
    retired
}

fn visible_messages<'a>(messages: &'a [Message]) -> Vec<&'a Message> {
    let mut slots: Vec<Option<&'a Message>> = Vec::with_capacity(messages.len());
    let mut positions: HashMap<&'a str, usize> = HashMap::new();
    for message in messages {
        match message.id() {
            Some(id) => {
                if let Some(index) = positions.get(id).copied() {
                    slots[index] = Some(message);
                } else {
                    positions.insert(id, slots.len());
                    slots.push(Some(message));
                }
            }
            None => slots.push(Some(message)),
        }
    }
    slots.into_iter().flatten().collect()
}

fn has_text(assistant: &AssistantMessage) -> bool {
    assistant
        .content
        .iter()
        .any(|part| matches!(part, Part::Text(text) if !text.text.trim().is_empty()))
}

fn final_step(turn: &Turn<'_>) -> Option<usize> {
    let index = turn.steps.len().checked_sub(1)?;
    let assistant = turn.steps[index];
    if assistant.finish.as_deref() == Some("stop") && has_text(assistant) {
        Some(index)
    } else {
        None
    }
}

fn turn_text(turn: &Turn<'_>) -> String {
    let mut out = String::new();
    for step in &turn.steps {
        for part in &step.content {
            if let Part::Text(text) = part {
                if text.text.trim().is_empty() {
                    continue;
                }
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&text.text);
            }
        }
    }
    out
}

fn turn_view(
    ui: &mut Ui,
    p: &Palette,
    md: &mut Markdown,
    turn: &Turn<'_>,
    retired: bool,
    open_file: &mut Option<String>,
) {
    if let Some(user) = turn.user
        && (!user.text.trim().is_empty() || !user.files.is_empty())
    {
        user_row(ui, p, md, user);
    }
    for notice in &turn.notices {
        notice_row(ui, p, md, notice);
    }

    let answer = final_step(turn);
    let settled = answer.is_some() || retired;
    let activity = activity_rows(turn, answer);
    let has_activity = !activity.is_empty();
    if has_activity {
        activity_fold(ui, p, md, turn, &activity, settled, open_file);
    }

    if let Some(index) = answer {
        let assistant = turn.steps[index];
        let start = ui.cursor().min;
        for part in &assistant.content {
            if let Part::Text(text) = part {
                if text.text.trim().is_empty() {
                    continue;
                }
                ui.add_space(4.0);
                md.show_at(
                    ui,
                    Id::unique(("answer", assistant.id.as_str(), text.text.len())),
                    &text.text,
                    p,
                );
                ui.add_space(4.0);
            }
        }
        let end = ui.cursor().min;
        if let Some(error) = &assistant.error {
            ui.add_space(12.0);
            error_card(ui, p, md, None, &error.message);
        }
        if has_text(assistant)
            && (assistant.finish.as_deref() == Some("stop") || assistant.error.is_some())
        {
            let body = Rect::from_min_max(start, Pos2::new(start.x + ui.available_width(), end.y));
            message_footer(ui, p, turn, assistant, body);
        }
    } else if let Some(assistant) = turn.steps.last()
        && let Some(error) = &assistant.error
    {
        if !has_activity {
            ui.add_space(12.0);
        }
        error_card(ui, p, md, None, &error.message);
    }

    if turn.steps.is_empty() && turn.user.is_some() {
        ui.add_space(8.0);
    }
}

#[derive(Clone, Copy)]
enum Activity<'a> {
    Text(&'a str),
    Reasoning {
        text: &'a str,
        streaming: bool,
        seconds: Option<f64>,
    },
    Tool(&'a ToolPart),
}

fn activity_rows<'a>(turn: &Turn<'a>, answer: Option<usize>) -> Vec<Activity<'a>> {
    let mut rows = Vec::new();
    for (index, step) in turn.steps.iter().enumerate() {
        let streaming = step.time.completed.is_none();
        let seconds = match step.time.completed {
            Some(completed) if completed > step.time.created && step.time.created > 0.0 => {
                Some((completed - step.time.created) / 1000.0)
            }
            _ => None,
        };
        for part in &step.content {
            match part {
                Part::Text(text) => {
                    if text.text.trim().is_empty() || answer == Some(index) {
                        continue;
                    }
                    rows.push(Activity::Text(&text.text));
                }
                Part::Reasoning(reasoning) => {
                    if reasoning.text.trim().is_empty() {
                        continue;
                    }
                    rows.push(Activity::Reasoning {
                        text: &reasoning.text,
                        streaming,
                        seconds,
                    });
                }
                Part::Tool(tool) => {
                    if tool_visible(tool) {
                        rows.push(Activity::Tool(tool));
                    }
                }
                Part::Unknown => {}
            }
        }
    }
    rows
}

#[derive(Default)]
struct TurnSummary {
    files: usize,
    additions: usize,
    deletions: usize,
    explored: bool,
    commands: usize,
    researched: bool,
    subagents: usize,
}

fn summarize(turn: &Turn<'_>) -> TurnSummary {
    let mut summary = TurnSummary::default();
    let mut changed: Vec<String> = Vec::new();
    for step in &turn.steps {
        for part in &step.content {
            let Part::Tool(tool) = part else {
                continue;
            };
            let name = tool.name.to_ascii_lowercase();
            if is_file_tool(&name)
                && let Some(path) = tool_path(tool)
                && !changed.iter().any(|known| known == &path)
            {
                changed.push(path);
            }
            if matches!(
                name.as_str(),
                "read" | "read_file" | "view" | "grep" | "search" | "glob" | "list"
            ) {
                summary.explored = true;
            }
            if matches!(name.as_str(), "bash" | "shell" | "cmd" | "terminal") {
                summary.commands += 1;
            }
            if matches!(
                name.as_str(),
                "webfetch" | "fetch" | "websearch" | "search_web"
            ) {
                summary.researched = true;
            }
            if matches!(name.as_str(), "task" | "agent") {
                summary.subagents += 1;
            }
        }
    }
    summary.files = changed.len();
    let (additions, deletions) = tool_diff_total(turn);
    summary.additions = additions;
    summary.deletions = deletions;
    summary
}

fn fold_label(summary: &TurnSummary) -> String {
    let mut parts: Vec<String> = Vec::new();
    if summary.explored {
        parts.push("Explored codebase".into());
    }
    if summary.commands > 0 {
        parts.push(if summary.commands == 1 {
            "Ran 1 command".into()
        } else {
            format!("Ran {} commands", summary.commands)
        });
    }
    if summary.researched {
        parts.push("Researched the web".into());
    }
    if summary.subagents > 0 {
        parts.push(if summary.subagents == 1 {
            "Used 1 subagent".into()
        } else {
            format!("Used {} subagents", summary.subagents)
        });
    }
    parts.join(" · ")
}

fn activity_fold(
    ui: &mut Ui,
    p: &Palette,
    md: &mut Markdown,
    turn: &Turn<'_>,
    activity: &[Activity<'_>],
    settled: bool,
    open_file: &mut Option<String>,
) {
    let id = Id::unique(("turn-activity", turn.key));
    let mut expanded = ui
        .memory(|memory| memory.data.get_temp::<bool>(id.with("open")))
        .unwrap_or(false);
    let open = expanded || !settled;
    let trimmed = open && !expanded;
    let hidden = if trimmed {
        activity.len().saturating_sub(FOLD_ROWS)
    } else {
        0
    };
    let summary = if settled { Some(summarize(turn)) } else { None };

    ui.add_space(4.0);
    let details = summary.as_ref().map(fold_label).unwrap_or_default();
    let files = summary
        .as_ref()
        .filter(|summary| summary.files > 0)
        .map(|summary| {
            if summary.files == 1 {
                "Changed 1 file".to_string()
            } else {
                format!("Changed {} files", summary.files)
            }
        })
        .unwrap_or_default();
    let additions = summary
        .as_ref()
        .map(|summary| summary.additions)
        .unwrap_or(0);
    let deletions = summary
        .as_ref()
        .map(|summary| summary.deletions)
        .unwrap_or(0);

    let response = fold_header(
        ui,
        p,
        id,
        expanded,
        &files,
        additions,
        deletions,
        &details,
        if hidden > 0 {
            Some(format!("+{hidden} more"))
        } else {
            None
        },
    );
    if response.clicked() {
        expanded = !expanded;
        ui.memory_mut(|memory| memory.data.insert_temp(id.with("open"), expanded));
    }

    let start = if hidden > 0 { hidden } else { 0 };
    collapsible_body(ui, id.with("rows"), open, None, |ui| {
        for (index, row) in activity.iter().enumerate() {
            if index < start {
                continue;
            }
            match row {
                Activity::Text(text) => {
                    ui.add_space(4.0);
                    md.show_at(
                        ui,
                        Id::unique(("activity-text", turn.key, text.len())),
                        text,
                        p,
                    );
                    ui.add_space(4.0);
                }
                Activity::Reasoning {
                    text,
                    streaming,
                    seconds,
                } => {
                    reasoning_row(
                        ui,
                        p,
                        md,
                        id.with(("reasoning", index)),
                        text,
                        *streaming,
                        *seconds,
                    );
                }
                Activity::Tool(tool) => {
                    ui.add_space(3.0);
                    tool_row(ui, p, md, tool, open_file);
                    ui.add_space(3.0);
                }
            }
        }
    });
}

fn fold_header(
    ui: &mut Ui,
    p: &Palette,
    _id: Id,
    expanded: bool,
    files: &str,
    additions: usize,
    deletions: usize,
    details: &str,
    more: Option<String>,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), FOLD_HEIGHT), Sense::click());
    let center = rect.center().y;
    let mut x = rect.left() + 1.0;
    let icon_rect =
        Rect::from_center_size(Pos2::new(x + ROW_ICON * 0.5, center), Vec2::splat(ROW_ICON));
    icons::paint(ui, Icon::Layers, icon_rect, p.tool_icon);
    x = icon_rect.right() + ROW_GAP;

    let title = ui.painter().layout_no_wrap(
        "Activity".to_string(),
        theme::font_semibold(FOLD_TITLE),
        p.tool_title,
    );
    let title_width = title.size().x;
    ui.painter().galley(
        Pos2::new(x, center - title.size().y * 0.5),
        title,
        p.tool_title,
    );
    x += title_width + ROW_GAP;

    let chevron = Rect::from_center_size(Pos2::new(x + 6.0, center), Vec2::splat(12.0));
    icons::paint(
        ui,
        if expanded {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        chevron,
        p.tool_description,
    );
    x = chevron.right() + ROW_GAP;

    let right_limit = rect.right() - ROW_PAD_END;
    let mut trailing = 0.0;
    if let Some(more) = more {
        let galley =
            ui.painter()
                .layout_no_wrap(more.clone(), theme::font(ROW_TEXT), p.tool_description);
        trailing = galley.size().x + ROW_GAP;
        ui.painter().galley(
            Pos2::new(
                right_limit - galley.size().x,
                center - galley.size().y * 0.5,
            ),
            galley,
            p.tool_description,
        );
    }

    if !files.is_empty() {
        let galley = row_galley(
            ui,
            files,
            theme::font(ROW_TEXT),
            p.tool_description,
            (right_limit - trailing - x).max(0.0),
        );
        let width = galley.size().x;
        ui.painter().galley(
            Pos2::new(x, center - galley.size().y * 0.5),
            galley,
            p.tool_description,
        );
        x += width + 4.0;
        if additions > 0 || deletions > 0 {
            let adds = ui.painter().layout_no_wrap(
                format!("+{additions}"),
                theme::font(ROW_TEXT),
                p.green,
            );
            let slash = ui.painter().layout_no_wrap(
                "/".to_string(),
                theme::font(ROW_TEXT),
                p.tool_description,
            );
            let dels =
                ui.painter()
                    .layout_no_wrap(format!("-{deletions}"), theme::font(ROW_TEXT), p.red);
            let total = adds.size().x + slash.size().x + dels.size().x;
            if x + total < right_limit - trailing {
                ui.painter()
                    .galley(Pos2::new(x, center - adds.size().y * 0.5), adds, p.green);
                ui.painter().galley(
                    Pos2::new(
                        x + total - dels.size().x - slash.size().x,
                        center - slash.size().y * 0.5,
                    ),
                    slash,
                    p.tool_description,
                );
                ui.painter().galley(
                    Pos2::new(x + total - dels.size().x, center - dels.size().y * 0.5),
                    dels,
                    p.red,
                );
                x += total + ROW_GAP;
            }
        } else {
            x += ROW_GAP;
        }
    }

    if !details.is_empty() && x < right_limit - trailing {
        let prefix = if files.is_empty() { "" } else { "· " };
        let text = format!("{prefix}{details}");
        let galley = row_galley(
            ui,
            &text,
            theme::font(ROW_TEXT),
            p.tool_description,
            (right_limit - trailing - x).max(0.0),
        );
        ui.painter().galley(
            Pos2::new(x, center - galley.size().y * 0.5),
            galley,
            p.tool_description,
        );
    }

    response
}

fn user_markdown(ui: &mut Ui, md: &mut Markdown, text: &str, id: Id, p: &Palette, long: bool) {
    let mut show = |ui: &mut Ui| {
        md.show_at_sized(ui, id, text, TEXT_USER, theme::LINE_USER, p.fg, p);
    };
    if long {
        ScrollArea::vertical()
            .id_salt(id.with("scroll"))
            .max_height(360.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.set_width(ui.available_width().max(40.0));
                show(ui);
            });
    } else {
        show(ui);
    }
}

fn linkify_session_ids(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("ses_") {
        out.push_str(&rest[..start]);
        let candidate = &rest[start..];
        let end = candidate
            .char_indices()
            .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_'))
            .map(|(index, _)| index)
            .unwrap_or(candidate.len());
        let id = &candidate[..end];
        if id.len() > 4 {
            out.push_str(&format!("[{id}](rchamber://session/{id})"));
            rest = &candidate[end..];
        } else {
            out.push_str("ses_");
            rest = &candidate[4..];
        }
    }
    out.push_str(rest);
    out
}

fn user_row(ui: &mut Ui, p: &Palette, md: &mut Markdown, user: &UserMessage) -> Rect {
    ui.add_space(16.0);
    let linked_text = linkify_session_ids(&user.text);
    let long = user.text.chars().count() > 4_000 || user.text.lines().count() > 40;
    let max = (ui.available_width() * USER_BUBBLE_MAX).max(120.0);
    let inner_width = (max - 2.0 * USER_PAD_X - 2.0).max(40.0);
    let frame = egui::Frame::new()
        .fill(p.user_bg)
        .stroke(Stroke::new(theme::BORDER, p.accent_border))
        .corner_radius(CornerRadius::same(theme::radius::XL))
        .inner_margin(Margin {
            left: USER_PAD_X as i8,
            right: USER_PAD_X as i8,
            top: USER_PAD_Y as i8,
            bottom: USER_PAD_Y as i8,
        });
    let bubble = ui
        .with_layout(Layout::right_to_left(Align::TOP), |ui| {
            frame
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                    if long {
                        ui.set_width(inner_width);
                    } else {
                        ui.set_max_width(inner_width);
                    }
                    if user.files.is_empty() {
                        if !user.text.is_empty() {
                            user_markdown(
                                ui,
                                md,
                                &linked_text,
                                Id::unique(("user-text", user.id.as_str())),
                                p,
                                long,
                            );
                        }
                    } else {
                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                                for file in &user.files {
                                    file_chip(ui, p, file);
                                }
                            });
                            if !user.text.is_empty() {
                                user_markdown(
                                    ui,
                                    md,
                                    &linked_text,
                                    Id::unique(("user-text", user.id.as_str())),
                                    p,
                                    long,
                                );
                            }
                        });
                    }
                })
                .response
        })
        .inner;

    let (actions, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ACTIONS_ROW), Sense::hover());
    let id = Id::unique(("user-actions", user.id.as_str()));
    let pointer = ui.ctx().pointer_hover_pos();
    let hovered = pointer
        .map(|pointer| bubble.rect.contains(pointer) || actions.contains(pointer))
        .unwrap_or(false);
    let reveal = motion::hover_fade(ui, id.with("reveal"), hovered);
    if reveal > 0.001 {
        let color = p.fg_muted.gamma_multiply(reveal);
        let copy = Rect::from_min_size(
            Pos2::new(
                actions.right() - ACTION_SIZE,
                actions.top() + (ACTIONS_ROW - ACTION_SIZE) * 0.5,
            ),
            Vec2::splat(ACTION_SIZE),
        );
        let copy_response = ui.interact(copy, id.with("copy"), Sense::click());
        crate::widgets::pointer(ui, &copy_response);
        if copy_response.hovered() {
            ui.painter().rect_filled(
                copy,
                CornerRadius::same(theme::radius::SM),
                p.hover.gamma_multiply(reveal),
            );
        }
        icons::paint(
            ui,
            Icon::Copy,
            Rect::from_center_size(copy.center(), Vec2::splat(ACTION_ICON)),
            if copy_response.hovered() {
                p.fg.gamma_multiply(reveal)
            } else {
                color
            },
        );
        if copy_response.clicked() {
            ui.ctx().copy_text(user.text.clone());
        }

        let time = time_label(user.time.created);
        if !time.is_empty() {
            let font = theme::font_mono(theme::TEXT_MICRO);
            let galley = ui.painter().layout_no_wrap(time, font, color);
            let right = copy.left() - 4.0;
            let text_left = right - galley.size().x;
            let clock = Rect::from_center_size(
                Pos2::new(text_left - 4.0 - 7.0, actions.center().y),
                Vec2::splat(14.0),
            );
            icons::paint(ui, Icon::Clock, clock, color);
            ui.painter().galley(
                Pos2::new(text_left, actions.center().y - galley.size().y * 0.5),
                galley,
                color,
            );
        }
    }

    bubble.rect
}

fn file_chip(ui: &mut Ui, p: &Palette, file: &Attachment) {
    let name = file
        .name
        .clone()
        .or_else(|| file.path.clone())
        .unwrap_or_else(|| "attachment".into());
    let font = theme::font_mono(theme::TEXT_MICRO);
    let galley = ui.painter().layout_no_wrap(
        format!("@{}", shorten(&name, 40)),
        font,
        p.primary_text,
    );
    let width = 8.0 + 12.0 + 5.0 + galley.size().x + 5.0 + 10.0 + 7.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::XS), p.raised_fill());
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::radius::XS),
        Stroke::new(theme::BORDER, p.border),
        StrokeKind::Inside,
    );
    icons::paint(
        ui,
        Icon::File,
        Rect::from_center_size(
            Pos2::new(rect.left() + 8.0 + 6.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        p.fg_muted,
    );
    let text_left = rect.left() + 8.0 + 12.0 + 5.0;
    ui.painter().galley(
        Pos2::new(text_left, rect.center().y - galley.size().y * 0.5),
        galley,
        p.primary_text,
    );
    icons::paint(
        ui,
        Icon::Close,
        Rect::from_center_size(
            Pos2::new(rect.right() - 8.0 - 5.0, rect.center().y),
            Vec2::splat(9.0),
        ),
        p.fg_faint,
    );
}

fn message_footer(
    ui: &mut Ui,
    p: &Palette,
    turn: &Turn<'_>,
    assistant: &AssistantMessage,
    body: Rect,
) {
    let id = Id::unique(("message-footer", assistant.id.as_str()));
    ui.add_space(6.0);
    let (row, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ACTIONS_ROW), Sense::hover());
    let pointer = ui.ctx().pointer_hover_pos();
    let region = body.union(row);
    let hovered = pointer
        .map(|pointer| region.contains(pointer))
        .unwrap_or(false);
    let reveal = motion::hover_fade(ui, id.with("reveal"), hovered);

    let started = turn
        .user
        .map(|user| user.time.created)
        .filter(|created| *created > 0.0);
    let duration = match (started, assistant.time.completed) {
        (Some(started), Some(completed)) if completed > started => {
            Some(turn_duration(completed - started))
        }
        _ => None,
    };
    let tokens = assistant
        .tokens
        .as_ref()
        .map(|usage| usage.total())
        .filter(|total| *total > 0.0);

    let font = theme::font_mono(theme::TEXT_MICRO);
    let mut x = row.left();
    if let Some(duration) = &duration {
        let galley = ui
            .painter()
            .layout_no_wrap(duration.clone(), font.clone(), p.fg_muted);
        ui.painter().galley(
            Pos2::new(x, row.center().y - galley.size().y * 0.5),
            galley.clone(),
            p.fg_muted,
        );
        x += galley.size().x;
    }
    if let Some(tokens) = tokens {
        if x > row.left() {
            let dot = ui
                .painter()
                .layout_no_wrap("·".to_string(), font.clone(), p.fg_faint);
            ui.painter().galley(
                Pos2::new(x + 6.0, row.center().y - dot.size().y * 0.5),
                dot.clone(),
                p.fg_faint,
            );
            x += 6.0 + dot.size().x + 6.0;
        }
        let label = format!("{} tok", token_label(tokens));
        let galley = ui.painter().layout_no_wrap(label, font, p.fg_muted);
        ui.painter().galley(
            Pos2::new(x, row.center().y - galley.size().y * 0.5),
            galley,
            p.fg_muted,
        );
    }

    if reveal > 0.001 {
        let copy = Rect::from_min_size(
            Pos2::new(row.right() - ACTION_SIZE, row.top()),
            Vec2::splat(ACTION_SIZE),
        );
        let response = ui.interact(copy, id.with("copy"), Sense::click());
        crate::widgets::pointer(ui, &response);
        if response.hovered() {
            ui.painter().rect_filled(
                copy,
                CornerRadius::same(theme::radius::SM),
                p.hover.gamma_multiply(reveal),
            );
        }
        icons::paint(
            ui,
            Icon::Copy,
            Rect::from_center_size(copy.center(), Vec2::splat(ACTION_ICON)),
            if response.hovered() {
                p.fg.gamma_multiply(reveal)
            } else {
                p.fg_muted.gamma_multiply(reveal)
            },
        );
        if response
            .on_hover_text("Copy the answer")
            .clicked()
        {
            let text = turn_text(turn);
            if !text.is_empty() {
                ui.ctx().copy_text(text);
            }
        }

        let apply = Rect::from_min_size(
            Pos2::new(row.right() - ACTION_SIZE - 4.0 - ACTION_SIZE, row.top()),
            Vec2::splat(ACTION_SIZE),
        );
        let response = ui.interact(apply, id.with("apply-all"), Sense::click());
        crate::widgets::pointer(ui, &response);
        if response.hovered() {
            ui.painter().rect_filled(
                apply,
                CornerRadius::same(theme::radius::SM),
                p.hover.gamma_multiply(reveal),
            );
        }
        icons::paint(
            ui,
            Icon::Diff,
            Rect::from_center_size(apply.center(), Vec2::splat(ACTION_ICON)),
            if response.hovered() {
                p.fg.gamma_multiply(reveal)
            } else {
                p.fg_muted.gamma_multiply(reveal)
            },
        );
        if response.on_hover_text("Copy all changes").clicked() {
            let patch = turn_patch(turn);
            if !patch.is_empty() {
                ui.ctx().copy_text(patch);
            }
        }

        let quote = Rect::from_min_size(
            Pos2::new(
                row.right() - ACTION_SIZE * 2.0 - 8.0 - ACTION_SIZE,
                row.top(),
            ),
            Vec2::splat(ACTION_SIZE),
        );
        let response = ui.interact(quote, id.with("quote"), Sense::click());
        crate::widgets::pointer(ui, &response);
        if response.hovered() {
            ui.painter().rect_filled(
                quote,
                CornerRadius::same(theme::radius::SM),
                p.hover.gamma_multiply(reveal),
            );
        }
        icons::paint(
            ui,
            Icon::CheckCheck,
            Rect::from_center_size(quote.center(), Vec2::splat(ACTION_ICON)),
            if response.hovered() {
                p.fg.gamma_multiply(reveal)
            } else {
                p.fg_muted.gamma_multiply(reveal)
            },
        );
        if response
            .on_hover_text("Reply with a quote and \"Looks good\"")
            .clicked()
        {
            let text = assistant_text(assistant);
            let snippet: String = text.chars().take(280).collect();
            let quoted = snippet
                .lines()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::unique("rchamber-compose"),
                    format!("{quoted}\n\nLooks good\n"),
                )
            });
        }
    }

    let slot = ui
        .ctx()
        .data(|data| data.get_temp::<MessageActionSlot>(egui::Id::unique("message-actions")));
    if let Some(slot) = slot
        && !slot.actions.is_empty()
        && reveal > 0.001
    {
        let more = Rect::from_min_size(
            Pos2::new(row.right() - ACTION_SIZE * 3.0 - 12.0, row.top()),
            Vec2::splat(ACTION_SIZE),
        );
        let response = ui.interact(more, id.with("more"), Sense::click());
        crate::widgets::pointer(ui, &response);
        if response.hovered() {
            ui.painter().rect_filled(
                more,
                CornerRadius::same(theme::radius::SM),
                p.hover.gamma_multiply(reveal),
            );
        }
        icons::paint(
            ui,
            Icon::More,
            Rect::from_center_size(more.center(), Vec2::splat(ACTION_ICON)),
            if response.hovered() {
                p.fg.gamma_multiply(reveal)
            } else {
                p.fg_muted.gamma_multiply(reveal)
            },
        );
        let mut open = ui.memory(|memory| {
            memory
                .data
                .get_temp::<bool>(id.with("open"))
                .unwrap_or(false)
        });
        if response.clicked() {
            open = !open;
        }
        let text = assistant_text(assistant);
        crate::app::menu_popup(
            ui.ctx(),
            &response,
            id.with("menu"),
            &mut open,
            p,
            6.0,
            |ui, _| {
                for (extension_id, action_id, label) in &slot.actions {
                    if crate::widgets::menu_item(ui, label, Some(Icon::Puzzle), None, p).clicked() {
                        slot.sender.send(rchamber_app::Action::ExtensionAction {
                            id: extension_id.clone(),
                            action: action_id.clone(),
                            item: serde_json::json!({
                                "kind": "message",
                                "role": "assistant",
                                "text": text,
                            }),
                        });
                    }
                }
            },
        );
        ui.memory_mut(|memory| memory.data.insert_temp(id.with("open"), open));
    }

    if assistant.finish.as_deref() == Some("stop") {
        let files = changed_files(turn);
        if !files.is_empty() {
            ui.add_space(6.0);
            changed_file_chips(ui, p, &files);
        }
    }
    ui.add_space(4.0);
}

fn changed_files(turn: &Turn<'_>) -> Vec<(String, usize, usize)> {
    let mut files: Vec<(String, usize, usize)> = Vec::new();
    for step in &turn.steps {
        for part in &step.content {
            let Part::Tool(tool) = part else {
                continue;
            };
            let Some(path) = tool_path(tool) else {
                continue;
            };
            let (additions, deletions) = tool_file_diff(tool, &path);
            if let Some(entry) = files.iter_mut().find(|entry| entry.0 == path) {
                entry.1 += additions;
                entry.2 += deletions;
            } else {
                files.push((path, additions, deletions));
            }
        }
    }
    files
}

fn changed_file_chips(ui: &mut Ui, p: &Palette, files: &[(String, usize, usize)]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(10.0, 4.0);
        for (path, additions, deletions) in files.iter().take(4) {
            let name = path.rsplit('/').next().unwrap_or(path);
            let font = theme::font(ROW_TEXT);
            let galley = ui
                .painter()
                .layout_no_wrap(shorten(name, 32), font, p.fg_muted);
            let counters = additions + deletions;
            let mut width = 8.0 + 14.0 + 6.0 + galley.size().x + 8.0;
            if counters > 0 {
                width += 26.0;
            }
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::radius::TABLE), tile_fill(p));
            icons::paint(
                ui,
                Icon::File,
                Rect::from_center_size(
                    Pos2::new(rect.left() + 8.0 + 7.0, rect.center().y),
                    Vec2::splat(14.0),
                ),
                p.fg_muted,
            );
            ui.painter().galley(
                Pos2::new(rect.left() + 28.0, rect.center().y - galley.size().y * 0.5),
                galley,
                p.fg_muted,
            );
            if counters > 0 {
                let adds = ui.painter().layout_no_wrap(
                    format!("+{additions}"),
                    theme::font(theme::TEXT_MICRO),
                    p.green,
                );
                let dels = ui.painter().layout_no_wrap(
                    format!("-{deletions}"),
                    theme::font(theme::TEXT_MICRO),
                    p.red,
                );
                let right = rect.right() - 8.0;
                let dels_width = dels.size().x;
                ui.painter().galley(
                    Pos2::new(right - dels_width, rect.center().y - dels.size().y * 0.5),
                    dels,
                    p.red,
                );
                ui.painter().galley(
                    Pos2::new(
                        right - dels_width - adds.size().x - 2.0,
                        rect.center().y - adds.size().y * 0.5,
                    ),
                    adds,
                    p.green,
                );
            }
        }
    });
}

fn turn_duration(ms: f64) -> String {
    if ms < 60_000.0 {
        format!("{:.1}s", ms / 1000.0)
    } else {
        let total = (ms / 1000.0).round() as i64;
        format!("{}m {}s", total / 60, total % 60)
    }
}

fn token_label(tokens: f64) -> String {
    let value = tokens.round().max(0.0) as i64;
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn turn_patch(turn: &Turn<'_>) -> String {
    let mut out = String::new();
    for step in &turn.steps {
        for part in &step.content {
            let Part::Tool(tool) = part else {
                continue;
            };
            for (path, patch) in tool_patches(tool) {
                if !out.is_empty() {
                    out.push('\n');
                }
                if !path.is_empty() {
                    out.push_str(&format!("--- {path}\n+++ {path}\n"));
                }
                out.push_str(&patch);
                if !patch.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    out
}

fn display_model(id: &str) -> String {
    let tail = id.rsplit('/').next().unwrap_or(id);
    let mut name = tail.to_string();
    if let Some((head, date)) = tail.rsplit_once('-')
        && date.len() == 8
        && date.chars().all(|c| c.is_ascii_digit())
    {
        name = head.to_string();
    }
    name
}

fn reasoning_open(touched: bool, user_open: bool, streaming: bool) -> bool {
    if touched { user_open } else { streaming }
}

fn reasoning_row(
    ui: &mut Ui,
    p: &Palette,
    md: &mut Markdown,
    id: Id,
    text: &str,
    streaming: bool,
    seconds: Option<f64>,
) {
    let touched = ui
        .memory(|memory| memory.data.get_temp::<bool>(id.with("touched")))
        .unwrap_or(false);
    let user_open = ui
        .memory(|memory| memory.data.get_temp::<bool>(id.with("user")))
        .unwrap_or(false);
    let mut open = reasoning_open(touched, user_open, streaming);
    let summary = first_line(text, 96);
    let mut label = if streaming {
        "Thinking".to_string()
    } else {
        match seconds {
            Some(seconds) if seconds >= 0.1 => format!("Thought for {seconds:.1}s"),
            _ => "Thought".to_string(),
        }
    };
    if !summary.is_empty() && !streaming {
        label.push_str(" — ");
        label.push_str(&summary);
    }

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), REASONING_ROW), Sense::click());
    let center = rect.center().y;
    icons::paint(
        ui,
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        Rect::from_center_size(Pos2::new(rect.left() + 5.0, center), Vec2::splat(12.0)),
        p.fg_faint,
    );
    let mut x = rect.left() + 12.0 + 6.0;
    if streaming {
        let galley =
            ui.painter()
                .layout_no_wrap(label, theme::font(12.5), p.fg_muted);
        let width = galley.size().x;
        ui.painter().galley(
            Pos2::new(x, center - galley.size().y * 0.5),
            galley,
            p.fg_muted,
        );
        x += width + 4.0;
        busy_dots(ui, Pos2::new(x, center), p.fg_muted);
    } else {
        let galley = row_galley(
            ui,
            &label,
            theme::font(12.5),
            p.fg_muted,
            (rect.right() - x).max(20.0),
        );
        ui.painter().galley(
            Pos2::new(x, center - galley.size().y * 0.5),
            galley,
            p.fg_muted,
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        open = !open;
        ui.memory_mut(|memory| {
            memory.data.insert_temp(id.with("touched"), true);
            memory.data.insert_temp(id.with("user"), open);
        });
    }
    reasoned_body(ui, p, md, id, text, open);
}

fn reasoned_body(ui: &mut Ui, p: &Palette, md: &mut Markdown, id: Id, text: &str, open: bool) {
    let start = ui.cursor().min;
    let width = ui.available_width();
    let rise = motion::animate_bool_with_time(ui.ctx(), id.with("rise"), open, REASONING_RISE_TIME);
    let lift = REASONING_RISE * (1.0 - rise);
    let content = collapsible_body(ui, id.with("body"), open, Some(rise), |ui| {
        ui.add_space(4.0);
        let left = REASONING_BAR + REASONING_INDENT;
        let mut body = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    Pos2::new(start.x + left, start.y + 4.0 - lift),
                    Vec2::new((width - left).max(40.0), 1.0e5),
                ))
                .layout(Layout::top_down(Align::Min)),
        );
        ScrollArea::vertical()
            .id_salt(id.with("scroll"))
            .max_height(REASONING_MAX)
            .auto_shrink([false, true])
            .show(&mut body, |ui| {
                md.show_at_sized(ui, id.with("md"), text, 12.5, 20.0, p.fg_muted, p);
            });
        ui.advance_cursor_after_rect(body.min_rect());
        ui.add_space(8.0);
    });
    if content > 0.5 {
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(start.x + 1.0, start.y),
                Vec2::new(REASONING_BAR, content),
            ),
            CornerRadius::same(1),
            p.border,
        );
    }
}

fn collapsible_body(
    ui: &mut Ui,
    id: Id,
    open: bool,
    fade: Option<f32>,
    add: impl FnOnce(&mut Ui),
) -> f32 {
    let t = motion::animate_bool_with_time(ui.ctx(), id.with("t"), open, DISCLOSURE);
    if t <= 0.001 {
        return 0.0;
    }
    let height_id = id.with("height");
    let stored = ui
        .memory(|memory| memory.data.get_temp::<f32>(height_id))
        .unwrap_or(0.0);
    let start = ui.cursor().min;
    let width = ui.available_width();
    let clip = if stored > 0.0 { stored * t } else { 1.0e5 * t };
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(start, Vec2::new(width, 1.0e5)))
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(Rect::from_min_size(start, Vec2::new(width, clip.max(0.0))));
    child.set_opacity(fade.unwrap_or(t));
    add(&mut child);
    let content = child.min_rect().height().max(0.0);
    if t >= 0.999 && fade.unwrap_or(1.0) >= 0.999 && content > 0.0 {
        ui.memory_mut(|memory| {
            memory.data.insert_temp(height_id, content);
        });
    }
    let shown = if stored > 0.0 {
        (stored * t).min(content)
    } else {
        content * t
    };
    ui.advance_cursor_after_rect(Rect::from_min_size(start, Vec2::new(width, shown)));
    content
}

fn disclosure_row(
    ui: &mut Ui,
    p: &Palette,
    id: Id,
    icon: Icon,
    title_color: Color32,
    icon_color: Color32,
    title: &str,
    summary: &str,
    open: bool,
    busy: bool,
    trailing: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), row_height_of(ui)),
        Sense::click(),
    );
    let center = rect.center().y;
    let hover = motion::Animated::from_id(id.with("hover"))
        .with_duration(motion::HOVER)
        .bool(ui.ctx(), response.hovered() || open);
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 1.0 + ROW_ICON * 0.5, center),
        Vec2::splat(ROW_ICON),
    );
    if hover < 0.999 {
        icons::paint(ui, icon, icon_rect, icon_color.gamma_multiply(1.0 - hover));
    }
    if hover > 0.001 {
        icons::paint(
            ui,
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            icon_rect,
            p.tool_description.gamma_multiply(hover),
        );
    }

    let mut x = icon_rect.right() + ROW_GAP;
    let galley = ui.painter().layout_no_wrap(
        title.to_string(),
        theme::font_medium(ROW_TITLE),
        title_color,
    );
    let title_width = galley.size().x;
    ui.painter().galley(
        Pos2::new(x, center - galley.size().y * 0.5),
        galley,
        title_color,
    );
    x += title_width;

    if busy {
        x += 4.0;
        x += busy_dots(ui, Pos2::new(x, center), p.tool_description);
    }

    let mut right = rect.right() - ROW_PAD_END;
    if !trailing.is_empty() {
        let galley = ui.painter().layout_no_wrap(
            trailing.to_string(),
            theme::font(ROW_TEXT),
            p.tool_description,
        );
        right -= galley.size().x;
        ui.painter().galley(
            Pos2::new(right, center - galley.size().y * 0.5),
            galley,
            p.tool_description,
        );
        right -= ROW_GAP;
    }

    let start = x + ROW_GAP;
    if !summary.is_empty() && right > start {
        let galley = row_galley(
            ui,
            summary,
            theme::font(ROW_TEXT),
            p.tool_description,
            right - start,
        );
        ui.painter()
            .with_clip_rect(Rect::from_min_max(
                Pos2::new(start, rect.top()),
                Pos2::new(right, rect.bottom()),
            ))
            .galley(
                Pos2::new(start, center - galley.size().y * 0.5),
                galley,
                p.tool_description,
            );
    }
    response
}

fn busy_dots(ui: &Ui, center: Pos2, color: Color32) -> f32 {
    const PHASES: [[bool; 3]; 5] = [
        [true, false, false],
        [true, true, false],
        [true, true, true],
        [false, true, true],
        [false, false, true],
    ];
    let phase = if motion::reduced_motion(ui.ctx()) {
        2
    } else {
        let time = ui.ctx().input(|input| input.time);
        ((time / motion::BUSY_PERIOD as f64) as usize) % PHASES.len()
    };
    let glyph = ui
        .painter()
        .layout_no_wrap(".".to_string(), theme::font(ROW_TITLE), color);
    let advance = glyph.size().x.max(2.0);
    for (index, shown) in PHASES[phase].iter().enumerate() {
        if *shown {
            ui.painter().galley(
                Pos2::new(
                    center.x + index as f32 * advance,
                    center.y - glyph.size().y * 0.5,
                ),
                glyph.clone(),
                color,
            );
        }
    }
    ui.ctx().request_repaint_after(Duration::from_millis(1000));
    advance * 3.0
}
fn notice_row(ui: &mut Ui, p: &Palette, md: &mut Markdown, message: &Message) {
    if let Message::Shell(shell) = message {
        shell_row(ui, p, shell);
        return;
    }
    let Notice {
        id,
        icon,
        title,
        text,
        accent,
    } = notice_content(p, message);
    let Some(title) = title else {
        return;
    };
    if text.is_none() {
        plain_notice_row(ui, p, accent.unwrap_or(p.fg_muted), &title);
        return;
    }
    let ui_id = Id::unique(("notice", id));
    let mut open = ui
        .memory(|memory| memory.data.get_temp::<bool>(ui_id.with("open")))
        .unwrap_or(false);
    let summary = String::new();
    let response = disclosure_row(
        ui,
        p,
        ui_id,
        icon,
        p.fg,
        accent.unwrap_or(p.fg_muted),
        &title,
        &summary,
        open,
        false,
        "",
    );
    if response.clicked() {
        open = !open;
        ui.memory_mut(|memory| memory.data.insert_temp(ui_id.with("open"), open));
    }
    if open && let Some(text) = &text {
        collapsible_body(ui, ui_id.with("body"), true, None, |ui| {
            ui.add_space(4.0);
            let start = ui.cursor().min;
            let width = ui.available_width();
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_size(
                        Pos2::new(start.x + 20.0, start.y),
                        Vec2::new((width - 20.0).max(40.0), 1.0e5),
                    ))
                    .layout(Layout::top_down(Align::Min)),
            );
            md.show_at(&mut body, ui_id.with("md"), text, p);
            ui.add_space(8.0);
        });
    }
}

fn plain_notice_row(ui: &mut Ui, p: &Palette, color: Color32, title: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    let icon = Rect::from_center_size(
        Pos2::new(rect.left() + 7.0, rect.center().y),
        Vec2::splat(12.0),
    );
    icons::paint(ui, Icon::Info, icon, color);
    ui.painter().galley(
        Pos2::new(rect.left() + 22.0, rect.center().y - 8.5),
        row_galley(
            ui,
            title,
            theme::font(ROW_TEXT),
            p.fg_muted,
            rect.width() - 26.0,
        ),
        p.fg_muted,
    );
}

fn shell_row(ui: &mut Ui, p: &Palette, shell: &rchamber_api::models::ShellMessage) {
    let id = Id::unique(("shell", shell.id.as_str()));
    let mut open = ui
        .memory(|memory| memory.data.get_temp::<bool>(id.with("open")))
        .unwrap_or(false);
    let running = shell.status.as_deref() == Some("running");
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), row_height_of(ui)),
        Sense::click(),
    );
    let center = rect.center().y;
    let hover = motion::Animated::from_id(id.with("hover"))
        .with_duration(motion::HOVER)
        .bool(ui.ctx(), response.hovered() || open);

    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 1.0 + ROW_ICON * 0.5, center),
        Vec2::splat(ROW_ICON),
    );
    if hover < 0.999 {
        icons::paint(
            ui,
            Icon::Terminal,
            icon_rect,
            p.tool_icon.gamma_multiply(1.0 - hover),
        );
    }
    if hover > 0.001 {
        icons::paint(
            ui,
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            icon_rect,
            p.tool_description.gamma_multiply(hover),
        );
    }

    let title = ui.painter().layout_no_wrap(
        "Shell".to_string(),
        theme::font_medium(ROW_TITLE),
        p.tool_title,
    );
    let mut x = icon_rect.right() + ROW_GAP;
    let title_width = title.size().x;
    ui.painter().galley(
        Pos2::new(x, center - title.size().y * 0.5),
        title,
        p.tool_title,
    );
    x += title_width;

    let mut right = rect.right() - ROW_PAD_END;
    let status = shell
        .exit
        .as_ref()
        .and_then(|value| match value {
            serde_json::Value::Number(number) => number.as_i64().map(|code| format!("exit {code}")),
            serde_json::Value::String(text) if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
        .or_else(|| shell.status.clone())
        .filter(|status| !status.is_empty());
    if let Some(status) = &status {
        let galley =
            ui.painter()
                .layout_no_wrap(status.clone(), theme::font(ROW_TEXT), p.tool_description);
        right -= galley.size().x;
        ui.painter().galley(
            Pos2::new(right, center - galley.size().y * 0.5),
            galley,
            if running {
                p.primary
            } else {
                p.tool_description
            },
        );
        right -= ROW_GAP;
    }

    let description = shell
        .description
        .clone()
        .filter(|description| !description.trim().is_empty())
        .unwrap_or_else(|| shell.command.lines().next().unwrap_or("").to_string());
    let start = x + ROW_GAP;
    if !description.is_empty() && right > start {
        let galley = row_galley(
            ui,
            &description,
            theme::font(ROW_TEXT),
            p.tool_description,
            right - start,
        );
        ui.painter()
            .with_clip_rect(Rect::from_min_max(
                Pos2::new(start, rect.top()),
                Pos2::new(right, rect.bottom()),
            ))
            .galley(
                Pos2::new(start, center - galley.size().y * 0.5),
                galley,
                p.tool_description,
            );
    }

    if response.clicked() {
        open = !open;
        ui.memory_mut(|memory| memory.data.insert_temp(id.with("open"), open));
    }
    if open {
        ui.add_space(2.0);
        output_block(ui, p, &format!("$ {}", shell.command), id.with("command"));
        if let Some(output) = &shell.output
            && !output.output.trim().is_empty()
        {
            ui.add_space(4.0);
            output_block(ui, p, &output.output, id.with("output"));
        }
        ui.add_space(2.0);
    }
    if running {
        ui.ctx().request_repaint_after(Duration::from_millis(1000));
    }
}

struct Notice {
    id: String,
    icon: Icon,
    title: Option<String>,
    text: Option<String>,
    accent: Option<Color32>,
}

fn notice_content(p: &Palette, message: &Message) -> Notice {
    match message {
        Message::Synthetic(synthetic) => Notice {
            id: synthetic.id.clone(),
            icon: Icon::Info,
            title: Some(
                synthetic
                    .description
                    .clone()
                    .unwrap_or_else(|| "Session note".into()),
            ),
            text: Some(synthetic.text.clone()),
            accent: None,
        },
        Message::System(system) => Notice {
            id: system.id.clone(),
            icon: Icon::Info,
            title: Some(
                system
                    .description
                    .clone()
                    .unwrap_or_else(|| "System".into()),
            ),
            text: Some(system.text.clone()),
            accent: None,
        },
        Message::Compaction(compaction) => Notice {
            id: compaction.id.clone(),
            icon: Icon::Archive,
            title: Some(
                compaction
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Context compacted".into()),
            ),
            text: compaction
                .summary
                .clone()
                .or_else(|| compaction.text.clone()),
            accent: None,
        },
        Message::Skill(skill) => Notice {
            id: skill.id.clone(),
            icon: Icon::Spark,
            title: Some(format!(
                "Skill {}",
                skill.name.as_deref().unwrap_or_default()
            )),
            text: Some(skill.text.clone()).filter(|text| !text.trim().is_empty()),
            accent: None,
        },
        Message::Idle(idle) => {
            let (title, accent) = match idle.outcome {
                Some(rchamber_api::models::Outcome::Failed) => {
                    (Some("Run failed".to_string()), Some(p.red))
                }
                Some(rchamber_api::models::Outcome::Interrupted) => {
                    (Some("Stopped".to_string()), Some(p.coral))
                }
                _ => (None, None),
            };
            Notice {
                id: idle.id.clone(),
                icon: Icon::Warning,
                title,
                text: idle.error.as_ref().map(|error| error.message.clone()),
                accent,
            }
        }
        Message::AgentSwitched(switched) => Notice {
            id: switched.id.clone(),
            icon: Icon::Bot,
            title: Some(format!("Agent switched to {}", switched.agent)),
            text: None,
            accent: None,
        },
        Message::ModelSwitched(switched) => Notice {
            id: switched.id.clone(),
            icon: Icon::Bot,
            title: Some(format!(
                "Model switched to {}",
                display_model(&switched.model.id)
            )),
            text: None,
            accent: None,
        },
        Message::Shell(shell) => Notice {
            id: shell.id.clone(),
            icon: Icon::Terminal,
            title: Some("Shell".into()),
            text: Some(shell.command.clone()),
            accent: None,
        },
        _ => Notice {
            id: "notice".into(),
            icon: Icon::Info,
            title: None,
            text: None,
            accent: None,
        },
    }
}

fn tool_visible(tool: &ToolPart) -> bool {
    if tool.state.is_running() {
        return true;
    }
    match (
        tool.time.ran.or(Some(tool.time.created)),
        tool.time.completed,
    ) {
        (Some(start), Some(end)) => end >= start,
        _ => true,
    }
}

#[derive(Clone, Default)]
struct ToolRenderers(Vec<ToolRenderer>);

fn tool_renderers(app: &RustChamber) -> ToolRenderers {
    ToolRenderers(
        app.model
            .extensions
            .installed
            .iter()
            .filter(|extension| extension.is_active())
            .flat_map(|extension| extension.manifest.contributes.tools.iter().cloned())
            .collect(),
    )
}

fn set_tool_renderers(ctx: &egui::Context, renderers: ToolRenderers) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique("tool-renderers"), renderers));
}

fn tool_renderer(ctx: &egui::Context, name: &str) -> Option<ToolRenderer> {
    let renderers = ctx
        .data(|data| data.get_temp::<ToolRenderers>(egui::Id::unique("tool-renderers")))
        .unwrap_or_default();
    renderers.0.into_iter().find(|renderer| {
        if let Some(prefix) = renderer.matcher.strip_suffix('*') {
            name.starts_with(prefix)
        } else {
            renderer.matcher == name
        }
    })
}

fn render_template(template: &str, tool: &ToolPart) -> String {
    let input = match &tool.state {
        ToolState::Running(state) => Some(&state.input),
        ToolState::Completed(state) => Some(&state.input),
        ToolState::Error(state) => Some(&state.input),
        ToolState::Streaming(_) | ToolState::Unknown => None,
    };
    let metadata = match &tool.state {
        ToolState::Running(state) => state.metadata.as_ref(),
        ToolState::Completed(state) => state.metadata.as_ref(),
        ToolState::Error(state) => state.metadata.as_ref(),
        ToolState::Streaming(_) | ToolState::Unknown => None,
    };
    let output = tool.state.output_text();
    let output = serde_json::from_str::<serde_json::Value>(&output).ok();
    let mut text = template.to_string();
    for (kind, value) in [
        ("input", input),
        ("output", output.as_ref()),
        ("metadata", metadata),
    ] {
        let mut cursor = 0;
        while let Some(start) = text[cursor..].find(&format!("{{{kind}.")) {
            let start = cursor + start;
            let Some(end) = text[start..].find('}') else {
                break;
            };
            let end = start + end;
            let path = &text[start + kind.len() + 2..end];
            let replacement = value
                .and_then(|value| json_path(value, path))
                .map(|value| match value {
                    serde_json::Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            text.replace_range(start..=end, &replacement);
            cursor = start + replacement.len();
        }
    }
    text
}

fn json_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn render_tool_output(renderer: Option<&ToolRenderer>, output: &str) -> Option<String> {
    let renderer = renderer?;
    match renderer.output.as_deref() {
        Some("table") => {
            let value = serde_json::from_str::<serde_json::Value>(output).ok()?;
            let items = value
                .as_array()
                .cloned()
                .or_else(|| {
                    value
                        .get("items")
                        .and_then(|items| items.as_array())
                        .cloned()
                })
                .unwrap_or_default();
            let columns = if renderer.columns.is_empty() {
                items
                    .first()
                    .and_then(|item| item.as_object())
                    .map(|object| object.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default()
            } else {
                renderer.columns.clone()
            };
            if columns.is_empty() {
                return None;
            }
            let mut text = String::new();
            text.push_str("| ");
            text.push_str(&columns.join(" | "));
            text.push_str(" |\n| ");
            text.push_str(
                &columns
                    .iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
            text.push_str(" |\n");
            for item in items.iter().take(50) {
                text.push_str("| ");
                for column in &columns {
                    let cell = item
                        .get(column)
                        .map(|value| match value {
                            serde_json::Value::String(text) => text.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default();
                    text.push_str(&cell.replace('|', "\\|"));
                    text.push_str(" | ");
                }
                text.push('\n');
            }
            Some(text)
        }
        Some("json") => {
            let value = serde_json::from_str::<serde_json::Value>(output).ok()?;
            Some(format!(
                "```json\n{}\n```",
                serde_json::to_string_pretty(&value).ok()?
            ))
        }
        Some("code") => Some(format!(
            "```{}\n{}\n```",
            renderer.language.as_deref().unwrap_or(""),
            output
        )),
        _ => None,
    }
}

fn extension_icon(name: &str) -> Icon {
    match name {
        "rocket" | "spark" | "sparkles" => Icon::Spark,
        "git-merge" | "git-branch" | "branch" => Icon::Branch,
        "check" | "checkbox-circle" | "check-circle" => Icon::CircleCheck,
        "window" | "panel-right" | "layout-right" => Icon::PanelRight,
        "file" | "file-text" => Icon::File,
        "terminal" | "command" => Icon::Terminal,
        "time" | "clock" => Icon::Clock,
        "chart" | "bar-chart" => Icon::ChartBar,
        _ => Icon::Puzzle,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolStatus {
    Pending,
    Running,
    Done,
    Error,
    Unknown,
}

fn tool_status(state: &ToolState) -> ToolStatus {
    match state {
        ToolState::Streaming(_) => ToolStatus::Pending,
        ToolState::Running(_) => ToolStatus::Running,
        ToolState::Completed(_) => ToolStatus::Done,
        ToolState::Error(_) => ToolStatus::Error,
        ToolState::Unknown => ToolStatus::Unknown,
    }
}

fn tool_row(
    ui: &mut Ui,
    p: &Palette,
    md: &mut Markdown,
    tool: &ToolPart,
    open_file: &mut Option<String>,
) -> Rect {
    let id = Id::unique(("tool", tool.id.as_str()));
    let mut open = ui
        .memory(|memory| memory.data.get_temp::<bool>(id.with("open")))
        .unwrap_or(false);
    let status = tool_status(&tool.state);
    let error = status == ToolStatus::Error;
    let running = matches!(status, ToolStatus::Running | ToolStatus::Pending);
    let renderer = tool_renderer(ui.ctx(), &tool.name);

    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), row_height_of(ui)),
        Sense::click(),
    );
    let hover = motion::Animated::from_id(id.with("hover"))
        .with_duration(motion::HOVER)
        .bool(ui.ctx(), response.hovered() || open);

    let border = if error {
        p.error_border
    } else if running {
        p.accent_border
    } else {
        p.border
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.card);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::radius::MD),
        Stroke::new(theme::BORDER, border),
        StrokeKind::Inside,
    );

    let center = rect.center().y;
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 10.0 + 7.0, center),
        Vec2::splat(ROW_ICON),
    );
    if hover < 0.999 && !open {
        icons::paint(
            ui,
            renderer
                .as_ref()
                .and_then(|renderer| renderer.icon.as_deref())
                .map(extension_icon)
                .unwrap_or_else(|| tool_icon(&tool.name)),
            icon_rect,
            if error {
                p.error_text
            } else {
                p.tool_icon
            }
            .gamma_multiply(1.0 - hover),
        );
    }
    if hover > 0.001 || open {
        icons::paint(
            ui,
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            icon_rect,
            if error {
                p.error_text
            } else {
                p.tool_icon
            }
            .gamma_multiply(if open { 1.0 } else { hover }),
        );
    }

    let title_color = if error { p.error_text } else { p.tool_title };
    let title = renderer
        .as_ref()
        .and_then(|renderer| renderer.title.as_deref())
        .map(|template| render_template(template, tool))
        .filter(|title| !title.trim().is_empty())
        .or_else(|| renderer.as_ref().and_then(|renderer| renderer.name.clone()))
        .unwrap_or_else(|| tool_label(&tool.name).to_string());
    let title_galley = row_galley(
        ui,
        &title,
        theme::font_semibold(ROW_TITLE),
        title_color,
        (rect.width() * 0.45).max(80.0),
    );
    let mut x = icon_rect.right() + 7.0;
    ui.painter().galley(
        Pos2::new(x, center - title_galley.size().y * 0.5),
        title_galley.clone(),
        title_color,
    );
    x += title_galley.size().x;

    let description = tool_description(tool, error, renderer.as_ref());
    let (additions, deletions) = tool_patch_total(tool);
    let duration_text = tool_duration(tool);

    let check = Rect::from_center_size(
        Pos2::new(rect.right() - 10.0 - 6.0, center),
        Vec2::splat(12.0),
    );
    let status_ink = if error { p.error_text } else { p.green };
    if running {
        busy_dots(ui, Pos2::new(check.left() - 16.0, center), p.primary);
    } else if error {
        icons::paint(ui, Icon::Warning, check, status_ink);
    } else if status == ToolStatus::Done {
        icons::paint(ui, Icon::Check, check, status_ink);
    }

    let mut right = check.left() - 6.0;
    let mono = theme::font_mono(11.5);
    if additions + deletions > 0 {
        let dels = ui.painter().layout_no_wrap(
            format!("-{deletions}"),
            mono.clone(),
            p.diff_removed,
        );
        right -= dels.size().x;
        ui.painter().galley(
            Pos2::new(right, center - dels.size().y * 0.5),
            dels.clone(),
            p.diff_removed,
        );
        right -= 4.0;
        let adds =
            ui.painter()
                .layout_no_wrap(format!("+{additions}"), mono.clone(), p.diff_added);
        right -= adds.size().x;
        ui.painter().galley(
            Pos2::new(right, center - adds.size().y * 0.5),
            adds.clone(),
            p.diff_added,
        );
        right -= 6.0;
    } else if !duration_text.is_empty() {
        let color = if running {
            p.primary_text
        } else if error {
            p.error_text
        } else {
            p.tool_description
        };
        let galley =
            ui.painter()
                .layout_no_wrap(duration_text.clone(), mono.clone(), color);
        right -= galley.size().x;
        ui.painter().galley(
            Pos2::new(right, center - galley.size().y * 0.5),
            galley.clone(),
            color,
        );
        right -= 6.0;
    }

    let path = tool_path(tool);
    let mut action_clicked = false;
    if let Some(path) = &path
        && hover > 0.001
    {
        let target = Rect::from_center_size(Pos2::new(right - 8.0, center), Vec2::splat(16.0));
        let open_response = ui.interact(target, id.with("open-file"), Sense::click());
        crate::widgets::pointer(ui, &open_response);
        let alpha = if open_response.hovered() { 1.0 } else { 0.5 };
        if open_response.hovered() {
            ui.painter()
                .rect_filled(target, CornerRadius::same(theme::radius::XS), p.hover);
            ui.painter().rect_stroke(
                target,
                CornerRadius::same(theme::radius::XS),
                Stroke::new(theme::HAIRLINE, p.border),
                StrokeKind::Inside,
            );
        }
        icons::paint(
            ui,
            Icon::ArrowUpRight,
            Rect::from_center_size(target.center(), Vec2::splat(11.0)),
            p.tool_icon.gamma_multiply(alpha),
        );
        if open_response.clicked() {
            action_clicked = true;
            *open_file = Some(path.clone());
        }
        right -= 18.0;
    }

    let start = x + 7.0;
    if !description.is_empty() && right > start {
        let galley = row_galley(
            ui,
            &description,
            theme::font_mono(theme::TEXT_MICRO),
            if error { p.error_text } else { p.tool_description },
            right - start,
        );
        ui.painter()
            .with_clip_rect(Rect::from_min_max(
                Pos2::new(start, center - 9.0),
                Pos2::new(right, center + 9.0),
            ))
            .galley(
                Pos2::new(start, center - galley.size().y * 0.5),
                galley,
                if error {
                    p.error_text
                } else {
                    p.tool_description
                },
            );
    }

    if response.clicked() && !action_clicked {
        open = !open;
        ui.memory_mut(|memory| memory.data.insert_temp(id.with("open"), open));
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if open {
        tool_body(ui, p, md, tool, id, renderer.as_ref());
    }
    if running {
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    rect
}

fn tool_description(tool: &ToolPart, error: bool, renderer: Option<&ToolRenderer>) -> String {
    if let Some(renderer) = renderer
        && let Some(subtitle) = &renderer.subtitle
    {
        let text = render_template(subtitle, tool);
        if !text.trim().is_empty() {
            return text;
        }
    }
    if error
        && let ToolState::Error(state) = &tool.state
        && let Some(error) = &state.error
        && !error.message.trim().is_empty()
    {
        return first_line(&error.message, 120);
    }
    let detail = detail_line(tool);
    if !detail.is_empty() {
        return detail;
    }
    if let ToolState::Streaming(stream) = &tool.state {
        return first_line(&stream.input, 120);
    }
    String::new()
}

fn tool_body(
    ui: &mut Ui,
    p: &Palette,
    md: &mut Markdown,
    tool: &ToolPart,
    id: Id,
    renderer: Option<&ToolRenderer>,
) {
    let output = tool.state.output_text();
    let input = tool_input_text(tool);
    let patches = tool_patches(tool);
    let shell = matches!(tool.name.as_str(), "bash" | "shell" | "cmd" | "terminal");
    if renderer.is_some()
        && !output.trim().is_empty()
        && patches.is_empty()
        && let Some(rendered) = render_tool_output(renderer, &output)
    {
        tool_output(ui, p, md, &rendered, id.with("output"));
        ui.add_space(2.0);
        return;
    }

    ui.add_space(2.0);
    if !patches.is_empty() {
        for (path, patch) in &patches {
            diff_block(ui, p, path, patch);
            ui.add_space(4.0);
        }
    }
    if !output.trim().is_empty() {
        if shell {
            output_block(ui, p, &output, id.with("output"));
        } else {
            tool_output(ui, p, md, &output, id.with("output"));
        }
    } else if patches.is_empty() && !input.trim().is_empty() {
        if shell {
            output_block(ui, p, &input, id.with("input"));
        } else {
            input_block(ui, p, &input, id.with("input"));
        }
    }
    if let ToolState::Error(state) = &tool.state
        && let Some(error) = &state.error
        && !error.message.trim().is_empty()
    {
        ui.add_space(4.0);
        let label = RichText::new(&error.message)
            .font(theme::font(ROW_TEXT))
            .color(p.red);
        ui.add(egui::Label::new(label).wrap_mode(egui::TextWrapMode::Wrap));
    }
    ui.add_space(2.0);
}

fn tool_output(ui: &mut Ui, p: &Palette, md: &mut Markdown, text: &str, id: Id) {
    ScrollArea::vertical()
        .id_salt(id)
        .max_height(OUTPUT_MAX)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            md.show_meta(ui, id.with("md"), text, p);
        });
}

fn output_block(ui: &mut Ui, p: &Palette, text: &str, id: Id) {
    egui::Frame::new()
        .fill(tile_fill(p))
        .corner_radius(CornerRadius::same(theme::radius::XL))
        .inner_margin(Margin {
            left: 10,
            right: 6,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::both()
                .id_salt(id)
                .max_height(OUTPUT_MAX)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text)
                                .font(theme::font_mono(theme::TEXT_CODE))
                                .color(p.fg_muted)
                                .line_height(Some(theme::LINE_CODE)),
                        )
                        .selectable(true)
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}

fn input_block(ui: &mut Ui, p: &Palette, text: &str, id: Id) {
    ScrollArea::both()
        .id_salt(id)
        .max_height(OUTPUT_MAX)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .font(theme::font_mono(theme::TEXT_CODE))
                        .color(p.fg_muted)
                        .line_height(Some(theme::LINE_CODE)),
                )
                .selectable(true)
                .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
}

fn diff_block(ui: &mut Ui, p: &Palette, path: &str, patch: &str) {
    egui::Frame::new()
        .fill(tile_fill(p))
        .corner_radius(CornerRadius::same(theme::radius::XL))
        .inner_margin(Margin {
            left: 10,
            right: 6,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(path)
                    .font(theme::font_medium(theme::TEXT_META))
                    .color(p.fg),
            );
            ui.add_space(4.0);
            ScrollArea::both()
                .id_salt(("diff", path, patch.len()))
                .max_height(OUTPUT_MAX)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for line in patch.lines() {
                        draw_diff_line(ui, p, line);
                    }
                });
        });
}

fn draw_diff_line(ui: &mut Ui, p: &Palette, line: &str) {
    let (kind, color) = if line.starts_with('+') && !line.starts_with("+++") {
        (1, p.green)
    } else if line.starts_with('-') && !line.starts_with("---") {
        (2, p.red)
    } else if line.starts_with("@@") {
        (3, p.fg_muted)
    } else {
        (0, p.fg_muted)
    };
    let galley =
        ui.painter()
            .layout_no_wrap(line.to_string(), theme::font_mono(theme::TEXT_CODE), color);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(galley.size().x + 8.0), DIFF_LINE),
        Sense::hover(),
    );
    match kind {
        1 => {
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, p.alpha(p.green, 0x25));
        }
        2 => {
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, p.alpha(p.red, 0x25));
        }
        3 => {
            ui.painter().rect_filled(rect, CornerRadius::ZERO, p.hover);
        }
        _ => {}
    }
    ui.painter().galley(
        Pos2::new(rect.left() + 4.0, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
}

fn tool_input_text(tool: &ToolPart) -> String {
    if let ToolState::Streaming(stream) = &tool.state {
        return stream.input.clone();
    }
    let Some(input) = tool.state.input() else {
        return String::new();
    };
    match tool.name.as_str() {
        "bash" | "shell" => input
            .get("command")
            .and_then(|value| value.as_str())
            .map(|command| format!("$ {command}"))
            .unwrap_or_else(|| pretty(input)),
        _ => pretty(input),
    }
}

fn tool_patches(tool: &ToolPart) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let metadata = match &tool.state {
        ToolState::Running(state) => state.metadata.as_ref(),
        ToolState::Completed(state) => state.metadata.as_ref(),
        ToolState::Error(state) => state.metadata.as_ref(),
        _ => None,
    };
    if let Some(metadata) = metadata {
        if let Some(files) = metadata.get("files").and_then(|value| value.as_array()) {
            for file in files {
                let patch = file
                    .get("patch")
                    .or_else(|| file.get("diff"))
                    .and_then(|value| value.as_str());
                if let Some(patch) = patch {
                    let path = file
                        .get("file")
                        .or_else(|| file.get("filePath"))
                        .or_else(|| file.get("relativePath"))
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    out.push((path, patch.to_string()));
                }
            }
        }
        if out.is_empty()
            && let Some(patch) = metadata
                .get("patch")
                .or_else(|| metadata.get("diff"))
                .and_then(|value| value.as_str())
        {
            out.push((tool_path(tool).unwrap_or_default(), patch.to_string()));
        }
        if out.is_empty()
            && let Some(file) = metadata.get("filediff")
            && let Some(patch) = file
                .get("patch")
                .or_else(|| file.get("diff"))
                .and_then(|value| value.as_str())
        {
            let path = file
                .get("file")
                .or_else(|| file.get("filePath"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            out.push((path, patch.to_string()));
        }
    }
    if out.is_empty()
        && let Some(input) = tool.state.input()
    {
        let old = input
            .get("oldString")
            .or_else(|| input.get("old_string"))
            .and_then(|value| value.as_str());
        let new = input
            .get("newString")
            .or_else(|| input.get("new_string"))
            .and_then(|value| value.as_str());
        if let (Some(old), Some(new)) = (old, new) {
            let path = tool_path(tool).unwrap_or_default();
            let mut patch = String::new();
            for line in old.lines() {
                patch.push('-');
                patch.push_str(line);
                patch.push('\n');
            }
            for line in new.lines() {
                patch.push('+');
                patch.push_str(line);
                patch.push('\n');
            }
            out.push((path, patch));
        }
    }
    out
}

fn tool_diff_total(turn: &Turn<'_>) -> (usize, usize) {
    let mut additions = 0;
    let mut deletions = 0;
    for step in &turn.steps {
        for part in &step.content {
            let Part::Tool(tool) = part else {
                continue;
            };
            for (_, patch) in tool_patches(tool) {
                let (adds, dels) = count_patch(&patch);
                additions += adds;
                deletions += dels;
            }
        }
    }
    (additions, deletions)
}

fn tool_file_diff(tool: &ToolPart, path: &str) -> (usize, usize) {
    for (file, patch) in tool_patches(tool) {
        if file == path || file.ends_with(path) || path.ends_with(&file) {
            return count_patch(&patch);
        }
    }
    (0, 0)
}

fn count_patch(patch: &str) -> (usize, usize) {
    let mut additions = 0;
    let mut deletions = 0;
    for line in patch.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            additions += 1;
        } else if line.starts_with('-') {
            deletions += 1;
        }
    }
    (additions, deletions)
}

fn detail_line(tool: &ToolPart) -> String {
    tool.state
        .input()
        .map(|input| tool_detail(&tool.name, Some(input)))
        .or_else(|| match &tool.state {
            ToolState::Streaming(stream) if !stream.input.is_empty() => Some(stream.input.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn tool_path(tool: &ToolPart) -> Option<String> {
    if !is_file_tool(&tool.name) {
        return None;
    }
    tool.state
        .input()
        .and_then(|input| {
            input
                .get("filePath")
                .or_else(|| input.get("file_path"))
                .or_else(|| input.get("path"))
                .and_then(|value| value.as_str())
        })
        .map(str::to_string)
}

fn is_file_tool(name: &str) -> bool {
    matches!(
        name,
        "read"
            | "read_file"
            | "view"
            | "write"
            | "write_file"
            | "create"
            | "edit"
            | "edit_file"
            | "multi_edit"
            | "patch"
            | "apply_patch"
            | "str_replace"
    )
}

fn tool_patch_total(tool: &ToolPart) -> (usize, usize) {
    let mut additions = 0;
    let mut deletions = 0;
    for (_, patch) in tool_patches(tool) {
        let (adds, dels) = count_patch(&patch);
        additions += adds;
        deletions += dels;
    }
    (additions, deletions)
}

fn tool_duration(tool: &ToolPart) -> String {
    if tool.state.is_running() {
        let start = tool.time.ran.unwrap_or(tool.time.created);
        let now = now_ms();
        if start > 0.0 && now > start {
            return elapsed_label(now - start);
        }
        return String::new();
    }
    if tool.time.created <= 0.0 {
        return String::new();
    }
    tool.time
        .completed
        .or(tool.time.ran)
        .filter(|end| *end >= tool.time.created)
        .map(|end| elapsed_label(end - tool.time.created))
        .unwrap_or_default()
}

fn elapsed_label(ms: f64) -> String {
    if ms < 1_000.0 {
        format!("{} ms", ms.round().max(1.0) as i64)
    } else if ms < 60_000.0 {
        format!("{:.1} s", ms / 1000.0)
    } else {
        turn_duration(ms)
    }
}

fn tool_icon(name: &str) -> Icon {
    match name {
        "edit" | "edit_file" | "multi_edit" | "patch" | "apply_patch" | "str_replace" => {
            Icon::Pencil
        }
        "write" | "write_file" | "create" => Icon::File,
        "read" | "read_file" | "view" | "cat" | "glob" | "list" => Icon::File,
        "grep" | "search" | "find" => Icon::Search,
        "bash" | "shell" | "cmd" | "terminal" => Icon::Terminal,
        "webfetch" | "fetch" | "websearch" | "search_web" => Icon::ArrowUpRight,
        "task" | "agent" => Icon::Layers,
        "todowrite" | "todo" => Icon::Check,
        _ => Icon::Spark,
    }
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn error_card(ui: &mut Ui, p: &Palette, md: &mut Markdown, title: Option<&str>, message: &str) {
    egui::Frame::new()
        .fill(p.wash(p.blue, 32))
        .corner_radius(CornerRadius::same(theme::radius::CODE_BLOCK))
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 12,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                icons::paint(ui, Icon::Info, icon, p.blue);
                ui.vertical(|ui| {
                    if let Some(title) = title {
                        ui.label(
                            RichText::new(title)
                                .font(theme::font_medium(theme::TEXT_UI_LABEL))
                                .color(p.fg),
                        );
                        ui.add_space(2.0);
                    }
                    let id = ui.next_auto_id();
                    md.show_at(ui, id, message, p);
                });
            });
        });
}

fn permission_title(action: &str) -> String {
    let lower = action.to_ascii_lowercase();
    if lower.contains("bash")
        || lower.contains("shell")
        || lower.contains("command")
        || lower.contains("execute")
    {
        "Run shell command".into()
    } else if lower.contains("edit") || lower.contains("write") || lower.contains("patch") {
        "Edit file".into()
    } else if lower.contains("read") || lower.contains("view") {
        "Read file".into()
    } else if lower.contains("fetch") || lower.contains("network") || lower.contains("http") {
        "Access network".into()
    } else if action.trim().is_empty() {
        "Use tool".into()
    } else {
        capitalize(action.trim())
    }
}

fn permission_command(request: &rchamber_api::models::PermissionRequest) -> Option<String> {
    request
        .resources
        .iter()
        .find(|resource| !resource.trim().is_empty())
        .cloned()
        .or_else(|| {
            request
                .message
                .clone()
                .filter(|message| !message.trim().is_empty())
        })
}

fn permission_cwd(request: &rchamber_api::models::PermissionRequest) -> Option<String> {
    let metadata = request.metadata.as_ref()?;
    ["cwd", "directory", "dir", "workdir"]
        .iter()
        .find_map(|key| metadata.get(key).and_then(|value| value.as_str()))
        .map(str::to_string)
}

fn permission_tag(request: &rchamber_api::models::PermissionRequest, command: Option<&str>) -> String {
    if !request.action.trim().is_empty() {
        return request.action.trim().to_ascii_lowercase();
    }
    command
        .and_then(|command| command.split_whitespace().next())
        .unwrap_or("tool")
        .to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApprovalKind {
    Primary,
    Secondary,
    Danger,
}

fn approval_button(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    hint: &str,
    kind: ApprovalKind,
) -> bool {
    let font = theme::font_medium(theme::TEXT_BUTTON);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let hint_galley = ui.painter().layout_no_wrap(
        hint.to_string(),
        theme::font_mono(10.5),
        Color32::PLACEHOLDER,
    );
    let width = 10.0 + galley.size().x + 6.0 + hint_galley.size().x + 8.0 + 10.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, theme::BUTTON_HEIGHT),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let (fill, border, ink) = match kind {
        ApprovalKind::Primary => (p.primary, Color32::TRANSPARENT, p.on_primary),
        ApprovalKind::Secondary => (p.raised_fill(), p.border, p.fg),
        ApprovalKind::Danger => (p.error_soft, p.error_border, p.error_text),
    };
    let hovered = response.hovered();
    let fill = match kind {
        ApprovalKind::Primary => {
            if hovered {
                p.primary_hover
            } else {
                fill
            }
        }
        _ => {
            if hovered {
                p.over(fill, p.hover)
            } else {
                fill
            }
        }
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::DEMO), fill);
    if border != Color32::TRANSPARENT {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::radius::DEMO),
            Stroke::new(theme::BORDER, border),
            StrokeKind::Inside,
        );
    }
    let text_left = rect.left() + 10.0;
    ui.painter().galley(
        Pos2::new(text_left, rect.center().y - galley.size().y * 0.5),
        galley,
        ink,
    );
    let hint_rect = Rect::from_center_size(
        Pos2::new(
            rect.right() - 8.0 - (hint_galley.size().x + 8.0) * 0.5,
            rect.center().y,
        ),
        Vec2::new(hint_galley.size().x + 8.0, 15.0),
    );
    ui.painter().rect_filled(
        hint_rect,
        CornerRadius::same(3),
        p.alpha(ink, 22),
    );
    ui.painter().galley(
        Pos2::new(
            hint_rect.center().x - hint_galley.size().x * 0.5,
            rect.center().y - hint_galley.size().y * 0.5,
        ),
        hint_galley,
        ink.gamma_multiply(0.82),
    );
    response.clicked()
}

pub(crate) fn approval_card(
    ui: &mut Ui,
    p: &Palette,
    request: &rchamber_api::models::PermissionRequest,
    index: usize,
    total: usize,
    actions: &mut Vec<Action>,
) {
    let command = permission_command(request);
    let cwd = permission_cwd(request);
    let tag = permission_tag(request, command.as_deref());
    let mut decision: Option<&str> = None;
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(theme::BORDER, p.warning_border))
        .corner_radius(CornerRadius::same(theme::radius::PANEL))
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 10,
            bottom: 10,
        });
    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
            let (icon, _) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
            icons::paint(ui, Icon::Shield, icon, p.warning_text);
            let mut title = format!("Permission required · {}", permission_title(&request.action));
            if total > 1 {
                title.push_str(&format!(" ({}/{total})", index + 1));
            }
            ui.label(
                RichText::new(title)
                    .font(theme::font_semibold(theme::TEXT_UI_LABEL))
                    .color(p.fg),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(cwd) = &cwd {
                    ui.label(
                        RichText::new(format!("cwd: {}", shorten(cwd, 48)))
                            .font(theme::font_mono(theme::TEXT_MICRO))
                            .color(p.fg_muted),
                    );
                }
            });
        });
        if let Some(command) = &command {
            let width = ui.available_width();
            egui::Frame::new()
                .fill(p.code_bg)
                .stroke(Stroke::new(theme::BORDER, p.border))
                .corner_radius(CornerRadius::same(theme::radius::SM))
                .inner_margin(Margin {
                    left: 10,
                    right: 10,
                    top: 6,
                    bottom: 6,
                })
                .show(ui, |ui| {
                    let inner = (width - 22.0).max(40.0);
                    ui.set_width(inner);
                    let tag_galley = ui.painter().layout_no_wrap(
                        tag.clone(),
                        theme::font_mono(theme::TEXT_TAG),
                        p.fg_muted,
                    );
                    let tag_width = tag_galley.size().x + 12.0;
                    let text_max = (inner - tag_width - 8.0).max(40.0);
                    let galley = row_galley(
                        ui,
                        &shorten(command, 180),
                        theme::font_mono(theme::TEXT_CODE),
                        p.fg,
                        text_max,
                    );
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(inner, galley.size().y.max(18.0)), Sense::hover());
                    ui.painter().galley(
                        Pos2::new(rect.left(), rect.center().y - galley.size().y * 0.5),
                        galley,
                        p.fg,
                    );
                    let chip = Rect::from_center_size(
                        Pos2::new(rect.right() - tag_width * 0.5, rect.center().y),
                        Vec2::new(tag_width, theme::TAG_HEIGHT),
                    );
                    ui.painter().rect_filled(
                        chip,
                        CornerRadius::same(theme::radius::XS),
                        p.tag_bg,
                    );
                    ui.painter().galley(
                        Pos2::new(
                            chip.center().x - tag_galley.size().x * 0.5,
                            chip.center().y - tag_galley.size().y * 0.5,
                        ),
                        tag_galley,
                        p.fg_muted,
                    );
                });
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
            if approval_button(ui, p, "Allow once", "↵", ApprovalKind::Primary) {
                decision = Some("once");
            }
            if approval_button(
                ui,
                p,
                "Allow in this session",
                "⌘↵",
                ApprovalKind::Secondary,
            ) {
                decision = Some("always");
            }
            if approval_button(ui, p, "Deny", "Esc", ApprovalKind::Danger) {
                decision = Some("reject");
            }
        });
    });
    let rect = response.response.rect;
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left() + 1.0, rect.top() + 1.0),
            Vec2::new(3.0, (rect.height() - 2.0).max(0.0)),
        ),
        CornerRadius::same(1),
        p.warning_text,
    );
    if let Some(decision) = decision {
        actions.push(Action::ReplyPermission {
            session_id: request.session_id.clone(),
            request_id: request.id.clone(),
            decision: decision.into(),
        });
    }
}

fn scroll_pill(
    ctx: &egui::Context,
    app: &RustChamber,
    p: &Palette,
    session_id: &str,
    dock: Dock,
    away: bool,
) {
    let id = Id::unique(("pill-visible", session_id));
    let Some(t) = motion::presence(ctx, id, away, motion::REVEAL, motion::REVEAL) else {
        return;
    };
    if t <= 0.001 {
        return;
    }
    let busy = app.model.active.as_ref().is_some_and(|chat| chat.busy);
    let label = if busy {
        app.model
            .active
            .as_ref()
            .map(status_label)
            .unwrap_or_else(|| "Working".into())
    } else {
        String::new()
    };
    let position = pill_origin(dock);
    egui::Area::new(Id::unique(("scroll-pill", session_id)))
        .order(egui::Order::Middle)
        .fixed_pos(position)
        .interactable(away)
        .show(ctx, |ui| {
            let galley = if label.is_empty() {
                None
            } else {
                Some(row_galley(
                    ui,
                    &label,
                    theme::font(13.0),
                    p.fg_muted.gamma_multiply(t),
                    PILL_LABEL_MAX,
                ))
            };
            let label_width = galley.as_ref().map_or(0.0, |galley| galley.size().x);
            let width = if label.is_empty() {
                PILL_HEIGHT
            } else {
                PILL_HEIGHT + label_width + PILL_LABEL_PAD_END
            };
            let border = if p.dark {
                Color32::from_white_alpha((20.0 * t) as u8)
            } else {
                Color32::from_black_alpha((15.0 * t) as u8)
            };
            let radius = CornerRadius::same(theme::radius::PILL);
            let shadow = egui::epaint::Shadow {
                offset: [0, 2],
                blur: 6,
                spread: 0,
                color: Color32::from_black_alpha((if p.dark { 89.0 } else { 26.0 } * t) as u8),
            };
            let caster = Rect::from_min_size(ui.cursor().min, Vec2::new(width, PILL_HEIGHT));
            ui.painter()
                .add(shadow.as_shape(caster.shrink(2.0), radius));
            egui::Frame::new()
                .fill(p.popover.gamma_multiply(t))
                .corner_radius(radius)
                .stroke(Stroke::new(theme::BORDER, border))
                .show(ui, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, PILL_HEIGHT), Sense::click());
                    icons::paint(
                        ui,
                        Icon::ArrowDown,
                        Rect::from_center_size(
                            Pos2::new(rect.left() + PILL_HEIGHT * 0.5, rect.center().y),
                            Vec2::splat(16.0),
                        ),
                        p.fg_muted.gamma_multiply(t),
                    );
                    if let Some(galley) = &galley {
                        ui.painter().galley(
                            Pos2::new(
                                rect.left() + PILL_HEIGHT,
                                rect.center().y - galley.size().y * 0.5,
                            ),
                            galley.clone(),
                            p.fg_muted,
                        );
                    }
                    if response.clicked() {
                        ui.memory_mut(|memory| {
                            memory
                                .data
                                .insert_temp(Id::unique(("chat-pin", session_id)), true);
                        });
                    }
                });
        });
}

fn status_chip(
    ctx: &egui::Context,
    app: &RustChamber,
    p: &Palette,
    session_id: &str,
    dock: Dock,
    away: bool,
) {
    if away {
        return;
    }
    let state_id = Id::unique(("chat-status-label", session_id));
    let Some(chat) = &app.model.active else {
        return;
    };
    if !chat.busy {
        ctx.data_mut(|data| {
            data.remove::<(String, f64)>(state_id);
        });
        return;
    }
    let candidate = status_label(chat);
    let retrying = chat.messages.iter().rev().any(|message| match message {
        Message::Assistant(assistant) => assistant.retry.is_some(),
        _ => false,
    });
    let now = ctx.input(|input| input.time);
    let stored = ctx.memory(|memory| memory.data.get_temp::<(String, f64)>(state_id));
    let (label, since) = match stored {
        Some((shown, since)) if shown == candidate => (shown, since),
        Some((shown, since)) if !retrying && now - since < STATUS_MIN_DISPLAY => {
            let remaining = STATUS_MIN_DISPLAY - (now - since);
            ctx.request_repaint_after(Duration::from_secs_f64(remaining.max(0.01)));
            (shown, since)
        }
        _ => (candidate, now),
    };
    ctx.memory_mut(|memory| memory.data.insert_temp(state_id, (label.clone(), since)));
    let mark = status_mark(chat, &app.model.providers);
    let font = theme::font(13.0);
    let galley = ctx.fonts_mut(|fonts| fonts.layout_no_wrap(label, font, p.fg_muted));
    let width = STATUS_PAD_START + STATUS_MARK + STATUS_GAP + galley.size().x + STATUS_PAD_END;
    let position = pill_origin(dock);
    egui::Area::new(Id::unique(("chat-status", session_id)))
        .order(egui::Order::Middle)
        .fixed_pos(position)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.popover)
                .corner_radius(CornerRadius::same(theme::radius::PILL))
                .show(ui, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(width, PILL_HEIGHT), Sense::hover());
                    let center = Pos2::new(
                        rect.left() + STATUS_PAD_START + STATUS_MARK * 0.5,
                        rect.center().y,
                    );
                    let color = mark
                        .provider_id
                        .as_deref()
                        .map(|provider| provider_color(provider, p))
                        .unwrap_or(p.tool_icon);
                    ui.painter().circle_filled(center, STATUS_MARK * 0.5, color);
                    ui.painter().text(
                        center,
                        Align2::CENTER_CENTER,
                        &mark.initial,
                        theme::font_semibold(STATUS_MARK_INITIAL),
                        mark_ink(color),
                    );
                    ui.painter().galley(
                        Pos2::new(
                            rect.left() + STATUS_PAD_START + STATUS_MARK + STATUS_GAP,
                            rect.center().y - galley.size().y * 0.5,
                        ),
                        galley,
                        p.fg_muted,
                    );
                });
        });
    ctx.request_repaint_after(Duration::from_millis(500));
}

struct StatusMark {
    provider_id: Option<String>,
    initial: String,
}

fn status_mark(chat: &rchamber_app::state::Chat, providers: &[Provider]) -> StatusMark {
    if let Some(model) = model_ref(chat) {
        let provider_id = model.provider_id.clone();
        let name = providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .map(|provider| provider.name.as_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(provider_id.as_str());
        if let Some(initial) = initial_of(name).or_else(|| initial_of(&model.id)) {
            return StatusMark {
                provider_id: Some(provider_id),
                initial,
            };
        }
    }
    let initial = chat
        .agent
        .as_deref()
        .and_then(initial_of)
        .unwrap_or_else(|| "A".into());
    StatusMark {
        provider_id: None,
        initial,
    }
}

fn model_ref(chat: &rchamber_app::state::Chat) -> Option<&ModelRef> {
    chat.model.as_ref().or_else(|| {
        chat.messages
            .iter()
            .rev()
            .find_map(|message| match message {
                Message::Assistant(assistant) => assistant.model.as_ref(),
                _ => None,
            })
    })
}

fn initial_of(text: &str) -> Option<String> {
    text.chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().collect())
}

pub(crate) fn provider_color(provider_id: &str, p: &Palette) -> Color32 {
    match provider_id.to_ascii_lowercase().as_str() {
        "anthropic" | "claude" => Color32::from_rgb(0xD9, 0x77, 0x57),
        "openai" => Color32::from_rgb(0x10, 0xA3, 0x7F),
        "google" | "google-vertex" | "google-vertex-anthropic" | "gemini" => {
            Color32::from_rgb(0x42, 0x85, 0xF4)
        }
        "github" | "github-copilot" => Color32::from_rgb(0x6E, 0x76, 0x81),
        "openrouter" => Color32::from_rgb(0x63, 0x66, 0xF1),
        "groq" => Color32::from_rgb(0xF5, 0x50, 0x36),
        "mistral" => Color32::from_rgb(0xFA, 0x52, 0x0F),
        "deepseek" => Color32::from_rgb(0x4D, 0x6B, 0xFE),
        "amazon-bedrock" | "aws" => Color32::from_rgb(0xFF, 0x99, 0x00),
        "azure" => Color32::from_rgb(0x00, 0x78, 0xD4),
        "xai" => {
            if p.dark {
                Color32::from_rgb(0x9A, 0x9A, 0x9A)
            } else {
                Color32::from_rgb(0x1F, 0x1F, 0x1F)
            }
        }
        "opencode" => p.primary,
        _ => {
            let accents = [p.purple, p.blue, p.green, p.coral, p.primary];
            let index = provider_id
                .bytes()
                .fold(0usize, |sum, byte| sum.wrapping_add(byte as usize))
                % accents.len();
            accents[index]
        }
    }
}

fn mark_ink(color: Color32) -> Color32 {
    let [red, green, blue, _] = color.to_srgba_unmultiplied();
    let luma = 0.2126 * linear_channel(red)
        + 0.7152 * linear_channel(green)
        + 0.0722 * linear_channel(blue);
    if luma > 0.4 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn linear_channel(value: u8) -> f32 {
    let value = value as f32 / 255.0;
    if value <= 0.03928 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn status_label(chat: &rchamber_app::state::Chat) -> String {
    if let Some(retry) = chat
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::Assistant(assistant) => assistant.retry.as_ref(),
            _ => None,
        })
    {
        return retry_label(retry);
    }
    if chat.awaiting_permission {
        return "Waiting for permission".into();
    }
    let activity = chat
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::Assistant(assistant) => Some(assistant.as_ref()),
            _ => None,
        })
        .map(working_phrase)
        .unwrap_or_else(|| "working".into());
    match chat.model.as_ref().map(|model| display_model(&model.id)) {
        Some(model) if !model.is_empty() => format!("{model} is {activity}"),
        _ => capitalize(&activity),
    }
}

fn working_phrase(assistant: &AssistantMessage) -> String {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for part in &assistant.content {
        if let Part::Tool(tool) = part
            && tool.state.is_running()
        {
            *counts.entry(tool_bucket(&tool.name)).or_default() += 1;
        }
    }
    let streaming = assistant.time.completed.is_none();
    for part in assistant.content.iter().rev() {
        match part {
            Part::Tool(tool) if tool.state.is_running() => {
                let bucket = tool_bucket(&tool.name);
                let count = counts.get(bucket).copied().unwrap_or(1);
                return match bucket {
                    "read" if count > 1 => format!("reading {count} files"),
                    "read" => "reading a file".into(),
                    "edit" if count > 1 => format!("editing {count} files"),
                    "edit" => "editing a file".into(),
                    "command" if count > 1 => format!("running {count} commands"),
                    "command" => "running a command".into(),
                    "search" => "searching files".into(),
                    "lookup" => "looking things up".into(),
                    "delegate" if count > 1 => format!("handing off {count} tasks"),
                    "delegate" => "handing off a task".into(),
                    _ => format!("using {}", tool_label(&tool.name)),
                };
            }
            Part::Reasoning(reasoning) if streaming && !reasoning.text.trim().is_empty() => {
                return "thinking".into();
            }
            Part::Text(text) if streaming && !text.text.trim().is_empty() => {
                return "composing".into();
            }
            _ => {}
        }
    }
    "working".into()
}

fn tool_bucket(name: &str) -> &'static str {
    match name {
        "read" | "read_file" | "view" | "cat" => "read",
        "grep" | "search" | "glob" | "list" | "find" => "search",
        "edit" | "edit_file" | "multi_edit" | "write" | "write_file" | "patch" | "apply_patch" => {
            "edit"
        }
        "bash" | "shell" | "cmd" | "terminal" => "command",
        "webfetch" | "fetch" | "websearch" | "search_web" => "lookup",
        "task" | "agent" => "delegate",
        _ => "other",
    }
}

fn retry_label(retry: &RetryInfo) -> String {
    let remaining = ((retry.at - now_ms()) / 1000.0).ceil().max(0.0) as i64;
    if remaining > 0 {
        format!(
            "Retrying in {remaining}s (attempt {})",
            retry.attempt.max(1)
        )
    } else {
        format!("Retrying (attempt {})", retry.attempt.max(1))
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn tile_fill(p: &Palette) -> Color32 {
    p.alpha(p.fg, if p.dark { 8 } else { 10 })
}

fn row_galley(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::text::TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap.max_width = width.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

fn first_line(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    shorten(line, max)
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn fade_band(painter: &egui::Painter, rect: Rect, from: Color32, to: Color32) {
    if let Some(mesh) = fade_mesh(rect, from, to) {
        painter.add(Shape::mesh(mesh));
    }
}

fn fade_mesh(rect: Rect, from: Color32, to: Color32) -> Option<Mesh> {
    if rect.height() <= 0.5 || rect.width() <= 0.5 {
        return None;
    }
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: rect.left_top(),
        uv: WHITE_UV,
        color: from,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_top(),
        uv: WHITE_UV,
        color: from,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_bottom(),
        uv: WHITE_UV,
        color: to,
    });
    mesh.vertices.push(Vertex {
        pos: rect.left_bottom(),
        uv: WHITE_UV,
        color: to,
    });
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> UserMessage {
        UserMessage {
            id: format!("u{}", text.len()),
            time: Default::default(),
            text: text.to_string(),
            files: Vec::new(),
            agents: Vec::new(),
            skills: Vec::new(),
            metadata: None,
        }
    }

    #[test]
    fn user_bubble_is_content_sized() {
        let palette = Palette::light();
        let mut md = Markdown::default();
        egui::__run_test_ui(|ui| {
            ui.set_width(768.0);
            ui.set_max_height(1_000_000.0);
            let rect = user_row(ui, &palette, &mut md, &user(&"hello world ".repeat(40)));
            assert!(rect.width() <= 768.0 * USER_BUBBLE_MAX + 1.0);
            let rect = user_row(ui, &palette, &mut md, &user("Hi"));
            assert!(rect.width() < 200.0, "short bubble {}", rect.width());
        });
    }

    #[test]
    fn attachment_chips_do_not_squeeze_the_bubble_text() {
        let palette = Palette::light();
        let mut md = Markdown::default();
        egui::__run_test_ui(|ui| {
            ui.set_width(768.0);
            ui.set_max_height(1_000_000.0);
            let mut message = user("imagem 1.png imagem 2.png imagem 3.png imagem 4.png");
            message.files = (1..=5)
                .map(|index| Attachment {
                    kind: Some("file".into()),
                    path: Some(format!("/tmp/image-{index}.png")),
                    name: Some(format!("image-{index}.png")),
                    url: None,
                    mime: None,
                })
                .collect();
            let rect = user_row(ui, &palette, &mut md, &message);
            assert!(
                rect.width() <= 768.0 * USER_BUBBLE_MAX + 1.0,
                "bubble width {}",
                rect.width()
            );
            assert!(
                rect.height() < 320.0,
                "attachments squeezed the text into one character per line: {}",
                rect.height()
            );
        });
    }

    #[test]
    fn tool_rows_take_the_reference_line_box() {
        assert_eq!(row_height(639.0), 28.0);
        assert_eq!(row_height(640.0), 28.0);
        assert_eq!(row_height(1600.0), 28.0);
        assert_eq!(row_height(640.0), ROW_HEIGHT);
        assert_eq!(row_height(639.0), ROW_HEIGHT_NARROW);
    }

    #[test]
    fn activity_fold_title_takes_the_label_size() {
        assert_eq!(FOLD_TITLE, 13.5);
        assert_eq!(FOLD_HEIGHT, 28.0);
        assert_eq!(ROW_HEIGHT, 28.0);
        assert_eq!(ROW_HEIGHT_NARROW, 28.0);
        assert_eq!(REASONING_ROW, 26.0);
    }

    #[test]
    fn session_switch_holds_the_previous_timeline() {
        let previous = Timeline {
            session_id: "a".into(),
            messages: vec![Message::Unknown],
            has_more: false,
            error: None,
            busy: false,
        };
        assert!(hold_previous(true, true, 0.0, Some(&previous), "b"));
        assert!(hold_previous(true, true, 0.399, Some(&previous), "b"));
        assert!(!hold_previous(true, true, 0.4, Some(&previous), "b"));
        assert!(!hold_previous(false, true, 0.1, Some(&previous), "b"));
        assert!(!hold_previous(true, false, 0.1, Some(&previous), "b"));
        assert!(!hold_previous(true, true, 0.1, Some(&previous), "a"));
        assert!(!hold_previous(true, true, 0.1, None, "b"));
        let empty = Timeline {
            messages: Vec::new(),
            ..previous.clone()
        };
        assert!(!hold_previous(true, true, 0.1, Some(&empty), "b"));
    }

    #[test]
    fn waited_timeline_reveals_over_one_hundred_ms() {
        assert_eq!(reveal_alpha(0.0, true), 1.0);
        assert_eq!(reveal_alpha(0.0, false), 0.0);
        let mid = reveal_alpha(0.05, false);
        assert!(mid > 0.3 && mid < 1.0, "mid fade {mid}");
        assert_eq!(reveal_alpha(0.1, false), 1.0);
        assert_eq!(reveal_alpha(1.0, false), 1.0);
    }

    #[test]
    fn scroll_pill_hangs_eight_points_above_the_composer() {
        assert_eq!(PILL_HEIGHT, 28.0);
        assert_eq!(PILL_GAP, 8.0);
        assert_eq!(composer_top(900.0, 244.0), 640.0);
        let dock = Dock {
            left: 128.0,
            top: composer_top(900.0, 244.0) - PILL_GAP,
        };
        let origin = pill_origin(dock);
        assert_eq!(origin.x, dock.left);
        assert_eq!(
            origin.y + PILL_HEIGHT,
            composer_top(900.0, 244.0) - PILL_GAP
        );
        assert_eq!(
            composer_top(900.0, 244.0) - (origin.y + PILL_HEIGHT),
            PILL_GAP
        );
    }

    #[test]
    fn dock_column_matches_the_composer_width() {
        egui::__run_test_ui(|ui| {
            let area = Rect::from_min_size(Pos2::new(64.0, 0.0), Vec2::new(1000.0, 800.0));
            let dock = dock_metrics(ui.ctx(), "s", area);
            assert_eq!(dock.left, 184.0);
            assert_eq!(dock.top, composer_top(800.0, 96.0) - PILL_GAP);

            let wide = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1440.0, 900.0));
            let dock = dock_metrics(ui.ctx(), "s", wide);
            assert_eq!(dock.left, (1440.0 - COLUMN_WIDTH) * 0.5);
        });
    }

    #[test]
    fn composer_fade_runs_from_transparent_to_background() {
        assert_eq!(COMPOSER_FADE, 64.0);
        let rect = Rect::from_min_size(Pos2::new(10.0, 200.0), Vec2::new(600.0, COMPOSER_FADE));
        let transparent = Color32::from_rgba_unmultiplied(0, 0, 0, 0);
        let mesh = fade_mesh(rect, transparent, Color32::BLACK).expect("band");
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.vertices[0].color, transparent);
        assert_eq!(mesh.vertices[1].color, transparent);
        assert_eq!(mesh.vertices[2].color, Color32::BLACK);
        assert_eq!(mesh.vertices[3].color, Color32::BLACK);
        assert_eq!(mesh.vertices[0].pos.y, rect.top());
        assert_eq!(mesh.vertices[1].pos.y, rect.top());
        assert_eq!(mesh.vertices[2].pos.y, rect.bottom());
        assert_eq!(mesh.vertices[3].pos.y, rect.bottom());
        assert!(
            fade_mesh(
                Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 0.0)),
                transparent,
                Color32::BLACK
            )
            .is_none()
        );
        assert_eq!(Palette::dark().popover.a(), 255);
        assert_eq!(Palette::light().popover.a(), 255);
    }

    #[test]
    fn status_mark_uses_the_provider_identity() {
        assert_eq!(initial_of("Anthropic").as_deref(), Some("A"));
        assert_eq!(initial_of("  openai").as_deref(), Some("O"));
        assert_eq!(initial_of("4o").as_deref(), Some("4"));
        assert_eq!(initial_of("   "), None);

        let dark = Palette::dark();
        let light = Palette::light();
        assert_eq!(
            provider_color("anthropic", &dark),
            Color32::from_rgb(0xD9, 0x77, 0x57)
        );
        assert_eq!(
            provider_color("Anthropic", &light),
            provider_color("anthropic", &dark)
        );
        assert_ne!(
            provider_color("unknown-provider", &dark),
            provider_color("unknown-provider", &light)
        );
        assert_eq!(mark_ink(Color32::WHITE), Color32::BLACK);
        assert_eq!(mark_ink(Color32::BLACK), Color32::WHITE);
    }

    #[test]
    fn column_keeps_reference_padding() {
        let (left, inner) = column_geometry(1600.0, true);
        assert!((left - 420.0).abs() < 0.01);
        assert!((inner - 760.0).abs() < 0.01);
        assert!((left + inner * 0.5 - 800.0).abs() < 0.01);

        let (left, inner) = column_geometry(900.0, false);
        assert!((left - 70.0).abs() < 0.01);
        assert!((inner - 760.0).abs() < 0.01);

        let (left, inner) = column_geometry(400.0, false);
        assert!((left - 12.0).abs() < 0.01);
        assert!((inner - 376.0).abs() < 0.01);
        assert!((left + inner * 0.5 - 200.0).abs() < 0.01);
    }

    #[test]
    fn tool_row_keeps_the_reference_height() {
        let palette = Palette::light();
        let mut md = Markdown::default();
        egui::__run_test_ui(|ui| {
            ui.set_width(700.0);
            ui.set_max_height(100_000.0);
            let tool = ToolPart {
                id: "t1".into(),
                name: "read".into(),
                executed: true,
                state: ToolState::Running(rchamber_api::models::RunningTool {
                    input: serde_json::json!({ "filePath": "src/main.rs" }),
                    metadata: None,
                }),
                time: Default::default(),
                native: None,
            };
            let mut open = None;
            let rect = tool_row(ui, &palette, &mut md, &tool, &mut open);
            assert!(
                (rect.height() - row_height_of(ui)).abs() < 0.01,
                "tool row {}",
                rect.height()
            );
            assert_eq!(rect.height(), ROW_HEIGHT);
        });
    }

    #[test]
    fn tool_output_renders_markdown_and_code() {
        let palette = Palette::light();
        let mut md = Markdown::default();
        egui::__run_test_ui(|ui| {
            ui.set_width(700.0);
            ui.set_max_height(100_000.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let before = ui.cursor().min.y;
            tool_output(
                ui,
                &palette,
                &mut md,
                "**Result**\n\n```rust\nfn main() {}\n```",
                Id::unique("tool-output"),
            );
            assert!(
                ui.cursor().min.y - before > 40.0,
                "markdown output {}",
                ui.cursor().min.y - before
            );
        });
    }

    #[test]
    fn reasoning_body_follows_the_auto_expansion() {
        let palette = Palette::light();
        let mut md = Markdown::default();
        egui::__run_test_ui(|ui| {
            ui.set_width(700.0);
            ui.set_max_height(100_000.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let before = ui.cursor().min.y;
            reasoning_row(
                ui,
                &palette,
                &mut md,
                Id::unique("streaming"),
                "some thinking text",
                true,
                None,
            );
            assert!(
                ui.cursor().min.y - before > REASONING_ROW,
                "streaming body hidden"
            );
            let before = ui.cursor().min.y;
            reasoning_row(
                ui,
                &palette,
                &mut md,
                Id::unique("settled"),
                "some thinking text",
                false,
                Some(1.8),
            );
            assert!(
                (ui.cursor().min.y - before - REASONING_ROW).abs() < 0.01,
                "settled body shown {}",
                ui.cursor().min.y - before
            );
        });
    }

    #[test]
    fn reasoning_rows_auto_expand_while_streaming() {
        assert!(reasoning_open(false, false, true));
        assert!(!reasoning_open(false, true, false));
        assert!(reasoning_open(true, true, false));
        assert!(!reasoning_open(true, false, true));
    }

    #[test]
    fn follow_rearms_only_within_the_threshold() {
        assert!(follow_next(true, 900.0, 10.0, 10.0, false));
        assert!(follow_next(false, 0.0, 100.0, 300.0, false));
        assert!(follow_next(false, REARM, 100.0, 300.0, false));
        assert!(!follow_next(false, REARM + 1.0, 400.0, 300.0, true));
        assert!(follow_next(false, REARM + 1.0, 300.0, 300.0, true));
    }

    #[test]
    fn turns_group_steps_notices_and_answers() {
        let user_message = Message::User(Box::new(UserMessage {
            id: "u1".into(),
            time: Default::default(),
            text: "hello".into(),
            files: Vec::new(),
            agents: Vec::new(),
            skills: Vec::new(),
            metadata: None,
        }));
        let assistant = Message::Assistant(Box::new(AssistantMessage {
            id: "a1".into(),
            time: Default::default(),
            agent: None,
            model: None,
            content: vec![Part::Text(rchamber_api::models::TextPart {
                text: "done".into(),
                native: None,
            })],
            snapshot: None,
            finish: Some("stop".into()),
            raw_finish: None,
            cost: 0.0,
            tokens: None,
            error: None,
            retry: None,
        }));
        let notice = Message::Synthetic(rchamber_api::models::SyntheticMessage {
            id: "s1".into(),
            time: Default::default(),
            text: "note".into(),
            description: Some("Session note".into()),
            metadata: None,
        });
        let messages = vec![&user_message, &notice, &assistant, &assistant];
        let turns = build_turns(&messages);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].steps.len(), 2);
        assert_eq!(turns[0].notices.len(), 1);
        assert_eq!(final_step(&turns[0]), Some(1));

        let messages = vec![&user_message, &assistant, &notice, &assistant];
        let turns = build_turns(&messages);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].steps.len(), 1);
        assert_eq!(turns[1].notices.len(), 1);
        assert_eq!(turns[2].steps.len(), 1);
    }

    #[test]
    fn final_step_requires_a_stopped_answer_with_text() {
        let step = |finish: Option<&str>, text: &str| AssistantMessage {
            id: "a".into(),
            time: Default::default(),
            agent: None,
            model: None,
            content: vec![Part::Text(rchamber_api::models::TextPart {
                text: text.into(),
                native: None,
            })],
            snapshot: None,
            finish: finish.map(str::to_string),
            raw_finish: None,
            cost: 0.0,
            tokens: None,
            error: None,
            retry: None,
        };
        let stopped = step(Some("stop"), "answer");
        let running = step(None, "answer");
        let empty = step(Some("stop"), "   ");
        fn turn_of(assistant: &AssistantMessage) -> Turn<'_> {
            Turn {
                key: "a",
                user: None,
                steps: vec![assistant],
                notices: Vec::new(),
            }
        }
        assert_eq!(final_step(&turn_of(&stopped)), Some(0));
        assert_eq!(final_step(&turn_of(&running)), None);
        assert_eq!(final_step(&turn_of(&empty)), None);
    }

    #[test]
    fn fold_summary_counts_the_turn() {
        let assistant = AssistantMessage {
            id: "a1".into(),
            time: Default::default(),
            agent: None,
            model: None,
            content: vec![Part::Tool(Box::new(ToolPart {
                id: "t1".into(),
                name: "bash".into(),
                executed: true,
                state: ToolState::Completed(rchamber_api::models::CompletedTool {
                    input: serde_json::json!({ "command": "ls" }),
                    content: Vec::new(),
                    metadata: None,
                }),
                time: Default::default(),
                native: None,
            }))],
            snapshot: None,
            finish: None,
            raw_finish: None,
            cost: 0.0,
            tokens: None,
            error: None,
            retry: None,
        };
        let turn = Turn {
            key: "a1",
            user: None,
            steps: vec![&assistant],
            notices: Vec::new(),
        };
        let summary = summarize(&turn);
        assert_eq!(summary.commands, 1);
        assert!(fold_label(&summary).contains("Ran 1 command"));
    }

    #[test]
    fn session_ids_become_deep_links() {
        assert_eq!(
            linkify_session_ids("see ses_abc123 now"),
            "see [ses_abc123](rchamber://session/ses_abc123) now"
        );
        assert_eq!(linkify_session_ids("no id"), "no id");
    }

    fn tool_renderer(name: &str, output: &str, columns: &[&str]) -> ToolRenderer {
        ToolRenderer {
            matcher: name.into(),
            name: Some("Deploys".into()),
            icon: Some("rocket".into()),
            title: Some("{input.service}".into()),
            subtitle: Some("deploy".into()),
            output: Some(output.into()),
            language: None,
            columns: columns.iter().map(|column| column.to_string()).collect(),
        }
    }

    #[test]
    fn renders_declared_tool_titles_and_tables() {
        let tool: ToolPart = serde_json::from_value(serde_json::json!({
            "id": "t",
            "name": "mcp.deploys.status",
            "state": {
                "status": "completed",
                "input": {"service": "api"},
                "content": [{"type": "text", "text": "[{\"service\":\"api\",\"state\":\"running\"}]"}]
            }
        }))
        .unwrap();
        assert_eq!(render_template("{input.service}", &tool), "api");
        let renderer = tool_renderer("mcp.deploys.*", "table", &["service", "state"]);
        let table = render_tool_output(Some(&renderer), &tool.state.output_text()).unwrap();
        assert!(table.contains("| api | running |"));
    }
}
