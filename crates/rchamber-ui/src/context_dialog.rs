use std::sync::Arc;

use chrono::{Local, TimeZone};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Pos2, Rect, RichText, ScrollArea, Sense, TextStyle, Ui,
    Vec2, WidgetText,
};
use rchamber_api::models::{Message, Part, TokenUsage, UserMessage};
use rchamber_app::Chat;
use rchamber_app::format::{money, time_label};

use crate::app::RustChamber;
use crate::icons::Icon;
use crate::theme::{self, Palette};

const CARD_RADIUS: u8 = 9;
const ROW_RADIUS: u8 = 6;
const ROW_HEIGHT: f32 = 26.0;
const ROW_GAP: f32 = 6.0;
const CARD_GAP: f32 = 9.0;
const SECTION_GAP: f32 = 14.0;
const TOP_PAD: f32 = 12.0;
const SIDE_PAD: f32 = 12.0;
const BOTTOM_PAD: f32 = 14.0;
const CONTEXT_CARD: f32 = 74.0;
const STAT_CARD: f32 = 56.0;
const TOKENS_CARD: f32 = 108.0;
const BAR_HEIGHT: f32 = 6.0;
const LEGEND_HEIGHT: f32 = 17.0;
const LEGEND_ROW_GAP: f32 = 5.0;
const LEGEND_GAP: f32 = 16.0;
const LEGEND_DOT: f32 = 6.0;
const LEGEND_TEXT_GAP: f32 = 4.0;
const RAW_LABEL_HEIGHT: f32 = 20.0;
const SHORT_CONTENT: f32 = 470.0;
const SHORT_RAW_HEIGHT: f32 = 240.0;

#[derive(Clone, Debug, Default)]
struct Reading {
    used: Option<f64>,
    limit: f64,
    fraction: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Split {
    user: f64,
    assistant: f64,
    tool: f64,
    system: f64,
}

impl Split {
    fn total(&self) -> f64 {
        self.user + self.assistant + self.tool + self.system
    }

    fn percentages(&self) -> [f32; 4] {
        let total = self.total();
        if total <= 0.0 {
            return [0.0; 4];
        }
        let user = (self.user / total * 100.0) as f32;
        let assistant = (self.assistant / total * 100.0) as f32;
        let tool = (self.tool / total * 100.0) as f32;
        let system = (100.0 - user - assistant - tool).clamp(0.0, 100.0);
        [user, assistant, tool, system]
    }
}

#[derive(Clone, Debug)]
struct View {
    meta: String,
    reading: Reading,
    messages: usize,
    users: usize,
    assistants: usize,
    cost: f64,
    tokens: Option<TokenUsage>,
    split: [f32; 4],
    rows: Vec<RawRow>,
}

#[derive(Clone, Debug)]
struct RawRow {
    label: String,
    metrics: String,
    time: String,
}

pub fn show_panel(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let mut close = false;
    let mut toggle_expanded = false;
    let expanded = app.ui.expanded;
    let tabs = [crate::surfaces::PanelTab::new("Context", Icon::Circle).active(true)];
    crate::surfaces::panel_header(
        ui,
        &p,
        &tabs,
        |_| {},
        |ui| {
            if crate::surfaces::close_button(ui, &p).clicked() {
                close = true;
            }
            if crate::surfaces::expand_button(ui, &p, expanded).clicked() {
                toggle_expanded = true;
            }
        },
    );
    if close {
        app.ui.context_open = false;
    }
    if toggle_expanded {
        app.ui.expanded = !app.ui.expanded;
    }

    let Some(chat) = app.model.active.as_ref() else {
        crate::surfaces::empty_state(
            ui,
            &p,
            Icon::Circle,
            "No session open",
            "Open a session to see its context breakdown.",
        );
        return;
    };
    let view = view(app, chat);
    content(ui, &p, &view);
}

fn content(ui: &mut Ui, p: &Palette, view: &View) {
    if ui.available_height() < SHORT_CONTENT {
        ScrollArea::vertical()
            .id_salt("context-surface-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(TOP_PAD);
                ui.horizontal(|ui| {
                    ui.add_space(SIDE_PAD);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - SIDE_PAD).max(120.0));
                        summary(ui, p, view);
                        ui.add_space(SECTION_GAP);
                        raw_messages(ui, p, &view.rows, SHORT_RAW_HEIGHT);
                        ui.add_space(BOTTOM_PAD);
                    });
                });
            });
        return;
    }

    let rect = Rect::from_min_max(
        Pos2::new(ui.cursor().min.x + SIDE_PAD, ui.cursor().min.y + TOP_PAD),
        Pos2::new(
            (ui.max_rect().right() - SIDE_PAD).max(ui.cursor().min.x + SIDE_PAD + 120.0),
            (ui.max_rect().bottom() - BOTTOM_PAD).max(ui.cursor().min.y + TOP_PAD),
        ),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            summary(ui, p, view);
            ui.add_space(SECTION_GAP);
            raw_messages(ui, p, &view.rows, ui.available_height());
        },
    );
}

