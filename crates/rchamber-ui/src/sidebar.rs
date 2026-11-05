use eframe::egui::{
    self, Align, CornerRadius, Layout, Rect, RichText, ScrollArea, Sense, Stroke, TextStyle, Ui,
    Vec2, WidgetText,
};
use rchamber_api::models::Session;
use rchamber_app::format::{compact_number, money, relative_time};
use rchamber_app::{Action, Connection};

use crate::app::{RustChamber, hover_amount, menu_nav_focus, menu_popup, motion_time};
use crate::chrome;
use crate::icons::{self, Icon};
use crate::motion;
use crate::theme::{self, Palette};

const HEADER_BAND: f32 = 40.0;
const HEADER_PAD_X: f32 = 8.0;
const HEADER_PAD_Y: f32 = 4.0;
const TOOLBAR_ROW: f32 = 32.0;
const TOOLBAR_GAP: f32 = 8.0;
const ACTION: f32 = 24.0;
const ACTION_GLYPH: f32 = 16.0;
const GRID_INSET: f32 = 12.0;
const GRID_PAD: f32 = GRID_INSET - (ACTION - ACTION_GLYPH) * 0.5;
const ROW_HEIGHT: f32 = 34.0;
const ROW_HEIGHT_DETAILED: f32 = 52.0;
const ROW_PAD_START: f32 = 6.0;
const ROW_PAD_END: f32 = 6.0;
const ROW_TEXT_INSET: f32 = GRID_INSET - ROW_PAD_START;
const ROW_TEXT_INSET_TREE: f32 = 20.0;
const ROW_END_INSET: f32 = 6.0;
const ROW_CHEVRON_CENTER: f32 = 10.0;
const ROW_PAD_Y_DETAILED: f32 = 6.0;
const TITLE_SIZE: f32 = 13.5;
const META_SIZE: f32 = 12.0;
const FOOTER_HEIGHT: f32 = 40.0;
const FOOTER_PAD_X: f32 = GRID_PAD;
const FOOTER_PAD_END: f32 = 4.0;
const SECTION_HEIGHT: f32 = 28.0;
const SECTION_LABEL_INSET: f32 = GRID_INSET;
const SECTION_GAP: f32 = 12.0;
const SECTION_CHEVRON: f32 = 14.0;
const SECTION_ACTION: f32 = 24.0;
const SECTION_ACTION_GLYPH: f32 = 16.0;
const PROJECT_SLOT: f32 = 32.0;
const PROJECT_GLYPH: f32 = 14.0;
const DISCLOSURE_SECONDS: f32 = 0.2;
const EMPTY_BLOCK: f32 = 80.0;
const MENU_TRIGGER: f32 = 20.0;
const MENU_TRIGGER_GLYPH: f32 = 16.0;
const MENU_TRIGGER_INSET: f32 = 4.0;
const MENU_WIDTH: f32 = 180.0;
const MARK: f32 = 12.0;
const CHILD_ROW_HEIGHT: f32 = 28.0;
const CHILD_TITLE_SIZE: f32 = 12.5;
const TREE_INDENT: f32 = 16.0;
const SUB_LABEL_HEIGHT: f32 = 24.0;
const SUB_LABEL_GAP: f32 = 8.0;
const SHOW_MORE_HEIGHT: f32 = 28.0;
const MAX_VISIBLE_ROOTS: usize = 8;

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let side = ui.max_rect();
    ui.spacing_mut().item_spacing = Vec2::ZERO;

    chrome::sidebar_strip(ui);
    header(ui, app, &p);

    let query = app.model.search.trim().to_string();
    let all: Vec<Session> = app.model.sorted_sessions().into_iter().cloned().collect();
    let mut roots: Vec<Session> = all
        .iter()
        .filter(|session| session.parent_id.is_none())
        .cloned()
        .collect();
    if query.starts_with("ses_")
        && let Some(hit) = app
            .model
            .sessions
            .iter()
            .find(|session| session.id == query)
            .cloned()
        && !roots.iter().any(|session| session.id == hit.id)
    {
        roots.insert(0, hit);
    }

    let mut in_progress: Vec<Session> = Vec::new();
    let mut normal: Vec<Session> = Vec::new();
    for root in &roots {
        if busy_any(app, &all, &root.id) {
            in_progress.push(root.clone());
        } else {
            normal.push(root.clone());
        }
    }

    let ctx = ui.ctx().clone();
    let show_all_key = egui::Id::unique("sidebar-show-more-expanded");
    let mut show_all = ctx
        .data(|data| data.get_temp::<bool>(show_all_key))
        .unwrap_or(false);

    let detailed = app.ui.detailed_rows;
    let rows_height = if roots.is_empty() {
        8.0 + EMPTY_BLOCK
    } else {
        chat_rows_height(&all, &in_progress, &normal, detailed, show_all, &ctx)
    };
    let projects_height = PROJECT_SLOT + SECTION_GAP + SECTION_HEIGHT + rows_height;

    let duration = motion_time(&ctx, DISCLOSURE_SECONDS);
    let reveal_projects = motion::Animated::new("sidebar/projects-disclosure")
        .with_duration(duration)
        .with_easing(motion::Easing::EaseOut)
        .bool(&ctx, !app.ui.sidebar_projects_closed);
    let reveal_chats = motion::Animated::new("sidebar/chats-disclosure")
        .with_duration(duration)
        .with_easing(motion::Easing::EaseOut)
        .bool(&ctx, !app.ui.sidebar_chats_closed);

    let body_height = (ui.available_height() - FOOTER_HEIGHT).max(80.0);
    let mut select: Option<String> = None;
    ScrollArea::vertical()
        .id_salt("session-list")
        .auto_shrink([false, false])
        .max_height(body_height)
        .show(ui, |ui| {
            let (caption, _) = section_header(
                ui,
                &p,
                "projects",
                "Projects",
                app.ui.sidebar_projects_closed,
                false,
            );
            if caption {
                app.ui.sidebar_projects_closed = !app.ui.sidebar_projects_closed;
            }
            disclosure(ui, projects_height, reveal_projects, |ui| {
                project_row(ui, app, &p);
                ui.add_space(SECTION_GAP);
                let (toggle, add) =
                    section_header(ui, &p, "chats", "Chats", app.ui.sidebar_chats_closed, true);
                if toggle {
                    app.ui.sidebar_chats_closed = !app.ui.sidebar_chats_closed;
                }
                if add {
                    app.create_session(None);
                }
                disclosure(ui, rows_height, reveal_chats, |ui| {
                    if roots.is_empty() {
                        empty_state(ui, &p, !query.is_empty());
                    } else {
                        if !in_progress.is_empty() {
                            sub_label(ui, &p, "In progress");
                            for root in &in_progress {
                                let has_children = all.iter().any(|session| {
                                    session.parent_id.as_deref() == Some(root.id.as_str())
                                });
                                let expanded = tree_expanded(&ctx, &root.id);
                                let event = session_row(
                                    ui,
                                    app,
                                    &p,
                                    root,
                                    detailed,
                                    has_children,
                                    expanded,
                                );
                                if event.open {
                                    select = Some(root.id.clone());
                                }
                                if event.toggle {
                                    tree_set_expanded(&ctx, &root.id, !expanded);
                                }
                                if expanded {
                                    render_children(
                                        ui,
                                        app,
                                        &p,
                                        &all,
                                        &root.id,
                                        1,
                                        &mut select,
                                        &ctx,
                                    );
                                }
                            }
                        }
                        let shown = if show_all {
                            normal.len()
                        } else {
                            normal.len().min(MAX_VISIBLE_ROOTS)
                        };
                        for root in normal.iter().take(shown) {
                            let event = session_row(ui, app, &p, root, detailed, false, false);
                            if event.open {
                                select = Some(root.id.clone());
                            }
                        }
                        if normal.len() > MAX_VISIBLE_ROOTS && show_more_row(ui, &p, show_all) {
                            show_all = !show_all;
                        }
                        ui.add_space(8.0);
                    }
                });
            });
        });

    ctx.data_mut(|data| data.insert_temp(show_all_key, show_all));

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        footer(ui, app, &p);
    });

    ui.painter().vline(
        side.right() - 0.25,
        side.y_range(),
        Stroke::new(0.5, crate::app::edge(&p)),
    );

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
    let height = HEADER_BAND + if app.ui.search_open { 57.0 } else { 0.0 };
    band(ui, height, |ui| {
        ui.add_space(HEADER_PAD_Y);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), TOOLBAR_ROW),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = TOOLBAR_GAP;
                ui.add_space(GRID_PAD);
                let search = action(
                    ui,
                    p,
                    Icon::Search,
                    app.ui.search_open,
                    if app.ui.search_open {
                        ActionTint::Selected
                    } else {
                        ActionTint::Idle
                    },
                );
                if search.on_hover_text("Search sessions (⌘F)").clicked() {
                    app.ui.search_open = !app.ui.search_open;
                    app.ui.session_search.clear();
                    app.model.search.clear();
                    app.ui.focus_search = app.ui.search_open;
                }
                let folder = action(ui, p, Icon::Folder, false, ActionTint::Idle);
                if folder.on_hover_text("Add project").clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    app.set_workspace(folder.to_string_lossy().to_string());
                }
                display_menu(ui, app, p);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(GRID_PAD);
                    more_menu(ui, app, p);
                });
            },
        );
        if app.ui.search_open {
            search_row(ui, app, p);
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionTint {
    Idle,
    Selected,
}

fn action(ui: &mut Ui, p: &Palette, icon: Icon, active: bool, tint: ActionTint) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ACTION), Sense::click());
    crate::widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    if tint == ActionTint::Selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.selection);
    }
    let color = match tint {
        ActionTint::Selected => p.selection_fg,
        ActionTint::Idle => p.fg_muted.lerp_to_gamma(p.fg, hovered),
    };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(ACTION_GLYPH)),
        if active { p.fg } else { color },
    );
    response
}

