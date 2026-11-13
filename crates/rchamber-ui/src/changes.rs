use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, Key, Layout, Modifiers, RichText, ScrollArea,
    Sense, Stroke, Ui, Vec2,
};
use rchamber_api::models::{FileDiff, Message, Part};
use rchamber_app::ToastKind;

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::surfaces;
use crate::theme::{self, Palette};
use crate::widgets;

const GATE_LINES: usize = 500;
const PAGE_LINES: usize = 400;
const FILE_HEADER_HEIGHT: f32 = 36.0;
const ROW_HEIGHT: f32 = 22.0;
const HUNK_HEIGHT: f32 = 24.0;
const META_HEIGHT: f32 = 18.0;
const TOOLBAR_HEIGHT: f32 = 36.0;
const TOOLBAR_WIDE: f32 = 660.0;
const TOOLBAR_TIGHT: f32 = 360.0;
const DIFF_FONT: f32 = 12.0;
const NUM_WIDTH: f32 = 38.0;
const SIGN_WIDTH: f32 = 22.0;
const CODE_PAD: f32 = 10.0;
const PATCH_MAX_HEIGHT: f32 = 360.0;
const MAX_PATCH_COLUMNS: usize = 4000;
const INDENT: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DiffMode {
    Unified,
    Split,
}

impl DiffMode {
    fn index(self) -> u8 {
        match self {
            DiffMode::Unified => 0,
            DiffMode::Split => 1,
        }
    }

    fn from_index(value: u8) -> Self {
        if value == 1 {
            DiffMode::Split
        } else {
            DiffMode::Unified
        }
    }
}

#[derive(Clone, Debug)]
enum FileAction {
    Stage(String),
    Unstage(String),
    Discard(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Changed,
    Turn,
}

impl Scope {
    fn index(self) -> u8 {
        match self {
            Scope::Changed => 0,
            Scope::Turn => 1,
        }
    }

    fn from_index(value: u8) -> Self {
        if value == 1 {
            Scope::Turn
        } else {
            Scope::Changed
        }
    }
}

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let workspace = app.workspace_directory();
    let repo = rchamber_git::discover(Path::new(&workspace));
    let staging = staging_cache(ui, &workspace);
    let session_id = app.model.active_id().map(str::to_string);
    let mode_id = Id::unique(("changes-mode", session_id.clone()));
    let mode =
        DiffMode::from_index(ui.memory(|memory| memory.data.get_temp::<u8>(mode_id).unwrap_or(0)));
    let spin_id = Id::unique(("changes-spin", session_id.clone()));
    let now = ui.input(|input| input.time);
    let spinning = ui
        .memory(|memory| memory.data.get_temp::<f64>(spin_id))
        .is_some_and(|until| now < until);
    let scope_id = Id::unique(("changes-scope", session_id.clone()));
    let mut close = false;
    let mut toggle_expanded = false;
    let expanded = app.ui.expanded;

    surfaces::panel_header(
        ui,
        &p,
        &[],
        |_| {},
        |ui| {
            if surfaces::close_button(ui, &p).clicked() {
                close = true;
            }
            if surfaces::expand_button(ui, &p, expanded).clicked() {
                toggle_expanded = true;
            }
        },
    );

    if close {
        app.ui.show_changes = false;
        app.ui.context_open = false;
    }
    if toggle_expanded {
        app.ui.expanded = !app.ui.expanded;
    }

    let mut actions: Vec<FileAction> = Vec::new();
    {
        let active = app.model.active.as_ref();
        let files: &[FileDiff] = active.map_or(&[], |chat| chat.diff.as_slice());
        let scope = Scope::from_index(ui.memory(|memory| {
            memory
                .data
                .get_temp::<u8>(scope_id)
                .unwrap_or(Scope::Changed.index())
        }));
        let turn_paths = active.map(last_turn_paths).unwrap_or_default();
        let turn_files: Vec<&FileDiff> = files
            .iter()
            .filter(|file| turn_paths.iter().any(|path| path_matches(path, &file.file)))
            .collect();
        let shown: Vec<&FileDiff> = match scope {
            Scope::Changed => files.iter().collect(),
            Scope::Turn => turn_files.clone(),
        };
        let additions: u32 = shown.iter().map(|file| file.additions).sum();
        let deletions: u32 = shown.iter().map(|file| file.deletions).sum();
        let all_staged = !shown.is_empty()
            && shown
                .iter()
                .all(|file| file_is_staged(&staging.staged, &file.file));
        let any_staged = shown
            .iter()
            .any(|file| file_is_staged(&staging.staged, &file.file));

        let toolbar = workbench_toolbar(
            ui,
            &p,
            scope,
            files.len(),
            turn_files.len(),
            additions,
            deletions,
            mode,
            all_staged,
            any_staged,
            repo.is_some(),
        );
        if let Some(next) = toolbar.scope {
            if next != scope {
                ui.memory_mut(|memory| memory.data.insert_temp(scope_id, next.index()));
            }
        }
        if let Some(next) = toolbar.mode {
            if next != mode {
                ui.memory_mut(|memory| memory.data.insert_temp(mode_id, next.index()));
            }
        }
        if toolbar.stage {
            for file in &shown {
                if !file_is_staged(&staging.staged, &file.file) {
                    actions.push(FileAction::Stage(file.file.clone()));
                }
            }
        }
        if toolbar.unstage {
            for file in &shown {
                if file_is_staged(&staging.staged, &file.file) {
                    actions.push(FileAction::Unstage(file.file.clone()));
                }
            }
        }
        if toolbar.discard {
            for file in &shown {
                actions.push(FileAction::Discard(file.file.clone()));
            }
        }

        let loading = active.is_some_and(|chat| chat.loading) && files.is_empty();

        if shown.is_empty() {
            if let rchamber_app::Connection::Failed(reason) = &app.model.connection {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 12.0).max(80.0));
                        if surfaces::error_banner(ui, &p, reason, "Retry")
                            && let Some(id) = &session_id
                        {
                            app.backend.send(rchamber_app::Action::LoadDiff(id.clone()));
                        }
                    });
                });
            } else if loading {
                surfaces::loading_state(ui, &p, "Loading changes…");
            } else {
                empty_state(ui, &p, scope);
            }
        } else {
            let (tree, reload) = diff_tree_mode(ui, &p, &session_id, spinning);
            if reload {
                ui.memory_mut(|memory| {
                    memory.data.insert_temp(spin_id, now + 0.7);
                });
                if let Some(id) = &session_id {
                    app.backend.send(rchamber_app::Action::LoadDiff(id.clone()));
                }
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(120));
            }
            let expanded = expanded_dirs(ui, &session_id, &shown);
            let mode = DiffMode::from_index(
                ui.memory(|memory| memory.data.get_temp::<u8>(mode_id).unwrap_or(0)),
            );
            let mut diff = DiffCtx {
                session: session_id.as_deref(),
                scope: &scope,
                mode,
                staged: &staging.staged,
                can_stage: repo.is_some(),
                actions: &mut actions,
            };

            ScrollArea::vertical()
                .id_salt(("changes-scroll", session_id.clone()))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if tree {
                        tree_rows(ui, &p, &shown, &expanded, &mut diff);
                    } else {
                        ui.add_space(2.0);
                        let single = shown.len() == 1;
                        for file in &shown {
                            file_block(ui, &p, file, 0, false, single, &mut diff);
                        }
                    }
                    ui.add_space(16.0);
                });
        }
    }

    if !actions.is_empty() {
        apply_actions(ui, app, repo.as_deref(), &mut actions);
    }
}

