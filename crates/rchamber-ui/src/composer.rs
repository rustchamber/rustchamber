use std::sync::Arc;

use eframe::egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use eframe::egui::widgets::text_edit::TextEditState;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Id, Key, Layout, Margin, Modifiers, Pos2,
    Rect, Response, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2,
};
use rchamber_api::PromptFile;
use rchamber_api::models::{Agent, Model, ModelRef, PermissionRequest};
use rchamber_app::Action;
use rchamber_app::attachments::{self, AttachmentItem};

use crate::app::{RustChamber, menu_nav, menu_nav_focus, menu_nav_reset, menu_popup};
use crate::icons::{self, Icon};
use crate::motion;
use crate::theme::{self, Palette};
use crate::widgets;

const COLUMN_WIDTH: f32 = 768.0;
const COLUMN_PAD_MIN: f32 = 12.0;
const COLUMN_PAD_MAX: f32 = 16.0;
const BOX_RADIUS: u8 = 12;
pub(crate) const FORM_PAD_BOTTOM: f32 = 16.0;

const EDITOR_MIN_HEIGHT: f32 = 64.0;
const EDITOR_PAD_X: f32 = 16.0;
const EDITOR_PAD_TOP: f32 = 16.0;
const EDITOR_PAD_BOTTOM: f32 = 10.0;
const EDITOR_LINE_HEIGHT: f32 = 20.0;
const EDITOR_MAX_LINES: f32 = 8.0;
const EDITOR_MAX_HEIGHT: f32 =
    EDITOR_MAX_LINES * EDITOR_LINE_HEIGHT + EDITOR_PAD_TOP + EDITOR_PAD_BOTTOM;
const EDITOR_FONT: f32 = 13.5;

const FOOTER_PAD_X: f32 = 10.0;
const FOOTER_PAD_Y: f32 = 6.0;
const FOOTER_BUTTON: f32 = 24.0;
const FOOTER_GLYPH: f32 = 16.0;
const FOOTER_SEND_GLYPH: f32 = 16.0;
const FOOTER_STOP_GLYPH: f32 = 20.0;
const FOOTER_GAP: f32 = 6.0;
const FOOTER_GROUP_GAP: f32 = 12.0;
const FOOTER_HEIGHT: f32 = FOOTER_BUTTON + FOOTER_PAD_Y * 2.0;
const LABEL_MAX_WIDTH: f32 = 220.0;
const FLIP_LABEL_CAP: f32 = 96.0;
const THINK_LABEL_CAP: f32 = 96.0;
const AGENT_CHROME: f32 = 24.0;
const MODEL_CHROME: f32 = 36.0;
const EFFORT_CHROME: f32 = 24.0;
const MODEL_LABEL_MIN: f32 = 48.0;

const MODEL_ROW: f32 = 31.0;
const MODEL_ROW_PAD: f32 = 6.0;
const MODEL_ROW_STAR: f32 = 18.0;
const MODEL_ROW_CHECK: f32 = 16.0;
const MODEL_ROW_SLOT_GAP: f32 = 4.0;
const MODEL_ROW_TRAILING: f32 = MODEL_ROW_PAD
    + MODEL_ROW_STAR
    + MODEL_ROW_SLOT_GAP
    + MODEL_ROW_CHECK
    + MODEL_ROW_SLOT_GAP
    + 6.0;

const QUEUE_ROW: f32 = 32.0;
const QUEUE_LIST_MAX: f32 = 160.0;
const QUEUE_ROW_INSET_START: f32 = 12.0;
const QUEUE_ROW_INSET_END: f32 = 6.0;

const ATTACHMENT_HEIGHT: f32 = 28.0;
const ATTACHMENT_NAME_MAX: f32 = 200.0;
const ATTACHMENT_ICON: f32 = 16.0;
const ATTACHMENT_REMOVE: f32 = 20.0;
const ATTACHMENT_GAP: f32 = 8.0;
const ATTACHMENT_ROW_TOP: f32 = 4.0;

const DOCK_GAP: f32 = 12.0;

const POPUP_GAP: f32 = 8.0;
const POPUP_EDGE: f32 = 8.0;
const POPUP_INSET: f32 = 8.0;
const POPUP_MIN_HEIGHT: f32 = 180.0;
const POPUP_MAX_HEIGHT: f32 = 420.0;
const POPUP_TEXT_INSET: f32 = 12.0;
const POPUP_ROW: f32 = 30.0;
const POPUP_LIMIT: usize = 20;

const FLIP_SECONDS: f32 = 0.180;
const QUEUE_EXPAND_SECONDS: f32 = 0.120;
const PICKER_ENTER_SECONDS: f32 = 0.090;
const GLASS_ENTER_SECONDS: f32 = 0.140;

const LARGE_PASTE_CHARS: usize = 2000;
const LARGE_PASTE_LINES: usize = 25;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CaretAnchor {
    trigger: Rect,
}

#[derive(Clone, Copy, Debug)]
struct PopupPlacement {
    left: f32,
    bottom: f32,
    width: f32,
    max_height: f32,
}

#[derive(Clone, Debug)]
struct PickerItem {
    value: String,
    label: String,
    detail: String,
    icon: Icon,
    section: &'static str,
}

#[derive(Clone, Debug)]
struct Picker {
    kind: char,
    start_byte: usize,
    end_byte: usize,
    query: String,
    items: Vec<PickerItem>,
    width: f32,
}

impl Picker {
    fn index_id(&self) -> Id {
        Id::unique(("composer-picker-index", self.kind, self.start_byte))
    }
}

pub fn floating(
    ui: &mut Ui,
    app: &mut RustChamber,
    session_id: Option<String>,
    home: bool,
    bounds: Rect,
) {
    if bounds.width() < 40.0 || bounds.height() < 40.0 {
        return;
    }
    let ctx = ui.ctx().clone();
    let key = session_id.clone().unwrap_or_else(|| "home".into());
    let width = composer_width(bounds.width());
    let metrics_id = Id::unique(("composer-metrics", key.as_str(), home));
    let stored = ctx.memory(|memory| memory.data.get_temp::<(f32, f32)>(metrics_id));
    let editor = predicted_editor_height(ui, app, width, home);
    let queue = queue_strip_height(app, &ctx, &key, home);
    let attachment_row = app
        .model
        .attachment_items_for(session_id.as_deref().unwrap_or("home"))
        .is_empty();
    let dock = if app
        .model
        .active
        .as_ref()
        .is_some_and(|chat| !chat.pending.is_empty())
    {
        ctx.memory(|memory| {
            memory
                .data
                .get_temp::<f32>(Id::unique(("composer-dock", key.as_str())))
        })
        .unwrap_or(88.0)
            + DOCK_GAP
    } else {
        0.0
    };
    let predicted = match stored {
        Some((total, _)) => total,
        None => {
            let attachment_row = if attachment_row {
                0.0
            } else {
                ATTACHMENT_HEIGHT + ATTACHMENT_ROW_TOP
            };
            (queue + attachment_row + editor + FOOTER_HEIGHT + dock).max(96.0)
        }
    };
    let mut position = Pos2::new(
        bounds.center().x - width / 2.0,
        (bounds.bottom() - FORM_PAD_BOTTOM - predicted).max(bounds.top() + 8.0),
    );

    let flip_id = Id::unique("composer-flip");
    let last_id = Id::unique("composer-placement");
    let now = ctx.input(|input| input.time);
    let previous = ctx.memory(|memory| memory.data.get_temp::<(String, Pos2)>(last_id));
    let mut flip = ctx
        .memory(|memory| memory.data.get_temp::<(f64, Vec2)>(flip_id))
        .unwrap_or((0.0, Vec2::ZERO));
    if let Some((previous_key, previous_pos)) = previous {
        let enters = previous_key != key && previous_key == "home" && !home;
        if enters && !motion::reduced_motion(&ctx) {
            flip = (now, previous_pos - position);
        }
    }
    let progress = if flip.0 > 0.0 {
        (((now - flip.0) as f32) / FLIP_SECONDS).clamp(0.0, 1.0)
    } else {
        1.0
    };
    if progress < 1.0 {
        let eased = motion::cubic_bezier(0.22, 1.0, 0.36, 1.0, progress);
        position += flip.1 * (1.0 - eased);
        ctx.request_repaint();
    }

    ctx.memory_mut(|memory| {
        memory
            .data
            .insert_temp(Id::unique(("composer-bounds", key.as_str(), home)), bounds);
    });
    let area = egui::Area::new(Id::unique(("composer-area", key.as_str(), home)))
        .order(egui::Order::Middle)
        .fixed_pos(position)
        .constrain(false)
        .show(&ctx, |ui| {
            ui.set_width(width);
            show(ui, app, session_id.clone(), home)
        });
    ctx.memory_mut(|memory| {
        memory.data.insert_temp(flip_id, flip);
        memory.data.insert_temp(
            last_id,
            (
                key.clone(),
                Pos2::new(
                    bounds.center().x - width / 2.0,
                    (bounds.bottom() - FORM_PAD_BOTTOM - predicted).max(bounds.top() + 8.0),
                ),
            ),
        );
    });
    let editor_height = area.inner;
    let total = area.response.rect.height();
    let changed = stored
        .map(|(stored_total, stored_editor)| {
            (stored_total - total).abs() > 0.5 || (stored_editor - editor_height).abs() > 0.5
        })
        .unwrap_or(true);
    ctx.memory_mut(|memory| {
        memory.data.insert_temp(metrics_id, (total, editor_height));
    });
    if changed || progress < 1.0 {
        ctx.request_repaint();
    }
}

pub fn reserved_height(ctx: &egui::Context, key: &str, home: bool) -> f32 {
    let id = Id::unique(("composer-metrics", key, home));
    ctx.memory(|memory| memory.data.get_temp::<(f32, f32)>(id))
        .map(|(total, _)| total)
        .unwrap_or(96.0)
}

fn composer_width(bounds_width: f32) -> f32 {
    let column = bounds_width.min(COLUMN_WIDTH);
    let pad = if bounds_width >= 1024.0 {
        (bounds_width * 0.03).clamp(16.0, 24.0)
    } else {
        (bounds_width * 0.025).clamp(COLUMN_PAD_MIN, COLUMN_PAD_MAX)
    };
    (column - pad * 2.0).max(240.0)
}

fn queue_strip_height(app: &RustChamber, ctx: &egui::Context, key: &str, home: bool) -> f32 {
    if home {
        return 0.0;
    }
    let count = app
        .model
        .active
        .as_ref()
        .map(|chat| chat.inbox.len())
        .unwrap_or(0);
    if count == 0 {
        return 0.0;
    }
    if count == 1 {
        return QUEUE_ROW;
    }
    let expanded = ctx.memory(|memory| {
        memory
            .data
            .get_temp::<bool>(Id::unique(("queue-expanded", key)))
            .unwrap_or(true)
    });
    if !expanded {
        return QUEUE_ROW;
    }
    let rows = ((count - 1) as f32 * QUEUE_ROW).min(QUEUE_LIST_MAX);
    QUEUE_ROW + rows
}

fn predicted_editor_height(ui: &Ui, app: &RustChamber, width: f32, home: bool) -> f32 {
    let draft = if home {
        app.ui.home_draft.as_str()
    } else {
        app.model
            .active
            .as_ref()
            .map(|chat| chat.draft.as_str())
            .unwrap_or("")
    };
    let wrap = (width - EDITOR_PAD_X * 2.0 - 2.0).max(80.0);
    let galley = ui.fonts_mut(|fonts| {
        fonts.layout_job(editor_job(draft, &app.palette, wrap, EDITOR_LINE_HEIGHT))
    });
    galley
        .size()
        .y
        .max(EDITOR_LINE_HEIGHT)
        .clamp(EDITOR_MIN_HEIGHT, EDITOR_MAX_HEIGHT)
}

fn editor_job(text: &str, p: &Palette, wrap_width: f32, line_height: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    job.keep_trailing_whitespace = true;
    let plain = TextFormat {
        font_id: theme::font(EDITOR_FONT),
        color: p.fg,
        line_height: Some(line_height),
        ..Default::default()
    };
    let mut cursor = 0;
    for span in token_spans(text) {
        if span.start_byte > cursor {
            job.append(&text[cursor..span.start_byte], 0.0, plain.clone());
        }
        job.append(
            &text[span.start_byte..span.end_byte],
            0.0,
            TextFormat {
                font_id: theme::font(EDITOR_FONT),
                color: p.blue,
                line_height: Some(line_height),
                ..Default::default()
            },
        );
        cursor = span.end_byte;
    }
    if cursor < text.len() {
        job.append(&text[cursor..], 0.0, plain);
    }
    job
}

#[derive(Clone, Copy)]
struct TokenSpan {
    start_byte: usize,
    end_byte: usize,
    start_char: usize,
    end_char: usize,
}

fn token_spans(text: &str) -> Vec<TokenSpan> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let (byte, trigger) = chars[index];
        if !matches!(trigger, '@' | '$' | '/' | '#') {
            index += 1;
            continue;
        }
        let boundary = index == 0 || chars[index - 1].1.is_whitespace();
        if !boundary {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < chars.len() && !chars[end].1.is_whitespace() {
            end += 1;
        }
        while end > index + 1 {
            let last = chars[end - 1].1;
            if matches!(
                last,
                ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '\'' | '"'
            ) {
                end -= 1;
            } else {
                break;
            }
        }
        if end > index + 1 {
            let end_byte = chars.get(end).map(|(byte, _)| *byte).unwrap_or(text.len());
            spans.push(TokenSpan {
                start_byte: byte,
                end_byte,
                start_char: index,
                end_char: end,
            });
        }
        index = end.max(index + 1);
    }
    let ends_with_whitespace = text
        .chars()
        .last()
        .map(|ch| ch.is_whitespace())
        .unwrap_or(true);
    if !ends_with_whitespace
        && let Some(last) = spans.last()
        && last.end_byte == text.len()
    {
        spans.pop();
    }
    spans
}

fn is_composing(ui: &Ui) -> bool {
    ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if !text.is_empty()
            )
        })
    })
}

fn prepare_atomic_deletion(ui: &Ui, id: Id, text: &str) {
    if !ui.memory(|memory| memory.has_focus(id)) {
        return;
    }
    if is_composing(ui) {
        return;
    }
    let (backspace, delete, blocked) = ui.input(|input| {
        (
            input.key_pressed(Key::Backspace),
            input.key_pressed(Key::Delete),
            input.modifiers.any(),
        )
    });
    if (!backspace && !delete) || blocked {
        return;
    }
    let Some(mut state) = TextEditState::load(ui.ctx(), id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    if !range.is_empty() {
        return;
    }
    let cursor = range.primary.index.0;
    let hit = token_spans(text).into_iter().find(|span| {
        let inside = cursor > span.start_char && cursor < span.end_char;
        if backspace {
            cursor == span.end_char || inside
        } else {
            cursor == span.start_char || inside
        }
    });
    if let Some(span) = hit {
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(span.start_char),
            CCursor::new(span.end_char),
        )));
        state.store(ui.ctx(), id);
    }
}

fn paint_chips(
    ui: &mut Ui,
    output: &egui::widgets::text_edit::TextEditOutput,
    text: &str,
    text_id: Id,
    clip: Rect,
) {
    let spans = token_spans(text);
    if spans.is_empty() {
        return;
    }
    let mut chips: Vec<(TokenSpan, Rect)> = Vec::new();
    let mut row_start = 0;
    for placed in &output.galley.rows {
        let count = placed.row.glyphs.len();
        let row_end = row_start + count;
        for span in &spans {
            let start = span.start_char.max(row_start);
            let end = span.end_char.min(row_end);
            if start >= end {
                continue;
            }
            let first = &placed.row.glyphs[start - row_start];
            let last = &placed.row.glyphs[end - 1 - row_start];
            let first_rect = first.logical_rect();
            let last_rect = last.logical_rect();
            let height = (EDITOR_FONT * 1.3).min(placed.rect().height());
            let center_y = output.galley_pos.y + placed.rect().center().y;
            let rect = Rect::from_min_max(
                Pos2::new(
                    output.galley_pos.x + placed.pos.x + first_rect.left() - 5.0,
                    center_y - height / 2.0,
                ),
                Pos2::new(
                    output.galley_pos.x + placed.pos.x + last_rect.right() + 5.0,
                    center_y + height / 2.0,
                ),
            );
            if rect.intersects(clip) {
                chips.push((*span, rect));
            }
        }
        row_start = row_end + usize::from(placed.ends_with_newline);
    }
    if chips.is_empty() {
        return;
    }
    let mut clicked = None;
    for (span, rect) in &chips {
        let response = ui.interact(
            *rect,
            Id::unique(("composer-chip", text_id, span.start_byte)),
            Sense::click(),
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            clicked = Some(*span);
        }
    }
    if let Some(span) = clicked {
        if let Some(mut state) = TextEditState::load(ui.ctx(), text_id) {
            state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(span.start_char),
                CCursor::new(span.end_char),
            )));
            state.store(ui.ctx(), text_id);
        }
        ui.memory_mut(|memory| memory.request_focus(text_id));
    }
}

