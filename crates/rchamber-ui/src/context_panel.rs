use std::path::PathBuf;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Key, Layout, Margin, Modifiers, Pos2, Rect,
    Response, RichText, ScrollArea, Sense, Shape, Stroke, Ui, Vec2,
};
use rchamber_api::models::{AssistantMessage, FileDiff, McpServer, Message, Part, TokenUsage};
use rchamber_app::format::{compact_number, duration, money};
use rchamber_app::{Action, AppModel, Chat, Connection, ToastKind};
use rchamber_git::{Commit, Status};

use crate::app::{RustChamber, hover_amount, menu_nav_focus, menu_popup};
use crate::icons::{self, Icon};
use crate::surfaces::{self, PanelTab, Surface};
use crate::theme::{self, Palette};
use crate::widgets;

const ROW_HEIGHT: f32 = 28.0;
const SECTION_GAP: f32 = 16.0;
const BLOCK_GAP: f32 = 12.0;
const TREE_ROW_HEIGHT: f32 = 26.0;
const TOUCHED_LIMIT: usize = 8;
const COMMIT_ROWS: usize = 3;
const GIT_REFRESH: f64 = 2.0;
const ICON_SIZE: f32 = 14.0;
const TEXT_X: f32 = 22.0;
const SIDE_PAD: f32 = 12.0;
const VALUE_MIN: f32 = 40.0;
const LABEL_MIN: f32 = 24.0;
const MENU_WIDTH: f32 = 184.0;
#[cfg(target_os = "macos")]
const OPEN_APPS: [&str; 6] = ["Finder", "Terminal", "iTerm2", "Cursor", "Zed", "Trae"];
#[cfg(target_os = "linux")]
const OPEN_APPS: [&str; 5] = ["File manager", "Terminal", "VS Code", "Cursor", "Zed"];
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const OPEN_APPS: [&str; 5] = ["Explorer", "Terminal", "VS Code", "Cursor", "Zed"];
const SECTION_KEYS: [&str; 6] = [
    "turn-stats",
    "session-totals",
    "subagents",
    "mcp",
    "sources",
    "extensions",
];
const MAX_SUBAGENTS: usize = 8;
const USAGE_NOTE: &str = "Not reported by this server";
const EMPTY_MCP: &str = "No MCP servers configured";
const MCP_HINT: &str = "MCP servers are managed by OpenCode";
const EMPTY_TITLE: &str = "No session open";
const EMPTY_HINT: &str = "Open a session to see its context, project and usage.";
const LOADING_LABEL: &str = "Loading…";
const RECONNECTING_LABEL: &str = "Reconnecting…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowTone {
    Muted,
    Metric,
    Placeholder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelState {
    Loading,
    Failed,
    Empty,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Columns {
    label: f32,
    value: f32,
}

#[derive(Clone, Debug, Default)]
struct TurnMetrics {
    response_speed: Option<f64>,
    request_speed: Option<f64>,
    elapsed_ms: f64,
    tools_ms: f64,
    steps: usize,
    tokens: TokenUsage,
    cache_hit: Option<f64>,
    cost: f64,
    model: String,
}

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let mut close = false;
    let mut toggle_expanded = false;
    let mut toggle_sections = false;
    let expanded = app.ui.expanded;
    let all_sections = sections_open(ui.ctx());

    let tab_id = egui::Id::unique("context.inspector.tab");
    let mut tab: u8 = ui
        .ctx()
        .data(|data| data.get_temp::<u8>(tab_id))
        .unwrap_or(0)
        .min(2);
    let tabs = [
        PanelTab::new("Context", Icon::Layers).active(tab == 0),
        PanelTab::new("Files", Icon::File).active(tab == 1),
        PanelTab::new("Git", Icon::Branch).active(tab == 2),
    ];
    let header = surfaces::panel_header(
        ui,
        &p,
        &tabs,
        |_| {},
        |ui| {
            if surfaces::close_button(ui, &p).clicked() {
                close = true;
            }
            if surfaces::expand_button(ui, &p, expanded).clicked() {
                toggle_expanded = true;
            }
            if tab == 0 {
                let tip = if all_sections {
                    "Collapse sections"
                } else {
                    "Expand sections"
                };
                if surfaces::panel_button(ui, &p, Icon::Settings, tip).clicked() {
                    toggle_sections = true;
                }
            }
        },
    );
    if let Some(select) = header.select {
        tab = select.min(2) as u8;
        ui.ctx().data_mut(|data| data.insert_temp(tab_id, tab));
    }

    if close {
        app.ui.context_open = false;
    }
    if toggle_expanded {
        app.ui.expanded = !app.ui.expanded;
    }
    if toggle_sections {
        set_all_sections(ui.ctx(), !all_sections);
    }

    let state = panel_state(app.model.active.is_some(), &app.model.connection);
    if state == PanelState::Failed {
        let reason = match &app.model.connection {
            Connection::Failed(reason) => reason.clone(),
            _ => String::new(),
        };
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(SIDE_PAD);
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - SIDE_PAD).max(80.0));
                if surfaces::error_banner(ui, &p, &reason, "Retry") {
                    app.backend.send(Action::Reconnect);
                }
            });
        });
        return;
    }
    if state == PanelState::Loading {
        loading_line(ui, &p, &app.model.connection);
        return;
    }
    if state == PanelState::Empty {
        surfaces::empty_state(ui, &p, Icon::Layers, EMPTY_TITLE, EMPTY_HINT);
        return;
    }

    body(ui, &p, app, tab);
}

fn panel_state(active: bool, connection: &Connection) -> PanelState {
    if active {
        return PanelState::Ready;
    }
    match connection {
        Connection::Connecting | Connection::Reconnecting(_) => PanelState::Loading,
        Connection::Failed(_) => PanelState::Failed,
        _ => PanelState::Empty,
    }
}

fn loading_label(connection: &Connection) -> &'static str {
    if matches!(connection, Connection::Reconnecting(_)) {
        RECONNECTING_LABEL
    } else {
        LOADING_LABEL
    }
}

fn loading_line(ui: &mut Ui, p: &Palette, connection: &Connection) {
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.add_space(SIDE_PAD);
        ui.label(
            RichText::new(loading_label(connection))
                .font(theme::font(12.5))
                .color(p.fg_muted),
        );
    });
}

fn body(ui: &mut Ui, p: &Palette, app: &mut RustChamber, tab: u8) {
    ScrollArea::vertical()
        .id_salt(("context-panel-scroll", tab))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin {
                    left: SIDE_PAD as i8,
                    right: SIDE_PAD as i8,
                    top: 10,
                    bottom: 24,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    match tab {
                        1 => files_tab(ui, p, app),
                        2 => git_tab(ui, p, app),
                        _ => context_tab(ui, p, app),
                    }
                });
        });
}

fn context_tab(ui: &mut Ui, p: &Palette, app: &mut RustChamber) {
    telemetry_section(ui, p, app);
    ui.add_space(BLOCK_GAP);
    touched_section(ui, p, app);
    ui.add_space(BLOCK_GAP);
    workspace_section(ui, p, app);
    ui.add_space(BLOCK_GAP);
    extension_status_section(ui, p, app);
    ui.add_space(SECTION_GAP);
    project_section(ui, p, app);
    ui.add_space(SECTION_GAP);
    let rect = ui.available_rect_before_wrap();
    ui.painter()
        .hline(rect.left()..=rect.right(), rect.top(), p.hairline());
    ui.add_space(SECTION_GAP);
    usage_section(ui, p);
    ui.add_space(SECTION_GAP);
    turn_section(ui, p, app);
    ui.add_space(SECTION_GAP);
    session_totals_section(ui, p, app);
    ui.add_space(SECTION_GAP);
    subagents_section(ui, p, app);
    ui.add_space(SECTION_GAP);
    mcp_section(ui, p, app);
    ui.add_space(SECTION_GAP);
    sources_section(ui, p, app);
    ui.add_space(SECTION_GAP);
    extensions_section(ui, p, app);
}

