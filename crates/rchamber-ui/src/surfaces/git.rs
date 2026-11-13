use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, Layout, Margin, Rect, RichText, ScrollArea,
    Sense, Ui, Vec2,
};
use rchamber_app::ToastKind;
use rchamber_app::format::{DiffLine, diff_line_style, relative_time, shorten_path};
use rchamber_git::{Commit, FileDiff, FileStatus, GitError, Status};

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::surfaces::{self, PanelTab};
use crate::theme::{self, Palette};
use crate::widgets;

const FILE_ROW_HEIGHT: f32 = 32.0;
const COMMIT_ROW_HEIGHT: f32 = 52.0;
const PATCH_MAX_HEIGHT: f32 = 320.0;
const LOG_PAGE: usize = 20;

#[derive(Clone, Default)]
struct GitState {
    start: String,
    root: Option<PathBuf>,
    loaded: bool,
    last_seen: f64,
    spin_until: f64,
    status: Status,
    branches: Vec<String>,
    commits: Vec<Commit>,
    diff: Vec<FileDiff>,
    selected: Option<(String, bool)>,
    message: String,
    new_branch: String,
    creating_branch: bool,
    confirm: Option<(String, bool)>,
    expanded_commit: Option<String>,
    error: Option<String>,
    log_limit: usize,
    branch_menu: bool,
}

#[derive(Default)]
struct Actions {
    refresh: bool,
    fetch: bool,
    push: bool,
    pull: bool,
    init: bool,
    commit: bool,
    commit_sync: bool,
    stage: Option<String>,
    unstage: Option<String>,
    stage_all: Vec<String>,
    unstage_all: Vec<String>,
    discard: Option<(String, bool)>,
    checkout: Option<String>,
    create_branch: Option<String>,
    open: Option<(String, bool)>,
}

impl Actions {
    fn any(&self) -> bool {
        self.refresh
            || self.fetch
            || self.push
            || self.pull
            || self.init
            || self.commit
            || self.commit_sync
            || self.stage.is_some()
            || self.unstage.is_some()
            || !self.stage_all.is_empty()
            || !self.unstage_all.is_empty()
            || self.discard.is_some()
            || self.checkout.is_some()
            || self.create_branch.is_some()
            || self.open.is_some()
    }
}

#[derive(Default)]
struct RowResult {
    open: bool,
    toggle_stage: bool,
    discard: bool,
}

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let id = Id::unique("rchamber-git-surface");
    let now = ui.input(|input| input.time);
    let mut state = ui
        .data(|data| data.get_temp::<GitState>(id))
        .unwrap_or_default();
    let start = app.workspace_directory();
    let reopened = state.loaded && now - state.last_seen > 0.9;
    state.last_seen = now;

    if !state.loaded || state.start != start {
        load(&mut state, &start, now);
    } else if reopened {
        refresh(&mut state, now);
    }

    let spinning = now < state.spin_until;
    let mut actions = Actions::default();
    let mut close = false;
    let mut toggle_expanded = false;
    let expanded = app.ui.expanded;
    let tabs = [PanelTab::new("Git", Icon::Branch).active(true)];
    surfaces::panel_header(
        ui,
        &p,
        &tabs,
        |ui| {
            if state.root.is_some() {
                branch_controls(ui, &p, &mut state, &mut actions, spinning);
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
    if close {
        app.ui.context_open = false;
    }
    if toggle_expanded {
        app.ui.expanded = !app.ui.expanded;
    }
    if spinning {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
    if state.root.is_some() {
        state_row(ui, &p, &state);
        if state.creating_branch {
            branch_input_row(ui, &p, &mut state, &mut actions);
        }
    }

    ScrollArea::vertical()
        .id_salt("git-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if state.root.is_none() {
                empty_state(ui, &p, &state, &mut actions);
                return;
            }
            if let Some(error) = state.error.clone() {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 12.0).max(80.0));
                        if surfaces::error_banner(ui, &p, &error, "Retry") {
                            actions.refresh = true;
                        }
                    });
                });
            }
            changes(ui, &p, &mut state, &mut actions);
            commit_box(ui, &p, &mut state, &mut actions);
            history(ui, &p, &mut state, &mut actions);
            ui.add_space(16.0);
        });

    let acted = actions.any();
    apply(&mut state, app, actions, now);
    if acted {
        ui.ctx().request_repaint();
    }
    ui.data_mut(|data| data.insert_temp(id, state));
}

fn load(state: &mut GitState, start: &str, now: f64) {
    state.start = start.to_string();
    state.loaded = true;
    state.error = None;
    state.status = Status::default();
    state.branches.clear();
    state.commits.clear();
    state.diff.clear();
    state.selected = None;
    state.confirm = None;
    state.expanded_commit = None;
    state.log_limit = LOG_PAGE;
    state.root = rchamber_git::discover(Path::new(start));
    if state.root.is_some() {
        refresh(state, now);
    }
}