fn should_submit_enter(
    modifiers: Modifiers,
    expanded: bool,
    configured: bool,
    enter_to_send: bool,
) -> bool {
    let modifier_enter = modifiers.command || modifiers.ctrl || modifiers.mac_cmd;
    if expanded {
        return modifier_enter;
    }
    if !configured {
        return !modifiers.shift && (!modifiers.alt || modifier_enter);
    }
    let sends_with_enter = enter_to_send && !modifiers.shift && !modifiers.alt;
    modifier_enter || sends_with_enter
}

fn insert_text_at_caret(ctx: &egui::Context, id: Id, buffer: &mut String, text: &str) {
    let count = buffer.chars().count();
    let Some(mut state) = TextEditState::load(ctx, id) else {
        buffer.push_str(text);
        return;
    };
    let range = state
        .cursor
        .char_range()
        .unwrap_or_else(|| CCursorRange::one(CCursor::new(count)));
    let start = range.primary.index.0.min(count);
    let end = range.secondary.index.0.min(count);
    let (from, to) = (start.min(end), start.max(end));
    let chars: Vec<char> = buffer.chars().collect();
    let head: String = chars[..from].iter().collect();
    let tail: String = chars[to..].iter().collect();
    let caret = from + text.chars().count();
    *buffer = format!("{head}{text}{tail}");
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(caret))));
    state.store(ctx, id);
}

fn selection_text(ctx: &egui::Context, id: Id, buffer: &str) -> Option<String> {
    let state = TextEditState::load(ctx, id)?;
    let range = state.cursor.char_range()?;
    if range.is_empty() {
        return None;
    }
    let chars: Vec<char> = buffer.chars().collect();
    let start = range.primary.index.0.min(chars.len());
    let end = range.secondary.index.0.min(chars.len());
    let (from, to) = (start.min(end), start.max(end));
    Some(chars[from..to].iter().collect())
}

fn looks_like_url(text: &str) -> bool {
    let trimmed = text.trim();
    (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && !trimmed
            .split_whitespace()
            .nth(1)
            .is_some_and(|rest| !rest.is_empty())
}

fn is_large_plain_text_paste(text: &str) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    if text.chars().count() >= LARGE_PASTE_CHARS {
        return true;
    }
    text.chars().filter(|ch| *ch == '\n').count() + 1 >= LARGE_PASTE_LINES
}

pub fn show(ui: &mut Ui, app: &mut RustChamber, session_id: Option<String>, home: bool) -> f32 {
    let p = app.palette.clone();
    let key = session_id.clone().unwrap_or_else(|| "home".into());
    let mut draft = if home {
        std::mem::take(&mut app.ui.home_draft)
    } else {
        app.model
            .active
            .as_mut()
            .map(|chat| std::mem::take(&mut chat.draft))
            .unwrap_or_default()
    };
    if let Ok(value) = std::env::var("RCHAMBER_DEBUG_DRAFT") {
        draft = match value.as_str() {
            "at" => "@".into(),
            "slash" => "/".into(),
            "dollar" => "$".into(),
            "hash" => "#".into(),
            _ => draft,
        };
    }
    let pending = app
        .model
        .active
        .as_ref()
        .map(|chat| chat.pending.clone())
        .unwrap_or_default();
    let busy = app
        .model
        .active
        .as_ref()
        .map(|chat| chat.busy)
        .unwrap_or(false);
    let inbox = app
        .model
        .active
        .as_ref()
        .map(|chat| chat.inbox.clone())
        .unwrap_or_default();
    let mut actions: Vec<Action> = Vec::new();
    let mut send = false;
    let mut send_now = false;
    let text_id = Id::unique(("composer-text", &session_id));
    let attachments = app.model.attachment_items_for(&key).to_vec();
    let has_attachments = !attachments.is_empty();
    let mut remove_attachment: Option<usize> = None;

    let focused = ui.memory(|memory| memory.has_focus(text_id));
    let composing = is_composing(ui);

    let box_rect_id = Id::unique(("composer-box-rect", &key));
    let previous_box = ui.memory(|memory| memory.data.get_temp::<Rect>(box_rect_id));
    if let Some(rect) = previous_box {
        let catch = ui.interact(
            rect,
            Id::unique(("composer-box-focus", &key)),
            Sense::click(),
        );
        if catch.clicked() {
            set_cursor_to_end(ui.ctx(), text_id, &draft);
        }
    }

    let paste_offer_id = Id::unique(("composer-paste-offer", &key));
    let mut paste_offer = ui.memory(|memory| memory.data.get_temp::<(u64, String)>(paste_offer_id));

    let mut picker = composer_picker(ui.ctx(), app, &draft, text_id);
    if focused && !composing && picker.is_none() && paste_offer.is_none() {
        let shift = ui.input(|input| input.modifiers.shift);
        let tab = ui.input_mut(|input| {
            input.consume_key(Modifiers::NONE, Key::Tab)
                || input.consume_key(Modifiers::SHIFT, Key::Tab)
        });
        if tab {
            cycle_agent(app, home, shift, &mut actions);
            ui.memory_mut(|memory| memory.request_focus(text_id));
        }
    }
    if let Some(offer) = paste_offer.clone() {
        if focused && !composing {
            let escape = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
            let enter = ui.input_mut(|input| {
                input.consume_key(Modifiers::NONE, Key::Enter)
                    || input.consume_key(Modifiers::COMMAND, Key::Enter)
            });
            if escape || enter {
                if enter {
                    attach_pasted_text(app, &key, &offer.1, ui.ctx(), text_id, &mut draft);
                } else {
                    paste_offer = None;
                }
                ui.memory_mut(|memory| memory.data.remove_temp::<(u64, String)>(paste_offer_id));
            }
        }
    } else if focused
        && !composing
        && let Some(selected) = intercept_paste(ui, text_id, &draft)
    {
        match selected {
            PasteIntercept::Url { selection, url } => {
                let link = format!("[{selection}]({url})");
                insert_text_at_caret(ui.ctx(), text_id, &mut draft, &link);
            }
            PasteIntercept::Large(text) => {
                let offer = (ui.input(|input| input.time) as u64, text);
                ui.memory_mut(|memory| memory.data.insert_temp(paste_offer_id, offer.clone()));
                paste_offer = Some(offer);
            }
            PasteIntercept::Image(image) => {
                attach_pasted_image(app, &key, &image, ui.ctx());
            }
        }
    }

    if let Some(request) = pending.first()
        && !composing
        && picker.is_none()
    {
        let (always, once, reject) = ui.input_mut(|input| {
            (
                input.consume_key(Modifiers::ALT | Modifiers::SHIFT, Key::Enter),
                input.consume_key(Modifiers::ALT, Key::Enter),
                input.consume_key(Modifiers::ALT, Key::Backspace),
            )
        });
        let decision = if always {
            Some("always")
        } else if once {
            Some("once")
        } else if reject {
            Some("reject")
        } else {
            None
        };
        if let Some(decision) = decision {
            actions.push(Action::ReplyPermission {
                session_id: request.session_id.clone(),
                request_id: request.id.clone(),
                decision: decision.into(),
            });
        }
    }

    if picker.is_none() && paste_offer.is_none() && focused && !composing {
        let modifiers = ui.input(|input| input.modifiers);
        if ui.input(|input| input.key_pressed(Key::Enter)) {
            let submit = should_submit_enter(modifiers, false, false, false);
            ui.input_mut(|input| input.consume_key(modifiers, Key::Enter));
            if submit {
                send = true;
                send_now = modifiers.command || modifiers.ctrl || modifiers.mac_cmd;
            } else {
                insert_text_at_caret(ui.ctx(), text_id, &mut draft, "\n");
            }
        }
    }

    if let Some(picker) = picker.as_mut() {
        picker_keyboard(ui, picker, focused && !composing, &mut draft, text_id);
    }
    if picker
        .as_ref()
        .is_some_and(|picker| picker.items.is_empty())
    {
        picker = None;
    }

    let dock = if let Some(request) = pending.first() {
        let height = permission_dock(ui, &p, &pending, request, &mut actions);
        ui.add_space(DOCK_GAP);
        height
    } else {
        0.0
    };
    ui.memory_mut(|memory| {
        memory
            .data
            .insert_temp(Id::unique(("composer-dock", &key)), dock)
    });

    let mut caret_anchor: Option<CaretAnchor> = None;
    let mut editor_height = 0.0;
    let frame = egui::Frame::new()
        .fill(p.card)
        .corner_radius(BOX_RADIUS)
        .inner_margin(Margin::ZERO)
        .shadow(box_shadow(&p));

    let box_response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        if !home && !inbox.is_empty() {
            queued_messages_strip(
                ui,
                app,
                &p,
                &inbox,
                session_id.as_deref(),
                &mut draft,
                text_id,
                &mut actions,
            );
        }

        if !attachments.is_empty() {
            egui::Frame::new()
                .inner_margin(Margin {
                    left: 12,
                    right: 12,
                    top: ATTACHMENT_ROW_TOP as i8,
                    bottom: 0,
                })
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(ATTACHMENT_GAP, ATTACHMENT_GAP);
                        for (index, item) in attachments.iter().enumerate() {
                            file_chip(ui, &p, item, index, &mut remove_attachment);
                        }
                    });
                });
        }

        prepare_atomic_deletion(ui, text_id, &draft);
        let palette = p.clone();
        let mut layouter = move |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
            ui.fonts_mut(|fonts| {
                fonts.layout_job(editor_job(
                    buffer.as_str(),
                    &palette,
                    wrap_width,
                    EDITOR_LINE_HEIGHT,
                ))
            })
        };
        let hint = placeholder_text(ui, home);
        if ui
            .ctx()
            .data_mut(|data| data.remove_temp::<bool>(Id::unique("focus-composer-once")))
            .unwrap_or(false)
        {
            ui.ctx().memory_mut(|memory| memory.request_focus(text_id));
        }
        if ui.input(|input| input.modifiers.shift && input.key_pressed(Key::Enter))
            && let Some(state) = TextEditState::load(ui.ctx(), text_id)
        {
            let caret = state
                .cursor
                .char_range()
                .map(|range| range.primary.index.0)
                .unwrap_or_else(|| draft.chars().count());
            let byte = char_to_byte(&draft, caret);
            let line_start = draft[..byte]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0);
            let line = &draft[line_start..byte];
            let indent_len = line.len() - line.trim_start().len();
            let trimmed = line.trim_start();
            if let Some((number, _)) = trimmed.split_once(". ")
                && !number.is_empty()
                && number.chars().all(|ch| ch.is_ascii_digit())
                && let Ok(number) = number.parse::<u32>()
            {
                ui.input_mut(|input| {
                    input.consume_key(Modifiers::SHIFT, Key::Enter);
                });
                let indent = &line[..indent_len];
                let insert = format!("\n{indent}{}. ", number + 1);
                draft.insert_str(byte, &insert);
                if let Some(mut state) = TextEditState::load(ui.ctx(), text_id) {
                    state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(CCursor::new(
                            caret + insert.chars().count(),
                        ))));
                    state.store(ui.ctx(), text_id);
                }
                ui.ctx().memory_mut(|memory| memory.request_focus(text_id));
            }
        }
        let scroll = ScrollArea::vertical()
            .id_salt(("composer-editor", &session_id))
            .max_height(EDITOR_MAX_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let editor_width = ui.available_width().max(40.0);
                ui.set_width(editor_width);
                let output = TextEdit::multiline(&mut draft)
                    .id(text_id)
                    .event_filter(egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    })
                    .frame(egui::Frame::NONE.inner_margin(Margin {
                        left: EDITOR_PAD_X as i8,
                        right: EDITOR_PAD_X as i8,
                        top: EDITOR_PAD_TOP as i8,
                        bottom: EDITOR_PAD_BOTTOM as i8,
                    }))
                    .min_size(Vec2::new(0.0, EDITOR_MIN_HEIGHT))
                    .desired_rows(1)
                    .desired_width(editor_width)
                    .hint_text(
                        RichText::new(hint)
                            .font(theme::font(EDITOR_FONT))
                            .color(p.fg_muted),
                    )
                    .text_color(p.fg)
                    .font(theme::font(EDITOR_FONT))
                    .layouter(&mut layouter)
                    .show(ui);
                caret_follow(ui, &output, focused, text_id, &draft);
                output
            });
        editor_height = scroll.inner_rect.height();
        let output = scroll.inner;
        let click_below_text = ui.input(|input| {
            input
                .pointer
                .primary_clicked()
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        if click_lands_below_text(
            output.response.rect,
            output.galley_pos.y + output.galley.size().y,
            click_below_text,
        ) {
            set_cursor_to_end(ui.ctx(), text_id, &draft);
        }
        paint_chips(ui, &output, &draft, text_id, scroll.inner_rect);
        caret_anchor = caret_anchor_of(&output, &draft, text_id);
        if let Some(anchor) = caret_anchor {
            ui.memory_mut(|memory| {
                memory
                    .data
                    .insert_temp(Id::unique(("composer-caret", &key)), anchor)
            });
        } else {
            ui.memory_mut(|memory| {
                memory.data.insert_temp(
                    Id::unique(("composer-caret", &key)),
                    Option::<CaretAnchor>::None,
                )
            });
        }

        composer_footer(
            ui,
            app,
            &p,
            text_id,
            home,
            busy,
            &draft,
            has_attachments,
            &key,
            &mut actions,
            &mut send,
        );
    });
    let composer_rect = box_response.response.rect;
    ui.memory_mut(|memory| memory.data.insert_temp(box_rect_id, composer_rect));
    let painter = ui.painter();
    painter.rect_stroke(
        composer_rect,
        CornerRadius::same(BOX_RADIUS),
        Stroke::new(theme::HAIRLINE, p.elevation_ring),
        StrokeKind::Inside,
    );
    let sheen = if p.dark {
        p.alpha(Color32::WHITE, 10)
    } else {
        p.alpha(Color32::WHITE, 179)
    };
    let top = Rect::from_min_max(
        Pos2::new(composer_rect.left() + 1.0, composer_rect.top() + 1.0),
        Pos2::new(composer_rect.right() - 1.0, composer_rect.top() + 2.0),
    );
    painter.rect_filled(top, CornerRadius::ZERO, sheen);

    let dragging_files = ui.input(|input| !input.raw.hovered_files.is_empty());
    if dragging_files {
        ui.painter().rect_stroke(
            composer_rect.expand(2.0),
            CornerRadius::same(BOX_RADIUS),
            Stroke::new(2.0, p.primary),
            StrokeKind::Middle,
        );
        ui.painter().rect_filled(
            composer_rect,
            CornerRadius::same(BOX_RADIUS),
            p.over(p.card, p.alpha(p.bg, 230)),
        );
        let label = if ui.input(|input| {
            input
                .raw
                .hovered_files
                .iter()
                .any(|file| file.path.is_some())
        }) {
            "Drop files here to attach"
        } else {
            "Drop to insert as mention"
        };
        ui.painter().text(
            composer_rect.center(),
            Align2::CENTER_CENTER,
            label,
            theme::font(12.5),
            p.fg_muted,
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
    }

    let dropped: Vec<std::path::PathBuf> = ui.input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect()
    });
    if !dropped.is_empty() {
        let (items, errors) = attachments::prepare_paths(&dropped, app.model.workspace.as_deref());
        for item in items {
            app.model.add_attachment(&key, item);
        }
        for error in errors {
            app.model.toast(rchamber_app::ToastKind::Warning, error);
        }
    }

    if let Some(index) = remove_attachment {
        app.model.remove_attachment(&key, index);
    }

    if send && (!draft.trim().is_empty() || app.model.has_attachments(&key)) {
        let raw = draft.clone();
        let mut handled = false;
        if let Some((id, name, args)) = extension_command(app, &raw) {
            app.backend
                .send(Action::ExtensionCommand { id, name, args });
            draft.clear();
            if home {
                app.ui.home_draft.clear();
            } else if let Some(chat) = &mut app.model.active {
                chat.draft.clear();
            }
            handled = true;
        }
        if !handled {
            let (text, guide) = expand_extension_command(&raw);
            let mut files = app.model.attachments_for(&key);
            if let Some(guide) = guide {
                files.push(guide);
            }
            let delivery = if busy && !send_now { "queue" } else { "steer" };
            if home {
                let model = current_model(app);
                let agent = current_agent(app);
                let directory = app.home_target_directory();
                if let Some(target) = app.ui.home_project.clone()
                    && target != crate::app::CHATS_TARGET
                {
                    app.model.workspace_store.mark_active_project(&target);
                }
                if let Some(target) = app.ui.home_project.clone() {
                    app.model.workspace_store.new_session_drafts.remove(&target);
                }
                app.model.workspace_store.save();
                actions.push(Action::CreateSession {
                    directory,
                    prompt: Some(text),
                    files,
                    model,
                    agent,
                    allow_all: app.ui.home_allow_all,
                });
                draft.clear();
            } else if let Some(id) = &session_id {
                if let Some(session) = app.model.sessions.iter().find(|session| &session.id == id)
                    && !app.model.is_work(session)
                {
                    app.model.set_work_local(id, true);
                    actions.push(Action::SetSessionWork {
                        id: id.clone(),
                        work: true,
                    });
                }
                actions.push(Action::Send {
                    session_id: id.clone(),
                    text,
                    files,
                    delivery: delivery.into(),
                });
                ui.memory_mut(|memory| {
                    memory
                        .data
                        .insert_temp(Id::unique(("chat-pin", id.as_str())), true)
                });
                draft.clear();
            }
            app.model.clear_attachments(&key);
        }
    }

    if home {
        app.ui.home_draft = draft.clone();
    } else if let Some(chat) = &mut app.model.active {
        chat.draft = draft.clone();
    }

    if let Some(picker) = picker.as_ref() {
        let anchor = ui
            .memory(|memory| {
                memory
                    .data
                    .get_temp::<CaretAnchor>(Id::unique(("composer-caret", &key)))
            })
            .or(caret_anchor);
        let area = ui
            .memory(|memory| {
                memory
                    .data
                    .get_temp::<Rect>(Id::unique(("composer-bounds", &key)))
            })
            .unwrap_or_else(|| ui.ctx().content_rect());
        let mut accepted: Option<String> = None;
        if let Some(anchor) = anchor {
            accepted =
                composer_autocomplete_popup(ui.ctx(), &p, picker, anchor, composer_rect, area);
        }
        if let Some(value) = accepted {
            if let Some(caret) = apply_picker_choice(&mut draft, picker, &value)
                && let Some(mut state) = TextEditState::load(ui.ctx(), text_id)
            {
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::one(CCursor::new(caret))));
                state.store(ui.ctx(), text_id);
            }
            ui.ctx().memory_mut(|memory| memory.request_focus(text_id));
            ui.memory_mut(|memory| memory.data.remove_temp::<usize>(picker.index_id()));
        }
        if home {
            app.ui.home_draft = draft.clone();
        } else if let Some(chat) = &mut app.model.active {
            chat.draft = draft.clone();
        }
    } else {
        ui.memory_mut(|memory| {
            memory
                .data
                .insert_temp(Id::unique("composer-picker-open"), false)
        });
    }

    if let Some(offer) = paste_offer {
        let mut resolved: Option<bool> = None;
        let area = egui::Area::new(Id::unique(("composer-paste-choice", &key)))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(
                composer_rect.left() + 12.0,
                composer_rect.top() - 8.0,
            ))
            .pivot(egui::Align2::LEFT_BOTTOM)
            .show(ui.ctx(), |ui| {
                resolved = large_text_paste_choice(ui, &p);
            });
        ui.ctx().move_to_top(area.response.layer_id);
        match resolved {
            Some(true) => {
                attach_pasted_text(app, &key, &offer.1, ui.ctx(), text_id, &mut draft);
                ui.memory_mut(|memory| memory.data.remove_temp::<(u64, String)>(paste_offer_id));
                ui.memory_mut(|memory| memory.request_focus(text_id));
            }
            Some(false) => {
                insert_text_at_caret(ui.ctx(), text_id, &mut draft, &offer.1);
                ui.memory_mut(|memory| memory.data.remove_temp::<(u64, String)>(paste_offer_id));
                ui.memory_mut(|memory| memory.request_focus(text_id));
            }
            None => {}
        }
    }

    if home {
        app.ui.home_draft = draft.clone();
    } else if let Some(chat) = &mut app.model.active {
        chat.draft = draft.clone();
    }

    if home && app.ui.focus_home_composer {
        ui.memory_mut(|memory| memory.request_focus(text_id));
        app.ui.focus_home_composer = false;
    }
    let first_focus = Id::unique(("composer-first-focus", &key));
    let seen = ui.memory(|memory| memory.data.get_temp::<bool>(first_focus).unwrap_or(false));
    if !seen && !app.ui.model_menu && !app.ui.agent_menu {
        ui.memory_mut(|memory| {
            memory.request_focus(text_id);
            memory.data.insert_temp(first_focus, true);
        });
    }

    for action in actions {
        app.backend.send(action);
    }
    app.model.save_draft();
    editor_height
}