fn summary(ui: &mut Ui, p: &Palette, view: &View) {
    ui.add(
        egui::Label::new(
            RichText::new(&view.meta)
                .font(theme::font(11.5))
                .color(p.fg_muted),
        )
        .truncate(),
    );
    ui.add_space(8.0);
    context_card(ui, p, &view.reading);
    ui.add_space(CARD_GAP);
    stat_grid(ui, p, view);
    ui.add_space(CARD_GAP);
    tokens_card(ui, p, view);
    ui.add_space(CARD_GAP);
    breakdown(ui, p, &view.split);
}

fn view(app: &RustChamber, chat: &Chat) -> View {
    let reading = reading(app, chat);
    let provider_model = provider_model(app, chat);
    let created = session_created(chat);
    let meta = match (provider_model, created) {
        (Some(model), Some(date)) => format!("{model} · {date}"),
        (Some(model), None) => model,
        (None, Some(date)) => date,
        (None, None) => "—".into(),
    };
    assemble(chat, meta, reading)
}

fn assemble(chat: &Chat, meta: String, reading: Reading) -> View {
    let tokens = latest_tokens(chat).cloned();
    let split = context_split(&chat.messages, reading.used.unwrap_or(0.0));
    let users = chat
        .messages
        .iter()
        .filter(|message| matches!(message, Message::User(_)))
        .count();
    let assistants = chat
        .messages
        .iter()
        .filter(|message| matches!(message, Message::Assistant(_)))
        .count();
    let rows = chat.messages.iter().rev().map(raw_row).collect::<Vec<_>>();

    View {
        meta,
        reading,
        messages: chat.messages.len(),
        users,
        assistants,
        cost: chat.session.cost,
        tokens,
        split: split.percentages(),
        rows,
    }
}

fn reading(app: &RustChamber, chat: &Chat) -> Reading {
    let used = latest_tokens(chat)
        .map(TokenUsage::context_used)
        .filter(|value| *value > 0.0)
        .or_else(|| Some(chat.session.tokens.context_used()).filter(|value| *value > 0.0));
    let limit = app
        .model
        .current_model()
        .and_then(|model| model.context_limit())
        .unwrap_or(200_000) as f64;
    let fraction = used.map(|value| (value / limit.max(1.0)) as f32);
    Reading {
        used,
        limit,
        fraction,
    }
}

fn latest_tokens(chat: &Chat) -> Option<&TokenUsage> {
    chat.messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::Assistant(assistant) => assistant.tokens.as_ref(),
            _ => None,
        })
}

fn provider_model(app: &RustChamber, chat: &Chat) -> Option<String> {
    let reference = chat
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::Assistant(assistant) => assistant.model.as_ref(),
            _ => None,
        })
        .or(chat.model.as_ref())?;
    let model = app.model.model_for(reference);
    let provider = app
        .model
        .providers
        .iter()
        .find(|provider| provider.id == reference.provider_id)
        .map(|provider| provider.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| reference.provider_id.clone());
    let name = model
        .map(|model| model.display_name().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| reference.id.clone());
    Some(format!("{provider} / {name}"))
}

fn session_created(chat: &Chat) -> Option<String> {
    let stamp = if chat.session.time.created > 0.0 {
        chat.session.time.created
    } else {
        chat.messages.iter().find_map(Message::created)?
    };
    let seconds = stamp / 1000.0;
    let secs = seconds.trunc() as i64;
    let nanos = ((seconds - seconds.trunc()) * 1_000_000_000.0) as u32;
    Local
        .timestamp_opt(secs, nanos)
        .single()
        .filter(|_| stamp > 0.0)
        .map(|datetime| datetime.format("%b %-d, %Y, %-I:%M").to_string())
}