#[derive(Clone, Default)]
struct Staging {
    root: Option<PathBuf>,
    staged: HashSet<String>,
    loaded_at: f64,
}

fn staging_cache(ui: &Ui, workspace: &str) -> Staging {
    let now = ui.input(|input| input.time);
    let id = Id::unique("changes-git-staging");
    let root = rchamber_git::discover(Path::new(workspace));
    let mut cache = ui
        .ctx()
        .data(|data| data.get_temp::<Staging>(id))
        .unwrap_or_default();
    if cache.root != root || cache.loaded_at == 0.0 || now - cache.loaded_at > 1.5 {
        let staged = root
            .as_deref()
            .and_then(|root| rchamber_git::status(root).ok())
            .map(|status| {
                status
                    .staged()
                    .map(|file| file.path.clone())
                    .collect::<HashSet<String>>()
            })
            .unwrap_or_default();
        cache = Staging {
            root,
            staged,
            loaded_at: now,
        };
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, cache.clone()));
    }
    cache
}

fn file_is_staged(staged: &HashSet<String>, path: &str) -> bool {
    staged.iter().any(|candidate| path_matches(candidate, path))
}

fn apply_actions(ui: &Ui, app: &mut RustChamber, repo: Option<&Path>, actions: &[FileAction]) {
    let Some(repo) = repo else {
        app.model
            .toast(ToastKind::Warning, "This workspace is not a Git repository");
        return;
    };
    let mut staged = 0;
    let mut unstaged = 0;
    let mut discarded = 0;
    let mut error: Option<String> = None;
    for action in actions {
        let path = match action {
            FileAction::Stage(path) | FileAction::Unstage(path) | FileAction::Discard(path) => {
                git_path(repo, path)
            }
        };
        let result = match action {
            FileAction::Stage(_) => rchamber_git::stage(repo, &path).map(|()| staged += 1),
            FileAction::Unstage(_) => rchamber_git::unstage(repo, &path).map(|()| unstaged += 1),
            FileAction::Discard(_) => rchamber_git::discard(repo, &path, false).map(|()| {
                discarded += 1;
            }),
        };
        if let Err(failure) = result {
            error = Some(failure.to_string());
        }
    }
    if let Some(error) = error {
        app.model.toast(ToastKind::Error, error);
    } else {
        let mut parts = Vec::new();
        if staged > 0 {
            parts.push(format!("Staged {staged}"));
        }
        if unstaged > 0 {
            parts.push(format!("Unstaged {unstaged}"));
        }
        if discarded > 0 {
            parts.push(format!("Discarded {discarded}"));
        }
        if !parts.is_empty() {
            app.model.toast(ToastKind::Success, parts.join(" · "));
        }
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(Id::unique("changes-git-staging"), Staging::default()));
    ui.ctx().request_repaint();
}

struct ToolbarResult {
    scope: Option<Scope>,
    mode: Option<DiffMode>,
    stage: bool,
    unstage: bool,
    discard: bool,
}

#[allow(clippy::too_many_arguments)]
fn workbench_toolbar(
    ui: &mut Ui,
    p: &Palette,
    scope: Scope,
    files: usize,
    turn: usize,
    additions: u32,
    deletions: u32,
    mode: DiffMode,
    all_staged: bool,
    any_staged: bool,
    can_stage: bool,
) -> ToolbarResult {
    let can_stage = can_stage && files > 0;
    let width = ui.available_width();
    let narrow = width < TOOLBAR_WIDE;
    let height = if narrow {
        TOOLBAR_HEIGHT * 2.0
    } else {
        TOOLBAR_HEIGHT
    };
    let (band, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter()
        .rect_filled(band, CornerRadius::ZERO, p.bg_soft);
    ui.painter().hline(
        band.x_range(),
        band.bottom() - theme::HAIRLINE * 0.5,
        Stroke::new(theme::HAIRLINE, p.border),
    );

    let mut result = ToolbarResult {
        scope: None,
        mode: None,
        stage: false,
        unstage: false,
        discard: false,
    };
    if can_stage
        && ui.ui_contains_pointer()
        && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::S))
    {
        if all_staged {
            result.unstage = true;
        } else {
            result.stage = true;
        }
    }
    let mut index = mode.index() as usize;
    let arm_id = Id::unique("changes-discard-all");
    let armed = ui
        .memory(|memory| memory.data.get_temp::<f64>(arm_id))
        .is_some_and(|stamp| ui.input(|input| input.time - stamp < 3.0));

    if narrow {
        let row = egui::Rect::from_min_size(band.min, Vec2::new(width, TOOLBAR_HEIGHT));
        let mut row_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row.shrink2(Vec2::new(8.0, 4.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        if let Some(next) = scope_selector(&mut row_ui, p, scope, files, turn, true) {
            result.scope = Some(next);
        }
        row_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            stats_label(ui, p, additions, deletions);
        });

        let row = egui::Rect::from_min_size(
            egui::Pos2::new(band.left(), band.top() + TOOLBAR_HEIGHT),
            Vec2::new(width, TOOLBAR_HEIGHT),
        );
        let mut row_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row.shrink2(Vec2::new(8.0, 4.0)))
                .layout(Layout::right_to_left(Align::Center)),
        );
        row_ui.spacing_mut().item_spacing.x = 6.0;
        stage_controls(
            &mut row_ui,
            p,
            &mut result,
            all_staged,
            any_staged,
            can_stage,
            armed,
            arm_id,
            true,
            width < TOOLBAR_TIGHT,
        );
        if widgets::segmented_ex(&mut row_ui, &["Unified", "Split"], &mut index, 26.0, p) {
            result.mode = Some(DiffMode::from_index(index as u8));
        }
    } else {
        let row = band.shrink2(Vec2::new(8.0, 4.0));
        let mut row_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(Layout::left_to_right(Align::Center)),
        );
        if let Some(next) = scope_selector(&mut row_ui, p, scope, files, turn, false) {
            result.scope = Some(next);
        }
        row_ui.spacing_mut().item_spacing.x = 8.0;
        row_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            stage_controls(
                ui,
                p,
                &mut result,
                all_staged,
                any_staged,
                can_stage,
                armed,
                arm_id,
                false,
                false,
            );
            stats_label(ui, p, additions, deletions);
            if widgets::segmented_ex(ui, &["Unified", "Split"], &mut index, 26.0, p) {
                result.mode = Some(DiffMode::from_index(index as u8));
            }
        });
    }

    if result.discard {
        ui.memory_mut(|memory| memory.data.remove_temp::<f64>(arm_id));
    }
    result
}

fn stage_controls(
    ui: &mut Ui,
    p: &Palette,
    result: &mut ToolbarResult,
    all_staged: bool,
    any_staged: bool,
    can_stage: bool,
    armed: bool,
    arm_id: Id,
    compact: bool,
    tight: bool,
) {
    let primary = if all_staged {
        widgets::button_ex(
            ui,
            if tight {
                ""
            } else if compact {
                "Unstage"
            } else {
                "Unstage all"
            },
            Some(Icon::Check),
            widgets::ButtonVariant::Outline,
            widgets::ButtonSize::App,
            can_stage && any_staged,
            p,
        )
    } else {
        widgets::button_ex(
            ui,
            if tight {
                ""
            } else if compact {
                "Stage (⌘S)"
            } else {
                "Stage all (⌘S)"
            },
            Some(Icon::CheckCheck),
            widgets::ButtonVariant::Primary,
            widgets::ButtonSize::App,
            can_stage,
            p,
        )
    };
    let primary = if tight {
        primary.on_hover_text(if all_staged {
            "Unstage all files"
        } else {
            "Stage all files (⌘S)"
        })
    } else {
        primary
    };
    if primary.clicked() {
        if all_staged {
            result.unstage = true;
        } else {
            result.stage = true;
        }
    }
    let discard_label = if armed {
        "Confirm discard"
    } else if tight {
        ""
    } else if compact {
        "Discard"
    } else {
        "Discard all"
    };
    let discard = widgets::button_ex(
        ui,
        discard_label,
        Some(Icon::Trash),
        widgets::ButtonVariant::Outline,
        widgets::ButtonSize::App,
        can_stage && any_staged,
        p,
    );
    let discard = if tight && !armed {
        discard.on_hover_text("Discard all files")
    } else {
        discard
    };
    if discard.clicked() {
        if armed {
            result.discard = true;
        } else {
            ui.memory_mut(|memory| {
                memory
                    .data
                    .insert_temp(arm_id, ui.input(|input| input.time))
            });
        }
    }
}