fn display_menu(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let trigger = action(ui, p, Icon::Menu, app.ui.sidebar_menu, ActionTint::Idle);
    trigger.clone().on_hover_text("Display");
    let id = egui::Id::unique("sidebar-display");
    let mut open = app.ui.sidebar_menu;
    let mut state = app.ui.sidebar_menu;
    let mut items: Vec<egui::Response> = Vec::new();
    menu_popup(ui.ctx(), &trigger, id, &mut open, p, 4.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(200.0);
        let response =
            crate::widgets::menu_item(ui, "New session", Some(Icon::Plus), Some("⌘N"), p);
        if response.clicked() {
            app.create_session(None);
            state = false;
        }
        items.push(response);
        let response = crate::widgets::menu_item_ex(
            ui,
            "Detailed rows",
            Some(Icon::Layers),
            None,
            app.ui.detailed_rows,
            true,
            p,
        );
        if response.clicked() {
            app.ui.detailed_rows = !app.ui.detailed_rows;
            state = false;
        }
        items.push(response);
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 4.0);
        ui.add_space(2.0);
        let response = crate::widgets::menu_item(ui, "Reconnect", Some(Icon::Refresh), None, p);
        if response.clicked() {
            app.backend.send(Action::Reconnect);
            state = false;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "Settings", Some(Icon::Settings), Some("⌘,"), p);
        if response.clicked() {
            app.open_settings();
            state = false;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "Keyboard shortcuts", Some(Icon::Command), None, p);
        if response.clicked() {
            app.model.toast(
                rchamber_app::ToastKind::Info,
                "⌘K commands · ⌘N new session · ⌘, settings",
            );
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

fn more_menu(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let trigger = action(ui, p, Icon::More, false, ActionTint::Idle);
    trigger.clone().on_hover_text("More");
    let id = egui::Id::unique("sidebar-more");
    let mut open = app.ui.more_menu;
    let mut state = app.ui.more_menu;
    let mut items: Vec<egui::Response> = Vec::new();
    menu_popup(ui.ctx(), &trigger, id, &mut open, p, 4.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(200.0);
        let response = crate::widgets::menu_item(ui, "Open folder…", Some(Icon::Folder), None, p);
        if response.clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                app.set_workspace(folder.to_string_lossy().to_string());
            }
            state = false;
        }
        items.push(response);
        let response = crate::widgets::menu_item(ui, "Reconnect", Some(Icon::Refresh), None, p);
        if response.clicked() {
            app.backend.send(Action::Reconnect);
            state = false;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "Settings", Some(Icon::Settings), Some("⌘,"), p);
        if response.clicked() {
            app.open_settings();
            state = false;
        }
        items.push(response);
        let response =
            crate::widgets::menu_item(ui, "Keyboard shortcuts", Some(Icon::Command), None, p);
        if response.clicked() {
            app.model.toast(
                rchamber_app::ToastKind::Info,
                "⌘K commands · ⌘N new session · ⌘, settings",
            );
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
    app.ui.more_menu = state;
}

fn search_row(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let width = (ui.available_width() - HEADER_PAD_X * 2.0).max(80.0);
    band(ui, 57.0, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(HEADER_PAD_X);
            let field = Rect::from_min_size(ui.cursor().left_top(), Vec2::new(width, 32.0));
            let response = ui.allocate_rect(field, Sense::click());
            let id = egui::Id::unique("sidebar-search");
            let focused = ui.memory(|memory| memory.has_focus(id));
            ui.painter().rect_stroke(
                field,
                CornerRadius::same(theme::radius::MD),
                Stroke::new(1.0, if focused { p.alpha(p.fg, 66) } else { p.border }),
                egui::StrokeKind::Inside,
            );
            icons::paint(
                ui,
                Icon::Search,
                Rect::from_center_size(
                    egui::Pos2::new(field.left() + 12.0, field.center().y),
                    Vec2::splat(16.0),
                ),
                p.fg_muted,
            );
            let text_rect = Rect::from_min_max(
                egui::Pos2::new(field.left() + 32.0, field.top() + 6.0),
                egui::Pos2::new(field.right() - 32.0, field.bottom() - 4.0),
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
                    .hint_text(
                        RichText::new("Search sessions")
                            .font(theme::font(TITLE_SIZE))
                            .color(p.fg_muted),
                    )
                    .text_color(p.fg)
                    .font(theme::font(TITLE_SIZE))
                    .desired_width(f32::INFINITY),
            );
            if response.clicked() || app.ui.focus_search {
                edit.request_focus();
                app.ui.focus_search = false;
            }
            let enter = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if enter || (focused && ui.input(|input| input.key_pressed(egui::Key::Enter))) {
                app.model.search = app.ui.session_search.trim().to_string();
                edit.request_focus();
            }
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) && focused {
                if app.ui.session_search.is_empty() {
                    app.ui.search_open = false;
                } else {
                    app.ui.session_search.clear();
                    app.model.search.clear();
                    edit.request_focus();
                }
            }
            if !app.ui.session_search.is_empty() {
                let clear = Rect::from_center_size(
                    egui::Pos2::new(field.right() - 16.0, field.center().y),
                    Vec2::splat(24.0),
                );
                let response = ui.interact(
                    clear,
                    egui::Id::unique("sidebar-search-clear"),
                    Sense::click(),
                );
                icons::paint(
                    ui,
                    Icon::Close,
                    Rect::from_center_size(clear.center(), Vec2::splat(14.0)),
                    if response.hovered() { p.fg } else { p.fg_muted },
                );
                if response.clicked() {
                    app.ui.session_search.clear();
                    app.model.search.clear();
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
            }
        });
        ui.add_space(4.0);
        let (hint_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), META_SIZE + 5.0),
            Sense::hover(),
        );
        let hint = if app.ui.session_search.trim().is_empty() {
            "Press ↵ to search"
        } else {
            "Press ↵ to search · Esc to clear"
        };
        ui.painter().text(
            egui::Pos2::new(hint_rect.left() + GRID_INSET, hint_rect.center().y),
            egui::Align2::LEFT_CENTER,
            hint,
            theme::font(META_SIZE),
            p.fg_muted,
        );
    });
}

