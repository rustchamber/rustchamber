use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, EventFilter, Id, Key, Layout, Margin, Modifiers,
    Pos2, Rect, RichText, ScrollArea, Sense, Stroke, Ui, UiBuilder, Vec2,
};
use rchamber_app::format::relative_time;
use rchamber_app::{Action, Connection};

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::motion;
use crate::settings;
use crate::surfaces::Surface;
use crate::theme::{self, Palette};

const PANEL_MAX_WIDTH: f32 = 720.0;
const PANEL_MIN_WIDTH: f32 = 200.0;
const PANEL_EDGE_GAP: f32 = 16.0;
const TOP_RATIO: f32 = 0.12;
const INPUT_HEIGHT: f32 = 32.0;
const LIST_MAX_HEIGHT: f32 = 440.0;
const LIST_HEIGHT_RATIO: f32 = 0.6;
const ITEM_HEIGHT: f32 = 30.0;
const ITEM_RADIUS: u8 = 9;
const ITEM_PAD_X: f32 = 8.0;
const GROUP_PAD_X: f32 = 8.0;
const GROUP_PAD_Y: f32 = 4.0;
const GROUP_HEADING_HEIGHT: f32 = 28.0;
const FOOTER_HEIGHT: f32 = 29.0;
const EMPTY_HEIGHT: f32 = 66.0;
const MOTION_SECONDS: f32 = motion::FAST;
const SCALE_FROM: f32 = 0.98;
const SESSION_LIMIT: usize = 5;
const RESULT_LIMIT: usize = 7;
const REOPEN_AFTER: f64 = 0.25;

#[derive(Clone, Debug)]
enum Command {
    NewSession,
    Settings,
    SettingsPage(usize),
    ToggleTheme,
    ToggleSidebar,
    ToggleTerminal,
    OpenFolder,
    Reconnect,
    Home,
    OpenSession(String),
    OpenProject(String),
}

#[derive(Clone, Debug)]
struct Entry {
    label: String,
    detail: String,
    icon: Icon,
    shortcut: Option<String>,
    command: Command,
}

#[derive(Clone, Debug)]
struct Group {
    title: &'static str,
    score: u32,
    items: Vec<Entry>,
}