fn stats_label(ui: &mut Ui, p: &Palette, additions: u32, deletions: u32) {
    let font = theme::font_mono(11.5);
    let added = ui
        .painter()
        .layout_no_wrap(format!("+{additions}"), font.clone(), p.green);
    let removed = ui
        .painter()
        .layout_no_wrap(format!("−{deletions}"), font, p.red);
    let width = added.size().x + 8.0 + removed.size().x;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
    ui.painter().galley(
        egui::Pos2::new(rect.left(), rect.center().y - added.size().y * 0.5),
        added,
        p.green,
    );
    ui.painter().galley(
        egui::Pos2::new(
            rect.left() + width - removed.size().x,
            rect.center().y - removed.size().y * 0.5,
        ),
        removed,
        p.red,
    );
}

fn git_path(repo: &Path, path: &str) -> String {
    Path::new(path)
        .strip_prefix(repo)
        .map(|rest| rest.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.trim_start_matches("./").to_string())
}

struct DiffCtx<'a> {
    session: Option<&'a str>,
    scope: &'a Scope,
    mode: DiffMode,
    staged: &'a HashSet<String>,
    can_stage: bool,
    actions: &'a mut Vec<FileAction>,
}

fn diff_tree_mode(
    ui: &mut Ui,
    p: &Palette,
    session: &Option<String>,
    spinning: bool,
) -> (bool, bool) {
    let id = Id::unique(("changes-tree-mode", session.clone()));
    if ui
        .memory(|memory| memory.data.get_temp::<bool>(id))
        .is_none()
    {
        ui.memory_mut(|memory| memory.data.insert_temp(id, true));
    }
    let mut tree = ui
        .memory(|memory| memory.data.get_temp::<bool>(id))
        .unwrap_or(true);
    let mut changed = false;
    let mut reload = false;
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.spacing_mut().item_spacing.x = 2.0;
        if view_toggle(ui, p, Icon::Layers, tree, "Tree view") {
            tree = true;
            changed = true;
        }
        if view_toggle(ui, p, Icon::Menu, !tree, "Flat view") {
            tree = false;
            changed = true;
        }
        ui.label(
            RichText::new(if tree { "Tree" } else { "Flat" })
                .font(theme::font(12.0))
                .color(p.fg_muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(6.0);
            if spinning {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                widgets::paint_spinner(ui, rect.center(), 14.0, p.primary);
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(40));
            } else if surfaces::mini_icon_button(ui, p, Icon::Refresh, "Refresh changes").clicked()
            {
                reload = true;
            }
        });
        if changed {
            ui.memory_mut(|memory| memory.data.insert_temp(id, tree));
        }
    });
    surfaces::hairline(ui, p);
    (tree, reload)
}

fn view_toggle(ui: &mut Ui, p: &Palette, icon: Icon, active: bool, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    if active {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::SM), p.selection);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::SM), p.hover);
    }
    icons::paint(
        ui,
        icon,
        egui::Rect::from_center_size(rect.center(), Vec2::splat(13.0)),
        if active { p.fg } else { p.fg_muted },
    );
    response.on_hover_text(tip).clicked()
}

fn expanded_dirs(ui: &mut Ui, session: &Option<String>, files: &[&FileDiff]) -> HashSet<String> {
    let id = Id::unique(("changes-expanded", session.clone()));
    let initialized = Id::unique(("changes-expanded-init", session.clone()));
    let mut expanded = ui
        .memory(|memory| memory.data.get_temp::<HashSet<String>>(id))
        .unwrap_or_default();
    if !ui
        .memory(|memory| memory.data.get_temp::<bool>(initialized))
        .unwrap_or(false)
    {
        for file in files {
            if let Some((head, _)) = file.file.split_once('/') {
                expanded.insert(head.to_string());
            }
        }
        ui.memory_mut(|memory| memory.data.insert_temp(initialized, true));
    }
    ui.memory_mut(|memory| memory.data.insert_temp(id, expanded.clone()));
    expanded
}

fn tree_rows(
    ui: &mut Ui,
    p: &Palette,
    files: &[&FileDiff],
    expanded: &HashSet<String>,
    diff: &mut DiffCtx,
) {
    let mut nodes: Vec<DirNode> = Vec::new();
    let mut top_files: Vec<usize> = Vec::new();
    for (index, file) in files.iter().enumerate() {
        if file.file.contains('/') {
            insert_path(&mut nodes, "", &file.file, index);
        } else {
            top_files.push(index);
        }
    }
    sort_nodes(&mut nodes);
    let mut toggles: Vec<(String, bool)> = Vec::new();
    ui.add_space(2.0);
    walk(ui, p, &nodes, files, expanded, diff, &mut toggles);
    for index in top_files {
        if let Some(file) = files.get(index) {
            file_block(ui, p, file, 0, true, files.len() == 1, diff);
        }
    }
    if !toggles.is_empty() {
        let id = Id::unique(("changes-expanded", diff.session.map(str::to_string)));
        let mut next = expanded.clone();
        for (path, open) in toggles {
            if open {
                next.insert(path);
            } else {
                next.remove(&path);
            }
        }
        ui.memory_mut(|memory| memory.data.insert_temp(id, next));
    }
}

struct DirNode {
    name: String,
    path: String,
    files: Vec<usize>,
    children: Vec<DirNode>,
}

fn insert_path(nodes: &mut Vec<DirNode>, prefix: &str, path: &str, index: usize) {
    let trimmed = path.trim_start_matches('/');
    let (head, rest) = match trimmed.split_once('/') {
        Some((head, rest)) => (head, Some(rest)),
        None => (trimmed, None),
    };
    let full = if prefix.is_empty() {
        head.to_string()
    } else {
        format!("{prefix}/{head}")
    };
    let position = nodes.iter().position(|node| node.name == head);
    let position = match position {
        Some(position) => position,
        None => {
            nodes.push(DirNode {
                name: head.to_string(),
                path: full.clone(),
                files: Vec::new(),
                children: Vec::new(),
            });
            nodes.len() - 1
        }
    };
    match rest {
        Some(rest) if !rest.is_empty() => {
            insert_path(&mut nodes[position].children, &full, rest, index)
        }
        _ => nodes[position].files.push(index),
    }
}

fn sort_nodes(nodes: &mut [DirNode]) {
    nodes.sort_by(|a, b| {
        (
            a.children.is_empty() && !a.files.is_empty(),
            a.name.to_lowercase(),
        )
            .cmp(&(
                b.children.is_empty() && !b.files.is_empty(),
                b.name.to_lowercase(),
            ))
    });
    for node in nodes {
        sort_nodes(&mut node.children);
    }
}