fn project_row(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), PROJECT_SLOT),
        Sense::hover(),
    );
    let rect = Rect::from_min_max(
        egui::Pos2::new(slot.left() + ROW_PAD_START, slot.top() + 2.0),
        egui::Pos2::new(slot.right() - ROW_PAD_END, slot.bottom() - 2.0),
    );
    let response = ui.interact(rect, egui::Id::unique("workspace-row"), Sense::click());
    crate::widgets::pointer(ui, &response);
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.scale_alpha(p.hover, 0.6),
        );
    }
    icons::paint(
        ui,
        Icon::Folder,
        Rect::from_center_size(
            egui::Pos2::new(
                slot.left() + GRID_INSET + PROJECT_GLYPH * 0.5,
                rect.center().y,
            ),
            Vec2::splat(PROJECT_GLYPH),
        ),
        p.fg_muted,
    );
    let label_left = slot.left() + GRID_INSET + PROJECT_GLYPH + 6.0;
    let galley = WidgetText::from(
        RichText::new(app.model.workspace_name())
            .font(theme::font_medium(theme::TEXT_UI_SECTION))
            .color(p.fg),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (rect.right() - label_left - 24.0).max(40.0),
        TextStyle::Body,
    );
    ui.painter().galley(
        egui::Pos2::new(label_left, rect.center().y - galley.size().y / 2.0),
        galley,
        p.fg,
    );
    icons::paint(
        ui,
        Icon::ChevronDown,
        Rect::from_center_size(
            egui::Pos2::new(slot.right() - GRID_INSET - 6.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        p.fg_muted,
    );
    let id = egui::Id::unique("workspace-menu");
    let mut menu_open = app.ui.workspace_menu;
    let mut close_menu = false;
    let mut items: Vec<egui::Response> = Vec::new();
    menu_popup(ui.ctx(), &response, id, &mut menu_open, p, 6.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(250.0);
        crate::widgets::section_label(ui, "Workspaces", p);
        ui.add_space(4.0);
        let projects = app.model.projects.clone();
        let mut picked: Option<String> = None;
        for project in &projects {
            let selected = app.model.workspace.as_deref() == Some(project.canonical.as_str());
            let label = project.display_name();
            let resp = crate::widgets::menu_item(ui, &label, Some(Icon::Folder), None, p);
            if resp.clicked() {
                picked = Some(project.canonical.clone());
            }
            if selected {
                icons::paint(
                    ui,
                    Icon::Check,
                    Rect::from_center_size(
                        egui::Pos2::new(resp.rect.right() - 14.0, resp.rect.center().y),
                        Vec2::splat(12.0),
                    ),
                    p.primary,
                );
            }
            items.push(resp);
        }
        if projects.is_empty() {
            ui.label(
                RichText::new("No known projects")
                    .font(theme::font(META_SIZE))
                    .color(p.fg_muted),
            );
        }
        ui.add_space(4.0);
        crate::widgets::hline(ui, p, 4.0);
        ui.add_space(2.0);
        let open_folder = crate::widgets::menu_item(ui, "Open folder…", Some(Icon::Plus), None, p);
        if open_folder.clicked()
            && let Some(folder) = rfd::FileDialog::new().pick_folder()
        {
            picked = Some(folder.to_string_lossy().to_string());
        }
        items.push(open_folder);
        if let Some(directory) = picked {
            app.set_workspace(directory);
            close_menu = true;
        }
    });
    menu_nav_focus(ui.ctx(), id, menu_open, &items);
    app.ui.workspace_menu = if close_menu { false } else { menu_open };
}

