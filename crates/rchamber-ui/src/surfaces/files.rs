use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::text::{CCursor, CCursorRange, CharIndex, LayoutJob};
use eframe::egui::widgets::text_edit::TextEditState;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Frame, Id, Layout, Margin, Pos2, Rangef, Rect,
    Response, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2,
};
use rchamber_api::models::FileEntry;
use rchamber_app::state::FilesState;
use rchamber_app::{Action, Backend, Connection, ToastKind};

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::markdown::Markdown;
use crate::surfaces::{self, PanelTab};
use crate::theme::{self, Palette};
use crate::widgets;

const TREE_WIDTH: f32 = 240.0;
const TREE_MIN_WIDTH: f32 = 168.0;
const TREE_MAX_WIDTH: f32 = 320.0;
const TREE_ROW_HEIGHT: f32 = 28.0;
const INDENT: f32 = 12.0;
const END_LANE: f32 = 20.0;
const TOOLBAR_HEIGHT: f32 = 32.0;
const FIND_HEIGHT: f32 = 36.0;
const CODE_ROW_HEIGHT: f32 = 19.0;
const CODE_FONT: f32 = 12.0;
const CODE_PAD_X: f32 = 14.0;
const CODE_PAD_TOP: f32 = 8.0;
const CODE_PAD_BOTTOM: f32 = 12.0;
const MIN_GUTTER: f32 = 44.0;
const STATUS_SLOT: f32 = 72.0;
const SAVE_FLASH: f64 = 2.4;
const MAX_EDIT_BYTES: usize = 512 * 1024;
const SEARCH_DEBOUNCE: f64 = 0.25;
const SEARCH_MIN_CHARS: usize = 2;
const LOAD_TIMEOUT: f64 = 6.0;
const MAX_TEXT_BYTES: usize = 10 * 1024 * 1024;
const MAX_LINE_CHARS: usize = 4000;
const MAX_VIEW_LINES: usize = 20_000;
const MARKDOWN_PREVIEW_BYTES: usize = 260_000;
const MAX_OPEN_TABS: usize = 50;
const FIND_MAX_MATCHES: usize = 2000;

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let root = app.workspace_directory();
    let ctx = ui.ctx().clone();

    if !app.model.files.tree.contains_key(&root) {
        request_directory(&ctx, &app.backend, &root, false);
    }

    let mut close = false;
    let mut toggle_expanded = false;
    let expanded = app.ui.expanded;

    let tabs: Vec<PanelTab> = if app.model.files.open_tabs.is_empty() {
        vec![PanelTab::new("Files", Icon::File).active(true)]
    } else {
        let dirty = dirty_paths(&ctx);
        app.model
            .files
            .open_tabs
            .iter()
            .map(|path| {
                let mut label = file_name(path);
                if dirty.contains(path) {
                    label.insert_str(0, "● ");
                }
                PanelTab::new(label, Icon::File)
                    .active(app.model.files.active_tab.as_deref() == Some(path.as_str()))
                    .closable(true)
            })
            .collect()
    };
    let show_tree = tree_visible(ui, ui.available_width());
    let mut toggle_tree = false;
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
            if surfaces::panel_button(
                ui,
                &p,
                Icon::Sidebar,
                if show_tree {
                    "Hide file tree"
                } else {
                    "Show file tree"
                },
            )
            .clicked()
            {
                toggle_tree = true;
            }
        },
    );
    if toggle_tree {
        ui.memory_mut(|memory| {
            memory
                .data
                .insert_temp(Id::unique("files-tree-visible"), !show_tree)
        });
    }

    if close {
        app.ui.context_open = false;
    }
    if toggle_expanded {
        app.ui.expanded = !app.ui.expanded;
    }
    if !app.model.files.open_tabs.is_empty() {
        if let Some(index) = header.select
            && let Some(path) = app.model.files.open_tabs.get(index).cloned()
        {
            app.model.files.active_tab = Some(path.clone());
            request_file(&ctx, &app.backend, &path, false);
        }
        if let Some(index) = header.close
            && let Some(path) = app.model.files.open_tabs.get(index).cloned()
        {
            close_tab_animated(&ctx, &mut app.model.files, &path);
        }
        if let Some((from, to)) = header.reorder {
            surfaces::move_item(&mut app.model.files.open_tabs, from, to);
        }
    }

    let body = ui.available_rect_before_wrap();
    if body.height() < 20.0 {
        return;
    }
    ui.allocate_rect(body, Sense::hover());
    let show_tree = tree_visible(ui, body.width());
    let tree_width = if show_tree {
        tree_width(ui, body.width())
    } else {
        0.0
    };
    if show_tree {
        let tree_rect = Rect::from_min_max(
            body.min,
            egui::Pos2::new(body.left() + tree_width, body.bottom()),
        );
        let mut tree_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(tree_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        tree_panel(&mut tree_ui, app, &p, &root, &ctx);
        separator(ui, &p, body, tree_width);
        let viewer_rect = Rect::from_min_max(
            egui::Pos2::new(body.left() + tree_width, body.top()),
            body.max,
        );
        let mut viewer_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(viewer_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        viewer(&mut viewer_ui, app, &p, &root, &ctx, show_tree);
    } else {
        let mut viewer_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body)
                .layout(Layout::top_down(Align::Min)),
        );
        viewer(&mut viewer_ui, app, &p, &root, &ctx, show_tree);
    }
}

fn tree_visible(ui: &Ui, width: f32) -> bool {
    let id = Id::unique("files-tree-visible");
    let stored = ui.memory(|memory| memory.data.get_temp::<bool>(id));
    stored.unwrap_or(width >= 520.0)
}

fn tree_width(ui: &Ui, available: f32) -> f32 {
    let id = Id::unique("files-tree-width");
    let stored = ui
        .memory(|memory| memory.data.get_temp::<f32>(id))
        .unwrap_or(TREE_WIDTH);
    stored
        .clamp(TREE_MIN_WIDTH, TREE_MAX_WIDTH)
        .min((available * 0.55).max(TREE_MIN_WIDTH))
}

fn separator(ui: &mut Ui, p: &Palette, body: Rect, tree_width: f32) {
    let x = body.left() + tree_width;
    ui.painter().vline(
        x,
        Rangef::new(body.top(), body.bottom()),
        Stroke::new(0.5, surfaces::edge(p)),
    );
    let handle = Rect::from_min_max(
        egui::Pos2::new(x - 3.0, body.top()),
        egui::Pos2::new(x + 3.0, body.bottom()),
    );
    let response = ui.interact(handle, Id::unique("files-tree-resize"), Sense::drag());
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        ui.painter().rect_filled(
            Rect::from_min_max(
                egui::Pos2::new(x - 1.0, body.top()),
                egui::Pos2::new(x + 1.0, body.bottom()),
            ),
            0.0,
            p.border_hover,
        );
    }
    if response.dragged() {
        let id = Id::unique("files-tree-width");
        let next = (tree_width + response.drag_delta().x).clamp(TREE_MIN_WIDTH, TREE_MAX_WIDTH);
        ui.memory_mut(|memory| memory.data.insert_temp(id, next));
    }
}

fn tree_panel(ui: &mut Ui, app: &mut RustChamber, p: &Palette, root: &str, ctx: &egui::Context) {
    let connected = matches!(app.model.connection, Connection::Ready);
    let failed = match &app.model.connection {
        Connection::Failed(reason) => Some(reason.clone()),
        _ => None,
    };
    let marks = git_marks(&app.model, p);
    let backend = &app.backend;
    let files = &mut app.model.files;

    ui.add_space(8.0);
    search_field_row(ui, files, backend, p, root);
    ui.add_space(8.0);

    let query = files.search_query.trim().to_string();
    let chars = query.chars().count();

    if chars >= SEARCH_MIN_CHARS {
        search_results(ui, files, backend, ctx, p, &query, connected);
        return;
    }
    if chars == 1 {
        surfaces::inline_empty(ui, p, "Type one more character to search");
        return;
    }
    root_header(ui, files, p, root, &marks);

    if !files.tree.contains_key(root) {
        if connected {
            request_directory(ctx, backend, root, false);
        }
        if let Some(reason) = failed {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 8.0).max(80.0));
                    if surfaces::error_banner(ui, p, &reason, "Retry") {
                        request_directory(ctx, backend, root, true);
                    }
                });
            });
        } else {
            surfaces::loading_state(
                ui,
                p,
                if connected {
                    "Reading workspace…"
                } else {
                    "Waiting for OpenCode…"
                },
            );
        }
        return;
    }

    let rows = collect_rows(files, root, ctx, backend, connected);

    if rows.is_empty() {
        surfaces::inline_empty(ui, p, "This folder is empty");
        return;
    }

    let mut toggle: Option<(String, bool)> = None;
    let mut open: Option<(String, bool)> = None;

    ScrollArea::vertical()
        .id_salt(("files-tree-rows", root))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 8.0).max(80.0));
                    for row in &rows {
                        let selected = !row.directory
                            && files.active_tab.as_deref() == Some(row.path.as_str());
                        let open_tab =
                            !row.directory && files.open_tabs.iter().any(|tab| tab == &row.path);
                        let mark = marks.get(row.path.as_str()).copied();
                        let response = tree_row(ui, p, row, selected, open_tab, mark);
                        if response.clicked() {
                            if row.directory {
                                toggle = Some((row.path.clone(), !row.open));
                            } else {
                                open = Some((row.path.clone(), false));
                            }
                        }
                        if response.double_clicked() && !row.directory {
                            open = Some((row.path.clone(), true));
                        }
                    }
                    ui.add_space(10.0);
                });
            });
        });

    if let Some((path, expand)) = toggle {
        if expand {
            files.expanded.insert(path.clone());
            if !files.tree.contains_key(&path) {
                request_directory(ctx, backend, &path, true);
            }
        } else {
            files.expanded.remove(&path);
        }
    }

    if let Some((path, pinned)) = open {
        open_file(ctx, files, backend, &path, pinned);
    }
}