fn walk(
    ui: &mut Ui,
    p: &Palette,
    nodes: &[DirNode],
    files: &[&FileDiff],
    expanded: &HashSet<String>,
    diff: &mut DiffCtx,
    toggles: &mut Vec<(String, bool)>,
) {
    for node in nodes {
        let depth = node.path.matches('/').count();
        let open = expanded.contains(&node.path);
        let row = dir_row(ui, p, &node.name, depth, open, node.count());
        if row.clicked() {
            toggles.push((node.path.clone(), !open));
        }
        if open {
            walk(ui, p, &node.children, files, expanded, diff, toggles);
            for index in &node.files {
                if let Some(file) = files.get(*index) {
                    file_block(ui, p, file, depth + 1, true, files.len() == 1, diff);
                }
            }
        }
    }
}

impl DirNode {
    fn count(&self) -> usize {
        self.files.len() + self.children.iter().map(DirNode::count).sum::<usize>()
    }
}

fn dir_row(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    depth: usize,
    open: bool,
    count: usize,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    indent_guides(ui, rect, depth, p);
    let hover = crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if hover > 0.001 {
        ui.painter().rect_filled(
            rect.shrink2(Vec2::new(6.0, 0.0)),
            CornerRadius::same(theme::radius::MD),
            p.hover.gamma_multiply(hover),
        );
    }
    let x = rect.left() + 8.0 + depth as f32 * INDENT;
    icons::paint(
        ui,
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        egui::Rect::from_center_size(egui::Pos2::new(x + 7.0, rect.center().y), Vec2::splat(12.0)),
        p.fg_muted,
    );
    let galley = surfaces::truncated_galley(
        ui,
        label.to_string(),
        theme::font(13.0),
        p.fg,
        (rect.right() - x - 70.0).max(20.0),
    );
    ui.painter().galley(
        egui::Pos2::new(x + 20.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg,
    );
    ui.painter().text(
        egui::Pos2::new(rect.right() - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        theme::font(12.0),
        p.fg_muted,
    );
    response
}

fn indent_guides(ui: &Ui, rect: egui::Rect, depth: usize, p: &Palette) {
    for level in 0..depth {
        let x = rect.left() + 8.0 + level as f32 * INDENT + 7.0;
        ui.painter().vline(
            x,
            egui::Rangef::new(rect.top() + 2.0, rect.bottom() - 2.0),
            Stroke::new(0.5, surfaces::edge_muted(p)),
        );
    }
}

fn scope_selector(
    ui: &mut Ui,
    p: &Palette,
    scope: Scope,
    files: usize,
    turn: usize,
    compact: bool,
) -> Option<Scope> {
    let text = match (scope, compact) {
        (Scope::Changed, false) => format!("All session changes ({files} files)"),
        (Scope::Turn, false) => format!("Last turn ({turn} files)"),
        (Scope::Changed, true) => format!("All changes · {files}"),
        (Scope::Turn, true) => format!("Last turn · {turn}"),
    };
    let font = theme::font_medium(12.5);
    let galley = ui.painter().layout_no_wrap(text, font, p.fg);
    let width = (galley.size().x + 16.0 + 16.0 + 8.0).min(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    let hover = crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if hover > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.hover.gamma_multiply(hover),
        );
    }
    ui.painter().galley(
        egui::Pos2::new(rect.left() + 6.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg,
    );
    icons::paint(
        ui,
        Icon::ChevronDown,
        egui::Rect::from_center_size(
            egui::Pos2::new(rect.right() - 13.0, rect.center().y),
            Vec2::splat(13.0),
        ),
        p.fg_muted,
    );
    let menu_id = Id::unique("changes-scope-menu");
    let mut open = ui
        .memory(|memory| memory.data.get_temp::<bool>(menu_id))
        .unwrap_or(false);
    let mut chosen = None;
    let mut close_menu = false;
    egui::Popup::from_response(&response)
        .id(menu_id)
        .open_bool(&mut open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(theme::popover_frame(p))
        .gap(4.0)
        .show(|ui| {
            ui.set_width(190.0);
            for (candidate, label, count) in [
                (Scope::Changed, "All session changes", files),
                (Scope::Turn, "Last turn", turn),
            ] {
                let row = widgets::menu_item(ui, label, None, None, p);
                let count_text = count.to_string();
                let count_galley =
                    ui.painter()
                        .layout_no_wrap(count_text, theme::font(12.5), p.fg_muted);
                ui.painter().galley(
                    egui::Pos2::new(
                        row.rect.right() - 8.0 - count_galley.size().x,
                        row.rect.center().y - count_galley.size().y * 0.5,
                    ),
                    count_galley,
                    p.fg_muted,
                );
                if candidate == scope {
                    icons::paint(
                        ui,
                        Icon::Check,
                        egui::Rect::from_center_size(
                            egui::Pos2::new(row.rect.right() - 26.0, row.rect.center().y),
                            Vec2::splat(12.0),
                        ),
                        p.primary,
                    );
                }
                if row.clicked() && candidate != scope {
                    chosen = Some(candidate);
                    close_menu = true;
                }
            }
        });
    ui.memory_mut(|memory| memory.data.insert_temp(menu_id, open && !close_menu));
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
    }
    chosen
}

fn empty_state(ui: &mut Ui, p: &Palette, scope: Scope) {
    let (title, hint) = match scope {
        Scope::Changed => (
            "No changes yet",
            "Edits made by the agent will show up here.",
        ),
        Scope::Turn => (
            "No changes in this turn",
            "Switch to Changed to see the whole session.",
        ),
    };
    surfaces::empty_state(ui, p, Icon::Diff, title, hint);
}

fn file_block(
    ui: &mut Ui,
    p: &Palette,
    file: &FileDiff,
    depth: usize,
    tree: bool,
    single: bool,
    diff: &mut DiffCtx,
) {
    let open_id = open_id(&diff.session.map(str::to_string), diff.scope, &file.file);
    let key = format!(
        "{}:{}:{}",
        diff.session.unwrap_or("session"),
        diff.scope.index(),
        file.file
    );
    let staged = file_is_staged(diff.staged, &file.file);
    let mut open = ui
        .memory(|memory| memory.data.get_temp::<bool>(open_id))
        .unwrap_or(single);
    let name = if tree {
        file_name(&file.file)
    } else {
        file.file.clone()
    };

    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), FILE_HEADER_HEIGHT),
        Sense::click(),
    );
    let mut copy_clicked = false;
    if ui.is_rect_visible(rect) {
        indent_guides(ui, rect, depth, p);
        let x = rect.left() + 8.0 + depth as f32 * INDENT;
        if open {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(theme::radius::MD),
                p.scale_alpha(p.selection, 0.35),
            );
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.hover);
        }
        icons::paint(
            ui,
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            egui::Rect::from_center_size(
                egui::Pos2::new(x + 9.0, rect.center().y),
                Vec2::splat(13.0),
            ),
            p.fg_muted,
        );
        let status = file.status.clone().unwrap_or_else(|| "modified".into());
        let (letter, color) = status_mark(&status, p);
        surfaces::status_letter(
            ui,
            egui::Rect::from_center_size(
                egui::Pos2::new(x + 29.0, rect.center().y),
                Vec2::new(16.0, 16.0),
            ),
            p,
            letter,
            color,
        );
        let (icon, tint) = file_visual(&file.file, p);
        icons::paint(
            ui,
            icon,
            egui::Rect::from_center_size(
                egui::Pos2::new(x + 46.0, rect.center().y),
                Vec2::splat(14.0),
            ),
            tint,
        );

        let actions_width = if diff.can_stage { 68.0 } else { 24.0 };
        let stats_width = 84.0;
        let text_left = x + 58.0;
        let text_right = rect.right() - stats_width - actions_width - 8.0;
        let (path_color, name_color) = match file.file.rsplit_once('/') {
            Some(_) if tree => (p.fg_muted, p.fg),
            _ => (p.fg, p.fg),
        };
        let dir_label = if tree {
            String::new()
        } else {
            file.file
                .rsplit_once('/')
                .map(|(dir, _)| format!("{dir}/"))
                .unwrap_or_default()
        };
        let dir_galley = surfaces::truncated_galley(
            ui,
            dir_label,
            theme::font(13.0),
            path_color,
            ((text_right - text_left) * 0.52).max(20.0),
        );
        ui.painter().galley(
            egui::Pos2::new(text_left, rect.center().y - dir_galley.size().y * 0.5),
            dir_galley.clone(),
            path_color,
        );
        let name_width = (text_right - text_left - dir_galley.size().x).max(24.0);
        let name_galley =
            surfaces::truncated_galley(ui, name.clone(), theme::font(13.0), name_color, name_width);
        ui.painter().galley(
            egui::Pos2::new(
                text_left + dir_galley.size().x,
                rect.center().y - name_galley.size().y * 0.5,
            ),
            name_galley,
            name_color,
        );

        let stats_right = rect.right() - actions_width - 8.0;
        let added = format!("+{}", file.additions);
        let removed = format!("−{}", file.deletions);
        ui.painter().text(
            egui::Pos2::new(stats_right, rect.center().y),
            Align2::RIGHT_CENTER,
            &removed,
            theme::font_mono(11.5),
            p.red,
        );
        let removed_width = ui
            .painter()
            .layout_no_wrap(removed, theme::font_mono(11.5), p.red)
            .size()
            .x;
        ui.painter().text(
            egui::Pos2::new(stats_right - 2.0 - removed_width, rect.center().y),
            Align2::RIGHT_CENTER,
            &added,
            theme::font_mono(11.5),
            p.green,
        );
        let copy_rect = egui::Rect::from_center_size(
            egui::Pos2::new(rect.right() - 14.0, rect.center().y),
            Vec2::splat(20.0),
        );
        if diff.can_stage {
            let stage_rect = egui::Rect::from_center_size(
                egui::Pos2::new(rect.right() - 36.0, rect.center().y),
                Vec2::splat(20.0),
            );
            let stage = ui.interact(
                stage_rect,
                Id::unique(("diff-stage", key.clone())),
                Sense::click(),
            );
            if stage.hovered() {
                ui.painter().rect_filled(
                    stage_rect,
                    CornerRadius::same(theme::radius::XS),
                    p.hover,
                );
            }
            icons::paint(
                ui,
                if staged {
                    Icon::CheckCheck
                } else {
                    Icon::Check
                },
                egui::Rect::from_center_size(stage_rect.center(), Vec2::splat(12.0)),
                if staged { p.success_text } else { p.fg_muted },
            );
            let tip = if staged { "Unstage file" } else { "Stage file" };
            let stage = stage.on_hover_text(tip);
            if stage.clicked() {
                copy_clicked = true;
                diff.actions.push(if staged {
                    FileAction::Unstage(file.file.clone())
                } else {
                    FileAction::Stage(file.file.clone())
                });
            }

            let discard_rect = egui::Rect::from_center_size(
                egui::Pos2::new(rect.right() - 58.0, rect.center().y),
                Vec2::splat(20.0),
            );
            let arm_id = Id::unique(("diff-discard", key.clone()));
            let armed = ui
                .memory(|memory| memory.data.get_temp::<f64>(arm_id))
                .is_some_and(|stamp| ui.input(|input| input.time - stamp < 3.0));
            let discard = ui.interact(
                discard_rect,
                Id::unique(("diff-discard-hit", key.clone())),
                Sense::click(),
            );
            if discard.hovered() {
                ui.painter().rect_filled(
                    discard_rect,
                    CornerRadius::same(theme::radius::XS),
                    p.hover,
                );
            }
            icons::paint(
                ui,
                Icon::Trash,
                egui::Rect::from_center_size(discard_rect.center(), Vec2::splat(12.0)),
                if armed { p.red } else { p.fg_muted },
            );
            let tip = if armed {
                "Click again to discard"
            } else {
                "Discard file"
            };
            let discard = discard.on_hover_text(tip);
            if discard.clicked() {
                copy_clicked = true;
                if armed {
                    ui.memory_mut(|memory| memory.data.remove_temp::<f64>(arm_id));
                    diff.actions.push(FileAction::Discard(file.file.clone()));
                } else {
                    ui.memory_mut(|memory| {
                        memory
                            .data
                            .insert_temp(arm_id, ui.input(|input| input.time))
                    });
                }
            }
        }
        let copy = ui.interact(
            copy_rect,
            Id::unique(("diff-copy", key.clone())),
            Sense::click(),
        );
        if copy.hovered() {
            ui.painter()
                .rect_filled(copy_rect, CornerRadius::same(theme::radius::XS), p.hover);
        }
        icons::paint(
            ui,
            Icon::Copy,
            egui::Rect::from_center_size(copy_rect.center(), Vec2::splat(12.0)),
            if copy.hovered() { p.fg } else { p.fg_muted },
        );
        let copy = copy.on_hover_text("Copy path");
        if copy.clicked() {
            copy_clicked = true;
            ui.ctx().copy_text(file.file.clone());
        }
    }
    let response = response.on_hover_text(file.file.clone());
    if response.clicked() && !copy_clicked {
        open = !open;
        ui.memory_mut(|memory| memory.data.insert_temp(open_id, open));
    }
    if !open {
        return;
    }
    if file.additions + file.deletions > GATE_LINES as u32 {
        let gate_id = Id::unique(("diff-gate", key.clone()));
        let force = ui
            .memory(|memory| memory.data.get_temp::<bool>(gate_id))
            .unwrap_or(false);
        if !force {
            ui.add_space(4.0);
            if gate(ui, p, file.additions + file.deletions) {
                ui.memory_mut(|memory| memory.data.insert_temp(gate_id, true));
            }
            return;
        }
    }
    let limit_id = Id::unique(("diff-limit", key.clone()));
    let mut limit = ui
        .memory(|memory| memory.data.get_temp::<usize>(limit_id))
        .unwrap_or(PAGE_LINES);
    let total = patch_total(ui.ctx(), &key, &file.patch);
    patch_view(
        ui,
        p,
        &key,
        &file.file,
        &file.patch,
        limit,
        staged,
        diff.mode,
    );
    if total > limit {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0 + depth as f32 * INDENT);
            let remaining = total - limit;
            if widgets::ghost_button(ui, &format!("Show {remaining} more lines"), None, p).clicked()
            {
                limit += PAGE_LINES;
                ui.memory_mut(|memory| memory.data.insert_temp(limit_id, limit));
            }
        });
    } else if file.more.unwrap_or(false) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0 + depth as f32 * INDENT);
            ui.label(
                RichText::new("Diff truncated by the server.")
                    .font(theme::font(12.0))
                    .color(p.fg_muted),
            );
        });
    }
    ui.add_space(2.0);
}

