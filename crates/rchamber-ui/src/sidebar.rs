use std::collections::{HashMap, HashSet};
use std::time::Duration;


use eframe::egui::{
    self, Align, CornerRadius, Layout, Rect, RichText, ScrollArea, Sense, Stroke, TextStyle, Ui,
    Vec2, WidgetText,
};
use rchamber_api::models::{Message, Outcome, Part, Session};
use rchamber_app::format::{compact_number, money, relative_time};
use rchamber_app::workspace::{SessionFolder, paths_match};
use rchamber_app::{Action, Connection, ProjectEntry, ProjectSort, SessionGrouping, ToastKind};

use crate::app::{RustChamber, hover_amount, menu_nav_focus, menu_popup, motion_time};
use crate::chrome;
use crate::icons::{self, Icon};
use crate::motion;
use crate::theme::{self, Palette};

const HEADER_BAND: f32 = 40.0;
const HEADER_PAD_X: f32 = 12.0;
const SEARCH_BAND: f32 = 38.0;
const SEARCH_HEIGHT: f32 = 26.0;
const SEARCH_PAD_Y: f32 = 6.0;
const ACTION: f32 = 24.0;
const ACTION_GLYPH: f32 = 16.0;
const GRID_INSET: f32 = 12.0;
const GRID_PAD: f32 = GRID_INSET - (ACTION - ACTION_GLYPH) * 0.5;
const ROW_HEIGHT: f32 = 28.0;
const ROW_HEIGHT_DETAILED: f32 = 52.0;
const CHILD_ROW_HEIGHT: f32 = 28.0;
const ROW_PAD_START: f32 = 6.0;
const ROW_PAD_END: f32 = 6.0;
const ROW_TEXT_INSET: f32 = 24.0;
const ROW_END_INSET: f32 = 6.0;
const ROW_STATE_CENTER: f32 = 12.0;
const ROW_PAD_Y_DETAILED: f32 = 6.0;
const TITLE_SIZE: f32 = 13.5;
const CHILD_TITLE_SIZE: f32 = 12.5;
const META_SIZE: f32 = 12.0;
const META_MONO: f32 = 11.5;
const FOOTER_HEIGHT: f32 = 34.0;
const SECTION_HEIGHT: f32 = 28.0;
const SECTION_LABEL_INSET: f32 = GRID_INSET;
const SECTION_GAP: f32 = 10.0;
const SECTION_CHEVRON: f32 = 14.0;
const SECTION_ACTION: f32 = 24.0;
const SECTION_ACTION_GLYPH: f32 = 16.0;
const CAPTION_HEIGHT: f32 = 24.0;
const PROJECT_SLOT: f32 = 32.0;
const PROJECT_GLYPH: f32 = 14.0;
const FOLDER_SLOT: f32 = 28.0;
const FOLDER_GLYPH: f32 = 14.0;
const FOLDER_INDENT: f32 = 14.0;
const DISCLOSURE_SECONDS: f32 = 0.2;
const EMPTY_BLOCK: f32 = 80.0;
const EMPTY_ROW_HEIGHT: f32 = 28.0;
const MENU_TRIGGER: f32 = 20.0;
const MENU_TRIGGER_GLYPH: f32 = 16.0;
const MENU_TRIGGER_INSET: f32 = 4.0;
const MARK: f32 = 12.0;
const TREE_INDENT: f32 = 14.0;
const SHOW_MORE_HEIGHT: f32 = 28.0;
const BOTTOM_GAP: f32 = 8.0;
const MAX_VISIBLE_ROOTS: usize = 7;
const BULK_HEIGHT: f32 = 38.0;
const PROJECT_HASH_TONES: usize = 3;

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let side = ui.max_rect();
    ui.spacing_mut().item_spacing = Vec2::ZERO;

    chrome::sidebar_strip(ui);
    header(ui, app, &p);
    search_row(ui, app, &p);

    let ctx = ui.ctx().clone();
    let quiz = app.model.search.trim().to_string();
    let all: Vec<Session> = app.model.sorted_sessions().into_iter().cloned().collect();
    let sessions: Vec<Session> = all
        .iter()
        .filter(|session| session.time.archived.is_none())
        .cloned()
        .collect();
    if sessions.iter().any(|session| running(app, session)) {
        ctx.request_repaint_after(if motion::reduced_motion(&ctx) {
            Duration::from_secs(1)
        } else {
            Duration::from_millis(40)
        });
    }
    let projects: Vec<ProjectEntry> = app
        .model
        .workspace_projects()
        .into_iter()
        .cloned()
        .collect();
    let project_of: HashMap<String, String> = all
        .iter()
        .filter_map(|session| {
            app.model
                .project_for_session(session)
                .map(|project| (session.id.clone(), project.id.clone()))
        })
        .collect();

    let display = app.model.workspace_store.display.clone();
    let hidden = |key: &str| {
        app.model
            .workspace_store
            .hidden_sections
            .iter()
            .any(|section| section == key)
    };
    let show_in_progress = display.show_in_progress && !hidden("in_progress");
    let show_chats = display.show_chats && !hidden("chats");
    let roots: Vec<Session> = sessions
        .iter()
        .filter(|session| session.parent_id.is_none())
        .cloned()
        .collect();

    let in_progress: Vec<Session> = if show_in_progress {
        roots
            .iter()
            .filter(|session| app.model.is_work(session) || busy_any(app, &sessions, &session.id))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let in_progress_ids: HashSet<String> = in_progress
        .iter()
        .map(|session| session.id.clone())
        .collect();
    let chats: Vec<Session> = if show_chats {
        roots
            .iter()
            .filter(|session| {
                !project_of.contains_key(&session.id) && !in_progress_ids.contains(&session.id)
            })
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let folders = app.model.workspace_store.folders.clone();
    let folder_sessions = app.model.workspace_store.folder_sessions.clone();
    let chats_layout =
        partition_folders(&folders, &folder_sessions, crate::app::CHATS_TARGET, &chats);

    let stored_active = app
        .model
        .workspace_store
        .active_project_id
        .clone()
        .filter(|id| projects.iter().any(|project| &project.id == id));
    let workspace_project = app.model.workspace.as_deref().and_then(|directory| {
        projects
            .iter()
            .find(|project| paths_match(&project.canonical, directory))
            .map(|project| project.id.clone())
    });
    let active_project_id = stored_active.or(workspace_project);
    let active_project = active_project_id
        .as_ref()
        .and_then(|id| projects.iter().find(|project| &project.id == id))
        .cloned()
        .or_else(|| projects.first().cloned());

    let projects_with_sessions: HashSet<String> = sessions
        .iter()
        .filter_map(|session| project_of.get(&session.id).cloned())
        .collect();
    let projects_with_folders: HashSet<String> = folders
        .iter()
        .map(|folder| folder.project_id.clone())
        .collect();
    let visible_projects: Vec<ProjectEntry> = if display.show_projects {
        projects
            .iter()
            .filter(|project| {
                projects_with_sessions.contains(&project.id)
                    || projects_with_folders.contains(&project.id)
                    || active_project_id.as_deref() == Some(project.id.as_str())
                    || project.id.starts_with("dir:")
            })
            .cloned()
            .collect()
    } else {
        active_project.clone().into_iter().collect()
    };
    let project_lists: Vec<(ProjectEntry, FolderPartition)> = visible_projects
        .iter()
        .map(|project| {
            let rows: Vec<Session> = roots
                .iter()
                .filter(|session| {
                    project_of.get(&session.id) == Some(&project.id)
                        && !in_progress_ids.contains(&session.id)
                })
                .cloned()
                .collect();
            (
                project.clone(),
                partition_folders(&folders, &folder_sessions, &project.id, &rows),
            )
        })
        .collect();
    let search_results: Vec<Session> = if quiz.is_empty() {
        Vec::new()
    } else {
        sessions.to_vec()
    };

    let duration = motion_time(&ctx, DISCLOSURE_SECONDS);
    let detailed = display.detailed_rows;

    let mut select: Option<String> = None;
    let reserved = if app.ui.select_mode { BULK_HEIGHT } else { 0.0 };
    let body_height = (ui.available_height() - FOOTER_HEIGHT - reserved).max(80.0);
    if app.ui.select_mode {
        bulk_bar(ui, app, &p);
    }

    ScrollArea::vertical()
        .id_salt("session-list")
        .stick_to_bottom(false)
        .auto_shrink([false, false])
        .max_height(body_height)
        .show(ui, |ui| {
            if !quiz.is_empty() {
                caption(ui, &p, "Search results");
                if search_results.is_empty() {
                    empty_state(ui, &p, true);
                } else {
                    let show_all = app.ui.search_show_all;
                    let height = flat_height(search_results.len(), detailed, show_all);
                    let reveal = motion::Animated::new("sidebar/search-results")
                        .with_duration(duration)
                        .with_easing(motion::Easing::EaseOut)
                        .bool(&ctx, true);
                    disclosure(ui, height, reveal, |ui| {
                        let shown = if show_all {
                            search_results.len()
                        } else {
                            search_results.len().min(MAX_VISIBLE_ROOTS)
                        };
                        for session in search_results.iter().take(shown) {
                            let event = session_row(
                                ui, app, &p, &sessions, session, 0, detailed, false, false, true,
                                false,
                            );
                            if event.open {
                                select = Some(session.id.clone());
                            }
                        }
                        if search_results.len() > MAX_VISIBLE_ROOTS
                            && show_more_row(ui, &p, show_all, "search")
                        {
                            app.ui.search_show_all = !show_all;
                        }
                        ui.add_space(BOTTOM_GAP);
                    });
                }
            } else if display.grouping == SessionGrouping::Time {
                if roots.is_empty() {
                    empty_state(ui, &p, false);
                } else {
                    let show_all = app.ui.timeline_show_all;
                    let height = roots_height(&sessions, &roots, detailed, show_all, &ctx);
                    let reveal = motion::Animated::new("sidebar/timeline")
                        .with_duration(duration)
                        .with_easing(motion::Easing::EaseOut)
                        .bool(&ctx, true);
                    disclosure(ui, height, reveal, |ui| {
                        let outcome = root_list(
                            ui, app, &p, &sessions, &roots, detailed, show_all, &ctx, "timeline",
                            true,
                        );
                        if let Some(id) = outcome.select {
                            select = Some(id);
                        }
                        if outcome.show_all != show_all {
                            app.ui.timeline_show_all = outcome.show_all;
                        }
                    });
                }
            } else {
                if !in_progress.is_empty() {
                    let collapsed = app
                        .model
                        .workspace_store
                        .collapsed_sections
                        .contains("in_progress");
                    let label = format!("In progress ({})", in_progress.len());
                    let (toggle, _, _) = section_header(
                        ui,
                        &p,
                        "in_progress",
                        &label,
                        collapsed,
                        false,
                        false,
                    );
                    if toggle {
                        toggle_section(app, "in_progress");
                    }
                    let collapsed = app
                        .model
                        .workspace_store
                        .collapsed_sections
                        .contains("in_progress");
                    let show_all = app.ui.in_progress_show_all;
                    let height = roots_height(&sessions, &in_progress, detailed, show_all, &ctx);
                    let reveal = motion::Animated::new("sidebar/in-progress")
                        .with_duration(duration)
                        .with_easing(motion::Easing::EaseOut)
                        .bool(&ctx, !collapsed);
                    disclosure(ui, height, reveal, |ui| {
                        let outcome = root_list(
                            ui,
                            app,
                            &p,
                            &sessions,
                            &in_progress,
                            detailed,
                            show_all,
                            &ctx,
                            "in-progress",
                            true,
                        );
                        if let Some(id) = outcome.select {
                            select = Some(id);
                        }
                        if outcome.show_all != show_all {
                            app.ui.in_progress_show_all = outcome.show_all;
                        }
                    });
                }

                if show_chats {
                    let collapsed = app
                        .model
                        .workspace_store
                        .collapsed_sections
                        .contains("chats");
                    let label = if chats.is_empty() && chats_layout.folders.is_empty() {
                        "Conversations".to_string()
                    } else {
                        format!("Conversations ({})", chats.len())
                    };
                    let (toggle, add, _) =
                        section_header(ui, &p, "chats", &label, collapsed, true, false);
                    if toggle {
                        toggle_section(app, "chats");
                    }
                    if add {
                        app.begin_new_session(Some(crate::app::CHATS_TARGET.to_string()));
                    }
                    let collapsed = app
                        .model
                        .workspace_store
                        .collapsed_sections
                        .contains("chats");
                    if collapsed {
                        // The section is folded; keep its header only.
                    } else if chats.is_empty() && chats_layout.folders.is_empty() {
                        let (row, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), EMPTY_ROW_HEIGHT),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            egui::Pos2::new(row.left() + GRID_INSET, row.center().y),
                            egui::Align2::LEFT_CENTER,
                            "No conversations yet",
                            theme::font(theme::TEXT_META),
                            p.fg_faint,
                        );
                    } else {
                        let show_all = app.ui.chats_show_all;
                        let height = layout_height(
                            &sessions,
                            &chats_layout,
                            detailed,
                            show_all,
                            &app.ui.show_all_folders,
                            &ctx,
                        );
                        let reveal = motion::Animated::new("sidebar/chats")
                            .with_duration(duration)
                            .with_easing(motion::Easing::EaseOut)
                            .bool(&ctx, !collapsed);
                        disclosure(ui, height, reveal, |ui| {
                            let outcome = folder_layout(
                                ui,
                                app,
                                &p,
                                &sessions,
                                &chats_layout,
                                detailed,
                                &ctx,
                                true,
                                show_all,
                                "chats",
                            );
                            if let Some(id) = outcome.select {
                                select = Some(id);
                            }
                            if outcome.show_all != show_all {
                                app.ui.chats_show_all = outcome.show_all;
                            }
                        });
                    }
                }

                if !visible_projects.is_empty() {
                    ui.add_space(SECTION_GAP);
                    let all_collapsed = visible_projects.iter().all(|project| {
                        app.model
                            .workspace_store
                            .collapsed_projects
                            .contains(&project.id)
                    });
                    let label = format!("Projects ({})", visible_projects.len());
                    let (toggle, _, _) =
                        section_header(ui, &p, "projects", &label, all_collapsed, false, false);
                    if toggle {
                        set_all_projects_collapsed(app, &visible_projects, !all_collapsed);
                    }
                }

                for (project, layout) in &project_lists {
                    let collapsed = app
                        .model
                        .workspace_store
                        .collapsed_projects
                        .contains(&project.id);
                    let active = active_project_id.as_deref() == Some(project.id.as_str());
                    let event = project_header(ui, app, &p, project, collapsed, active);
                    if event.toggle {
                        set_project_collapsed(app, &project.id, !collapsed);
                    }
                    if event.add {
                        app.begin_new_session(Some(project.id.clone()));
                    }

                    let collapsed = app
                        .model
                        .workspace_store
                        .collapsed_projects
                        .contains(&project.id);
                    let show_all = app.ui.show_all_projects.contains(&project.id);
                    let height = if layout.is_empty() {
                        EMPTY_ROW_HEIGHT + BOTTOM_GAP
                    } else {
                        layout_height(
                            &sessions,
                            layout,
                            detailed,
                            show_all,
                            &app.ui.show_all_folders,
                            &ctx,
                        )
                    };
                    let reveal = motion::Animated::new(("sidebar/project", &project.id))
                        .with_duration(duration)
                        .with_easing(motion::Easing::EaseOut)
                        .bool(&ctx, !collapsed);
                    disclosure(ui, height, reveal, |ui| {
                        if layout.is_empty() {
                            if empty_project_row(ui, &p, &project.id) {
                                app.begin_new_session(Some(project.id.clone()));
                            }
                            ui.add_space(BOTTOM_GAP);
                        } else {
                            let outcome = folder_layout(
                                ui,
                                app,
                                &p,
                                &sessions,
                                layout,
                                detailed,
                                &ctx,
                                false,
                                show_all,
                                &project.id,
                            );
                            if let Some(id) = outcome.select {
                                select = Some(id);
                            }
                            if outcome.show_all != show_all {
                                if outcome.show_all {
                                    app.ui.show_all_projects.insert(project.id.clone());
                                } else {
                                    app.ui.show_all_projects.remove(&project.id);
                                }
                            }
                        }
                    });
                }
            }
        });

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        footer(ui, app, &p);
    });

    ui.painter().vline(
        side.right() - theme::BORDER * 0.5,
        side.y_range(),
        Stroke::new(theme::BORDER, crate::app::edge(&p)),
    );

    dialogs(&ctx, app);

    if let Some(id) = select {
        app.open_session(id);
    }
}