fn search_field_row(
    ui: &mut Ui,
    files: &mut FilesState,
    backend: &Backend,
    p: &Palette,
    root: &str,
) {
    let field_id = Id::unique("files-search-field");
    let (row, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), widgets::INPUT_HEIGHT),
        Sense::hover(),
    );
    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                egui::Pos2::new(row.left() + 8.0, row.top()),
                egui::Pos2::new(row.right() - 8.0, row.bottom()),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row_ui.spacing_mut().item_spacing.x = 4.0;
    let field_width = (row_ui.available_width() - 32.0).max(80.0);
    let start = row_ui.cursor().min;
    let mut field_ui = row_ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                start,
                Vec2::new(field_width, widgets::INPUT_HEIGHT),
            ))
            .layout(Layout::top_down(Align::Min)),
    );
    let changed = widgets::search_field(
        &mut field_ui,
        &mut files.search_query,
        "Search files",
        p,
        field_id,
    );
    row_ui.add_space(field_width + 4.0);
    if surfaces::panel_button(&mut row_ui, p, Icon::Refresh, "Refresh tree").clicked() {
        backend.send(Action::LoadDirectory(root.to_string()));
    }
    dispatch_search(&mut row_ui, files, backend, changed);
}

fn dispatch_search(ui: &mut Ui, files: &mut FilesState, backend: &Backend, changed: bool) {
    let now = ui.input(|input| input.time);
    let stamp_id = Id::unique("files-search-stamp");
    let sent_id = Id::unique("files-search-sent");

    if changed {
        ui.memory_mut(|memory| memory.data.insert_temp(stamp_id, now));
    }

    let query = files.search_query.trim().to_string();
    if query.chars().count() < SEARCH_MIN_CHARS {
        if changed && query.is_empty() {
            files.search.clear();
        }
        ui.memory_mut(|memory| memory.data.insert_temp(sent_id, String::new()));
        return;
    }

    let stamp = ui
        .memory(|memory| memory.data.get_temp::<f64>(stamp_id))
        .unwrap_or(now);
    let sent = ui
        .memory(|memory| memory.data.get_temp::<String>(sent_id))
        .unwrap_or_default();

    if sent == query {
        return;
    }

    if now - stamp >= SEARCH_DEBOUNCE {
        backend.send(Action::SearchFiles(query.clone()));
        ui.memory_mut(|memory| memory.data.insert_temp(sent_id, query));
    } else {
        ui.ctx().request_repaint_after(Duration::from_millis(80));
    }
}

fn root_header(
    ui: &mut Ui,
    files: &FilesState,
    p: &Palette,
    root: &str,
    marks: &HashMap<String, (char, Color32)>,
) {
    let name = Path::new(root)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| root.to_string());
    let count = files.tree.get(root).map(Vec::len).unwrap_or(0);
    let modified = marks.values().filter(|(letter, _)| *letter == 'M').count();

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    let x = rect.left() + 8.0;
    icons::paint(
        ui,
        Icon::Folder,
        Rect::from_center_size(egui::Pos2::new(x + 7.0, rect.center().y), Vec2::splat(14.0)),
        p.fg_muted,
    );
    let galley = surfaces::truncated_galley(
        ui,
        name,
        theme::font_medium(13.0),
        p.fg,
        (rect.width() - 72.0).max(24.0),
    );
    ui.painter().galley(
        egui::Pos2::new(x + 20.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg,
    );
    let mut right = rect.right() - 8.0;
    if count > 0 {
        let label = count.to_string();
        let galley = ui
            .painter()
            .layout_no_wrap(label.clone(), theme::font(12.0), p.fg_muted);
        right -= galley.size().x;
        ui.painter().galley(
            egui::Pos2::new(right, rect.center().y - galley.size().y * 0.5),
            galley,
            p.fg_muted,
        );
        right -= 8.0;
    }
    if modified > 0 {
        let label = format!("M{modified}");
        let galley = ui
            .painter()
            .layout_no_wrap(label, theme::font(12.0), p.coral);
        right -= galley.size().x;
        ui.painter().galley(
            egui::Pos2::new(right, rect.center().y - galley.size().y * 0.5),
            galley,
            p.coral,
        );
    }
    response.on_hover_text(root);
    surfaces::hairline(ui, p);
}

fn tree_row(
    ui: &mut Ui,
    p: &Palette,
    row: &TreeRow,
    selected: bool,
    open_tab: bool,
    mark: Option<(char, Color32)>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TREE_ROW_HEIGHT),
        Sense::click(),
    );
    if !ui.is_rect_visible(rect) {
        return response;
    }
    for level in 0..row.depth {
        let x = rect.left() + 8.0 + level as f32 * INDENT + 7.0;
        ui.painter().vline(
            x,
            Rangef::new(rect.top() + 2.0, rect.bottom() - 2.0),
            Stroke::new(0.5, surfaces::edge_muted(p)),
        );
    }
    let hover = crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.selection);
    } else if hover > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.hover.gamma_multiply(hover),
        );
    }

    let x = rect.left() + 8.0 + row.depth as f32 * INDENT;
    let color = if selected { p.selection_fg } else { p.fg };
    if row.directory {
        let open = row.open;
        icons::paint(
            ui,
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            Rect::from_center_size(egui::Pos2::new(x + 7.0, rect.center().y), Vec2::splat(14.0)),
            if response.hovered() { p.fg } else { p.fg_muted },
        );
        let galley = surfaces::truncated_galley(
            ui,
            row.name.clone(),
            theme::font_medium(13.0),
            color,
            (rect.right() - x - 20.0 - END_LANE).max(20.0),
        );
        ui.painter().galley(
            egui::Pos2::new(x + 22.0, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
    } else {
        let (icon, tint) = file_visual(&row.name, p);
        icons::paint(
            ui,
            icon,
            Rect::from_center_size(egui::Pos2::new(x + 9.0, rect.center().y), Vec2::splat(14.0)),
            tint,
        );
        let galley = surfaces::truncated_galley(
            ui,
            row.name.clone(),
            theme::font(12.5),
            color,
            (rect.right() - x - 22.0 - END_LANE - 20.0).max(20.0),
        );
        ui.painter().galley(
            egui::Pos2::new(x + 22.0, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
    }

    let lane = rect.right() - END_LANE * 0.5;
    if open_tab {
        ui.painter()
            .circle_filled(egui::Pos2::new(lane - 12.0, rect.center().y), 3.0, p.blue);
    }
    if let Some((letter, tint)) = mark.filter(|_| !row.directory) {
        ui.painter().text(
            egui::Pos2::new(lane + 6.0, rect.center().y),
            Align2::CENTER_CENTER,
            letter,
            theme::font_semibold(12.0),
            tint,
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
    }
    response
}

fn search_results(
    ui: &mut Ui,
    files: &mut FilesState,
    backend: &Backend,
    ctx: &egui::Context,
    p: &Palette,
    query: &str,
    connected: bool,
) {
    let sent = ui
        .memory(|memory| {
            memory
                .data
                .get_temp::<String>(Id::unique("files-search-sent"))
        })
        .unwrap_or_default();
    let dispatched = sent == query;
    let results: Vec<FileEntry> = files
        .search
        .iter()
        .filter(|entry| !is_directory(entry))
        .cloned()
        .collect();

    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
    surfaces::section_label_at(ui, rect, "Results", p);
    ui.painter().text(
        egui::Pos2::new(rect.right() - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        results.len().to_string(),
        theme::font(12.0),
        p.fg_muted,
    );
    surfaces::hairline(ui, p);

    if results.is_empty() {
        if !connected {
            surfaces::inline_empty(ui, p, "Waiting for OpenCode…");
        } else if dispatched {
            surfaces::inline_empty(ui, p, "No matching files");
        } else {
            surfaces::loading_state(ui, p, "Searching…");
        }
        return;
    }

    let mut open: Option<(String, bool)> = None;

    ScrollArea::vertical()
        .id_salt(("files-search-results", query))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 8.0).max(80.0));
                    for entry in &results {
                        let active = files.active_tab.as_deref() == Some(entry.path.as_str());
                        let response = search_row(ui, p, &entry.path, &entry.path, active);
                        if response.clicked() {
                            open = Some((entry.path.clone(), false));
                        }
                        if response.double_clicked() {
                            open = Some((entry.path.clone(), true));
                        }
                    }
                });
            });
        });

    if let Some((path, pinned)) = open {
        open_file(ctx, files, backend, &path, pinned);
    }
}

fn search_row(ui: &mut Ui, p: &Palette, label: &str, path: &str, active: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let hover = crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if active {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.selection);
    } else if hover > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.hover.gamma_multiply(hover),
        );
    }
    let name = file_name(path);
    let (icon, tint) = file_visual(&name, p);
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(
            egui::Pos2::new(rect.left() + 16.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        tint,
    );
    let galley = surfaces::truncated_galley(
        ui,
        label.to_string(),
        theme::font(12.5),
        if active { p.selection_fg } else { p.fg },
        (rect.width() - 34.0).max(20.0),
    );
    ui.painter().galley(
        egui::Pos2::new(rect.left() + 30.0, rect.center().y - galley.size().y * 0.5),
        galley,
        if active { p.selection_fg } else { p.fg },
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
    }
    response.on_hover_text(path)
}

fn git_marks(model: &rchamber_app::AppModel, p: &Palette) -> HashMap<String, (char, Color32)> {
    let mut marks = HashMap::new();
    if let Some(chat) = &model.active {
        for file in &chat.diff {
            let (letter, color) = match file.status.as_deref().unwrap_or("modified") {
                "added" | "untracked" => ('A', p.green),
                "deleted" | "removed" => ('D', p.red),
                "renamed" => ('R', p.blue),
                _ => ('M', p.coral),
            };
            marks.insert(file.file.clone(), (letter, color));
            if let Some((_, name)) = file.file.rsplit_once('/') {
                marks.entry(name.to_string()).or_insert((letter, color));
            }
        }
    }
    marks
}