fn refresh(state: &mut GitState, now: f64) {
    state.spin_until = now + 0.6;
    let Some(root) = state.root.clone() else {
        return;
    };
    match rchamber_git::status(&root) {
        Ok(status) => {
            state.status = status;
            state.error = None;
        }
        Err(error) => state.error = Some(error.to_string()),
    }
    state.branches = rchamber_git::branches(&root).unwrap_or_default();
    state.commits = rchamber_git::log(&root, state.log_limit.max(LOG_PAGE)).unwrap_or_default();
    if let Some((path, staged)) = state.selected.clone() {
        let present = state.status.files.iter().any(|file| {
            file.path == path
                && if staged {
                    file.staged
                } else {
                    file.has_worktree_changes()
                }
        });
        if present {
            state.diff = rchamber_git::diff(&root, Some(&path), staged, 3).unwrap_or_default();
        } else {
            state.selected = None;
            state.diff.clear();
        }
    } else {
        state.diff.clear();
    }
}

fn branch_controls(
    ui: &mut Ui,
    p: &Palette,
    state: &mut GitState,
    actions: &mut Actions,
    _spinning: bool,
) {
    let current = state.status.branch.clone();
    let upstream = state.status.upstream.clone();
    let can_pull = upstream.is_some();
    let can_push = upstream.is_some() && !state.commits.is_empty();
    let mut names = state.branches.clone();
    if let Some(current) = current.as_ref()
        && !names.iter().any(|name| name == current)
    {
        names.push(current.clone());
        names.sort();
    }
    let label = current.clone().unwrap_or_else(|| "detached".to_string());

    let available = ui.available_width();
    let compact = available < 280.0;
    let controls = if compact {
        2.0 * 28.0 + 2.0 * 4.0
    } else {
        3.0 * 28.0 + 2.0 * 22.0 + 5.0 * 4.0
    };
    let label_width = ui
        .painter()
        .layout_no_wrap(label.clone(), theme::font(13.0), p.fg)
        .size()
        .x;
    let width = (label_width + 58.0)
        .clamp(72.0, 168.0)
        .min((available - controls).max(72.0));
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
    let hover = crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if hover > 0.001 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.hover.gamma_multiply(hover),
        );
    }
    icons::paint(
        ui,
        Icon::Branch,
        Rect::from_center_size(
            egui::Pos2::new(rect.left() + 12.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        p.fg_muted,
    );
    let galley = surfaces::truncated_galley(
        ui,
        label,
        theme::font(13.0),
        p.fg,
        (rect.width() - 58.0).max(20.0),
    );
    ui.painter().galley(
        egui::Pos2::new(rect.left() + 24.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg,
    );
    if upstream.is_some() {
        let ahead = state.status.ahead;
        let behind = state.status.behind;
        if ahead > 0 || behind > 0 {
            ui.painter().text(
                egui::Pos2::new(rect.right() - 30.0, rect.center().y),
                Align2::RIGHT_CENTER,
                format!("{ahead}↑ {behind}↓"),
                theme::font(11.0),
                p.fg_muted,
            );
        }
    }
    icons::paint(
        ui,
        Icon::ChevronDown,
        Rect::from_center_size(
            egui::Pos2::new(rect.right() - 11.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        p.fg_muted,
    );
    let mut branch_menu = state.branch_menu;
    let mut close_menu = false;
    egui::Popup::from_response(&response)
        .id(Id::unique("git-branch-menu"))
        .open_bool(&mut branch_menu)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(crate::theme::popover_frame(p))
        .gap(4.0)
        .show(|ui| {
            ui.set_width(220.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            widgets::section_label(ui, "Branches", p);
            ui.add_space(2.0);
            for name in &names {
                let selected = current.as_deref() == Some(name.as_str());
                let response = widgets::menu_item(ui, name, Some(Icon::Branch), None, p);
                if response.clicked() && !selected {
                    actions.checkout = Some(name.clone());
                    close_menu = true;
                }
                if selected {
                    icons::paint(
                        ui,
                        Icon::Check,
                        Rect::from_center_size(
                            egui::Pos2::new(response.rect.right() - 14.0, response.rect.center().y),
                            Vec2::splat(12.0),
                        ),
                        p.primary,
                    );
                }
            }
            if names.is_empty() {
                ui.label(
                    RichText::new("No branches")
                        .font(theme::font(12.5))
                        .color(p.fg_muted),
                );
            }
        });
    state.branch_menu = branch_menu && !close_menu;

    if !compact && surfaces::mini_icon_button(ui, p, Icon::Plus, "New branch").clicked() {
        state.creating_branch = !state.creating_branch;
        if !state.creating_branch {
            state.new_branch.clear();
        }
    }
    if !compact && surfaces::mini_icon_button(ui, p, Icon::Refresh, "Fetch all remotes").clicked() {
        actions.fetch = true;
    }
    if surfaces::panel_action(ui, p, Icon::ArrowDown, can_pull, "Pull from upstream").clicked() {
        actions.pull = true;
    }
    if surfaces::panel_action(ui, p, Icon::ArrowUp, can_push, "Push to upstream").clicked() {
        actions.push = true;
    }
}

fn state_row(ui: &mut Ui, p: &Palette, state: &GitState) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
    let repo = state
        .root
        .as_ref()
        .and_then(|root| root.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string());
    icons::paint(
        ui,
        Icon::Folder,
        Rect::from_center_size(
            egui::Pos2::new(rect.left() + 18.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        p.fg_muted,
    );
    let galley = surfaces::truncated_galley(
        ui,
        repo,
        theme::font(12.5),
        p.fg_muted,
        (rect.width() * 0.4).max(40.0),
    );
    let repo_width = galley.size().x;
    ui.painter().galley(
        egui::Pos2::new(rect.left() + 30.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg_muted,
    );
    if let Some(upstream) = state.status.upstream.as_deref() {
        let ahead = state.status.ahead;
        let behind = state.status.behind;
        let label = if ahead > 0 || behind > 0 {
            format!("{upstream} · ↑{ahead} ↓{behind}")
        } else {
            format!("{upstream} · synced")
        };
        let galley = surfaces::truncated_galley(
            ui,
            label,
            theme::font(12.0),
            p.fg_muted,
            (rect.width() * 0.42).max(40.0),
        );
        ui.painter().galley(
            egui::Pos2::new(
                rect.left() + 30.0 + repo_width + 12.0,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            p.fg_muted,
        );
    }
    let additions: u32 = state.status.files.iter().map(|file| file.additions).sum();
    let deletions: u32 = state.status.files.iter().map(|file| file.deletions).sum();
    ui.painter().text(
        egui::Pos2::new(rect.right() - 12.0, rect.center().y),
        Align2::RIGHT_CENTER,
        format!("−{deletions}"),
        theme::font(12.0),
        p.red,
    );
    let removed_width = ui
        .painter()
        .layout_no_wrap(format!("−{deletions}"), theme::font(12.0), p.red)
        .size()
        .x;
    ui.painter().text(
        egui::Pos2::new(rect.right() - 18.0 - removed_width, rect.center().y),
        Align2::RIGHT_CENTER,
        format!("+{additions}"),
        theme::font(12.0),
        p.green,
    );
    surfaces::hairline(ui, p);
}

fn branch_input_row(ui: &mut Ui, p: &Palette, state: &mut GitState, actions: &mut Actions) {
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let start = ui.cursor().min;
        let field_width = (ui.available_width() - 160.0).max(100.0);
        let mut field_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(start, Vec2::new(field_width, 28.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        let response = field_ui.add(
            egui::TextEdit::singleline(&mut state.new_branch)
                .id(Id::unique("git-new-branch"))
                .hint_text(
                    RichText::new("new-branch-name")
                        .font(theme::font(13.0))
                        .color(p.fg_muted),
                )
                .text_color(p.fg)
                .font(theme::font(13.0))
                .desired_width(f32::INFINITY)
                .margin(Margin::symmetric(8, 5)),
        );
        if !response.has_focus() {
            response.request_focus();
        }
        let submit = response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        ui.add_space(field_width + 6.0);
        ui.add_space(6.0);
        let enabled = !state.new_branch.trim().is_empty();
        if widgets::button(
            ui,
            "Create",
            Some(Icon::Plus),
            widgets::ButtonVariant::Outline,
            widgets::ButtonSize::Sm,
            p,
        )
        .clicked()
            && enabled
            || submit && enabled
        {
            actions.create_branch = Some(state.new_branch.clone());
        }
        if widgets::ghost_button(ui, "Cancel", None, p).clicked() {
            state.creating_branch = false;
            state.new_branch.clear();
        }
    });
    ui.add_space(4.0);
}

fn changes(ui: &mut Ui, p: &Palette, state: &mut GitState, actions: &mut Actions) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        surfaces::section_label(ui, "Changes", p);
        ui.label(
            RichText::new(state.status.files.len().to_string())
                .font(theme::font(12.5))
                .color(p.fg_muted),
        );
    });
    ui.add_space(6.0);

    let staged: Vec<FileStatus> = state.status.staged().cloned().collect();
    let unstaged: Vec<FileStatus> = state.status.unstaged().cloned().collect();

    if staged.is_empty() && unstaged.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 12.0).max(80.0));
                egui::Frame::new()
                    .fill(p.status_bg(p.green))
                    .corner_radius(theme::radius::XL)
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            icons::icon(ui, Icon::Check, 14.0, p.status_text(p.green));
                            ui.label(
                                RichText::new("Working tree clean")
                                    .font(theme::font(13.0))
                                    .color(p.status_text(p.green)),
                            );
                        });
                    });
            });
        });
        return;
    }

    if !staged.is_empty() {
        group(ui, p, "Staged", staged.len(), false, true, actions, &staged);
        for file in &staged {
            file_entry(ui, p, state, actions, file, true);
        }
    }
    if !unstaged.is_empty() {
        if !staged.is_empty() {
            ui.add_space(4.0);
            surfaces::hairline(ui, p);
        }
        group(
            ui,
            p,
            "Unstaged",
            unstaged.len(),
            true,
            false,
            actions,
            &unstaged,
        );
        for file in &unstaged {
            file_entry(ui, p, state, actions, file, false);
        }
    }
}