fn telemetry_section(ui: &mut Ui, p: &Palette, app: &RustChamber) {
    widgets::section_eyebrow(ui, "Session telemetry (local)", p);
    ui.add_space(6.0);
    let (used, limit) = context_reading(app);
    let percent = fraction_used(used, limit) * 100.0;
    let cost = app
        .model
        .active
        .as_ref()
        .map(session_totals)
        .map(|totals| totals.cost)
        .unwrap_or(0.0);
    let context_value = format!(
        "{} / {} ({:.0}%)",
        quota_number(used),
        quota_number(limit),
        percent
    );
    let cost_value = format!("{} (Local key)", money(cost));

    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(theme::HAIRLINE, p.border))
        .corner_radius(theme::radius::MD)
        .inner_margin(Margin::symmetric(10, 10))
        .show(ui, |ui| {
            let width = ui.available_width();
            let column = ((width - 8.0) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                metric_cell(ui, p, "Context window", &context_value, column);
                metric_cell(ui, p, "Session cost", &cost_value, column);
            });
            ui.add_space(8.0);
            let (track, _) = ui.allocate_exact_size(Vec2::new(width, 3.0), Sense::hover());
            ui.painter()
                .rect_filled(track, CornerRadius::same(2), p.alpha(p.fg, 20));
            let fraction = fraction_used(used, limit);
            if fraction > 0.0 {
                let fill =
                    Rect::from_min_size(track.min, Vec2::new((width * fraction).max(3.0), 3.0));
                ui.painter()
                    .rect_filled(fill, CornerRadius::same(2), meter_color(fraction, p));
            }
        });
}

fn metric_cell(ui: &mut Ui, p: &Palette, label: &str, value: &str, width: f32) {
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add(
            egui::Label::new(
                RichText::new(label)
                    .font(theme::font_mono(11.5))
                    .color(p.fg_muted),
            )
            .truncate(),
        );
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                RichText::new(value)
                    .font(theme::font_mono(11.5))
                    .color(p.fg),
            )
            .truncate(),
        );
    });
}

fn quota_number(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        let thousands = value / 1_000.0;
        if (thousands - thousands.round()).abs() < 0.05 {
            format!("{}k", thousands.round() as i64)
        } else {
            format!("{thousands:.1}k")
        }
    } else {
        (value.max(0.0).round() as i64).to_string()
    }
}

fn session_files(app: &RustChamber) -> Vec<FileDiff> {
    app.model
        .active
        .as_ref()
        .map(|chat| chat.diff.clone())
        .unwrap_or_default()
}

fn touched_section(ui: &mut Ui, p: &Palette, app: &mut RustChamber) {
    let files = session_files(app);
    widgets::section_eyebrow(ui, &format!("Touched in this session ({})", files.len()), p);
    ui.add_space(2.0);
    if files.is_empty() {
        muted_note(ui, p, "No files touched yet");
        return;
    }
    let mut copy: Option<String> = None;
    for file in files.iter().take(TOUCHED_LIMIT) {
        let status = file.status.clone().unwrap_or_else(|| "modified".into());
        let (letter, color) = status_mark(&status, p);
        let stats = format!("{letter} +{} −{}", file.additions, file.deletions);
        let row = tree_row(ui, p, &file.file, &stats, color, None, 0.0);
        if row.clicked() {
            copy = Some(file.file.clone());
        }
        row.on_hover_text(&file.file);
    }
    if let Some(path) = copy {
        ui.ctx().copy_text(path);
        app.model.toast(ToastKind::Success, "Path copied");
    }
}

fn workspace_section(ui: &mut Ui, p: &Palette, app: &RustChamber) {
    widgets::section_eyebrow(ui, "Workspace tree", p);
    ui.add_space(2.0);
    let root = app.workspace_directory();
    let files = session_files(app);
    let mut rows = 0;
    if let Some(entries) = app.model.files.tree.get(&root) {
        for entry in entries.iter().take(8) {
            let is_dir = entry.kind == "directory" || entry.kind == "dir";
            let name = file_name(&entry.path);
            if is_dir {
                let count = app
                    .model
                    .files
                    .tree
                    .get(&entry.path)
                    .map(|children| children.len())
                    .unwrap_or(0);
                let chip = if name.contains("ext") {
                    Some("Lua 5.4".to_string())
                } else if count > 0 {
                    Some(count.to_string())
                } else {
                    None
                };
                tree_row(
                    ui,
                    p,
                    &format!("{name}/"),
                    "",
                    p.fg_muted,
                    chip.as_deref(),
                    0.0,
                );
            } else {
                let (badge, color) = workspace_badge(&files, &entry.path, p);
                tree_row(ui, p, &name, &badge, color, None, 0.0);
            }
            rows += 1;
        }
    }
    if rows == 0 {
        let mut dirs: Vec<(String, usize)> = Vec::new();
        for file in &files {
            let Some((dir, _)) = file.file.split_once('/') else {
                let (badge, color) = workspace_badge(&files, &file.file, p);
                tree_row(ui, p, &file.file, &badge, color, None, 0.0);
                continue;
            };
            match dirs.iter_mut().find(|(name, _)| name == dir) {
                Some((_, count)) => *count += 1,
                None => dirs.push((dir.to_string(), 1)),
            }
        }
        for (dir, count) in dirs.iter().take(6) {
            tree_row(
                ui,
                p,
                &format!("{dir}/"),
                &count.to_string(),
                p.fg_muted,
                None,
                0.0,
            );
        }
        if dirs.is_empty() && files.is_empty() {
            muted_note(ui, p, "No workspace files loaded");
        }
    }
}

fn workspace_badge(files: &[FileDiff], path: &str, p: &Palette) -> (String, Color32) {
    let Some(file) = files.iter().find(|file| file.file == path) else {
        return (String::new(), p.fg_muted);
    };
    let status = file.status.clone().unwrap_or_else(|| "modified".into());
    let (letter, color) = status_mark(&status, p);
    (letter.to_string(), color)
}

fn extension_status_section(ui: &mut Ui, p: &Palette, app: &RustChamber) {
    widgets::section_eyebrow(ui, "Lua extension status", p);
    ui.add_space(4.0);
    let active: Vec<&rchamber_ext::InstalledExtension> = app
        .model
        .extensions
        .installed
        .iter()
        .filter(|extension| extension.is_active())
        .collect();
    if active.is_empty() {
        muted_note(ui, p, "No Lua extensions enabled");
        return;
    }
    for extension in active.iter().take(4) {
        egui::Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(theme::HAIRLINE, p.border))
            .corner_radius(theme::radius::MD)
            .inner_margin(Margin::symmetric(10, 7))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        RichText::new(&extension.manifest.name)
                            .font(theme::font_medium(12.0))
                            .color(p.fg),
                    );
                    ui.label(
                        RichText::new(format!("v{}", extension.manifest.version))
                            .font(theme::font_mono(11.0))
                            .color(p.fg_muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        status_chip(ui, "sandboxed", p.success_text, p.success_soft);
                    });
                });
            });
        ui.add_space(4.0);
    }
}

fn files_tab(ui: &mut Ui, p: &Palette, app: &mut RustChamber) {
    let files = session_files(app);
    widgets::section_eyebrow(ui, &format!("Touched in this session ({})", files.len()), p);
    ui.add_space(2.0);
    if files.is_empty() {
        muted_note(ui, p, "No files touched yet");
    }
    for file in files.iter().take(TOUCHED_LIMIT) {
        let status = file.status.clone().unwrap_or_else(|| "modified".into());
        let (letter, color) = status_mark(&status, p);
        let stats = format!("{letter} +{} −{}", file.additions, file.deletions);
        tree_row(ui, p, &file.file, &stats, color, None, 0.0);
    }
    ui.add_space(BLOCK_GAP);
    workspace_section(ui, p, app);
    ui.add_space(BLOCK_GAP);
    if widgets::app_button(
        ui,
        "Open the Files panel",
        Some(Icon::File),
        widgets::ButtonVariant::Outline,
        p,
    )
    .clicked()
    {
        app.ui.surface = Surface::Files;
        app.ui.context_open = true;
    }
}