fn gate(ui: &mut Ui, p: &Palette, lines: u32) -> bool {
    let mut render = false;
    ui.vertical_centered(|ui| {
        ui.add_space(16.0);
        ui.label(
            RichText::new(format!("Large diff ({lines} changed lines)"))
                .font(theme::font_semibold(13.5))
                .color(p.fg),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("Rendering a diff this size costs a frame budget every scroll.")
                .font(theme::font(12.5))
                .color(p.fg_muted),
        );
        ui.add_space(6.0);
        if ui
            .add(
                egui::Label::new(
                    RichText::new("Render anyway")
                        .font(theme::font(13.5))
                        .color(p.primary),
                )
                .sense(Sense::click()),
            )
            .clicked()
        {
            render = true;
        }
        ui.add_space(16.0);
    });
    render
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone)]
struct Cell {
    old: Option<u32>,
    new: Option<u32>,
    sign: char,
    text: String,
    kind: LineKind,
    emph: Option<(usize, usize)>,
}

#[derive(Clone)]
enum RowKind {
    Meta(String),
    Hunk {
        text: String,
        raw: String,
    },
    Line(Cell),
    Split {
        left: Option<Cell>,
        right: Option<Cell>,
    },
}

#[derive(Clone)]
struct PatchRow {
    kind: RowKind,
    top: f32,
    height: f32,
}