fn section_header(
    ui: &mut Ui,
    p: &Palette,
    id: &str,
    label: &str,
    collapsed: bool,
    add_button: bool,
) -> (bool, bool) {
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
    if add_button {
        let rect = Rect::from_center_size(
            egui::Pos2::new(
                slot.right() - GRID_PAD - SECTION_ACTION * 0.5,
                slot.center().y,
            ),
            Vec2::splat(SECTION_ACTION),
        );
        let add = ui.interact(
            rect,
            egui::Id::unique(("sidebar-section-add", id)),
            Sense::click(),
        );
        crate::widgets::pointer(ui, &add);
        add_hovered = add.hovered();
        add_clicked = add.clicked();
        if add_hovered || response.hovered() {
            icons::paint(
                ui,
                Icon::Plus,
                Rect::from_center_size(rect.center(), Vec2::splat(SECTION_ACTION_GLYPH)),
                if add_hovered { p.fg } else { p.fg_muted },
            );
        }
    }
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        theme::font_medium(theme::TEXT_UI_LABEL),
        p.fg_muted,
    );
    let left = slot.left() + SECTION_LABEL_INSET;
    ui.painter().galley(
        egui::Pos2::new(left, slot.center().y - galley.size().y * 0.5),
        galley.clone(),
        p.fg_muted,
    );
    if collapsed || response.hovered() || add_hovered {
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
    (response.clicked() && !add_clicked, add_clicked)
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
                "No sessions match"
            } else {
                "No sessions yet"
            })
            .font(theme::font_medium(13.0))
            .color(p.fg_muted),
        );
        ui.add_space(3.0);
        ui.label(
            RichText::new(if searching {
                "Press ↵ to search, Esc to close"
            } else {
                "Start one with ⌘N"
            })
            .font(theme::font(theme::TEXT_MICRO))
            .color(p.fg_muted),
        );
    });
}