#[derive(Clone)]
struct State {
    open: bool,
    opened_at: f64,
    closed_at: Option<f64>,
    last_seen: f64,
    focus_taken: bool,
    prev_focus: Option<Id>,
    query: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            opened_at: 0.0,
            closed_at: None,
            last_seen: 0.0,
            focus_taken: false,
            prev_focus: None,
            query: String::new(),
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut RustChamber) {
    let now = ctx.input(|input| input.time);
    let mut state = ctx
        .data(|data| data.get_temp::<State>(state_id()))
        .unwrap_or_default();
    if !state.open || now - state.last_seen > REOPEN_AFTER {
        state = State {
            open: true,
            opened_at: now,
            closed_at: None,
            last_seen: now,
            focus_taken: false,
            prev_focus: ctx.memory(|memory| memory.focused()),
            query: String::new(),
        };
        app.ui.palette_index = 0;
    }
    state.last_seen = now;

    let closing = state.closed_at.is_some();
    let duration = motion::duration_for(ctx, MOTION_SECONDS);
    let t = presence(now, state.opened_at, state.closed_at, duration);
    if t < 1.0 {
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    let mut reset_scroll = false;
    if state.query != app.ui.palette_query {
        state.query = app.ui.palette_query.clone();
        app.ui.palette_index = 0;
        reset_scroll = true;
    }

    let p = app.palette.clone();
    let screen = ctx.content_rect();
    let width = (screen.width() - PANEL_EDGE_GAP * 2.0)
        .clamp(PANEL_MIN_WIDTH.min(screen.width()), PANEL_MAX_WIDTH)
        .min(screen.width());
    let list_height = (screen.height() * LIST_HEIGHT_RATIO).min(LIST_MAX_HEIGHT);
    let top = (screen.height() * TOP_RATIO).max(PANEL_EDGE_GAP);
    let height = INPUT_HEIGHT + list_height + FOOTER_HEIGHT + theme::HAIRLINE;

    let groups = groups(app, &app.ui.palette_query);
    let total: usize = groups.iter().map(|group| group.items.len()).sum();
    if total == 0 {
        app.ui.palette_index = 0;
    } else if app.ui.palette_index >= total {
        app.ui.palette_index = total - 1;
    }

    let mut moved = false;
    let mut run: Option<Command> = None;
    let mut close_requested = false;

    if !closing {
        let down = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown));
        let up = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp));
        let home = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Home));
        let end = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::End));
        let next = ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::N))
            || ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::N));
        let previous = ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::P))
            || ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::P));
        let enter = ctx.input(|input| input.key_pressed(Key::Enter));
        if down || next {
            app.ui.palette_index = wrap(app.ui.palette_index, 1, total);
            moved = true;
        }
        if up || previous {
            app.ui.palette_index = wrap(app.ui.palette_index, -1, total);
            moved = true;
        }
        if home && total > 0 {
            app.ui.palette_index = 0;
            moved = true;
        }
        if end && total > 0 {
            app.ui.palette_index = total - 1;
            moved = true;
        }
        if enter && let Some(entry) = entry_at(&groups, app.ui.palette_index) {
            run = Some(entry.command.clone());
        }
    }

    let shadow = egui::epaint::Shadow {
        offset: [0, 18],
        blur: 48,
        spread: 0,
        color: p.shadow,
    };
    let backdrop = scale_alpha(p.overlay, 0.6 * t);
    if backdrop.a() > 0 {
        egui::Area::new(Id::unique("palette/backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .interactable(false)
            .show(ctx, |ui| {
                ui.painter().rect_filled(screen, 0.0, backdrop);
            });
    }
    let reveal = moved || reset_scroll || state.opened_at == now;

    let modal = egui::Modal::new(Id::unique("palette"))
        .area(
            egui::Modal::default_area(Id::unique("palette"))
                .anchor(Align2::CENTER_TOP, Vec2::new(0.0, top)),
        )
        .frame(egui::Frame::NONE)
        .backdrop_color(Color32::TRANSPARENT)
        .show(ctx, |ui| {
            ui.set_opacity(if closing { t } else { 1.0 });
            let (card, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
            paint_card(ui, card, &p, &shadow);

            let mut body = ui.new_child(
                UiBuilder::new()
                    .max_rect(card)
                    .layout(Layout::top_down(Align::Min)),
            );
            body.spacing_mut().item_spacing = Vec2::ZERO;
            body.set_width(width);

            let field_id = Id::unique("palette/query");
            let (header, header_response) =
                body.allocate_exact_size(Vec2::new(width, INPUT_HEIGHT), Sense::click());
            icons::paint(
                &body,
                Icon::Search,
                Rect::from_center_size(
                    Pos2::new(header.left() + 20.0, header.center().y),
                    Vec2::splat(16.0),
                ),
                scale_alpha(p.fg_muted, 0.5),
            );
            if !state.focus_taken {
                ctx.memory_mut(|memory| memory.request_focus(field_id));
                state.focus_taken = true;
            }
            let text_rect = Rect::from_min_max(
                Pos2::new(header.left() + 36.0, header.top() + 4.0),
                Pos2::new(header.right() - 12.0, header.bottom() - 4.0),
            );
            let mut field = body.new_child(
                UiBuilder::new()
                    .max_rect(text_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            let edit = egui::TextEdit::singleline(&mut app.ui.palette_query)
                .id(field_id)
                .frame(egui::Frame::NONE)
                .margin(Margin::ZERO)
                .hint_text(
                    RichText::new("Search sessions, commands, settings…")
                        .font(theme::font(theme::TEXT_META))
                        .color(p.fg_muted),
                )
                .text_color(p.fg)
                .font(theme::font(theme::TEXT_META))
                .desired_width(f32::INFINITY);
            let edit_response = field.add(edit);
            if header_response.clicked() {
                edit_response.request_focus();
            }
            ctx.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    field_id,
                    EventFilter {
                        tab: false,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                )
            });
            painter_hline(
                &body,
                header.bottom(),
                header.left(),
                header.right(),
                p.border,
            );

            let list_width = body.available_width();
            ScrollArea::vertical()
                .id_salt("palette/list")
                .max_height(list_height)
                .min_scrolled_height(list_height)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(&mut body, |ui| {
                    ui.set_width(list_width);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    if groups.is_empty() {
                        empty_state(ui, &p);
                    } else {
                        ui.add_space(GROUP_PAD_Y);
                        let mut index = 0;
                        for group in &groups {
                            group_heading(ui, &p, group.title);
                            for entry in &group.items {
                                let selected = index == app.ui.palette_index;
                                let response = entry_row(ui, &p, entry, selected);
                                if selected && reveal {
                                    ui.scroll_to_rect_animation(
                                        response.rect,
                                        None,
                                        egui::style::ScrollAnimation::none(),
                                    );
                                }
                                if !closing
                                    && response.hovered()
                                    && ctx.input(|input| input.pointer.delta() != Vec2::ZERO)
                                    && !selected
                                {
                                    app.ui.palette_index = index;
                                }
                                if !closing && response.clicked() {
                                    run = Some(entry.command.clone());
                                }
                                index += 1;
                            }
                        }
                        ui.add_space(GROUP_PAD_Y);
                    }
                });

            let cursor = body.cursor();
            painter_hline(&body, cursor.top(), cursor.left(), cursor.right(), p.border);
            body.add_space(theme::HAIRLINE);
            body.allocate_ui_with_layout(
                Vec2::new(width, FOOTER_HEIGHT),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add_space(12.0);
                    footer_hint(ui, &p, "↑↓", "to navigate");
                    ui.add_space(16.0);
                    footer_hint(ui, &p, "↵", "to open");
                    ui.add_space(16.0);
                    footer_hint(ui, &p, "esc", "to close");
                },
            );
        });

    let layer = modal.response.layer_id;
    let scale = SCALE_FROM + (1.0 - SCALE_FROM) * t;
    if t < 0.999 && !motion::reduced_motion(ctx) {
        let center = modal.response.rect.center().to_vec2();
        ctx.set_transform_layer(
            layer,
            egui::emath::TSTransform::new(center * (1.0 - scale), scale),
        );
    } else {
        ctx.set_transform_layer(layer, egui::emath::TSTransform::IDENTITY);
    }

    if !closing && modal.should_close() {
        close_requested = true;
    }
    if let Some(command) = run {
        execute(app, command, ctx);
        close_requested = true;
    }
    if close_requested && !closing {
        state.closed_at = Some(now);
        ctx.request_repaint();
    }
    if closing && t <= 0.0001 {
        if !app.ui.show_settings
            && let Some(previous) = state.prev_focus
        {
            ctx.memory_mut(|memory| memory.request_focus(previous));
        }
        app.ui.palette_open = false;
        state = State::default();
    }
    ctx.data_mut(|data| data.insert_temp(state_id(), state));
}

fn state_id() -> Id {
    Id::unique("palette/state")
}

fn paint_card(ui: &Ui, rect: Rect, p: &Palette, shadow: &egui::epaint::Shadow) {
    let corner = CornerRadius::same(theme::radius::XL);
    ui.painter()
        .add(egui::Shape::Rect(shadow.as_shape(rect, corner)));
    ui.painter().rect_filled(rect, corner, p.bg);
}

fn presence(now: f64, opened_at: f64, closed_at: Option<f64>, duration: f32) -> f32 {
    let duration = (duration as f64).max(0.001);
    let (from, elapsed) = match closed_at {
        Some(closed) => (1.0, now - closed),
        None => (0.0, now - opened_at),
    };
    let progress = (elapsed / duration).clamp(0.0, 1.0) as f32;
    let eased = motion::Easing::EaseOut.apply(progress);
    (from + (1.0 - 2.0 * from) * eased).clamp(0.0, 1.0)
}

fn scale_alpha(color: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * factor.clamp(0.0, 1.0)) as u8)
}