fn group(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    count: usize,
    can_stage: bool,
    can_unstage: bool,
    actions: &mut Actions,
    files: &[FileStatus],
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
    let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
    let bulk = Rect::from_center_size(
        egui::Pos2::new(rect.left() + 22.0, rect.center().y),
        Vec2::splat(20.0),
    );
    let bulk_response = ui.interact(bulk, Id::unique(("git-bulk", label)), Sense::click());
    let bulk_clicked = bulk_response.clicked();
    if bulk_response.hovered() {
        ui.painter()
            .rect_filled(bulk, CornerRadius::same(theme::radius::XS), p.hover);
    }
    ui.painter().text(
        bulk.center(),
        Align2::CENTER_CENTER,
        if can_stage { "+" } else { "−" },
        theme::font_semibold(13.0),
        if bulk_response.hovered() {
            p.fg
        } else {
            p.fg_muted
        },
    );
    let _ = bulk_response.on_hover_text(if can_stage {
        "Stage all"
    } else {
        "Unstage all"
    });
    if bulk_clicked {
        if can_stage {
            actions.stage_all = paths;
        } else if can_unstage {
            actions.unstage_all = paths;
        }
    }
    ui.painter().text(
        egui::Pos2::new(rect.left() + 40.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::font_semibold(14.0),
        p.fg,
    );
    let label_width = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::font_semibold(14.0), p.fg)
        .size()
        .x;
    let count_left = rect.left() + 40.0 + label_width + 8.0;
    let count_width = ui
        .painter()
        .layout_no_wrap(count.to_string(), theme::font(12.5), p.fg_muted)
        .size()
        .x;
    ui.painter().text(
        egui::Pos2::new(count_left, rect.center().y),
        Align2::LEFT_CENTER,
        count.to_string(),
        theme::font(12.5),
        p.fg_muted,
    );
    icons::paint(
        ui,
        Icon::ChevronDown,
        Rect::from_center_size(
            egui::Pos2::new(count_left + count_width + 10.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        p.fg_muted,
    );
}

fn file_entry(
    ui: &mut Ui,
    p: &Palette,
    state: &mut GitState,
    actions: &mut Actions,
    file: &FileStatus,
    staged: bool,
) {
    let selected = state
        .selected
        .as_ref()
        .is_some_and(|(path, value)| path == &file.path && *value == staged);
    let result = file_row(ui, p, file, staged, selected);
    if result.open {
        actions.open = Some((file.path.clone(), staged));
    }
    if result.toggle_stage {
        if staged {
            actions.unstage = Some(file.path.clone());
        } else {
            actions.stage = Some(file.path.clone());
        }
    }
    if result.discard {
        state.confirm = Some((file.path.clone(), staged));
    }
    if state
        .confirm
        .as_ref()
        .is_some_and(|(path, value)| path == &file.path && *value == staged)
    {
        confirm_row(ui, p, &file.path, staged, state, actions);
    }
    if selected {
        if state.diff.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label(
                    RichText::new("No diff available (untracked or binary file)")
                        .font(theme::font(12.0))
                        .color(p.fg_muted),
                );
            });
            ui.add_space(4.0);
        } else {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 24.0).max(120.0));
                    patch_view(ui, p, &state.diff);
                });
            });
            ui.add_space(6.0);
        }
    }
}