fn session_row(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    session: &Session,
    detailed: bool,
    has_children: bool,
    expanded: bool,
) -> RowEvent {
    let selected = app.model.active_id() == Some(session.id.as_str());
    let slot_height = if detailed {
        ROW_HEIGHT_DETAILED
    } else {
        ROW_HEIGHT
    };
    let (slot, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), slot_height), Sense::hover());
    let card = Rect::from_min_max(
        egui::Pos2::new(slot.left() + ROW_PAD_START, slot.top() + 2.0),
        egui::Pos2::new(slot.right() - ROW_PAD_END, slot.bottom() - 2.0),
    );
    let menu_open = app.ui.session_menu.as_deref() == Some(session.id.as_str());
    let menu_id = egui::Id::unique(("session-menu", &session.id));
    let menu_visible = menu_open || motion::popup(ui.ctx(), menu_id.with("popup"), false).is_some();

    let mark_y = if detailed {
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

    let fill = if selected {
        p.scale_alpha(p.selection, 0.7)
    } else if pressed {
        p.scale_alpha(p.hover, 0.7)
    } else {
        p.scale_alpha(p.hover, 0.6)
    };
    if selected || pressed || show_menu {
        ui.painter()
            .rect_filled(card, CornerRadius::same(theme::radius::MD), fill);
    }

    let running = running(app, session);
    let pending = permission_pending(app, &session.id);
    let unread = unread(app, &session.id);
    let title_color = if selected { p.selection_fg } else { p.fg };

    let time_galley = if detailed {
        None
    } else {
        let time = relative_time(session.time.updated);
        (!time.is_empty()).then(|| {
            ui.painter()
                .layout_no_wrap(time, theme::font(META_SIZE), p.fg_muted)
        })
    };
    let trailing = if show_menu {
        MENU_TRIGGER_INSET + MENU_TRIGGER + 6.0 + MARK + 6.0
    } else {
        let time_width = time_galley
            .as_ref()
            .map(|galley| galley.size().x + 4.0)
            .unwrap_or(0.0);
        ROW_END_INSET + time_width + MARK + 4.0
    };
    let title_inset = if has_children {
        ROW_TEXT_INSET_TREE
    } else {
        ROW_TEXT_INSET
    };
    let title_left = card.left() + title_inset;
    let title_room = (card.right() - title_left - trailing).max(40.0);
    let title = WidgetText::from(
        RichText::new(session.title())
            .font(theme::font(TITLE_SIZE))
            .color(title_color),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        title_room,
        TextStyle::Body,
    );
    let title_y = if detailed {
        card.top() + ROW_PAD_Y_DETAILED + 20.0 * 0.5 - title.size().y * 0.5
    } else {
        card.center().y - title.size().y * 0.5
    };
    ui.painter()
        .galley(egui::Pos2::new(title_left, title_y), title, title_color);

    let mut chevron_clicked = false;
    if has_children {
        let chevron_center = egui::Pos2::new(card.left() + ROW_CHEVRON_CENTER, card.center().y);
        let chevron = tree_chevron(ui, p, expanded, chevron_center, &session.id);
        chevron_clicked = chevron.clicked();
    }

    if detailed {
        let meta = meta_line(app, session);
        let meta_galley = WidgetText::from(
            RichText::new(meta)
                .font(theme::font(META_SIZE))
                .color(p.fg_muted),
        )
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (card.width() - title_inset - 14.0).max(40.0),
            TextStyle::Body,
        );
        ui.painter().galley(
            egui::Pos2::new(title_left, card.top() + ROW_PAD_Y_DETAILED + 21.5),
            meta_galley,
            p.fg_muted,
        );
    }

    let mark_center = if show_menu {
        egui::Pos2::new(trigger_rect.left() - 6.0 - MARK * 0.5, mark_y)
    } else {
        let mut end = card.right() - ROW_END_INSET;
        if let Some(galley) = &time_galley {
            end -= galley.size().x;
            ui.painter().galley(
                egui::Pos2::new(end, card.center().y - galley.size().y * 0.5),
                galley.clone(),
                p.fg_muted,
            );
            end -= 4.0;
        }
        egui::Pos2::new(end - MARK * 0.5, mark_y)
    };
    if pending {
        icons::paint(
            ui,
            Icon::Shield,
            Rect::from_center_size(mark_center, Vec2::splat(MARK)),
            p.coral,
        );
    } else if running {
        running_mark(ui, mark_center, p);
    } else if unread > 0 && !selected {
        unread_mark(ui, mark_center, p);
    }

    if show_menu {
        menu_trigger_paint(ui, p, menu_open, trigger.hovered(), trigger_rect);
    }
    let menu_clicked = if show_menu {
        session_menu(ui, app, p, session, &trigger, menu_open)
    } else {
        false
    };
    let tooltip = tooltip_for(app, session);
    let response = response.on_hover_text(tooltip);
    RowEvent {
        open: response.clicked() && !menu_clicked && !chevron_clicked,
        toggle: chevron_clicked,
    }
}