struct HunkBlock {
    header: String,
    lines: Vec<String>,
}

fn parse_blocks(patch: &str, limit: usize) -> Vec<HunkBlock> {
    let mut blocks: Vec<HunkBlock> = Vec::new();
    let mut current: Option<HunkBlock> = None;
    for raw in patch.split('\n').take(limit) {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with("@@") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            current = Some(HunkBlock {
                header: line.to_string(),
                lines: Vec::new(),
            });
        } else if let Some(block) = current.as_mut() {
            block.lines.push(line.to_string());
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
}

fn hunk_starts(header: &str) -> (u32, u32) {
    let mut parts = header.split_whitespace();
    parts.next();
    let old = parts
        .next()
        .and_then(|part| part.trim_start_matches('-').split(',').next())
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    let new = parts
        .next()
        .and_then(|part| part.trim_start_matches('+').split(',').next())
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    (old.max(1), new.max(1))
}

fn expand_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

fn trim_span(chars: &[char], start: usize, end: usize) -> Option<(usize, usize)> {
    let mut start = start;
    let mut end = end;
    while start < end && chars[start].is_whitespace() {
        start += 1;
    }
    while end > start && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    (start < end).then_some((start, end))
}

fn intraline(removed: &str, added: &str) -> (Option<(usize, usize)>, Option<(usize, usize)>) {
    let removed: Vec<char> = removed.chars().collect();
    let added: Vec<char> = added.chars().collect();
    let mut prefix = 0;
    while prefix < removed.len() && prefix < added.len() && removed[prefix] == added[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < removed.len().saturating_sub(prefix)
        && suffix < added.len().saturating_sub(prefix)
        && removed[removed.len() - 1 - suffix] == added[added.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let left = trim_span(&removed, prefix, removed.len() - suffix);
    let right = trim_span(&added, prefix, added.len() - suffix);
    (left, right)
}

fn flush_pairs(
    rows: &mut Vec<(RowKind, f32)>,
    removals: &mut Vec<Cell>,
    additions: &mut Vec<Cell>,
    mode: DiffMode,
) {
    let count = removals.len().max(additions.len());
    if count == 0 {
        return;
    }
    let mut pairs: Vec<(Option<Cell>, Option<Cell>)> = Vec::with_capacity(count);
    for index in 0..count {
        let mut left = removals.get(index).cloned();
        let mut right = additions.get(index).cloned();
        if let (Some(removed), Some(added)) = (left.as_mut(), right.as_mut()) {
            let (left_span, right_span) = intraline(&removed.text, &added.text);
            removed.emph = left_span;
            added.emph = right_span;
        }
        pairs.push((left, right));
    }
    if mode == DiffMode::Split {
        for (left, right) in pairs {
            rows.push((RowKind::Split { left, right }, ROW_HEIGHT));
        }
    } else {
        for (left, _) in &pairs {
            if let Some(cell) = left {
                rows.push((RowKind::Line(cell.clone()), ROW_HEIGHT));
            }
        }
        for (_, right) in &pairs {
            if let Some(cell) = right {
                rows.push((RowKind::Line(cell.clone()), ROW_HEIGHT));
            }
        }
    }
    removals.clear();
    additions.clear();
}

fn build_rows(patch: &str, limit: usize, mode: DiffMode) -> Vec<PatchRow> {
    let mut rows: Vec<(RowKind, f32)> = Vec::new();
    for block in parse_blocks(patch, limit) {
        let (mut old, mut new) = hunk_starts(&block.header);
        let raw = if block.lines.is_empty() {
            block.header.clone()
        } else {
            format!("{}\n{}", block.header, block.lines.join("\n"))
        };
        rows.push((
            RowKind::Hunk {
                text: block.header.clone(),
                raw,
            },
            HUNK_HEIGHT,
        ));
        let mut removals: Vec<Cell> = Vec::new();
        let mut additions: Vec<Cell> = Vec::new();
        for line in &block.lines {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            let sign = line.chars().next().unwrap_or(' ');
            let text = expand_tabs(line.get(1..).unwrap_or(""));
            match sign {
                '+' => {
                    additions.push(Cell {
                        old: None,
                        new: Some(new),
                        sign: '+',
                        text,
                        kind: LineKind::Added,
                        emph: None,
                    });
                    new += 1;
                }
                '-' => {
                    removals.push(Cell {
                        old: Some(old),
                        new: None,
                        sign: '-',
                        text,
                        kind: LineKind::Removed,
                        emph: None,
                    });
                    old += 1;
                }
                ' ' => {
                    flush_pairs(&mut rows, &mut removals, &mut additions, mode);
                    if mode == DiffMode::Split {
                        rows.push((
                            RowKind::Split {
                                left: Some(Cell {
                                    old: Some(old),
                                    new: None,
                                    sign: ' ',
                                    text: text.clone(),
                                    kind: LineKind::Context,
                                    emph: None,
                                }),
                                right: Some(Cell {
                                    old: None,
                                    new: Some(new),
                                    sign: ' ',
                                    text,
                                    kind: LineKind::Context,
                                    emph: None,
                                }),
                            },
                            ROW_HEIGHT,
                        ));
                    } else {
                        rows.push((
                            RowKind::Line(Cell {
                                old: Some(old),
                                new: Some(new),
                                sign: ' ',
                                text,
                                kind: LineKind::Context,
                                emph: None,
                            }),
                            ROW_HEIGHT,
                        ));
                    }
                    old += 1;
                    new += 1;
                }
                '\\' => {
                    flush_pairs(&mut rows, &mut removals, &mut additions, mode);
                    rows.push((RowKind::Meta(line.to_string()), META_HEIGHT));
                }
                _ => {
                    flush_pairs(&mut rows, &mut removals, &mut additions, mode);
                    rows.push((RowKind::Meta(line.to_string()), META_HEIGHT));
                }
            }
        }
        flush_pairs(&mut rows, &mut removals, &mut additions, mode);
    }
    let mut top = 0.0;
    rows.into_iter()
        .map(|(kind, height)| {
            let row = PatchRow { kind, top, height };
            top += height;
            row
        })
        .collect()
}

fn patch_rows(ui: &Ui, key: &str, patch: &str, limit: usize, mode: DiffMode) -> Arc<Vec<PatchRow>> {
    let id = Id::unique(("patch-rows", key, patch.len(), limit, mode.index()));
    if let Some(rows) = ui
        .ctx()
        .data(|data| data.get_temp::<Arc<Vec<PatchRow>>>(id))
    {
        return rows;
    }
    let rows = Arc::new(build_rows(patch, limit, mode));
    ui.ctx().data_mut(|data| data.insert_temp(id, rows.clone()));
    rows
}

fn line_color(kind: LineKind, p: &Palette) -> Color32 {
    match kind {
        LineKind::Added => p.diff_added,
        LineKind::Removed => p.diff_removed,
        LineKind::Context => p.body,
    }
}

fn line_background(kind: LineKind, p: &Palette) -> Option<Color32> {
    match kind {
        LineKind::Added => Some(p.over(p.code_bg, p.diff_added_bg)),
        LineKind::Removed => Some(p.over(p.code_bg, p.diff_removed_bg)),
        LineKind::Context => None,
    }
}

fn emph_color(kind: LineKind, p: &Palette) -> Color32 {
    match kind {
        LineKind::Added => p.over(p.code_bg, p.diff_added_emph),
        LineKind::Removed => p.over(p.code_bg, p.diff_removed_emph),
        LineKind::Context => Color32::TRANSPARENT,
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_cell(
    painter: &egui::Painter,
    p: &Palette,
    origin: f32,
    width: f32,
    row: egui::Rect,
    cell: &Cell,
    font: &egui::FontId,
    num_font: &egui::FontId,
    char_width: f32,
    columns: usize,
) {
    let clip = egui::Rect::from_min_size(
        egui::Pos2::new(origin, row.top()),
        Vec2::new(width, row.height()),
    );
    if let Some(background) = line_background(cell.kind, p) {
        painter.rect_filled(clip, CornerRadius::ZERO, background);
    }
    let center_y = row.center().y;
    let numbers = if columns >= 2 {
        [cell.old, cell.new]
    } else {
        [cell.old.or(cell.new), None]
    };
    for (index, number) in numbers.iter().enumerate() {
        if let Some(number) = number {
            painter.text(
                egui::Pos2::new(origin + NUM_WIDTH * (index + 1) as f32 - 8.0, center_y),
                Align2::RIGHT_CENTER,
                number.to_string(),
                num_font.clone(),
                p.fg_muted,
            );
        }
    }
    let sign_x = origin + NUM_WIDTH * columns as f32;
    painter.text(
        egui::Pos2::new(sign_x + SIGN_WIDTH * 0.5, center_y),
        Align2::CENTER_CENTER,
        cell.sign.to_string(),
        font.clone(),
        line_color(cell.kind, p),
    );
    let code_x = sign_x + SIGN_WIDTH + CODE_PAD;
    if let Some((start, end)) = cell.emph {
        if end > start {
            let highlight = egui::Rect::from_min_max(
                egui::Pos2::new(code_x + start as f32 * char_width, row.top() + 3.0),
                egui::Pos2::new(code_x + end as f32 * char_width, row.bottom() - 3.0),
            );
            painter.rect_filled(highlight, CornerRadius::same(2), emph_color(cell.kind, p));
        }
    }
    painter.with_clip_rect(clip).text(
        egui::Pos2::new(code_x, center_y),
        Align2::LEFT_CENTER,
        &cell.text,
        font.clone(),
        line_color(cell.kind, p),
    );
}

fn patch_view(
    ui: &mut Ui,
    p: &Palette,
    key: &str,
    path: &str,
    patch: &str,
    limit: usize,
    staged: bool,
    mode: DiffMode,
) {
    let font = theme::font_mono(DIFF_FONT);
    let num_font = theme::font_mono(11.0);
    let char_width = ui
        .painter()
        .layout_no_wrap("0000000000".to_string(), font.clone(), p.fg)
        .size()
        .x
        / 10.0;
    let rows = patch_rows(ui, key, patch, limit, mode);
    let columns = patch_columns(ui.ctx(), key, patch);
    let content_height = rows
        .last()
        .map(|row| row.top + row.height)
        .unwrap_or(ROW_HEIGHT)
        + 4.0;
    let needed = match mode {
        DiffMode::Unified => {
            NUM_WIDTH * 2.0 + SIGN_WIDTH + CODE_PAD * 2.0 + columns as f32 * char_width
        }
        DiffMode::Split => {
            (NUM_WIDTH + SIGN_WIDTH + CODE_PAD + columns as f32 * char_width) * 2.0 + 1.0
        }
    };

    ScrollArea::both()
        .id_salt(("patch", key, mode.index()))
        .max_height(PATCH_MAX_HEIGHT)
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let width = needed.max(ui.available_width());
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(width, content_height), Sense::hover());
            let painter = ui.painter().clone();
            painter.rect_filled(rect, CornerRadius::ZERO, p.code_bg);
            let mut attach: Option<String> = None;
            for (index, row) in rows.iter().enumerate() {
                let top = rect.top() + row.top;
                if top + row.height < viewport.min.y {
                    continue;
                }
                if top > viewport.max.y {
                    break;
                }
                let row_rect = egui::Rect::from_min_size(
                    egui::Pos2::new(rect.left(), top),
                    Vec2::new(width, row.height),
                );
                match &row.kind {
                    RowKind::Meta(text) => {
                        painter.text(
                            egui::Pos2::new(rect.left() + 8.0, row_rect.center().y),
                            Align2::LEFT_CENTER,
                            text,
                            theme::font_mono(11.0),
                            p.fg_faint,
                        );
                    }
                    RowKind::Hunk { text, raw } => {
                        painter.rect_filled(row_rect, CornerRadius::ZERO, p.card);
                        painter.hline(
                            row_rect.x_range(),
                            row_rect.top() + theme::HAIRLINE * 0.5,
                            Stroke::new(theme::HAIRLINE, p.border),
                        );
                        painter.hline(
                            row_rect.x_range(),
                            row_rect.bottom() - theme::HAIRLINE * 0.5,
                            Stroke::new(theme::HAIRLINE, p.border),
                        );
                        let action_width = if staged { 96.0 } else { 156.0 };
                        let text_width = (width - action_width - 20.0).max(40.0);
                        painter
                            .with_clip_rect(egui::Rect::from_min_size(
                                row_rect.min,
                                Vec2::new(text_width, row_rect.height()),
                            ))
                            .text(
                                egui::Pos2::new(rect.left() + 10.0, row_rect.center().y),
                                Align2::LEFT_CENTER,
                                text,
                                theme::font_mono(11.5),
                                p.fg_muted,
                            );
                        if staged {
                            painter.text(
                                egui::Pos2::new(rect.right() - 12.0, row_rect.center().y),
                                Align2::RIGHT_CENTER,
                                "✓ Staged hunk",
                                theme::font(11.5),
                                p.success_text,
                            );
                        } else {
                            let label = "+ Attach hunk to chat";
                            let galley = painter.layout_no_wrap(
                                label.to_string(),
                                theme::font(11.5),
                                p.primary_text,
                            );
                            let action_rect = egui::Rect::from_min_max(
                                egui::Pos2::new(
                                    rect.right() - 12.0 - galley.size().x,
                                    row_rect.top(),
                                ),
                                egui::Pos2::new(rect.right() - 12.0, row_rect.bottom()),
                            );
                            let response = ui.interact(
                                action_rect,
                                Id::unique(("attach", key, index)),
                                Sense::click(),
                            );
                            let color = if response.hovered() {
                                p.fg
                            } else {
                                p.primary_text
                            };
                            painter.galley(
                                egui::Pos2::new(
                                    action_rect.left(),
                                    row_rect.center().y - galley.size().y * 0.5,
                                ),
                                galley,
                                color,
                            );
                            if response.clicked() {
                                attach = Some(format!("{}\n```diff\n{}\n```", path, raw));
                            }
                        }
                    }
                    RowKind::Line(cell) => {
                        paint_cell(
                            &painter,
                            p,
                            rect.left(),
                            width,
                            row_rect,
                            cell,
                            &font,
                            &num_font,
                            char_width,
                            2,
                        );
                    }
                    RowKind::Split { left, right } => {
                        let half = ((width - 1.0) * 0.5).max(80.0);
                        painter.vline(
                            rect.left() + half,
                            row_rect.y_range(),
                            Stroke::new(theme::HAIRLINE, p.border),
                        );
                        if let Some(cell) = left {
                            paint_cell(
                                &painter,
                                p,
                                rect.left(),
                                half,
                                row_rect,
                                cell,
                                &font,
                                &num_font,
                                char_width,
                                1,
                            );
                        }
                        if let Some(cell) = right {
                            paint_cell(
                                &painter,
                                p,
                                rect.left() + half + 1.0,
                                half,
                                row_rect,
                                cell,
                                &font,
                                &num_font,
                                char_width,
                                1,
                            );
                        }
                    }
                }
            }
            if let Some(text) = attach {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(Id::unique("rchamber-compose"), text);
                });
            }
        });
}