fn disclosure(ui: &mut Ui, full_height: f32, reveal: f32, add: impl FnOnce(&mut Ui)) {
    if reveal <= 0.001 {
        return;
    }
    let top = ui.cursor().min;
    let width = ui.available_width();
    let (clip, _) = ui.allocate_exact_size(
        Vec2::new(width, full_height * reveal.clamp(0.0, 1.0)),
        Sense::hover(),
    );
    let rect = Rect::from_min_size(top, Vec2::new(width, full_height));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(clip.intersect(child.clip_rect()));
    child.spacing_mut().item_spacing = Vec2::ZERO;
    add(&mut child);
}

fn band(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui)) {
    let rect = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), height));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            add(ui);
        },
    );
    let remaining = rect.bottom() - ui.cursor().min.y;
    if remaining > 0.0 {
        ui.add_space(remaining);
    }
}

fn header(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    band(ui, HEADER_BAND, |ui| {
        let rect = Rect::from_min_size(
            ui.cursor().min,
            Vec2::new(ui.available_width(), HEADER_BAND),
        );
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - theme::BORDER * 0.5,
            Stroke::new(theme::BORDER, p.border),
        );
        let mut right = rect.right() - HEADER_PAD_X;
        let plus = header_action(ui, p, rect.center().y, &mut right, Icon::Plus, "new-session");
        if plus
            .on_hover_text(format!(
                "New session ({})",
                crate::widgets::shortcut_label("N")
            ))
            .clicked()
        {
            app.begin_new_session(None);
        }
        let folder = header_action(
            ui,
            p,
            rect.center().y,
            &mut right,
            Icon::FolderPlus,
            "add-project",
        );
        if folder
            .on_hover_text(format!(
                "Add project ({})",
                crate::widgets::shortcut_label("O")
            ))
            .clicked()
        {
            app.open_add_project();
        }
        let more = header_action(ui, p, rect.center().y, &mut right, Icon::More, "menu");
        display_menu(ui, app, p, more);

        let project = header_project(app);
        let name = project
            .as_ref()
            .map(|project| project.display_name().to_string())
            .unwrap_or_else(|| "RustChamber".into());
        let branch = project
            .as_ref()
            .and_then(|project| project_branch(ui.ctx(), &project.canonical));
        let chip_room = branch
            .as_ref()
            .map(|branch| tag_width(ui, branch) + 6.0)
            .unwrap_or(0.0);
        let room = (right - rect.left() - HEADER_PAD_X - chip_room).max(40.0);
        let galley = WidgetText::from(
            RichText::new(name)
                .font(theme::font_semibold(theme::TEXT_UI_HEADER))
                .color(p.fg),
        )
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            room,
            TextStyle::Body,
        );
        let left = rect.left() + HEADER_PAD_X;
        ui.painter().galley(
            egui::Pos2::new(left, rect.center().y - galley.size().y * 0.5),
            galley.clone(),
            p.fg,
        );
        if let Some(branch) = branch {
            let width = tag_width(ui, &branch);
            let x = (left + galley.size().x + 6.0).min(right - width);
            paint_tag(
                ui,
                Rect::from_min_size(
                    egui::Pos2::new(x, rect.center().y - theme::TAG_HEIGHT * 0.5),
                    Vec2::new(width, theme::TAG_HEIGHT),
                ),
                &branch,
                p.fg_muted,
                p.tag_bg,
            );
        }
    });
}

fn header_action(
    ui: &mut Ui,
    p: &Palette,
    center_y: f32,
    right: &mut f32,
    icon: Icon,
    id: &'static str,
) -> egui::Response {
    let rect = Rect::from_center_size(
        egui::Pos2::new(*right - ACTION * 0.5, center_y),
        Vec2::splat(ACTION),
    );
    *right -= ACTION + 4.0;
    let response = ui.interact(rect, egui::Id::unique(("sidebar-action", id)), Sense::click());
    crate::widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    if hovered > 0.01 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::SM),
            p.fade(p.hover, hovered),
        );
    }
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(ACTION_GLYPH)),
        p.fg_muted.lerp_to_gamma(p.fg, hovered),
    );
    response
}

fn header_project(app: &RustChamber) -> Option<ProjectEntry> {
    let stored = app
        .model
        .workspace_store
        .active_project_id
        .clone()
        .filter(|id| app.model.project_by_id(id).is_some());
    stored
        .or_else(|| {
            app.model.workspace.as_deref().and_then(|directory| {
                app.model
                    .workspace_projects()
                    .into_iter()
                    .find(|project| paths_match(&project.canonical, directory))
                    .map(|project| project.id.clone())
            })
        })
        .or_else(|| {
            app.model
                .active
                .as_ref()
                .and_then(|chat| app.model.project_for_session(&chat.session))
                .map(|project| project.id.clone())
        })
        .and_then(|id| app.model.project_by_id(&id).cloned())
        .or_else(|| app.model.workspace_projects().first().cloned().cloned())
}

fn tag_width(ui: &Ui, label: &str) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        theme::font_mono(theme::TEXT_TAG),
        egui::Color32::PLACEHOLDER,
    );
    galley.size().x + 12.0
}

fn paint_tag(ui: &Ui, rect: Rect, label: &str, fg: egui::Color32, bg: egui::Color32) {
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::XS), bg);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::font_mono(theme::TEXT_TAG), fg);
    ui.painter().galley(
        egui::Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        fg,
    );
}

fn project_branch(ctx: &egui::Context, directory: &str) -> Option<String> {
    let key = egui::Id::unique(("sidebar-branch", directory));
    let now = ctx.input(|input| input.time);
    if let Some((at, value)) = ctx.data(|data| data.get_temp::<(f64, Option<String>)>(key))
        && now - at < 2.0
    {
        return value;
    }
    let value = read_branch(std::path::Path::new(directory));
    ctx.data_mut(|data| data.insert_temp(key, (now, value.clone())));
    value
}

fn read_branch(start: &std::path::Path) -> Option<String> {
    let mut current = start.to_path_buf();
    loop {
        let marker = current.join(".git");
        let git = if marker.is_dir() {
            Some(marker)
        } else if marker.is_file() {
            let text = std::fs::read_to_string(&marker).ok()?;
            let target = text.strip_prefix("gitdir:")?.trim();
            let target = std::path::Path::new(target);
            Some(if target.is_absolute() {
                target.to_path_buf()
            } else {
                current.join(target)
            })
        } else {
            None
        };
        if let Some(git) = git {
            let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
            let head = head.trim();
            if let Some(reference) = head.strip_prefix("ref:") {
                let reference = reference.trim();
                return Some(reference.rsplit('/').next().unwrap_or(reference).to_string());
            }
            if !head.is_empty() {
                return Some(head.chars().take(7).collect());
            }
            return None;
        }
        if !current.pop() {
            return None;
        }
    }
}

fn display_menu(ui: &mut Ui, app: &mut RustChamber, p: &Palette, trigger: egui::Response) {
    let _ = trigger.clone().on_hover_text("Sidebar");
    let id = egui::Id::unique("sidebar-display");
    let mut open = app.ui.sidebar_menu;
    let mut state = app.ui.sidebar_menu;
    let mut items: Vec<egui::Response> = Vec::new();
    menu_popup(ui.ctx(), &trigger, id, &mut open, p, 4.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(248.0);
        let response = crate::widgets::menu_item(
            ui,
            "Customize sections…",
            Some(Icon::SlidersHorizontal),
            None,
            p,
        );
        if response.clicked() {
            app.ui.customize_sections = true;
            state = false;
        }
        items.push(response);
        let response = crate::widgets::menu_item_ex(
            ui,
            if app.ui.select_mode {
                "Exit selection"
            } else {
                "Select sessions"
            },
            Some(Icon::CheckCheck),
            None,
            app.ui.select_mode,
            true,
            p,
        );
        if response.clicked() {
            app.ui.select_mode = !app.ui.select_mode;
            app.ui.selected_sessions.clear();
            state = false;
        }
        items.push(response);
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 4.0);
        ui.add_space(2.0);
        let response = crate::widgets::menu_item_value(
            ui,
            "Sessions",
            grouping_label(app.model.workspace_store.display.grouping),
            Some(Icon::Layers),
            true,
            p,
        );
        if response.clicked() {
            let display = &mut app.model.workspace_store.display;
            display.grouping = match display.grouping {
                SessionGrouping::Project => SessionGrouping::Time,
                SessionGrouping::Time => SessionGrouping::Project,
            };
            app.model.workspace_store.save();
        }
        items.push(response);
        let response = crate::widgets::menu_item_value(
            ui,
            "Order projects",
            sort_label(app.model.workspace_store.display.project_sort),
            Some(Icon::ArrowUpDown),
            true,
            p,
        );
        if response.clicked() {
            let display = &mut app.model.workspace_store.display;
            display.project_sort = match display.project_sort {
                ProjectSort::Manual => ProjectSort::Recent,
                ProjectSort::Recent => ProjectSort::Name,
                ProjectSort::Name => ProjectSort::Manual,
            };
            app.model.workspace_store.save();
        }
        items.push(response);
        let response = crate::widgets::menu_item_value(
            ui,
            "Show projects",
            if app.model.workspace_store.display.show_projects {
                "All projects"
            } else {
                "Active project"
            },
            Some(Icon::FolderOpen),
            true,
            p,
        );
        if response.clicked() {
            app.model.workspace_store.display.show_projects =
                !app.model.workspace_store.display.show_projects;
            app.model.workspace_store.save();
        }
        items.push(response);
        let response = crate::widgets::menu_item_ex(
            ui,
            "Detailed rows",
            Some(Icon::Layers),
            None,
            app.model.workspace_store.display.detailed_rows,
            true,
            p,
        );
        if response.clicked() {
            app.model.workspace_store.display.detailed_rows =
                !app.model.workspace_store.display.detailed_rows;
            app.model.workspace_store.save();
        }
        items.push(response);
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 4.0);
        ui.add_space(2.0);
        let response =
            crate::widgets::menu_item(ui, "Collapse all", Some(Icon::ChevronUp), None, p);
        if response.clicked() {
            collapse_all(app);
            state = false;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "Expand all", Some(Icon::ChevronDown), None, p);
        if response.clicked() {
            expand_all(app);
            state = false;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "Mark all as read", Some(Icon::CheckCheck), None, p);
        if response.clicked() {
            mark_all_read(app);
            state = false;
        }
        items.push(response);
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 4.0);
        ui.add_space(2.0);
        let response = crate::widgets::menu_item(ui, "Archive", Some(Icon::Archive), None, p);
        if response.clicked() {
            app.ui.archive_open = true;
            state = false;
        }
        items.push(response);
        let response = crate::widgets::menu_item(
            ui,
            "Settings",
            Some(Icon::Settings),
            Some(crate::widgets::shortcut_label(",").as_str()),
            p,
        );
        if response.clicked() {
            app.open_settings();
            state = false;
        }
        items.push(response);
    });
    menu_nav_focus(ui.ctx(), id, open, &items);
    if trigger.clicked() {
        state = !state;
    } else if !open {
        state = false;
    }
    app.ui.sidebar_menu = state;
}

fn search_row(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    band(ui, SEARCH_BAND, |ui| {
        ui.add_space(SEARCH_PAD_Y);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(HEADER_PAD_X);
            let width = (ui.available_width() - HEADER_PAD_X).max(60.0);
            let field =
                Rect::from_min_size(ui.cursor().left_top(), Vec2::new(width, SEARCH_HEIGHT));
            let response = ui.allocate_rect(field, Sense::click());
            let id = egui::Id::unique("sidebar-search");
            let focused = ui.memory(|memory| memory.has_focus(id));
            let corner = CornerRadius::same(theme::radius::SM);
            ui.painter().rect_filled(field, corner, p.bg);
            ui.painter().rect_stroke(
                field,
                corner,
                Stroke::new(
                    theme::BORDER,
                    if focused { p.border_focus } else { p.tool_border },
                ),
                egui::StrokeKind::Inside,
            );
            let hint_room = (field.width() - 16.0 - 34.0).max(20.0);
            let text_rect = Rect::from_min_max(
                egui::Pos2::new(field.left() + 8.0, field.top()),
                egui::Pos2::new(field.left() + 8.0 + hint_room, field.bottom()),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(text_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            let edit = child.add(
                egui::TextEdit::singleline(&mut app.ui.session_search)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .align(egui::Align2::LEFT_CENTER)
                    .text_color(p.fg)
                    .font(theme::font(theme::TEXT_META))
                    .desired_width(f32::INFINITY),
            );
            if edit.changed() {
                app.model.search = app.ui.session_search.trim().to_string();
            }
            if response.clicked() || app.ui.focus_search {
                edit.request_focus();
                app.ui.focus_search = false;
            }
            if focused && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                app.ui.session_search.clear();
                app.model.search.clear();
                ui.memory_mut(|memory| memory.surrender_focus(id));
            }
            if !app.ui.session_search.is_empty() {
                let clear = Rect::from_center_size(
                    egui::Pos2::new(field.right() - 16.0, field.center().y),
                    Vec2::splat(22.0),
                );
                let response = ui.interact(
                    clear,
                    egui::Id::unique("sidebar-search-clear"),
                    Sense::click(),
                );
                icons::paint(
                    ui,
                    Icon::Close,
                    Rect::from_center_size(clear.center(), Vec2::splat(12.0)),
                    if response.hovered() { p.fg } else { p.fg_faint },
                );
                if response.clicked() {
                    app.ui.session_search.clear();
                    app.model.search.clear();
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
            } else {
                let label = crate::widgets::shortcut_label("F");
                let galley = ui
                    .painter()
                    .layout_no_wrap(label, theme::font_mono(10.5), p.fg_faint);
                ui.painter().galley(
                    egui::Pos2::new(
                        field.right() - 8.0 - galley.size().x,
                        field.center().y - galley.size().y * 0.5,
                    ),
                    galley,
                    p.fg_faint,
                );
            }
            if app.ui.session_search.is_empty() && !focused {
                ui.painter().text(
                    egui::Pos2::new(field.left() + 8.0, field.center().y),
                    egui::Align2::LEFT_CENTER,
                    "Filter sessions…",
                    theme::font(theme::TEXT_META),
                    p.fg_faint,
                );
            }
        });
        ui.add_space(SEARCH_PAD_Y);
    });
}

fn caption(ui: &mut Ui, p: &Palette, label: &str) {
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), CAPTION_HEIGHT),
        Sense::hover(),
    );
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        theme::font_medium(theme::TEXT_META),
        p.fg_muted,
    );
    ui.painter().galley(
        egui::Pos2::new(
            slot.left() + SECTION_LABEL_INSET,
            slot.center().y - galley.size().y * 0.5,
        ),
        galley,
        p.fg_muted,
    );
}

#[derive(Clone)]
struct FolderGroup {
    folder: SessionFolder,
    sessions: Vec<Session>,
}

struct FolderPartition {
    folders: Vec<FolderGroup>,
    loose: Vec<Session>,
}

impl FolderPartition {
    fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.loose.is_empty()
    }
}

fn partition_folders(
    folders: &[SessionFolder],
    assignments: &HashMap<String, Vec<String>>,
    project_id: &str,
    rows: &[Session],
) -> FolderPartition {
    let mut assigned: HashSet<String> = HashSet::new();
    let mut groups = Vec::new();
    for folder in folders
        .iter()
        .filter(|folder| folder.project_id == project_id)
    {
        let sessions: Vec<Session> = rows
            .iter()
            .filter(|session| {
                !assigned.contains(&session.id)
                    && assignments
                        .get(&folder.id)
                        .is_some_and(|ids| ids.iter().any(|id| id == &session.id))
            })
            .cloned()
            .collect();
        for session in &sessions {
            assigned.insert(session.id.clone());
        }
        groups.push(FolderGroup {
            folder: folder.clone(),
            sessions,
        });
    }
    let loose = rows
        .iter()
        .filter(|session| !assigned.contains(&session.id))
        .cloned()
        .collect();
    FolderPartition {
        folders: groups,
        loose,
    }
}

fn layout_height(
    all: &[Session],
    layout: &FolderPartition,
    detailed: bool,
    loose_show_all: bool,
    show_all_folders: &HashSet<String>,
    ctx: &egui::Context,
) -> f32 {
    let mut height = 0.0;
    for group in &layout.folders {
        height += FOLDER_SLOT;
        if group.folder.collapsed {
            continue;
        }
        if group.sessions.is_empty() {
            height += EMPTY_ROW_HEIGHT + BOTTOM_GAP;
        } else {
            let show_all = show_all_folders.contains(&group.folder.id);
            height += roots_height(all, &group.sessions, detailed, show_all, ctx);
        }
    }
    if !layout.loose.is_empty() {
        height += roots_height(all, &layout.loose, detailed, loose_show_all, ctx);
    }
    height
}