#[derive(Clone, Copy, Debug, Default)]
struct PasteEvents {
    text: bool,
    image: bool,
    command: bool,
}

impl PasteEvents {
    fn wants_image(self) -> bool {
        !self.text && (self.image || self.command)
    }
}

fn paste_events(events: &[egui::Event]) -> PasteEvents {
    let mut found = PasteEvents::default();
    for event in events {
        match event {
            egui::Event::Paste(text) if !text.is_empty() => found.text = true,
            egui::Event::PasteImage(_) => found.image = true,
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if (*key == Key::Paste
                || (*key == Key::V && (modifiers.command || modifiers.ctrl))) =>
            {
                found.command = true;
            }
            _ => {}
        }
    }
    found
}

enum PasteIntercept {
    Url { selection: String, url: String },
    Large(String),
    Image(image::RgbaImage),
}

fn intercept_paste(ui: &Ui, text_id: Id, draft: &str) -> Option<PasteIntercept> {
    let pasted = ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
    });
    if let Some(pasted) = pasted {
        if looks_like_url(&pasted)
            && let Some(selection) = selection_text(ui.ctx(), text_id, draft)
        {
            let trimmed = selection.trim();
            if !trimmed.is_empty() && !selection.contains('\n') {
                ui.input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, egui::Event::Paste(_)))
                });
                return Some(PasteIntercept::Url {
                    selection,
                    url: pasted.trim().to_string(),
                });
            }
        }
        if is_large_plain_text_paste(&pasted) {
            ui.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Paste(_)))
            });
            return Some(PasteIntercept::Large(pasted));
        }
        return None;
    }
    pasted_image(ui).map(PasteIntercept::Image)
}

fn pasted_image(ui: &Ui) -> Option<image::RgbaImage> {
    let (events, native) = ui.input(|input| {
        let native = input.events.iter().find_map(|event| match event {
            egui::Event::PasteImage(image) => Some(image.clone()),
            _ => None,
        });
        (paste_events(&input.events), native)
    });
    if !events.wants_image() {
        return None;
    }
    ui.input_mut(|input| {
        input
            .events
            .retain(|event| !matches!(event, egui::Event::PasteImage(_)))
    });
    let clipboard = arboard::Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get_image().ok())
        .and_then(|image| {
            image::RgbaImage::from_raw(
                image.width as u32,
                image.height as u32,
                image.bytes.into_owned(),
            )
        });
    clipboard.or_else(|| {
        native.and_then(|image| {
            let bytes = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_srgba_unmultiplied())
                .collect();
            image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, bytes)
        })
    })
}

fn save_pasted_image(ctx: &egui::Context, image: &image::RgbaImage) -> Option<std::path::PathBuf> {
    let index = ctx.memory_mut(|memory| {
        let id = Id::unique("composer-pasted-image");
        let next = memory.data.get_temp::<usize>(id).unwrap_or(0) + 1;
        memory.data.insert_temp(id, next);
        next
    });
    let path = std::env::temp_dir().join(format!("rchamber-pasted-image-{index}.png"));
    image.save(&path).ok()?;
    Some(path)
}

fn attach_pasted_image(
    app: &mut RustChamber,
    key: &str,
    image: &image::RgbaImage,
    ctx: &egui::Context,
) {
    let Some(path) = save_pasted_image(ctx, image) else {
        app.model.toast(
            rchamber_app::ToastKind::Warning,
            "Could not save the pasted image",
        );
        return;
    };
    match attachments::prepare_path(&path, app.model.workspace.as_deref()) {
        Ok(items) => {
            for item in items {
                app.model.add_attachment(key, item);
            }
        }
        Err(error) => app.model.toast(rchamber_app::ToastKind::Warning, error),
    }
}

fn attach_pasted_text(
    app: &mut RustChamber,
    key: &str,
    text: &str,
    ctx: &egui::Context,
    text_id: Id,
    draft: &mut String,
) {
    let index = ctx.memory_mut(|memory| {
        let id = Id::unique("composer-pasted-context");
        let next = memory.data.get_temp::<usize>(id).unwrap_or(0) + 1;
        memory.data.insert_temp(id, next);
        next
    });
    let name = format!("pasted-context-{index}.txt");
    let path = std::env::temp_dir().join(format!("rchamber-{name}"));
    if std::fs::write(&path, text).is_ok() {
        match attachments::prepare_path(&path, app.model.workspace.as_deref()) {
            Ok(items) => {
                for item in items {
                    app.model.add_attachment(key, item);
                }
                insert_text_at_caret(ctx, text_id, draft, &format!("[{name}] "));
                app.model
                    .toast(rchamber_app::ToastKind::Info, format!("Attached {name}"));
            }
            Err(error) => {
                app.model.toast(rchamber_app::ToastKind::Warning, error);
                insert_text_at_caret(ctx, text_id, draft, text);
            }
        }
    } else {
        insert_text_at_caret(ctx, text_id, draft, text);
    }
}

fn large_text_paste_choice(ui: &mut Ui, p: &Palette) -> Option<bool> {
    let frame = egui::Frame::new()
        .fill(p.popover)
        .corner_radius(theme::radius::PILL)
        .inner_margin(Margin::same(4))
        .shadow(glass_shadow(p));
    let mut resolved = None;
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            if paste_choice_button(ui, "Attach as file", p) {
                resolved = Some(true);
            }
            let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 20.0), Sense::hover());
            ui.painter().rect_filled(
                Rect::from_center_size(rect.center(), Vec2::new(1.0, 20.0)),
                CornerRadius::ZERO,
                p.border,
            );
            if paste_choice_button(ui, "Keep in message", p) {
                resolved = Some(false);
            }
        });
    });
    resolved
}

fn paste_choice_button(ui: &mut Ui, label: &str, p: &Palette) -> bool {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::font_medium(13.0), p.fg);
    let size = Vec2::new(galley.size().x + 28.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::PILL), p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().galley(
        Pos2::new(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0),
        galley,
        p.fg,
    );
    response.clicked()
}

fn placeholder_text(ui: &Ui, home: bool) -> String {
    let _ = home;
    let long = "Plan, build, or ask. @ to mention, $ for skills, / for commands";
    let short = "Plan, build, or ask";
    let fits = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                long.to_string(),
                theme::font(EDITOR_FONT),
                Color32::PLACEHOLDER,
            )
            .size()
            .x
            <= ui.available_width() - EDITOR_PAD_X * 2.0
    });
    if fits {
        long.to_string()
    } else {
        short.to_string()
    }
}

fn caret_follow(
    ui: &Ui,
    output: &egui::widgets::text_edit::TextEditOutput,
    focused: bool,
    _text_id: Id,
    _draft: &str,
) {
    if !focused {
        return;
    }
    let Some(range) = output.cursor_range else {
        return;
    };
    let caret = output
        .galley
        .pos_from_cursor(range.primary)
        .translate(output.galley_pos.to_vec2());
    let clip = output.text_clip_rect;
    if caret.top() < clip.top() + 1.0 || caret.bottom() > clip.bottom() - 1.0 {
        ui.scroll_to_rect(caret, Some(Align::BOTTOM));
    }
}

fn click_lands_below_text(rect: Rect, text_bottom: f32, pos: Option<Pos2>) -> bool {
    pos.is_some_and(|pos| rect.contains(pos) && pos.y > text_bottom)
}

fn caret_anchor_of(
    output: &egui::widgets::text_edit::TextEditOutput,
    draft: &str,
    text_id: Id,
) -> Option<CaretAnchor> {
    let ctx = &output.response.ctx;
    let state = TextEditState::load(ctx, text_id)?;
    let range = state.cursor.char_range()?;
    let caret = output
        .galley
        .pos_from_cursor(range.primary)
        .translate(output.galley_pos.to_vec2());
    let caret_index = range.primary.index.0;
    let trigger_index =
        trigger(draft, caret_index).map(|(_, byte, _, _)| draft[..byte].chars().count());
    let trigger = match trigger_index {
        Some(index) => {
            let start = output
                .galley
                .pos_from_cursor(CCursor::new(index))
                .translate(output.galley_pos.to_vec2());
            Rect::from_min_max(
                Pos2::new(start.left(), caret.top()),
                Pos2::new(start.left() + 1.0, caret.bottom()),
            )
        }
        None => caret,
    };
    Some(CaretAnchor { trigger })
}

fn cycle_agent(app: &mut RustChamber, home: bool, backwards: bool, actions: &mut Vec<Action>) {
    let agents: Vec<String> = app
        .model
        .primary_agents()
        .into_iter()
        .map(|agent| agent.id.clone())
        .collect();
    if agents.is_empty() {
        return;
    }
    let current = current_agent(app);
    let index = current
        .as_ref()
        .and_then(|id| agents.iter().position(|agent| agent == id))
        .unwrap_or(0);
    let next = if backwards {
        (index + agents.len() - 1) % agents.len()
    } else {
        (index + 1) % agents.len()
    };
    let id = agents[next].clone();
    if home {
        app.ui.home_agent = Some(id);
    } else if let Some(session) = app.model.active_id().map(str::to_string) {
        actions.push(Action::SwitchAgent {
            session_id: session,
            agent: id,
        });
    }
}

fn box_shadow(p: &Palette) -> egui::epaint::Shadow {
    if p.dark {
        egui::epaint::Shadow {
            offset: [0, 2],
            blur: 4,
            spread: 0,
            color: Color32::from_black_alpha(64),
        }
    } else {
        egui::epaint::Shadow {
            offset: [0, 4],
            blur: 10,
            spread: 0,
            color: Color32::from_black_alpha(18),
        }
    }
}

fn glass_shadow(p: &Palette) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 8],
        blur: 16,
        spread: 0,
        color: p.shadow,
    }
}

fn popover_frame(p: &Palette) -> egui::Frame {
    theme::popover_frame(p)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FooterCaps {
    agent: f32,
    effort: f32,
    model: f32,
}

fn footer_caps(available: f32, agent_text: f32, effort_text: Option<f32>) -> FooterCaps {
    let mut agent = agent_text.clamp(0.0, FLIP_LABEL_CAP - AGENT_CHROME);
    let mut effort = effort_text
        .map(|width| width.clamp(0.0, THINK_LABEL_CAP - EFFORT_CHROME))
        .unwrap_or(0.0);
    let effort_chrome = if effort_text.is_some() {
        EFFORT_CHROME
    } else {
        0.0
    };
    let fixed =
        FOOTER_BUTTON + FOOTER_GROUP_GAP * 3.0 + AGENT_CHROME + MODEL_CHROME + effort_chrome;
    let mut room = available - fixed - agent - effort;
    if room < MODEL_LABEL_MIN {
        let need = MODEL_LABEL_MIN - room;
        let taken = need.min(effort);
        effort -= taken;
        agent -= (need - taken).min(agent).max(0.0);
        room = available - fixed - agent - effort;
    }
    FooterCaps {
        agent,
        effort,
        model: room.clamp(0.0, LABEL_MAX_WIDTH),
    }
}

fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER)
            .size()
            .x
    })
}

fn agent_label(app: &RustChamber) -> String {
    current_agent(app)
        .and_then(|id| app.model.agents.iter().find(|entry| entry.id == id))
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| "Build".into())
}

fn effort_label(app: &RustChamber) -> Option<String> {
    let reference = current_model(app)?;
    let entry = app.model.model_for(&reference)?;
    if entry.variants.is_empty() {
        return None;
    }
    Some(display_effort(reference.variant.as_deref()))
}