fn context_card(ui: &mut Ui, p: &Palette, reading: &Reading) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), CONTEXT_CARD),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::same(CARD_RADIUS), p.card);
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + 12.0, rect.top() + 11.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 10.0),
    );
    ui.painter().text(
        inner.left_top(),
        Align2::LEFT_TOP,
        "Context",
        theme::font_micro(),
        p.fg_muted,
    );
    let readout = match reading.used {
        Some(used) => format!("{} / {}", grouped(used), grouped(reading.limit)),
        None => "—".into(),
    };
    ui.painter().text(
        Pos2::new(inner.right(), inner.top()),
        Align2::RIGHT_TOP,
        readout,
        theme::font_mono(11.5),
        p.fg_muted,
    );

    let bar = Rect::from_min_size(
        Pos2::new(inner.left(), inner.top() + 28.0),
        Vec2::new(inner.width(), BAR_HEIGHT),
    );
    ui.painter()
        .rect_filled(bar, CornerRadius::same(2), p.over(p.card, p.hover));
    if let Some(fraction) = reading.fraction {
        let fraction = fraction.clamp(0.0, 1.0);
        if fraction > 0.0 {
            let filled = Rect::from_min_size(
                bar.min,
                Vec2::new((bar.width() * fraction).max(3.0), bar.height()),
            );
            ui.painter()
                .rect_filled(filled, CornerRadius::same(2), tone(p, fraction * 100.0));
        }
    }
    let percent = match reading.fraction {
        Some(fraction) => format!("{:.1}% used", fraction * 100.0),
        None => "—".into(),
    };
    ui.painter().text(
        Pos2::new(inner.left(), inner.top() + 42.0),
        Align2::LEFT_TOP,
        percent,
        theme::font_micro_medium(),
        p.fg,
    );
}

fn tone(p: &Palette, percent: f32) -> Color32 {
    if percent >= 80.0 {
        p.red
    } else if percent >= 50.0 {
        p.coral
    } else {
        p.green
    }
}

fn stat_grid(ui: &mut Ui, p: &Palette, view: &View) {
    let gap = 8.0;
    let width = ((ui.available_width() - gap) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        stat_card(ui, p, width, "Messages", &grouped(view.messages as f64));
        stat_card(ui, p, width, "User", &grouped(view.users as f64));
    });
    ui.add_space(gap);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        stat_card(ui, p, width, "Assistant", &grouped(view.assistants as f64));
        stat_card(ui, p, width, "Cost", &money(view.cost));
    });
}

fn stat_card(ui: &mut Ui, p: &Palette, width: f32, label: &str, value: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, STAT_CARD), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(CARD_RADIUS), p.card);
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + 12.0, rect.top() + 11.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 10.0),
    );
    ui.painter().text(
        inner.left_top(),
        Align2::LEFT_TOP,
        label,
        theme::font_micro(),
        p.fg_muted,
    );
    ui.painter().text(
        inner.left_bottom(),
        Align2::LEFT_BOTTOM,
        value,
        theme::font_mono(13.0),
        p.fg,
    );
}

fn tokens_card(ui: &mut Ui, p: &Palette, view: &View) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), TOKENS_CARD), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(CARD_RADIUS), p.card);
    ui.painter().text(
        Pos2::new(rect.left() + 12.0, rect.top() + 11.0),
        Align2::LEFT_TOP,
        "Last assistant message",
        theme::font_micro(),
        p.fg_muted,
    );

    let tokens = view.tokens.as_ref();
    let toggled = |value: f64| {
        if tokens.is_some() {
            grouped(value)
        } else {
            "—".into()
        }
    };
    let input = toggled(tokens.map(|tokens| tokens.input).unwrap_or(0.0));
    let output = toggled(tokens.map(|tokens| tokens.output).unwrap_or(0.0));
    let reasoning = toggled(tokens.map(|tokens| tokens.reasoning).unwrap_or(0.0));
    let cache_read = toggled(tokens.map(|tokens| tokens.cache.read).unwrap_or(0.0));
    let cache_write = toggled(tokens.map(|tokens| tokens.cache.write).unwrap_or(0.0));
    let cache_hit = tokens
        .and_then(cache_hit)
        .map(|hit| format!("{:.1}%", hit * 100.0))
        .unwrap_or_else(|| "—".into());
    let cells = [
        ("Input", input),
        ("Output", output),
        ("Reasoning", reasoning),
        ("Cache read", cache_read),
        ("Cache write", cache_write),
        ("Cache hit", cache_hit),
    ];

    let gap = 10.0;
    let cell_width = ((rect.width() - 24.0 - gap * 2.0) / 3.0).max(0.0);
    for (index, (label, value)) in cells.iter().enumerate() {
        let column = index % 3;
        let row = index / 3;
        let x = rect.left() + 12.0 + column as f32 * (cell_width + gap);
        let y = rect.top() + 35.0 + row as f32 * 33.0;
        ui.painter().text(
            Pos2::new(x, y),
            Align2::LEFT_TOP,
            *label,
            theme::font(10.5),
            p.fg_muted,
        );
        ui.painter().text(
            Pos2::new(x, y + 17.0),
            Align2::LEFT_TOP,
            value,
            theme::font_mono(12.0),
            p.fg,
        );
    }
}