fn file_row(
    ui: &mut Ui,
    p: &Palette,
    file: &FileStatus,
    staged: bool,
    selected: bool,
) -> RowResult {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), FILE_ROW_HEIGHT),
        Sense::click(),
    );
    let hover = crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, theme::scale_alpha(p.selection, 0.5));
    } else if hover > 0.001 {
        ui.painter()
            .rect_filled(rect, 0.0, theme::scale_alpha(p.hover, 0.5 * hover));
    }

    let letter = file.letter(staged);
    let color = status_color(p, letter);
    ui.painter().text(
        egui::Pos2::new(rect.left() + 22.0, rect.center().y),
        Align2::CENTER_CENTER,
        letter.to_uppercase(),
        theme::font_semibold(12.0),
        color,
    );
    let name = file
        .path
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(&file.path);
    let dir = file
        .path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{dir}/"))
        .unwrap_or_default();
    let (icon, tint) = file_visual(&file.path, p);
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(
            egui::Pos2::new(rect.left() + 42.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        tint,
    );
    let text_left = rect.left() + 54.0;
    let end_lane = 92.0;
    let dir_galley = surfaces::truncated_galley(
        ui,
        dir,
        theme::font(13.0),
        p.fg_muted,
        ((rect.right() - text_left - end_lane) * 0.52).max(16.0),
    );
    let dir_width = dir_galley.size().x;
    ui.painter().galley(
        egui::Pos2::new(text_left, rect.center().y - dir_galley.size().y * 0.5),
        dir_galley,
        p.fg_muted,
    );
    let name_galley = surfaces::truncated_galley(
        ui,
        name.to_string(),
        theme::font(13.0),
        p.fg,
        (rect.right() - text_left - end_lane - dir_width).max(20.0),
    );
    ui.painter().galley(
        egui::Pos2::new(
            text_left + dir_width,
            rect.center().y - name_galley.size().y * 0.5,
        ),
        name_galley,
        p.fg,
    );

    let actions_lane = Rect::from_min_max(
        egui::Pos2::new(rect.right() - end_lane, rect.top()),
        egui::Pos2::new(rect.right(), rect.bottom()),
    );
    let actions_visible = hover > 0.5;
    if !actions_visible {
        let added = format!("+{}", file.additions);
        let removed = format!("−{}", file.deletions);
        ui.painter().text(
            egui::Pos2::new(rect.right() - 12.0, rect.center().y),
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
            egui::Pos2::new(rect.right() - 14.0 - removed_width, rect.center().y),
            Align2::RIGHT_CENTER,
            &added,
            theme::font(12.0),
            p.green,
        );
    }

    let mut result = RowResult::default();
    if actions_visible {
        ui.painter()
            .rect_filled(actions_lane, 0.0, p.over(p.bg, p.wash(p.fg, 10)));
        let stage_rect = Rect::from_center_size(
            egui::Pos2::new(rect.right() - 46.0, rect.center().y),
            Vec2::splat(20.0),
        );
        let stage_response = ui.interact(
            stage_rect,
            Id::unique(("git-stage", file.path.as_str(), staged)),
            Sense::click(),
        );
        if stage_response.hovered() {
            ui.painter()
                .rect_filled(stage_rect, CornerRadius::same(theme::radius::XS), p.hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
        ui.painter().text(
            stage_rect.center(),
            Align2::CENTER_CENTER,
            if staged { "−" } else { "+" },
            theme::font_semibold(13.0),
            if stage_response.hovered() {
                p.fg
            } else {
                p.fg_muted
            },
        );
        let stage_response = stage_response.on_hover_text(if staged { "Unstage" } else { "Stage" });

        let discard_rect = Rect::from_center_size(
            egui::Pos2::new(rect.right() - 20.0, rect.center().y),
            Vec2::splat(20.0),
        );
        let discard_response = ui.interact(
            discard_rect,
            Id::unique(("git-discard", file.path.as_str(), staged)),
            Sense::click(),
        );
        if discard_response.hovered() {
            ui.painter().rect_filled(
                discard_rect,
                CornerRadius::same(theme::radius::XS),
                p.status_bg(p.red),
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
        icons::paint(
            ui,
            Icon::Trash,
            Rect::from_center_size(discard_rect.center(), Vec2::splat(12.0)),
            if discard_response.hovered() {
                p.red
            } else {
                p.fg_muted
            },
        );
        let discard_response = discard_response.on_hover_text("Discard changes");

        if stage_response.clicked() {
            result.toggle_stage = true;
        }
        if discard_response.clicked() {
            result.discard = true;
        }
        if result.toggle_stage || result.discard {
            return result;
        }
    }

    if response.clicked() {
        result.open = true;
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
    }
    let _ = response.on_hover_text(file.path.clone());
    result
}

fn confirm_row(
    ui: &mut Ui,
    p: &Palette,
    path: &str,
    staged: bool,
    state: &mut GitState,
    actions: &mut Actions,
) {
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        let width = (ui.available_width() - 32.0).max(120.0);
        egui::Frame::new()
            .fill(p.status_bg(p.red))
            .corner_radius(theme::radius::MD)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    icons::icon(ui, Icon::Warning, 13.0, p.status_text(p.red));
                    ui.label(
                        RichText::new(format!("Discard changes to {}?", shorten_path(path, 32)))
                            .font(theme::font(12.5))
                            .color(p.status_text(p.red)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::ghost_button(ui, "Cancel", None, p).clicked() {
                            state.confirm = None;
                        }
                        if widgets::button(
                            ui,
                            "Discard",
                            Some(Icon::Trash),
                            widgets::ButtonVariant::Destructive,
                            widgets::ButtonSize::Sm,
                            p,
                        )
                        .clicked()
                        {
                            actions.discard = Some((path.to_string(), staged));
                            state.confirm = None;
                        }
                    });
                });
            });
    });
    ui.add_space(2.0);
}