fn viewer(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    root: &str,
    ctx: &egui::Context,
    show_tree: bool,
) {
    let location = app
        .model
        .workspace
        .clone()
        .unwrap_or_else(|| root.to_string());
    let surface = ui.available_rect_before_wrap();
    let over = ui.rect_contains_pointer(surface);

    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::W))
        && let Some(path) = app.model.files.active_tab.clone()
    {
        close_tab_animated(ctx, &mut app.model.files, &path);
    }
    let find_open_id = Id::unique("files-find-open");
    let find_before = ctx
        .memory(|memory| memory.data.get_temp::<bool>(find_open_id))
        .unwrap_or(false);
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F))
        && (over || !show_tree)
    {
        ctx.memory_mut(|memory| memory.data.insert_temp(find_open_id, !find_before));
    }

    if app.model.files.open_tabs.is_empty() {
        app.model.files.active_tab = None;
        surfaces::empty_state(
            ui,
            p,
            Icon::File,
            "Select a file",
            "Pick a file from the tree, or search the workspace.",
        );
        return;
    }

    if app.model.files.active_tab.is_none() {
        app.model.files.active_tab = app.model.files.open_tabs.last().cloned();
    }

    let Some(active) = app
        .model
        .files
        .active_tab
        .clone()
        .or_else(|| app.model.files.open_tabs.first().cloned())
    else {
        surfaces::empty_state(ui, p, Icon::File, "Select a file", "");
        return;
    };

    let loaded = app
        .model
        .files
        .preview
        .as_ref()
        .is_some_and(|(path, _)| path == &active);
    let loaded_text = app
        .model
        .files
        .preview
        .as_ref()
        .filter(|(path, _)| path == &active)
        .map(|(_, text)| text.as_str());

    if let Some(text) = loaded_text {
        if text.len() > MAX_TEXT_BYTES {
            large_card(ui, p, &location, &active, text.len());
            return;
        }
        if binary_preview(&active, text) {
            binary_card(ui, p, &location, &active, text.len());
            return;
        }
    }

    let mut save_error: Option<String> = None;
    let mut buffers = EditorBuffers::take(ctx);
    let writable_file = writable(&location, &active);
    if !buffers.map.contains_key(&active) {
        let mut buffer = EditorBuffer::new(
            loaded_text.unwrap_or_default().to_string(),
            writable_file,
            loaded_text.is_some(),
        );
        if let Some(text) = loaded_text {
            buffer.width_hint = estimate_width(ui, text);
        }
        buffers.map.insert(active.clone(), buffer);
    } else if let Some(text) = loaded_text
        && let Some(buffer) = buffers.map.get_mut(&active)
    {
        buffer.sync(text);
    }
    let Some(buffer) = buffers.map.get_mut(&active) else {
        return;
    };
    if let Some(text) = loaded_text
        && !buffer.dirty()
    {
        buffer.sync_large(text);
    }
    let large = buffer.large;
    let editor_id = Id::unique(("files-editor", &active));
    let find_field_id = Id::unique(("files-find", &active));
    let focused =
        ctx.memory(|memory| memory.has_focus(editor_id) || memory.has_focus(find_field_id));
    let save_shortcut = (over || focused)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
    let preview = is_markdown(&active) && preview_mode(ctx, &active, buffer.text.len());
    let ready = loaded || buffer.loaded;

    let toolbar = toolbar(ui, p, ctx, &location, &active, buffer, preview, ready);
    let mut reload = toolbar.reload;
    let save = toolbar.save || save_shortcut;
    if toolbar.revert {
        buffer.revert();
        reset_editor_state(ctx, editor_id);
    }
    if reload && buffer.dirty() {
        open_reload_confirm(ctx);
        reload = false;
    }

    if !loaded {
        request_file(ctx, &app.backend, &active, false);
        if !buffer.loaded {
            loading_file(ui, p, ctx, &app.backend, &active, &location);
            return;
        }
    }

    let find_open = ctx
        .memory(|memory| memory.data.get_temp::<bool>(find_open_id))
        .unwrap_or(false);
    if find_open && !find_before {
        ctx.memory_mut(|memory| memory.request_focus(find_field_id));
    }
    if find_open {
        find_bar(ui, p, ctx, &active, &buffer.text, buffer.revision);
    } else {
        ctx.memory_mut(|memory| {
            memory
                .data
                .remove::<Option<usize>>(Id::unique("files-find-active-line"));
        });
    }

    body_view(ui, p, &mut app.md, ctx, &active, buffer, large);

    let confirm_id = Id::unique("files-reload-confirm");
    let confirm = ctx
        .memory(|memory| memory.data.get_temp::<bool>(confirm_id))
        .unwrap_or(false);
    let mut reload_after = false;
    let mut drop_after = false;
    if confirm {
        let choice = reload_dialog(ui, p, surface, &file_name(&active), buffer.dirty());
        match choice {
            Some(ReloadChoice::Cancel) => close_reload_confirm(ctx),
            Some(ReloadChoice::Save) => {
                if buffer.read_only() {
                    close_reload_confirm(ctx);
                } else {
                    let path = local_path(&location, &active);
                    match buffer.save(&path, ctx.input(|input| input.time)) {
                        Ok(()) => {
                            adopt_saved_text(app, &active, &buffer.text);
                            reload_after = true;
                            drop_after = true;
                            close_reload_confirm(ctx);
                        }
                        Err(error) => {
                            buffer.error = Some(error.clone());
                            save_error = Some(error);
                        }
                    }
                }
            }
            Some(ReloadChoice::Discard) => {
                reload_after = true;
                drop_after = true;
                close_reload_confirm(ctx);
            }
            None => {}
        }
    }

    if save && !buffer.read_only() && buffer.dirty() {
        let path = local_path(&location, &active);
        match buffer.save(&path, ctx.input(|input| input.time)) {
            Ok(()) => {
                adopt_saved_text(app, &active, &buffer.text);
                ctx.request_repaint();
            }
            Err(error) => {
                buffer.error = Some(error.clone());
                save_error = Some(error);
            }
        }
    }
    if drop_after {
        buffers.map.remove(&active);
    }
    if reload || reload_after {
        request_file(ctx, &app.backend, &active, true);
    }

    if let Some(error) = save_error {
        app.model.toast(
            ToastKind::Error,
            format!("Could not save {}: {error}", file_name(&active)),
        );
    }
}

fn adopt_saved_text(app: &mut RustChamber, path: &str, text: &str) {
    if let Some((saved, content)) = app.model.files.preview.as_mut()
        && saved == path
    {
        *content = text.to_string();
    }
}

fn reset_editor_state(ctx: &egui::Context, id: Id) {
    if let Some(mut state) = TextEditState::load(ctx, id) {
        state.clear_undoer();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(0))));
        state.store(ctx, id);
    }
    ctx.memory_mut(|memory| memory.request_focus(id));
}

fn find_bar(ui: &mut Ui, p: &Palette, ctx: &egui::Context, path: &str, text: &str, revision: u64) {
    let query_id = Id::unique("files-find-query");
    let index_id = Id::unique("files-find-index");
    let mut query = ctx
        .memory(|memory| memory.data.get_temp::<String>(query_id))
        .unwrap_or_default();
    let mut index = ctx
        .memory(|memory| memory.data.get_temp::<usize>(index_id))
        .unwrap_or(0);

    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), FIND_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.bg_soft);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.25,
        Stroke::new(0.5, surfaces::edge_muted(p)),
    );
    let matches = {
        let id = Id::unique((
            "files-find-matches",
            path,
            query.clone(),
            text.len(),
            revision,
        ));
        if let Some(matches) = ctx.memory(|memory| memory.data.get_temp::<Vec<usize>>(id)) {
            matches
        } else {
            let matches = find_matches(text, &query);
            ctx.memory_mut(|memory| memory.data.insert_temp(id, matches.clone()));
            matches
        }
    };
    if index >= matches.len() {
        index = 0;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                egui::Pos2::new(rect.left() + 8.0, rect.top() + 4.0),
                egui::Pos2::new(rect.right() - 8.0, rect.bottom() - 4.0),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 6.0;
    let field_width = (child.available_width() - 150.0).max(80.0);
    let mut field_ui = child.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                child.cursor().min,
                Vec2::new(field_width, widgets::INPUT_HEIGHT - 4.0),
            ))
            .layout(Layout::top_down(Align::Min)),
    );
    let changed = widgets::search_field(
        &mut field_ui,
        &mut query,
        "Find in file",
        p,
        Id::unique(("files-find", path)),
    );
    child.add_space(field_width + 6.0);
    if changed {
        index = 0;
    }
    let position = if matches.is_empty() {
        if query.is_empty() {
            "0/0".to_string()
        } else {
            "No matches".to_string()
        }
    } else {
        format!("{}/{}", index + 1, matches.len())
    };
    child.label(
        RichText::new(position)
            .font(theme::font(12.0))
            .color(p.fg_muted),
    );
    if surfaces::mini_icon_button(&mut child, p, Icon::ArrowUp, "Previous match").clicked()
        && !matches.is_empty()
    {
        index = (index + matches.len() - 1) % matches.len();
    }
    if surfaces::mini_icon_button(&mut child, p, Icon::ArrowDown, "Next match").clicked()
        && !matches.is_empty()
    {
        index = (index + 1) % matches.len();
    }
    if surfaces::mini_icon_button(&mut child, p, Icon::Close, "Close find").clicked() {
        ctx.memory_mut(|memory| {
            memory
                .data
                .insert_temp(Id::unique("files-find-open"), false)
        });
    }
    let field_id = Id::unique(("files-find", path));
    let focused = ctx.memory(|memory| memory.has_focus(field_id));
    if focused && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        ctx.memory_mut(|memory| {
            memory
                .data
                .insert_temp(Id::unique("files-find-open"), false)
        });
        ctx.memory_mut(|memory| memory.stop_text_input());
    }
    let enter = focused && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if enter && !matches.is_empty() {
        index = (index + 1) % matches.len();
    }
    ctx.memory_mut(|memory| {
        memory.data.insert_temp(query_id, query);
        memory.data.insert_temp(index_id, index);
        memory.data.insert_temp(
            Id::unique("files-find-active-line"),
            matches.get(index).copied(),
        );
    });
    if changed || enter {
        ctx.request_repaint();
    }
}

fn find_matches(text: &str, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle = query.to_lowercase();
    let mut lines = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        if lines.len() >= FIND_MAX_MATCHES {
            break;
        }
        if line.to_lowercase().contains(&needle) {
            lines.push(index);
        }
    }
    lines
}