fn patch_total(ctx: &egui::Context, key: &str, patch: &str) -> usize {
    let id = Id::unique(("patch-total", key, patch.len()));
    if let Some(total) = ctx.memory(|memory| memory.data.get_temp::<usize>(id)) {
        return total;
    }
    let total = patch.split('\n').count();
    ctx.memory_mut(|memory| memory.data.insert_temp(id, total));
    total
}

fn patch_columns(ctx: &egui::Context, key: &str, patch: &str) -> usize {
    let id = Id::unique(("patch-columns", key, patch.len()));
    if let Some(columns) = ctx.memory(|memory| memory.data.get_temp::<usize>(id)) {
        return columns;
    }
    let mut columns = 1;
    for line in patch.lines() {
        let mut width = 0;
        for character in line.chars() {
            if width >= MAX_PATCH_COLUMNS {
                break;
            }
            if character == '\t' {
                width += 4 - width % 4;
            } else {
                width += 1;
            }
        }
        columns = columns.max(width);
    }
    ctx.memory_mut(|memory| memory.data.insert_temp(id, columns));
    columns
}

fn status_mark(status: &str, p: &Palette) -> (&'static str, Color32) {
    match status.to_ascii_lowercase().as_str() {
        "added" | "untracked" => ("A", p.green),
        "deleted" | "removed" => ("D", p.red),
        "renamed" => ("R", p.blue),
        "copied" => ("C", p.blue),
        "conflicted" | "conflict" => ("!", p.coral),
        "modified" | "changed" => ("M", p.diff_modified),
        _ => ("M", p.diff_modified),
    }
}