fn tree_row(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    right: &str,
    right_color: Color32,
    chip: Option<&str>,
    indent: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TREE_ROW_HEIGHT),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::XS), p.hover);
    }
    let left = rect.left() + 6.0 + indent;
    let right_edge = rect.right() - 6.0;
    let chip_width = chip
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.to_string(), theme::font_mono(11.0), p.fg_muted)
                .size()
                .x
                + 12.0
        })
        .unwrap_or(0.0);
    let stats_width = if right.is_empty() {
        0.0
    } else {
        ui.painter()
            .layout_no_wrap(right.to_string(), theme::font_mono(11.0), right_color)
            .size()
            .x
            + 8.0
    };
    let label_width = (right_edge - left - chip_width - stats_width).max(24.0);
    let galley = surfaces::truncated_galley(
        ui,
        label.to_string(),
        theme::font_mono(12.0),
        p.fg_muted,
        label_width,
    );
    ui.painter().galley(
        Pos2::new(left, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg_muted,
    );
    let mut x = right_edge;
    if let Some(label) = chip {
        let galley =
            ui.painter()
                .layout_no_wrap(label.to_string(), theme::font_mono(11.0), p.fg_muted);
        let chip_rect = Rect::from_min_size(
            Pos2::new(
                x - galley.size().x - 12.0,
                rect.center().y - theme::TAG_HEIGHT * 0.5,
            ),
            Vec2::new(galley.size().x + 12.0, theme::TAG_HEIGHT),
        );
        ui.painter()
            .rect_filled(chip_rect, CornerRadius::same(theme::radius::XS), p.tag_bg);
        ui.painter().galley(
            Pos2::new(
                chip_rect.left() + 6.0,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            p.fg_muted,
        );
        x = chip_rect.left() - 6.0;
    }
    if !right.is_empty() {
        ui.painter().text(
            Pos2::new(x, rect.center().y),
            Align2::RIGHT_CENTER,
            right,
            theme::font_mono(11.0),
            right_color,
        );
    }
    response
}

fn status_chip(ui: &mut Ui, label: &str, fg: Color32, bg: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::font_mono(11.0), fg);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + 12.0, theme::TAG_HEIGHT),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::XS), bg);
    ui.painter().galley(
        Pos2::new(rect.left() + 6.0, rect.center().y - galley.size().y * 0.5),
        galley,
        fg,
    );
}

fn status_mark(status: &str, p: &Palette) -> (&'static str, Color32) {
    match status.to_ascii_lowercase().as_str() {
        "added" | "untracked" => ("A", p.green),
        "deleted" | "removed" => ("D", p.red),
        "renamed" => ("R", p.blue),
        "copied" => ("C", p.blue),
        "conflicted" | "conflict" => ("!", p.coral),
        _ => ("M", p.coral),
    }
}

fn file_name(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| path.to_string())
}

#[derive(Clone, Default)]
struct GitCache {
    root: Option<PathBuf>,
    status: Status,
    commits: Vec<Commit>,
    loaded_at: f64,
}

fn git_tab(ui: &mut Ui, p: &Palette, app: &mut RustChamber) {
    let now = ui.input(|input| input.time);
    let cache_id = egui::Id::unique("context.git.cache");
    let mut cache: GitCache = ui
        .ctx()
        .data(|data| data.get_temp(cache_id))
        .unwrap_or_default();
    let start = app.workspace_directory();
    let root = rchamber_git::discover(std::path::Path::new(&start));
    let stale = cache.loaded_at == 0.0 || now - cache.loaded_at > GIT_REFRESH || cache.root != root;
    if stale {
        cache = GitCache {
            root: root.clone(),
            status: root
                .as_deref()
                .and_then(|root| rchamber_git::status(root).ok())
                .unwrap_or_default(),
            commits: root
                .as_deref()
                .and_then(|root| rchamber_git::log(root, COMMIT_ROWS).ok())
                .unwrap_or_default(),
            loaded_at: now,
        };
    }

    let summary_id = egui::Id::unique("context.git.summary");
    let description_id = egui::Id::unique("context.git.description");
    let mut summary: String = ui
        .ctx()
        .data(|data| data.get_temp(summary_id))
        .unwrap_or_default();
    let mut description: String = ui
        .ctx()
        .data(|data| data.get_temp(description_id))
        .unwrap_or_default();

    let staged: Vec<String> = cache
        .status
        .staged()
        .map(|file| file.path.clone())
        .collect();
    let unstaged: Vec<String> = cache
        .status
        .unstaged()
        .map(|file| file.path.clone())
        .collect();
    let branch = cache
        .status
        .branch
        .clone()
        .or_else(|| {
            app.model
                .vcs
                .as_ref()
                .and_then(|vcs| vcs.branch.as_ref())
                .and_then(|branch| branch.current.clone())
        })
        .unwrap_or_else(|| "No repository".into());

    let mut stage_all = false;
    let mut unstage_all = false;
    let mut fetch = false;
    let mut pull = false;
    let mut push = false;
    let mut commit = false;

    let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::hover());
    ui.painter().rect_filled(bar, CornerRadius::ZERO, p.bg_soft);
    ui.painter().hline(
        bar.x_range(),
        bar.bottom(),
        Stroke::new(theme::HAIRLINE, p.border),
    );
    icons::paint(
        ui,
        Icon::Branch,
        Rect::from_center_size(
            Pos2::new(bar.left() + 12.0, bar.center().y),
            Vec2::splat(14.0),
        ),
        p.fg_muted,
    );
    let branch_galley = surfaces::truncated_galley(
        ui,
        branch.clone(),
        theme::font_medium(12.5),
        p.fg,
        (bar.width() - 160.0).max(40.0),
    );
    ui.painter().galley(
        Pos2::new(
            bar.left() + 22.0,
            bar.center().y - branch_galley.size().y * 0.5,
        ),
        branch_galley,
        p.fg,
    );
    let mut button_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(Vec2::new(6.0, 5.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    button_ui.spacing_mut().item_spacing.x = 4.0;
    let can_fetch = cache.root.is_some();
    if widgets::button_ex(
        &mut button_ui,
        "Push",
        None,
        widgets::ButtonVariant::Outline,
        widgets::ButtonSize::App,
        can_fetch,
        p,
    )
    .clicked()
    {
        push = true;
    }
    if widgets::button_ex(
        &mut button_ui,
        "Pull",
        None,
        widgets::ButtonVariant::Outline,
        widgets::ButtonSize::App,
        can_fetch,
        p,
    )
    .clicked()
    {
        pull = true;
    }
    if widgets::button_ex(
        &mut button_ui,
        "Fetch",
        None,
        widgets::ButtonVariant::Outline,
        widgets::ButtonSize::App,
        can_fetch,
        p,
    )
    .clicked()
    {
        fetch = true;
    }
    ui.painter().text(
        Pos2::new(button_ui.min_rect().left() - 8.0, bar.center().y),
        Align2::RIGHT_CENTER,
        format!("↑{} ↓{}", cache.status.ahead, cache.status.behind),
        theme::font_mono(11.0),
        p.fg_muted,
    );

    ui.add_space(10.0);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(theme::HAIRLINE, p.border))
        .corner_radius(theme::radius::MD)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            commit_field(ui, p, &mut summary, "Commit summary", 28.0);
            ui.add_space(6.0);
            commit_field(ui, p, &mut description, "Description", 54.0);
            ui.add_space(8.0);
            let label = if staged.len() == 1 {
                "Commit 1 staged file (⌘↵)".to_string()
            } else {
                format!("Commit {} staged files (⌘↵)", staged.len())
            };
            let enabled = !staged.is_empty() && !summary.trim().is_empty();
            let clicked = widgets::button_ex(
                ui,
                &label,
                None,
                widgets::ButtonVariant::Primary,
                widgets::ButtonSize::Sm,
                enabled,
                p,
            )
            .clicked();
            if enabled
                && (clicked
                    || (ui.ui_contains_pointer()
                        && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter))))
            {
                commit = true;
            }
        });

    ui.add_space(BLOCK_GAP);
    if eyebrow_action(
        ui,
        p,
        &format!("Staged changes ({})", staged.len()),
        Some("Unstage all"),
        Some(!staged.is_empty()),
    ) {
        unstage_all = true;
    }
    for path in staged.iter().take(TOUCHED_LIMIT) {
        tree_row(ui, p, path, "M −", p.success_text, None, 0.0);
    }
    ui.add_space(BLOCK_GAP);
    if eyebrow_action(
        ui,
        p,
        &format!("Unstaged changes ({})", unstaged.len()),
        Some("Stage all"),
        Some(!unstaged.is_empty()),
    ) {
        stage_all = true;
    }
    for path in unstaged.iter().take(TOUCHED_LIMIT) {
        tree_row(ui, p, path, "M +", p.coral, None, 0.0);
    }
    ui.add_space(BLOCK_GAP);
    eyebrow_action(ui, p, "Recent commits", None, None);
    for commit in cache.commits.iter().take(COMMIT_ROWS) {
        let short = commit.hash.get(..7).unwrap_or(&commit.hash);
        tree_row(ui, p, &commit.subject, short, p.primary_text, None, 0.0);
    }
    if cache.commits.is_empty() {
        muted_note(ui, p, "No commits yet");
    }

    if !stage_all && !unstage_all && !fetch && !pull && !push && !commit {
        ui.ctx().data_mut(|data| {
            data.insert_temp(summary_id, summary.clone());
            data.insert_temp(description_id, description.clone());
        });
        ui.ctx().data_mut(|data| data.insert_temp(cache_id, cache));
        return;
    }

    if let Some(root) = cache.root.clone() {
        let mut result: Vec<String> = Vec::new();
        let mut error: Option<String> = None;
        if stage_all {
            for path in &unstaged {
                if let Err(failure) = rchamber_git::stage(&root, path) {
                    error = Some(failure.to_string());
                }
            }
            result.push(format!("Staged {} files", unstaged.len()));
        }
        if unstage_all {
            for path in &staged {
                if let Err(failure) = rchamber_git::unstage(&root, path) {
                    error = Some(failure.to_string());
                }
            }
            result.push(format!("Unstaged {} files", staged.len()));
        }
        if fetch && let Err(failure) = rchamber_git::fetch(&root) {
            error = Some(failure.to_string());
        }
        if pull && let Err(failure) = rchamber_git::pull(&root) {
            error = Some(failure.to_string());
        }
        if push && let Err(failure) = rchamber_git::push(&root) {
            error = Some(failure.to_string());
        }
        if commit && !summary.trim().is_empty() {
            let message = if description.trim().is_empty() {
                summary.trim().to_string()
            } else {
                format!("{}\n\n{}", summary.trim(), description.trim())
            };
            match rchamber_git::commit(&root, &message) {
                Ok(()) => {
                    result.push("Committed staged changes".into());
                    summary.clear();
                    description.clear();
                }
                Err(failure) => error = Some(failure.to_string()),
            }
        }
        if let Some(error) = error {
            app.model.toast(ToastKind::Error, error);
        } else if let Some(message) = result.first() {
            app.model.toast(ToastKind::Success, message.clone());
        }
        ui.ctx().data_mut(|data| {
            data.insert_temp(summary_id, summary.clone());
            data.insert_temp(description_id, description.clone());
            data.insert_temp(cache_id, GitCache::default());
        });
    }
}