fn folder_layout(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    all: &[Session],
    layout: &FolderPartition,
    detailed: bool,
    ctx: &egui::Context,
    show_project: bool,
    loose_show_all: bool,
    loose_key: &str,
) -> ListOutcome {
    let mut select = None;
    for group in &layout.folders {
        if let Some(id) = folder_section(ui, app, p, all, group, detailed, ctx, show_project) {
            select = Some(id);
        }
    }
    let mut show_all = loose_show_all;
    if !layout.loose.is_empty() {
        let outcome = root_list(
            ui,
            app,
            p,
            all,
            &layout.loose,
            detailed,
            loose_show_all,
            ctx,
            loose_key,
            show_project,
        );
        if outcome.select.is_some() {
            select = outcome.select;
        }
        show_all = outcome.show_all;
    }
    ListOutcome { select, show_all }
}

fn folder_section(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    all: &[Session],
    group: &FolderGroup,
    detailed: bool,
    ctx: &egui::Context,
    show_project: bool,
) -> Option<String> {
    let collapsed = group.folder.collapsed;
    if folder_header(ui, app, p, &group.folder, collapsed, group.sessions.len()) {
        app.model.set_folder_collapsed(&group.folder.id, !collapsed);
    }
    let collapsed = app
        .model
        .workspace_store
        .folders
        .iter()
        .find(|folder| folder.id == group.folder.id)
        .map(|folder| folder.collapsed)
        .unwrap_or(collapsed);
    let show_all = app.ui.show_all_folders.contains(&group.folder.id);
    let height = if group.sessions.is_empty() {
        EMPTY_ROW_HEIGHT + BOTTOM_GAP
    } else {
        roots_height(all, &group.sessions, detailed, show_all, ctx)
    };
    let reveal = if collapsed { 0.0 } else { 1.0 };
    let mut select = None;
    disclosure(ui, height, reveal, |ui| {
        indent_ui(ui, FOLDER_INDENT, |ui| {
            if group.sessions.is_empty() {
                empty_folder_row(ui, p);
                ui.add_space(BOTTOM_GAP);
            } else {
                let outcome = root_list(
                    ui,
                    app,
                    p,
                    all,
                    &group.sessions,
                    detailed,
                    show_all,
                    ctx,
                    &group.folder.id,
                    show_project,
                );
                if let Some(id) = outcome.select {
                    select = Some(id);
                }
                if outcome.show_all != show_all {
                    if outcome.show_all {
                        app.ui.show_all_folders.insert(group.folder.id.clone());
                    } else {
                        app.ui.show_all_folders.remove(&group.folder.id);
                    }
                }
            }
        });
    });
    select
}

fn indent_ui(ui: &mut Ui, amount: f32, add: impl FnOnce(&mut Ui)) {
    let top = ui.cursor().min;
    let rect = Rect::from_min_max(egui::Pos2::new(top.x + amount, top.y), ui.max_rect().max);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect());
    child.spacing_mut().item_spacing = Vec2::ZERO;
    add(&mut child);
}

fn folder_header(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    folder: &SessionFolder,
    collapsed: bool,
    count: usize,
) -> bool {
    let (slot, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), FOLDER_SLOT), Sense::hover());
    let indent = FOLDER_INDENT;
    let card = Rect::from_min_max(
        egui::Pos2::new(slot.left() + ROW_PAD_START + indent, slot.top()),
        egui::Pos2::new(slot.right() - ROW_PAD_END, slot.bottom()),
    );
    let response = ui.interact(
        card,
        egui::Id::unique(("folder-header", &folder.id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    if hovered > 0.0 {
        ui.painter().rect_filled(
            card,
            CornerRadius::same(theme::radius::MD),
            p.scale_alpha(p.hover, 0.4 * hovered),
        );
    }

    let trigger_rect = Rect::from_center_size(
        egui::Pos2::new(
            card.right() - MENU_TRIGGER_INSET - MENU_TRIGGER * 0.5,
            card.center().y,
        ),
        Vec2::splat(MENU_TRIGGER),
    );
    let trigger = ui.interact(
        trigger_rect,
        egui::Id::unique(("folder-menu-trigger", &folder.id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &trigger);
    let menu_open = app.ui.folder_menu.as_deref() == Some(folder.id.as_str());
    let menu_visible = menu_open
        || motion::popup(
            ui.ctx(),
            egui::Id::unique(("folder-menu", &folder.id)).with("popup"),
            false,
        )
        .is_some();
    let show_actions = hovered > 0.0 || trigger.hovered() || menu_visible;
    let action = if show_actions {
        MENU_TRIGGER_INSET + MENU_TRIGGER + 6.0
    } else {
        ROW_END_INSET
    };

    let chevron_center = egui::Pos2::new(card.left() + 8.0, card.center().y);
    icons::paint(
        ui,
        if collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        },
        Rect::from_center_size(chevron_center, Vec2::splat(12.0)),
        p.fg_muted,
    );
    let icon_center = egui::Pos2::new(card.left() + 24.0, card.center().y);
    icons::paint(
        ui,
        if collapsed {
            Icon::Folder
        } else {
            Icon::FolderOpen
        },
        Rect::from_center_size(icon_center, Vec2::splat(FOLDER_GLYPH)),
        if hovered > 0.0 { p.fg } else { p.fg_muted },
    );

    let name_left = card.left() + 38.0;
    let count_galley = (count > 0).then(|| {
        ui.painter()
            .layout_no_wrap(count.to_string(), theme::font(META_SIZE), p.fg_muted)
    });
    let count_width = count_galley
        .as_ref()
        .map(|galley| galley.size().x + 6.0)
        .unwrap_or(0.0);
    let title_room = (card.right() - action - count_width - name_left).max(40.0);
    let label_color = if hovered > 0.0 { p.fg } else { p.fg_muted };
    let name = WidgetText::from(
        RichText::new(&folder.name)
            .font(theme::font_medium(TITLE_SIZE))
            .color(label_color),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        title_room,
        TextStyle::Body,
    );
    ui.painter().galley(
        egui::Pos2::new(name_left, card.center().y - name.size().y * 0.5),
        name.clone(),
        label_color,
    );
    if let Some(count_galley) = count_galley {
        ui.painter().galley(
            egui::Pos2::new(
                name_left + name.size().x + 6.0,
                card.center().y - count_galley.size().y * 0.5,
            ),
            count_galley,
            p.fg_muted,
        );
    }

    if show_actions {
        menu_trigger_paint(ui, p, menu_open, trigger.hovered(), trigger_rect);
    }
    if show_actions || menu_visible {
        folder_menu(ui, app, p, folder, &trigger, menu_open);
    }
    let menu_clicked = trigger.clicked();
    response.clicked() && !menu_clicked
}

fn folder_menu(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    folder: &SessionFolder,
    trigger: &egui::Response,
    menu_open: bool,
) {
    let id = egui::Id::unique(("folder-menu", &folder.id));
    let mut open = menu_open;
    let mut items: Vec<egui::Response> = Vec::new();
    menu_popup(ui.ctx(), trigger, id, &mut open, p, 4.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(196.0);
        let response = crate::widgets::menu_item(ui, "Rename", Some(Icon::Pencil), None, p);
        if response.clicked() {
            app.ui.folder_rename = Some((folder.id.clone(), folder.name.clone()));
            app.ui.folder_menu = None;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item_destructive(ui, "Delete", Some(Icon::Trash), None, true, p);
        if response.clicked() {
            app.ui.confirm_delete_folder = Some(folder.id.clone());
            app.ui.folder_menu = None;
        }
        items.push(response);
    });
    menu_nav_focus(ui.ctx(), id, open, &items);
    if trigger.clicked() {
        app.ui.folder_menu = if menu_open {
            None
        } else {
            Some(folder.id.clone())
        };
    } else if !open && app.ui.folder_menu.as_deref() == Some(folder.id.as_str()) {
        app.ui.folder_menu = None;
    }
}

fn empty_folder_row(ui: &mut Ui, p: &Palette) {
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), EMPTY_ROW_HEIGHT),
        Sense::hover(),
    );
    ui.painter().text(
        egui::Pos2::new(
            slot.left() + ROW_PAD_START + ROW_TEXT_INSET,
            slot.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        "Empty folder",
        theme::font(theme::TEXT_MICRO),
        p.fg_muted,
    );
}

fn project_header(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    project: &ProjectEntry,
    collapsed: bool,
    active: bool,
) -> ProjectEvent {
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), PROJECT_SLOT),
        Sense::hover(),
    );
    let card = Rect::from_min_max(
        egui::Pos2::new(slot.left() + ROW_PAD_START, slot.top() + 1.0),
        egui::Pos2::new(slot.right() - ROW_PAD_END, slot.bottom() - 1.0),
    );
    let response = ui.interact(
        card,
        egui::Id::unique(("project-header", &project.id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let color = project_color(project, p);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    if hovered > 0.0 {
        ui.painter().rect_filled(
            card,
            CornerRadius::same(theme::radius::MD),
            p.scale_alpha(p.hover, 0.4 * hovered),
        );
    }
    if active {
        let bar = Rect::from_min_size(
            egui::Pos2::new(card.left() + 1.0, card.top() + 8.0),
            Vec2::new(2.5, card.height() - 16.0),
        );
        ui.painter().rect_filled(bar, CornerRadius::same(1), color);
    }

    let more_rect = Rect::from_center_size(
        egui::Pos2::new(
            slot.right() - GRID_PAD - SECTION_ACTION * 0.5,
            slot.center().y,
        ),
        Vec2::splat(SECTION_ACTION),
    );
    let plus_rect = Rect::from_center_size(
        egui::Pos2::new(more_rect.left() - SECTION_ACTION * 0.5, slot.center().y),
        Vec2::splat(SECTION_ACTION),
    );
    let more = ui.interact(
        more_rect,
        egui::Id::unique(("project-more", &project.id)),
        Sense::click(),
    );
    let plus = ui.interact(
        plus_rect,
        egui::Id::unique(("project-plus", &project.id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &more);
    crate::widgets::pointer(ui, &plus);
    let menu_open = app.ui.project_menu.as_deref() == Some(project.id.as_str());
    let menu_visible = menu_open
        || motion::popup(
            ui.ctx(),
            egui::Id::unique(("project-menu", &project.id)).with("popup"),
            false,
        )
        .is_some();
    let show_actions = hovered > 0.0 || plus.hovered() || more.hovered() || menu_visible;
    if show_actions {
        icons::paint(
            ui,
            Icon::Plus,
            Rect::from_center_size(plus_rect.center(), Vec2::splat(SECTION_ACTION_GLYPH)),
            if plus.hovered() { p.fg } else { p.fg_muted },
        );
        icons::paint(
            ui,
            Icon::More,
            Rect::from_center_size(more_rect.center(), Vec2::splat(SECTION_ACTION_GLYPH)),
            if more.hovered() || menu_open {
                p.fg
            } else {
                p.fg_muted
            },
        );
    }

    icons::paint(
        ui,
        Icon::Folder,
        Rect::from_center_size(
            egui::Pos2::new(
                slot.left() + GRID_INSET + PROJECT_GLYPH * 0.5,
                slot.center().y,
            ),
            Vec2::splat(PROJECT_GLYPH),
        ),
        color,
    );
    let label_left = slot.left() + GRID_INSET + PROJECT_GLYPH + 7.0;
    let label_color = if active {
        p.fg
    } else {
        p.fg.lerp_to_gamma(p.fg_muted, 0.15)
    };
    let galley = WidgetText::from(
        RichText::new(project.display_name())
            .font(theme::font_medium(theme::TEXT_UI_SECTION))
            .color(label_color),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (plus_rect.left() - label_left - 16.0).max(40.0),
        TextStyle::Body,
    );
    ui.painter().galley(
        egui::Pos2::new(label_left, slot.center().y - galley.size().y / 2.0),
        galley.clone(),
        label_color,
    );
    icons::paint(
        ui,
        if collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        },
        Rect::from_center_size(
            egui::Pos2::new(label_left + galley.size().x + 10.0, slot.center().y),
            Vec2::splat(12.0),
        ),
        p.fg_muted,
    );

    if show_actions || menu_visible {
        project_menu(ui, app, p, project, &more, menu_open);
    }

    let menu_clicked = more.clicked();
    let add_clicked = plus.clicked();
    let header_clicked = response.clicked() && !menu_clicked && !add_clicked;
    ProjectEvent {
        toggle: header_clicked,
        add: add_clicked,
    }
}

#[derive(Default)]
struct ProjectEvent {
    toggle: bool,
    add: bool,
}

fn project_menu(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    project: &ProjectEntry,
    trigger: &egui::Response,
    menu_open: bool,
) {
    let id = egui::Id::unique(("project-menu", &project.id));
    let mut open = menu_open;
    let mut items: Vec<egui::Response> = Vec::new();
    menu_popup(ui.ctx(), trigger, id, &mut open, p, 4.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(228.0);
        let response = crate::widgets::menu_item(ui, "New session", Some(Icon::Plus), None, p);
        if response.clicked() {
            app.begin_new_session(Some(project.id.clone()));
            app.ui.project_menu = None;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "New folder…", Some(Icon::FolderPlus), None, p);
        if response.clicked() {
            app.ui.new_folder = Some(project.id.clone());
            app.ui.new_folder_name.clear();
            app.ui.project_menu = None;
        }
        items.push(response);
        let response = crate::widgets::menu_item(
            ui,
            crate::widgets::reveal_label(),
            Some(Icon::FolderOpen),
            None,
            p,
        );
        if response.clicked() {
            if let Err(error) =
                crate::widgets::reveal_in_finder(std::path::Path::new(&project.canonical))
            {
                app.model
                    .toast(ToastKind::Error, format!("Could not open folder: {error}"));
            }
            app.ui.project_menu = None;
        }
        items.push(response);
        let response = crate::widgets::menu_item(ui, "Edit", Some(Icon::Pencil), None, p);
        if response.clicked() {
            app.ui.project_rename = Some((project.id.clone(), project.display_name().to_string()));
            app.ui.project_menu = None;
        }
        items.push(response);
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 0.0);
        ui.add_space(2.0);
        let response =
            crate::widgets::menu_item(ui, "Archive all sessions", Some(Icon::Archive), None, p);
        if response.clicked() {
            let ids: Vec<String> = app
                .model
                .sessions_for_project(project)
                .into_iter()
                .filter(|session| session.time.archived.is_none())
                .map(|session| session.id.clone())
                .collect();
            if ids.is_empty() {
                app.model
                    .toast(ToastKind::Info, "No active sessions in this project");
            } else {
                app.ui.confirm_archive_all = Some(project.id.clone());
            }
            app.ui.project_menu = None;
        }
        items.push(response);
        let response = crate::widgets::menu_item_destructive(
            ui,
            "Close project",
            Some(Icon::Close),
            None,
            true,
            p,
        );
        if response.clicked() {
            let count = app.model.sessions_for_project(project).len();
            if count > 0 {
                app.ui.confirm_close_project = Some(project.id.clone());
            } else {
                close_project(app, &project.id);
            }
            app.ui.project_menu = None;
        }
        items.push(response);
    });
    menu_nav_focus(ui.ctx(), id, open, &items);
    if trigger.clicked() {
        app.ui.project_menu = if menu_open {
            None
        } else {
            Some(project.id.clone())
        };
    } else if !open && app.ui.project_menu.as_deref() == Some(project.id.as_str()) {
        app.ui.project_menu = None;
    }
}

fn section_header(
    ui: &mut Ui,
    p: &Palette,
    id: &str,
    label: &str,
    collapsed: bool,
    add_button: bool,
    folder_button: bool,
) -> (bool, bool, bool) {
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), SECTION_HEIGHT),
        Sense::hover(),
    );
    let response = ui.interact(
        slot,
        egui::Id::unique(("sidebar-section", id)),
        Sense::click(),
    );
    let mut add_clicked = false;
    let mut add_hovered = false;
    let mut folder_clicked = false;
    let mut folder_hovered = false;
    let add_rect = Rect::from_center_size(
        egui::Pos2::new(
            slot.right() - GRID_PAD - SECTION_ACTION * 0.5,
            slot.center().y,
        ),
        Vec2::splat(SECTION_ACTION),
    );
    let folder_rect = if add_button {
        Rect::from_center_size(
            egui::Pos2::new(add_rect.left() - SECTION_ACTION * 0.5, slot.center().y),
            Vec2::splat(SECTION_ACTION),
        )
    } else {
        add_rect
    };
    if folder_button {
        let folder = ui.interact(
            folder_rect,
            egui::Id::unique(("sidebar-section-folder", id)),
            Sense::click(),
        );
        crate::widgets::pointer(ui, &folder);
        folder_hovered = folder.hovered();
        folder_clicked = folder.clicked();
        if folder_hovered || response.hovered() {
            icons::paint(
                ui,
                Icon::FolderPlus,
                Rect::from_center_size(folder_rect.center(), Vec2::splat(SECTION_ACTION_GLYPH)),
                if folder_hovered { p.fg } else { p.fg_muted },
            );
        }
    }
    if add_button {
        let add = ui.interact(
            add_rect,
            egui::Id::unique(("sidebar-section-add", id)),
            Sense::click(),
        );
        crate::widgets::pointer(ui, &add);
        add_hovered = add.hovered();
        add_clicked = add.clicked();
        icons::paint(
            ui,
            Icon::Plus,
            Rect::from_center_size(add_rect.center(), Vec2::splat(SECTION_ACTION_GLYPH)),
            if add_hovered { p.fg } else { p.fg_faint },
        );
    }
    let galley = ui.painter().layout_no_wrap(
        label.to_uppercase(),
        theme::font_mono(11.0),
        p.fg_muted,
    );
    let left = slot.left() + SECTION_LABEL_INSET;
    ui.painter().galley(
        egui::Pos2::new(left, slot.center().y - galley.size().y * 0.5),
        galley.clone(),
        p.fg_muted,
    );
    if collapsed || response.hovered() || add_hovered || folder_hovered {
        icons::paint(
            ui,
            if collapsed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            },
            Rect::from_center_size(
                egui::Pos2::new(
                    left + galley.size().x + 4.0 + SECTION_CHEVRON * 0.5,
                    slot.center().y,
                ),
                Vec2::splat(SECTION_CHEVRON),
            ),
            p.fg_muted,
        );
    }
    (
        response.clicked() && !add_clicked && !folder_clicked,
        add_clicked,
        folder_clicked,
    )
}