fn cache_hit(tokens: &TokenUsage) -> Option<f64> {
    let window = tokens.input + tokens.cache.read + tokens.cache.write;
    (window > 0.0).then(|| tokens.cache.read / window)
}

fn breakdown(ui: &mut Ui, p: &Palette, split: &[f32; 4]) {
    let width = ui.available_width();
    let labels = ["User", "Assistant", "Tool calls", "System & tools"];
    let percentages = display_percentages(split);
    let mut galleys = Vec::with_capacity(labels.len());
    let mut widths = Vec::with_capacity(labels.len());
    for (index, label) in labels.iter().enumerate() {
        let text = format!("{label} {}%", percentages[index]);
        let galley = ui
            .painter()
            .layout_no_wrap(text, theme::font(11.5), p.fg_muted);
        widths.push(LEGEND_DOT + LEGEND_TEXT_GAP + galley.size().x);
        galleys.push(galley);
    }
    let rows = pack_rows(&widths, width, LEGEND_GAP);
    let row_count = rows.len().max(1) as f32;
    let height = BAR_HEIGHT + 10.0 + row_count * LEGEND_HEIGHT + (row_count - 1.0) * LEGEND_ROW_GAP;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), BAR_HEIGHT));
    ui.painter()
        .rect_filled(bar, CornerRadius::same(3), p.over(p.bg, p.hover));

    let colors = [p.green, p.primary, p.coral, p.fg_faint];
    let painter = ui.painter().with_clip_rect(bar);
    let segments = split
        .iter()
        .enumerate()
        .filter(|(_, percent)| **percent > 0.0)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut x = bar.left();
    for (position, index) in segments.iter().enumerate() {
        let segment_width = bar.width() * (split[*index] / 100.0);
        if segment_width <= 0.0 {
            continue;
        }
        let radius = if segments.len() == 1 {
            CornerRadius::same(3)
        } else if position == 0 {
            CornerRadius {
                nw: 3,
                sw: 3,
                ne: 0,
                se: 0,
            }
        } else if position + 1 == segments.len() {
            CornerRadius {
                nw: 0,
                sw: 0,
                ne: 3,
                se: 3,
            }
        } else {
            CornerRadius::ZERO
        };
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(x, bar.top()),
                Vec2::new(segment_width, bar.height()),
            ),
            radius,
            colors[*index],
        );
        x += segment_width;
    }

    let mut y = rect.top() + BAR_HEIGHT + 10.0;
    for row in &rows {
        let mut cursor = rect.left();
        for index in row {
            let galley = &galleys[*index];
            let dot =
                Rect::from_center_size(Pos2::new(cursor + 3.0, y + 7.0), Vec2::splat(LEGEND_DOT));
            ui.painter()
                .circle_filled(dot.center(), 3.0, colors[*index]);
            ui.painter().galley(
                Pos2::new(
                    cursor + LEGEND_DOT + LEGEND_TEXT_GAP,
                    y + (LEGEND_HEIGHT - galley.size().y) * 0.5,
                ),
                galley.clone(),
                p.fg_muted,
            );
            cursor += widths[*index] + LEGEND_GAP;
        }
        y += LEGEND_HEIGHT + LEGEND_ROW_GAP;
    }
}