fn commit_field(ui: &mut Ui, p: &Palette, text: &mut String, hint: &str, height: f32) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let id = response.id.with(("commit-field", hint));
    let focused = ui.memory(|memory| memory.has_focus(id));
    let corner = CornerRadius::same(theme::radius::XS);
    ui.painter().rect_filled(rect, corner, p.bg_soft);
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(
            if focused {
                theme::BORDER
            } else {
                theme::HAIRLINE
            },
            if focused { p.border_focus } else { p.border },
        ),
        egui::StrokeKind::Inside,
    );
    let text_rect = rect.shrink2(Vec2::new(8.0, 5.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Min)),
    );
    let multiline = height > 40.0;
    let edit = if multiline {
        egui::TextEdit::multiline(text)
    } else {
        egui::TextEdit::singleline(text)
    }
    .id(id)
    .frame(egui::Frame::NONE)
    .margin(Margin::ZERO)
    .hint_text(
        RichText::new(hint)
            .font(theme::font(12.0))
            .color(p.fg_faint),
    )
    .font(theme::font(12.0))
    .text_color(p.fg)
    .desired_width(f32::INFINITY);
    child.add(edit);
    if response.clicked() {
        child.memory_mut(|memory| memory.request_focus(id));
    }
}

fn eyebrow_action(
    ui: &mut Ui,
    p: &Palette,
    text: &str,
    action: Option<&str>,
    enabled: Option<bool>,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    ui.painter().text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        text.to_uppercase(),
        theme::font_mono(11.0),
        p.fg_muted,
    );
    let Some(action) = action else {
        return false;
    };
    let enabled = enabled.unwrap_or(true);
    let galley = ui.painter().layout_no_wrap(
        action.to_string(),
        theme::font(11.5),
        if enabled { p.primary } else { p.disabled },
    );
    let action_rect = Rect::from_min_max(
        Pos2::new(rect.right() - galley.size().x - 2.0, rect.top()),
        rect.max,
    );
    ui.painter().galley(
        Pos2::new(action_rect.left(), rect.center().y - galley.size().y * 0.5),
        galley,
        if enabled { p.primary } else { p.disabled },
    );
    enabled
        && ui
            .interact(
                action_rect,
                ui.scope_id().with((text, action)),
                Sense::click(),
            )
            .clicked()
}

fn fraction_used(used: f64, limit: f64) -> f32 {
    if limit <= 0.0 {
        return 0.0;
    }
    (used / limit).clamp(0.0, 1.0) as f32
}

fn context_reading(app: &RustChamber) -> (f64, f64) {
    let Some(chat) = &app.model.active else {
        return (0.0, 200_000.0);
    };
    let used = chat
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::Assistant(assistant) => assistant.tokens.as_ref(),
            _ => None,
        })
        .map(|tokens| tokens.context_used())
        .unwrap_or(chat.session.tokens.context_used());
    let limit = app
        .model
        .current_model()
        .and_then(|model| model.context_limit())
        .unwrap_or(200_000) as f64;
    (used, limit.max(1.0))
}

fn project_section(ui: &mut Ui, p: &Palette, app: &mut RustChamber) {
    let directory = project_directory(app);
    let project = project_name(app, &directory);
    let has_folder = !directory.is_empty();
    let rows = if has_folder { 4.0 } else { 3.0 };
    let pad_y = 4.0;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), rows * ROW_HEIGHT + pad_y * 2.0),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        let corner = CornerRadius::same(theme::radius::LG);
        ui.painter().rect_filled(rect, corner, p.card);
        ui.painter().rect_stroke(
            rect,
            corner,
            Stroke::new(theme::HAIRLINE, p.alpha(p.fg, if p.dark { 16 } else { 22 })),
            egui::StrokeKind::Inside,
        );
    }
    let inner = rect.shrink2(Vec2::new(4.0, pad_y));
    let mut card = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    card.spacing_mut().item_spacing = Vec2::ZERO;

    let heading = project_heading(&mut card, p, &project, has_folder);
    project_menu(&mut card, app, p, &heading, &directory);

    let branch = app
        .model
        .vcs
        .as_ref()
        .and_then(|vcs| vcs.branch.as_ref())
        .and_then(|branch| branch.current.clone());
    match branch {
        Some(branch) => {
            row(
                &mut card,
                p,
                Some(Icon::Branch),
                &branch,
                "",
                RowTone::Muted,
            )
            .on_hover_text("Current branch");
        }
        None => {
            row(
                &mut card,
                p,
                Some(Icon::Branch),
                "No repository",
                "",
                RowTone::Placeholder,
            )
            .on_hover_text("This session has no Git repository");
        }
    }

    if has_folder {
        row(
            &mut card,
            p,
            Some(Icon::Folder),
            &directory,
            "",
            RowTone::Muted,
        )
        .on_hover_text(&directory);
    }

    dev_server_row(&mut card, p);
}

fn project_directory(app: &RustChamber) -> String {
    app.model
        .active
        .as_ref()
        .and_then(|chat| chat.session.directory().map(str::to_string))
        .or_else(|| app.model.workspace.clone())
        .unwrap_or_default()
}

fn project_name(app: &RustChamber, directory: &str) -> String {
    app.model
        .projects
        .iter()
        .find(|project| project.canonical == directory)
        .map(|project| project.display_name())
        .unwrap_or_else(|| {
            std::path::Path::new(directory)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "Workspace".into())
        })
}

fn project_heading(ui: &mut Ui, p: &Palette, project: &str, enabled: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), ROW_HEIGHT),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let hover = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
        if hover > 0.001 {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(theme::radius::MD),
                p.hover.gamma_multiply(hover),
            );
        }
        let center_y = rect.center().y;
        let title_x = rect.left() + 4.0;
        let title = ui
            .painter()
            .layout_no_wrap("Project".into(), theme::font_semibold(12.5), p.fg);
        ui.painter().galley(
            Pos2::new(title_x, center_y - title.size().y * 0.5),
            title.clone(),
            p.fg,
        );
        let mut x = title_x + title.size().x + 8.0;
        icons::paint(
            ui,
            Icon::Folder,
            Rect::from_center_size(
                Pos2::new(x + ICON_SIZE * 0.5, center_y),
                Vec2::splat(ICON_SIZE),
            ),
            if enabled { p.fg_muted } else { p.fg_faint },
        );
        x += ICON_SIZE + 8.0;
        let chevron_x = rect.right() - 10.0;
        let label_width = (chevron_x - 10.0 - x).max(24.0);
        let name_color = if enabled { p.fg } else { p.fg_faint };
        let galley = surfaces::truncated_galley(
            ui,
            project.to_string(),
            theme::font(13.0),
            name_color,
            label_width,
        );
        ui.painter().galley(
            Pos2::new(x, center_y - galley.size().y * 0.5),
            galley,
            name_color,
        );
        icons::paint(
            ui,
            Icon::ChevronDown,
            Rect::from_center_size(Pos2::new(chevron_x, center_y), Vec2::splat(12.0)),
            if enabled {
                p.mix(p.fg_muted, p.fg, hover)
            } else {
                p.disabled
            },
        );
    }
    response
}