fn menu_shown(row_hovered: bool, trigger_hovered: bool, menu_visible: bool) -> bool {
    row_hovered || trigger_hovered || menu_visible
}

fn row_interactions(
    ui: &mut Ui,
    session_id: &str,
    card: Rect,
    trigger: Rect,
) -> (egui::Response, egui::Response) {
    let row = ui.interact(
        card,
        egui::Id::unique(("session-row", session_id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &row);
    let trigger = ui.interact(
        trigger,
        egui::Id::unique(("session-menu-trigger", session_id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &trigger);
    (row, trigger)
}

fn empty_state(ui: &mut Ui, p: &Palette, searching: bool) {
    ui.add_space(18.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(if searching {
                "No matching sessions"
            } else {
                "No sessions yet"
            })
            .font(theme::font_medium(13.0))
            .color(p.fg_muted),
        );
        ui.add_space(3.0);
        ui.label(
            RichText::new(if searching {
                "Try a different title, ID, or path."
            } else {
                "Create your first session to start coding."
            })
            .font(theme::font(theme::TEXT_MICRO))
            .color(p.fg_muted),
        );
        let _ = ui.allocate_exact_size(Vec2::new(1.0, EMPTY_BLOCK - 40.0), Sense::hover());
    });
}

fn empty_project_row(ui: &mut Ui, p: &Palette, project_id: &str) -> bool {
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), EMPTY_ROW_HEIGHT),
        Sense::hover(),
    );
    let response = ui.interact(
        slot,
        egui::Id::unique(("project-empty", project_id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let color = if response.hovered() { p.fg } else { p.fg_muted };
    ui.painter().text(
        egui::Pos2::new(slot.left() + ROW_TEXT_INSET, slot.center().y),
        egui::Align2::LEFT_CENTER,
        "Start a session",
        theme::font(theme::TEXT_MICRO),
        color,
    );
    response.clicked()
}

struct RowEvent {
    open: bool,
    toggle: bool,
}

fn session_row(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    all: &[Session],
    session: &Session,
    depth: usize,
    detailed: bool,
    has_children: bool,
    expanded: bool,
    show_project: bool,
    in_progress: bool,
) -> RowEvent {
    let active = app.model.active_id() == Some(session.id.as_str());
    let selected = app.ui.select_mode && app.ui.selected_sessions.contains(&session.id);
    let row_detailed = detailed && depth == 0;
    let slot_height = row_height(depth, detailed);
    let indent = depth as f32 * TREE_INDENT;
    let (slot, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), slot_height), Sense::hover());
    let card = Rect::from_min_max(
        egui::Pos2::new(slot.left() + ROW_PAD_START + indent, slot.top() + 2.0),
        egui::Pos2::new(slot.right() - ROW_PAD_END, slot.bottom() - 2.0),
    );
    let menu_open = app.ui.session_menu.as_deref() == Some(session.id.as_str());
    let menu_id = egui::Id::unique(("session-menu", &session.id));
    let menu_visible = menu_open || motion::popup(ui.ctx(), menu_id.with("popup"), false).is_some();
    let mark_y = if row_detailed {
        card.top() + ROW_PAD_Y_DETAILED + 10.0
    } else {
        card.center().y
    };
    let trigger_rect = Rect::from_center_size(
        egui::Pos2::new(
            card.right() - MENU_TRIGGER_INSET - MENU_TRIGGER * 0.5,
            mark_y,
        ),
        Vec2::splat(MENU_TRIGGER),
    );
    let (response, trigger) = row_interactions(ui, &session.id, card, trigger_rect);
    let pressed = response.is_pointer_button_down_on();
    let show_menu = menu_shown(response.hovered(), trigger.hovered(), menu_visible);
    let shift = ui.input(|input| input.modifiers.shift);
    let check_rect = Rect::from_center_size(
        egui::Pos2::new(trigger_rect.left() - 6.0 - 10.0, mark_y),
        Vec2::splat(20.0),
    );
    let archive_rect = Rect::from_center_size(
        egui::Pos2::new(check_rect.left() - 4.0 - 10.0, mark_y),
        Vec2::splat(20.0),
    );
    let offscreen = Rect::from_min_size(egui::Pos2::new(-10_000.0, -10_000.0), Vec2::ZERO);
    let check = if in_progress {
        ui.interact(
            check_rect,
            egui::Id::unique(("session-done", &session.id)),
            Sense::click(),
        )
        .on_hover_text("Mark done")
    } else {
        ui.interact(
            offscreen,
            egui::Id::unique(("session-done", &session.id)),
            Sense::hover(),
        )
    };
    let archive = if in_progress {
        ui.interact(
            archive_rect,
            egui::Id::unique(("session-archive", &session.id)),
            Sense::click(),
        )
        .on_hover_text(if shift {
            "Delete session (⇧)"
        } else {
            "Archive session"
        })
    } else {
        ui.interact(
            offscreen,
            egui::Id::unique(("session-archive", &session.id)),
            Sense::hover(),
        )
    };
    crate::widgets::pointer(ui, &check);
    crate::widgets::pointer(ui, &archive);
    let actions_visible = in_progress
        && (response.hovered()
            || trigger.hovered()
            || check.hovered()
            || archive.hovered()
            || menu_visible);
    let show_trigger = show_menu || actions_visible;
    let check_clicked = actions_visible && check.clicked();
    let archive_clicked = actions_visible && archive.clicked();

    let fill = if active || selected {
        p.active
    } else if pressed {
        p.hover
    } else {
        p.hover
    };
    if active || selected || pressed || show_trigger {
        ui.painter()
            .rect_filled(card, CornerRadius::same(theme::radius::SM), fill);
    }
    if active {
        ui.painter().rect_filled(
            Rect::from_center_size(
                egui::Pos2::new(card.left() + 1.0, card.center().y),
                Vec2::new(2.0, 16.0),
            ),
            CornerRadius::same(1),
            p.primary,
        );
    }
    if selected {
        ui.painter().rect_stroke(
            card,
            CornerRadius::same(theme::radius::SM),
            Stroke::new(theme::HAIRLINE, p.primary),
            egui::StrokeKind::Inside,
        );
    }

    let running_since = running_since(app, all, &session.id);
    let running = running_since.is_some();
    let pending = permission_pending(app, &session.id);
    let unread = app.model.is_unread(session);
    let pinned = app.model.is_pinned(session);
    let work = app.model.is_work(session);
    let title_color = if active || selected {
        p.fg
    } else if depth > 0 {
        p.fg_muted
    } else {
        p.fg
    };

    let state_center = egui::Pos2::new(card.left() + ROW_STATE_CENTER, card.center().y);
    let mut chevron_clicked = false;
    if has_children {
        let chevron = tree_chevron(ui, p, expanded, state_center, &session.id);
        chevron_clicked = chevron.clicked();
    } else if app.ui.select_mode {
        icons::paint(
            ui,
            if selected {
                Icon::CircleCheck
            } else {
                Icon::Circle
            },
            Rect::from_center_size(state_center, Vec2::splat(MARK)),
            if selected { p.primary } else { p.fg_muted },
        );
    } else if pending {
        icons::paint(
            ui,
            Icon::Shield,
            Rect::from_center_size(state_center, Vec2::splat(MARK)),
            p.coral,
        );
    } else if running {
        ui.painter()
            .circle_filled(state_center, 3.5, p.primary);
    } else if session.outcome == Some(Outcome::Failed) {
        ui.painter().circle_filled(state_center, 3.0, p.red);
    } else if unread {
        ui.painter().circle_filled(state_center, 2.5, p.primary);
    } else {
        ui.painter().circle_filled(state_center, 2.0, p.fg_muted);
    }

    let now = rchamber_app::now_ms();
    let time_color = p.fg_muted;
    let diff_stats = session_diff(app, &session.id);
    let time_galley = if row_detailed || diff_stats.is_some() {
        None
    } else {
        let time = match running_since {
            Some(started) => format_elapsed(now - started),
            None => compact_time(session.time.updated),
        };
        (!time.is_empty()).then(|| {
            ui.painter()
                .layout_no_wrap(time, theme::font_mono(META_MONO), time_color)
        })
    };
    let diff_meta = diff_stats.map(|(added, removed)| {
        (
            ui.painter().layout_no_wrap(
                format!("+{added}"),
                theme::font_mono(META_MONO),
                p.diff_added,
            ),
            ui.painter().layout_no_wrap(
                format!("-{removed}"),
                theme::font_mono(META_MONO),
                p.diff_removed,
            ),
        )
    });
    let marker_width = if pinned { MARK + 4.0 } else { 0.0 }
        + if work && !running { MARK + 4.0 } else { 0.0 }
        + if running { MARK + 4.0 } else { 0.0 };
    let action_width = if actions_visible { 48.0 } else { 0.0 };
    let trailing = if show_trigger {
        MENU_TRIGGER_INSET + MENU_TRIGGER + 6.0 + action_width
    } else {
        let meta_width = if let Some((added, removed)) = diff_meta.as_ref() {
            added.size().x + 4.0 + removed.size().x + 4.0
        } else {
            time_galley
                .as_ref()
                .map(|galley| galley.size().x + 4.0)
                .unwrap_or(0.0)
        };
        ROW_END_INSET + meta_width + marker_width
    };
    let title_left = card.left() + ROW_TEXT_INSET;
    let title_room = (card.right() - title_left - trailing).max(40.0);
    let title_font = if depth > 0 {
        theme::font(CHILD_TITLE_SIZE)
    } else if unread || pending || running {
        theme::font_medium(TITLE_SIZE)
    } else {
        theme::font(TITLE_SIZE)
    };
    let title = WidgetText::from(
        RichText::new(session.title())
            .font(title_font)
            .color(title_color),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        title_room,
        TextStyle::Body,
    );
    let title_y = if row_detailed {
        card.top() + ROW_PAD_Y_DETAILED + 20.0 * 0.5 - title.size().y * 0.5
    } else {
        card.center().y - title.size().y * 0.5
    };
    ui.painter()
        .galley(egui::Pos2::new(title_left, title_y), title, title_color);

    if row_detailed {
        let meta = meta_line(app, session, show_project);
        let meta_galley = WidgetText::from(
            RichText::new(meta)
                .font(theme::font(META_SIZE))
                .color(p.fg_muted),
        )
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (card.width() - ROW_TEXT_INSET - 14.0).max(40.0),
            TextStyle::Body,
        );
        ui.painter().galley(
            egui::Pos2::new(title_left, card.top() + ROW_PAD_Y_DETAILED + 22.0),
            meta_galley,
            p.fg_muted,
        );
    }

    if !show_trigger {
        let mut end = card.right() - ROW_END_INSET;
        if let Some((added, removed)) = &diff_meta {
            end -= removed.size().x;
            ui.painter().galley(
                egui::Pos2::new(end, card.center().y - removed.size().y * 0.5),
                removed.clone(),
                p.diff_removed,
            );
            end -= 4.0;
            end -= added.size().x;
            ui.painter().galley(
                egui::Pos2::new(end, card.center().y - added.size().y * 0.5),
                added.clone(),
                p.diff_added,
            );
            end -= 4.0;
        } else if let Some(galley) = &time_galley {
            end -= galley.size().x;
            ui.painter().galley(
                egui::Pos2::new(end, card.center().y - galley.size().y * 0.5),
                galley.clone(),
                time_color,
            );
            end -= 4.0;
        }
        if running {
            running_spinner(
                ui,
                egui::Pos2::new(end - MARK * 0.5, mark_y),
                p,
                ui.input(|input| input.time),
            );
            end -= MARK + 4.0;
        } else if work {
            icons::paint(
                ui,
                Icon::CircleDot,
                Rect::from_center_size(
                    egui::Pos2::new(end - MARK * 0.5, mark_y),
                    Vec2::splat(MARK),
                ),
                p.fg_muted,
            );
            end -= MARK + 4.0;
        }
        if pinned {
            icons::paint(
                ui,
                Icon::Pin,
                Rect::from_center_size(
                    egui::Pos2::new(end - MARK * 0.5, mark_y),
                    Vec2::splat(MARK),
                ),
                p.fg_muted,
            );
        }
    }

    if actions_visible {
        icons::paint(
            ui,
            Icon::CircleCheck,
            Rect::from_center_size(check_rect.center(), Vec2::splat(MARK + 2.0)),
            if check.hovered() {
                p.green
            } else {
                p.green.lerp_to_gamma(p.fg, 0.12)
            },
        );
        icons::paint(
            ui,
            if shift { Icon::Trash } else { Icon::Archive },
            Rect::from_center_size(archive_rect.center(), Vec2::splat(MARK + 1.0)),
            if shift {
                if archive.hovered() {
                    p.red
                } else {
                    p.red.lerp_to_gamma(p.fg, 0.15)
                }
            } else if archive.hovered() {
                p.fg
            } else {
                p.fg_muted
            },
        );
    }
    if show_trigger {
        menu_trigger_paint(ui, p, menu_open, trigger.hovered(), trigger_rect);
    }
    let menu_clicked = if show_trigger {
        session_menu(ui, app, p, session, &trigger, menu_open)
    } else {
        false
    };
    let action_clicked = check_clicked || archive_clicked;
    if check_clicked {
        app.model.set_work_local(&session.id, false);
        app.backend.send(Action::SetSessionWork {
            id: session.id.clone(),
            work: false,
        });
    }
    if archive_clicked {
        if shift {
            app.ui.confirm_delete = Some(session.id.clone());
        } else {
            app.backend.send(Action::ArchiveSession {
                id: session.id.clone(),
                archived: true,
            });
            if active {
                app.model.clear_active();
            }
        }
    }
    let recap = session_recap(session);
    let cached_preview = app.model.session_previews.get(&session.id).cloned();
    if in_progress
        && recap.is_none()
        && cached_preview.is_none()
        && response.hovered()
        && app.model.request_session_preview(&session.id)
    {
        app.backend
            .send(Action::LoadSessionPreview(session.id.clone()));
    }
    let response = if in_progress {
        if check.hovered() || archive.hovered() {
            response
        } else {
            let title = session.title().to_string();
            let elapsed = relative_time(session.time.updated);
            let project = app
                .model
                .project_for_session(session)
                .map(|project| project.display_name().to_string())
                .unwrap_or_else(|| "Chats".into());
            let model = session
                .model
                .as_ref()
                .and_then(|reference| app.model.model_for(reference))
                .map(|model| model.display_name().to_string())
                .unwrap_or_default();
            let preview = recap.or(cached_preview).unwrap_or_default();
            response.on_hover_ui(|ui| {
                session_hover_card(ui, p, &title, &elapsed, &project, &model, &preview)
            })
        }
    } else {
        response.on_hover_text(tooltip_for(app, session, show_project))
    };
    let mut open = false;
    if response.clicked() && !menu_clicked && !chevron_clicked && !action_clicked {
        if app.ui.select_mode {
            if app.ui.selected_sessions.contains(&session.id) {
                app.ui.selected_sessions.remove(&session.id);
            } else {
                app.ui.selected_sessions.insert(session.id.clone());
            }
        } else {
            open = true;
        }
    }
    RowEvent {
        open,
        toggle: chevron_clicked,
    }
}