fn display_percentages(split: &[f32; 4]) -> [u32; 4] {
    let total: f32 = split.iter().sum();
    if total <= 0.0 {
        return [0; 4];
    }
    let scaled = split.map(|value| value / total * 100.0);
    let mut shown = scaled.map(|value| value.floor() as u32);
    let assigned: u32 = shown.iter().sum();
    let mut remainders = scaled
        .iter()
        .enumerate()
        .map(|(index, value)| (index, value.fract()))
        .collect::<Vec<_>>();
    remainders.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (index, _) in remainders
        .into_iter()
        .take(100usize.saturating_sub(assigned as usize))
    {
        shown[index] += 1;
    }
    shown
}

fn pack_rows(widths: &[f32], available: f32, gap: f32) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut row: Vec<usize> = Vec::new();
    let mut used = 0.0f32;
    for (index, width) in widths.iter().enumerate() {
        let extra = if row.is_empty() { *width } else { gap + *width };
        if !row.is_empty() && used + extra > available + 0.01 {
            rows.push(std::mem::take(&mut row));
            used = *width;
        } else {
            used += extra;
        }
        row.push(index);
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn raw_messages(ui: &mut Ui, p: &Palette, rows: &[RawRow], height: f32) {
    if height < RAW_LABEL_HEIGHT {
        return;
    }
    ui.painter().text(
        ui.cursor().min,
        Align2::LEFT_TOP,
        "Raw messages",
        theme::font_micro(),
        p.fg_muted,
    );
    ui.add_space(RAW_LABEL_HEIGHT);
    let stride = ROW_HEIGHT + ROW_GAP;
    let available = height - RAW_LABEL_HEIGHT;
    let full_rows = ((available + ROW_GAP) / stride).floor();
    let list_height = full_rows * stride - ROW_GAP;
    if full_rows < 1.0 {
        return;
    }
    ScrollArea::vertical()
        .id_salt("context-dialog-raw")
        .max_height(list_height)
        .min_scrolled_height(list_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            for row in rows {
                paint_raw_row(ui, p, row);
            }
        });
}

fn paint_raw_row(ui: &mut Ui, p: &Palette, row: &RawRow) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(ROW_RADIUS), p.card);

    let time_galley =
        ui.painter()
            .layout_no_wrap(row.time.clone(), theme::font_mono(11.0), p.fg_muted);
    let time_x = rect.right() - 10.0 - time_galley.size().x;
    ui.painter().galley(
        Pos2::new(time_x, rect.center().y - time_galley.size().y * 0.5),
        time_galley,
        p.fg_muted,
    );
    let mut label_right = time_x - 12.0;
    if !row.metrics.is_empty() {
        let galley =
            ui.painter()
                .layout_no_wrap(row.metrics.clone(), theme::font_mono(11.0), p.fg_muted);
        let metrics_x = label_right - galley.size().x;
        ui.painter().galley(
            Pos2::new(metrics_x, rect.center().y - galley.size().y * 0.5),
            galley,
            p.fg_muted,
        );
        label_right = metrics_x - 12.0;
    }
    let label_width = (label_right - rect.left() - 10.0).max(24.0);
    let galley = elide(ui, &row.label, theme::font(11.5), p.fg_muted, label_width);
    ui.painter().galley(
        Pos2::new(rect.left() + 10.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg_muted,
    );
}

fn raw_row(message: &Message) -> RawRow {
    let metrics = match message {
        Message::Assistant(assistant) => assistant
            .tokens
            .as_ref()
            .map(|tokens| format!("{} / {}", grouped(tokens.input), grouped(tokens.output)))
            .unwrap_or_default(),
        _ => String::new(),
    };
    RawRow {
        label: message_label(message),
        metrics,
        time: message
            .created()
            .filter(|stamp| *stamp > 0.0)
            .map(time_label)
            .unwrap_or_default(),
    }
}

fn message_label(message: &Message) -> String {
    match message {
        Message::User(user) => format!("user: {}", user_snippet(user)),
        Message::Assistant(assistant) => {
            let label = parts_label(&assistant.content);
            if label.is_empty() {
                "—".into()
            } else {
                label
            }
        }
        Message::Synthetic(_) => "synthetic".into(),
        Message::System(_) => "system".into(),
        Message::Shell(_) => "shell".into(),
        Message::Skill(_) => "skill".into(),
        Message::Compaction(_) => "compaction".into(),
        Message::Idle(_) => "idle".into(),
        Message::AgentSwitched(_) => "agent".into(),
        Message::ModelSwitched(_) => "model".into(),
        Message::LocationSwitched(_) => "location".into(),
        Message::Unknown => "unknown".into(),
    }
}