fn body_view(
    ui: &mut Ui,
    p: &Palette,
    md: &mut Markdown,
    ctx: &egui::Context,
    path: &str,
    buffer: &mut EditorBuffer,
    large: bool,
) {
    if is_markdown(path) && preview_mode(ctx, path, buffer.text.len()) {
        markdown_view(ui, md, p, path, &buffer.text);
    } else if large {
        large_file_note(ui, p);
        code_view(ui, p, path, &buffer.text);
    } else {
        editor_view(ui, p, ctx, path, buffer);
    }
}

fn large_file_note(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(
        Pos2::new(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Large file · read-only preview",
        theme::font(12.0),
        p.fg_muted,
    );
    surfaces::hairline(ui, p);
}

fn markdown_view(ui: &mut Ui, md: &mut Markdown, p: &Palette, path: &str, text: &str) {
    ScrollArea::vertical()
        .id_salt(("files-md-scroll", path))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let available = ui.available_width();
            let inner = available.clamp(240.0, 760.0);
            let side = ((available - inner) * 0.5).max(12.0);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_max_width(inner);
                    md.show_at(ui, Id::unique(("files-md", path)), text, p);
                });
            });
            ui.add_space(24.0);
        });
}

fn loading_file(
    ui: &mut Ui,
    p: &Palette,
    ctx: &egui::Context,
    backend: &Backend,
    path: &str,
    root: &str,
) {
    let started = ctx.memory(|memory| {
        memory
            .data
            .get_temp::<f64>(Id::unique(("files-load", path)))
    });
    let elapsed = started
        .map(|at| ctx.input(|input| input.time) - at)
        .unwrap_or(0.0);

    ctx.request_repaint_after(Duration::from_millis(90));

    if elapsed < LOAD_TIMEOUT {
        surfaces::loading_state(ui, p, "Loading file…");
        return;
    }
    let top = (ui.available_height() * 0.3).clamp(24.0, 180.0);
    ui.add_space(top);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new("Preview unavailable")
                .font(theme::font_semibold(13.5))
                .color(p.fg),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(display_path(root, path))
                .font(theme::font_mono(12.0))
                .color(p.fg_muted),
        );
        ui.add_space(10.0);
        if widgets::ghost_button(ui, "Try again", Some(Icon::Refresh), p).clicked() {
            request_file(ctx, backend, path, true);
        }
    });
}

fn binary_preview(path: &str, text: &str) -> bool {
    binary_extension(path) || text.as_bytes().iter().take(1024).any(|byte| *byte == 0)
}

fn binary_card(ui: &mut Ui, p: &Palette, location: &str, path: &str, bytes: usize) {
    let name = file_name(path);
    let local = local_path(location, path);
    surfaces::empty_state(
        ui,
        p,
        Icon::File,
        "Binary file",
        &format!("{} · {}", name, human_size(bytes)),
    );
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        if widgets::ghost_button(ui, "Open in default app", Some(Icon::ArrowUpRight), p).clicked()
            && local.exists()
        {
            let _ = open::that_detached(&local);
        }
    });
}

fn large_card(ui: &mut Ui, p: &Palette, location: &str, path: &str, bytes: usize) {
    let name = file_name(path);
    let local = local_path(location, path);
    surfaces::empty_state(
        ui,
        p,
        Icon::File,
        "File too large to preview",
        &format!("{} · {}", name, human_size(bytes)),
    );
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        if widgets::ghost_button(ui, "Open in default app", Some(Icon::ArrowUpRight), p).clicked()
            && local.exists()
        {
            let _ = open::that_detached(&local);
        }
    });
}

fn code_view(ui: &mut Ui, p: &Palette, path: &str, text: &str) {
    let font = theme::font_mono(CODE_FONT);
    let probe = ui
        .painter()
        .layout_no_wrap("Ag".to_string(), font.clone(), p.fg);
    let row_height = (probe.size().y + 2.0).max(CODE_ROW_HEIGHT);
    let sample = ui
        .painter()
        .layout_no_wrap("0000000000".to_string(), font.clone(), p.fg);
    let char_width = (sample.size().x / 10.0).max(1.0);

    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let available = lines.len().max(1);
    let total = available.min(MAX_VIEW_LINES);
    let truncated = available > MAX_VIEW_LINES;
    let digits = available.to_string().len();
    let gutter = (digits as f32 * char_width + 26.0).max(44.0);
    let max_columns = line_columns(ui.ctx(), path, text).max(1);
    let content_width = gutter + max_columns as f32 * char_width + 36.0;
    let pad = 8.0;
    let footer = if truncated { 24.0 } else { 0.0 };
    let content_height = pad * 2.0 + total as f32 * row_height + footer;
    let active_line = ui.memory(|memory| {
        memory
            .data
            .get_temp::<Option<usize>>(Id::unique("files-find-active-line"))
            .flatten()
    });
    let last_scrolled = Id::unique("files-find-scrolled");
    let current_scroll = ui.memory(|memory| memory.data.get_temp::<(String, usize)>(last_scrolled));
    let should_scroll = active_line.is_some()
        && current_scroll
            .as_ref()
            .map(|(path, line)| (path.as_str(), *line))
            != active_line.map(|line| (path, line));

    ScrollArea::both()
        .id_salt(("files-code", path))
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let width = content_width;
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(width, content_height), Sense::hover());
            let painter = ui.painter();

            let first = (((viewport.min.y - pad) / row_height).floor()).max(0.0) as usize;
            let last =
                ((((viewport.max.y - pad) / row_height).ceil()).max(0.0) as usize + 1).min(total);
            let number_font = theme::font_mono(11.0);

            if let Some(line) = active_line
                && line < total
            {
                let y = rect.top() + pad + line as f32 * row_height;
                painter.rect_filled(
                    Rect::from_min_size(
                        egui::Pos2::new(rect.left(), y),
                        Vec2::new(width, row_height),
                    ),
                    CornerRadius::ZERO,
                    p.over(p.bg, p.wash(p.coral, 32)),
                );
            }

            for index in first..last.max(first) {
                let y = rect.top() + pad + index as f32 * row_height;
                let row = Rect::from_min_size(
                    egui::Pos2::new(rect.left(), y),
                    Vec2::new(width, row_height),
                );
                if ui.rect_contains_pointer(row) {
                    painter.rect_filled(row, CornerRadius::ZERO, theme::scale_alpha(p.hover, 0.6));
                }
                painter.text(
                    egui::Pos2::new(rect.left() + gutter - 10.0, y + row_height * 0.5),
                    Align2::RIGHT_CENTER,
                    (index + 1).to_string(),
                    number_font.clone(),
                    p.fg_muted,
                );
                let Some(line) = lines.get(index) else {
                    continue;
                };
                let galley = painter.layout_no_wrap(display_line(line), font.clone(), p.body);
                painter.galley(
                    egui::Pos2::new(
                        rect.left() + gutter,
                        y + (row_height - galley.size().y) * 0.5,
                    ),
                    galley,
                    p.body,
                );
            }

            painter.vline(
                rect.left() + gutter - 8.0,
                Rangef::new(rect.top(), rect.bottom()),
                Stroke::new(0.5, surfaces::edge_muted(p)),
            );

            if truncated {
                painter.text(
                    egui::Pos2::new(rect.left() + gutter, rect.bottom() - 14.0),
                    Align2::LEFT_CENTER,
                    format!("Showing the first {MAX_VIEW_LINES} lines"),
                    theme::font(12.0),
                    p.fg_muted,
                );
            }
            if should_scroll
                && let Some(line) = active_line
                && line < total
            {
                let y = rect.top() + pad + line as f32 * row_height;
                ui.scroll_to_rect(
                    Rect::from_min_size(
                        egui::Pos2::new(rect.left(), y),
                        Vec2::new(width, row_height),
                    ),
                    Some(Align::Center),
                );
                ui.ctx().request_repaint();
                ui.memory_mut(|memory| {
                    memory
                        .data
                        .insert_temp(last_scrolled, (path.to_string(), line));
                });
            }
        });
}

#[derive(Default)]
struct ToolbarResult {
    reload: bool,
    save: bool,
    revert: bool,
}

#[derive(Default)]
struct ToolbarFlags {
    wrap: bool,
    stats: bool,
    status: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    None,
    Saved,
    Dirty,
    Error,
    Copied,
    ReadOnly,
}