fn composer_footer(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    text_id: Id,
    home: bool,
    busy: bool,
    draft: &str,
    has_attachments: bool,
    key: &str,
    actions: &mut Vec<Action>,
    send: &mut bool,
) {
    let can_send = !draft.trim().is_empty() || has_attachments;
    egui::Frame::new()
        .inner_margin(Margin {
            left: FOOTER_PAD_X as i8,
            right: FOOTER_PAD_X as i8,
            top: FOOTER_PAD_Y as i8,
            bottom: FOOTER_PAD_Y as i8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(FOOTER_GAP, 0.0);
                composer_attachment_controls(ui, app, p, key);
                let pending = app
                    .model
                    .active
                    .as_ref()
                    .map(|chat| chat.pending.len())
                    .unwrap_or(0);
                let allow_all = if home {
                    app.ui.home_allow_all
                } else {
                    app.model
                        .active
                        .as_ref()
                        .is_some_and(|chat| chat.session.allows_all())
                };
                let shield = footer_icon_button(
                    ui,
                    if allow_all {
                        Icon::ShieldCheck
                    } else {
                        Icon::ShieldUser
                    },
                    FOOTER_GLYPH,
                    if allow_all { p.blue } else { p.fg_muted },
                    true,
                );
                let label = if pending > 0 {
                    "Permission needed"
                } else if allow_all {
                    "Permissions: allow all"
                } else {
                    "Permissions: ask always"
                };
                if shield.on_hover_text(label).clicked() {
                    let next = !allow_all;
                    if home {
                        app.ui.home_allow_all = next;
                    } else if let Some(chat) = &app.model.active {
                        actions.push(Action::SetSessionPermissions {
                            id: chat.session.id.clone(),
                            allow_all: next,
                        });
                    }
                    app.model.toast(
                        if next {
                            rchamber_app::ToastKind::Warning
                        } else {
                            rchamber_app::ToastKind::Info
                        },
                        if next {
                            "Permissions: allow all for this session"
                        } else {
                            "Permissions: ask always"
                        },
                    );
                }

                let right_space = ui.available_width();
                let caps = footer_caps(
                    right_space,
                    text_width(ui, &agent_label(app), theme::font_medium(12.5)),
                    effort_label(app).map(|label| text_width(ui, &label, theme::font_medium(12.5))),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(FOOTER_GROUP_GAP, 0.0);
                    match composer_action_buttons(ui, p, can_send, busy) {
                        ActionChoice::Send => *send = true,
                        ActionChoice::Stop => {
                            if let Some(id) = app.model.active_id().map(str::to_string) {
                                actions.push(Action::Interrupt(id));
                            }
                        }
                        ActionChoice::None => {}
                    }
                    agent_control(ui, app, p, home, actions, caps.agent);
                    model_control(ui, app, p, home, actions, caps.model, text_id);
                    thinking_control(ui, app, p, home, actions, caps.effort, text_id);
                    if busy && can_send {
                        let rect = ui.min_rect();
                        let target = Rect::from_min_size(
                            Pos2::new(
                                rect.right() - FOOTER_BUTTON,
                                rect.top() - 4.0 - FOOTER_BUTTON,
                            ),
                            Vec2::splat(FOOTER_BUTTON),
                        );
                        let response = ui.interact(
                            target,
                            Id::unique(("composer-follow-up", key)),
                            Sense::click(),
                        );
                        icons::paint(
                            ui,
                            Icon::ArrowUp,
                            Rect::from_center_size(target.center(), Vec2::splat(FOOTER_SEND_GLYPH)),
                            p.primary,
                        );
                        if response.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if response.clicked() {
                            *send = true;
                        }
                    }
                });
            });
        });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionChoice {
    None,
    Send,
    Stop,
}

fn composer_action_buttons(ui: &mut Ui, p: &Palette, can_send: bool, busy: bool) -> ActionChoice {
    if !busy {
        if footer_icon_button(ui, Icon::Send, FOOTER_SEND_GLYPH, p.primary, can_send).clicked() {
            return ActionChoice::Send;
        }
        return ActionChoice::None;
    }
    if footer_icon_button(ui, Icon::Stop, FOOTER_STOP_GLYPH, p.red, true).clicked() {
        return ActionChoice::Stop;
    }
    ActionChoice::None
}

fn footer_icon_button(
    ui: &mut Ui,
    icon: Icon,
    glyph: f32,
    tint: Color32,
    enabled: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(FOOTER_BUTTON), Sense::click());
    let color = if enabled {
        tint
    } else {
        Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 77)
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(glyph)),
        color,
    );
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn composer_attachment_controls(ui: &mut Ui, app: &mut RustChamber, p: &Palette, key: &str) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(FOOTER_BUTTON), Sense::click());
    icons::paint(
        ui,
        Icon::Plus,
        Rect::from_center_size(rect.center(), Vec2::splat(FOOTER_GLYPH)),
        if response.hovered() { p.fg } else { p.fg_muted },
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let id = Id::unique(("attach-menu", key.to_string()));
    let mut open = app.ui.attach_menu;
    let mut items: Vec<Response> = Vec::new();
    menu_popup(ui.ctx(), &response, id, &mut open, p, 0.0, |ui, _| {
        ui.set_width(190.0);
        let attach = widgets::menu_item(ui, "Attach files", Some(Icon::Paperclip), None, p);
        if attach.clicked()
            && let Some(paths) = rfd::FileDialog::new().pick_files()
        {
            let (prepared, errors) =
                attachments::prepare_paths(&paths, app.model.workspace.as_deref());
            for item in prepared {
                app.model.add_attachment(key, item);
            }
            for error in errors {
                app.model.toast(rchamber_app::ToastKind::Warning, error);
            }
        }
        items.push(attach);
    });
    menu_nav_focus(ui.ctx(), id, open, &items);
    if response.clicked() {
        app.ui.attach_menu = !app.ui.attach_menu;
    } else if !open {
        app.ui.attach_menu = false;
    }
}

fn model_control(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    home: bool,
    actions: &mut Vec<Action>,
    max_width: f32,
    text_id: Id,
) {
    let model = current_model(app);
    let was_open = app.ui.model_menu;
    if std::env::var_os("RCHAMBER_DEBUG_MODEL").is_some() {
        app.ui.model_menu = true;
        if std::env::var_os("RCHAMBER_DEBUG_FILTER").is_some() {
            app.ui.model_query = "deepseek".into();
        }
    }
    let model_entry = model
        .as_ref()
        .and_then(|reference| app.model.model_for(reference));
    let name_label = model_entry
        .map(|entry| entry.display_name().to_string())
        .or_else(|| model.as_ref().map(|reference| reference.id.clone()))
        .unwrap_or_else(|| "Select model".into());
    let provider_id = model
        .as_ref()
        .map(|reference| reference.provider_id.as_str());
    let name_response = model_button(ui, &name_label, provider_id, p, max_width);
    let id = Id::unique("model-picker");
    let mut open = app.ui.model_menu;
    menu_popup(
        ui.ctx(),
        &name_response,
        id,
        &mut open,
        p,
        POPUP_GAP,
        |ui, _| {
            ui.set_width(380.0);
            ui.add_space(6.0);
            widgets::search_field(
                ui,
                &mut app.ui.model_query,
                "Search models",
                p,
                Id::unique("model-search"),
            );
            ui.add_space(6.0);
            ScrollArea::vertical()
                .max_height(400.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let query = app.ui.model_query.to_lowercase();
                    let models: Vec<Model> = app
                        .model
                        .enabled_models()
                        .into_iter()
                        .filter(|candidate| {
                            query.is_empty()
                                || candidate.display_name().to_lowercase().contains(&query)
                                || candidate.provider_id.to_lowercase().contains(&query)
                                || candidate
                                    .family
                                    .as_deref()
                                    .map(|family| family.to_lowercase().contains(&query))
                                    .unwrap_or(false)
                        })
                        .cloned()
                        .collect();
                    let previous = ui
                        .ctx()
                        .data(|data| data.get_temp::<String>(id.with("nav-query")));
                    if previous.as_deref() != Some(app.ui.model_query.as_str()) {
                        menu_nav_reset(ui.ctx(), id);
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(id.with("nav-query"), app.ui.model_query.clone())
                        });
                    }
                    let nav = menu_nav(ui.ctx(), id, app.ui.model_menu, models.len(), true);
                    let mut last_provider = String::new();
                    for (row, candidate) in models.iter().enumerate() {
                        if candidate.provider_id != last_provider {
                            last_provider = candidate.provider_id.clone();
                            ui.add_space(4.0);
                            let name = app
                                .model
                                .providers
                                .iter()
                                .find(|provider| provider.id == candidate.provider_id)
                                .map(|provider| provider.name.clone())
                                .unwrap_or_else(|| candidate.provider_id.clone());
                            menu_section(ui, &name, p);
                        }
                        let row_rect = Rect::from_min_size(
                            ui.cursor().min,
                            Vec2::new(ui.available_width(), MODEL_ROW),
                        );
                        if nav.index == Some(row) {
                            ui.painter().rect_filled(
                                row_rect,
                                CornerRadius::same(theme::radius::LG),
                                p.hover,
                            );
                        }
                        model_row(
                            ui,
                            app,
                            p,
                            candidate,
                            &model,
                            home,
                            actions,
                            nav.index == Some(row),
                        );
                        if nav.index == Some(row) && nav.moved {
                            ui.scroll_to_rect(row_rect, Some(Align::Center));
                        }
                    }
                    if let Some(index) = nav.index
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        && let Some(candidate) = models.get(index)
                    {
                        activate_model(app, candidate, home, actions);
                    }
                    if nav.index.is_some()
                        && ui.ctx().input(|input| input.pointer.delta() != Vec2::ZERO)
                    {
                        ui.ctx()
                            .data_mut(|data| data.remove::<usize>(id.with("nav-index")));
                    }
                    if models.is_empty() {
                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("No matching models")
                                    .font(theme::font(12.0))
                                    .color(p.fg_faint),
                            );
                        });
                        ui.add_space(12.0);
                    }
                });
        },
    );
    if name_response.clicked() {
        app.ui.model_menu = !app.ui.model_menu;
    } else if !open {
        app.ui.model_menu = false;
    }
    let focus_id = Id::unique("model-search-focus");
    if app.ui.model_menu {
        let focused_once = ui
            .memory(|memory| memory.data.get_temp::<bool>(focus_id))
            .unwrap_or(false);
        if !focused_once {
            ui.ctx()
                .memory_mut(|memory| memory.request_focus(Id::unique("model-search")));
            ui.memory_mut(|memory| memory.data.insert_temp(focus_id, true));
        }
    } else {
        ui.memory_mut(|memory| memory.data.remove_temp::<bool>(focus_id));
    }
    if was_open && !app.ui.model_menu {
        ui.ctx().memory_mut(|memory| memory.request_focus(text_id));
    }
}

fn agent_control(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    home: bool,
    actions: &mut Vec<Action>,
    max_width: f32,
) {
    let agent = current_agent(app);
    let agent_label = agent_label(app);
    let response = icon_label_button(
        ui,
        Icon::Bot,
        &agent_label,
        p.green,
        p.fg_muted,
        p,
        max_width,
    );
    let id = Id::unique("agent-picker");
    let mut open = app.ui.agent_menu;
    let mut items: Vec<Response> = Vec::new();
    menu_popup(ui.ctx(), &response, id, &mut open, p, POPUP_GAP, |ui, _| {
        ui.set_width(280.0);
        menu_section(ui, "Agents", p);
        ui.add_space(2.0);
        let agents: Vec<Agent> = app.model.primary_agents().into_iter().cloned().collect();
        for entry in &agents {
            let selected = agent.as_deref() == Some(entry.id.as_str());
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
            if response.hovered() || response.has_focus() || selected {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(theme::radius::LG),
                    if selected { p.selection } else { p.hover },
                );
            }
            ui.painter().circle_filled(
                Pos2::new(rect.left() + 10.0, rect.center().y),
                2.0,
                p.primary,
            );
            ui.painter().text(
                Pos2::new(rect.left() + 20.0, rect.center().y),
                Align2::LEFT_CENTER,
                shorten(&entry.name, 30),
                theme::font(13.5),
                p.fg,
            );
            if selected {
                icons::paint(
                    ui,
                    Icon::Check,
                    Rect::from_center_size(
                        Pos2::new(rect.right() - 14.0, rect.center().y),
                        Vec2::splat(14.0),
                    ),
                    p.primary,
                );
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if let Some(description) = entry.description.as_deref() {
                widgets::tooltip(response.clone(), description.to_string());
            }
            if response.clicked() {
                if home {
                    app.ui.home_agent = Some(entry.id.clone());
                } else if let Some(chat) = &app.model.active {
                    actions.push(Action::SwitchAgent {
                        session_id: chat.session.id.clone(),
                        agent: entry.id.clone(),
                    });
                }
                app.ui.agent_menu = false;
            }
            items.push(response);
        }
    });
    menu_nav_focus(ui.ctx(), id, open, &items);
    if response.clicked() {
        app.ui.agent_menu = !app.ui.agent_menu;
    } else if !open {
        app.ui.agent_menu = false;
    }
}