fn user_snippet(user: &UserMessage) -> String {
    let text = user.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let limit = 56;
    let mut snippet = text.chars().take(limit).collect::<String>();
    if text.chars().count() > limit {
        snippet.push('…');
    }
    if snippet.is_empty() {
        let count = user.files.len();
        if count == 0 {
            return "—".into();
        }
        return format!("{count} attachment{}", if count == 1 { "" } else { "s" });
    }
    snippet
}

fn parts_label(parts: &[Part]) -> String {
    let mut labels: Vec<String> = Vec::new();
    for part in parts {
        let label = match part {
            Part::Text(text) if !text.text.trim().is_empty() => "text".to_string(),
            Part::Reasoning(reasoning) if !reasoning.text.trim().is_empty() => {
                "reasoning".to_string()
            }
            Part::Tool(tool) => {
                let name = tool.name.trim().to_lowercase();
                if name.is_empty() {
                    "tool".to_string()
                } else {
                    name
                }
            }
            _ => continue,
        };
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    labels.join(" + ")
}

fn context_split(messages: &[Message], context_used: f64) -> Split {
    let mut user = 0.0;
    let mut assistant = 0.0;
    let mut tool = 0.0;
    for message in messages {
        match message {
            Message::User(message) => {
                user += estimate_tokens(message.text.chars().count())
                    + message
                        .files
                        .iter()
                        .map(|file| {
                            file.name
                                .as_deref()
                                .or(file.path.as_deref())
                                .map(|value| estimate_tokens(value.chars().count()))
                                .unwrap_or(0.0)
                        })
                        .sum::<f64>();
            }
            Message::Assistant(message) => {
                for part in &message.content {
                    match part {
                        Part::Text(text) => assistant += estimate_tokens(text.text.chars().count()),
                        Part::Reasoning(text) => {
                            assistant += estimate_tokens(text.text.chars().count())
                        }
                        Part::Tool(call) => {
                            let input = call.state.input().map(value_chars).unwrap_or(0);
                            let output = call.state.output_text().chars().count();
                            tool += estimate_tokens(input + output);
                        }
                        Part::Unknown => {}
                    }
                }
            }
            Message::Shell(shell) => {
                let output = shell
                    .output
                    .as_ref()
                    .map(|output| output.output.chars().count())
                    .unwrap_or(0);
                tool += estimate_tokens(shell.command.chars().count() + output);
            }
            _ => {}
        }
    }
    let estimated = user + assistant + tool;
    let total = context_used.max(estimated);
    Split {
        user,
        assistant,
        tool,
        system: (total - estimated).max(0.0),
    }
}

fn estimate_tokens(chars: usize) -> f64 {
    (chars as f64 / 4.0).ceil()
}

fn value_chars(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(text) => text.chars().count(),
        serde_json::Value::Array(items) => items.iter().map(value_chars).sum(),
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, value)| key.chars().count() + value_chars(value))
            .sum(),
        serde_json::Value::Number(number) => number.to_string().chars().count(),
        serde_json::Value::Bool(flag) => usize::from(*flag),
        serde_json::Value::Null => 0,
    }
}

fn grouped(value: f64) -> String {
    let rounded = value.max(0.0).round() as i64;
    let digits = rounded.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push('.');
        }
        out.push(character);
    }
    out
}