fn toolbar(
    ui: &mut Ui,
    p: &Palette,
    ctx: &egui::Context,
    location: &str,
    active: &str,
    buffer: &mut EditorBuffer,
    preview: bool,
    ready: bool,
) -> ToolbarResult {
    let dirty = buffer.dirty();
    let read_only = buffer.read_only();
    let now = ctx.input(|input| input.time);
    let saved = ready && !dirty && now - buffer.saved_at < SAVE_FLASH;
    let local = local_path(location, active);
    let display = display_path(location, active);
    let copied_id = Id::unique("files-copied-at");
    let copied = ctx
        .memory(|memory| memory.data.get_temp::<f64>(copied_id))
        .is_some_and(|at| now - at < 1.4);
    let markdown = is_markdown(active) && ready;
    let source = !preview;
    let slot = if !ready {
        Slot::None
    } else if copied {
        Slot::Copied
    } else if read_only {
        Slot::ReadOnly
    } else if buffer.error.is_some() {
        Slot::Error
    } else if dirty {
        Slot::Dirty
    } else if saved {
        Slot::Saved
    } else {
        Slot::None
    };
    let mut result = ToolbarResult::default();

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TOOLBAR_HEIGHT),
        Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.25,
        Stroke::new(0.5, surfaces::edge_muted(p)),
    );
    let mut content = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                Pos2::new(rect.left() + 12.0, rect.top()),
                Pos2::new(rect.right() - 12.0, rect.bottom()),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    content.spacing_mut().item_spacing.x = 4.0;

    let size = human_size(buffer.text.len());
    let lines = line_count(ctx, active, &buffer.text, buffer.revision);
    let stats = if markdown && preview {
        format!("Markdown · {size}")
    } else {
        format!(
            "{lines} {} · {size}",
            if lines == 1 { "line" } else { "lines" }
        )
    };
    let stats_width = content
        .painter()
        .layout_no_wrap(stats.clone(), theme::font(12.0), p.fg_muted)
        .size()
        .x;
    let mode_width = if markdown {
        mode_toggle_width(&content, p) + 4.0
    } else {
        0.0
    };
    let wrap_width = if ready && source {
        chip_width(&content, "Wrap") + 4.0
    } else {
        0.0
    };
    let save_width = if ready && !read_only {
        chip_width(&content, "Save") + 4.0
    } else {
        0.0
    };
    let revert_width = if ready && !read_only {
        chip_width(&content, "Revert") + 4.0
    } else {
        0.0
    };
    let status_width = if ready { STATUS_SLOT } else { 0.0 };

    let available = content.available_width();
    let fixed = 4.0 * (surfaces::PANEL_ACTION + 4.0)
        + mode_width
        + save_width
        + if dirty { revert_width } else { 0.0 };
    let mut extra = 0.0;
    let mut budget = (available - fixed - 120.0).max(0.0);
    let mut flags = ToolbarFlags::default();
    if wrap_width > 0.0 && budget >= wrap_width {
        flags.wrap = true;
        budget -= wrap_width;
        extra += wrap_width;
    }
    if status_width > 0.0 && budget >= status_width {
        flags.status = true;
        budget -= status_width;
        extra += status_width;
    }
    if ready && budget >= stats_width + 8.0 {
        flags.stats = true;
        extra += stats_width + 8.0;
    }
    let left = (available - fixed - extra).max(60.0);

    let (path_rect, path_response) =
        content.allocate_exact_size(Vec2::new(left, TOOLBAR_HEIGHT), Sense::hover());
    if dirty && !read_only {
        content.painter().circle_filled(
            Pos2::new(path_rect.left() + 4.0, path_rect.center().y),
            3.0,
            p.coral,
        );
    }
    let galley = surfaces::truncated_galley(
        &content,
        display.clone(),
        theme::font_mono(12.0),
        p.fg_muted,
        (left - 18.0).max(20.0),
    );
    content.painter().galley(
        Pos2::new(
            path_rect.left() + 12.0,
            path_rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        p.fg_muted,
    );
    let path_response = path_response.on_hover_text(local.to_string_lossy().to_string());
    if path_response.clicked() {
        ctx.copy_text(local.to_string_lossy().to_string());
        ctx.memory_mut(|memory| memory.data.insert_temp(copied_id, now));
        ctx.request_repaint_after(Duration::from_millis(1600));
    }
    if path_response.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::Default);
    }

    content.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if surfaces::panel_action(ui, p, Icon::Refresh, true, "Reload file").clicked() {
            result.reload = true;
        }
        let open = surfaces::panel_action(ui, p, Icon::ArrowUpRight, true, "Open in default app");
        if open.clicked() && local.exists() {
            let _ = open::that_detached(&local);
        }
        let copy = surfaces::panel_action(ui, p, Icon::Copy, true, "Copy contents");
        if copy.clicked() {
            ctx.copy_text(buffer.text.clone());
            ctx.memory_mut(|memory| memory.data.insert_temp(copied_id, now));
            ctx.request_repaint_after(Duration::from_millis(1600));
        }
        let find = surfaces::panel_action(ui, p, Icon::Search, true, "Find in file");
        if find.clicked() {
            let open = ctx
                .memory(|memory| memory.data.get_temp::<bool>(Id::unique("files-find-open")))
                .unwrap_or(false);
            ctx.memory_mut(|memory| {
                memory
                    .data
                    .insert_temp(Id::unique("files-find-open"), !open)
            });
        }
        if markdown && let Some(mode) = mode_toggle(ui, p, preview) {
            set_preview_mode(ctx, active, mode);
        }
        if flags.wrap
            && toolbar_chip(ui, p, "Wrap", buffer.wrap, false, true, "Toggle word wrap").clicked()
        {
            buffer.wrap = !buffer.wrap;
        }
        if save_width > 0.0
            && toolbar_chip(ui, p, "Save", false, dirty, dirty, "Save file (⌘S)").clicked()
        {
            result.save = true;
        }
        if dirty
            && revert_width > 0.0
            && toolbar_chip(ui, p, "Revert", false, false, true, "Discard changes").clicked()
        {
            result.revert = true;
        }
        if flags.stats {
            ui.label(
                RichText::new(stats)
                    .font(theme::font(12.0))
                    .color(p.fg_muted),
            );
        }
        if flags.status {
            status_slot(ui, p, slot, buffer.error.as_deref(), saved);
        }
    });

    if slot == Slot::Saved {
        let remaining = (buffer.saved_at + SAVE_FLASH - now).max(0.05);
        ctx.request_repaint_after(Duration::from_secs_f64(remaining));
    }
    result
}

fn chip_width(ui: &Ui, label: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            label.to_string(),
            theme::font_medium(12.0),
            Color32::PLACEHOLDER,
        )
        .size()
        .x
        + 18.0
}

fn toolbar_chip(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    pressed: bool,
    strong: bool,
    enabled: bool,
    tip: &str,
) -> Response {
    let font = theme::font_medium(12.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), Color32::PLACEHOLDER);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + 18.0, 22.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hover =
        crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered() && enabled);
    let radius = CornerRadius::same(theme::radius::SM);
    if pressed {
        ui.painter().rect_filled(rect, radius, p.active);
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(0.5, surfaces::edge(p)),
            egui::StrokeKind::Inside,
        );
    } else if hover > 0.001 {
        ui.painter()
            .rect_filled(rect, radius, p.hover.gamma_multiply(hover));
    } else if enabled {
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(0.5, surfaces::edge_muted(p)),
            egui::StrokeKind::Inside,
        );
    }
    let color = if !enabled {
        p.disabled
    } else if pressed || strong {
        p.fg
    } else {
        p.fg_muted
    };
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, label, font, color);
    response.on_hover_text(tip)
}

fn status_slot(ui: &mut Ui, p: &Palette, slot: Slot, error: Option<&str>, saved: bool) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(STATUS_SLOT, 24.0), Sense::hover());
    let (label, color) = match slot {
        Slot::Saved => ("Saved", p.green),
        Slot::Dirty => ("Unsaved", p.coral),
        Slot::Error => ("Save failed", p.red),
        Slot::Copied => ("Copied", p.primary),
        Slot::ReadOnly => ("Read-only", p.fg_muted),
        Slot::None => ("", p.fg_muted),
    };
    let mut x = rect.left();
    if saved {
        icons::paint(
            ui,
            Icon::Check,
            Rect::from_center_size(Pos2::new(x + 6.0, rect.center().y), Vec2::splat(12.0)),
            color,
        );
        x += 16.0;
    }
    ui.painter().text(
        Pos2::new(x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::font_medium(12.0),
        color,
    );
    if slot == Slot::Error
        && let Some(error) = error
    {
        response.on_hover_text(error);
    }
}

#[derive(Clone)]
struct EditorBuffer {
    text: String,
    baseline: String,
    wrap: bool,
    writable: bool,
    large: bool,
    loaded: bool,
    saved_at: f64,
    error: Option<String>,
    width_hint: f32,
    revision: u64,
}

impl EditorBuffer {
    fn new(text: String, writable: bool, loaded: bool) -> Self {
        let large = is_large_text(&text);
        Self {
            baseline: text.clone(),
            text,
            wrap: false,
            writable,
            large,
            loaded,
            saved_at: f64::NEG_INFINITY,
            error: None,
            width_hint: 0.0,
            revision: 0,
        }
    }

    fn dirty(&self) -> bool {
        self.text != self.baseline
    }

    fn read_only(&self) -> bool {
        !self.writable || self.large
    }

    fn sync_large(&mut self, disk: &str) {
        self.large = is_large_text(disk);
    }

    fn sync(&mut self, disk: &str) {
        self.loaded = true;
        if self.dirty() {
            return;
        }
        if self.baseline != disk {
            self.text = disk.to_string();
            self.baseline = disk.to_string();
            self.large = is_large_text(disk);
            self.error = None;
            self.width_hint = 0.0;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn save(&mut self, path: &Path, now: f64) -> Result<(), String> {
        std::fs::write(path, self.text.as_bytes()).map_err(|error| error.to_string())?;
        self.baseline = self.text.clone();
        self.saved_at = now;
        self.error = None;
        Ok(())
    }

    fn revert(&mut self) {
        if !self.dirty() {
            return;
        }
        self.text = self.baseline.clone();
        self.error = None;
        self.revision = self.revision.wrapping_add(1);
    }
}

fn buffers_id() -> Id {
    Id::unique("files-editor-buffers")
}

struct EditorBuffers {
    ctx: egui::Context,
    map: HashMap<String, EditorBuffer>,
}

impl EditorBuffers {
    fn take(ctx: &egui::Context) -> Self {
        let map = ctx
            .data_mut(|data| data.remove_temp::<HashMap<String, EditorBuffer>>(buffers_id()))
            .unwrap_or_default();
        Self {
            ctx: ctx.clone(),
            map,
        }
    }
}

impl Drop for EditorBuffers {
    fn drop(&mut self) {
        let map = std::mem::take(&mut self.map);
        self.ctx
            .data_mut(|data| data.insert_temp(buffers_id(), map));
    }
}

fn forget_clean_buffer(ctx: &egui::Context, path: &str) {
    ctx.data_mut(|data| {
        let map = data.get_temp_mut_or_default::<HashMap<String, EditorBuffer>>(buffers_id());
        if map.get(path).is_some_and(|buffer| !buffer.dirty()) {
            map.remove(path);
        }
    });
}

fn dirty_paths(ctx: &egui::Context) -> std::collections::HashSet<String> {
    ctx.data_mut(|data| {
        let map = data.get_temp_mut_or_default::<HashMap<String, EditorBuffer>>(buffers_id());
        map.iter()
            .filter(|(_, buffer)| buffer.dirty())
            .map(|(path, _)| path.clone())
            .collect()
    })
}

struct ReadOnlyBuffer<'a> {
    text: &'a str,
}

impl egui::TextBuffer for ReadOnlyBuffer<'_> {
    fn is_mutable(&self) -> bool {
        false
    }

    fn as_str(&self) -> &str {
        self.text
    }