fn commit_box(ui: &mut Ui, p: &Palette, state: &mut GitState, actions: &mut Actions) {
    let staged = state.status.staged_count();
    let has_staged = staged > 0;
    ui.add_space(12.0);
    let body = ui.allocate_exact_size(Vec2::new(ui.available_width(), 190.0), Sense::hover());
    let (rect, _) = body;
    let mut body_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                egui::Pos2::new(rect.left() + 12.0, rect.top()),
                egui::Pos2::new(rect.right() - 12.0, rect.top() + 220.0),
            ))
            .layout(Layout::top_down(Align::Min)),
    );
    body_ui.horizontal(|ui| {
        surfaces::section_label(ui, "Commit", p);
        if has_staged {
            ui.label(
                RichText::new(format!("{staged} staged"))
                    .font(theme::font(12.5))
                    .color(p.fg_muted),
            );
        } else {
            ui.label(
                RichText::new("Stage files to enable commit.")
                    .font(theme::font(12.5))
                    .color(p.fg_muted),
            );
        }
    });
    body_ui.add_space(6.0);
    let edit = body_ui.add(
        egui::TextEdit::multiline(&mut state.message)
            .id(Id::unique("git-commit-message"))
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text(
                RichText::new("Commit message")
                    .font(theme::font(13.0))
                    .color(p.fg_muted),
            )
            .text_color(p.fg)
            .font(theme::font(13.0))
            .margin(Margin::symmetric(10, 8)),
    );
    let submit = edit.has_focus()
        && ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
    if submit && has_staged && !state.message.trim().is_empty() {
        actions.commit = true;
    }
    body_ui.add_space(8.0);
    body_ui.horizontal(|ui| {
        if widgets::button(
            ui,
            "Generate",
            Some(Icon::Spark),
            widgets::ButtonVariant::Outline,
            widgets::ButtonSize::Sm,
            p,
        )
        .on_hover_text("Draft a message from the staged changes")
        .clicked()
            && has_staged
        {
            state.message = generated_message(state);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let enabled = has_staged && !state.message.trim().is_empty();
            if widgets::button(
                ui,
                "Commit & sync",
                Some(Icon::ArrowUp),
                widgets::ButtonVariant::Primary,
                widgets::ButtonSize::Sm,
                p,
            )
            .on_hover_text("Commit, then push to the upstream branch")
            .clicked()
                && enabled
            {
                actions.commit_sync = true;
            }
            if widgets::button(
                ui,
                "Commit",
                Some(Icon::Check),
                widgets::ButtonVariant::Outline,
                widgets::ButtonSize::Sm,
                p,
            )
            .clicked()
                && enabled
            {
                actions.commit = true;
            }
        });
    });
    body_ui.add_space(4.0);
}