fn project_menu(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    anchor: &Response,
    directory: &str,
) {
    if directory.is_empty() {
        return;
    }
    let menu_id = egui::Id::unique("context-project-menu");
    let mut open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(menu_id.with("open")))
        .unwrap_or(false);
    if anchor.clicked() {
        open = !open;
    }
    let app_id = egui::Id::unique(("context-project-app", directory));
    let selected = ui
        .ctx()
        .data(|data| data.get_temp::<String>(app_id))
        .unwrap_or_else(|| OPEN_APPS[0].to_string());
    let mut chosen: Option<String> = None;
    let mut copy = false;
    let mut items = Vec::new();
    menu_popup(
        ui.ctx(),
        anchor,
        menu_id.with("popup"),
        &mut open,
        p,
        6.0,
        |ui, _| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.set_width(MENU_WIDTH);
            let response = widgets::menu_item(ui, "Copy path", Some(Icon::Copy), None, p);
            if response.clicked() {
                copy = true;
            }
            items.push(response);
            ui.add_space(2.0);
            widgets::menu_separator(ui, p);
            ui.add_space(2.0);
            for name in OPEN_APPS {
                let response =
                    widgets::menu_item_ex(ui, name, None, None, selected == name, true, p);
                if response.clicked() {
                    chosen = Some(name.to_string());
                }
                items.push(response);
            }
        },
    );
    menu_nav_focus(ui.ctx(), menu_id.with("popup"), open, &items);
    if copy {
        ui.ctx().copy_text(directory.to_string());
        app.model.toast(ToastKind::Success, "Project path copied");
        open = false;
    }
    if let Some(chosen) = chosen {
        ui.ctx().data_mut(|data| data.insert_temp(app_id, chosen));
        open = false;
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(menu_id.with("open"), open));
}

fn dev_server_row(ui: &mut Ui, p: &Palette) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let center_y = rect.center().y;
    icons::paint(
        ui,
        Icon::Terminal,
        Rect::from_center_size(
            Pos2::new(rect.left() + 4.0 + ICON_SIZE * 0.5, center_y),
            Vec2::splat(ICON_SIZE),
        ),
        p.fg_faint,
    );
    let chevron_x = rect.right() - 12.0;
    let play_x = chevron_x - 26.0;
    let galley = surfaces::truncated_galley(
        ui,
        "dev server".into(),
        theme::font(13.0),
        p.fg_muted,
        (play_x - 12.0 - (rect.left() + TEXT_X)).max(24.0),
    );
    ui.painter().galley(
        Pos2::new(rect.left() + TEXT_X, center_y - galley.size().y * 0.5),
        galley,
        p.fg_muted,
    );
    play_triangle(ui, Pos2::new(play_x, center_y), p.fg_faint);
    icons::paint(
        ui,
        Icon::ChevronRight,
        Rect::from_center_size(Pos2::new(chevron_x, center_y), Vec2::splat(12.0)),
        p.fg_faint,
    );
    response.on_hover_text("No dev server is configured for this project");
}

fn play_triangle(ui: &Ui, center: Pos2, color: Color32) {
    let points = vec![
        Pos2::new(center.x - 3.0, center.y - 5.0),
        Pos2::new(center.x + 5.0, center.y),
        Pos2::new(center.x - 3.0, center.y + 5.0),
    ];
    ui.painter()
        .add(Shape::convex_polygon(points, color, Stroke::NONE));
}

fn usage_section(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        let center_y = rect.center().y;
        ui.painter().text(
            Pos2::new(rect.left() + 4.0, center_y),
            Align2::LEFT_CENTER,
            "Usage",
            theme::font_semibold(12.5),
            p.fg_muted,
        );
        let galley = ui
            .painter()
            .layout_no_wrap("—".into(), theme::font_mono(12.0), p.fg_faint);
        ui.painter().galley(
            Pos2::new(
                rect.right() - 2.0 - galley.size().x,
                center_y - galley.size().y * 0.5,
            ),
            galley,
            p.fg_faint,
        );
    }
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let center_y = rect.center().y;
    ui.painter().text(
        Pos2::new(rect.left() + 4.0, center_y),
        Align2::LEFT_CENTER,
        "5 hours",
        theme::font(13.0),
        p.fg_muted,
    );
    let galley = surfaces::truncated_galley(
        ui,
        USAGE_NOTE.to_string(),
        theme::font(11.5),
        p.fg_faint,
        (rect.width() - 4.0 - 64.0 - 8.0).max(24.0),
    );
    ui.painter().galley(
        Pos2::new(
            rect.right() - 2.0 - galley.size().x,
            center_y - galley.size().y * 0.5,
        ),
        galley,
        p.fg_faint,
    );
}

fn turn_section(ui: &mut Ui, p: &Palette, app: &RustChamber) {
    let Some(chat) = &app.model.active else {
        return;
    };
    let metrics = turn_metrics(chat, &app.model);
    let summary = speed_text(metrics.response_speed);
    let open = collapsible(ui, p, "turn-stats", "Turn stats", &summary);
    if !open {
        return;
    }
    for (label, value) in metric_rows(&metrics) {
        row(ui, p, None, label, &value, RowTone::Metric);
    }
}

fn metric_rows(metrics: &TurnMetrics) -> Vec<(&'static str, String)> {
    vec![
        ("Response", speed_text(metrics.response_speed)),
        ("Request", speed_text(metrics.request_speed)),
        ("Elapsed", non_empty(duration(metrics.elapsed_ms))),
        ("Model", non_empty(metrics.model.clone())),
        ("Tools", non_empty(duration(metrics.tools_ms))),
        ("Steps", metrics.steps.to_string()),
        ("Tokens", tokens_text(&metrics.tokens)),
        (
            "Cache",
            metrics
                .cache_hit
                .map(|hit| format!("{:.0}%", hit * 100.0))
                .unwrap_or_else(|| "—".into()),
        ),
        ("Cost", money(metrics.cost)),
    ]
}

fn tokens_text(tokens: &TokenUsage) -> String {
    format!(
        "{} ↑ · {} ↓",
        compact_number(tokens.input),
        compact_number(tokens.output + tokens.reasoning)
    )
}

fn session_totals_section(ui: &mut Ui, p: &Palette, app: &RustChamber) {
    let Some(chat) = &app.model.active else {
        return;
    };
    let metrics = session_totals(chat);
    let summary = speed_text(metrics.response_speed);
    let open = collapsible(ui, p, "session-totals", "Session totals", &summary);
    if !open {
        return;
    }
    for (label, value) in [
        ("Requests", metrics.requests.to_string()),
        ("Input", compact_number(metrics.tokens.input)),
        ("Output", compact_number(metrics.tokens.output)),
        ("Reasoning", compact_number(metrics.tokens.reasoning)),
        ("Cache read", compact_number(metrics.tokens.cache.read)),
        ("Cache write", compact_number(metrics.tokens.cache.write)),
        (
            "Cache hit",
            metrics
                .cache_hit
                .map(|hit| format!("{:.0}%", hit * 100.0))
                .unwrap_or_else(|| "—".into()),
        ),
        ("Cost", money(metrics.cost)),
    ] {
        row(ui, p, None, label, &value, RowTone::Metric);
    }
}

#[derive(Default)]
struct SessionTotals {
    tokens: TokenUsage,
    cache_hit: Option<f64>,
    cost: f64,
    requests: usize,
    response_speed: Option<f64>,
}

fn session_totals(chat: &Chat) -> SessionTotals {
    let mut totals = SessionTotals::default();
    let mut output_ms = 0.0;
    for message in &chat.messages {
        let Message::Assistant(assistant) = message else {
            continue;
        };
        totals.requests += 1;
        totals.cost += assistant.cost;
        if let Some(tokens) = &assistant.tokens {
            totals.tokens.input += tokens.input;
            totals.tokens.output += tokens.output;
            totals.tokens.reasoning += tokens.reasoning;
            totals.tokens.cache.read += tokens.cache.read;
            totals.tokens.cache.write += tokens.cache.write;
        }
        if let Some(completed) = assistant.time.completed {
            let elapsed = completed - assistant.time.created;
            if elapsed > 0.0 {
                output_ms += elapsed;
            }
        }
    }
    totals.cache_hit = hit_rate(
        totals.tokens.input,
        totals.tokens.cache.read,
        totals.tokens.cache.write,
    );
    if output_ms > 0.0 {
        let tokens = totals.tokens.output + totals.tokens.reasoning;
        if tokens > 0.0 {
            totals.response_speed = Some(tokens / (output_ms / 1000.0));
        }
    }
    totals
}