#[derive(Default)]
struct RowEvent {
    open: bool,
    toggle: bool,
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

fn sub_label(ui: &mut Ui, p: &Palette, label: &str) {
    ui.add_space(SUB_LABEL_GAP);
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), SUB_LABEL_HEIGHT),
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

fn show_more_row(ui: &mut Ui, p: &Palette, expanded: bool) -> bool {
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), SHOW_MORE_HEIGHT),
        Sense::hover(),
    );
    let response = ui.interact(
        slot,
        egui::Id::unique("sidebar-show-more-row"),
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
            slot.left() + GRID_INSET,
            slot.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    response.clicked()
}

fn child_row(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    session: &Session,
    depth: usize,
    has_children: bool,
    expanded: bool,
) -> RowEvent {
    let selected = app.model.active_id() == Some(session.id.as_str());
    let indent = depth as f32 * TREE_INDENT;
    let (slot, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), CHILD_ROW_HEIGHT),
        Sense::hover(),
    );
    let card = Rect::from_min_max(
        egui::Pos2::new(slot.left() + ROW_PAD_START + indent, slot.top() + 2.0),
        egui::Pos2::new(slot.right() - ROW_PAD_END, slot.bottom() - 2.0),
    );
    let response = ui.interact(
        card,
        egui::Id::unique(("session-child", &session.id)),
        Sense::click(),
    );
    let pressed = response.is_pointer_button_down_on();
    let fill = if selected {
        p.scale_alpha(p.selection, 0.7)
    } else if pressed {
        p.scale_alpha(p.hover, 0.7)
    } else {
        p.scale_alpha(p.hover, 0.6)
    };
    if selected || pressed || response.hovered() {
        ui.painter()
            .rect_filled(card, CornerRadius::same(theme::radius::MD), fill);
    }

    let running = running(app, session);
    let pending = permission_pending(app, &session.id);
    let unread = unread(app, &session.id);
    let title_color = if selected { p.selection_fg } else { p.fg_muted };

    let title_inset = if has_children {
        ROW_TEXT_INSET_TREE
    } else {
        ROW_TEXT_INSET
    };
    let title_left = card.left() + title_inset;
    let trailing = ROW_END_INSET + MARK + 4.0;
    let title_room = (card.right() - title_left - trailing).max(30.0);
    let title = WidgetText::from(
        RichText::new(session.title())
            .font(theme::font(CHILD_TITLE_SIZE))
            .color(title_color),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        title_room,
        TextStyle::Body,
    );
    let title_y = card.center().y - title.size().y * 0.5;
    ui.painter()
        .galley(egui::Pos2::new(title_left, title_y), title, title_color);

    let mut chevron_clicked = false;
    if has_children {
        let chevron_center = egui::Pos2::new(card.left() + ROW_CHEVRON_CENTER, card.center().y);
        let chevron = tree_chevron(ui, p, expanded, chevron_center, &session.id);
        chevron_clicked = chevron.clicked();
    }

    let mark_center = egui::Pos2::new(card.right() - ROW_END_INSET - MARK * 0.5, card.center().y);
    if pending {
        icons::paint(
            ui,
            Icon::Shield,
            Rect::from_center_size(mark_center, Vec2::splat(MARK)),
            p.coral,
        );
    } else if running {
        running_mark(ui, mark_center, p);
    } else if unread > 0 && !selected {
        unread_mark(ui, mark_center, p);
    }

    let tooltip = tooltip_for(app, session);
    let response = response.on_hover_text(tooltip);
    RowEvent {
        open: response.clicked() && !chevron_clicked,
        toggle: chevron_clicked,
    }
}