fn model_button(
    ui: &mut Ui,
    label: &str,
    provider: Option<&str>,
    p: &Palette,
    max_width: f32,
) -> Response {
    let cap = max_width.clamp(0.0, LABEL_MAX_WIDTH);
    let show_label = cap >= 12.0;
    let galley = row_galley(
        ui,
        label,
        theme::font_medium(12.5),
        p.fg_muted,
        cap.max(1.0),
    );
    let lead = if provider.is_some() { 22.0 } else { 0.0 };
    let width = if show_label {
        galley.size().x + lead + 4.0 + 10.0
    } else {
        FOOTER_BUTTON
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, FOOTER_BUTTON), Sense::click());
    if let Some(id) = provider {
        paint_provider_badge(
            ui,
            Pos2::new(
                if show_label {
                    rect.left() + 9.0
                } else {
                    rect.center().x
                },
                rect.center().y,
            ),
            id,
            crate::chat::provider_color(id, p),
            p,
        );
    }
    if show_label {
        let color = if response.hovered() { p.fg } else { p.fg_muted };
        ui.painter().with_clip_rect(rect).galley(
            Pos2::new(
                rect.left() + lead + 2.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );
    }
    if response.hovered() {
        icons::paint(
            ui,
            Icon::ChevronDown,
            Rect::from_center_size(
                Pos2::new(rect.right() - 4.0, rect.center().y),
                Vec2::splat(9.0),
            ),
            p.fg_muted,
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn paint_provider_badge(ui: &Ui, center: Pos2, provider: &str, tint: Color32, p: &Palette) {
    let rect = Rect::from_center_size(center, Vec2::splat(14.0));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(4),
        p.alpha(tint, if p.dark { 44 } else { 34 }),
    );
    let initial = provider
        .chars()
        .next()
        .map(|value| value.to_ascii_uppercase())
        .unwrap_or('?');
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        initial,
        theme::font_semibold(9.0),
        tint,
    );
}

fn icon_label_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    icon_color: Color32,
    text_color: Color32,
    p: &Palette,
    max_width: f32,
) -> Response {
    let cap = max_width.clamp(0.0, LABEL_MAX_WIDTH);
    let show_label = cap >= 12.0;
    let galley = row_galley(
        ui,
        label,
        theme::font_medium(12.5),
        text_color,
        cap.max(1.0),
    );
    let width = if show_label {
        galley.size().x + 24.0
    } else {
        FOOTER_BUTTON
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, FOOTER_BUTTON), Sense::click());
    let icon_center = if show_label {
        Pos2::new(rect.left() + 8.0, rect.center().y)
    } else {
        rect.center()
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(icon_center, Vec2::splat(14.0)),
        icon_color,
    );
    if show_label {
        ui.painter().with_clip_rect(rect).galley(
            Pos2::new(rect.left() + 22.0, rect.center().y - galley.size().y / 2.0),
            galley,
            if response.hovered() { p.fg } else { text_color },
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn display_effort(variant: Option<&str>) -> String {
    match variant
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default")
    {
        None => "Default".to_string(),
        Some(value) => {
            let mut chars = value.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Default".to_string(),
            }
        }
    }
}

fn thinking_control(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    home: bool,
    actions: &mut Vec<Action>,
    max_width: f32,
    text_id: Id,
) {
    let model = current_model(app);
    let Some(entry) = model
        .as_ref()
        .and_then(|reference| app.model.model_for(reference))
    else {
        return;
    };
    let variants: Vec<String> = entry
        .variants
        .iter()
        .map(|variant| variant.id.clone())
        .collect();
    if variants.is_empty() {
        return;
    }
    let current = model
        .as_ref()
        .and_then(|reference| reference.variant.clone());
    let active = current
        .as_deref()
        .is_some_and(|value| !value.is_empty() && value != "default");
    let tint = if active { p.primary_text } else { p.fg_muted };
    let trigger = icon_label_button(
        ui,
        Icon::Spark,
        &display_effort(current.as_deref()),
        tint,
        tint,
        p,
        max_width,
    );
    thinking_menu(
        ui,
        app,
        p,
        &trigger,
        &variants,
        current.as_deref(),
        home,
        actions,
        text_id,
    );
}

fn favorite_key(provider: &str, id: &str) -> String {
    format!("{provider}/{id}")
}

fn is_favorite(ctx: &egui::Context, key: &str) -> bool {
    ctx.data(|data| {
        data.get_temp::<std::collections::BTreeSet<String>>(Id::unique("composer-model-favorites"))
    })
    .is_some_and(|favorites| favorites.contains(key))
}

fn toggle_favorite(ctx: &egui::Context, key: &str) {
    ctx.data_mut(|data| {
        let id = Id::unique("composer-model-favorites");
        let mut favorites = data
            .get_temp::<std::collections::BTreeSet<String>>(id)
            .unwrap_or_default();
        if !favorites.remove(key) {
            favorites.insert(key.to_string());
        }
        data.insert_temp(id, favorites);
    });
}

fn paint_star(ui: &Ui, center: Pos2, radius: f32, fill: Option<Color32>, outline: Color32) {
    let inner = radius * 0.46;
    let mut points = Vec::with_capacity(10);
    for step in 0..10 {
        let angle = -std::f32::consts::FRAC_PI_2 + step as f32 * std::f32::consts::PI / 5.0;
        let arm = if step % 2 == 0 { radius } else { inner };
        points.push(center + Vec2::angled(angle) * arm);
    }
    let painter = ui.painter();
    if let Some(fill) = fill {
        for index in 0..10 {
            painter.add(egui::Shape::convex_polygon(
                vec![center, points[index], points[(index + 1) % 10]],
                fill,
                Stroke::NONE,
            ));
        }
    } else {
        let mut outline_points = points.clone();
        outline_points.push(points[0]);
        painter.add(egui::Shape::line(outline_points, Stroke::new(1.0, outline)));
    }
}

fn model_row(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    candidate: &Model,
    selected: &Option<ModelRef>,
    home: bool,
    actions: &mut Vec<Action>,
    highlighted: bool,
) {
    let is_selected = selected.as_ref().is_some_and(|model| {
        model.id == candidate.id && model.provider_id == candidate.provider_id
    });
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), MODEL_ROW), Sense::click());
    let highlighted = highlighted || response.hovered() || response.has_focus();
    if highlighted || is_selected {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::LG),
            if is_selected { p.selection } else { p.hover },
        );
    }
    paint_provider_badge(
        ui,
        Pos2::new(rect.left() + 15.0, rect.center().y),
        &candidate.provider_id,
        crate::chat::provider_color(&candidate.provider_id, p),
        p,
    );
    let name = shorten(candidate.display_name(), 34);
    let name_galley = row_galley(
        ui,
        &name,
        theme::font_medium(12.5),
        p.fg,
        rect.width() * 0.55,
    );
    ui.painter().with_clip_rect(rect).galley(
        Pos2::new(
            rect.left() + 27.0,
            rect.center().y - name_galley.size().y / 2.0,
        ),
        name_galley.clone(),
        p.fg,
    );
    if (highlighted || is_selected) && !candidate.variants.is_empty() {
        let effort = if is_selected {
            display_effort(selected.as_ref().and_then(|model| model.variant.as_deref()))
        } else {
            "Default".to_string()
        };
        let thinking = format!("Thinking: {effort}");
        let thinking_galley = row_galley(
            ui,
            &thinking,
            theme::font(theme::TEXT_MICRO),
            p.fg_muted,
            (rect.right()
                - MODEL_ROW_TRAILING
                - (rect.left() + 27.0 + name_galley.size().x + 8.0)
                - 8.0)
                .max(20.0),
        );
        ui.painter().with_clip_rect(rect).galley(
            Pos2::new(
                rect.left() + 27.0 + name_galley.size().x + 8.0,
                rect.center().y - thinking_galley.size().y / 2.0,
            ),
            thinking_galley,
            p.fg_muted,
        );
    }
    let details = candidate
        .context_limit()
        .map(|limit| format!("{} ctx", rchamber_app::format::compact_number(limit as f64)))
        .unwrap_or_default();
    if !details.is_empty() {
        ui.painter().text(
            Pos2::new(rect.right() - MODEL_ROW_TRAILING, rect.center().y),
            Align2::RIGHT_CENTER,
            details,
            theme::font_mono(11.0),
            p.fg_muted,
        );
    }
    if is_selected {
        icons::paint(
            ui,
            Icon::Check,
            Rect::from_center_size(
                Pos2::new(
                    rect.right()
                        - MODEL_ROW_PAD
                        - MODEL_ROW_STAR
                        - MODEL_ROW_SLOT_GAP
                        - MODEL_ROW_CHECK / 2.0,
                    rect.center().y,
                ),
                Vec2::splat(14.0),
            ),
            p.primary,
        );
    }
    let key = favorite_key(&candidate.provider_id, &candidate.id);
    let favorite = is_favorite(ui.ctx(), &key);
    let star_rect = Rect::from_center_size(
        Pos2::new(
            rect.right() - MODEL_ROW_PAD - MODEL_ROW_STAR / 2.0,
            rect.center().y,
        ),
        Vec2::splat(MODEL_ROW_STAR),
    );
    let star = ui
        .interact(
            star_rect,
            Id::unique(("model-favorite", key.as_str())),
            Sense::click(),
        )
        .on_hover_text(if favorite {
            "Remove from favorites"
        } else {
            "Add to favorites"
        });
    if favorite || highlighted || is_selected || star.hovered() {
        paint_star(
            ui,
            star_rect.center(),
            6.5,
            favorite.then_some(p.primary),
            if favorite { p.primary } else { p.fg_muted },
        );
    }
    if star.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if star.clicked() {
        toggle_favorite(ui.ctx(), &key);
        return;
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if !response.clicked() {
        return;
    }
    activate_model(app, candidate, home, actions);
}

fn activate_model(app: &mut RustChamber, candidate: &Model, home: bool, actions: &mut Vec<Action>) {
    let reference = ModelRef {
        id: candidate.id.clone(),
        provider_id: candidate.provider_id.clone(),
        variant: None,
    };
    select_model(app, reference, home, actions);
    app.ui.model_menu = false;
}

fn select_model(app: &mut RustChamber, reference: ModelRef, home: bool, actions: &mut Vec<Action>) {
    if home {
        app.ui.home_model = Some(reference);
    } else if let Some(chat) = &app.model.active {
        actions.push(Action::SwitchModel {
            session_id: chat.session.id.clone(),
            model: reference,
        });
    }
}

fn thinking_menu(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    trigger: &Response,
    variants: &[String],
    current: Option<&str>,
    home: bool,
    actions: &mut Vec<Action>,
    text_id: Id,
) {
    let id = Id::unique("variant-menu");
    if std::env::var_os("RCHAMBER_DEBUG_THINK").is_some() {
        ui.memory_mut(|memory| memory.data.insert_temp(id, true));
    }
    let mut open = ui
        .memory(|memory| memory.data.get_temp::<bool>(id))
        .unwrap_or(false);
    let had_open = open;
    let is_default = current.is_none_or(|value| value.is_empty() || value == "default");
    let mut items: Vec<Response> = Vec::new();
    if open {
        let mut held = true;
        let mut close = false;
        let width = thinking_menu_width(ui, variants);
        egui::Popup::from_response(trigger)
            .id(id)
            .open_bool(&mut held)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align(egui::RectAlign::TOP_END)
            .align_alternatives(&[])
            .gap(POPUP_GAP)
            .frame(popover_frame(p))
            .show(|ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 2.0;
                menu_section(ui, "Thinking", p);
                let default_item =
                    widgets::menu_item_ex(ui, "Default", None, None, is_default, true, p);
                if default_item.clicked() {
                    if let Some(reference) = current_model(app) {
                        select_model(
                            app,
                            ModelRef {
                                variant: None,
                                ..reference
                            },
                            home,
                            actions,
                        );
                    }
                    close = true;
                }
                items.push(default_item);
                ui.add_space(2.0);
                widgets::hline(ui, p, 4.0);
                ui.add_space(2.0);
                for variant in variants {
                    let selected = current == Some(variant.as_str());
                    let label = display_effort(Some(variant));
                    let item = widgets::menu_item_ex(ui, &label, None, None, selected, true, p);
                    if item.clicked() {
                        if let Some(reference) = current_model(app) {
                            select_model(
                                app,
                                ModelRef {
                                    variant: Some(variant.clone()),
                                    ..reference
                                },
                                home,
                                actions,
                            );
                        }
                        close = true;
                    }
                    items.push(item);
                }
            });
        if !held {
            open = false;
        }
        if close {
            open = false;
        }
    }
    menu_nav_focus(ui.ctx(), id, open, &items);
    if trigger.clicked() {
        open = !open;
    }
    if !open {
        clear_variant_nav(ui.ctx(), id);
        if had_open {
            ui.ctx().memory_mut(|memory| memory.request_focus(text_id));
        }
    }
    ui.memory_mut(|memory| memory.data.insert_temp(id, open));
}

fn clear_variant_nav(ctx: &egui::Context, id: Id) {
    let focused = ctx.memory(|memory| memory.focused());
    let ids = ctx.data(|data| data.get_temp::<Vec<Id>>(id.with("nav-items")));
    if let (Some(focused), Some(ids)) = (focused, ids)
        && ids.contains(&focused)
    {
        ctx.memory_mut(|memory| memory.surrender_focus(focused));
    }
    ctx.data_mut(|data| {
        data.remove::<Vec<Id>>(id.with("nav-items"));
        data.remove::<usize>(id.with("nav-index"));
    });
}

fn menu_section(ui: &mut Ui, text: &str, p: &Palette) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        widgets::section_label(ui, text, p);
    });
    ui.add_space(2.0);
}

fn thinking_menu_width(ui: &Ui, variants: &[String]) -> f32 {
    let widest = std::iter::once("Default".to_string())
        .chain(variants.iter().map(|variant| display_effort(Some(variant))))
        .map(|label| {
            ui.painter()
                .layout_no_wrap(
                    label,
                    theme::font(theme::TEXT_UI_LABEL),
                    Color32::PLACEHOLDER,
                )
                .size()
                .x
        })
        .fold(0.0f32, f32::max);
    (widest + 56.0).max(128.0)
}

struct QueueEntry<'a> {
    id: String,
    item: &'a serde_json::Value,
    text: String,
    file_count: usize,
    delivery: String,
    agent: Option<String>,
    scheduled: bool,
}