fn subagents_section(ui: &mut Ui, p: &Palette, app: &mut RustChamber) {
    let Some(active_id) = app.model.active_id().map(str::to_string) else {
        return;
    };
    let mut children: Vec<(String, String, f64)> = app
        .model
        .sessions
        .iter()
        .filter(|session| session.parent_id.as_deref() == Some(active_id.as_str()))
        .map(|session| {
            (
                session.id.clone(),
                session.title().to_string(),
                session.time.created,
            )
        })
        .collect();
    children.sort_by(|left, right| right.2.total_cmp(&left.2));
    let summary = children.len().to_string();
    let open = collapsible(ui, p, "subagents", "Subagents", &summary);
    if !open {
        return;
    }
    if children.is_empty() {
        muted_note(ui, p, "No subagents in this session");
        return;
    }
    let mut open_id: Option<String> = None;
    for (id, title, _) in children.iter().take(MAX_SUBAGENTS) {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::click());
        crate::widgets::pointer(ui, &response);
        if ui.is_rect_visible(rect) {
            let hover = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
            if hover > 0.001 {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(theme::radius::MD),
                    p.hover.gamma_multiply(hover),
                );
            }
            let center_y = rect.center().y;
            check_circle(ui, Pos2::new(rect.left() + 4.0 + 7.0, center_y), p);
            let galley = surfaces::truncated_galley(
                ui,
                title.clone(),
                theme::font(13.0),
                p.mix(p.fg_muted, p.fg, hover),
                (rect.width() - TEXT_X - 16.0).max(24.0),
            );
            ui.painter().galley(
                Pos2::new(rect.left() + TEXT_X, center_y - galley.size().y * 0.5),
                galley,
                p.fg,
            );
        }
        if response.clicked() {
            open_id = Some(id.clone());
        }
    }
    if children.len() > MAX_SUBAGENTS {
        muted_note(ui, p, &format!("+{} more", children.len() - MAX_SUBAGENTS));
    }
    if let Some(id) = open_id {
        app.open_session(id);
    }
}

fn check_circle(ui: &Ui, center: Pos2, p: &Palette) {
    ui.painter().circle_filled(center, 7.0, p.green);
    icons::paint(
        ui,
        Icon::Check,
        Rect::from_center_size(center, Vec2::splat(9.0)),
        p.on_primary,
    );
}

fn mcp_section(ui: &mut Ui, p: &Palette, app: &RustChamber) {
    let servers = &app.model.mcp;
    let summary = mcp_summary(servers);
    let open = collapsible_summary(ui, p, "mcp", "MCP", &summary);
    if !open {
        return;
    }
    if servers.is_empty() {
        muted_note(ui, p, EMPTY_MCP);
        return;
    }
    for server in servers {
        mcp_row(ui, p, server);
    }
}

fn mcp_summary(servers: &[McpServer]) -> String {
    if servers.is_empty() {
        return "—".into();
    }
    let connected = servers
        .iter()
        .filter(|server| server.is_connected())
        .count();
    format!("{connected}/{}", servers.len())
}

fn mcp_row(ui: &mut Ui, p: &Palette, server: &McpServer) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let center_y = rect.center().y;
    let switch_rect = Rect::from_center_size(
        Pos2::new(
            rect.right() - 2.0 - widgets::SWITCH_SM_WIDTH * 0.5,
            center_y,
        ),
        Vec2::new(widgets::SWITCH_SM_WIDTH, widgets::SWITCH_SM_HEIGHT),
    );
    let status = server.status.status.trim();
    let status_galley = (!status.is_empty() && !server.is_connected()).then(|| {
        ui.painter()
            .layout_no_wrap(status.to_string(), theme::font(11.5), p.fg_faint)
    });
    let status_width = status_galley
        .as_ref()
        .map(|galley| galley.size().x + 10.0)
        .unwrap_or(0.0);
    let name_x = rect.left() + 4.0;
    let name_width = (switch_rect.left() - 10.0 - status_width - name_x).max(24.0);
    let name =
        surfaces::truncated_galley(ui, server.name.clone(), theme::font(13.0), p.fg, name_width);
    ui.painter().galley(
        Pos2::new(name_x, center_y - name.size().y * 0.5),
        name,
        p.fg,
    );
    if let Some(galley) = status_galley {
        ui.painter().galley(
            Pos2::new(
                switch_rect.left() - 10.0 - galley.size().x,
                center_y - galley.size().y * 0.5,
            ),
            galley,
            p.fg_faint,
        );
    }
    let mut on = server.is_connected();
    let mut switch_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(switch_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    widgets::switch_ex(&mut switch_ui, &mut on, widgets::SwitchSize::Sm, false, p)
        .on_hover_text(MCP_HINT);
}

fn sources_section(ui: &mut Ui, p: &Palette, app: &RustChamber) {
    let skills = app.model.skills.len();
    let agents = app
        .model
        .agents
        .iter()
        .filter(|agent| !agent.hidden)
        .count();
    let commands = app.model.commands.len();
    let snippets = app.model.snippets.len();
    let summary = format!("{skills} skills · {agents} agents");
    let open = collapsible(ui, p, "sources", "Context sources", &summary);
    if !open {
        return;
    }
    row(ui, p, None, "Skills", &skills.to_string(), RowTone::Metric);
    row(ui, p, None, "Agents", &agents.to_string(), RowTone::Metric);
    row(
        ui,
        p,
        None,
        "Commands",
        &commands.to_string(),
        RowTone::Metric,
    );
    row(
        ui,
        p,
        None,
        "Snippets",
        &snippets.to_string(),
        RowTone::Metric,
    );
}

fn extensions_section(ui: &mut Ui, p: &Palette, app: &mut RustChamber) {
    let active: Vec<rchamber_ext::InstalledExtension> = app
        .model
        .extensions
        .installed
        .iter()
        .filter(|extension| {
            extension.is_active() && extension.manifest.contributes.status_section.is_some()
        })
        .cloned()
        .collect();
    if active.is_empty() {
        return;
    }
    let summary = active.len().to_string();
    let open = collapsible(ui, p, "extensions", "Extensions", &summary);
    if !open {
        return;
    }
    for extension in active {
        let id = extension.manifest.id.clone();
        let title = extension
            .manifest
            .contributes
            .status_section
            .as_ref()
            .map(|status| status.title.clone())
            .unwrap_or_else(|| extension.manifest.name.clone());
        let requested = ui.ctx().data(|data| {
            data.get_temp::<bool>(egui::Id::unique(("ext-status-requested", id.as_str())))
                .unwrap_or(false)
        });
        if !requested {
            app.backend.send(Action::ExtensionCall {
                id: id.clone(),
                call_id: "status".into(),
                method: extension
                    .manifest
                    .contributes
                    .status_section
                    .as_ref()
                    .map(|status| status.method.clone())
                    .unwrap_or_else(|| "status".into()),
                args: Vec::new(),
            });
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::unique(("ext-status-requested", id.as_str())),
                    true,
                )
            });
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(title)
                .font(theme::font_medium(theme::TEXT_META))
                .color(p.fg_muted),
        );
        ui.add_space(4.0);
        let host = crate::extension_panel::host_for(app, &id);
        crate::blocks::set_host(ui.ctx(), host);
        if let Some(widgets) = app.model.extensions.status.get(&id).cloned() {
            crate::blocks::show_widgets(ui, &widgets, p, theme::TEXT_META);
        } else if let Some(error) = app
            .model
            .extensions
            .errors
            .get(&id)
            .map(|error| &error.message)
        {
            ui.label(
                RichText::new(error)
                    .font(theme::font(theme::TEXT_META))
                    .color(p.red),
            );
        } else {
            ui.label(
                RichText::new("Loading…")
                    .font(theme::font(theme::TEXT_META))
                    .color(p.fg_faint),
            );
        }
    }
}

fn section_default(key: &str) -> bool {
    key == "turn-stats"
}

fn section_open(ctx: &egui::Context, key: &str) -> bool {
    ctx.data(|data| data.get_temp::<bool>(egui::Id::unique(("context-section", key))))
        .unwrap_or_else(|| section_default(key))
}

fn collapsible(ui: &mut Ui, p: &Palette, key: &str, title: &str, summary: &str) -> bool {
    section_header(ui, p, key, title, summary, false)
}

fn collapsible_summary(ui: &mut Ui, p: &Palette, key: &str, title: &str, summary: &str) -> bool {
    section_header(ui, p, key, title, summary, true)
}