fn file_visual(path: &str, p: &Palette) -> (Icon, Color32) {
    let extension = path
        .rsplit_once('.')
        .map(|(_, value)| value.to_ascii_lowercase())
        .unwrap_or_default();
    let icon = match extension.as_str() {
        "sh" | "bash" | "zsh" => Icon::Terminal,
        _ => Icon::File,
    };
    (icon, p.fg_muted)
}

fn open_id(session: &Option<String>, scope: &Scope, path: &str) -> Id {
    Id::unique((
        "diff-open",
        session.as_deref().unwrap_or("session"),
        scope.index(),
        path,
    ))
}

fn file_name(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn last_turn_paths(chat: &rchamber_app::Chat) -> Vec<String> {
    let mut paths = Vec::new();
    for message in chat.messages.iter().rev() {
        match message {
            Message::User(_) => break,
            Message::Assistant(assistant) => {
                for part in &assistant.content {
                    let Part::Tool(tool) = part else {
                        continue;
                    };
                    let Some(input) = tool.state.input() else {
                        continue;
                    };
                    for key in ["filePath", "path", "file"] {
                        let Some(value) = input.get(key).and_then(|value| value.as_str()) else {
                            continue;
                        };
                        if !paths.iter().any(|path| path == value) {
                            paths.push(value.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    paths
}

fn path_matches(candidate: &str, file: &str) -> bool {
    !candidate.is_empty()
        && (candidate == file
            || candidate.trim_start_matches("./") == file
            || file.ends_with(candidate)
            || candidate.ends_with(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch() -> &'static str {
        "@@ -1,1 +1,1 @@ impl Palette {\n-    let a = 1;\n+    let a = 2;\n     tail();\n"
    }

    #[test]
    fn hunk_headers_expose_both_line_starts() {
        assert_eq!(hunk_starts("@@ -118,6 +118,14 @@"), (118, 118));
        assert_eq!(hunk_starts("@@ -4 +9 @@"), (4, 9));
        assert_eq!(hunk_starts("@@ malformed @@"), (1, 1));
    }

    #[test]
    fn unified_rows_keep_one_number_column_per_side() {
        let rows = build_rows(patch(), 100, DiffMode::Unified);
        assert_eq!(rows.len(), 4);
        match &rows[1].kind {
            RowKind::Line(cell) => {
                assert_eq!(cell.old, Some(1));
                assert_eq!(cell.new, None);
                assert_eq!(cell.sign, '-');
            }
            _ => panic!("expected a removed line"),
        }
        match &rows[2].kind {
            RowKind::Line(cell) => {
                assert_eq!(cell.old, None);
                assert_eq!(cell.new, Some(1));
                assert_eq!(cell.sign, '+');
            }
            _ => panic!("expected an added line"),
        }
        match &rows[3].kind {
            RowKind::Line(cell) => {
                assert_eq!(cell.old, Some(2));
                assert_eq!(cell.new, Some(2));
                assert_eq!(cell.sign, ' ');
            }
            _ => panic!("expected a context line"),
        }
    }

    #[test]
    fn split_rows_pair_changes_side_by_side() {
        let rows = build_rows(patch(), 100, DiffMode::Split);
        assert_eq!(rows.len(), 3);
        match &rows[1].kind {
            RowKind::Split { left, right } => {
                let left = left.as_ref().unwrap();
                let right = right.as_ref().unwrap();
                assert_eq!(left.old, Some(1));
                assert_eq!(right.new, Some(1));
                assert!(left.emph.is_some() && right.emph.is_some());
            }
            _ => panic!("expected a split change row"),
        }
        match &rows[2].kind {
            RowKind::Split { left, right } => {
                assert_eq!(left.as_ref().unwrap().old, Some(2));
                assert_eq!(right.as_ref().unwrap().new, Some(2));
            }
            _ => panic!("expected a split context row"),
        }
    }

    #[test]
    fn intraline_marks_only_the_changed_token() {
        let (removed, added) = intraline("let a = 1;", "let a = 2;");
        let removed_chars: Vec<char> = "let a = 1;".chars().collect();
        let added_chars: Vec<char> = "let a = 2;".chars().collect();
        let (start, end) = removed.expect("removed span");
        assert_eq!(removed_chars[start..end].iter().collect::<String>(), "1");
        let (start, end) = added.expect("added span");
        assert_eq!(added_chars[start..end].iter().collect::<String>(), "2");
    }

    #[test]
    fn diff_workbench_renders_at_panel_and_wide_widths() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(900.0, 700.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            for width in [320.0f32, 720.0] {
                for mode in [DiffMode::Unified, DiffMode::Split] {
                    let key = format!("render-{width}-{}", mode.index());
                    ui.allocate_ui_with_layout(
                        Vec2::new(width, 420.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(width);
                            patch_view(ui, &p, &key, "src/lib.rs", patch(), 100, false, mode);
                        },
                    );
                }
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn references_to_the_mock_metrics_hold() {
        assert_eq!(TOOLBAR_HEIGHT, 36.0);
        assert_eq!(HUNK_HEIGHT, 24.0);
        assert_eq!(ROW_HEIGHT, 22.0);
        assert_eq!(DIFF_FONT, 12.0);
        assert_eq!(NUM_WIDTH, 38.0);
        assert_eq!(SIGN_WIDTH, 22.0);
    }
}