fn queue_entry(item: &serde_json::Value) -> Option<QueueEntry<'_>> {
    let payload = item
        .get("payload")
        .or_else(|| item.get("item").and_then(|item| item.get("payload")));
    let id = item
        .get("id")
        .or_else(|| item.get("inboxID"))
        .or_else(|| item.get("item").and_then(|item| item.get("id")))
        .and_then(|value| value.as_str())?
        .to_string();
    let text = payload
        .and_then(|payload| payload.get("text"))
        .or_else(|| item.get("text"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let file_count = payload
        .and_then(|payload| payload.get("files"))
        .and_then(|value| value.as_array())
        .map(Vec::len)
        .unwrap_or(0);
    let delivery = item
        .get("delivery")
        .and_then(|value| value.as_str())
        .unwrap_or("queue")
        .to_string();
    let agent = payload
        .and_then(|payload| payload.get("agent"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let scheduled = item
        .get("scheduledTask")
        .or_else(|| payload.and_then(|payload| payload.get("scheduledTask")))
        .is_some_and(|value| !value.is_null());
    Some(QueueEntry {
        id,
        item,
        text,
        file_count,
        delivery,
        agent,
        scheduled,
    })
}

fn queued_messages_strip(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    items: &[serde_json::Value],
    session_id: Option<&str>,
    draft: &mut String,
    text_id: Id,
    actions: &mut Vec<Action>,
) {
    let entries: Vec<QueueEntry> = items.iter().filter_map(queue_entry).collect();
    if entries.is_empty() {
        return;
    }
    let key = session_id.unwrap_or("home").to_string();
    let width = ui.available_width();
    let count = entries.len();
    let strip_top = ui.cursor().min.y;
    let mut expanded = true;
    let mut can_collapse = false;
    if count > 1 {
        let expanded_id = Id::unique(("queue-expanded", &key));
        expanded = ui
            .memory(|memory| memory.data.get_temp::<bool>(expanded_id))
            .unwrap_or(true);
        can_collapse = true;
        let header_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(width, QUEUE_ROW));
        let mut header = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(header_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        header.spacing_mut().item_spacing = Vec2::ZERO;
        let response = header.interact(
            header_rect,
            Id::unique(("queue-header", &key)),
            Sense::click(),
        );
        if response.hovered() {
            header
                .painter()
                .rect_filled(header_rect, CornerRadius::ZERO, p.hover);
        }
        icons::paint(
            &header,
            Icon::Clock,
            Rect::from_center_size(
                Pos2::new(
                    header_rect.left() + QUEUE_ROW_INSET_START + 7.0,
                    header_rect.center().y,
                ),
                Vec2::splat(14.0),
            ),
            p.fg_muted,
        );
        let label = format!("Queued messages {count}");
        let galley = header
            .painter()
            .layout_no_wrap(label, theme::font(14.0), p.fg_muted);
        header.painter().with_clip_rect(header_rect).galley(
            Pos2::new(
                header_rect.left() + QUEUE_ROW_INSET_START + 22.0,
                header_rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            if response.hovered() { p.fg } else { p.fg_muted },
        );
        icons::paint(
            &header,
            if expanded {
                Icon::ArrowUp
            } else {
                Icon::ArrowDown
            },
            Rect::from_center_size(
                Pos2::new(
                    header_rect.right() - QUEUE_ROW_INSET_START - 8.0,
                    header_rect.center().y,
                ),
                Vec2::splat(16.0),
            ),
            p.fg_muted,
        );
        if response.hovered() {
            header.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            expanded = !expanded;
            header.memory_mut(|memory| memory.data.insert_temp(expanded_id, expanded));
        }
        ui.advance_cursor_after_rect(header_rect);
    }

    let anim = if can_collapse {
        motion::animate_bool_with_time(
            ui.ctx(),
            Id::unique(("queue-anim", &key)),
            expanded,
            QUEUE_EXPAND_SECONDS,
        )
    } else {
        1.0
    };
    let first = if can_collapse { 1 } else { 0 };
    let content = ((count - first) as f32 * QUEUE_ROW).min(QUEUE_LIST_MAX);
    let shown = (content * anim).max(0.0);
    if shown > 0.5 {
        let start = ui.cursor().min;
        let list_rect = Rect::from_min_size(start, Vec2::new(width, shown));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(list_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        child.set_clip_rect(list_rect);
        child.spacing_mut().item_spacing = Vec2::ZERO;
        ScrollArea::vertical()
            .id_salt(("composer-queue", &key))
            .max_height(QUEUE_LIST_MAX)
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                for index in first..entries.len() {
                    let row_rect = Rect::from_min_size(
                        ui.cursor().min,
                        Vec2::new(ui.available_width(), QUEUE_ROW),
                    );
                    let mut row = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(row_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    row.spacing_mut().item_spacing = Vec2::ZERO;
                    paint_queue_row(
                        &mut row, app, p, &entries, index, session_id, draft, text_id, actions,
                    );
                    ui.advance_cursor_after_rect(row_rect);
                }
            });
        ui.advance_cursor_after_rect(Rect::from_min_size(start, Vec2::new(width, shown)));
    }
    let strip_bottom = strip_top + QUEUE_ROW + shown;
    ui.painter().hline(
        ui.max_rect().x_range(),
        strip_bottom - 0.25,
        Stroke::new(theme::HAIRLINE, p.alpha(p.border, 134)),
    );
}

fn paint_queue_row(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    entries: &[QueueEntry],
    index: usize,
    session_id: Option<&str>,
    draft: &mut String,
    text_id: Id,
    actions: &mut Vec<Action>,
) {
    let Some(entry) = entries.get(index) else {
        return;
    };
    let full = ui.max_rect();
    let rect = Rect::from_min_size(full.min, Vec2::new(full.width(), QUEUE_ROW));
    let drag_id = Id::unique(("queue-drag", session_id.unwrap_or("home")));
    let dragging = ui.memory(|memory| memory.data.get_temp::<usize>(drag_id)) == Some(index);
    let offset = if dragging {
        let start_y = ui
            .memory(|memory| memory.data.get_temp::<f32>(Id::unique("queue-drag-start")))
            .unwrap_or(rect.center().y);
        let current = ui
            .ctx()
            .input(|input| input.pointer.interact_pos().map(|pos| pos.y))
            .unwrap_or(start_y);
        Vec2::new(0.0, current - start_y)
    } else {
        Vec2::ZERO
    };
    let lifted = dragging;
    let row = rect.translate(offset);

    let grip = Rect::from_center_size(
        Pos2::new(row.left() + QUEUE_ROW_INSET_START + 7.0, row.center().y),
        Vec2::splat(14.0),
    );
    let single = entries.len() == 1;
    if single {
        icons::paint(ui, Icon::Clock, grip, p.fg_muted);
    } else {
        icons::paint(
            ui,
            Icon::Menu,
            grip,
            if ui.rect_contains_pointer(grip) {
                p.fg
            } else {
                p.fg_muted
            },
        );
        let drag = ui.interact(grip, Id::unique(("queue-grip", &entry.id)), Sense::drag());
        if drag.hovered() || drag.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if drag.drag_started() {
            let start_y = ui
                .ctx()
                .input(|input| input.pointer.interact_pos().map(|pos| pos.y))
                .unwrap_or(row.center().y);
            ui.memory_mut(|memory| memory.data.insert_temp(drag_id, index));
            ui.memory_mut(|memory| {
                memory
                    .data
                    .insert_temp(Id::unique("queue-drag-start"), start_y)
            });
        }
        if drag.drag_stopped() {
            let from = ui.memory(|memory| memory.data.get_temp::<usize>(drag_id));
            let start_y = ui
                .memory(|memory| memory.data.get_temp::<f32>(Id::unique("queue-drag-start")))
                .unwrap_or(row.center().y);
            let current = ui
                .ctx()
                .input(|input| input.pointer.interact_pos().map(|pos| pos.y))
                .unwrap_or(start_y);
            if let Some(from) = from {
                let delta = current - start_y;
                let moved = (delta / QUEUE_ROW).round() as i32;
                let target = (from as i32 + moved).clamp(0, entries.len() as i32 - 1) as usize;
                if target != from
                    && let Some(session) = session_id
                {
                    let mut ids = entries
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect::<Vec<_>>();
                    let item = ids.remove(from);
                    ids.insert(target, item);
                    actions.push(Action::ReorderInbox {
                        session_id: session.to_string(),
                        inbox_ids: ids,
                    });
                }
                ui.memory_mut(|memory| memory.data.remove_temp::<usize>(drag_id));
                ui.memory_mut(|memory| {
                    memory
                        .data
                        .remove_temp::<f32>(Id::unique("queue-drag-start"))
                });
            }
        }
    }

    let text_left = row.left() + QUEUE_ROW_INSET_START + 22.0;
    let right = row.right() - QUEUE_ROW_INSET_END;
    let mut buttons: Vec<(&str, Icon, Color32)> = Vec::new();
    if !entry.scheduled {
        buttons.push(("Send", Icon::Send, p.fg_muted));
        buttons.push(("Edit", Icon::Pencil, p.fg_muted));
    }
    let remove = Icon::Close;
    let mut reserved = FOOTER_BUTTON + FOOTER_GAP;
    for (label, icon, _) in &buttons {
        reserved += xs_button_width(ui, label, Some(*icon)) + FOOTER_GAP;
    }
    let text_right = (row.right() - QUEUE_ROW_INSET_END - reserved).max(text_left);

    let mut suffix = String::new();
    if entry.file_count > 0 {
        suffix.push_str(&format!(
            "  +{} file{}",
            entry.file_count,
            if entry.file_count == 1 { "" } else { "s" }
        ));
    }
    if entry.delivery == "steer" {
        suffix.push_str("  · steer");
    }
    let first_line = entry
        .text
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let label = if first_line.is_empty() {
        "Empty message".to_string()
    } else {
        format!("{first_line}{suffix}")
    };
    let agent = entry
        .agent
        .clone()
        .filter(|agent| current_agent(app).as_deref() != Some(agent.as_str()));
    let mut badge_width = 0.0;
    if let Some(agent) = &agent {
        let galley =
            ui.painter()
                .layout_no_wrap(agent.clone(), theme::font_medium(13.5), p.primary);
        badge_width = (galley.size().x + 20.0).min((text_right - text_left) * 0.4);
        let badge = Rect::from_min_size(
            Pos2::new(text_right - badge_width, row.center().y - 9.0),
            Vec2::new(badge_width, 18.0),
        );
        icons::paint(
            ui,
            Icon::Bot,
            Rect::from_center_size(
                Pos2::new(badge.left() + 7.0, row.center().y),
                Vec2::splat(14.0),
            ),
            p.primary,
        );
        ui.painter().with_clip_rect(badge).text(
            Pos2::new(badge.left() + 18.0, row.center().y),
            Align2::LEFT_CENTER,
            agent,
            theme::font_medium(13.5),
            p.primary,
        );
    }

    if text_right > text_left {
        let galley = row_galley(
            ui,
            &label,
            theme::font(14.0),
            if lifted { p.alpha(p.fg, 153) } else { p.fg },
            (text_right - text_left - badge_width).max(1.0),
        );
        ui.painter()
            .with_clip_rect(Rect::from_min_max(
                Pos2::new(text_left, row.top()),
                Pos2::new(text_right, row.bottom()),
            ))
            .galley(
                Pos2::new(text_left, row.center().y - galley.size().y / 2.0),
                galley,
                if lifted { p.alpha(p.fg, 153) } else { p.fg },
            );
    }

    let mut x = right;
    let remove_width = FOOTER_BUTTON;
    x -= remove_width;
    if queue_icon_button(
        ui,
        Id::unique(("queue-cancel", &entry.id)),
        remove,
        Rect::from_center_size(
            Pos2::new(x + remove_width / 2.0, row.center().y),
            Vec2::splat(16.0),
        ),
        p,
        lifted,
    ) && !lifted
        && let Some(session) = session_id
    {
        actions.push(Action::CancelInbox {
            session_id: session.to_string(),
            inbox_id: entry.id.clone(),
        });
    }
    for (label, icon, tint) in buttons.iter() {
        let width = xs_button_width(ui, label, Some(*icon));
        x -= FOOTER_GAP;
        let button_rect = Rect::from_min_size(
            Pos2::new(x - width, row.center().y - 12.0),
            Vec2::new(width, 24.0),
        );
        if queue_label_button(ui, &entry.id, label, *icon, *tint, button_rect, p, lifted) && !lifted
        {
            match *label {
                "Send" => {
                    if let Some(session) = session_id {
                        actions.push(Action::SetInboxDelivery {
                            session_id: session.to_string(),
                            inbox_id: entry.id.clone(),
                            delivery: "steer".into(),
                        });
                    }
                }
                _ => {
                    if !entry.text.trim().is_empty() {
                        draft.clear();
                        draft.push_str(&entry.text);
                    }
                    let files = entry
                        .item
                        .get("payload")
                        .or_else(|| entry.item.get("item").and_then(|item| item.get("payload")))
                        .and_then(|payload| payload.get("files"))
                        .cloned()
                        .and_then(|value| serde_json::from_value::<Vec<PromptFile>>(value).ok())
                        .unwrap_or_default();
                    let restored = files
                        .into_iter()
                        .map(|file| AttachmentItem {
                            file,
                            source_document_id: None,
                            size: 0,
                        })
                        .collect::<Vec<_>>();
                    app.model
                        .restore_attachments(&key_for(session_id), restored);
                    if let Some(session) = session_id {
                        actions.push(Action::CancelInbox {
                            session_id: session.to_string(),
                            inbox_id: entry.id.clone(),
                        });
                    }
                    set_cursor_to_end(ui.ctx(), text_id, draft);
                }
            }
        }
        x -= width;
    }
}

fn xs_button_width(ui: &Ui, label: &str, icon: Option<Icon>) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        theme::font(theme::TEXT_MICRO),
        Color32::PLACEHOLDER,
    );
    let icon_space = icon.map(|_| 18.0).unwrap_or(0.0);
    (galley.size().x + icon_space + 16.0).max(24.0)
}

fn queue_label_button(
    ui: &mut Ui,
    key: &str,
    label: &str,
    icon: Icon,
    tint: Color32,
    rect: Rect,
    p: &Palette,
    lifted: bool,
) -> bool {
    let response = ui.interact(
        rect,
        Id::unique((key, "queue-button", label)),
        Sense::click(),
    );
    if response.hovered() && !lifted {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::SM), p.hover);
    }
    let color = if lifted {
        p.alpha(tint, 153)
    } else if response.hovered() {
        p.fg
    } else {
        tint
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 8.0 + 6.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        color,
    );
    ui.painter().text(
        Pos2::new(rect.left() + 8.0 + 18.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::font(theme::TEXT_MICRO),
        color,
    );
    if response.hovered() && !lifted {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

fn queue_icon_button(
    ui: &mut Ui,
    id: Id,
    icon: Icon,
    rect: Rect,
    p: &Palette,
    lifted: bool,
) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    if response.hovered() && !lifted {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::SM), p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::paint(
        ui,
        icon,
        rect,
        if lifted {
            p.alpha(p.fg_muted, 153)
        } else if response.hovered() {
            p.fg
        } else {
            p.fg_muted
        },
    );
    response.clicked()
}

fn key_for(session_id: Option<&str>) -> String {
    session_id.unwrap_or("home").to_string()
}

fn file_chip(
    ui: &mut Ui,
    p: &Palette,
    item: &AttachmentItem,
    index: usize,
    remove: &mut Option<usize>,
) {
    let name = item
        .file
        .name
        .as_deref()
        .unwrap_or("attachment")
        .to_string();
    let size = if item.size > 0 {
        format_size(item.size)
    } else {
        String::new()
    };
    let name_galley = row_galley(ui, &name, theme::font(12.0), p.fg, ATTACHMENT_NAME_MAX);
    let size_galley = if size.is_empty() {
        None
    } else {
        Some(
            ui.painter()
                .layout_no_wrap(size.clone(), theme::font(12.0), p.fg_muted),
        )
    };
    let width = 8.0
        + ATTACHMENT_ICON
        + 6.0
        + name_galley.size().x
        + size_galley
            .as_ref()
            .map(|galley| 6.0 + galley.size().x)
            .unwrap_or(0.0)
        + 4.0
        + ATTACHMENT_REMOVE;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, ATTACHMENT_HEIGHT), Sense::hover());
    let stroke = if response.hovered() {
        p.alpha(p.border, 191)
    } else {
        p.alpha(p.border, 134)
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::TABLE), p.bg);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::radius::TABLE),
        Stroke::new(1.0, stroke),
        StrokeKind::Inside,
    );
    icons::paint(
        ui,
        attachment_icon(&item.file),
        Rect::from_center_size(
            Pos2::new(rect.left() + 8.0 + ATTACHMENT_ICON / 2.0, rect.center().y),
            Vec2::splat(ATTACHMENT_ICON),
        ),
        p.fg_muted,
    );
    let mut x = rect.left() + 8.0 + ATTACHMENT_ICON + 6.0;
    ui.painter().with_clip_rect(rect).galley(
        Pos2::new(x, rect.center().y - name_galley.size().y / 2.0),
        name_galley.clone(),
        p.fg,
    );
    x += name_galley.size().x + 6.0;
    if let Some(galley) = size_galley {
        ui.painter().with_clip_rect(rect).galley(
            Pos2::new(x, rect.center().y - galley.size().y / 2.0),
            galley,
            p.fg_muted,
        );
    }
    let close = Rect::from_center_size(
        Pos2::new(
            rect.right() - 4.0 - ATTACHMENT_REMOVE / 2.0,
            rect.center().y,
        ),
        Vec2::splat(ATTACHMENT_REMOVE),
    );
    let close_response = ui.interact(
        close,
        Id::unique(("attachment-remove", index, &name)),
        Sense::click(),
    );
    if close_response.hovered() {
        ui.painter()
            .rect_filled(close, CornerRadius::same(theme::radius::MD), p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::paint(
        ui,
        Icon::Close,
        Rect::from_center_size(close.center(), Vec2::splat(14.0)),
        p.fg_muted,
    );
    if close_response.clicked() {
        *remove = Some(index);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn attachment_icon(file: &PromptFile) -> Icon {
    if file.mime.starts_with("image/")
        || file.mime.contains("pdf")
        || file.mime.starts_with("text/")
    {
        Icon::File
    } else {
        Icon::Paperclip
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let value = bytes as f64;
    if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.0} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

fn permission_dock(
    ui: &mut Ui,
    p: &Palette,
    requests: &[PermissionRequest],
    request: &PermissionRequest,
    actions: &mut Vec<Action>,
) -> f32 {
    let index = requests
        .iter()
        .position(|entry| entry.id == request.id)
        .unwrap_or(0);
    let collapsed_id = Id::unique(("composer-dock-collapsed", request.id.as_str()));
    let mut collapsed = ui
        .memory(|memory| memory.data.get_temp::<bool>(collapsed_id))
        .unwrap_or(false);
    let response = composer_floating_panel(
        ui,
        p,
        Id::unique(("composer-dock-motion", request.id.as_str())),
        |ui| {
            egui::Frame::new()
                .inner_margin(Margin {
                    left: 12,
                    right: 12,
                    top: 6,
                    bottom: 6,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                        icons::icon(ui, Icon::Warning, 14.0, p.coral);
                        let toggle = footer_icon_button(
                            ui,
                            if collapsed {
                                Icon::ChevronRight
                            } else {
                                Icon::ChevronDown
                            },
                            16.0,
                            p.fg_muted,
                            true,
                        );
                        let title = ui.label(
                            RichText::new("Permission needed")
                                .font(theme::font(13.5))
                                .color(p.fg),
                        );
                        if toggle.clicked() || title.clicked() {
                            collapsed = !collapsed;
                            ui.memory_mut(|memory| {
                                memory.data.insert_temp(collapsed_id, collapsed)
                            });
                        }
                        if requests.len() > 1 {
                            ui.label(
                                RichText::new(format!("{}/{}", index + 1, requests.len()))
                                    .font(theme::font(12.0))
                                    .color(p.fg_muted),
                            );
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(shorten(&request.action, 40))
                                    .font(theme::font_medium(12.5))
                                    .color(p.fg_muted),
                            );
                        });
                    });
                });
            if collapsed {
                return;
            }
            egui::Frame::new()
                .inner_margin(Margin {
                    left: 16,
                    right: 16,
                    top: 0,
                    bottom: 6,
                })
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .id_salt(("permission-body", &request.id))
                        .max_height(ui.ctx().content_rect().height() * 0.5)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            if let Some(resource) = request.resources.first() {
                                ui.label(
                                    RichText::new(shorten(resource, 160))
                                        .font(theme::font_mono(11.5))
                                        .color(p.fg_muted),
                                );
                            }
                            if let Some(message) = &request.message {
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(shorten(message, 200))
                                        .font(theme::font(12.5))
                                        .color(p.fg_muted),
                                );
                            }
                        });
                });
            egui::Frame::new()
                .inner_margin(Margin {
                    left: 12,
                    right: 12,
                    top: 4,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                        if widgets::button(
                            ui,
                            "Deny",
                            Some(Icon::Close),
                            widgets::ButtonVariant::Ghost,
                            widgets::ButtonSize::Xs,
                            p,
                        )
                        .clicked()
                        {
                            actions.push(Action::ReplyPermission {
                                session_id: request.session_id.clone(),
                                request_id: request.id.clone(),
                                decision: "reject".into(),
                            });
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                            if widgets::button(
                                ui,
                                "Allow once",
                                Some(Icon::Check),
                                widgets::ButtonVariant::Primary,
                                widgets::ButtonSize::Xs,
                                p,
                            )
                            .clicked()
                            {
                                actions.push(Action::ReplyPermission {
                                    session_id: request.session_id.clone(),
                                    request_id: request.id.clone(),
                                    decision: "once".into(),
                                });
                            }
                            if widgets::button(
                                ui,
                                "Always allow",
                                None,
                                widgets::ButtonVariant::Outline,
                                widgets::ButtonSize::Xs,
                                p,
                            )
                            .clicked()
                            {
                                actions.push(Action::ReplyPermission {
                                    session_id: request.session_id.clone(),
                                    request_id: request.id.clone(),
                                    decision: "always".into(),
                                });
                            }
                        });
                    });
                });
        },
    );
    response.response.rect.height()
}