fn wrap(index: usize, delta: i32, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        (index as i32 + delta).rem_euclid(total as i32) as usize
    }
}

fn entry_at(groups: &[Group], index: usize) -> Option<&Entry> {
    let mut remaining = index;
    for group in groups {
        if remaining < group.items.len() {
            return group.items.get(remaining);
        }
        remaining -= group.items.len();
    }
    None
}

fn painter_hline(ui: &Ui, y: f32, left: f32, right: f32, color: Color32) {
    ui.painter()
        .hline(left..=right, y, Stroke::new(theme::BORDER, color));
}

fn footer_hint(ui: &mut Ui, p: &Palette, keys: &str, label: &str) {
    let key = ui.painter().layout_no_wrap(
        keys.to_string(),
        theme::font(theme::TEXT_MICRO),
        Color32::PLACEHOLDER,
    );
    let text = ui.painter().layout_no_wrap(
        label.to_string(),
        theme::font(theme::TEXT_MICRO),
        Color32::PLACEHOLDER,
    );
    let width = key.size().x + 4.0 + text.size().x;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(width, text.size().y.max(key.size().y)),
        Sense::hover(),
    );
    let y = rect.center().y - key.size().y / 2.0;
    ui.painter()
        .galley(Pos2::new(rect.left(), y), key, p.fg_muted);
    ui.painter().galley(
        Pos2::new(rect.left() + rect.width() - text.size().x, y),
        text,
        p.fg_muted,
    );
}