fn tree_chevron(
    ui: &mut Ui,
    p: &Palette,
    expanded: bool,
    center: egui::Pos2,
    session_id: &str,
) -> egui::Response {
    let rect = Rect::from_center_size(center, Vec2::splat(20.0));
    let response = ui.interact(
        rect,
        egui::Id::unique(("session-chevron", session_id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let color = if response.hovered() { p.fg } else { p.fg_muted };
    icons::paint(
        ui,
        if expanded {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        Rect::from_center_size(center, Vec2::splat(SECTION_CHEVRON)),
        color,
    );
    response
}

fn tree_expanded(ctx: &egui::Context, session_id: &str) -> bool {
    ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::unique(("session-tree-expanded", session_id)))
            .unwrap_or(true)
    })
}

fn tree_set_expanded(ctx: &egui::Context, session_id: &str, expanded: bool) {
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::unique(("session-tree-expanded", session_id)),
            expanded,
        )
    });
}

fn busy_any(app: &RustChamber, all: &[Session], id: &str) -> bool {
    let Some(session) = all.iter().find(|session| session.id == id) else {
        return false;
    };
    if running(app, session) {
        return true;
    }
    all.iter()
        .any(|session| session.parent_id.as_deref() == Some(id) && busy_any(app, all, &session.id))
}

fn show_more_row(ui: &mut Ui, p: &Palette, expanded: bool, id: &str) -> bool {
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), SHOW_MORE_HEIGHT),
        Sense::hover(),
    );
    let response = ui.interact(
        slot,
        egui::Id::unique(("sidebar-show-more-row", id)),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let label = if expanded { "Show less" } else { "Show more" };
    let color = if response.hovered() { p.fg } else { p.fg_muted };
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), theme::font(theme::TEXT_MICRO), color);
    ui.painter().galley(
        egui::Pos2::new(
            slot.left() + ROW_TEXT_INSET,
            slot.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    response.clicked()
}

struct ListOutcome {
    select: Option<String>,
    show_all: bool,
}

fn root_list(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    all: &[Session],
    roots: &[Session],
    detailed: bool,
    show_all: bool,
    ctx: &egui::Context,
    key: &str,
    show_project: bool,
) -> ListOutcome {
    let in_progress = key == "in-progress";
    let mut select = None;
    let shown = if show_all {
        roots.len()
    } else {
        roots.len().min(MAX_VISIBLE_ROOTS)
    };
    for root in roots.iter().take(shown) {
        let has_children = all
            .iter()
            .any(|session| session.parent_id.as_deref() == Some(root.id.as_str()));
        let expanded = tree_expanded(ctx, &root.id);
        let event = session_row(
            ui,
            app,
            p,
            all,
            root,
            0,
            detailed,
            has_children,
            expanded,
            show_project,
            in_progress,
        );
        if event.open {
            select = Some(root.id.clone());
        }
        if event.toggle {
            tree_set_expanded(ctx, &root.id, !expanded);
        }
        if expanded {
            render_children(ui, app, p, all, &root.id, 1, ctx, show_project, &mut select);
        }
    }
    if roots.len() > MAX_VISIBLE_ROOTS && show_more_row(ui, p, show_all, key) {
        return ListOutcome {
            select,
            show_all: !show_all,
        };
    }
    ui.add_space(BOTTOM_GAP);
    ListOutcome { select, show_all }
}

fn render_children(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    all: &[Session],
    parent_id: &str,
    depth: usize,
    ctx: &egui::Context,
    show_project: bool,
    select: &mut Option<String>,
) {
    for child in all
        .iter()
        .filter(|session| session.parent_id.as_deref() == Some(parent_id))
    {
        let has_children = all
            .iter()
            .any(|session| session.parent_id.as_deref() == Some(child.id.as_str()));
        let expanded = tree_expanded(ctx, &child.id);
        let event = session_row(
            ui,
            app,
            p,
            all,
            child,
            depth,
            false,
            has_children,
            expanded,
            show_project,
            false,
        );
        if event.open {
            *select = Some(child.id.clone());
        }
        if event.toggle {
            tree_set_expanded(ctx, &child.id, !expanded);
        }
        if expanded {
            render_children(
                ui,
                app,
                p,
                all,
                &child.id,
                depth + 1,
                ctx,
                show_project,
                select,
            );
        }
    }
}

fn row_height(depth: usize, detailed: bool) -> f32 {
    if depth > 0 {
        CHILD_ROW_HEIGHT
    } else if detailed {
        ROW_HEIGHT_DETAILED
    } else {
        ROW_HEIGHT
    }
}

fn roots_height(
    all: &[Session],
    roots: &[Session],
    detailed: bool,
    show_all: bool,
    ctx: &egui::Context,
) -> f32 {
    let shown = if show_all {
        roots.len()
    } else {
        roots.len().min(MAX_VISIBLE_ROOTS)
    };
    let mut height = 0.0;
    for root in roots.iter().take(shown) {
        height += row_height(0, detailed);
        if tree_expanded(ctx, &root.id) {
            height += subtree_height(all, ctx, &root.id);
        }
    }
    if roots.len() > MAX_VISIBLE_ROOTS {
        height += SHOW_MORE_HEIGHT;
    }
    height + BOTTOM_GAP
}

fn flat_height(count: usize, detailed: bool, show_all: bool) -> f32 {
    let shown = if show_all {
        count
    } else {
        count.min(MAX_VISIBLE_ROOTS)
    };
    let mut height = shown as f32 * row_height(0, detailed);
    if count > MAX_VISIBLE_ROOTS {
        height += SHOW_MORE_HEIGHT;
    }
    height + BOTTOM_GAP
}

fn subtree_height(all: &[Session], ctx: &egui::Context, parent_id: &str) -> f32 {
    let mut height = 0.0;
    for child in all
        .iter()
        .filter(|session| session.parent_id.as_deref() == Some(parent_id))
    {
        height += CHILD_ROW_HEIGHT;
        if tree_expanded(ctx, &child.id) {
            height += subtree_height(all, ctx, &child.id);
        }
    }
    height
}

fn running_since(app: &RustChamber, all: &[Session], id: &str) -> Option<f64> {
    let session = all.iter().find(|session| session.id == id)?;
    let mut started = if running(app, session) {
        if let Some(chat) = app
            .model
            .active
            .as_ref()
            .filter(|chat| chat.session.id == session.id)
        {
            session
                .time
                .busy
                .filter(|busy| session.time.idle.is_none_or(|idle| *busy > idle))
                .or(chat.turn_started)
                .or(session.time.busy)
                .or(Some(session.time.updated))
        } else {
            Some(session.time.busy.unwrap_or(session.time.updated))
        }
    } else {
        None
    };
    for child in all
        .iter()
        .filter(|child| child.parent_id.as_deref() == Some(id))
    {
        if let Some(child_started) = running_since(app, all, &child.id) {
            started = Some(started.map_or(child_started, |value| value.min(child_started)));
        }
    }
    started
}

fn compact_time(updated: f64) -> String {
    const MINUTE: f64 = 60_000.0;
    let delta = (rchamber_app::now_ms() - updated).max(0.0);
    if delta < MINUTE {
        "now".into()
    } else if delta < 60.0 * MINUTE {
        format!("{}m", (delta / MINUTE).round() as i64)
    } else if delta < 24.0 * 60.0 * MINUTE {
        format!("{}h", (delta / (60.0 * MINUTE)).round() as i64)
    } else {
        format!("{}d", (delta / (24.0 * 60.0 * MINUTE)).round() as i64)
    }
}

fn format_elapsed(ms: f64) -> String {
    let total = (ms.max(0.0) / 1000.0) as u64;
    if total < 60 {
        format!("{total}s")
    } else if total < 3_600 {
        format!("{}m {}s", total / 60, total % 60)
    } else {
        format!("{}h {}m", total / 3_600, (total % 3_600) / 60)
    }
}

fn running_spinner(ui: &Ui, center: egui::Pos2, p: &Palette, time: f64) {
    let radius = 5.0;
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.0, p.alpha(p.blue, 45)));
    let start = if motion::reduced_motion(ui.ctx()) {
        -std::f32::consts::FRAC_PI_2
    } else {
        -std::f32::consts::FRAC_PI_2 + (time * 2.2) as f32
    };
    let sweep = 2.3_f32;
    let steps = 16;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|index| {
            let angle = start + sweep * index as f32 / steps as f32;
            egui::Pos2::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.5, p.blue)));
}

fn running(app: &RustChamber, session: &Session) -> bool {
    if let Some(chat) = app
        .model
        .active
        .as_ref()
        .filter(|chat| chat.session.id == session.id)
    {
        return chat.busy;
    }
    match (session.time.busy, session.time.idle) {
        (Some(busy), Some(idle)) => busy > idle,
        (Some(_), None) => true,
        _ => false,
    }
}

fn session_diff(app: &RustChamber, id: &str) -> Option<(u32, u32)> {
    let chat = app
        .model
        .active
        .as_ref()
        .filter(|chat| chat.session.id == id)?;
    let added: u32 = chat.diff.iter().map(|file| file.additions).sum();
    let removed: u32 = chat.diff.iter().map(|file| file.deletions).sum();
    (added + removed > 0).then_some((added, removed))
}

fn permission_pending(app: &RustChamber, id: &str) -> bool {
    app.model
        .active
        .as_ref()
        .is_some_and(|chat| chat.session.id == id && chat.awaiting_permission)
}

fn menu_trigger_paint(ui: &Ui, p: &Palette, active: bool, hovered: bool, rect: Rect) {
    if hovered && !active {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::SM),
            p.scale_alpha(p.hover, 0.9),
        );
    }
    icons::paint(
        ui,
        Icon::More,
        Rect::from_center_size(rect.center(), Vec2::splat(MENU_TRIGGER_GLYPH)),
        if active || hovered { p.fg } else { p.fg_muted },
    );
}

fn session_menu(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    session: &Session,
    trigger: &egui::Response,
    menu_open: bool,
) -> bool {
    let id = egui::Id::unique(("session-menu", &session.id));
    let mut open = menu_open;
    let mut items: Vec<egui::Response> = Vec::new();
    let pinned = app.model.is_pinned(session);
    let work = app.model.is_work(session);
    menu_popup(ui.ctx(), trigger, id, &mut open, p, 4.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(236.0);
        let response = crate::widgets::menu_item(ui, "Rename", Some(Icon::Pencil), None, p);
        if response.clicked() {
            app.ui.rename = Some((session.id.clone(), session.title().to_string()));
            app.ui.session_menu = None;
        }
        items.push(response);
        let response = crate::widgets::menu_item_ex(
            ui,
            "Rename with AI",
            Some(Icon::Spark),
            None,
            false,
            false,
            p,
        );
        let response = response.on_hover_text("Small model not configured");
        items.push(response);
        let response = crate::widgets::menu_item(ui, "Copy session ID", Some(Icon::Copy), None, p);
        if response.clicked() {
            copy_to_clipboard(ui.ctx(), &session.id);
            app.model.toast(ToastKind::Success, "Session ID copied");
            app.ui.session_menu = None;
        }
        items.push(response);
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 0.0);
        ui.add_space(2.0);
        let response = crate::widgets::menu_item(
            ui,
            if pinned { "Unpin" } else { "Pin" },
            Some(Icon::Pin),
            None,
            p,
        );
        if response.clicked() {
            app.model.set_pinned_local(&session.id, !pinned);
            app.backend.send(Action::SetSessionPinned {
                id: session.id.clone(),
                pinned: !pinned,
            });
            app.model.toast(
                ToastKind::Success,
                if pinned {
                    "Session unpinned"
                } else {
                    "Session pinned"
                },
            );
            app.ui.session_menu = None;
        }
        items.push(response);
        let response = crate::widgets::menu_item(
            ui,
            if work {
                "Mark done"
            } else {
                "Track as in progress"
            },
            Some(if work { Icon::EyeOff } else { Icon::Eye }),
            None,
            p,
        );
        if response.clicked() {
            app.model.set_work_local(&session.id, !work);
            app.backend.send(Action::SetSessionWork {
                id: session.id.clone(),
                work: !work,
            });
            app.ui.session_menu = None;
        }
        items.push(response);
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 0.0);
        ui.add_space(2.0);
        let response =
            crate::widgets::menu_item(ui, "Export Markdown", Some(Icon::Download), None, p);
        if response.clicked() {
            export_markdown(app, session);
            app.ui.session_menu = None;
        }
        items.push(response);
        let response = crate::widgets::menu_item_ex(
            ui,
            "Move to worktree",
            Some(Icon::Branch),
            None,
            false,
            false,
            p,
        );
        let response = response.on_hover_text("Worktree support is coming next");
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "Move session…", Some(Icon::FolderInput), None, p);
        if response.clicked() {
            move_session(app, session);
            app.ui.session_menu = None;
        }
        items.push(response);
        let response = if session.parent_id.is_none() {
            let response =
                crate::widgets::menu_item(ui, "Move to folder…", Some(Icon::Move), None, p);
            if response.clicked() {
                app.ui.move_to_folder = Some(session.id.clone());
                app.ui.move_folder_name.clear();
                app.ui.session_menu = None;
            }
            response
        } else {
            let response = crate::widgets::menu_item_ex(
                ui,
                "Move to folder…",
                Some(Icon::Move),
                None,
                false,
                false,
                p,
            );
            response.on_hover_text("Move the parent session")
        };
        items.push(response);
        let response = crate::widgets::menu_item(
            ui,
            "Open in side panel",
            Some(Icon::PanelRightOpen),
            None,
            p,
        );
        if response.clicked() {
            app.model.toast(
                ToastKind::Info,
                "The side panel opens this session in a later build",
            );
            app.ui.session_menu = None;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "Open in Mini Chat", Some(Icon::MessageSquare), None, p);
        if response.clicked() {
            app.model.toast(
                ToastKind::Info,
                "Mini Chat is not available in this build yet",
            );
            app.ui.session_menu = None;
        }
        items.push(response);
        let extension_actions: Vec<(String, String, String)> = app
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
                    .filter(|action| action.target == "session")
                    .map(move |action| (id.clone(), action.id.clone(), action.label.clone()))
            })
            .collect();
        if !extension_actions.is_empty() {
            ui.add_space(2.0);
            crate::widgets::hline(ui, p, 0.0);
            ui.add_space(2.0);
            for (extension_id, action_id, label) in extension_actions {
                let response = crate::widgets::menu_item(ui, &label, Some(Icon::Puzzle), None, p);
                if response.clicked() {
                    app.backend.send(Action::ExtensionAction {
                        id: extension_id,
                        action: action_id,
                        item: serde_json::json!({
                            "kind": "session",
                            "id": session.id,
                            "title": session.title(),
                            "directory": session.directory(),
                        }),
                    });
                    app.ui.session_menu = None;
                }
                items.push(response);
            }
        }
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 0.0);
        ui.add_space(2.0);
        let response = crate::widgets::menu_item(ui, "Archive", Some(Icon::Archive), None, p);
        if response.clicked() {
            app.backend.send(Action::ArchiveSession {
                id: session.id.clone(),
                archived: true,
            });
            app.ui.session_menu = None;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item_destructive(ui, "Delete", Some(Icon::Trash), None, true, p);
        if response.clicked() {
            app.ui.confirm_delete = Some(session.id.clone());
            app.ui.session_menu = None;
        }
        items.push(response);
    });
    menu_nav_focus(ui.ctx(), id, open, &items);
    if trigger.clicked() {
        app.ui.session_menu = if menu_open {
            None
        } else {
            Some(session.id.clone())
        };
    } else if !open && app.ui.session_menu.as_deref() == Some(session.id.as_str()) {
        app.ui.session_menu = None;
    }
    trigger.clicked()
}

fn meta_line(app: &RustChamber, session: &Session, show_project: bool) -> String {
    let model = session
        .model
        .as_ref()
        .and_then(|reference| app.model.model_for(reference))
        .map(|model| model.display_name().to_string())
        .unwrap_or_else(|| "No model".into());
    let time = relative_time(session.time.updated);
    let mut meta = if time.is_empty() {
        model
    } else {
        format!("{model} · {time}")
    };
    let scope = if show_project {
        app.model
            .project_for_session(session)
            .map(|project| project.display_name().to_string())
    } else {
        session
            .subpath
            .as_deref()
            .filter(|subpath| !subpath.trim().is_empty())
            .map(str::to_string)
    };
    if let Some(scope) = scope {
        meta.push_str(" · ");
        meta.push_str(&scope);
    }
    meta
}