fn composer_floating_panel(
    ui: &mut Ui,
    p: &Palette,
    motion_id: Id,
    add: impl FnOnce(&mut Ui),
) -> egui::InnerResponse<()> {
    let enter = motion::animate_bool_with_time(ui.ctx(), motion_id, true, GLASS_ENTER_SECONDS);
    let offset = 3.0 * (1.0 - enter);

    ui.scope(|ui| {
        ui.add_space(offset);
        let frame = egui::Frame::new()
            .fill(p.over(p.popover, Color32::TRANSPARENT))
            .corner_radius(theme::radius::XL)
            .inner_margin(Margin::ZERO)
            .shadow(glass_shadow(p));
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    })
}

fn trigger(draft: &str, caret: usize) -> Option<(char, usize, String, usize)> {
    let chars: Vec<(usize, char)> = draft.char_indices().collect();
    let limit = caret.min(chars.len());
    let mut found = None;
    for index in (0..limit).rev() {
        let ch = chars[index].1;
        if matches!(ch, '@' | '$' | '/' | '#') {
            let boundary = index == 0 || chars[index - 1].1.is_whitespace();
            if boundary {
                found = Some(index);
            }
            break;
        }
        if ch.is_whitespace() {
            break;
        }
    }
    let index = found?;
    let kind = chars[index].1;
    if kind == '/' && index != 0 {
        return None;
    }
    let byte = chars[index].0;
    let end = chars
        .get(limit)
        .map(|(byte, _)| *byte)
        .unwrap_or(draft.len());
    let query = draft[byte + kind.len_utf8()..end].to_lowercase();
    if query.contains(char::is_whitespace) {
        return None;
    }
    if kind == '$' && query.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some((kind, byte, query, end))
}

fn apply_picker_choice(draft: &mut String, picker: &Picker, value: &str) -> Option<usize> {
    let start = picker.start_byte.min(draft.len());
    let end = picker.end_byte.min(draft.len());
    if start > end || !draft.is_char_boundary(start) || !draft.is_char_boundary(end) {
        return None;
    }
    let inserted = format!("{}{value} ", picker.kind);
    let prefix = draft[..start].chars().count();
    draft.replace_range(start..end, &inserted);
    Some(prefix + inserted.chars().count())
}

fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn expand_extension_command(text: &str) -> (String, Option<rchamber_api::PromptFile>) {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix("/new-extension") else {
        return (text.to_string(), None);
    };
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return (text.to_string(), None);
    }
    let materialized = rchamber_ext::materialize_authoring_skill().ok();
    let guide = materialized.as_ref().and_then(|path| {
        rchamber_api::PromptFile::from_path(
            path,
            Some("RustChamber extension guide".into()),
            "text/markdown",
            Some("Read this before writing the extension.".into()),
        )
    });
    let fallback = materialized
        .as_ref()
        .map(|path| {
            format!(
                " If the attachment is unavailable, read {} and follow it.",
                path.display()
            )
        })
        .unwrap_or_default();
    let attachment = if guide.is_some() {
        " Follow the attached guide."
    } else {
        ""
    };
    (
        format!(
            "Build the RustChamber extension described below. Keep it minimal and finish quickly: one panel or one status section, no test harness, no repository exploration, and at most one `rchamber ext check`.{attachment}{fallback}\n\n{}",
            rest.trim()
        ),
        guide,
    )
}

fn extension_command(app: &RustChamber, text: &str) -> Option<(String, String, String)> {
    let trimmed = text.trim_start();
    let rest = trimmed.strip_prefix('/')?;
    let (name, args) = match rest.find(char::is_whitespace) {
        Some(index) => (&rest[..index], rest[index..].trim()),
        None => (rest, ""),
    };
    if name == "new-extension"
        || app
            .model
            .commands
            .iter()
            .any(|command| command.name == name)
    {
        return None;
    }
    let extension = app.model.extensions.installed.iter().find(|extension| {
        extension.is_active()
            && extension
                .manifest
                .contributes
                .commands
                .iter()
                .any(|command| command.name == name)
    })?;
    Some((
        extension.manifest.id.clone(),
        name.to_string(),
        args.to_string(),
    ))
}

fn composer_picker(
    ctx: &egui::Context,
    app: &RustChamber,
    draft: &str,
    text_id: Id,
) -> Option<Picker> {
    let caret = TextEditState::load(ctx, text_id)
        .and_then(|state| state.cursor.char_range())
        .map(|range| range.primary.index.0)
        .unwrap_or_else(|| draft.chars().count());
    let (kind, start, query, end) = trigger(draft, caret)?;
    let dismissed = ctx.memory(|memory| {
        memory
            .data
            .get_temp::<(usize, String)>(Id::unique("composer-picker-dismissed"))
    });
    if let Some((dismissed_start, dismissed_query)) = dismissed
        && dismissed_start == start
        && dismissed_query == query
    {
        return None;
    }
    let mut items: Vec<PickerItem> = Vec::new();
    let mut width = 420.0;
    match kind {
        '@' => {
            width = 560.0;
            if !query.is_empty() {
                let last = ctx.memory(|memory| {
                    memory
                        .data
                        .get_temp::<(String, f64)>(Id::unique("mention-query"))
                });
                let now = ctx.input(|input| input.time);
                if last
                    .as_ref()
                    .map(|(previous, at)| previous != &query || now - at > 0.25)
                    .unwrap_or(true)
                {
                    app.backend.send(Action::SearchFiles(query.clone()));
                    ctx.memory_mut(|memory| {
                        memory
                            .data
                            .insert_temp(Id::unique("mention-query"), (query.clone(), now))
                    });
                }
            }
            let last = ctx.memory(|memory| {
                memory
                    .data
                    .get_temp::<(String, f64)>(Id::unique("ext-mention-query"))
            });
            let now = ctx.input(|input| input.time);
            if last
                .as_ref()
                .map(|(previous, at)| previous != &query || now - at > 0.25)
                .unwrap_or(true)
            {
                app.backend.send(Action::ExtensionMentions {
                    query: query.clone(),
                });
                ctx.memory_mut(|memory| {
                    memory
                        .data
                        .insert_temp(Id::unique("ext-mention-query"), (query.clone(), now))
                });
            }
            for row in &app.model.extensions.mentions {
                if items.len() >= POPUP_LIMIT {
                    break;
                }
                if !query.is_empty()
                    && !row.label.to_lowercase().contains(&query)
                    && !row.detail.to_lowercase().contains(&query)
                {
                    continue;
                }
                items.push(PickerItem {
                    value: row.text.clone(),
                    label: row.label.clone(),
                    detail: if row.detail.is_empty() {
                        row.extension_name.clone()
                    } else {
                        format!("{} · {}", row.extension_name, row.detail)
                    },
                    icon: Icon::Puzzle,
                    section: "Extensions",
                });
            }
            for agent in app.model.primary_agents() {
                if !query.is_empty()
                    && !agent.name.to_lowercase().contains(&query)
                    && !agent.id.to_lowercase().contains(&query)
                {
                    continue;
                }
                items.push(PickerItem {
                    value: agent.id.clone(),
                    label: agent.name.clone(),
                    detail: "agent".into(),
                    icon: Icon::Bot,
                    section: "Agents",
                });
            }
            for entry in &app.model.files.search {
                if items.len() >= POPUP_LIMIT {
                    break;
                }
                let suffix = if entry.kind == "directory" && !entry.path.ends_with('/') {
                    "/"
                } else {
                    ""
                };
                let value = format!("{}{suffix}", entry.path);
                items.push(PickerItem {
                    value: value.clone(),
                    label: value,
                    detail: if entry.kind == "directory" {
                        "folder".into()
                    } else {
                        String::new()
                    },
                    icon: if entry.kind == "directory" {
                        Icon::Folder
                    } else {
                        Icon::File
                    },
                    section: "Files",
                });
            }
            if !query.is_empty() {
                for session in &app.model.sessions {
                    if items.len() >= POPUP_LIMIT {
                        break;
                    }
                    let title = session.title();
                    if !title.to_lowercase().contains(&query) {
                        continue;
                    }
                    if Some(session.id.as_str()) == app.model.active_id() {
                        continue;
                    }
                    items.push(PickerItem {
                        value: session.id.clone(),
                        label: title.to_string(),
                        detail: "session".into(),
                        icon: Icon::Layers,
                        section: "Sessions",
                    });
                }
            }
        }
        '$' => {
            for skill in &app.model.skills {
                if items.len() >= POPUP_LIMIT {
                    break;
                }
                if !query.is_empty()
                    && !skill.name.to_lowercase().contains(&query)
                    && !skill.id.to_lowercase().contains(&query)
                {
                    continue;
                }
                items.push(PickerItem {
                    value: skill.id.clone(),
                    label: skill.name.clone(),
                    detail: skill.description.clone().unwrap_or_default(),
                    icon: Icon::Spark,
                    section: "Skills",
                });
            }
        }
        '#' => {
            for snippet in &app.model.snippets {
                if items.len() >= POPUP_LIMIT {
                    break;
                }
                if !query.is_empty()
                    && !snippet.name.to_lowercase().contains(&query)
                    && !snippet
                        .aliases
                        .iter()
                        .any(|alias| alias.to_lowercase().contains(&query))
                {
                    continue;
                }
                items.push(PickerItem {
                    value: snippet.name.clone(),
                    label: snippet.name.clone(),
                    detail: snippet.description.clone().unwrap_or_default(),
                    icon: Icon::Copy,
                    section: "Snippets",
                });
            }
        }
        _ => {
            let new_extension = "new-extension";
            if query.is_empty() || new_extension.contains(&query) {
                items.push(PickerItem {
                    value: new_extension.into(),
                    label: "/new-extension".into(),
                    detail: "Build a RustChamber extension with the agent".into(),
                    icon: Icon::Puzzle,
                    section: "RustChamber",
                });
            }
            for extension in app
                .model
                .extensions
                .installed
                .iter()
                .filter(|extension| extension.is_active())
            {
                for command in &extension.manifest.contributes.commands {
                    if items.len() >= POPUP_LIMIT {
                        break;
                    }
                    if !query.is_empty() && !command.name.to_lowercase().contains(&query) {
                        continue;
                    }
                    if command.name == "new-extension"
                        || app
                            .model
                            .commands
                            .iter()
                            .any(|builtin| builtin.name == command.name)
                    {
                        continue;
                    }
                    items.push(PickerItem {
                        value: command.name.clone(),
                        label: format!("/{}", command.name),
                        detail: command
                            .description
                            .clone()
                            .unwrap_or_else(|| extension.manifest.name.clone()),
                        icon: Icon::Puzzle,
                        section: "Extensions",
                    });
                }
            }
            for command in &app.model.commands {
                if items.len() >= POPUP_LIMIT {
                    break;
                }
                if !query.is_empty() && !command.name.to_lowercase().contains(&query) {
                    continue;
                }
                items.push(PickerItem {
                    value: command.name.clone(),
                    label: format!("/{}", command.name),
                    detail: command.description.clone().unwrap_or_default(),
                    icon: Icon::Command,
                    section: "Commands",
                });
            }
        }
    }
    if items.is_empty() {
        return None;
    }
    items.sort_by_key(|item| {
        let prefix = !item.label.to_lowercase().starts_with(&query);
        (prefix, item.label.to_lowercase())
    });
    items.truncate(POPUP_LIMIT);
    Some(Picker {
        kind,
        start_byte: start,
        end_byte: end,
        query,
        items,
        width,
    })
}

fn picker_keyboard(
    ui: &mut Ui,
    picker: &mut Picker,
    focused: bool,
    draft: &mut String,
    text_id: Id,
) {
    if !focused {
        return;
    }
    let index_id = picker.index_id();
    let mut index = ui
        .memory(|memory| memory.data.get_temp::<usize>(index_id))
        .unwrap_or(0);
    let mut accept = false;
    let mut dismiss = false;
    let count = picker.items.len();
    if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown)) {
        index = (index + 1) % count;
    }
    if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp)) {
        index = (index + count - 1) % count;
    }
    if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
        dismiss = true;
    }
    let tab = ui.input_mut(|input| {
        input.consume_key(Modifiers::NONE, Key::Tab)
            || input.consume_key(Modifiers::SHIFT, Key::Tab)
    });
    if tab {
        index = (index + 1) % count;
    }
    if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
        accept = true;
    }
    if dismiss {
        ui.memory_mut(|memory| {
            memory.data.insert_temp(
                Id::unique("composer-picker-dismissed"),
                (picker.start_byte, picker.query.clone()),
            );
            memory.data.remove_temp::<usize>(index_id);
        });
        return;
    }
    ui.memory_mut(|memory| memory.data.insert_temp(index_id, index));
    if accept {
        if let Some(item) = picker.items.get(index) {
            if let Some(caret) = apply_picker_choice(draft, picker, &item.value)
                && let Some(mut state) = TextEditState::load(ui.ctx(), text_id)
            {
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::one(CCursor::new(caret))));
                state.store(ui.ctx(), text_id);
            }
            ui.memory_mut(|memory| {
                memory.data.remove_temp::<usize>(index_id);
            });
            ui.ctx().memory_mut(|memory| memory.request_focus(text_id));
        }
        picker.items.clear();
    }
}

fn place_caret_popup(
    anchor: CaretAnchor,
    composer: Rect,
    area_top: f32,
    width: f32,
) -> PopupPlacement {
    let fitted = width.max(0.0).min(composer.width());
    let start = anchor.trigger.left() - POPUP_TEXT_INSET;
    let left = start.max(composer.left()).min(composer.right() - fitted);
    let bottom = composer.top() - POPUP_GAP;
    let available = (bottom - area_top - POPUP_EDGE).max(0.0);
    PopupPlacement {
        left,
        bottom,
        width: fitted,
        max_height: available
            .min(POPUP_MAX_HEIGHT)
            .max(POPUP_MIN_HEIGHT.min(available)),
    }
}

fn composer_autocomplete_popup(
    ctx: &egui::Context,
    p: &Palette,
    picker: &Picker,
    anchor: CaretAnchor,
    composer_rect: Rect,
    area: Rect,
) -> Option<String> {
    if picker.items.is_empty() {
        return None;
    }
    let t = motion::animate_bool_with_time(
        ctx,
        Id::unique("composer-picker-motion"),
        true,
        PICKER_ENTER_SECONDS,
    );
    let placement = place_caret_popup(anchor, composer_rect, area.top(), picker.width);
    let bottom = placement.bottom + (1.0 - t) * 3.0;
    let index = ctx
        .memory(|memory| memory.data.get_temp::<usize>(picker.index_id()))
        .unwrap_or(0);
    let last_index_id = Id::unique((
        "composer-picker-last-index",
        picker.kind,
        picker.start_byte,
        picker.query.as_str(),
    ));
    let previous_index = ctx.memory(|memory| memory.data.get_temp::<usize>(last_index_id));
    let open_id = Id::unique("composer-picker-open");
    let was_open = ctx.memory(|memory| memory.data.get_temp::<bool>(open_id).unwrap_or(false));
    let reveal = !was_open || previous_index != Some(index);
    ctx.memory_mut(|memory| {
        memory.data.insert_temp(open_id, true);
        memory.data.insert_temp(last_index_id, index);
    });
    let mut chosen = None;
    let area_response = egui::Area::new(Id::unique("composer-autocomplete"))
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(placement.left, bottom))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .constrain(false)
        .show(ctx, |ui| {
            let frame = popover_frame(p).inner_margin(Margin::same(POPUP_INSET as i8));
            frame.show(ui, |ui| {
                ui.set_width((placement.width - POPUP_INSET * 2.0).max(0.0));
                ScrollArea::vertical()
                    .id_salt("composer-picker-scroll")
                    .max_height((placement.max_height - POPUP_INSET * 2.0).max(POPUP_ROW))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut section = "";
                        for (row, item) in picker.items.iter().enumerate() {
                            if item.section != section {
                                section = item.section;
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(section)
                                            .font(theme::font_medium(11.0))
                                            .color(p.fg_muted),
                                    );
                                });
                                ui.add_space(2.0);
                            }
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), POPUP_ROW),
                                Sense::click(),
                            );
                            if reveal && row == index {
                                ui.scroll_to_rect(rect, Some(Align::Center));
                            }
                            if response.hovered() || row == index {
                                ui.painter().rect_filled(
                                    rect,
                                    CornerRadius::same(theme::radius::LG),
                                    if row == index { p.selection } else { p.hover },
                                );
                            }
                            icons::paint(
                                ui,
                                item.icon,
                                Rect::from_center_size(
                                    Pos2::new(rect.left() + 18.0, rect.center().y),
                                    Vec2::splat(14.0),
                                ),
                                p.fg_muted,
                            );
                            let detail = if item.detail.is_empty() {
                                None
                            } else {
                                Some(row_galley(
                                    ui,
                                    &item.detail,
                                    theme::font(12.0),
                                    p.fg_muted,
                                    (rect.width() * 0.42).max(40.0),
                                ))
                            };
                            let detail_width = detail
                                .as_ref()
                                .map(|galley| galley.size().x + 8.0)
                                .unwrap_or(0.0);
                            let label = row_galley(
                                ui,
                                &item.label,
                                theme::font(13.5),
                                p.fg,
                                (rect.width() - 40.0 - detail_width).max(40.0),
                            );
                            ui.painter().with_clip_rect(rect).galley(
                                Pos2::new(
                                    rect.left() + 32.0,
                                    rect.center().y - label.size().y / 2.0,
                                ),
                                label,
                                p.fg,
                            );
                            if let Some(detail) = detail {
                                ui.painter().with_clip_rect(rect).galley(
                                    Pos2::new(
                                        rect.right() - 8.0 - detail.size().x,
                                        rect.center().y - detail.size().y / 2.0,
                                    ),
                                    detail,
                                    p.fg_muted,
                                );
                            }
                            if response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if response.clicked() {
                                chosen = Some(item.value.clone());
                            }
                        }
                        ui.add_space(POPUP_INSET);
                    });
            });
        });
    ctx.move_to_top(area_response.response.layer_id);
    chosen
}