fn group_heading(ui: &mut Ui, p: &Palette, title: &str) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), GROUP_HEADING_HEIGHT),
        Sense::hover(),
    );
    ui.painter().text(
        Pos2::new(rect.left() + GROUP_PAD_X + ITEM_PAD_X, rect.top() + 8.0),
        Align2::LEFT_TOP,
        title,
        theme::font_medium(11.0),
        p.fg_muted,
    );
}

fn empty_state(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), EMPTY_HEIGHT),
        Sense::hover(),
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "No results found.",
        theme::font(theme::TEXT_UI_LABEL),
        p.fg,
    );
}

fn entry_row(ui: &mut Ui, p: &Palette, entry: &Entry, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ITEM_HEIGHT), Sense::click());
    let row = Rect::from_min_max(
        Pos2::new(rect.left() + GROUP_PAD_X, rect.top()),
        Pos2::new(rect.right() - GROUP_PAD_X, rect.bottom()),
    );
    let pressed = response.is_pointer_button_down_on();
    let fill = if selected {
        p.selection
    } else if pressed {
        p.active
    } else if response.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(row, CornerRadius::same(ITEM_RADIUS), fill);
    }
    let left = row.left() + ITEM_PAD_X;
    let right = row.right() - ITEM_PAD_X;
    icons::paint(
        ui,
        entry.icon,
        Rect::from_center_size(Pos2::new(left + 8.0, row.center().y), Vec2::splat(16.0)),
        p.fg_muted,
    );
    let text_left = left + 24.0;
    let shortcut = entry.shortcut.as_ref().map(|text| {
        ui.painter().layout_no_wrap(
            text.clone(),
            theme::font(theme::TEXT_META),
            Color32::PLACEHOLDER,
        )
    });
    let shortcut_left = shortcut
        .as_ref()
        .map(|galley| right - galley.size().x)
        .unwrap_or(right);
    let label = ui.painter().layout_no_wrap(
        entry.label.clone(),
        theme::font(theme::TEXT_META),
        Color32::PLACEHOLDER,
    );
    let label_clip_right = (text_left + label.size().x)
        .min((shortcut_left - 8.0).max(text_left))
        .max(text_left);
    let label_painter = ui.painter().with_clip_rect(Rect::from_min_max(
        Pos2::new(text_left, row.top()),
        Pos2::new(label_clip_right, row.bottom()),
    ));
    label_painter.galley(
        Pos2::new(text_left, row.center().y - label.size().y / 2.0),
        label.clone(),
        p.fg,
    );
    if !entry.detail.is_empty() {
        let galley = ui.painter().layout_no_wrap(
            entry.detail.clone(),
            theme::font(theme::TEXT_META),
            Color32::PLACEHOLDER,
        );
        let detail_right = shortcut_left - 8.0;
        let detail_left = detail_right - galley.size().x;
        let clip_left = (text_left + label.size().x + 8.0)
            .max(detail_left)
            .min(detail_right);
        let painter = ui.painter().with_clip_rect(Rect::from_min_max(
            Pos2::new(clip_left, row.top()),
            Pos2::new(detail_right, row.bottom()),
        ));
        painter.galley(
            Pos2::new(detail_left, row.center().y - galley.size().y / 2.0),
            galley,
            p.fg_muted,
        );
    }
    if let Some(galley) = shortcut {
        ui.painter().galley(
            Pos2::new(shortcut_left, row.center().y - galley.size().y / 2.0),
            galley,
            p.fg_muted,
        );
    }
    response
}