fn section_header(
    ui: &mut Ui,
    p: &Palette,
    key: &str,
    title: &str,
    summary: &str,
    always_summary: bool,
) -> bool {
    let id = egui::Id::unique(("context-section", key));
    let stored = section_open(ui.ctx(), key);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let right = rect.right() - 2.0;
    let response = ui.interact(
        rect,
        ui.scope_id().with(("context-section-hit", key)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let hover = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    let open = if response.clicked() { !stored } else { stored };
    if response.clicked() {
        ui.ctx().data_mut(|data| data.insert_temp(id, open));
    }
    if !ui.is_rect_visible(rect) {
        return open;
    }
    if hover > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.hover.gamma_multiply(hover),
        );
    }
    let center_y = rect.center().y;
    icons::paint(
        ui,
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        Rect::from_center_size(Pos2::new(rect.left() + 10.0, center_y), Vec2::splat(12.0)),
        p.fg_muted,
    );
    let title_x = rect.left() + 22.0;
    let title_galley =
        ui.painter()
            .layout_no_wrap(title.to_string(), theme::font_semibold(12.5), p.fg);
    ui.painter().galley(
        Pos2::new(title_x, center_y - title_galley.size().y * 0.5),
        title_galley.clone(),
        p.mix(p.fg_muted, p.fg, hover),
    );
    if (!open || always_summary) && !summary.is_empty() {
        let start = title_x + title_galley.size().x + 10.0;
        let available = right - start;
        if available > 32.0 {
            let measured = ui
                .painter()
                .layout_no_wrap(summary.to_string(), theme::font_mono(12.0), p.fg_muted)
                .size()
                .x;
            let galley = surfaces::truncated_galley(
                ui,
                summary.to_string(),
                theme::font_mono(12.0),
                p.fg_muted,
                available.min(measured),
            );
            ui.painter().galley(
                Pos2::new(right - galley.size().x, center_y - galley.size().y * 0.5),
                galley,
                p.fg_muted,
            );
        }
    }
    open
}

fn sections_open(ctx: &egui::Context) -> bool {
    SECTION_KEYS.iter().all(|key| section_open(ctx, key))
}

fn set_all_sections(ctx: &egui::Context, open: bool) {
    ctx.data_mut(|data| {
        for key in SECTION_KEYS {
            data.insert_temp(egui::Id::unique(("context-section", key)), open);
        }
    });
}

fn muted_note(ui: &mut Ui, p: &Palette, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(TEXT_X + 2.0);
        ui.label(
            RichText::new(text)
                .font(theme::font(12.0))
                .color(p.fg_muted),
        );
    });
}

fn row_columns(width: f32, inset: f32, measured: f32) -> Columns {
    let width = width.max(0.0);
    let value_budget = (width * 0.52)
        .clamp(VALUE_MIN, 300.0)
        .min((width - inset).max(VALUE_MIN));
    let value = measured.min(value_budget);
    let label = (width - inset - value - 8.0).max(LABEL_MIN);
    Columns { label, value }
}

fn row(
    ui: &mut Ui,
    p: &Palette,
    icon: Option<Icon>,
    label: &str,
    value: &str,
    tone: RowTone,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let center_y = rect.center().y;
    let mut x = rect.left() + 4.0;
    if let Some(icon) = icon {
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(
                Pos2::new(x + ICON_SIZE * 0.5, center_y),
                Vec2::splat(ICON_SIZE),
            ),
            match tone {
                RowTone::Metric => p.fg_muted,
                RowTone::Muted => p.fg_faint,
                RowTone::Placeholder => p.disabled,
            },
        );
        x += ICON_SIZE + 8.0;
    }
    let (label_color, value_color) = match tone {
        RowTone::Metric => (p.fg_muted, p.fg),
        RowTone::Muted => (p.fg_muted, p.fg_muted),
        RowTone::Placeholder => (p.fg_faint, p.fg_faint),
    };
    let value_font = theme::font_mono(12.0);
    let measured = if value.is_empty() {
        0.0
    } else {
        ui.painter()
            .layout_no_wrap(value.to_string(), value_font.clone(), value_color)
            .size()
            .x
    };
    let columns = row_columns(rect.width(), x - rect.left(), measured);
    let galley = surfaces::truncated_galley(
        ui,
        label.to_string(),
        theme::font(13.0),
        label_color,
        columns.label,
    );
    ui.painter().galley(
        Pos2::new(x, center_y - galley.size().y * 0.5),
        galley,
        label_color,
    );
    if !value.is_empty() {
        let galley = surfaces::truncated_galley(
            ui,
            value.to_string(),
            value_font,
            value_color,
            columns.value,
        );
        ui.painter().galley(
            Pos2::new(
                rect.right() - 2.0 - galley.size().x,
                center_y - galley.size().y * 0.5,
            ),
            galley,
            value_color,
        );
    }
    response
}

fn turn_metrics(chat: &Chat, app: &AppModel) -> TurnMetrics {
    let now = rchamber_app::now_ms();
    let last_user = chat.messages.iter().rposition(Message::is_user);
    let mut start = last_user.map(|index| index + 1).unwrap_or(0);
    let has_reply = chat
        .messages
        .get(start..)
        .is_some_and(|messages| messages.iter().any(|m| matches!(m, Message::Assistant(_))));
    if !has_reply {
        start = last_user
            .and_then(|index| {
                chat.messages[..index]
                    .iter()
                    .rposition(Message::is_user)
                    .map(|previous| previous + 1)
            })
            .unwrap_or(0);
    }
    let assistants: Vec<&AssistantMessage> = chat
        .messages
        .get(start..)
        .unwrap_or_default()
        .iter()
        .filter_map(|message| match message {
            Message::Assistant(assistant) => Some(assistant.as_ref()),
            _ => None,
        })
        .collect();
    let model = chat
        .model
        .as_ref()
        .and_then(|reference| app.model_for(reference))
        .map(|model| model.display_name().to_string())
        .or_else(|| chat.model.as_ref().map(|reference| reference.id.clone()))
        .unwrap_or_else(|| "—".into());
    let mut metrics = TurnMetrics {
        model,
        ..TurnMetrics::default()
    };
    if assistants.is_empty() {
        return metrics;
    }
    metrics.steps = assistants.len();
    let first = chat
        .messages
        .get(start.saturating_sub(1))
        .and_then(Message::created)
        .or_else(|| assistants.first().map(|assistant| assistant.time.created))
        .unwrap_or(0.0);
    let mut end = 0.0;
    let mut model_ms = 0.0;
    let mut tool_intervals = Vec::new();
    for assistant in &assistants {
        let step_end = assistant.time.completed.unwrap_or(now);
        if step_end > assistant.time.created {
            model_ms += step_end - assistant.time.created;
        }
        if step_end > end {
            end = step_end;
        }
        if let Some(tokens) = &assistant.tokens {
            metrics.tokens.input += tokens.input;
            metrics.tokens.output += tokens.output;
            metrics.tokens.reasoning += tokens.reasoning;
            metrics.tokens.cache.read += tokens.cache.read;
            metrics.tokens.cache.write += tokens.cache.write;
        }
        metrics.cost += assistant.cost;
        for part in &assistant.content {
            if let Part::Tool(tool) = part {
                let tool_start = tool.time.ran.unwrap_or(tool.time.created);
                let tool_end = tool.time.completed.unwrap_or(now);
                if tool_start > 0.0 && tool_end > tool_start {
                    tool_intervals.push((tool_start, tool_end));
                }
            }
        }
    }
    metrics.tools_ms = merged_ms(tool_intervals);
    if first > 0.0 && end > first {
        metrics.elapsed_ms = end - first;
    }
    let request_ms = (model_ms - metrics.tools_ms).max(0.0);
    metrics.request_speed = speed(metrics.tokens.output + metrics.tokens.reasoning, request_ms);
    if let Some(last) = assistants.last() {
        let last_end = last.time.completed.unwrap_or(now);
        let last_start = last.time.streamed.unwrap_or(last.time.created);
        if let Some(tokens) = &last.tokens {
            metrics.response_speed = speed(tokens.output, last_end - last_start);
        }
        if let Some(model) = last
            .model
            .as_ref()
            .and_then(|reference| app.model_for(reference))
            .map(|model| model.display_name().to_string())
            .or_else(|| last.model.as_ref().map(|reference| reference.id.clone()))
        {
            metrics.model = model;
        }
    }
    metrics.cache_hit = hit_rate(
        metrics.tokens.input,
        metrics.tokens.cache.read,
        metrics.tokens.cache.write,
    );
    metrics
}