fn generated_message(state: &GitState) -> String {
    let staged: Vec<&FileStatus> = state.status.staged().collect();
    let mut names: Vec<&str> = staged
        .iter()
        .map(|file| {
            file.path
                .rsplit_once('/')
                .map(|(_, name)| name)
                .unwrap_or(&file.path)
        })
        .collect();
    names.truncate(4);
    if names.is_empty() {
        return String::new();
    }
    format!("Update {}", names.join(", "))
}

fn history(ui: &mut Ui, p: &Palette, state: &mut GitState, actions: &mut Actions) {
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        surfaces::section_label(ui, "History", p);
        ui.label(
            RichText::new(format!("{} commits", state.commits.len()))
                .font(theme::font(12.5))
                .color(p.fg_muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            if state.commits.len() >= state.log_limit
                && widgets::ghost_button(ui, "Load more", None, p).clicked()
            {
                state.log_limit += LOG_PAGE;
                actions.refresh = true;
            }
        });
    });
    ui.add_space(4.0);
    if state.commits.is_empty() {
        surfaces::inline_empty(ui, p, "No commits yet");
        return;
    }
    let commits = state.commits.clone();
    for commit in &commits {
        let expanded = state.expanded_commit.as_deref() == Some(commit.hash.as_str());
        let clicked = commit_row(ui, p, commit, expanded);
        if clicked {
            state.expanded_commit = if expanded {
                None
            } else {
                Some(commit.hash.clone())
            };
        }
        if state.expanded_commit.as_deref() == Some(commit.hash.as_str()) {
            commit_detail(ui, p, commit);
        }
        surfaces::divider(ui, p);
    }
}