    fn insert_text(&mut self, _text: &str, _char_index: CharIndex) -> usize {
        0
    }

    fn delete_char_range(&mut self, _char_range: std::ops::Range<CharIndex>) {}

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<ReadOnlyBuffer<'static>>()
    }
}

fn is_large_text(text: &str) -> bool {
    text.len() > MAX_EDIT_BYTES || text.split('\n').count() > MAX_VIEW_LINES
}

fn writable(location: &str, path: &str) -> bool {
    let root = Path::new(location);
    if location.is_empty() || !root.is_absolute() {
        return true;
    }
    local_path(location, path).starts_with(root)
}

fn estimate_width(ui: &Ui, text: &str) -> f32 {
    let font = theme::font_mono(CODE_FONT);
    let mut widest = "";
    let mut widest_cells = 0;
    let mut longest = "";
    let mut longest_bytes = 0;
    for line in text.split('\n') {
        let cells = display_width(line);
        if cells > widest_cells {
            widest_cells = cells;
            widest = line;
        }
        if line.len() > longest_bytes {
            longest_bytes = line.len();
            longest = line;
        }
    }
    let a = ui
        .painter()
        .layout_no_wrap(widest.to_string(), font.clone(), Color32::PLACEHOLDER)
        .size()
        .x;
    let b = ui
        .painter()
        .layout_no_wrap(longest.to_string(), font, Color32::PLACEHOLDER)
        .size()
        .x;
    a.max(b)
}

fn wrapped_rows(text: &str, width: f32, char_width: f32) -> usize {
    let columns = (width / char_width).floor().max(8.0);
    let mut rows = 0;
    for line in text.split('\n') {
        rows += (display_width(line) as f32 / columns).ceil().max(1.0) as usize;
    }
    rows.max(1)
}

fn find_line(ctx: &egui::Context) -> Option<usize> {
    let open = ctx
        .memory(|memory| memory.data.get_temp::<bool>(Id::unique("files-find-open")))
        .unwrap_or(false);
    if !open {
        return None;
    }
    let query = ctx
        .memory(|memory| {
            memory
                .data
                .get_temp::<String>(Id::unique("files-find-query"))
        })
        .unwrap_or_default();
    if query.is_empty() {
        return None;
    }
    ctx.memory(|memory| {
        memory
            .data
            .get_temp::<Option<usize>>(Id::unique("files-find-active-line"))
            .flatten()
    })
}

fn editor_view(
    ui: &mut Ui,
    p: &Palette,
    ctx: &egui::Context,
    path: &str,
    buffer: &mut EditorBuffer,
) {
    let font = theme::font_mono(CODE_FONT);
    let natural_row = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let row_height = CODE_ROW_HEIGHT.max(natural_row);
    let extra_spacing = (row_height - natural_row).max(0.0);
    let id = Id::unique(("files-editor", path));
    let lines = buffer.text.split('\n').count().max(1);
    let digits = lines.to_string().len();
    let char_width = (ui
        .painter()
        .layout_no_wrap("0000000000".to_string(), font.clone(), p.fg)
        .size()
        .x
        / 10.0)
        .max(1.0);
    let gutter = (digits as f32 * char_width + 26.0)
        .clamp(MIN_GUTTER, 120.0)
        .round();
    let wrap = buffer.wrap;
    let read_only = buffer.read_only();
    let width_hint = buffer.width_hint;
    let target = find_line(ctx);
    let applied_id = Id::unique(("files-editor-find", path));
    let applied = ctx.memory(|memory| memory.data.get_temp::<usize>(applied_id));
    let scroll_to = target.filter(|line| applied != Some(*line));
    if target.is_none() {
        ctx.memory_mut(|memory| memory.data.insert_temp(applied_id, usize::MAX));
    }

    let mut text = std::mem::take(&mut buffer.text);
    let text_color = p.body;
    let scroll = if wrap {
        ScrollArea::vertical()
    } else {
        ScrollArea::both()
    };
    scroll
        .id_salt(("files-editor-scroll", path))
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let content_width = if wrap {
                viewport.width().max(1.0)
            } else {
                (gutter + width_hint + CODE_PAD_X).max(viewport.width())
            };
            let rows = if wrap {
                wrapped_rows(&text, content_width - gutter - CODE_PAD_X, char_width)
            } else {
                lines
            };
            let height =
                (CODE_PAD_TOP + CODE_PAD_BOTTOM + rows as f32 * row_height).max(viewport.height());
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(content_width, height), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, p.bg);
            let frame = Frame::NONE.fill(p.bg).inner_margin(Margin {
                left: gutter as i8,
                right: CODE_PAD_X as i8,
                top: CODE_PAD_TOP as i8,
                bottom: CODE_PAD_BOTTOM as i8,
            });
            let filled = (((height - CODE_PAD_TOP - CODE_PAD_BOTTOM) / row_height)
                .ceil()
                .max(1.0)) as usize;
            let desired_rows = filled.max(lines);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            child.spacing_mut().extra_text_line_spacing = extra_spacing;
            let output = if read_only {
                let mut source = ReadOnlyBuffer { text: &text };
                show_code_edit(
                    &mut child,
                    id,
                    &mut source,
                    frame,
                    content_width,
                    desired_rows,
                    &font,
                    text_color,
                    wrap,
                    row_height,
                )
            } else {
                show_code_edit(
                    &mut child,
                    id,
                    &mut text,
                    frame,
                    content_width,
                    desired_rows,
                    &font,
                    text_color,
                    wrap,
                    row_height,
                )
            };
            if output.response.changed() {
                buffer.revision = buffer.revision.wrapping_add(1);
            }
            if !wrap {
                let measured = output.galley.rect.width();
                if (measured - width_hint).abs() > 0.5 {
                    ctx.request_repaint();
                }
                buffer.width_hint = measured;
            }
            let used_height = CODE_PAD_TOP + output.galley.size().y + CODE_PAD_BOTTOM;
            let used_width = gutter + output.galley.rect.width() + CODE_PAD_X + 24.0;
            if used_height > height + 1.0 || used_width > content_width + 1.0 {
                ui.advance_cursor_after_rect(Rect::from_min_size(
                    rect.min,
                    Vec2::new(content_width.max(used_width), height.max(used_height)),
                ));
            }

            let clip = ui.clip_rect();
            let _ = ui.interact(
                Rect::from_min_max(
                    Pos2::new(clip.left(), clip.top()),
                    Pos2::new(clip.left() + gutter, clip.bottom()),
                ),
                id.with("gutter"),
                Sense::click_and_drag(),
            );
            let painter = ui.painter();
            if let Some(line) = target
                && let Some((top, bottom)) = line_band(&output.galley, output.galley_pos, line)
            {
                let band = Rect::from_min_max(
                    Pos2::new(clip.left() + gutter, top),
                    Pos2::new(clip.right(), bottom),
                );
                painter.rect_filled(band, 0.0, p.selection);
                if scroll_to == Some(line) {
                    ui.scroll_to_rect(band, Some(Align::Center));
                    ctx.memory_mut(|memory| memory.data.insert_temp(applied_id, line));
                }
            }
            let gutter_rect = Rect::from_min_max(
                Pos2::new(clip.left(), clip.top()),
                Pos2::new(clip.left() + gutter, clip.bottom()),
            );
            painter.rect_filled(gutter_rect, 0.0, p.bg);
            painter.vline(
                gutter_rect.right() - 0.5,
                Rangef::new(gutter_rect.top(), gutter_rect.bottom()),
                Stroke::new(0.5, surfaces::edge_muted(p)),
            );
            let number_font = theme::font_mono(11.0);
            let mut line = 0usize;
            let mut starts_line = true;
            for placed in &output.galley.rows {
                if starts_line && line < lines {
                    line += 1;
                    let y = output.galley_pos.y + placed.pos.y;
                    let row_rect = Rect::from_min_size(
                        Pos2::new(gutter_rect.left(), y),
                        Vec2::new(gutter, placed.size.y.max(row_height)),
                    );
                    if row_rect.intersects(clip) {
                        painter.text(
                            Pos2::new(gutter_rect.right() - 10.0, row_rect.center().y),
                            Align2::RIGHT_CENTER,
                            line.to_string(),
                            number_font.clone(),
                            p.fg_muted,
                        );
                    }
                }
                starts_line = placed.ends_with_newline;
            }
        });
    buffer.text = text;
}

fn show_code_edit(
    ui: &mut Ui,
    id: Id,
    text: &mut dyn egui::TextBuffer,
    frame: Frame,
    width: f32,
    rows: usize,
    font: &egui::FontId,
    color: Color32,
    wrap: bool,
    row_height: f32,
) -> egui::widgets::text_edit::TextEditOutput {
    let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
        let width = if wrap { wrap_width } else { f32::INFINITY };
        let mut job = LayoutJob::simple(buffer.as_str().to_owned(), font.clone(), color, width);
        job.keep_trailing_whitespace = true;
        for section in &mut job.sections {
            section.format.line_height = Some(row_height);
        }
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    TextEdit::multiline(text)
        .id(id)
        .code_editor()
        .font(font.clone())
        .text_color(color)
        .cursor_at_end(false)
        .frame(frame)
        .desired_width(width)
        .desired_rows(rows)
        .layouter(&mut layouter)
        .show(ui)
}