fn is_secondary(entry: &Entry) -> bool {
    matches!(entry.label.as_str(), "Home" | "Reconnect")
}

fn match_score(text: &str, needle: &str) -> Option<u32> {
    if needle.is_empty() {
        return Some(0);
    }
    let lower = text.to_lowercase();
    if lower.starts_with(needle) {
        return Some(3);
    }
    let words: Vec<&str> = lower
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if needle
        .split_whitespace()
        .all(|token| words.iter().any(|word| word.starts_with(token)))
    {
        return Some(2);
    }
    needle
        .split_whitespace()
        .all(|token| lower.contains(token))
        .then_some(1)
}

fn groups(app: &RustChamber, query: &str) -> Vec<Group> {
    let needle = query.trim().to_lowercase();
    let has_query = !needle.is_empty();
    let mut groups: Vec<Group> = Vec::new();

    let commands = command_entries(app, &needle, has_query);
    let sessions = session_entries(app, &needle, has_query);
    if !has_query {
        if !sessions.items.is_empty() {
            groups.push(sessions);
        }
        if !commands.items.is_empty() {
            groups.push(commands);
        }
    } else {
        if !commands.items.is_empty() {
            groups.push(commands);
        }
        groups.push(settings_group(&needle));
        if !sessions.items.is_empty() {
            groups.push(sessions);
        }
        let projects = project_entries(app, &needle);
        if !projects.items.is_empty() {
            groups.push(projects);
        }
        groups.sort_by_key(|group| std::cmp::Reverse(group.score));
    }

    groups.retain(|group| !group.items.is_empty());
    groups
}

fn settings_group(needle: &str) -> Group {
    let mut items = Vec::new();
    let mut score = 0;
    for (index, page) in settings::PAGES.iter().enumerate() {
        if let Some(match_score) = match_score(page.label, needle) {
            score = score.max(match_score);
            items.push(Entry {
                label: page.label.to_string(),
                detail: String::new(),
                icon: Icon::Settings,
                shortcut: None,
                command: Command::SettingsPage(index),
            });
        }
        if items.len() >= RESULT_LIMIT {
            break;
        }
    }
    Group {
        title: "Settings",
        score,
        items,
    }
}