fn render_children(
    ui: &mut Ui,
    app: &mut RustChamber,
    p: &Palette,
    all: &[Session],
    parent_id: &str,
    depth: usize,
    select: &mut Option<String>,
    ctx: &egui::Context,
) {
    for child in all
        .iter()
        .filter(|session| session.parent_id.as_deref() == Some(parent_id))
    {
        let has_children = all
            .iter()
            .any(|session| session.parent_id.as_deref() == Some(child.id.as_str()));
        let expanded = tree_expanded(ctx, &child.id);
        let event = child_row(ui, app, p, child, depth, has_children, expanded);
        if event.open {
            *select = Some(child.id.clone());
        }
        if event.toggle {
            tree_set_expanded(ctx, &child.id, !expanded);
        }
        if expanded {
            render_children(ui, app, p, all, &child.id, depth + 1, select, ctx);
        }
    }
}

fn chat_rows_height(
    all: &[Session],
    in_progress: &[Session],
    normal: &[Session],
    detailed: bool,
    show_all: bool,
    ctx: &egui::Context,
) -> f32 {
    let row_height = if detailed {
        ROW_HEIGHT_DETAILED
    } else {
        ROW_HEIGHT
    };
    let mut height = 0.0;
    if !in_progress.is_empty() {
        height += SUB_LABEL_GAP + SUB_LABEL_HEIGHT;
        for root in in_progress {
            height += row_height;
            if tree_expanded(ctx, &root.id) {
                height += subtree_height(all, ctx, &root.id);
            }
        }
    }
    let shown = if show_all {
        normal.len()
    } else {
        normal.len().min(MAX_VISIBLE_ROOTS)
    };
    height += shown as f32 * row_height;
    if normal.len() > MAX_VISIBLE_ROOTS {
        height += SHOW_MORE_HEIGHT;
    }
    height + 8.0
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

fn permission_pending(app: &RustChamber, id: &str) -> bool {
    app.model
        .active
        .as_ref()
        .is_some_and(|chat| chat.session.id == id && chat.awaiting_permission)
}

fn unread(app: &RustChamber, id: &str) -> usize {
    app.model.unread.get(id).copied().unwrap_or(0)
}

fn running_mark(ui: &Ui, center: egui::Pos2, p: &Palette) {
    ui.painter()
        .circle_stroke(center, 5.0, Stroke::new(1.5, p.blue));
}

fn unread_mark(ui: &Ui, center: egui::Pos2, p: &Palette) {
    icons::paint(
        ui,
        Icon::Check,
        Rect::from_center_size(center, Vec2::splat(MARK)),
        p.green,
    );
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
    let pad = theme::popover_frame(p).inner_margin.sum().x;
    menu_popup(ui.ctx(), trigger, id, &mut open, p, 4.0, |ui, _| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.set_width(MENU_WIDTH - pad);
        let response = crate::widgets::menu_item(ui, "Rename", Some(Icon::Pencil), None, p);
        if response.clicked() {
            app.ui.rename = Some((session.id.clone(), session.title().to_string()));
            app.ui.session_menu = None;
        }
        items.push(response);
        let response = crate::widgets::menu_item(ui, "Archive", Some(Icon::Archive), None, p);
        if response.clicked() {
            app.backend.send(Action::ArchiveSession {
                id: session.id.clone(),
                archived: true,
            });
            app.ui.session_menu = None;
        }
        items.push(response);
        ui.add_space(2.0);
        crate::widgets::hline(ui, p, 0.0);
        ui.add_space(2.0);
        let response =
            crate::widgets::menu_item_destructive(ui, "Delete", Some(Icon::Trash), None, true, p);
        if response.clicked() {
            app.backend.send(Action::DeleteSession(session.id.clone()));
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

fn meta_line(app: &RustChamber, session: &Session) -> String {
    let model = session
        .model
        .as_ref()
        .and_then(|reference| app.model.model_for(reference))
        .map(|model| model.display_name().to_string())
        .unwrap_or_else(|| "No model".into());
    let time = relative_time(session.time.updated);
    format!("{} · {}", model, time)
}

fn tooltip_for(app: &RustChamber, session: &Session) -> String {
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
    if session.cost > 0.0001 {
        parts.push(money(session.cost));
    }
    if session.tokens.total() > 0.0 {
        parts.push(format!("{} tokens", compact_number(session.tokens.total())));
    }
    parts.join("\n")
}

fn footer(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), FOOTER_HEIGHT),
        Sense::hover(),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_space(FOOTER_PAD_X);
            let settings = footer_button(ui, p, Icon::Settings);
            if settings.on_hover_text("Settings (⌘,)").clicked() {
                app.open_settings();
            }
            let help = footer_button(ui, p, Icon::Info);
            if help.on_hover_text("Keyboard shortcuts").clicked() {
                app.model.toast(
                    rchamber_app::ToastKind::Info,
                    "⌘K commands · ⌘N new session · ⌘, settings",
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(FOOTER_PAD_END);
                let (color, label) = match &app.model.connection {
                    Connection::Ready => (p.green, "Local".to_string()),
                    Connection::Connecting => (p.primary, "Starting".to_string()),
                    Connection::Reconnecting(_) => (p.coral, "Reconnecting".to_string()),
                    Connection::Failed(_) => (p.red, "Offline".to_string()),
                    Connection::Disconnected => (p.fg_muted, "Disconnected".to_string()),
                };
                let version = app
                    .model
                    .server
                    .as_ref()
                    .map(|server| server.version.clone());
                let galley = ui.painter().layout_no_wrap(
                    label.clone(),
                    theme::font(theme::TEXT_META),
                    p.fg_muted,
                );
                let width = 8.0 + 6.0 + galley.size().x + 8.0;
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(width.min(192.0), ACTION), Sense::hover());
                if response.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::same(theme::radius::MD),
                        p.scale_alpha(p.hover, 0.5),
                    );
                }
                ui.painter().circle_filled(
                    egui::Pos2::new(rect.left() + 4.0 + 4.0, rect.center().y),
                    4.0,
                    color,
                );
                ui.painter().galley(
                    egui::Pos2::new(
                        rect.left() + 4.0 + 12.0,
                        rect.center().y - galley.size().y * 0.5,
                    ),
                    galley,
                    p.fg_muted,
                );
                if let Some(version) = version {
                    let _ = response.on_hover_text(format!("OpenCode {version}"));
                }
            });
        },
    );
}