fn session_recap(session: &Session) -> Option<String> {
    session
        .metadata
        .as_ref()?
        .get("openchamber")?
        .get("assist")?
        .get("recap")?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn session_hover_card(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    elapsed: &str,
    project: &str,
    model: &str,
    preview: &str,
) {
    ui.set_max_width(304.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .font(theme::font_medium(theme::TEXT_UI_LABEL))
                .color(p.fg),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if !elapsed.is_empty() {
                ui.label(
                    RichText::new(elapsed)
                        .font(theme::font(theme::TEXT_MICRO))
                        .color(p.fg_muted),
                );
            }
        });
    });
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        icons::icon(ui, Icon::Folder, 12.0, p.fg_muted);
        ui.label(
            RichText::new(project)
                .font(theme::font(theme::TEXT_MICRO))
                .color(p.fg_muted),
        );
        if !model.is_empty() {
            ui.label(
                RichText::new(format!("· {model}"))
                    .font(theme::font(theme::TEXT_MICRO))
                    .color(p.fg_muted),
            );
        }
    });
    if !preview.is_empty() {
        ui.add_space(5.0);
        ui.label(
            RichText::new(preview)
                .font(theme::font(theme::TEXT_MICRO))
                .color(p.fg_muted),
        );
    }
}

fn tooltip_for(app: &RustChamber, session: &Session, show_project: bool) -> String {
    let model = session
        .model
        .as_ref()
        .and_then(|reference| app.model.model_for(reference))
        .map(|model| model.display_name().to_string())
        .unwrap_or_else(|| "No model".into());
    let mut parts = vec![
        session.title().to_string(),
        format!("{} · {}", model, relative_time(session.time.updated)),
    ];
    if let Some(project) = app.model.project_for_session(session) {
        parts.push(project.display_name().to_string());
    }
    if show_project && let Some(directory) = session.directory() {
        parts.push(directory.to_string());
    }
    if session.cost > 0.0001 {
        parts.push(money(session.cost));
    }
    if session.tokens.total() > 0.0 {
        parts.push(format!("{} tokens", compact_number(session.tokens.total())));
    }
    parts.join("\n")
}

fn bulk_bar(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), BULK_HEIGHT), Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, p.scale_alpha(p.selection, 0.35));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_space(GRID_INSET);
            ui.label(
                RichText::new(format!("{} selected", app.ui.selected_sessions.len()))
                    .font(theme::font_medium(theme::TEXT_UI_LABEL))
                    .color(p.fg),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(GRID_PAD);
                if crate::widgets::ghost_button(ui, "Cancel", None, p).clicked() {
                    app.ui.selected_sessions.clear();
                    app.ui.select_mode = false;
                }
                let enabled = !app.ui.selected_sessions.is_empty();
                let delete = crate::widgets::button_ex(
                    ui,
                    "Delete",
                    Some(Icon::Trash),
                    crate::widgets::ButtonVariant::Destructive,
                    crate::widgets::ButtonSize::Xs,
                    enabled,
                    p,
                );
                if delete.clicked() {
                    app.ui.confirm_delete_bulk = app.ui.selected_sessions.iter().cloned().collect();
                }
                if crate::widgets::button_ex(
                    ui,
                    "Archive",
                    Some(Icon::Archive),
                    crate::widgets::ButtonVariant::Neutral,
                    crate::widgets::ButtonSize::Xs,
                    enabled,
                    p,
                )
                .clicked()
                {
                    let ids: Vec<String> = app.ui.selected_sessions.iter().cloned().collect();
                    app.backend.send(Action::ArchiveSessions {
                        ids,
                        archived: true,
                    });
                    app.ui.selected_sessions.clear();
                    app.ui.select_mode = false;
                    app.model.toast(ToastKind::Success, "Sessions archived");
                }
            });
        },
    );
}

fn footer(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), FOOTER_HEIGHT),
        Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.top() + theme::BORDER * 0.5,
        Stroke::new(theme::BORDER, p.border),
    );
    let response = ui.interact(
        rect,
        egui::Id::unique("sidebar-footer"),
        Sense::hover(),
    );
    let (color, status) = match &app.model.connection {
        Connection::Ready => (p.green, "Local"),
        Connection::Connecting => (p.primary, "Starting"),
        Connection::Reconnecting(_) => (p.coral, "Reconnecting"),
        Connection::Failed(_) => (p.red, "Offline"),
        Connection::Disconnected => (p.fg_faint, "Disconnected"),
    };
    let font = theme::font_medium(theme::TEXT_META);
    let galley = ui
        .painter()
        .layout_no_wrap(status.to_string(), font, p.fg_muted);
    let end = rect.right() - HEADER_PAD_X;
    let pill_width = 8.0 + 14.0 + 6.0 + galley.size().x + 10.0;
    let pill_rect = Rect::from_min_max(
        egui::Pos2::new(end - pill_width, rect.center().y - 12.0),
        egui::Pos2::new(end, rect.center().y + 12.0),
    );
    let pill = ui.interact(
        pill_rect,
        egui::Id::unique("sidebar-instance"),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &pill);
    if pill.hovered() {
        ui.painter().rect_filled(
            pill_rect,
            CornerRadius::same(theme::radius::SM),
            p.hover,
        );
    }
    icons::paint(
        ui,
        Icon::Layers,
        Rect::from_center_size(
            egui::Pos2::new(pill_rect.left() + 8.0 + 7.0, pill_rect.center().y),
            Vec2::splat(14.0),
        ),
        p.fg_muted,
    );
    ui.painter().galley(
        egui::Pos2::new(
            pill_rect.left() + 8.0 + 14.0 + 6.0,
            pill_rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        p.fg_muted,
    );
    let _ = pill.clone().on_hover_text("Current instance");
    let detail = app
        .model
        .server
        .as_ref()
        .map(|server| format!("OpenCode {}", server.version))
        .unwrap_or_else(|| "OpenCode server".into());
    let endpoint = app
        .settings
        .endpoint
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            app.model
                .server
                .as_ref()
                .and_then(|server| server.urls.first().cloned())
        })
        .unwrap_or_else(|| "127.0.0.1".into());
    let _ = response.on_hover_text(format!("{detail}\n{endpoint}"));

    let popup_id = egui::Id::unique("sidebar-instance-popup");
    let mut open = ui
        .data(|data| data.get_temp::<bool>(popup_id))
        .unwrap_or(false);
    if pill.clicked() {
        open = !open;
    }
    let mut add_instance = false;
    crate::app::menu_popup(ui.ctx(), &pill, popup_id, &mut open, p, 8.0, |ui, _| {
        ui.set_width(330.0);
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Current")
                    .font(theme::font(theme::TEXT_META))
                    .color(p.fg_muted),
            );
            ui.label(
                RichText::new("Local")
                    .font(theme::font_semibold(theme::TEXT_UI_LABEL))
                    .color(p.fg),
            );
            ui.label(
                RichText::new("· Default")
                    .font(theme::font(theme::TEXT_META))
                    .color(p.fg_faint),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let refresh = crate::widgets::icon_button_hit(
                    ui,
                    Icon::Refresh,
                    13.0,
                    24.0,
                    p.fg_muted,
                )
                .on_hover_text("Reconnect");
                if refresh.clicked() {
                    app.apply_connection_settings();
                }
            });
        });
        egui::Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(theme::BORDER, p.border))
            .corner_radius(theme::radius::PANEL)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Local")
                            .font(theme::font_semibold(theme::TEXT_UI_LABEL))
                            .color(p.fg),
                    );
                    crate::widgets::tag_chip(ui, "Current", p);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        crate::widgets::icon_button_hit(
                            ui,
                            Icon::Layers,
                            14.0,
                            24.0,
                            p.fg_muted,
                        )
                        .on_hover_text("Local server");
                        crate::widgets::icon_button_hit(
                            ui,
                            Icon::Pin,
                            14.0,
                            24.0,
                            p.primary,
                        )
                        .on_hover_text("Default instance");
                    });
                });
                ui.horizontal(|ui| {
                    ui.painter().circle_filled(
                        ui.cursor().left_center() + Vec2::new(3.0, 0.0),
                        3.0,
                        color,
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(if matches!(app.model.connection, Connection::Ready) {
                            "Connected"
                        } else {
                            status
                        })
                        .font(theme::font(theme::TEXT_META))
                        .color(if matches!(app.model.connection, Connection::Ready) {
                            p.green
                        } else {
                            p.fg_muted
                        }),
                    );
                });
                ui.label(
                    RichText::new(&endpoint)
                        .font(theme::font_mono(11.5))
                        .color(p.fg_muted),
                );
            });
        let (row, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 30.0),
            Sense::click(),
        );
        crate::widgets::pointer(ui, &response);
        if response.hovered() {
            ui.painter().rect_filled(
                row,
                CornerRadius::same(theme::radius::SM),
                p.hover,
            );
        }
        icons::paint(
            ui,
            Icon::Plus,
            Rect::from_center_size(
                egui::Pos2::new(row.left() + 12.0, row.center().y),
                Vec2::splat(14.0),
            ),
            p.fg_muted,
        );
        ui.painter().text(
            egui::Pos2::new(row.left() + 28.0, row.center().y),
            egui::Align2::LEFT_CENTER,
            "Add instance",
            theme::font(theme::TEXT_UI_LABEL),
            p.fg,
        );
        let chip = egui::Rect::from_min_size(
            egui::Pos2::new(row.right() - 92.0, row.center().y - 9.0),
            Vec2::new(80.0, 18.0),
        );
        ui.painter().rect_filled(
            chip,
            CornerRadius::same(theme::radius::XS),
            p.tag_bg,
        );
        ui.painter().text(
            chip.center(),
            egui::Align2::CENTER_CENTER,
            "Coming soon",
            theme::font_mono(10.0),
            p.fg_faint,
        );
        if response.on_hover_text("Remote instances are not available yet").clicked() {
            add_instance = true;
        }
    });
    if add_instance {
        app.open_settings();
        crate::settings::open_page(ui.ctx(), 0);
        open = false;
    }
    ui.data_mut(|data| data.insert_temp(popup_id, open));
}


fn grouping_label(grouping: SessionGrouping) -> &'static str {
    match grouping {
        SessionGrouping::Project => "By project",
        SessionGrouping::Time => "Timeline",
    }
}

fn sort_label(sort: ProjectSort) -> &'static str {
    match sort {
        ProjectSort::Manual => "Manual",
        ProjectSort::Recent => "Recent",
        ProjectSort::Name => "Name",
    }
}

fn toggle_section(app: &mut RustChamber, key: &str) {
    let store = &mut app.model.workspace_store;
    if !store.collapsed_sections.remove(key) {
        store.collapsed_sections.insert(key.to_string());
    }
    store.save();
}

fn set_project_collapsed(app: &mut RustChamber, id: &str, collapsed: bool) {
    let store = &mut app.model.workspace_store;
    if collapsed {
        store.collapsed_projects.insert(id.to_string());
        app.ui.show_all_projects.remove(id);
    } else {
        store.collapsed_projects.remove(id);
    }
    store.save();
}

fn set_all_projects_collapsed(app: &mut RustChamber, projects: &[ProjectEntry], collapsed: bool) {
    let store = &mut app.model.workspace_store;
    for project in projects {
        if collapsed {
            store.collapsed_projects.insert(project.id.clone());
        } else {
            store.collapsed_projects.remove(&project.id);
        }
    }
    if collapsed {
        app.ui.show_all_projects.clear();
    }
    store.save();
}

fn collapse_all(app: &mut RustChamber) {
    let ids: Vec<String> = app
        .model
        .workspace_store
        .projects
        .iter()
        .map(|project| project.id.clone())
        .collect();
    let store = &mut app.model.workspace_store;
    store.collapsed_projects.extend(ids);
    for folder in &mut store.folders {
        folder.collapsed = true;
    }
    store.collapsed_sections.insert("in_progress".into());
    store.collapsed_sections.insert("chats".into());
    store.save();
    app.ui.show_all_projects.clear();
}

fn expand_all(app: &mut RustChamber) {
    let store = &mut app.model.workspace_store;
    store.collapsed_projects.clear();
    for folder in &mut store.folders {
        folder.collapsed = false;
    }
    store.collapsed_sections.remove("in_progress");
    store.collapsed_sections.remove("chats");
    store.save();
}

fn mark_all_read(app: &mut RustChamber) {
    let ids: Vec<String> = app
        .model
        .sessions
        .iter()
        .filter(|session| session.time.archived.is_none() && app.model.is_unread(session))
        .map(|session| session.id.clone())
        .collect();
    if ids.is_empty() {
        app.model
            .toast(ToastKind::Info, "Everything is already read");
        return;
    }
    for id in &ids {
        let idle = app
            .model
            .sessions
            .iter()
            .find(|session| &session.id == id)
            .and_then(|session| session.time.idle)
            .unwrap_or(0.0);
        app.model.mark_viewed_local(id, idle);
    }
    app.backend.send(Action::MarkAllSessionsViewed(ids.clone()));
    app.model.toast(
        ToastKind::Success,
        format!("Marked {} sessions as read", ids.len()),
    );
}

fn close_project(app: &mut RustChamber, id: &str) {
    let was_active = app.model.workspace_store.active_project_id.as_deref() == Some(id);
    app.model.remove_project(id);
    app.sync_tracked_projects();
    if was_active {
        let next = app
            .model
            .workspace_projects()
            .first()
            .map(|project| project.id.clone());
        if let Some(next) = next {
            app.select_project(&next);
        } else {
            app.model.workspace_store.active_project_id = None;
            app.model.workspace_store.save();
        }
    }
    app.model
        .toast(ToastKind::Success, "Project closed. Sessions were kept.");
}

fn copy_to_clipboard(ctx: &egui::Context, text: &str) {
    if arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_string()))
        .is_err()
    {
        ctx.copy_text(text.to_string());
    }
}

fn move_session(app: &mut RustChamber, session: &Session) {
    let mut dialog = rfd::FileDialog::new();
    if let Some(directory) = session.directory() {
        dialog = dialog.set_directory(directory);
    }
    let Some(folder) = dialog.pick_folder() else {
        return;
    };
    let directory = folder.to_string_lossy().to_string();
    app.backend.send(Action::MoveSession {
        id: session.id.clone(),
        directory: directory.clone(),
    });
    app.model
        .toast(ToastKind::Info, format!("Moving session to {directory}"));
}

pub(crate) fn export_markdown(app: &mut RustChamber, session: &Session) {
    let markdown = match app
        .model
        .active
        .as_ref()
        .filter(|chat| chat.session.id == session.id)
    {
        Some(chat) => session_markdown(session, &chat.messages),
        None => {
            app.model
                .toast(ToastKind::Info, "Open the session to export");
            return;
        }
    };
    let name = file_name(session.title());
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(format!("{name}.md"))
        .save_file()
    else {
        return;
    };
    match std::fs::write(&path, markdown) {
        Ok(()) => app.model.toast(
            ToastKind::Success,
            format!("Exported to {}", path.display()),
        ),
        Err(error) => app
            .model
            .toast(ToastKind::Error, format!("Could not export: {error}")),
    }
}

fn file_name(title: &str) -> String {
    let name: String = title
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = name.trim_matches('-');
    if trimmed.is_empty() {
        "session".into()
    } else {
        trimmed.chars().take(60).collect()
    }
}

fn session_markdown(session: &Session, messages: &[Message]) -> String {
    let mut out = format!("# {}\n\n", session.title());
    out.push_str(&format!("- Session: `{}`\n", session.id));
    if let Some(directory) = session.directory() {
        out.push_str(&format!("- Directory: `{directory}`\n"));
    }
    let created = timestamp_label(session.time.created);
    if !created.is_empty() {
        out.push_str(&format!("- Created: {created}\n"));
    }
    out.push_str("\n---\n\n");
    for message in messages {
        match message {
            Message::User(message) if !message.text.trim().is_empty() => {
                out.push_str("## User\n\n");
                out.push_str(message.text.trim());
                out.push_str("\n\n");
            }
            Message::Assistant(message) => {
                let text = message
                    .content
                    .iter()
                    .filter_map(|part| match part {
                        Part::Text(text) => Some(text.text.trim()),
                        _ => None,
                    })
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if !text.is_empty() {
                    out.push_str("## Assistant\n\n");
                    out.push_str(&text);
                    out.push_str("\n\n");
                }
            }
            Message::System(message) if !message.text.trim().is_empty() => {
                out.push_str("## System\n\n");
                out.push_str(message.text.trim());
                out.push_str("\n\n");
            }
            Message::Compaction(message) => {
                if let Some(text) = message
                    .summary
                    .as_deref()
                    .or(message.text.as_deref())
                    .filter(|text| !text.trim().is_empty())
                {
                    out.push_str("## Compaction\n\n");
                    out.push_str(text.trim());
                    out.push_str("\n\n");
                }
            }
            _ => {}
        }
    }
    out
}