fn command_entries(app: &RustChamber, needle: &str, has_query: bool) -> Group {
    let mut entries = vec![
        Entry {
            label: "New session".into(),
            detail: "start a fresh agent session".into(),
            icon: Icon::Plus,
            shortcut: Some(
                if cfg!(target_os = "macos") {
                    "⌘N"
                } else {
                    "Ctrl N"
                }
                .into(),
            ),
            command: Command::NewSession,
        },
        Entry {
            label: "Settings".into(),
            detail: "appearance, workspace, server".into(),
            icon: Icon::Settings,
            shortcut: Some(
                if cfg!(target_os = "macos") {
                    "⌘,"
                } else {
                    "Ctrl ,"
                }
                .into(),
            ),
            command: Command::Settings,
        },
        Entry {
            label: "Toggle theme".into(),
            detail: "dark and light".into(),
            icon: Icon::Sun,
            shortcut: None,
            command: Command::ToggleTheme,
        },
        Entry {
            label: "Toggle sidebar".into(),
            detail: if app.ui.sidebar_collapsed {
                "show the session list".into()
            } else {
                "hide the session list".into()
            },
            icon: Icon::Sidebar,
            shortcut: Some(
                if cfg!(target_os = "macos") {
                    "⌘B"
                } else {
                    "Ctrl B"
                }
                .into(),
            ),
            command: Command::ToggleSidebar,
        },
        Entry {
            label: "Toggle terminal".into(),
            detail: "open the terminal panel".into(),
            icon: Icon::Terminal,
            shortcut: Some(
                if cfg!(target_os = "macos") {
                    "⌘J"
                } else {
                    "Ctrl J"
                }
                .into(),
            ),
            command: Command::ToggleTerminal,
        },
        Entry {
            label: "Open folder…".into(),
            detail: "switch workspace".into(),
            icon: Icon::Folder,
            shortcut: None,
            command: Command::OpenFolder,
        },
        Entry {
            label: "Reconnect".into(),
            detail: match app.model.connection {
                Connection::Failed(_) => "retry the OpenCode connection".into(),
                _ => "restart the connection".into(),
            },
            icon: Icon::Refresh,
            shortcut: None,
            command: Command::Reconnect,
        },
    ];
    if has_query {
        entries.push(Entry {
            label: "Home".into(),
            detail: "return to the overview".into(),
            icon: Icon::Layers,
            shortcut: None,
            command: Command::Home,
        });
    }
    let mut score = 0;
    entries.retain(|entry| {
        if !has_query {
            return !is_secondary(entry);
        }
        let haystack = format!("{} {}", entry.label, entry.detail);
        match match_score(&haystack, needle) {
            Some(value) => {
                score = score.max(value);
                true
            }
            None => false,
        }
    });
    entries.truncate(RESULT_LIMIT);
    Group {
        title: "Commands",
        score,
        items: entries,
    }
}

fn session_entries(app: &RustChamber, needle: &str, has_query: bool) -> Group {
    let limit = if has_query {
        RESULT_LIMIT
    } else {
        SESSION_LIMIT
    };
    let mut score = 0;
    let items = app
        .model
        .sorted_sessions()
        .into_iter()
        .filter(|session| match match_score(session.title(), needle) {
            Some(value) => {
                score = score.max(value);
                true
            }
            None => false,
        })
        .take(limit)
        .map(|session| Entry {
            label: session.title().to_string(),
            detail: relative_time(session.time.updated),
            icon: Icon::Clock,
            shortcut: None,
            command: Command::OpenSession(session.id.clone()),
        })
        .collect();
    Group {
        title: "Sessions",
        score,
        items,
    }
}

fn project_entries(app: &RustChamber, needle: &str) -> Group {
    let mut score = 0;
    let items = app
        .model
        .projects
        .iter()
        .filter(|project| match match_score(&project.canonical, needle) {
            Some(value) => {
                score = score.max(value);
                true
            }
            None => false,
        })
        .take(RESULT_LIMIT)
        .map(|project| Entry {
            label: project.display_name(),
            detail: project.canonical.clone(),
            icon: Icon::Folder,
            shortcut: None,
            command: Command::OpenProject(project.canonical.clone()),
        })
        .collect();
    Group {
        title: "Projects",
        score,
        items,
    }
}

fn execute(app: &mut RustChamber, command: Command, ctx: &egui::Context) {
    match command {
        Command::NewSession => app.create_session(None),
        Command::Settings => app.open_settings(),
        Command::SettingsPage(index) => {
            settings::open_page(ctx, index);
            app.open_settings();
        }
        Command::ToggleTheme => {
            app.settings.theme = if app.palette.dark {
                theme::ThemeMode::Light
            } else {
                theme::ThemeMode::Dark
            };
            app.apply_theme(ctx);
            app.settings.save();
        }
        Command::ToggleSidebar => {
            app.ui.sidebar_collapsed = !app.ui.sidebar_collapsed;
        }
        Command::ToggleTerminal => {
            if app.ui.surface == Surface::Terminal && app.ui.context_open {
                app.ui.context_open = false;
            } else {
                app.ui.surface = Surface::Terminal;
                app.ui.context_open = true;
            }
        }
        Command::Reconnect => app.backend.send(Action::Reconnect),
        Command::OpenFolder => {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                app.set_workspace(folder.to_string_lossy().to_string());
            }
        }
        Command::Home => app.model.clear_active(),
        Command::OpenSession(id) => app.open_session(id),
        Command::OpenProject(directory) => app.set_workspace(directory),
    }
}