fn commit_row(ui: &mut Ui, p: &Palette, commit: &Commit, expanded: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), COMMIT_ROW_HEIGHT),
        Sense::click(),
    );
    let hover = crate::motion::hover_fade(ui, response.id.with("hover"), response.hovered());
    if expanded {
        ui.painter()
            .rect_filled(rect, 0.0, theme::scale_alpha(p.selection, 0.5));
    } else if hover > 0.001 {
        ui.painter()
            .rect_filled(rect, 0.0, theme::scale_alpha(p.hover, 0.55 * hover));
    }
    ui.painter().circle_filled(
        egui::Pos2::new(rect.left() + 14.0, rect.center().y),
        3.5,
        p.green,
    );
    let short: String = commit.hash.chars().take(8).collect();
    let subject_color = if response.hovered() || expanded {
        p.fg
    } else {
        p.body
    };
    let subject = surfaces::truncated_galley(
        ui,
        commit.subject.clone(),
        theme::font_medium(13.5),
        subject_color,
        (rect.right() - rect.left() - 44.0).max(40.0),
    );
    ui.painter().galley(
        egui::Pos2::new(rect.left() + 26.0, rect.top() + 9.0),
        subject,
        subject_color,
    );
    let date = relative_time((commit.time as f64) * 1000.0);
    let meta = if date.is_empty() {
        format!("{} · {}", commit.author, short)
    } else {
        format!("{} · {} · {}", commit.author, date, short)
    };
    let meta = surfaces::truncated_galley(
        ui,
        meta,
        theme::font(12.5),
        p.fg_muted,
        (rect.right() - rect.left() - 44.0).max(40.0),
    );
    ui.painter().galley(
        egui::Pos2::new(rect.left() + 26.0, rect.top() + 28.0),
        meta,
        p.fg_muted,
    );
    let mut copy_clicked = false;
    if hover > 0.5 {
        let copy = Rect::from_center_size(
            egui::Pos2::new(rect.right() - 18.0, rect.center().y),
            Vec2::splat(22.0),
        );
        let copy_response = ui.interact(
            copy,
            Id::unique(("git-copy-sha", commit.hash.as_str())),
            Sense::click(),
        );
        if copy_response.hovered() {
            ui.painter()
                .rect_filled(copy, CornerRadius::same(theme::radius::XS), p.hover);
        }
        icons::paint(
            ui,
            Icon::Copy,
            Rect::from_center_size(copy.center(), Vec2::splat(12.0)),
            if copy_response.hovered() {
                p.fg
            } else {
                p.fg_muted
            },
        );
        if copy_response.on_hover_text("Copy SHA").clicked() {
            copy_clicked = true;
            ui.ctx().copy_text(commit.hash.clone());
        }
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
    }
    response.clicked() && !copy_clicked
}

fn commit_detail(ui: &mut Ui, p: &Palette, commit: &Commit) {
    ui.horizontal(|ui| {
        ui.add_space(26.0);
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 38.0).max(120.0));
            egui::Frame::new()
                .fill(p.code_bg)
                .corner_radius(theme::radius::MD)
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(&commit.subject)
                            .font(theme::font_medium(13.0))
                            .color(p.fg),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("{} · {}", commit.author, commit_time(commit.time)))
                            .font(theme::font(12.0))
                            .color(p.fg_muted),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(&commit.hash)
                            .font(theme::font_mono(12.0))
                            .color(p.fg_muted),
                    );
                });
        });
    });
    ui.add_space(6.0);
}

fn patch_view(ui: &mut Ui, p: &Palette, files: &[FileDiff]) {
    let added_bg = p.over(p.bg, p.wash(p.green, 37));
    let removed_bg = p.over(p.bg, p.wash(p.red, 37));
    let hunk_bg = p.over(p.bg, p.wash(p.blue, 32));
    ScrollArea::both()
        .id_salt("git-patch")
        .max_height(PATCH_MAX_HEIGHT)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut total = 0usize;
            for file in files {
                for line in file.patch.lines() {
                    if total >= 800 {
                        ui.label(
                            RichText::new("Diff truncated — open the file for the rest.")
                                .font(theme::font(12.0))
                                .color(p.fg_muted),
                        );
                        return;
                    }
                    total += 1;
                    let font = theme::font_mono(11.5);
                    let galley =
                        ui.painter()
                            .layout_no_wrap(line.to_string(), font.clone(), p.body);
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(
                            galley.size().x.max(10.0),
                            20.0_f32.max(galley.size().y + 3.0),
                        ),
                        Sense::hover(),
                    );
                    let (color, background) = match diff_line_style(line) {
                        DiffLine::Added => (p.green, Some(added_bg)),
                        DiffLine::Removed => (p.red, Some(removed_bg)),
                        DiffLine::Hunk => (p.blue, Some(hunk_bg)),
                        DiffLine::Meta => (p.fg_muted, None),
                        DiffLine::Context => (p.body, None),
                    };
                    if let Some(background) = background {
                        ui.painter().rect_filled(
                            rect.expand2(Vec2::new(8.0, 0.0)),
                            0.0,
                            background,
                        );
                    }
                    ui.painter().galley(rect.min, galley, color);
                }
            }
        });
}

fn empty_state(ui: &mut Ui, p: &Palette, state: &GitState, actions: &mut Actions) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        icons::icon(ui, Icon::Branch, 28.0, p.fg_muted);
        ui.add_space(12.0);
        ui.label(
            RichText::new("Not a git repository")
                .font(theme::font_semibold(14.0))
                .color(p.fg),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(shorten_path(&state.start, 52))
                .font(theme::font_mono(12.0))
                .color(p.fg_muted),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new("Track changes, stage files and commit without leaving RustChamber.")
                .font(theme::font(12.5))
                .color(p.fg_muted),
        );
        ui.add_space(14.0);
        if widgets::primary_button(ui, "Initialize repository", Some(Icon::Plus), p).clicked() {
            actions.init = true;
        }
    });
}