fn elide(ui: &Ui, text: &str, font: egui::FontId, color: Color32, width: f32) -> Arc<egui::Galley> {
    WidgetText::from(RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        width,
        TextStyle::Body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rchamber_api::models::{AssistantMessage, MessageTime, ToolPart, ToolState};

    fn assistant(id: &str, parts: Vec<Part>, tokens: Option<TokenUsage>) -> Message {
        Message::Assistant(Box::new(AssistantMessage {
            id: id.into(),
            time: MessageTime::default(),
            agent: None,
            model: None,
            content: parts,
            snapshot: None,
            finish: None,
            raw_finish: None,
            cost: 0.0,
            tokens,
            error: None,
            retry: None,
        }))
    }

    fn user(text: &str) -> Message {
        Message::User(Box::new(UserMessage {
            id: "u1".into(),
            time: MessageTime::default(),
            text: text.into(),
            files: Vec::new(),
            agents: Vec::new(),
            skills: Vec::new(),
            metadata: None,
        }))
    }

    fn text_part(text: &str) -> Part {
        Part::Text(rchamber_api::models::TextPart {
            text: text.into(),
            native: None,
        })
    }

    fn tool_part(name: &str, input: &str, output: &str) -> Part {
        Part::Tool(Box::new(ToolPart {
            id: "t1".into(),
            name: name.into(),
            executed: true,
            state: ToolState::Completed(rchamber_api::models::CompletedTool {
                input: serde_json::json!({ "command": input }),
                content: vec![rchamber_api::models::ToolContent::Text {
                    text: output.into(),
                }],
                metadata: None,
            }),
            time: Default::default(),
            native: None,
        }))
    }

    #[test]
    fn percentages_are_a_share_of_the_window() {
        let split = context_split(&[user("aaaa")], 1000.0);
        let percentages = split.percentages();
        let sum: f32 = percentages.iter().sum();
        assert!((sum - 100.0).abs() < 0.01);
        assert!(percentages[3] > 99.0);
    }

    #[test]
    fn estimate_becomes_its_own_denominator_when_it_exceeds_the_window() {
        let split = context_split(
            &[
                user(&"a".repeat(400)),
                assistant("a1", vec![text_part(&"b".repeat(400))], None),
            ],
            10.0,
        );
        assert_eq!(split.user, 100.0);
        assert_eq!(split.assistant, 100.0);
        assert_eq!(split.system, 0.0);
        let percentages = split.percentages();
        assert!((percentages[0] - 50.0).abs() < 0.01);
        assert!((percentages[1] - 50.0).abs() < 0.01);
        assert!((percentages.iter().sum::<f32>() - 100.0).abs() < 0.01);
    }

    #[test]
    fn tool_payloads_land_in_the_tool_bucket() {
        let split = context_split(
            &[assistant(
                "a1",
                vec![tool_part("bash", "echo hi", "hi")],
                None,
            )],
            0.0,
        );
        assert_eq!(split.tool, estimate_tokens(7 + 7 + 2));
        assert_eq!(split.assistant, 0.0);
        assert_eq!(split.system, 0.0);
    }

    #[test]
    fn split_normalizes_to_one_hundred() {
        let split = Split {
            user: 1.0,
            assistant: 2.0,
            tool: 3.0,
            system: 4.0,
        };
        let percentages = split.percentages();
        assert!((percentages.iter().sum::<f32>() - 100.0).abs() < 0.01);
        assert!((percentages[0] - 10.0).abs() < 0.01);
        assert!((percentages[3] - 40.0).abs() < 0.01);
    }

    #[test]
    fn empty_split_has_no_share() {
        assert_eq!(Split::default().percentages(), [0.0; 4]);
    }

    #[test]
    fn cache_hit_is_the_cached_share_of_input() {
        let tokens = TokenUsage {
            input: 25.0,
            output: 0.0,
            reasoning: 0.0,
            cache: rchamber_api::models::TokenCache {
                read: 75.0,
                write: 0.0,
            },
        };
        assert!((cache_hit(&tokens).unwrap() - 0.75).abs() < 1e-6);
        assert!(cache_hit(&TokenUsage::default()).is_none());
    }

    #[test]
    fn grouped_numbers_use_dots_every_three_digits() {
        assert_eq!(grouped(539_798.0), "539.798");
        assert_eq!(grouped(1_000_000.0), "1.000.000");
        assert_eq!(grouped(999.0), "999");
        assert_eq!(grouped(1_000.0), "1.000");
        assert_eq!(grouped(0.0), "0");
    }

    #[test]
    fn assistant_rows_list_each_part_once() {
        let message = assistant(
            "a1",
            vec![
                Part::Tool(Box::new(ToolPart {
                    id: "t1".into(),
                    name: "bash".into(),
                    executed: true,
                    state: ToolState::Unknown,
                    time: Default::default(),
                    native: None,
                })),
                text_part("x"),
            ],
            None,
        );
        let row = raw_row(&message);
        assert_eq!(row.label, "bash + text");
        assert!(row.metrics.is_empty());
    }

    #[test]
    fn user_rows_carry_a_snippet() {
        let row = raw_row(&user("  hello   there "));
        assert_eq!(row.label, "user: hello there");
    }

    fn empty_chat() -> Chat {
        let session: rchamber_api::models::Session = serde_json::from_value(serde_json::json!({
            "id": "ses_empty",
            "projectID": "project",
            "time": {}
        }))
        .expect("empty session");
        Chat::new(session)
    }

    fn render_content(view: &View, width: f32) {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 800.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                content(ui, &p, view);
            });
        });
        output.textures_delta.clear();
    }

    #[test]
    fn empty_session_view_has_zero_counts_and_renders() {
        let chat = empty_chat();
        let reading = Reading {
            used: None,
            limit: 200_000.0,
            fraction: None,
        };
        let view = assemble(&chat, "—".into(), reading);
        assert_eq!(view.messages, 0);
        assert_eq!(view.users, 0);
        assert_eq!(view.assistants, 0);
        assert_eq!(view.cost, 0.0);
        assert!(view.tokens.is_none());
        assert!(view.rows.is_empty());
        assert_eq!(view.split, [0.0; 4]);
        assert_eq!(display_percentages(&view.split), [0; 4]);
        for width in [320.0, 420.0, 720.0] {
            render_content(&view, width);
        }
    }

    #[test]
    fn populated_view_renders_at_panel_widths() {
        let mut chat = empty_chat();
        chat.session.cost = 0.42;
        chat.messages.push(user(&"a".repeat(400)));
        chat.messages.push(assistant(
            "a1",
            vec![
                text_part(&"b".repeat(800)),
                tool_part("bash", "cargo test", "ok"),
            ],
            Some(TokenUsage {
                input: 12_345.0,
                output: 678.0,
                reasoning: 90.0,
                cache: rchamber_api::models::TokenCache {
                    read: 9_000.0,
                    write: 120.0,
                },
            }),
        ));
        let reading = Reading {
            used: Some(54_321.0),
            limit: 200_000.0,
            fraction: Some(0.2716),
        };
        let view = assemble(
            &chat,
            "Provider / Model · Nov 4, 2026, 8:05".into(),
            reading,
        );
        assert_eq!(view.messages, 2);
        assert_eq!(view.users, 1);
        assert_eq!(view.assistants, 1);
        assert_eq!(display_percentages(&view.split).iter().sum::<u32>(), 100);
        for width in [320.0, 420.0, 720.0] {
            render_content(&view, width);
        }
    }

    #[test]
    fn split_percentages_sum_to_one_hundred_at_panel_widths() {
        let messages = [
            user(&"a".repeat(160)),
            assistant(
                "a1",
                vec![
                    text_part(&"b".repeat(120)),
                    tool_part("bash", "echo hi", "hi"),
                ],
                None,
            ),
        ];
        let percentages = context_split(&messages, 10_000.0).percentages();
        let shown = display_percentages(&percentages);
        assert_eq!(shown.iter().sum::<u32>(), 100);

        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let labels = ["User", "Assistant", "Tool calls", "System & tools"];
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 800.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            for width in [320.0f32, 420.0, 720.0] {
                let column = (width - 38.0).max(120.0);
                let widths = labels
                    .iter()
                    .enumerate()
                    .map(|(index, label)| {
                        let text = format!("{label} {}%", shown[index]);
                        let galley = ui.painter().layout_no_wrap(
                            text,
                            theme::font(11.5),
                            Color32::PLACEHOLDER,
                        );
                        LEGEND_DOT + LEGEND_TEXT_GAP + galley.size().x
                    })
                    .collect::<Vec<_>>();
                let rows = pack_rows(&widths, column, LEGEND_GAP);
                assert_eq!(rows.iter().map(Vec::len).sum::<usize>(), 4);
                for row in &rows {
                    let total = row.iter().map(|index| widths[*index]).sum::<f32>()
                        + LEGEND_GAP * (row.len() as f32 - 1.0);
                    assert!(total <= column + 0.01, "width {width}: {total} > {column}");
                }
                assert_eq!(
                    display_percentages(&percentages).iter().sum::<u32>(),
                    100,
                    "width {width}"
                );
            }
        });
        output.textures_delta.clear();
    }
}