fn timestamp_label(ms: f64) -> String {
    if ms <= 0.0 {
        return String::new();
    }
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms as i64)
        .map(|datetime| datetime.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

fn project_color(project: &ProjectEntry, p: &Palette) -> egui::Color32 {
    let tone = project_hash(&project.id) % PROJECT_HASH_TONES;
    let base = match tone {
        0 => p.primary,
        1 => p.primary_text,
        _ => p.mix(p.fg_muted, p.fg, if p.dark { 0.2 } else { 0.1 }),
    };
    p.mix(base, p.fg, if p.dark { 0.0 } else { 0.08 })
}

fn project_hash(id: &str) -> usize {
    id.bytes().fold(0usize, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

fn dialogs(ctx: &egui::Context, app: &mut RustChamber) {
    customize_dialog(ctx, app);
    archive_dialog(ctx, app);
    project_rename_dialog(ctx, app);
    new_folder_dialog(ctx, app);
    folder_rename_dialog(ctx, app);
    delete_folder_dialog(ctx, app);
    move_to_folder_dialog(ctx, app);
    delete_dialog(ctx, app);
    delete_bulk_dialog(ctx, app);
    archive_all_dialog(ctx, app);
    close_project_dialog(ctx, app);
}

fn modal_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.bg)
        .corner_radius(theme::radius::XL)
        .inner_margin(egui::Margin::same(20))
}

fn customize_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    if !app.ui.customize_sections {
        return;
    }
    let p = app.palette.clone();
    let mut open = true;
    let modal = egui::Modal::new(egui::Id::unique("customize-sections-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(
                RichText::new("Customize sections")
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Choose which built-in sections appear in the sidebar.")
                    .font(theme::font(META_SIZE))
                    .color(p.fg_muted),
            );
            ui.add_space(10.0);
            let in_progress = !app
                .model
                .workspace_store
                .hidden_sections
                .iter()
                .any(|section| section == "in_progress")
                && app.model.workspace_store.display.show_in_progress;
            let mut in_progress_now = in_progress;
            ui.horizontal(|ui| {
                crate::widgets::checkbox(ui, &mut in_progress_now, &p);
                ui.label(
                    RichText::new("In progress")
                        .font(theme::font(theme::TEXT_UI_LABEL))
                        .color(p.fg),
                );
            });
            if in_progress_now != in_progress {
                update_section_visibility(app, "in_progress", in_progress_now);
            }
            let chats = !app
                .model
                .workspace_store
                .hidden_sections
                .iter()
                .any(|section| section == "chats")
                && app.model.workspace_store.display.show_chats;
            let mut chats_now = chats;
            ui.horizontal(|ui| {
                crate::widgets::checkbox(ui, &mut chats_now, &p);
                ui.label(
                    RichText::new("Chats")
                        .font(theme::font(theme::TEXT_UI_LABEL))
                        .color(p.fg),
                );
            });
            if chats_now != chats {
                update_section_visibility(app, "chats", chats_now);
            }
            let mut projects_now = true;
            ui.horizontal(|ui| {
                crate::widgets::checkbox_ex(ui, &mut projects_now, false, false, &p);
                ui.label(
                    RichText::new("Projects")
                        .font(theme::font(theme::TEXT_UI_LABEL))
                        .color(p.fg_muted),
                );
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, "Done", Some(Icon::Check), &p).clicked() {
                    open = false;
                }
            });
        });
    if modal.backdrop_response.clicked() {
        open = false;
    }
    if !open {
        app.ui.customize_sections = false;
    }
}

fn update_section_visibility(app: &mut RustChamber, key: &str, visible: bool) {
    let store = &mut app.model.workspace_store;
    match key {
        "in_progress" => store.display.show_in_progress = visible,
        "chats" => store.display.show_chats = visible,
        _ => {}
    }
    if visible {
        store.hidden_sections.retain(|section| section != key);
    } else if !store.hidden_sections.iter().any(|section| section == key) {
        store.hidden_sections.push(key.to_string());
    }
    store.save();
}

fn archive_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    if !app.ui.archive_open {
        return;
    }
    let p = app.palette.clone();
    let mut archived: Vec<Session> = app.model.archived_sessions().into_iter().cloned().collect();
    archived.sort_by(|left, right| {
        right
            .time
            .updated
            .partial_cmp(&left.time.updated)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut open = true;
    let mut restore: Option<String> = None;
    let mut delete: Option<String> = None;
    let modal = egui::Modal::new(egui::Id::unique("archive-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(470.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Archive")
                        .font(theme::font_semibold(15.0))
                        .color(p.fg),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if crate::widgets::ghost_button(ui, "Close", None, &p).clicked() {
                        open = false;
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Archived sessions stay on disk and can be restored.")
                    .font(theme::font(META_SIZE))
                    .color(p.fg_muted),
            );
            ui.add_space(8.0);
            if archived.is_empty() {
                ui.add_space(18.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("No archived sessions")
                            .font(theme::font_medium(13.0))
                            .color(p.fg_muted),
                    );
                });
                ui.add_space(18.0);
            } else {
                ScrollArea::vertical()
                    .id_salt("archive-list")
                    .max_height(380.0)
                    .show(ui, |ui| {
                        for session in &archived {
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 44.0),
                                Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(theme::radius::MD),
                                p.scale_alpha(p.hover, 0.35),
                            );
                            let text_left = rect.left() + 8.0;
                            let text_room = (rect.width() - 74.0).max(60.0);
                            let title = WidgetText::from(
                                RichText::new(session.title())
                                    .font(theme::font_medium(TITLE_SIZE))
                                    .color(p.fg),
                            )
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                text_room,
                                TextStyle::Body,
                            );
                            ui.painter().galley(
                                egui::Pos2::new(text_left, rect.top() + 7.0),
                                title,
                                p.fg,
                            );
                            let meta = meta_line(app, session, true);
                            let meta_galley = WidgetText::from(
                                RichText::new(meta)
                                    .font(theme::font(META_SIZE))
                                    .color(p.fg_muted),
                            )
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                text_room,
                                TextStyle::Body,
                            );
                            ui.painter().galley(
                                egui::Pos2::new(text_left, rect.top() + 25.0),
                                meta_galley,
                                p.fg_muted,
                            );
                            let delete_rect = Rect::from_center_size(
                                egui::Pos2::new(rect.right() - 18.0, rect.center().y),
                                Vec2::splat(24.0),
                            );
                            let restore_rect = Rect::from_center_size(
                                egui::Pos2::new(rect.right() - 46.0, rect.center().y),
                                Vec2::splat(24.0),
                            );
                            let delete_response = ui.interact(
                                delete_rect,
                                egui::Id::unique(("archive-delete", &session.id)),
                                Sense::click(),
                            );
                            let restore_response = ui.interact(
                                restore_rect,
                                egui::Id::unique(("archive-restore", &session.id)),
                                Sense::click(),
                            );
                            crate::widgets::pointer(ui, &delete_response);
                            crate::widgets::pointer(ui, &restore_response);
                            icons::paint(
                                ui,
                                Icon::Trash,
                                Rect::from_center_size(delete_rect.center(), Vec2::splat(14.0)),
                                if delete_response.hovered() {
                                    p.red
                                } else {
                                    p.fg_muted
                                },
                            );
                            icons::paint(
                                ui,
                                Icon::Upload,
                                Rect::from_center_size(restore_rect.center(), Vec2::splat(14.0)),
                                if restore_response.hovered() {
                                    p.fg
                                } else {
                                    p.fg_muted
                                },
                            );
                            if delete_response.clicked() {
                                delete = Some(session.id.clone());
                            }
                            if restore_response.clicked() {
                                restore = Some(session.id.clone());
                            }
                            ui.add_space(2.0);
                        }
                    });
            }
        });
    if modal.backdrop_response.clicked() {
        open = false;
    }
    if !open {
        app.ui.archive_open = false;
    }
    if let Some(id) = restore {
        app.backend.send(Action::ArchiveSession {
            id,
            archived: false,
        });
        app.model.toast(ToastKind::Success, "Session restored");
    }
    if let Some(id) = delete {
        app.ui.archive_open = false;
        app.ui.confirm_delete = Some(id);
    }
}

fn project_rename_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    let Some((id, mut name)) = app.ui.project_rename.take() else {
        return;
    };
    let p = app.palette.clone();
    let mut save = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::unique("project-rename-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(
                RichText::new("Edit project")
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(8.0);
            let field_id = egui::Id::unique("project-rename-field");
            let response = ui.add(
                egui::TextEdit::singleline(&mut name)
                    .id(field_id)
                    .font(theme::font(13.0))
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::symmetric(8, 7)),
            );
            if !response.has_focus() {
                response.request_focus();
            }
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                save = true;
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, "Save", Some(Icon::Check), &p).clicked() {
                    save = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    cancel = true;
                }
            });
        });
    if modal.backdrop_response.clicked() || cancel {
        return;
    }
    if save && !name.trim().is_empty() {
        app.model.rename_project(&id, name.trim());
    } else {
        app.ui.project_rename = Some((id, name));
    }
}

fn new_folder_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    let Some(project_id) = app.ui.new_folder.clone() else {
        return;
    };
    let p = app.palette.clone();
    let scope = if project_id == crate::app::CHATS_TARGET {
        "Chats".to_string()
    } else {
        app.model
            .project_by_id(&project_id)
            .map(|project| project.display_name().to_string())
            .unwrap_or_else(|| "this project".into())
    };
    let mut name = app.ui.new_folder_name.clone();
    let mut create = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::unique("new-folder-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(
                RichText::new("New folder")
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("Create a folder in {scope}."))
                    .font(theme::font(META_SIZE))
                    .color(p.fg_muted),
            );
            ui.add_space(8.0);
            let field_id = egui::Id::unique("new-folder-field");
            let response = ui.add(
                egui::TextEdit::singleline(&mut name)
                    .id(field_id)
                    .font(theme::font(13.0))
                    .hint_text("Folder name")
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::symmetric(8, 7)),
            );
            if !response.has_focus() {
                response.request_focus();
            }
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                create = true;
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, "Create", Some(Icon::Check), &p).clicked() {
                    create = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    cancel = true;
                }
            });
        });
    if modal.backdrop_response.clicked() {
        cancel = true;
    }
    if create && !name.trim().is_empty() {
        app.model.create_folder(&project_id, name.trim());
        app.ui.new_folder = None;
        app.ui.new_folder_name.clear();
    } else if cancel {
        app.ui.new_folder = None;
        app.ui.new_folder_name.clear();
    } else {
        app.ui.new_folder_name = name;
    }
}

fn folder_rename_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    let Some((id, mut name)) = app.ui.folder_rename.take() else {
        return;
    };
    let p = app.palette.clone();
    let mut save = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::unique("folder-rename-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(
                RichText::new("Rename folder")
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(8.0);
            let field_id = egui::Id::unique("folder-rename-field");
            let response = ui.add(
                egui::TextEdit::singleline(&mut name)
                    .id(field_id)
                    .font(theme::font(13.0))
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::symmetric(8, 7)),
            );
            if !response.has_focus() {
                response.request_focus();
            }
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                save = true;
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, "Save", Some(Icon::Check), &p).clicked() {
                    save = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    cancel = true;
                }
            });
        });
    if modal.backdrop_response.clicked() || cancel {
        return;
    }
    if save && !name.trim().is_empty() {
        app.model.rename_folder(&id, name.trim());
    } else {
        app.ui.folder_rename = Some((id, name));
    }
}

fn delete_folder_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    let Some(id) = app.ui.confirm_delete_folder.clone() else {
        return;
    };
    let Some(folder) = app
        .model
        .workspace_store
        .folders
        .iter()
        .find(|folder| folder.id == id)
        .cloned()
    else {
        app.ui.confirm_delete_folder = None;
        return;
    };
    let p = app.palette.clone();
    let count = app.model.workspace_store.sessions_in_folder(&id).len();
    let mut open = true;
    let mut confirm = false;
    let modal = egui::Modal::new(egui::Id::unique("delete-folder-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(
                RichText::new("Delete folder?")
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(if count == 1 {
                    format!(
                        "“{}” will be removed. Its session stays in the project.",
                        folder.name
                    )
                } else {
                    format!(
                        "“{}” will be removed. Its {} sessions stay in the project.",
                        folder.name, count
                    )
                })
                .font(theme::font(META_SIZE))
                .color(p.fg_muted),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if crate::widgets::destructive_button(ui, "Delete", Some(Icon::Trash), &p).clicked()
                {
                    confirm = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    open = false;
                }
            });
        });
    if modal.backdrop_response.clicked() {
        open = false;
    }
    if confirm {
        app.model.delete_folder(&id);
        app.ui.confirm_delete_folder = None;
        app.model
            .toast(ToastKind::Success, "Folder deleted. Sessions were kept.");
    } else if !open {
        app.ui.confirm_delete_folder = None;
    }
}

fn move_to_folder_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    let Some(session_id) = app.ui.move_to_folder.clone() else {
        return;
    };
    let Some(session) = app
        .model
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .cloned()
    else {
        app.ui.move_to_folder = None;
        return;
    };
    if session.parent_id.is_some() {
        app.ui.move_to_folder = None;
        return;
    }
    let scope = app
        .model
        .project_for_session(&session)
        .map(|project| project.id.clone())
        .unwrap_or_else(|| crate::app::CHATS_TARGET.to_string());
    let scope_label = if scope == crate::app::CHATS_TARGET {
        "Chats".to_string()
    } else {
        app.model
            .project_by_id(&scope)
            .map(|project| project.display_name().to_string())
            .unwrap_or_else(|| "this project".into())
    };
    let current = app
        .model
        .workspace_store
        .folder_for_session(&session_id)
        .map(|folder| folder.id.clone());
    let folders: Vec<SessionFolder> = app
        .model
        .workspace_store
        .folders_for_project(&scope)
        .into_iter()
        .cloned()
        .collect();
    let p = app.palette.clone();
    let mut new_name = app.ui.move_folder_name.clone();
    let mut target: Option<Option<String>> = None;
    let mut create = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::unique("move-to-folder-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(400.0);
            ui.label(
                RichText::new("Move to folder")
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(4.0);
            let subtitle = format!("{} · {scope_label}", session.title());
            ui.label(
                RichText::new(subtitle)
                    .font(theme::font(META_SIZE))
                    .color(p.fg_muted),
            );
            ui.add_space(8.0);
            ScrollArea::vertical()
                .id_salt("move-folder-list")
                .max_height(240.0)
                .show(ui, |ui| {
                    let response = crate::widgets::menu_item_ex(
                        ui,
                        "No folder",
                        Some(Icon::FolderOpen),
                        None,
                        current.is_none(),
                        true,
                        &p,
                    );
                    if response.clicked() {
                        target = Some(None);
                    }
                    for folder in &folders {
                        let response = crate::widgets::menu_item_ex(
                            ui,
                            &folder.name,
                            Some(if folder.collapsed {
                                Icon::Folder
                            } else {
                                Icon::FolderOpen
                            }),
                            None,
                            current.as_deref() == Some(folder.id.as_str()),
                            true,
                            &p,
                        );
                        if response.clicked() {
                            target = Some(Some(folder.id.clone()));
                        }
                    }
                });
            ui.add_space(6.0);
            crate::widgets::hline(ui, &p, 0.0);
            ui.add_space(6.0);
            ui.label(
                RichText::new("New folder")
                    .font(theme::font(theme::TEXT_MICRO))
                    .color(p.fg_muted),
            );
            ui.add_space(4.0);
            let field_id = egui::Id::unique("move-folder-field");
            let response = ui.add(
                egui::TextEdit::singleline(&mut new_name)
                    .id(field_id)
                    .font(theme::font(13.0))
                    .hint_text("Folder name")
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::symmetric(8, 7)),
            );
            if !response.has_focus() && response.hovered() {
                response.request_focus();
            }
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                create = true;
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, "Create & move", Some(Icon::Move), &p)
                    .clicked()
                {
                    create = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    cancel = true;
                }
            });
        });
    if modal.backdrop_response.clicked() {
        cancel = true;
    }
    if create && !new_name.trim().is_empty() {
        let folder = app.model.create_folder(&scope, new_name.trim());
        app.model.assign_session_to_folder(&folder, &session_id);
        app.model
            .toast(ToastKind::Success, format!("Moved to {}", new_name.trim()));
        app.ui.move_to_folder = None;
        app.ui.move_folder_name.clear();
    } else if let Some(target) = target {
        match target {
            Some(folder) => {
                app.model.assign_session_to_folder(&folder, &session_id);
                app.model.toast(ToastKind::Success, "Session moved");
            }
            None => {
                app.model.unassign_session_from_folder(&session_id);
                app.model
                    .toast(ToastKind::Success, "Session removed from folder");
            }
        }
        app.ui.move_to_folder = None;
        app.ui.move_folder_name.clear();
    } else if cancel {
        app.ui.move_to_folder = None;
        app.ui.move_folder_name.clear();
    } else {
        app.ui.move_folder_name = new_name;
    }
}