fn apply(state: &mut GitState, app: &mut RustChamber, actions: Actions, now: f64) {
    if actions.refresh {
        refresh(state, now);
    }
    if actions.init {
        let start = state.start.clone();
        match rchamber_git::init(Path::new(&start)) {
            Ok(()) => {
                app.model
                    .toast(ToastKind::Success, "Initialized git repository");
                load(state, &start, now);
            }
            Err(error) => {
                app.model.toast(ToastKind::Error, format!("Git: {error}"));
            }
        }
        return;
    }

    let Some(root) = state.root.clone() else {
        return;
    };

    if let Some((path, staged)) = actions.open.as_ref() {
        if state.selected.as_ref() == Some(&(path.clone(), *staged)) {
            state.selected = None;
            state.diff.clear();
        } else {
            match rchamber_git::diff(&root, Some(path), *staged, 3) {
                Ok(files) => {
                    state.selected = Some((path.clone(), *staged));
                    state.diff = files;
                }
                Err(error) => {
                    app.model.toast(ToastKind::Error, format!("Git: {error}"));
                }
            }
        }
    }

    for path in &actions.stage_all {
        if let Err(error) = rchamber_git::stage(&root, path) {
            app.model.toast(ToastKind::Error, format!("Git: {error}"));
            break;
        }
    }
    for path in &actions.unstage_all {
        if let Err(error) = rchamber_git::unstage(&root, path) {
            app.model.toast(ToastKind::Error, format!("Git: {error}"));
            break;
        }
    }

    if weights_changed(&actions) {
        refresh(state, now);
    }

    if let Some(path) = actions.stage.as_ref() {
        report(
            state,
            app,
            rchamber_git::stage(&root, path),
            "Staged file",
            now,
        );
    }
    if let Some(path) = actions.unstage.as_ref() {
        report(
            state,
            app,
            rchamber_git::unstage(&root, path),
            "Unstaged file",
            now,
        );
    }
    if let Some((path, staged)) = actions.discard.as_ref() {
        report(
            state,
            app,
            rchamber_git::discard(&root, path, *staged),
            "Discarded changes",
            now,
        );
    }
    if let Some(branch) = actions.checkout.as_ref() {
        report(
            state,
            app,
            rchamber_git::checkout(&root, branch),
            &format!("Checked out {branch}"),
            now,
        );
    }
    if let Some(name) = actions.create_branch.as_ref() {
        match rchamber_git::create_branch(&root, name) {
            Ok(()) => {
                state.new_branch.clear();
                state.creating_branch = false;
                let checkout = rchamber_git::checkout(&root, name);
                match checkout {
                    Ok(()) => app
                        .model
                        .toast(ToastKind::Success, format!("Created branch {name}")),
                    Err(error) => app.model.toast(ToastKind::Error, format!("Git: {error}")),
                }
                refresh(state, now);
            }
            Err(error) => app.model.toast(ToastKind::Error, format!("Git: {error}")),
        }
    }
    if actions.fetch {
        report(
            state,
            app,
            rchamber_git::fetch(&root),
            "Fetched from remote",
            now,
        );
    }
    if actions.pull {
        report(
            state,
            app,
            rchamber_git::pull(&root),
            "Pulled from upstream",
            now,
        );
    }
    if actions.push {
        report(
            state,
            app,
            rchamber_git::push(&root),
            "Pushed to upstream",
            now,
        );
    }
    if actions.commit || actions.commit_sync {
        match rchamber_git::commit(&root, state.message.trim()) {
            Ok(()) => {
                state.message.clear();
                state.selected = None;
                state.diff.clear();
                app.model.toast(ToastKind::Success, "Committed changes");
                refresh(state, now);
                if actions.commit_sync {
                    report(
                        state,
                        app,
                        rchamber_git::push(&root),
                        "Pushed to upstream",
                        now,
                    );
                }
            }
            Err(error) => app.model.toast(ToastKind::Error, format!("Git: {error}")),
        }
    }
}

fn weights_changed(actions: &Actions) -> bool {
    !actions.stage_all.is_empty() || !actions.unstage_all.is_empty()
}

fn report(
    state: &mut GitState,
    app: &mut RustChamber,
    result: Result<(), GitError>,
    success: &str,
    now: f64,
) {
    match result {
        Ok(()) => app.model.toast(ToastKind::Success, success),
        Err(error) => app.model.toast(ToastKind::Error, format!("Git: {error}")),
    }
    refresh(state, now);
}

fn status_color(p: &Palette, letter: char) -> Color32 {
    match letter {
        'A' => p.green,
        'M' => p.diff_modified,
        'D' => p.red,
        'R' | 'C' => p.blue,
        'U' => p.green,
        'T' => p.info_text,
        _ => p.fg_muted,
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

fn commit_time(seconds: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(seconds, 0)
        .single()
        .map(|time| time.format("%b %-d, %Y %H:%M").to_string())
        .unwrap_or_default()
}