fn speed(tokens: f64, ms: f64) -> Option<f64> {
    if tokens <= 0.0 || ms <= 0.0 {
        return None;
    }
    let rate = tokens / (ms / 1000.0);
    (rate.is_finite() && rate < 5_000.0).then_some(rate)
}

fn merged_ms(mut intervals: Vec<(f64, f64)>) -> f64 {
    intervals.retain(|(start, end)| *end > *start);
    intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut total = 0.0;
    let mut current: Option<(f64, f64)> = None;
    for (start, end) in intervals {
        current = match current {
            Some((open, close)) if start <= close => Some((open, close.max(end))),
            Some((open, close)) => {
                total += close - open;
                Some((start, end))
            }
            None => Some((start, end)),
        };
    }
    if let Some((start, end)) = current {
        total += end - start;
    }
    total
}

fn hit_rate(input: f64, read: f64, write: f64) -> Option<f64> {
    let total = input + read + write;
    if total <= 0.0 {
        return None;
    }
    Some((read / total).clamp(0.0, 1.0))
}

fn speed_text(rate: Option<f64>) -> String {
    match rate {
        Some(rate) if rate > 0.0 && rate.is_finite() => {
            format!("~{} tok/s", compact_number(rate.round()))
        }
        _ => "—".into(),
    }
}

fn non_empty(value: String) -> String {
    if value.is_empty() {
        "—".into()
    } else {
        value
    }
}

fn meter_color(fraction: f32, p: &Palette) -> Color32 {
    if fraction >= 0.8 {
        p.red
    } else if fraction >= 0.5 {
        p.coral
    } else {
        p.green
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_numbers_match_the_telemetry_card() {
        assert_eq!(quota_number(38_400.0), "38.4k");
        assert_eq!(quota_number(200_000.0), "200k");
        assert_eq!(quota_number(1_200.0), "1.2k");
        assert_eq!(quota_number(950.0), "950");
        assert_eq!(quota_number(1_250_000.0), "1.2M");
    }

    #[test]
    fn status_marks_cover_the_git_letters() {
        let p = Palette::dark();
        assert_eq!(status_mark("added", &p).0, "A");
        assert_eq!(status_mark("untracked", &p).0, "A");
        assert_eq!(status_mark("deleted", &p).0, "D");
        assert_eq!(status_mark("modified", &p).0, "M");
        assert_eq!(status_mark("renamed", &p).0, "R");
    }

    #[test]
    fn cache_hit_ignores_an_empty_window() {
        assert_eq!(hit_rate(0.0, 0.0, 0.0), None);
    }

    #[test]
    fn cache_hit_is_the_cached_share_of_input() {
        let hit = hit_rate(25.0, 75.0, 0.0).unwrap();
        assert!((hit - 0.75).abs() < 1e-6);
    }

    #[test]
    fn cache_hit_counts_cache_writes_as_input() {
        let hit = hit_rate(50.0, 25.0, 25.0).unwrap();
        assert!((hit - 0.25).abs() < 1e-6);
    }

    #[test]
    fn speed_needs_tokens_and_time() {
        assert_eq!(speed(0.0, 1000.0), None);
        assert_eq!(speed(10.0, 0.0), None);
        assert!((speed(100.0, 1000.0).unwrap() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn implausible_speeds_are_unknown() {
        assert_eq!(speed(10_000.0, 1000.0), None);
    }

    #[test]
    fn overlapping_tool_intervals_count_once() {
        assert_eq!(merged_ms(vec![(0.0, 10.0), (5.0, 15.0)]), 15.0);
        assert_eq!(merged_ms(vec![(0.0, 5.0), (10.0, 12.0)]), 7.0);
        assert_eq!(merged_ms(Vec::new()), 0.0);
    }

    #[test]
    fn speed_text_matches_the_collapsed_summary() {
        assert_eq!(speed_text(Some(174.2)), "~174 tok/s");
        assert_eq!(speed_text(None), "—");
    }

    #[test]
    fn panel_state_prefers_an_open_session() {
        assert_eq!(
            panel_state(true, &Connection::Connecting),
            PanelState::Ready
        );
        assert_eq!(
            panel_state(true, &Connection::Failed("offline".into())),
            PanelState::Ready
        );
    }

    #[test]
    fn panel_state_reports_connection_progress_without_a_session() {
        assert_eq!(
            panel_state(false, &Connection::Connecting),
            PanelState::Loading
        );
        assert_eq!(
            panel_state(false, &Connection::Reconnecting("retry".into())),
            PanelState::Loading
        );
    }

    #[test]
    fn panel_state_falls_back_to_copy_for_idle_and_failed_connections() {
        assert_eq!(panel_state(false, &Connection::Ready), PanelState::Empty);
        assert_eq!(
            panel_state(false, &Connection::Disconnected),
            PanelState::Empty
        );
        assert_eq!(
            panel_state(false, &Connection::Failed("offline".into())),
            PanelState::Failed
        );
    }

    #[test]
    fn loading_labels_distinguish_reconnects() {
        assert_eq!(loading_label(&Connection::Connecting), "Loading…");
        assert_eq!(
            loading_label(&Connection::Reconnecting("retry".into())),
            "Reconnecting…"
        );
        assert!(!EMPTY_TITLE.is_empty() && !EMPTY_HINT.is_empty());
    }

    #[test]
    fn turn_metrics_start_from_placeholders() {
        let rows = metric_rows(&TurnMetrics::default());
        let labels: Vec<&str> = rows.iter().map(|(label, _)| *label).collect();
        assert_eq!(
            labels,
            [
                "Response", "Request", "Elapsed", "Model", "Tools", "Steps", "Tokens", "Cache",
                "Cost",
            ]
        );
        assert_eq!(rows[0].1, "—");
        assert_eq!(rows[1].1, "—");
        assert_eq!(rows[2].1, "—");
        assert_eq!(rows[3].1, "—");
        assert_eq!(rows[4].1, "—");
        assert_eq!(rows[5].1, "0");
        assert_eq!(rows[6].1, "0 ↑ · 0 ↓");
        assert_eq!(rows[7].1, "—");
        assert_eq!(rows[8].1, "$0.00");
    }

    #[test]
    fn empty_token_usage_keeps_the_arrow_layout() {
        assert_eq!(tokens_text(&TokenUsage::default()), "0 ↑ · 0 ↓");
    }

    #[test]
    fn rows_keep_values_inside_the_reference_widths() {
        for width in [296.0_f32, 696.0] {
            let columns = row_columns(width, TEXT_X, 120.0);
            assert!(columns.value >= VALUE_MIN);
            assert!(columns.value <= 300.0);
            assert!(columns.label >= LABEL_MIN);
            assert!(columns.label + columns.value + TEXT_X <= width);
        }
    }

    #[test]
    fn long_values_still_leave_a_label_budget() {
        let columns = row_columns(296.0, TEXT_X, 10_000.0);
        assert!(columns.value <= 296.0 * 0.52);
        assert!(columns.label >= LABEL_MIN);
    }

    #[test]
    fn undersized_widths_clamp_to_readable_columns() {
        let columns = row_columns(40.0, TEXT_X, 10_000.0);
        assert_eq!(columns.label, LABEL_MIN);
        assert_eq!(columns.value, VALUE_MIN);
    }

    #[test]
    fn context_fraction_stays_within_the_meter() {
        assert_eq!(fraction_used(0.0, 200_000.0), 0.0);
        assert_eq!(fraction_used(100_000.0, 200_000.0), 0.5);
        assert_eq!(fraction_used(400_000.0, 200_000.0), 1.0);
        assert_eq!(fraction_used(10.0, 0.0), 0.0);
    }

    #[test]
    fn turn_stats_open_by_default() {
        assert!(section_default("turn-stats"));
        assert!(!section_default("subagents"));
        assert!(!section_default("mcp"));
        assert!(!section_default("sources"));
    }

    fn server(name: &str, status: &str) -> McpServer {
        McpServer {
            name: name.to_string(),
            status: rchamber_api::models::McpServerStatus {
                status: status.to_string(),
            },
        }
    }

    #[test]
    fn mcp_count_is_empty_without_servers() {
        assert_eq!(mcp_summary(&[]), "—");
    }

    #[test]
    fn mcp_count_reports_connected_servers() {
        assert_eq!(mcp_summary(&[server("rea", "connected")]), "1/1");
        assert_eq!(
            mcp_summary(&[server("rea", "connected"), server("fetch", "failed")]),
            "1/2"
        );
        assert_eq!(
            mcp_summary(&[server("rea", "failed"), server("fetch", "disconnected")]),
            "0/2"
        );
    }
}