fn footer_button(ui: &mut Ui, p: &Palette, icon: Icon) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ACTION), Sense::click());
    crate::widgets::pointer(ui, &response);
    let hovered = hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    if hovered > 0.0 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.scale_alpha(p.hover, 0.5 * hovered),
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

#[cfg(test)]
mod tests {
    use super::*;

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
            let (toggle, add) = section_header(ui, &p, "chats", "Chats", false, true);
            assert!(!toggle);
            assert!(!add);
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
            revert: None,
            location: None,
            fork: None,
        }
    }

    #[test]
    fn chat_height_counts_children_and_show_more() {
        let all = vec![
            mk_session("root-busy", None, true),
            mk_session("root-busy-child", Some("root-busy"), false),
            mk_session("idle-1", None, false),
            mk_session("idle-2", None, false),
        ];
        let in_progress = vec![all[0].clone()];
        let normal = vec![all[2].clone(), all[3].clone()];
        egui::__run_test_ui(|ui| {
            let ctx = ui.ctx();
            let height = chat_rows_height(&all, &in_progress, &normal, false, false, ctx);
            let expected = SUB_LABEL_GAP
                + SUB_LABEL_HEIGHT
                + ROW_HEIGHT
                + CHILD_ROW_HEIGHT
                + 2.0 * ROW_HEIGHT
                + 8.0;
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
            let collapsed = chat_rows_height(&roots, &[], &roots, false, false, ctx);
            assert!((collapsed - (8.0 * ROW_HEIGHT + SHOW_MORE_HEIGHT + 8.0)).abs() < 0.01);
            let expanded = chat_rows_height(&roots, &[], &roots, false, true, ctx);
            assert!((expanded - (10.0 * ROW_HEIGHT + SHOW_MORE_HEIGHT + 8.0)).abs() < 0.01);
        });
    }
}