fn row_galley(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(
        text.to_string(),
        TextFormat {
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

fn set_cursor_to_end(ctx: &egui::Context, text_id: Id, draft: &str) {
    let end = draft.chars().count();
    if let Some(mut state) = TextEditState::load(ctx, text_id) {
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(end))));
        state.store(ctx, text_id);
    }
    ctx.memory_mut(|memory| memory.request_focus(text_id));
}

pub fn current_model(app: &RustChamber) -> Option<ModelRef> {
    app.model
        .active
        .as_ref()
        .and_then(|chat| chat.model.clone())
        .or_else(|| app.ui.home_model.clone())
        .or_else(|| app.model.default_model())
}

pub fn current_agent(app: &RustChamber) -> Option<String> {
    app.model
        .active
        .as_ref()
        .and_then(|chat| chat.agent.clone())
        .or_else(|| app.ui.home_agent.clone())
        .or_else(|| {
            app.model
                .primary_agents()
                .first()
                .map(|agent| agent.id.clone())
        })
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modifiers(shift: bool, ctrl: bool, alt: bool, command: bool) -> Modifiers {
        Modifiers {
            alt,
            ctrl,
            shift,
            mac_cmd: false,
            command,
        }
    }

    #[test]
    fn enter_matrix_matches_the_reference() {
        let plain = modifiers(false, false, false, false);
        let shift = modifiers(true, false, false, false);
        let ctrl = modifiers(false, true, false, true);
        let alt = modifiers(false, false, true, false);

        assert!(should_submit_enter(plain, false, false, false));
        assert!(!should_submit_enter(shift, false, false, false));
        assert!(should_submit_enter(ctrl, false, false, false));
        assert!(!should_submit_enter(alt, false, false, false));

        assert!(should_submit_enter(plain, false, true, true));
        assert!(!should_submit_enter(shift, false, true, true));
        assert!(should_submit_enter(ctrl, false, true, true));
        assert!(!should_submit_enter(alt, false, true, true));

        assert!(!should_submit_enter(plain, false, true, false));
        assert!(should_submit_enter(ctrl, false, true, false));

        assert!(!should_submit_enter(plain, true, false, false));
        assert!(should_submit_enter(ctrl, true, false, false));
    }

    #[test]
    fn footer_caps_never_overlap_at_narrow_widths() {
        for available in [160.0f32, 220.0, 300.0, 390.0, 470.0, 630.0, 900.0] {
            for agent in [24.0f32, 57.0, 96.0] {
                for effort in [None, Some(26.0f32), Some(96.0)] {
                    let caps = footer_caps(available, agent, effort);
                    let effort_chrome = if effort.is_some() { EFFORT_CHROME } else { 0.0 };
                    let total = FOOTER_BUTTON
                        + FOOTER_GROUP_GAP * 3.0
                        + caps.agent
                        + AGENT_CHROME
                        + caps.model
                        + MODEL_CHROME
                        + caps.effort
                        + effort_chrome;
                    assert!(
                        total <= available + 0.01,
                        "available {available} agent {agent} effort {effort:?} total {total}"
                    );
                    assert!(caps.model >= 0.0);
                }
            }
        }
    }

    #[test]
    fn footer_caps_truncate_the_model_before_the_effort() {
        let wide = footer_caps(630.0, 33.0, Some(26.0));
        assert_eq!(wide.model, LABEL_MAX_WIDTH);
        assert_eq!(wide.agent, 33.0);
        assert_eq!(wide.effort, 26.0);

        let narrow = footer_caps(470.0, 96.0, Some(96.0));
        assert!(narrow.model >= MODEL_LABEL_MIN - 0.01);
        assert!(narrow.effort < 96.0);
        assert_eq!(narrow.agent, FLIP_LABEL_CAP - AGENT_CHROME);

        let tiny = footer_caps(220.0, 96.0, Some(96.0));
        assert_eq!(tiny.model, MODEL_LABEL_MIN);
        assert!(tiny.agent < FLIP_LABEL_CAP - AGENT_CHROME);
        assert_eq!(tiny.effort, 0.0);
    }

    #[test]
    fn effort_labels_are_capitalized_and_default_is_untinted() {
        assert_eq!(display_effort(None), "Default");
        assert_eq!(display_effort(Some("")), "Default");
        assert_eq!(display_effort(Some("default")), "Default");
        assert_eq!(display_effort(Some("low")), "Low");
        assert_eq!(display_effort(Some("high")), "High");
        assert_eq!(display_effort(Some("max")), "Max");
        assert_eq!(display_effort(Some("xhigh")), "Xhigh");
    }

    #[test]
    fn model_row_slots_leave_room_for_the_thinking_suffix() {
        let row = Rect::from_min_size(Pos2::new(100.0, 200.0), Vec2::new(380.0, MODEL_ROW));
        let thinking_left = row.left() + 27.0 + 120.0 + 8.0;
        let thinking_right = row.right() - MODEL_ROW_TRAILING - 8.0;
        assert!(thinking_right - thinking_left > 40.0);
        let ctx_right = row.right() - MODEL_ROW_TRAILING;
        let check_right =
            row.right() - MODEL_ROW_PAD - MODEL_ROW_STAR - MODEL_ROW_SLOT_GAP - MODEL_ROW_CHECK;
        let star_left = row.right() - MODEL_ROW_PAD - MODEL_ROW_STAR;
        assert!(thinking_right < ctx_right);
        assert!(ctx_right < check_right);
        assert!(check_right <= star_left);
        assert_eq!(favorite_key("anthropic", "claude"), "anthropic/claude");
    }

    #[test]
    fn clicking_below_the_last_line_goes_to_the_end() {
        let editor = Rect::from_min_size(Pos2::new(100.0, 200.0), Vec2::new(400.0, 64.0));
        let text_bottom = 236.0;
        assert!(click_lands_below_text(
            editor,
            text_bottom,
            Some(Pos2::new(120.0, 250.0))
        ));
        assert!(!click_lands_below_text(
            editor,
            text_bottom,
            Some(Pos2::new(120.0, 220.0))
        ));
        assert!(!click_lands_below_text(
            editor,
            text_bottom,
            Some(Pos2::new(120.0, 300.0))
        ));
        assert!(!click_lands_below_text(editor, text_bottom, None));
    }

    #[test]
    fn model_label_elides_when_the_footer_is_tight() {
        egui::__run_test_ui(|ui| {
            let galley = row_galley(
                ui,
                "DeepSeek V4.1 Flash",
                theme::font_medium(12.5),
                Color32::PLACEHOLDER,
                72.0,
            );
            assert_eq!(galley.rows.len(), 1);
            assert!(galley.size().x <= 73.0, "width {}", galley.size().x);
            assert!(galley.elided);
        });
    }

    #[test]
    fn image_paste_needs_an_empty_text_clipboard_and_a_paste_command() {
        let picture = std::sync::Arc::new(egui::ColorImage::filled([1, 1], Color32::RED));
        let paste_image = egui::Event::PasteImage(picture);
        let key = |modifiers| egui::Event::Key {
            key: Key::V,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };

        assert!(paste_events(std::slice::from_ref(&paste_image)).wants_image());
        assert!(paste_events(&[key(Modifiers::COMMAND)]).wants_image());
        assert!(paste_events(&[egui::Event::Paste("text".into())]).text);
        assert!(
            !paste_events(&[egui::Event::Paste("text".into()), paste_image.clone()]).wants_image()
        );
        assert!(!paste_events(&[key(Modifiers::NONE)]).wants_image());
        assert!(!paste_events(&[]).wants_image());
    }

    #[test]
    fn pasted_images_save_as_png_attachments() {
        use rchamber_api::PromptFileSource;

        let ctx = egui::Context::default();
        let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([180, 40, 60, 255]));
        let path = save_pasted_image(&ctx, &image).expect("png file");
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("png")
        );
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        let items = attachments::prepare_path(&path, None).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].file.mime, "image/png");
        assert!(matches!(items[0].file.source, PromptFileSource::Inline));
        assert!(items[0].size > 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn picker_sits_above_the_composer_and_never_overlaps_it() {
        let anchor = CaretAnchor {
            trigger: Rect::from_min_size(Pos2::new(200.0, 600.0), Vec2::new(1.0, 20.0)),
        };
        let composer = Rect::from_min_size(Pos2::new(100.0, 500.0), Vec2::new(800.0, 160.0));
        let placement = place_caret_popup(anchor, composer, 0.0, 560.0);
        assert_eq!(placement.left, 188.0);
        assert_eq!(placement.bottom, composer.top() - POPUP_GAP);
        assert_eq!(placement.width, 560.0);
        assert_eq!(placement.max_height, POPUP_MAX_HEIGHT);

        let tall = Rect::from_min_size(Pos2::new(100.0, 320.0), Vec2::new(800.0, 160.0));
        let placement = place_caret_popup(anchor, tall, 0.0, 560.0);
        assert!(placement.max_height < POPUP_MAX_HEIGHT);
        assert!(placement.max_height >= POPUP_MIN_HEIGHT);
        assert!(placement.bottom - placement.max_height >= 0.0);

        let tight = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(800.0, 160.0));
        let placement = place_caret_popup(anchor, tight, 0.0, 560.0);
        assert_eq!(placement.max_height, tight.top() - POPUP_GAP - POPUP_EDGE);
        assert!(placement.bottom - placement.max_height >= 0.0);

        let clamped = place_caret_popup(anchor, tight, 0.0, 1600.0);
        assert_eq!(clamped.width, composer.width());
        assert_eq!(clamped.left, composer.left());
    }

    fn test_picker(count: usize) -> Picker {
        Picker {
            kind: '@',
            start_byte: 0,
            end_byte: 1,
            query: String::new(),
            items: (0..count)
                .map(|index| PickerItem {
                    value: format!("item-{index}"),
                    label: format!("Item {index}"),
                    detail: String::new(),
                    icon: Icon::Bot,
                    section: "Agents",
                })
                .collect(),
            width: 560.0,
        }
    }

    fn popup_area_rect(picker: &Picker) -> Rect {
        let ctx = egui::Context::default();
        let palette = Palette::dark();
        let composer = Rect::from_min_size(Pos2::new(200.0, 600.0), Vec2::new(720.0, 160.0));
        let anchor = CaretAnchor {
            trigger: Rect::from_min_size(Pos2::new(300.0, 700.0), Vec2::new(1.0, 20.0)),
        };
        let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0));
        for _ in 0..3 {
            ctx.run_pass(egui::RawInput::default(), |ctx| {
                composer_autocomplete_popup(ctx, &palette, picker, anchor, composer, area);
            })
            .drop_without_applying_deltas();
        }
        let rect = ctx
            .memory(|memory| memory.area_rect(Id::unique("composer-autocomplete")))
            .expect("popup area");
        assert!((rect.bottom() - (composer.top() - POPUP_GAP)).abs() <= 3.0);
        assert!(rect.top() >= area.top());
        assert!(rect.width() <= composer.width());
        rect
    }

    #[test]
    fn a_single_item_popup_stays_compact() {
        let ctx = egui::Context::default();
        let context = ctx.clone();
        let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
            let picker = test_picker(1);
            let anchor = CaretAnchor {
                trigger: Rect::from_min_size(Pos2::new(100.0, 500.0), Vec2::new(2.0, 20.0)),
            };
            let composer = Rect::from_min_size(Pos2::new(80.0, 540.0), Vec2::new(600.0, 100.0));
            let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0));
            let _ = composer_autocomplete_popup(
                &context,
                &Palette::dark(),
                &picker,
                anchor,
                composer,
                area,
            );
            let rect = context
                .memory(|memory| memory.area_rect(Id::unique("composer-autocomplete")))
                .expect("popup area");
            assert!(rect.height() < POPUP_MIN_HEIGHT, "height {}", rect.height());
        });
        output.textures_delta.clear();
    }

    #[test]
    fn popup_area_is_anchored_above_the_composer() {
        let rect = popup_area_rect(&test_picker(1));
        assert!(rect.height() < POPUP_MIN_HEIGHT);
    }

    #[test]
    fn popup_area_grows_up_to_its_cap() {
        let rect = popup_area_rect(&test_picker(20));
        assert!(
            rect.height() <= POPUP_MAX_HEIGHT + 1.0,
            "height {}",
            rect.height()
        );
        assert!(
            rect.height() >= POPUP_MIN_HEIGHT,
            "height {}",
            rect.height()
        );
    }

    #[test]
    fn large_text_paste_thresholds() {
        assert!(!is_large_plain_text_paste("hello"));
        assert!(!is_large_plain_text_paste(&" ".repeat(4000)));
        assert!(is_large_plain_text_paste(&"a".repeat(LARGE_PASTE_CHARS)));
        assert!(is_large_plain_text_paste(
            &std::iter::repeat_n("line", LARGE_PASTE_LINES)
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    #[test]
    fn box_is_720_wide_inside_the_768_column() {
        assert_eq!(composer_width(1400.0), 720.0);
        assert_eq!(composer_width(768.0), 736.0);
        assert!(composer_width(600.0) < 600.0);
        assert_eq!(EDITOR_PAD_TOP, 16.0);
        assert_eq!(EDITOR_PAD_BOTTOM, 10.0);
    }

    #[test]
    fn queue_label_elides_inside_its_reserved_width() {
        egui::__run_test_ui(|ui| {
            let galley = row_galley(
                ui,
                &"a very long queued message ".repeat(20),
                theme::font(14.0),
                ui.visuals().text_color(),
                240.0,
            );
            assert_eq!(galley.rows.len(), 1);
            assert!(galley.size().x <= 241.0, "width {}", galley.size().x);
            assert!(galley.elided);
            assert!(xs_button_width(ui, "Edit", Some(Icon::Pencil)) >= 50.0);
            assert_eq!(xs_button_width(ui, "", None), 24.0);
        });
    }

    #[test]
    fn attachment_and_queue_metrics_match_the_reference() {
        assert_eq!(ATTACHMENT_HEIGHT, 28.0);
        assert_eq!(ATTACHMENT_NAME_MAX, 200.0);
        assert_eq!(QUEUE_ROW, 32.0);
        assert_eq!(EDITOR_MIN_HEIGHT, 64.0);
        assert_eq!(FOOTER_BUTTON, 24.0);
        assert_eq!(FOOTER_GLYPH, 16.0);
    }

    #[test]
    fn tokens_are_neutral_chips_at_word_boundaries() {
        let spans = token_spans("look @src/main.rs and $5");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start_byte, 5);
        assert_eq!(
            &"look @src/main.rs and $5"[spans[0].start_byte..spans[0].end_byte],
            "@src/main.rs"
        );
        let spans = token_spans("/usr/bin ");
        assert_eq!(spans.len(), 1);
        assert_eq!(token_spans("/usr/bin").len(), 0);
    }

    #[test]
    fn new_extension_expands_into_a_short_prompt_and_a_guide() {
        let (text, guide) = expand_extension_command("/new-extension a deploy board");
        assert!(text.contains("a deploy board"));
        assert!(text.contains("attached guide"));
        assert!(text.chars().count() < 400);
        let guide = guide.unwrap();
        assert!(guide.uri.starts_with("file://"));
        assert!(guide.uri.contains("SKILL.md"));
        assert_eq!(guide.mime, "text/markdown");
        assert_eq!(
            expand_extension_command("/new-extensionish").0,
            "/new-extensionish"
        );
        assert_eq!(expand_extension_command("hello").0, "hello");
    }
}