fn line_band(galley: &egui::Galley, origin: Pos2, line: usize) -> Option<(f32, f32)> {
    let mut current = 0usize;
    let mut starts_line = true;
    let mut top: Option<f32> = None;
    let mut bottom = 0.0f32;
    for placed in &galley.rows {
        if starts_line {
            current += 1;
        }
        if current == line {
            let y = origin.y + placed.pos.y;
            top = Some(top.map_or(y, |value| value.min(y)));
            bottom = bottom.max(y + placed.size.y);
        } else if current > line {
            break;
        }
        starts_line = placed.ends_with_newline;
    }
    top.map(|top| (top, bottom))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReloadChoice {
    Save,
    Discard,
    Cancel,
}

fn open_reload_confirm(ctx: &egui::Context) {
    ctx.memory_mut(|memory| {
        memory
            .data
            .insert_temp(Id::unique("files-reload-confirm"), true);
        memory.stop_text_input();
    });
}

fn close_reload_confirm(ctx: &egui::Context) {
    ctx.memory_mut(|memory| {
        memory
            .data
            .remove::<bool>(Id::unique("files-reload-confirm"))
    });
}

fn reload_dialog(
    ui: &mut Ui,
    p: &Palette,
    area: Rect,
    name: &str,
    dirty: bool,
) -> Option<ReloadChoice> {
    if area.width() < 80.0 || area.height() < 80.0 {
        return Some(ReloadChoice::Cancel);
    }
    ui.painter().rect_filled(
        area,
        0.0,
        Color32::from_black_alpha(if p.dark { 140 } else { 56 }),
    );
    let _ = ui.interact(area, Id::unique("files-reload-scrim"), Sense::click());
    let mut choice =
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            Some(ReloadChoice::Cancel)
        } else {
            None
        };
    let width = area.width().clamp(260.0, 420.0);
    let height = if dirty { 132.0 } else { 118.0 };
    let card = Rect::from_center_size(area.center(), Vec2::new(width, height));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(card)
            .layout(Layout::top_down(Align::Min)),
    );
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, surfaces::edge(p)))
        .corner_radius(theme::radius::XL)
        .shadow(egui::epaint::Shadow {
            offset: [0, 10],
            blur: 28,
            spread: 0,
            color: Color32::from_black_alpha(if p.dark { 170 } else { 60 }),
        })
        .inner_margin(Margin::same(16))
        .show(&mut child, |ui| {
            ui.label(
                RichText::new("Unsaved changes")
                    .font(theme::font_semibold(14.0))
                    .color(p.fg),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("Reloading {name} discards your edits."))
                    .font(theme::font(12.5))
                    .color(p.fg_muted),
            );
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(
                    ui,
                    "Discard & reload",
                    None,
                    widgets::ButtonVariant::Destructive,
                    widgets::ButtonSize::Sm,
                    p,
                )
                .clicked()
                {
                    choice = Some(ReloadChoice::Discard);
                }
                if widgets::button(
                    ui,
                    "Save changes",
                    None,
                    widgets::ButtonVariant::Outline,
                    widgets::ButtonSize::Sm,
                    p,
                )
                .clicked()
                {
                    choice = Some(ReloadChoice::Save);
                }
                if widgets::ghost_button(ui, "Cancel", None, p).clicked() {
                    choice = Some(ReloadChoice::Cancel);
                }
            });
        });
    choice
}

fn mode_toggle_width(ui: &Ui, p: &Palette) -> f32 {
    let font = theme::font_medium(12.0);
    let preview_width = ui
        .painter()
        .layout_no_wrap("Preview".to_string(), font.clone(), p.fg)
        .size()
        .x
        + 14.0;
    let source_width = ui
        .painter()
        .layout_no_wrap("Source".to_string(), font, p.fg)
        .size()
        .x
        + 14.0;
    preview_width + source_width + 4.0
}

fn mode_toggle(ui: &mut Ui, p: &Palette, preview: bool) -> Option<bool> {
    let font = theme::font_medium(12.0);
    let preview_width = ui
        .painter()
        .layout_no_wrap("Preview".to_string(), font.clone(), p.fg)
        .size()
        .x
        + 14.0;
    let source_width = ui
        .painter()
        .layout_no_wrap("Source".to_string(), font.clone(), p.fg)
        .size()
        .x
        + 14.0;
    let width = preview_width + source_width + 4.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::radius::INLINE_CODE),
        p.hover,
    );
    let mut chosen = None;
    for (index, (label, active)) in [("Preview", preview), ("Source", !preview)]
        .into_iter()
        .enumerate()
    {
        let left = rect.left() + 2.0 + index as f32 * (preview_width + 2.0);
        let segment = Rect::from_min_size(
            egui::Pos2::new(left, rect.top() + 2.0),
            Vec2::new(preview_width, rect.height() - 4.0),
        );
        let response = ui.interact(segment, Id::unique(("files-mode", label)), Sense::click());
        if active {
            ui.painter()
                .rect_filled(segment, CornerRadius::same(theme::radius::SM), p.card);
            ui.painter().rect_stroke(
                segment,
                CornerRadius::same(theme::radius::SM),
                Stroke::new(0.5, surfaces::edge(p)),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            ui.painter()
                .rect_filled(segment, CornerRadius::same(theme::radius::SM), p.active);
        }
        ui.painter().text(
            segment.center(),
            Align2::CENTER_CENTER,
            label,
            font.clone(),
            if active { p.fg } else { p.fg_muted },
        );
        if response.clicked() && !active {
            chosen = Some(label == "Preview");
        }
    }
    chosen
}

#[derive(Clone)]
struct TreeRow {
    path: String,
    name: String,
    directory: bool,
    depth: usize,
    open: bool,
}

fn collect_rows(
    files: &FilesState,
    root: &str,
    ctx: &egui::Context,
    backend: &Backend,
    connected: bool,
) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    if let Some(entries) = files.tree.get(root) {
        collect_children(&mut rows, entries, files, ctx, backend, connected, 0);
    }
    rows
}

fn collect_children(
    rows: &mut Vec<TreeRow>,
    entries: &[FileEntry],
    files: &FilesState,
    ctx: &egui::Context,
    backend: &Backend,
    connected: bool,
    depth: usize,
) {
    if depth > 32 {
        return;
    }

    let mut ordered: Vec<&FileEntry> = entries.iter().collect();
    ordered.sort_by_cached_key(|entry| {
        (
            !is_directory(entry),
            file_name(&entry.path).to_lowercase(),
            entry.path.clone(),
        )
    });

    for entry in ordered {
        let directory = is_directory(entry);
        let open = directory && files.expanded.contains(&entry.path);
        rows.push(TreeRow {
            path: entry.path.clone(),
            name: file_name(&entry.path),
            directory,
            depth,
            open,
        });

        if open {
            match files.tree.get(&entry.path) {
                Some(children) => {
                    collect_children(rows, children, files, ctx, backend, connected, depth + 1)
                }
                None => {
                    if connected {
                        request_directory(ctx, backend, &entry.path, false);
                    }
                }
            }
        }
    }
}

fn open_file(
    ctx: &egui::Context,
    files: &mut FilesState,
    backend: &Backend,
    path: &str,
    pinned: bool,
) {
    let preview_id = Id::unique("files-preview-tab");
    let preview = ctx
        .memory(|memory| memory.data.get_temp::<String>(preview_id))
        .filter(|preview| files.open_tabs.iter().any(|tab| tab == preview));
    let already = files.open_tabs.iter().any(|tab| tab == path);
    if !already {
        let mut insert_at = files.open_tabs.len();
        if !pinned
            && let Some(preview) = preview.as_ref()
            && let Some(index) = files.open_tabs.iter().position(|tab| tab == preview)
        {
            files.open_tabs.remove(index);
            insert_at = index;
        }
        while files.open_tabs.len() >= MAX_OPEN_TABS {
            files.open_tabs.remove(0);
            insert_at = insert_at.saturating_sub(1);
        }
        files
            .open_tabs
            .insert(insert_at.min(files.open_tabs.len()), path.to_string());
    }
    files.active_tab = Some(path.to_string());
    if pinned {
        ctx.memory_mut(|memory| memory.data.remove::<String>(preview_id));
    } else {
        ctx.memory_mut(|memory| memory.data.insert_temp(preview_id, path.to_string()));
    }
    request_file(ctx, backend, path, true);
}

fn close_tab_animated(ctx: &egui::Context, files: &mut FilesState, path: &str) {
    if ctx
        .memory(|memory| {
            memory
                .data
                .get_temp::<String>(Id::unique("files-preview-tab"))
        })
        .is_some_and(|preview| preview == path)
    {
        ctx.memory_mut(|memory| {
            memory
                .data
                .remove::<String>(Id::unique("files-preview-tab"))
        });
    }
    close_tab(files, path);
    forget_clean_buffer(ctx, path);
}

fn close_tab(files: &mut FilesState, path: &str) {
    let Some(index) = files.open_tabs.iter().position(|tab| tab == path) else {
        return;
    };

    files.open_tabs.remove(index);

    if files.active_tab.as_deref() != Some(path) {
        return;
    }

    files.active_tab = if files.open_tabs.is_empty() {
        None
    } else {
        let next = index.min(files.open_tabs.len() - 1);
        Some(files.open_tabs[next].clone())
    };

    if files
        .preview
        .as_ref()
        .is_some_and(|(preview, _)| preview == path)
    {
        files.preview = None;
    }
}

fn request_file(ctx: &egui::Context, backend: &Backend, path: &str, force: bool) {
    let id = Id::unique(("files-load", path));
    let now = ctx.input(|input| input.time);
    let last = ctx
        .memory(|memory| memory.data.get_temp::<f64>(id))
        .unwrap_or(f64::NEG_INFINITY);

    if force || now - last > LOAD_TIMEOUT * 0.5 {
        ctx.memory_mut(|memory| memory.data.insert_temp(id, now));
        backend.send(Action::LoadFile(path.to_string()));
    }
}

fn request_directory(ctx: &egui::Context, backend: &Backend, path: &str, force: bool) {
    let id = Id::unique(("files-dir", path));
    let now = ctx.input(|input| input.time);
    let last = ctx
        .memory(|memory| memory.data.get_temp::<f64>(id))
        .unwrap_or(f64::NEG_INFINITY);

    if force || now - last > 3.0 {
        ctx.memory_mut(|memory| memory.data.insert_temp(id, now));
        backend.send(Action::LoadDirectory(path.to_string()));
    }
}

fn preview_mode(ctx: &egui::Context, path: &str, bytes: usize) -> bool {
    let id = Id::unique(("files-mode", path));
    ctx.memory(|memory| memory.data.get_temp::<bool>(id))
        .unwrap_or(bytes <= MARKDOWN_PREVIEW_BYTES)
}

fn set_preview_mode(ctx: &egui::Context, path: &str, value: bool) {
    let id = Id::unique(("files-mode", path));
    ctx.memory_mut(|memory| memory.data.insert_temp(id, value));
}