fn delete_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    let Some(id) = app.ui.confirm_delete.clone() else {
        return;
    };
    let p = app.palette.clone();
    let title = app
        .model
        .sessions
        .iter()
        .find(|session| session.id == id)
        .map(|session| session.title().to_string())
        .unwrap_or_else(|| "this session".into());
    let mut open = true;
    let mut confirm = false;
    let modal = egui::Modal::new(egui::Id::unique("delete-session-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(
                RichText::new("Delete session?")
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "“{title}” will be permanently deleted. This cannot be undone."
                ))
                .font(theme::font(META_SIZE))
                .color(p.fg_muted),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if crate::widgets::destructive_button(ui, "Delete", Some(Icon::Trash), &p).clicked()
                {
                    confirm = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    open = false;
                }
            });
        });
    if modal.backdrop_response.clicked() {
        open = false;
    }
    if confirm {
        app.model
            .forget_sessions_from_folders(std::slice::from_ref(&id));
        app.ui.selected_sessions.remove(&id);
        app.backend.send(Action::DeleteSession(id));
        app.ui.confirm_delete = None;
        app.model.toast(ToastKind::Success, "Session deleted");
    } else if !open {
        app.ui.confirm_delete = None;
    }
}

fn delete_bulk_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    if app.ui.confirm_delete_bulk.is_empty() {
        return;
    }
    let p = app.palette.clone();
    let ids = app.ui.confirm_delete_bulk.clone();
    let mut open = true;
    let mut confirm = false;
    let modal = egui::Modal::new(egui::Id::unique("delete-sessions-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(
                RichText::new(format!("Delete {} sessions?", ids.len()))
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("This cannot be undone.")
                    .font(theme::font(META_SIZE))
                    .color(p.fg_muted),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if crate::widgets::destructive_button(ui, "Delete all", Some(Icon::Trash), &p)
                    .clicked()
                {
                    confirm = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    open = false;
                }
            });
        });
    if modal.backdrop_response.clicked() {
        open = false;
    }
    if confirm {
        app.model.forget_sessions_from_folders(&ids);
        app.backend.send(Action::DeleteSessions(ids));
        app.ui.confirm_delete_bulk.clear();
        app.ui.selected_sessions.clear();
        app.ui.select_mode = false;
        app.model.toast(ToastKind::Success, "Sessions deleted");
    } else if !open {
        app.ui.confirm_delete_bulk.clear();
    }
}

fn archive_all_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    let Some(id) = app.ui.confirm_archive_all.clone() else {
        return;
    };
    let Some(project) = app.model.project_by_id(&id).cloned() else {
        app.ui.confirm_archive_all = None;
        return;
    };
    let p = app.palette.clone();
    let ids: Vec<String> = app
        .model
        .sessions_for_project(&project)
        .into_iter()
        .filter(|session| session.time.archived.is_none())
        .map(|session| session.id.clone())
        .collect();
    let mut open = true;
    let mut confirm = false;
    let modal = egui::Modal::new(egui::Id::unique("archive-project-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(
                RichText::new(format!(
                    "Archive all sessions in {}?",
                    project.display_name()
                ))
                .font(theme::font_semibold(15.0))
                .color(p.fg),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "{} active sessions will be archived. They stay on disk and can be restored.",
                    ids.len()
                ))
                .font(theme::font(META_SIZE))
                .color(p.fg_muted),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, "Archive", Some(Icon::Archive), &p).clicked()
                {
                    confirm = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    open = false;
                }
            });
        });
    if modal.backdrop_response.clicked() {
        open = false;
    }
    if confirm {
        app.backend.send(Action::ArchiveSessions {
            ids,
            archived: true,
        });
        app.model.toast(ToastKind::Success, "Sessions archived");
        app.ui.confirm_archive_all = None;
    } else if !open {
        app.ui.confirm_archive_all = None;
    }
}

fn close_project_dialog(ctx: &egui::Context, app: &mut RustChamber) {
    let Some(id) = app.ui.confirm_close_project.clone() else {
        return;
    };
    let Some(project) = app.model.project_by_id(&id).cloned() else {
        app.ui.confirm_close_project = None;
        return;
    };
    let p = app.palette.clone();
    let count = app.model.sessions_for_project(&project).len();
    let mut open = true;
    let mut confirm = false;
    let modal = egui::Modal::new(egui::Id::unique("close-project-modal"))
        .frame(modal_frame(&p))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(
                RichText::new(format!("Close {}?", project.display_name()))
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "The project is removed from the sidebar. Its {count} sessions stay on disk."
                ))
                .font(theme::font(META_SIZE))
                .color(p.fg_muted),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if crate::widgets::destructive_button(ui, "Close project", Some(Icon::Close), &p)
                    .clicked()
                {
                    confirm = true;
                }
                if crate::widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                    open = false;
                }
            });
        });
    if modal.backdrop_response.clicked() {
        open = false;
    }
    if confirm {
        close_project(app, &id);
        app.ui.confirm_close_project = None;
    } else if !open {
        app.ui.confirm_close_project = None;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    #[test]
    fn elapsed_labels_stay_compact() {
        assert_eq!(format_elapsed(21_400.0), "21s");
        assert_eq!(format_elapsed(83_000.0), "1m 23s");
        assert_eq!(format_elapsed(3_720_000.0), "1h 2m");
    }

    #[test]
    fn disclosure_reserves_the_revealed_height() {
        egui::__run_test_ui(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let top = ui.cursor().min.y;
            disclosure(ui, 120.0, 0.5, |ui| {
                ui.allocate_space(Vec2::new(10.0, 120.0));
            });
            assert!((ui.cursor().min.y - top - 60.0).abs() < 0.01);
        });
    }

    #[test]
    fn collapsed_disclosure_reserves_nothing() {
        egui::__run_test_ui(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let top = ui.cursor().min.y;
            disclosure(ui, 120.0, 0.0, |ui| {
                ui.allocate_space(Vec2::new(10.0, 120.0));
            });
            assert!((ui.cursor().min.y - top).abs() < 0.01);
        });
    }

    #[test]
    fn section_headers_use_the_reference_band() {
        let p = Palette::light();
        egui::__run_test_ui(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let top = ui.cursor().min.y;
            let (toggle, add, new_folder) =
                section_header(ui, &p, "chats", "Chats", false, true, true);
            assert!(!toggle);
            assert!(!add);
            assert!(!new_folder);
            assert!((ui.cursor().min.y - top - SECTION_HEIGHT).abs() < 0.01);
        });
    }

    #[test]
    fn disclosure_motion_is_the_row_disclosure_duration() {
        assert!((DISCLOSURE_SECONDS - 0.2).abs() < 1e-6);
    }

    fn row_click_at(target: egui::Pos2) -> (bool, bool) {
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(400.0, 300.0));
        let card = Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(310.0, 50.0));
        let trigger = Rect::from_center_size(
            egui::pos2(
                card.right() - MENU_TRIGGER_INSET - MENU_TRIGGER * 0.5,
                card.center().y,
            ),
            Vec2::splat(MENU_TRIGGER),
        );
        let mut clicked = (false, false);
        let frames = [
            vec![egui::Event::PointerMoved(target)],
            vec![egui::Event::PointerMoved(target)],
            vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }],
            vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        ];
        for events in frames {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let (row, trigger_response) = row_interactions(ui, "ses_click", card, trigger);
                clicked.0 |= row.clicked();
                clicked.1 |= trigger_response.clicked();
            })
            .drop_without_applying_deltas();
        }
        clicked
    }

    #[test]
    fn a_first_click_on_the_trigger_opens_only_the_menu() {
        let card = Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(310.0, 50.0));
        let trigger = Rect::from_center_size(
            egui::pos2(
                card.right() - MENU_TRIGGER_INSET - MENU_TRIGGER * 0.5,
                card.center().y,
            ),
            Vec2::splat(MENU_TRIGGER),
        );
        let (row, menu) = row_click_at(trigger.center());
        assert!(menu, "the three-dot trigger takes the click");
        assert!(!row, "the row must not open on the same click");
    }

    #[test]
    fn a_click_away_from_the_trigger_opens_the_row() {
        let (row, menu) = row_click_at(egui::pos2(70.0, 35.0));
        assert!(row);
        assert!(!menu);
    }

    #[test]
    fn the_trigger_stays_hovered_while_the_pointer_rests_on_it() {
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(400.0, 300.0));
        let card = Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(310.0, 50.0));
        let trigger = Rect::from_center_size(
            egui::pos2(
                card.right() - MENU_TRIGGER_INSET - MENU_TRIGGER * 0.5,
                card.center().y,
            ),
            Vec2::splat(MENU_TRIGGER),
        );
        let mut hovered = 0;
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerMoved(trigger.center())],
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let (row, trigger_response) = row_interactions(ui, "ses_hover", card, trigger);
                if trigger_response.hovered() && !row.hovered() {
                    hovered += 1;
                }
            })
            .drop_without_applying_deltas();
        }
        assert!(
            hovered >= 3,
            "the trigger must stay hovered while the row is not"
        );
    }

    #[test]
    fn the_trigger_stays_visible_without_row_hover() {
        assert!(menu_shown(false, true, false));
        assert!(menu_shown(false, false, true));
        assert!(menu_shown(true, false, false));
        assert!(!menu_shown(false, false, false));
    }

    #[test]
    fn the_trigger_sits_inside_the_row_card() {
        let card = Rect::from_min_max(egui::pos2(6.0, 0.0), egui::pos2(315.0, 30.0));
        let trigger = Rect::from_center_size(
            egui::pos2(
                card.right() - MENU_TRIGGER_INSET - MENU_TRIGGER * 0.5,
                card.center().y,
            ),
            Vec2::splat(MENU_TRIGGER),
        );
        assert!(card.contains_rect(trigger));
        assert!(trigger.right() <= card.right() - MENU_TRIGGER_INSET + 0.01);
    }

    fn mk_session(id: &str, parent: Option<&str>, busy: bool) -> Session {
        Session {
            id: id.to_string(),
            parent_id: parent.map(str::to_string),
            project_id: "project".to_string(),
            slug: None,
            agent: None,
            model: None,
            cost: 0.0,
            tokens: rchamber_api::models::TokenUsage::default(),
            outcome: None,
            time: rchamber_api::models::SessionTime {
                busy: busy.then_some(2.0),
                idle: Some(1.0),
                ..Default::default()
            },
            title: None,
            subpath: None,
            metadata: None,
            permissions: None,
            revert: None,
            location: None,
            fork: None,
        }
    }

    fn mk_folder(id: &str, name: &str, collapsed: bool) -> SessionFolder {
        SessionFolder {
            id: id.into(),
            project_id: "project".into(),
            name: name.into(),
            collapsed,
        }
    }

    #[test]
    fn folders_partition_roots_and_keep_loose_sessions() {
        let folders = vec![
            mk_folder("folder:1", "First", false),
            mk_folder("folder:2", "Second", false),
        ];
        let assignments = HashMap::from([
            ("folder:1".to_string(), vec!["a".to_string()]),
            (
                "folder:2".to_string(),
                vec!["c".to_string(), "child".to_string()],
            ),
        ]);
        let rows = vec![
            mk_session("a", None, false),
            mk_session("b", None, false),
            mk_session("c", None, false),
        ];

        let layout = partition_folders(&folders, &assignments, "project", &rows);

        assert_eq!(layout.folders.len(), 2);
        assert_eq!(layout.folders[0].sessions[0].id, "a");
        assert_eq!(layout.folders[1].sessions[0].id, "c");
        assert_eq!(layout.loose.len(), 1);
        assert_eq!(layout.loose[0].id, "b");
    }

    #[test]
    fn folders_from_other_projects_do_not_hide_sessions() {
        let mut folder = mk_folder("folder:1", "First", false);
        folder.project_id = "other".into();
        let assignments = HashMap::from([("folder:1".to_string(), vec!["a".to_string()])]);
        let rows = vec![mk_session("a", None, false)];

        let layout = partition_folders(&[folder], &assignments, "project", &rows);

        assert!(layout.folders.is_empty());
        assert_eq!(layout.loose.len(), 1);
        assert_eq!(layout.loose[0].id, "a");
    }

    #[test]
    fn folder_heights_include_headers_empty_rows_and_loose_sessions() {
        let folders = [
            mk_folder("folder:1", "Empty", false),
            mk_folder("folder:2", "Collapsed", true),
        ];
        let all = vec![mk_session("a", None, false), mk_session("b", None, false)];
        let layout = FolderPartition {
            folders: vec![
                FolderGroup {
                    folder: folders[0].clone(),
                    sessions: Vec::new(),
                },
                FolderGroup {
                    folder: folders[1].clone(),
                    sessions: vec![all[0].clone()],
                },
            ],
            loose: vec![all[1].clone()],
        };
        egui::__run_test_ui(|ui| {
            let ctx = ui.ctx();
            let height = layout_height(&all, &layout, false, false, &HashSet::new(), ctx);
            let expected =
                FOLDER_SLOT + EMPTY_ROW_HEIGHT + BOTTOM_GAP + FOLDER_SLOT + ROW_HEIGHT + BOTTOM_GAP;
            assert!((height - expected).abs() < 0.01);
        });
    }

    #[test]
    fn row_heights_count_children_and_show_more() {
        let all = vec![
            mk_session("root-busy", None, true),
            mk_session("root-busy-child", Some("root-busy"), false),
        ];
        let roots = vec![all[0].clone()];
        egui::__run_test_ui(|ui| {
            let ctx = ui.ctx();
            let height = roots_height(&all, &roots, false, false, ctx);
            let expected = ROW_HEIGHT + CHILD_ROW_HEIGHT + BOTTOM_GAP;
            assert!((height - expected).abs() < 0.01);
        });
    }

    #[test]
    fn show_more_rows_reserve_their_slot() {
        let roots: Vec<Session> = (0..10)
            .map(|i| mk_session(&format!("s{i}"), None, false))
            .collect();
        egui::__run_test_ui(|ui| {
            let ctx = ui.ctx();
            let collapsed = roots_height(&roots, &roots, false, false, ctx);
            assert!((collapsed - (7.0 * ROW_HEIGHT + SHOW_MORE_HEIGHT + BOTTOM_GAP)).abs() < 0.01);
            let expanded = roots_height(&roots, &roots, false, true, ctx);
            assert!((expanded - (10.0 * ROW_HEIGHT + SHOW_MORE_HEIGHT + BOTTOM_GAP)).abs() < 0.01);
        });
    }

    #[test]
    fn project_colors_are_deterministic_tints() {
        let p = Palette::dark();
        let project = ProjectEntry {
            id: "dir:/tmp/alpha".into(),
            canonical: "/tmp/alpha".into(),
            name: "Alpha".into(),
            name_override: None,
            color: None,
            hidden: false,
            added_at: 1.0,
        };
        assert_eq!(project_color(&project, &p), project_color(&project, &p));
        let mut other = project.clone();
        other.id = "dir:/tmp/beta".into();
        assert_ne!(project_color(&other, &p), egui::Color32::PLACEHOLDER);
    }

    #[test]
    fn markdown_export_includes_messages() {
        let session = mk_session("ses_export", None, false);
        let messages = vec![Message::User(Box::new(rchamber_api::models::UserMessage {
            id: "msg_1".into(),
            time: rchamber_api::models::MessageTime::default(),
            text: "Hello there".into(),
            files: Vec::new(),
            agents: Vec::new(),
            skills: Vec::new(),
            metadata: None,
        }))];
        let markdown = session_markdown(&session, &messages);
        assert!(markdown.contains("# Untitled session"));
        assert!(markdown.contains("## User"));
        assert!(markdown.contains("Hello there"));
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("Hello / world: test"), "Hello---world--test");
        assert_eq!(file_name("   "), "session");
    }
}
