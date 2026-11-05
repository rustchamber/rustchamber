use std::collections::HashSet;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, Layout, RichText, ScrollArea, Sense, Stroke,
    Ui, Vec2,
};
use rchamber_api::models::{FileDiff, Message, Part};
use rchamber_app::format::{DiffLine, diff_line_style};

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::surfaces;
use crate::theme::{self, Palette};
use crate::widgets;

const GATE_LINES: usize = 500;
const PAGE_LINES: usize = 400;
const FILE_HEADER_HEIGHT: f32 = 36.0;
const ROW_HEIGHT: f32 = 20.0;
const PATCH_MAX_HEIGHT: f32 = 360.0;
const MAX_PATCH_COLUMNS: usize = 4000;
const INDENT: f32 = 12.0;

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
    let session_id = app.model.active_id().map(str::to_string);
    let active = app.model.active.as_ref();
    let files: &[FileDiff] = active.map_or(&[], |chat| chat.diff.as_slice());

    let scope_id = Id::unique(("changes-scope", session_id.clone()));
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
    let turn_count = Some(turn_files.len());

    let now = ui.input(|input| input.time);
    let spin_id = Id::unique(("changes-spin", session_id.clone()));
    let spinning = ui
        .memory(|memory| memory.data.get_temp::<f64>(spin_id))
        .is_some_and(|until| now < until);
    let mut close = false;
    let mut toggle_expanded = false;
    let expanded = app.ui.expanded;
    let mut next_scope = scope;

    surfaces::panel_header(
        ui,
        &p,
        &[],
        |ui| {
            if let Some(candidate) = scope_selector(ui, &p, scope, files.len(), turn_count) {
                next_scope = candidate;
            }
            let used = ui.cursor().min.x - ui.max_rect().left();
            let remaining = ui.max_rect().width() - used;
            if remaining > 110.0 {
                let y = ui.max_rect().center().y;
                let mut right = ui.max_rect().right() - 4.0;
                let removed = format!("−{deletions}");
                let removed_galley = ui
                    .painter()
                    .layout_no_wrap(removed, theme::font(12.0), p.red);
                right -= removed_galley.size().x;
                ui.painter().galley(
                    egui::Pos2::new(right, y - removed_galley.size().y * 0.5),
                    removed_galley,
                    p.red,
                );
                right -= 8.0;
                let added = format!("+{additions}");
                let added_galley = ui
                    .painter()
                    .layout_no_wrap(added, theme::font(12.0), p.green);
                right -= added_galley.size().x;
                ui.painter().galley(
                    egui::Pos2::new(right, y - added_galley.size().y * 0.5),
                    added_galley,
                    p.green,
                );
                if remaining > 260.0 {
                    right -= 12.0;
                    let label = format!("{} files changed", shown.len());
                    let galley = ui
                        .painter()
                        .layout_no_wrap(label, theme::font(12.5), p.fg_muted);
                    right -= galley.size().x;
                    ui.painter().galley(
                        egui::Pos2::new(right, y - galley.size().y * 0.5),
                        galley,
                        p.fg_muted,
                    );
                }
            }
        },
        |ui| {
            if surfaces::close_button(ui, &p).clicked() {
                close = true;
            }
            if surfaces::expand_button(ui, &p, expanded).clicked() {
                toggle_expanded = true;
            }
        },
    );

    if next_scope != scope {
        ui.memory_mut(|memory| memory.data.insert_temp(scope_id, next_scope.index()));
    }
    if close {
        app.ui.show_changes = false;
        app.ui.context_open = false;
    }
    if toggle_expanded {
        app.ui.expanded = !app.ui.expanded;
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
        return;
    }

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

    ScrollArea::vertical()
        .id_salt(("changes-scroll", session_id.clone()))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if tree {
                tree_rows(ui, &p, &shown, &expanded, session_id.as_deref(), &scope);
            } else {
                ui.add_space(2.0);
                let single = shown.len() == 1;
                for file in &shown {
                    file_block(
                        ui,
                        &p,
                        file,
                        0,
                        false,
                        single,
                        session_id.as_deref(),
                        &scope,
                    );
                }
            }
            ui.add_space(16.0);
        });
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
    session: Option<&str>,
    scope: &Scope,
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
    walk(ui, p, &nodes, files, expanded, session, scope, &mut toggles);
    for index in top_files {
        if let Some(file) = files.get(index) {
            file_block(ui, p, file, 0, true, files.len() == 1, session, scope);
        }
    }
    if !toggles.is_empty() {
        let id = Id::unique(("changes-expanded", session.map(str::to_string)));
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
    session: Option<&str>,
    scope: &Scope,
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
            walk(
                ui,
                p,
                &node.children,
                files,
                expanded,
                session,
                scope,
                toggles,
            );
            for index in &node.files {
                if let Some(file) = files.get(*index) {
                    file_block(
                        ui,
                        p,
                        file,
                        depth + 1,
                        true,
                        files.len() == 1,
                        session,
                        scope,
                    );
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
    turn: Option<usize>,
) -> Option<Scope> {
    let (label, count) = match scope {
        Scope::Changed => ("Changed", files),
        Scope::Turn => ("Last turn", turn.unwrap_or(0)),
    };
    let text = format!("{label}: {count}");
    let font = theme::font_semibold(13.5);
    let galley = ui.painter().layout_no_wrap(text, font, p.fg);
    let width = galley.size().x + 16.0 + 16.0 + 8.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
    let hover = crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if hover > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.hover.gamma_multiply(hover),
        );
    }
    ui.painter().galley(
        egui::Pos2::new(rect.left() + 8.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg,
    );
    icons::paint(
        ui,
        Icon::ChevronDown,
        egui::Rect::from_center_size(
            egui::Pos2::new(rect.right() - 15.0, rect.center().y),
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
            ui.set_width(168.0);
            for (candidate, label, count) in [
                (Scope::Changed, "Changed", files),
                (Scope::Turn, "Last turn", turn.unwrap_or(0)),
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
    session: Option<&str>,
    scope: &Scope,
) {
    let open_id = open_id(&session.map(str::to_string), scope, &file.file);
    let key = format!(
        "{}:{}:{}",
        session.unwrap_or("session"),
        scope.index(),
        file.file
    );
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
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(theme::radius::MD),
                theme::scale_alpha(p.hover, 0.5),
            );
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

        let stats_width = 84.0;
        let text_left = x + 58.0;
        let text_right = rect.right() - stats_width - 8.0;
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

        let stats_right = rect.right() - 30.0;
        let added = format!("+{}", file.additions);
        let removed = format!("−{}", file.deletions);
        ui.painter().text(
            egui::Pos2::new(stats_right, rect.center().y),
            Align2::RIGHT_CENTER,
            &removed,
            theme::font(12.0),
            p.red,
        );
        let removed_width = ui
            .painter()
            .layout_no_wrap(removed, theme::font(12.0), p.red)
            .size()
            .x;
        ui.painter().text(
            egui::Pos2::new(stats_right - 2.0 - removed_width, rect.center().y),
            Align2::RIGHT_CENTER,
            &added,
            theme::font(12.0),
            p.green,
        );
        let copy_rect = egui::Rect::from_center_size(
            egui::Pos2::new(rect.right() - 14.0, rect.center().y),
            Vec2::splat(20.0),
        );
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
    patch_view(ui, p, &key, &file.patch, limit);
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

fn patch_view(ui: &mut Ui, p: &Palette, key: &str, patch: &str, limit: usize) {
    let font = theme::font_mono(11.5);
    let probe = ui
        .painter()
        .layout_no_wrap("Ag".to_string(), font.clone(), p.fg);
    let row_height = (probe.size().y + 4.0).max(ROW_HEIGHT);
    let char_width = ui
        .painter()
        .layout_no_wrap("0000000000".to_string(), font.clone(), p.fg)
        .size()
        .x
        / 10.0;
    let lines: Vec<&str> = patch
        .split('\n')
        .take(limit)
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let total = lines.len().max(1);
    let digits = total.to_string().len();
    let gutter = (digits as f32 * char_width + 26.0).max(44.0);
    let columns = patch_columns(ui.ctx(), key, patch);
    let content_width = gutter + columns as f32 * char_width + 24.0;
    let content_height = total as f32 * row_height + 4.0;
    let added_bg = p.over(p.bg, p.wash(p.green, 37));
    let removed_bg = p.over(p.bg, p.wash(p.red, 37));
    let hunk_bg = p.over(p.bg, p.wash(p.blue, 32));

    ScrollArea::both()
        .id_salt(("patch", key))
        .max_height(PATCH_MAX_HEIGHT)
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(content_width, content_height), Sense::hover());
            let painter = ui.painter();
            let first = ((viewport.min.y / row_height).floor()).max(0.0) as usize;
            let last = (((viewport.max.y / row_height).ceil()).max(0.0) as usize + 1).min(total);
            for index in first..last {
                let top = rect.top() + index as f32 * row_height;
                let row = egui::Rect::from_min_size(
                    egui::Pos2::new(rect.left(), top),
                    Vec2::new(content_width, row_height),
                );
                let line = lines.get(index).copied().unwrap_or("");
                let (color, background) = match diff_line_style(line) {
                    DiffLine::Added => (p.green, Some(added_bg)),
                    DiffLine::Removed => (p.red, Some(removed_bg)),
                    DiffLine::Hunk => (p.blue, Some(hunk_bg)),
                    DiffLine::Meta => (p.fg_muted, None),
                    DiffLine::Context => (p.body, None),
                };
                if let Some(background) = background {
                    painter.rect_filled(row, CornerRadius::ZERO, background);
                }
                painter.text(
                    egui::Pos2::new(rect.left() + gutter - 8.0, row.center().y),
                    Align2::RIGHT_CENTER,
                    (index + 1).to_string(),
                    theme::font_mono(11.0),
                    p.fg_muted,
                );
                painter.text(
                    egui::Pos2::new(rect.left() + gutter, row.center().y),
                    Align2::LEFT_CENTER,
                    line,
                    font.clone(),
                    color,
                );
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
        "modified" | "changed" => ("M", p.coral),
        _ => ("M", p.coral),
    }
}

fn file_visual(path: &str, p: &Palette) -> (Icon, Color32) {
    let extension = path
        .rsplit_once('.')
        .map(|(_, value)| value.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "rs" => (Icon::File, p.coral),
        "ts" | "tsx" => (Icon::File, p.blue),
        "js" | "jsx" | "mjs" | "cjs" | "json" | "jsonc" => (Icon::File, p.primary),
        "md" | "mdx" | "txt" => (Icon::File, p.blue),
        "css" | "scss" | "sass" | "less" => (Icon::File, p.purple),
        "html" | "htm" | "xml" | "svg" => (Icon::File, p.coral),
        "sh" | "bash" | "zsh" => (Icon::Terminal, p.green),
        _ => (Icon::File, p.fg_muted),
    }
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