fn is_markdown(path: &str) -> bool {
    matches!(extension(path).as_str(), "md" | "markdown" | "mdx")
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_directory(entry: &FileEntry) -> bool {
    entry.kind == "directory" || entry.kind == "dir"
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path.to_string())
}

fn display_path(root: &str, path: &str) -> String {
    if !root.is_empty()
        && let Some(rest) = path.strip_prefix(root)
    {
        let trimmed = rest.trim_start_matches('/');
        if trimmed.is_empty() {
            return path.to_string();
        }
        return trimmed.to_string();
    }
    path.to_string()
}

fn local_path(root: &str, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        Path::new(root).join(candidate)
    }
}

fn line_count(ctx: &egui::Context, path: &str, text: &str, revision: u64) -> usize {
    let id = Id::unique(("files-line-count", path, text.len(), revision));
    if let Some(count) = ctx.memory(|memory| memory.data.get_temp::<usize>(id)) {
        return count;
    }
    let count = text.split('\n').count();
    ctx.memory_mut(|memory| memory.data.insert_temp(id, count));
    count
}

fn line_columns(ctx: &egui::Context, path: &str, text: &str) -> usize {
    let id = Id::unique(("files-line-columns", path, text.len()));
    if let Some(columns) = ctx.memory(|memory| memory.data.get_temp::<usize>(id)) {
        return columns;
    }
    let columns = text
        .split('\n')
        .map(|line| display_width(line.strip_suffix('\r').unwrap_or(line)))
        .max()
        .unwrap_or(1)
        .max(1);
    ctx.memory_mut(|memory| memory.data.insert_temp(id, columns));
    columns
}

fn human_size(bytes: usize) -> String {
    let value = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if value < 1024.0 * 1024.0 {
        format!("{:.1} KB", value / 1024.0)
    } else if value < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", value / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", value / (1024.0 * 1024.0 * 1024.0))
    }
}

fn display_width(line: &str) -> usize {
    let mut width = 0;
    for character in line.chars() {
        if width >= MAX_LINE_CHARS {
            return MAX_LINE_CHARS;
        }
        if character == '\t' {
            width += 4 - width % 4;
        } else {
            width += 1;
        }
    }
    width
}

fn display_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len().min(MAX_LINE_CHARS * 2));
    let mut width = 0;
    for character in line.chars() {
        if width >= MAX_LINE_CHARS {
            output.push('…');
            break;
        }
        if character == '\t' {
            let spaces = 4 - width % 4;
            for _ in 0..spaces {
                output.push(' ');
            }
            width += spaces;
        } else {
            output.push(character);
            width += 1;
        }
    }
    output
}

fn file_visual(name: &str, p: &Palette) -> (Icon, Color32) {
    match extension(name).as_str() {
        "rs" => (Icon::File, p.coral),
        "ts" | "tsx" => (Icon::File, p.blue),
        "js" | "jsx" | "mjs" | "cjs" => (Icon::File, p.primary),
        "json" | "jsonc" | "json5" => (Icon::File, p.primary),
        "md" | "mdx" | "txt" | "rst" => (Icon::File, p.blue),
        "css" | "scss" | "sass" | "less" => (Icon::File, p.purple),
        "html" | "htm" | "xml" => (Icon::File, p.coral),
        "svg" => (Icon::File, p.green),
        "toml" | "yaml" | "yml" | "ini" | "conf" | "env" => (Icon::File, p.purple),
        "sh" | "bash" | "zsh" | "fish" => (Icon::Terminal, p.green),
        "lock" => (Icon::File, p.fg_muted),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "avif" => (Icon::Layers, p.green),
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "7z" | "rar" => {
            (Icon::Archive, p.coral)
        }
        "pdf" => (Icon::File, p.red),
        _ => (Icon::File, p.fg_muted),
    }
}

fn binary_extension(path: &str) -> bool {
    matches!(
        extension(path).as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "bmp"
            | "ico"
            | "avif"
            | "pdf"
            | "zip"
            | "tar"
            | "gz"
            | "tgz"
            | "bz2"
            | "xz"
            | "zst"
            | "7z"
            | "rar"
            | "exe"
            | "dll"
            | "dylib"
            | "so"
            | "a"
            | "o"
            | "class"
            | "jar"
            | "wasm"
            | "mp3"
            | "mp4"
            | "mov"
            | "wav"
            | "flac"
            | "ogg"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "db"
            | "sqlite"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 420.0))),
            events,
            ..egui::RawInput::default()
        }
    }

    fn click(pos: Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ]
    }

    fn run_editor(ctx: &egui::Context, buffer: &mut EditorBuffer, events: Vec<egui::Event>) {
        let p = Palette::dark();
        let mut output = ctx.run_ui(input(events), |ui| {
            let ctx = ui.ctx().clone();
            editor_view(ui, &p, &ctx, "test.rs", buffer);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn editor_places_the_caret_and_accepts_typing() {
        let ctx = egui::Context::default();
        let mut buffer = EditorBuffer::new("alpha\nbeta".to_string(), true, true);
        run_editor(&ctx, &mut buffer, Vec::new());
        run_editor(&ctx, &mut buffer, click(Pos2::new(150.0, 20.0)));
        run_editor(&ctx, &mut buffer, vec![egui::Event::Text("Z".to_string())]);
        assert_eq!(buffer.text, "alphaZ\nbeta");
        assert!(buffer.dirty());
    }

    #[test]
    fn read_only_editor_ignores_typing() {
        let ctx = egui::Context::default();
        let mut buffer = EditorBuffer::new("alpha\nbeta".to_string(), false, true);
        run_editor(&ctx, &mut buffer, Vec::new());
        run_editor(&ctx, &mut buffer, click(Pos2::new(150.0, 20.0)));
        assert!(ctx.memory(|memory| memory.has_focus(Id::unique(("files-editor", "test.rs")))));
        run_editor(&ctx, &mut buffer, vec![egui::Event::Text("Z".to_string())]);
        assert_eq!(buffer.text, "alpha\nbeta");
        assert!(!buffer.dirty());
    }

    #[test]
    fn layouter_wraps_only_when_asked() {
        let p = Palette::dark();
        let source = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nb";
        for (wrap, expected) in [(false, 2usize), (true, 3usize)] {
            let ctx = egui::Context::default();
            let mut text = source.to_string();
            let mut rows = 0;
            let mut frame = ctx.run_ui(input(Vec::new()), |ui| {
                let output = show_code_edit(
                    ui,
                    Id::unique("wrap-test"),
                    &mut text,
                    Frame::NONE,
                    220.0,
                    2,
                    &theme::font_mono(CODE_FONT),
                    p.fg,
                    wrap,
                    CODE_ROW_HEIGHT,
                );
                rows = output.galley.rows.len();
            });
            frame.textures_delta.clear();
            assert_eq!(rows, expected);
        }
    }

    #[test]
    fn buffer_sync_keeps_dirty_edits_and_adopts_clean_ones() {
        let mut buffer = EditorBuffer::new("one".to_string(), true, true);
        buffer.sync("two");
        assert_eq!(buffer.text, "two");
        buffer.text = "edited".to_string();
        buffer.sync("three");
        assert_eq!(buffer.text, "edited");
        assert!(buffer.dirty());
        buffer.revert();
        assert_eq!(buffer.text, "two");
        assert!(!buffer.dirty());
    }

    #[test]
    fn buffer_save_writes_bytes_and_clears_dirty() {
        let mut path = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        path.push(format!("rchamber-files-{}-{stamp}.txt", std::process::id()));
        let mut buffer = EditorBuffer::new("one".to_string(), true, true);
        buffer.text = "two\nthree".to_string();
        assert!(buffer.dirty());
        assert!(buffer.save(&path, 1.5).is_ok());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            "two\nthree"
        );
        assert!(!buffer.dirty());
        assert_eq!(buffer.saved_at, 1.5);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn buffers_survive_frames_and_tab_switches() {
        let ctx = egui::Context::default();
        {
            let mut buffers = EditorBuffers::take(&ctx);
            buffers.map.insert(
                "a.rs".to_string(),
                EditorBuffer::new("one".to_string(), true, true),
            );
        }
        {
            let mut buffers = EditorBuffers::take(&ctx);
            buffers.map.get_mut("a.rs").unwrap().text = "edited".to_string();
        }
        {
            let buffers = EditorBuffers::take(&ctx);
            let buffer = buffers.map.get("a.rs").unwrap();
            assert_eq!(buffer.text, "edited");
            assert!(buffer.dirty());
        }
        forget_clean_buffer(&ctx, "a.rs");
        let buffers = EditorBuffers::take(&ctx);
        assert_eq!(
            buffers.map.get("a.rs").map(|buffer| buffer.text.as_str()),
            Some("edited")
        );
    }

    #[test]
    fn closing_a_tab_forgets_only_clean_buffers() {
        let ctx = egui::Context::default();
        {
            let mut buffers = EditorBuffers::take(&ctx);
            buffers.map.insert(
                "clean.rs".to_string(),
                EditorBuffer::new("one".to_string(), true, true),
            );
            buffers.map.insert(
                "dirty.rs".to_string(),
                EditorBuffer::new("one".to_string(), true, true),
            );
        }
        {
            let mut buffers = EditorBuffers::take(&ctx);
            buffers.map.get_mut("dirty.rs").unwrap().text = "two".to_string();
        }
        forget_clean_buffer(&ctx, "clean.rs");
        forget_clean_buffer(&ctx, "dirty.rs");
        let buffers = EditorBuffers::take(&ctx);
        assert!(!buffers.map.contains_key("clean.rs"));
        assert!(buffers.map.contains_key("dirty.rs"));
    }

    #[test]
    fn writable_stays_inside_the_workspace() {
        assert!(writable("/work/project", "src/main.rs"));
        assert!(writable("/work/project", "/work/project/src/main.rs"));
        assert!(!writable("/work/project", "/elsewhere/main.rs"));
    }

    #[test]
    fn wrapped_rows_counts_soft_lines() {
        assert_eq!(wrapped_rows("", 10.0, 1.0), 1);
        assert_eq!(wrapped_rows("a\nb", 10.0, 1.0), 2);
        assert_eq!(wrapped_rows("abcdefghijklmnopqrst", 10.0, 1.0), 2);
    }
}
