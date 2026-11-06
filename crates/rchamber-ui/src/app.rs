use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, Layout, Margin, RichText, Sense, Stroke, Ui,
    Vec2,
};
use rchamber_api::models::ModelRef;
use rchamber_app::{Action, AppModel, Backend, BackendEvent, LaunchConfig, ToastKind};

use crate::changes;
use crate::chat;
use crate::context_panel;
use crate::home;
use crate::icons::{self, Icon};
use crate::markdown::Markdown;
use crate::motion::{self, Easing as Curve, Motion};
use crate::palette;
use crate::persist::Settings;
use crate::rail;
use crate::settings;
use crate::sidebar;
use crate::surfaces::{self, PanelZone, Surface, TabDrag};
use crate::theme::{self, Palette, ThemeMode};
use crate::theme_store;
use crate::widgets;

#[path = "header.rs"]
pub mod header;

const SIDEBAR_MIN: f32 = 264.0;
const SIDEBAR_MAX: f32 = 500.0;
const RAIL_WIDTH: f32 = 44.0;
const ZONE_MIN: f32 = 320.0;
const CHAT_MIN: f32 = 320.0;
const BOTTOM_MIN: f32 = 160.0;
const BOTTOM_CHAT_MIN: f32 = 200.0;
const ZONE_HANDLE: f32 = 8.0;
const BOTTOM_HANDLE: f32 = 8.0;
const DROP_TARGET_HEIGHT: f32 = 88.0;
const RESIZE_HANDLE: f32 = 3.0;
const CARD_RADIUS: u8 = 10;
const TOAST_WIDTH: f32 = 356.0;
const TOAST_INSET: f32 = 24.0;
const TOAST_PAD_X: i8 = 14;
const TOAST_PAD_Y: i8 = 12;
const TOAST_EXIT_SECONDS: f32 = 0.110;
const SHELL_MS: f32 = crate::motion::COLUMN;
const HOVER_MS: f32 = crate::motion::HOVER;

pub const CHATS_TARGET: &str = "chats";

pub(crate) fn ease_shell(t: f32) -> f32 {
    crate::motion::ease_standard(t)
}

pub(crate) fn motion_time(ctx: &egui::Context, seconds: f32) -> f32 {
    crate::motion::duration_for(ctx, seconds)
}

pub(crate) fn hover_amount(ctx: &egui::Context, id: Id, hovered: bool) -> f32 {
    let time = motion_time(ctx, HOVER_MS);
    if time == 0.0 {
        return if hovered { 1.0 } else { 0.0 };
    }
    ctx.animate_bool_with_time(id, hovered, time)
}

pub(crate) fn menu_popup(
    ctx: &egui::Context,
    anchor: &egui::Response,
    id: Id,
    open: &mut bool,
    p: &Palette,
    gap: f32,
    add_contents: impl FnOnce(&mut Ui, f32),
) -> Option<egui::Response> {
    let Some(t) = motion::popup(ctx, id.with("popup"), *open) else {
        menu_nav_clear(ctx, id);
        *open = false;
        return None;
    };
    let mut held = true;
    let shown = egui::Popup::from_response(anchor)
        .id(id)
        .open_bool(&mut held)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(theme::popover_frame_t(p, t.opacity))
        .gap(gap)
        .show(|ui| {
            ui.set_opacity(t.opacity);
            add_contents(ui, t.opacity);
        });
    if let Some(inner) = &shown {
        let rect = inner.response.rect;
        let pivot = egui::pos2(rect.center().x, rect.top());
        let transform =
            egui::emath::TSTransform::new(pivot.to_vec2() * (1.0 - t.scale) + t.offset, t.scale);
        ctx.transform_layer_shapes_after_rounding(inner.response.layer_id, transform);
    }
    if !held {
        *open = false;
    }
    shown.map(|inner| inner.response)
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MenuNav {
    pub index: Option<usize>,
    pub moved: bool,
}

pub(crate) fn menu_nav(
    ctx: &egui::Context,
    id: Id,
    open: bool,
    len: usize,
    text_field: bool,
) -> MenuNav {
    let index_id = id.with("nav-index");
    if !open || len == 0 {
        ctx.data_mut(|data| data.remove::<usize>(index_id));
        return MenuNav::default();
    }
    if ctx.text_edit_focused() && !text_field {
        return MenuNav {
            index: ctx.data(|data| data.get_temp::<usize>(index_id)),
            moved: false,
        };
    }
    let plain = ctx.input(|input| !input.modifiers.any());
    let (down, up, home, end) = ctx.input(|input| {
        (
            plain && input.key_pressed(egui::Key::ArrowDown),
            plain && input.key_pressed(egui::Key::ArrowUp),
            plain && input.key_pressed(egui::Key::Home),
            plain && input.key_pressed(egui::Key::End),
        )
    });
    let mut index = ctx.data(|data| data.get_temp::<usize>(index_id));
    let mut moved = false;
    if down {
        index = Some(index.map_or(0, |value| (value + 1) % len));
        moved = true;
    } else if up {
        index = Some(index.map_or(len - 1, |value| (value + len - 1) % len));
        moved = true;
    } else if home {
        index = Some(0);
        moved = true;
    } else if end {
        index = Some(len - 1);
        moved = true;
    }
    if moved {
        ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
    }
    let index = index.map(|value| value.min(len - 1));
    if let Some(value) = index {
        ctx.data_mut(|data| data.insert_temp(index_id, value));
    }
    MenuNav { index, moved }
}

pub(crate) fn menu_nav_reset(ctx: &egui::Context, id: Id) {
    ctx.data_mut(|data| {
        data.remove::<usize>(id.with("nav-index"));
        data.remove::<String>(id.with("nav-query"));
    });
}

pub(crate) fn menu_nav_focus(ctx: &egui::Context, id: Id, open: bool, items: &[egui::Response]) {
    let ids: Vec<Id> = items.iter().map(|item| item.id).collect();
    ctx.data_mut(|data| data.insert_temp(id.with("nav-items"), ids.clone()));
    let nav = menu_nav(ctx, id, open, ids.len(), false);
    let Some(index) = nav.index else {
        return;
    };
    if nav.moved {
        let target = ids[index];
        if ctx.memory(|memory| memory.focused()) != Some(target) {
            ctx.memory_mut(|memory| memory.request_focus(target));
        }
        return;
    }
    if ctx.input(|input| input.pointer.delta() != egui::Vec2::ZERO)
        && let Some(focused) = ctx.memory(|memory| memory.focused())
        && ids.contains(&focused)
    {
        ctx.memory_mut(|memory| memory.surrender_focus(focused));
        ctx.data_mut(|data| data.remove::<usize>(id.with("nav-index")));
    }
}

fn menu_nav_clear(ctx: &egui::Context, id: Id) {
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

pub(crate) fn edge(p: &Palette) -> Color32 {
    p.border
}

fn toast_ring_sweep(age: f32, lifetime: f32) -> f32 {
    if lifetime <= 0.0 {
        return 0.0;
    }
    (1.0 - (age / lifetime).clamp(0.0, 1.0)) * std::f32::consts::TAU
}

fn fade(color: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * factor.clamp(0.0, 1.0)) as u8)
}

#[derive(Clone, Copy)]
struct ShellLayout {
    sidebar: f32,
    rail: f32,
    zone: f32,
    chat: f32,
}

fn zone_ceiling(available: f32) -> f32 {
    (available - CHAT_MIN).max(ZONE_MIN)
}

fn bottom_ceiling(body_height: f32) -> f32 {
    (body_height - BOTTOM_CHAT_MIN).max(BOTTOM_MIN)
}

fn solve_layout(
    width: f32,
    sidebar_open: bool,
    has_workspace: bool,
    zone_open: bool,
    sidebar_width: f32,
    zone_width: f32,
) -> ShellLayout {
    let rail = if has_workspace { RAIL_WIDTH } else { 0.0 };
    let row = (width - rail).max(0.0);
    let zone_max = (row - CHAT_MIN).max(0.0);
    let zone = if zone_open {
        zone_width.min(zone_max).max(0.0)
    } else {
        0.0
    };
    let full = sidebar_width.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
    let sidebar = if sidebar_open && row - zone - full >= CHAT_MIN {
        full
    } else {
        0.0
    };
    let chat = (row - zone - sidebar).max(0.0);
    ShellLayout {
        sidebar,
        rail,
        zone,
        chat,
    }
}

fn expanded_layout(width: f32, rail: f32, sidebar_open: bool, sidebar_width: f32) -> (f32, f32) {
    let row = (width - rail).max(0.0);
    let mut sidebar = if sidebar_open {
        sidebar_width.clamp(SIDEBAR_MIN, SIDEBAR_MAX)
    } else {
        0.0
    };
    if sidebar > 0.0 && row - sidebar < ZONE_MIN {
        sidebar = 0.0;
    }
    (sidebar, (row - sidebar).max(0.0))
}

fn expanded_bottom_height(body_height: f32) -> f32 {
    body_height.max(0.0)
}

fn card_ring(p: &Palette) -> Color32 {
    if p.dark {
        p.alpha(Color32::WHITE, 20)
    } else {
        p.alpha(Color32::BLACK, 36)
    }
}

fn zone_card_fill(p: &Palette) -> Color32 {
    if p.dark {
        p.bg_soft
    } else {
        p.mix(p.bg_soft, p.bg, 0.5)
    }
}

#[derive(Default)]
pub struct UiState {
    pub show_settings: bool,
    pub show_changes: bool,
    pub surface: Surface,
    pub zone: PanelZone,
    pub context_open: bool,
    pub expanded: bool,
    pub zone_width: f32,
    pub sidebar_collapsed: bool,
    pub sidebar_folded_by_zone: bool,
    pub sidebar_exposure: f32,
    pub sidebar_dragging: bool,
    pub zone_dragging: bool,
    pub bottom_dragging: bool,
    pub bottom_open: bool,
    pub bottom_height: f32,
    pub bottom_exposure: f32,
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_index: usize,
    pub model_menu: bool,
    pub agent_menu: bool,
    pub header_menu: bool,
    pub session_menu: Option<String>,
    pub rename: Option<(String, String)>,
    pub model_query: String,
    pub expanded_model: Option<String>,
    pub home_model: Option<ModelRef>,
    pub home_agent: Option<String>,
    pub home_draft: String,
    pub home_project: Option<String>,
    pub home_project_menu: bool,
    pub home_project_query: String,
    pub home_allow_all: bool,
    pub focus_home_composer: bool,
    pub settings_endpoint: String,
    pub settings_password: String,
    pub search_open: bool,
    pub session_search: String,
    pub sidebar_menu: bool,
    pub more_menu: bool,
    pub attach_menu: bool,
    pub focus_search: bool,
    pub show_all_projects: HashSet<String>,
    pub in_progress_show_all: bool,
    pub chats_show_all: bool,
    pub timeline_show_all: bool,
    pub search_show_all: bool,
    pub project_menu: Option<String>,
    pub select_mode: bool,
    pub selected_sessions: HashSet<String>,
    pub archive_open: bool,
    pub customize_sections: bool,
    pub confirm_delete: Option<String>,
    pub confirm_delete_bulk: Vec<String>,
    pub confirm_archive_all: Option<String>,
    pub confirm_close_project: Option<String>,
    pub project_rename: Option<(String, String)>,
    pub new_folder: Option<String>,
    pub new_folder_name: String,
    pub folder_rename: Option<(String, String)>,
    pub confirm_delete_folder: Option<String>,
    pub folder_menu: Option<String>,
    pub move_to_folder: Option<String>,
    pub move_folder_name: String,
    pub show_all_folders: HashSet<String>,
    pub add_project: crate::add_project::State,
}

pub struct RustChamber {
    pub model: AppModel,
    pub backend: Backend,
    pub updates: crate::update::UpdateCenter,
    pub settings: Settings,
    pub palette: Palette,
    pub ui: UiState,
    pub md: Markdown,
    system_dark: bool,
    legacy_theme: ThemeMode,
    legacy_scale: f32,
    started: std::time::Instant,
    frames: u64,
    screenshot_taken: bool,
    last_draft_save: f64,
    auto_open: bool,
}

impl RustChamber {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install_fonts(&cc.egui_ctx);
        let mut settings = Settings::load();
        let system_dark = matches!(cc.egui_ctx.system_theme(), Some(egui::Theme::Dark));
        let migrated = settings.theme_migrated;
        settings.resolve_theme_migration(system_dark);
        if migrated {
            settings.appearance.ui_scale = settings.ui_scale;
        }
        if let Ok(theme) = std::env::var("RCHAMBER_THEME") {
            match theme.as_str() {
                "dark" => {
                    settings.theme = ThemeMode::Dark;
                    settings.appearance.brightness = 0.0;
                }
                "light" => {
                    settings.theme = ThemeMode::Light;
                    settings.appearance.brightness = 1.0;
                }
                _ => {
                    settings.theme = ThemeMode::System;
                    settings.appearance.brightness = if system_dark { 0.0 } else { 1.0 };
                }
            }
        }
        settings.appearance.normalize();
        let legacy_theme = settings.theme;
        let legacy_scale = settings.ui_scale;
        let mut palette = theme_store::palette_for(&settings.appearance);
        if let Some(accent) = settings.accent.as_deref().and_then(theme::accent_color) {
            palette = palette.with_accent(accent);
        }
        theme::apply(&cc.egui_ctx, &palette);
        theme::apply_density(&cc.egui_ctx, settings.compact_density);
        theme::apply_typography(&cc.egui_ctx, &settings.appearance);

        let mut model = AppModel::default();
        model.drafts = crate::persist::load_drafts();
        let workspace = std::env::var("RCHAMBER_WORKSPACE")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| settings.workspace.clone())
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
            });
        model.workspace = workspace.clone();
        let mut ui = UiState {
            bottom_height: settings.bottom_height.max(BOTTOM_MIN),
            zone_width: settings.zone_width.max(ZONE_MIN),
            sidebar_exposure: 1.0,
            bottom_exposure: 0.0,
            ..UiState::default()
        };
        if std::env::var("RCHAMBER_ZONE").is_ok_and(|value| value == "open") {
            ui.context_open = true;
        }
        if std::env::var("RCHAMBER_ZONE").is_ok_and(|value| value == "bottom") {
            ui.context_open = true;
            ui.zone = PanelZone::Bottom;
        }
        if let Ok(surface) = std::env::var("RCHAMBER_SURFACE") {
            ui.surface = match surface.as_str() {
                "files" => Surface::Files,
                "git" => Surface::Git,
                "changes" => Surface::Changes,
                "terminal" => Surface::Terminal,
                "context" => Surface::Context,
                _ => Surface::Session,
            };
            ui.context_open = true;
        }
        if std::env::var("RCHAMBER_OPEN_PALETTE").is_ok() {
            ui.palette_open = true;
        }
        if std::env::var("RCHAMBER_OPEN_SETTINGS").is_ok() {
            ui.show_settings = true;
            if let Some(page) = std::env::var("RCHAMBER_SETTINGS_PAGE")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
            {
                settings::open_page(&cc.egui_ctx, page);
            }
        }
        let initial_target = model
            .workspace_store
            .active_project_id
            .clone()
            .filter(|id| model.project_by_id(id).is_some())
            .or_else(|| {
                model
                    .workspace_projects()
                    .first()
                    .map(|project| project.id.clone())
            })
            .unwrap_or_else(|| CHATS_TARGET.to_string());
        ui.home_draft = model
            .workspace_store
            .new_session_drafts
            .get(&initial_target)
            .cloned()
            .unwrap_or_default();
        ui.home_project = Some(initial_target);
        ui.home_allow_all = false;
        if std::env::var("RCHAMBER_OPEN_SEARCH").is_ok() {
            ui.search_open = true;
            ui.focus_search = true;
        }
        if let Ok(panel) = std::env::var("RCHAMBER_OPEN_ADD_PROJECT") {
            let projects = model
                .workspace_projects()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            ui.add_project.begin(workspace.as_deref(), &projects);
            ui.add_project.tab = match panel.as_str() {
                "clone" => crate::add_project::Tab::Clone,
                "projects" => crate::add_project::Tab::Projects,
                "opencode" => crate::add_project::Tab::OpenCode,
                _ => crate::add_project::Tab::Home,
            };
            if ui.add_project.tab == crate::add_project::Tab::Clone {
                ui.add_project.focus_clone = true;
            }
        }
        ui.settings_endpoint = settings.endpoint.clone().unwrap_or_default();
        ui.settings_password = settings.password.clone().unwrap_or_default();

        let ctx = cc.egui_ctx.clone();
        let wake = Arc::new(move || ctx.request_repaint()) as rchamber_app::Wake;
        let config = LaunchConfig {
            workspace,
            endpoint: settings.endpoint.clone(),
            password: settings.password.clone(),
        };
        let backend = Backend::spawn(config, Arc::clone(&wake));
        backend.send(Action::SetTrackedProjects(model.tracked_projects()));
        let updates = crate::update::UpdateCenter::new(crate::update::app_version(), wake);

        Self {
            model,
            backend,
            updates,
            settings,
            palette,
            ui,
            md: Markdown::default(),
            system_dark,
            legacy_theme,
            legacy_scale,
            started: std::time::Instant::now(),
            frames: 0,
            screenshot_taken: false,
            last_draft_save: 0.0,
            auto_open: false,
        }
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.backend.events.try_recv() {
            let connection_failed = matches!(
                &event,
                BackendEvent::Connection(rchamber_app::Connection::Failed(_))
            );
            self.model.apply(event);
            if connection_failed {
                self.ui.show_changes = false;
            }
        }
        self.updates.poll();
        if self.updates.take_open_request() {
            self.ui.show_settings = false;
            self.updates.open();
        }
    }

    pub fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.settings.theme != self.legacy_theme {
            self.legacy_theme = self.settings.theme;
            self.settings.appearance.brightness = match self.settings.theme {
                ThemeMode::Dark => 0.0,
                ThemeMode::Light => 1.0,
                ThemeMode::System => {
                    if self.system_dark {
                        0.0
                    } else {
                        1.0
                    }
                }
            };
        }
        if (self.settings.ui_scale - self.legacy_scale).abs() > 0.0001 {
            self.legacy_scale = self.settings.ui_scale;
            self.settings.appearance.ui_scale = self.settings.ui_scale;
        }
        self.settings.appearance.normalize();
        let mut palette = theme_store::palette_for(&self.settings.appearance);
        if let Some(accent) = self
            .settings
            .accent
            .as_deref()
            .and_then(theme::accent_color)
        {
            palette = palette.with_accent(accent);
        }
        self.palette = palette;
        theme::apply(ctx, &self.palette);
        theme::apply_density(ctx, self.settings.compact_density);
        theme::apply_typography(ctx, &self.settings.appearance);
    }

    pub fn open_session(&mut self, id: String) {
        let already_active = self.model.active_id() == Some(id.as_str());
        if already_active {
            return;
        }
        self.save_home_draft();
        let idle = self
            .model
            .sessions
            .iter()
            .find(|session| session.id == id)
            .and_then(|session| session.time.idle);
        if let Some(session) = self
            .model
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
        {
            self.model.save_draft();
            let draft = self.model.drafts.get(&id).cloned().unwrap_or_default();
            let mut chat = rchamber_app::Chat::new(session);
            chat.draft = draft;
            self.model.active = Some(chat);
            self.ui.show_changes = false;
        }
        self.backend.send(Action::SelectSession(id.clone()));
        if let Some(idle) = idle {
            self.model.mark_viewed_local(&id, idle);
            self.backend.send(Action::MarkSessionViewed(id));
        }
    }

    pub fn set_session_pinned(&mut self, id: &str, pinned: bool) {
        self.model.set_pinned_local(id, pinned);
        self.backend.send(Action::SetSessionPinned {
            id: id.to_string(),
            pinned,
        });
    }

    pub fn begin_new_session(&mut self, project: Option<String>) {
        self.model.save_draft();
        self.save_home_draft();
        let target = project
            .or_else(|| {
                self.model.active.as_ref().and_then(|chat| {
                    self.model
                        .project_for_session(&chat.session)
                        .map(|project| project.id.clone())
                })
            })
            .or_else(|| {
                self.model
                    .workspace_store
                    .active_project_id
                    .clone()
                    .filter(|id| self.model.project_by_id(id).is_some())
            })
            .or_else(|| self.ui.home_project.clone())
            .or_else(|| {
                self.model
                    .workspace_projects()
                    .first()
                    .map(|project| project.id.clone())
            })
            .unwrap_or_else(|| CHATS_TARGET.to_string());
        self.model.clear_active();
        self.ui.home_project = Some(target);
        self.ui.home_draft = self
            .ui
            .home_project
            .as_ref()
            .and_then(|target| self.model.workspace_store.new_session_drafts.get(target))
            .cloned()
            .unwrap_or_default();
        self.ui.home_project_menu = false;
        self.ui.home_project_query.clear();
        self.ui.surface = Surface::Session;
        self.ui.show_changes = false;
        self.ui.header_menu = false;
        self.ui.session_menu = None;
        self.model.composer_focus = true;
        self.ui.focus_home_composer = true;
    }

    pub fn save_home_draft(&mut self) {
        let Some(target) = self.ui.home_project.clone() else {
            return;
        };
        let current = self
            .model
            .workspace_store
            .new_session_drafts
            .get(&target)
            .cloned()
            .unwrap_or_default();
        if self.ui.home_draft == current {
            return;
        }
        if self.ui.home_draft.trim().is_empty() {
            self.model
                .workspace_store
                .new_session_drafts
                .remove(&target);
        } else {
            self.model
                .workspace_store
                .new_session_drafts
                .insert(target, self.ui.home_draft.clone());
        }
        self.model.workspace_store.save();
    }

    pub fn switch_home_target(&mut self, target: String) {
        if self.ui.home_project.as_deref() == Some(target.as_str()) {
            return;
        }
        self.save_home_draft();
        self.ui.home_draft = self
            .model
            .workspace_store
            .new_session_drafts
            .get(&target)
            .cloned()
            .unwrap_or_default();
        self.ui.home_project = Some(target);
        self.ui.home_project_menu = false;
        self.ui.home_project_query.clear();
        self.ui.focus_home_composer = true;
    }

    pub fn home_target_directory(&self) -> String {
        if let Some(target) = self.ui.home_project.as_deref() {
            if target == CHATS_TARGET {
                if let Some(chats) = rchamber_app::WorkspaceStore::chats_directory() {
                    return chats.to_string_lossy().to_string();
                }
            } else if let Some(project) = self.model.project_by_id(target) {
                return project.canonical.clone();
            }
        }
        self.workspace_directory()
    }

    pub fn sync_tracked_projects(&self) {
        self.backend
            .send(Action::SetTrackedProjects(self.model.tracked_projects()));
    }

    pub fn select_project(&mut self, id: &str) {
        let Some(project) = self.model.project_by_id(id).cloned() else {
            return;
        };
        let current = self.workspace_directory();
        self.model.workspace_store.mark_active_project(id);
        self.model.workspace_store.save();
        self.model.workspace = Some(project.canonical.clone());
        self.settings.workspace = Some(project.canonical.clone());
        self.settings.save();
        if current != project.canonical {
            self.backend.send(Action::SetWorkspace(project.canonical));
        }
        self.ui.show_settings = false;
    }

    pub fn set_workspace(&mut self, directory: String) {
        let id = self.model.add_project(&directory, None);
        self.sync_tracked_projects();
        self.select_project(&id);
        self.ui.show_settings = false;
    }

    pub fn workspace_directory(&self) -> String {
        self.model
            .active
            .as_ref()
            .and_then(|chat| chat.session.directory().map(str::to_string))
            .or_else(|| self.model.workspace.clone())
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| ".".into())
    }

    pub fn open_settings(&mut self) {
        self.ui.settings_endpoint = self.settings.endpoint.clone().unwrap_or_default();
        self.ui.settings_password = self.settings.password.clone().unwrap_or_default();
        self.ui.show_settings = true;
    }

    pub fn open_add_project(&mut self) {
        let directory = self
            .model
            .workspace_store
            .active_project_id
            .as_ref()
            .and_then(|id| self.model.project_by_id(id))
            .map(|project| project.canonical.clone())
            .or_else(|| self.model.workspace.clone())
            .or_else(|| {
                self.model
                    .active
                    .as_ref()
                    .and_then(|chat| chat.session.directory().map(str::to_string))
            });
        let projects = self
            .model
            .workspace_projects()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        self.ui.add_project.begin(directory.as_deref(), &projects);
    }

    pub fn check_for_updates(&mut self) {
        if self.updates.available().is_some() {
            self.ui.show_settings = false;
            self.updates.open();
        } else {
            self.updates.check();
            self.model.toast(ToastKind::Info, "Checking for updates…");
        }
    }

    pub fn apply_connection_settings(&mut self) {
        let endpoint = self.ui.settings_endpoint.trim().to_string();
        let password = self.ui.settings_password.trim().to_string();
        self.settings.endpoint = (!endpoint.is_empty()).then_some(endpoint.clone());
        self.settings.password = (!password.is_empty()).then_some(password.clone());
        self.settings.save();
        self.backend.send(Action::Configure {
            endpoint: (!endpoint.is_empty()).then_some(endpoint),
            password: (!password.is_empty()).then_some(password),
            workspace: self.model.workspace.clone(),
        });
        self.ui.show_settings = false;
        self.model
            .toast(ToastKind::Info, "Reconnecting to OpenCode…");
    }

    fn toggle_sidebar(&mut self) {
        self.ui.sidebar_collapsed = !self.ui.sidebar_collapsed;
        self.ui.sidebar_folded_by_zone = false;
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if !ctx.input(|input| input.focused) {
            self.ui.header_menu = false;
            self.ui.session_menu = None;
            self.ui.project_menu = None;
            self.ui.sidebar_menu = false;
            self.ui.more_menu = false;
            self.ui.model_menu = false;
            self.ui.agent_menu = false;
            self.ui.attach_menu = false;
        }
        if self.ui.surface == Surface::Terminal && surfaces::terminal::has_focus(ctx) {
            return;
        }
        let command = ctx.input(|input| input.modifiers.command);
        if ctx.input(|input| {
            input.key_pressed(egui::Key::K) && (input.modifiers.command || input.modifiers.ctrl)
        }) || ctx.input(|input| {
            input.key_pressed(egui::Key::P) && (input.modifiers.command || input.modifiers.ctrl)
        }) {
            self.ui.palette_open = !self.ui.palette_open;
            self.ui.palette_query.clear();
            self.ui.palette_index = 0;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::B) && input.modifiers.command) {
            self.toggle_sidebar();
            if self.ui.sidebar_collapsed {
                self.ui.search_open = false;
                self.ui.session_search.clear();
                self.model.search.clear();
            }
        }
        if ctx.input(|input| input.key_pressed(egui::Key::J) && input.modifiers.command) {
            self.ui.surface = Surface::Terminal;
            self.ui.context_open = true;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::F) && input.modifiers.command) {
            self.ui.sidebar_collapsed = false;
            self.ui.search_open = true;
            self.ui.focus_search = true;
        }
        if ctx.input(|input| {
            input.key_pressed(egui::Key::M) && input.modifiers.command && input.modifiers.shift
        }) {
            self.ui.model_menu = true;
        }
        if ctx.input(|input| {
            input.key_pressed(egui::Key::ArrowLeft)
                && input.modifiers.command
                && input.modifiers.alt
        }) {
            self.cycle_session(-1);
        }
        if ctx.input(|input| {
            input.key_pressed(egui::Key::ArrowRight)
                && input.modifiers.command
                && input.modifiers.alt
        }) {
            self.cycle_session(1);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::N) && (command || input.modifiers.ctrl)) {
            self.begin_new_session(None);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Comma) && command) {
            self.open_settings();
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape))
            && !self.ui.palette_open
            && !self.ui.show_settings
        {
            let dismissed = self.ui.show_changes
                || self.ui.header_menu
                || self.ui.session_menu.is_some()
                || self.ui.project_menu.is_some()
                || self.ui.sidebar_menu
                || self.ui.more_menu
                || self.ui.model_menu
                || self.ui.agent_menu
                || self.ui.attach_menu
                || self.ui.archive_open
                || self.ui.customize_sections
                || self.ui.select_mode
                || self.ui.confirm_delete.is_some()
                || !self.ui.confirm_delete_bulk.is_empty()
                || self.ui.confirm_archive_all.is_some()
                || self.ui.confirm_close_project.is_some()
                || self.ui.project_rename.is_some()
                || self.ui.add_project.open;
            self.ui.show_changes = false;
            self.ui.header_menu = false;
            self.ui.session_menu = None;
            self.ui.project_menu = None;
            self.ui.sidebar_menu = false;
            self.ui.more_menu = false;
            self.ui.model_menu = false;
            self.ui.agent_menu = false;
            self.ui.attach_menu = false;
            self.ui.archive_open = false;
            self.ui.customize_sections = false;
            self.ui.select_mode = false;
            self.ui.selected_sessions.clear();
            self.ui.confirm_delete = None;
            self.ui.confirm_delete_bulk.clear();
            self.ui.confirm_archive_all = None;
            self.ui.confirm_close_project = None;
            self.ui.project_rename = None;
            self.ui.add_project.open = false;
            if !dismissed {
                self.ui.expanded = false;
            }
        }
    }

    fn cycle_session(&mut self, delta: i32) {
        let sessions: Vec<String> = self
            .model
            .sorted_sessions()
            .into_iter()
            .map(|session| session.id.clone())
            .collect();
        if sessions.is_empty() {
            return;
        }
        let current = self
            .model
            .active_id()
            .and_then(|id| sessions.iter().position(|candidate| candidate == id))
            .unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(sessions.len() as i32) as usize;
        self.open_session(sessions[next].clone());
    }

    fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some((id, mut title)) = self.ui.rename.take() else {
            return;
        };
        let p = self.palette.clone();
        let mut save = false;
        let mut cancel = false;
        let frame = egui::Frame::new()
            .fill(p.bg)
            .corner_radius(theme::radius::XL)
            .inner_margin(Margin::same(20));
        let modal = egui::Modal::new(Id::unique("rename-modal"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(
                    RichText::new("Rename session")
                        .font(theme::font_semibold(15.0))
                        .color(p.fg),
                );
                ui.add_space(8.0);
                let field_id = Id::unique("rename-field");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut title)
                        .id(field_id)
                        .font(theme::font(13.0))
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(8, 7)),
                );
                if !response.has_focus() {
                    response.request_focus();
                }
                if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    save = true;
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, "Save", Some(Icon::Check), &p).clicked() {
                        save = true;
                    }
                    if widgets::ghost_button(ui, "Cancel", None, &p).clicked() {
                        cancel = true;
                    }
                });
            });
        if modal.backdrop_response.clicked() || cancel {
            return;
        }
        if save && !title.trim().is_empty() {
            self.backend.send(Action::RenameSession {
                id,
                title: title.trim().to_string(),
            });
        } else {
            self.ui.rename = Some((id, title));
        }
    }

    fn toasts(&mut self, ctx: &egui::Context, rail: f32) {
        let exits_id = Id::unique("toast-exits");
        let pause_id = Id::unique("toast-pause");
        if self.model.toasts.is_empty() {
            ctx.data_mut(|data| {
                data.remove_temp::<HashMap<u64, f64>>(exits_id);
                data.remove_temp::<f64>(pause_id);
            });
            self.model.toasts_paused = false;
            return;
        }
        let p = self.palette.clone();
        let now = rchamber_app::now_ms();
        let lifetime = (rchamber_app::state::TOAST_LIFETIME_MS / 1000.0) as f32;
        let mut exits = ctx
            .data(|data| data.get_temp::<HashMap<u64, f64>>(exits_id))
            .unwrap_or_default();
        exits.retain(|id, _| self.model.toasts.iter().any(|toast| toast.id == *id));
        let exit_time = motion_time(ctx, TOAST_EXIT_SECONDS);
        let mut leave: Vec<u64> = Vec::new();
        let mut remove: Vec<u64> = Vec::new();
        let mut cards: Vec<egui::Rect> = Vec::new();
        let mut hovered = false;
        let toast_area = egui::Area::new(Id::unique("toasts"))
            .order(egui::Order::Foreground)
            .anchor(
                Align2::RIGHT_BOTTOM,
                Vec2::new(-(TOAST_INSET + rail), -TOAST_INSET),
            )
            .show(ctx, |ui| {
                for toast in self.model.toasts.iter() {
                    let icon = match toast.kind {
                        ToastKind::Info => Icon::Info,
                        ToastKind::Success => Icon::Check,
                        ToastKind::Warning => Icon::Warning,
                        ToastKind::Error => Icon::Warning,
                    };
                    let age = ((now - toast.created) / 1000.0) as f32;
                    let eased = ease_shell((age / 0.18).clamp(0.0, 1.0));
                    let exiting = exits.get(&toast.id).copied();
                    if exiting.is_none() && age >= lifetime {
                        leave.push(toast.id);
                    }
                    let opacity = match exiting {
                        Some(started) => {
                            let progress = (((now - started) / 1000.0) as f32
                                / exit_time.max(0.001))
                            .clamp(0.0, 1.0);
                            if progress >= 1.0 {
                                remove.push(toast.id);
                            }
                            1.0 - ease_shell(progress)
                        }
                        None => eased,
                    };
                    ui.add_space(2.0 + eased * 6.0);
                    ui.scope(|ui| {
                        ui.set_opacity(opacity);
                        let card = egui::Frame::new()
                            .fill(p.popover)
                            .corner_radius(theme::radius::XL)
                            .inner_margin(Margin {
                                left: TOAST_PAD_X,
                                right: TOAST_PAD_X,
                                top: TOAST_PAD_Y,
                                bottom: TOAST_PAD_Y,
                            })
                            .shadow(egui::epaint::Shadow {
                                offset: [0, 8],
                                blur: 22,
                                spread: 0,
                                color: p.shadow,
                            })
                            .show(ui, |ui| {
                                ui.set_width(TOAST_WIDTH - (TOAST_PAD_X as f32) * 2.0);
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let (rect, _) =
                                        ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                                    icons::paint(ui, icon, rect, p.fg_muted);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&toast.message)
                                                .font(theme::font_medium(13.5))
                                                .color(p.fg),
                                        )
                                        .wrap()
                                        .selectable(false),
                                    );
                                });
                            });
                        let rect = card.response.rect;
                        cards.push(rect);
                        let close_rect = egui::Rect::from_center_size(
                            egui::Pos2::new(rect.left() + 4.0, rect.top() + 4.0),
                            Vec2::splat(20.0),
                        );
                        let close = ui.interact(
                            close_rect,
                            Id::unique(("toast-close", toast.id)),
                            Sense::click(),
                        );
                        let disc = if p.dark { p.card } else { p.bg };
                        ui.painter().circle_filled(close_rect.center(), 9.0, disc);
                        ui.painter().circle_stroke(
                            close_rect.center(),
                            9.0,
                            Stroke::new(theme::HAIRLINE, p.raised_edge()),
                        );
                        if !motion::reduced_motion(ui.ctx()) {
                            let sweep = toast_ring_sweep(age, lifetime);
                            if sweep > 0.0 {
                                let center = close_rect.center();
                                let radius = 8.0;
                                let start = -std::f32::consts::FRAC_PI_2;
                                let steps = 40;
                                let points: Vec<egui::Pos2> = (0..=steps)
                                    .map(|index| {
                                        let angle = start + sweep * index as f32 / steps as f32;
                                        egui::Pos2::new(
                                            center.x + radius * angle.cos(),
                                            center.y + radius * angle.sin(),
                                        )
                                    })
                                    .collect();
                                ui.painter().add(egui::Shape::line(
                                    points,
                                    Stroke::new(1.25, p.scale_alpha(p.fg_muted, 0.55)),
                                ));
                            }
                        }
                        icons::paint(
                            ui,
                            Icon::Close,
                            egui::Rect::from_center_size(close_rect.center(), Vec2::splat(9.0)),
                            if close.hovered() { p.fg } else { p.fg_muted },
                        );
                        if close.clicked() {
                            leave.push(toast.id);
                        }
                    });
                }
                let stack = cards
                    .iter()
                    .copied()
                    .reduce(|left, right| left.union(right));
                hovered = stack.is_some_and(|rect| ui.rect_contains_pointer(rect));
            });
        ctx.move_to_top(toast_area.response.layer_id);
        for id in leave {
            if exit_time > 0.0 {
                exits.entry(id).or_insert(now);
            } else {
                remove.push(id);
            }
        }
        if !remove.is_empty() {
            self.model
                .toasts
                .retain(|toast| !remove.contains(&toast.id));
        }
        let paused = hovered && !self.model.toasts.is_empty();
        let held_since = ctx.data(|data| data.get_temp::<f64>(pause_id));
        match (paused, held_since) {
            (true, None) => {
                ctx.data_mut(|data| data.insert_temp(pause_id, now));
            }
            (false, Some(started)) => {
                let held = (now - started).max(0.0);
                if held > 0.0 {
                    for toast in &mut self.model.toasts {
                        toast.created += held;
                    }
                }
                ctx.data_mut(|data| data.remove_temp::<f64>(pause_id));
            }
            _ => {}
        }
        self.model.toasts_paused = paused;
        let exiting = !exits.is_empty();
        ctx.data_mut(|data| data.insert_temp(exits_id, exits));
        if exiting {
            ctx.request_repaint();
        } else if self.model.toasts_paused {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn panel_drop_overlay(ctx: &egui::Context, p: &Palette, drag: &TabDrag, target: egui::Rect) {
    let pointer = ctx.input(|input| input.pointer.interact_pos());
    let hovered = pointer.is_some_and(|pos| target.contains(pos));
    let p = p.clone();
    egui::Area::new(Id::unique("panel-drop-overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::Pos2::ZERO)
        .interactable(false)
        .show(ctx, |ui| {
            let screen = ui.ctx().content_rect();
            ui.painter()
                .rect_filled(screen, 0.0, p.alpha(p.overlay, 102));
            let t = motion::hover_fade(ui, Id::unique("panel-drop-hover"), hovered);
            let radius = CornerRadius::same(theme::radius::PANEL);
            ui.painter().rect_filled(target, radius, p.alpha(p.bg, 236));
            ui.painter().rect_stroke(
                target,
                radius,
                Stroke::new(theme::BORDER, p.mix(p.border, p.primary, t)),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                target.center(),
                Align2::CENTER_CENTER,
                PanelZone::Bottom.move_label(),
                theme::font_medium(13.5),
                if hovered { p.fg } else { p.fg_muted },
            );
            let Some(pos) = pointer else {
                return;
            };
            let galley = ui
                .painter()
                .layout_no_wrap(drag.label.clone(), theme::font(12.5), p.fg);
            let size = Vec2::new(galley.size().x + 16.0, 22.0);
            let origin = egui::Pos2::new(
                (pos.x + 14.0)
                    .min(screen.right() - size.x - 4.0)
                    .max(screen.left() + 4.0),
                (pos.y + 14.0)
                    .min(screen.bottom() - size.y - 4.0)
                    .max(screen.top() + 4.0),
            );
            let chip = egui::Rect::from_min_size(origin, size);
            ui.painter()
                .rect_filled(chip, CornerRadius::same(theme::radius::MD), p.popover);
            ui.painter().rect_stroke(
                chip,
                CornerRadius::same(theme::radius::MD),
                Stroke::new(theme::HAIRLINE, p.border),
                egui::StrokeKind::Inside,
            );
            ui.painter().galley(
                egui::Pos2::new(chip.left() + 8.0, chip.center().y - galley.size().y * 0.5),
                galley,
                p.fg,
            );
        });
}

impl eframe::App for RustChamber {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.palette.bg.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self) {
        self.model.save_draft();
        self.save_home_draft();
        self.model.workspace_store.save();
        crate::persist::save_drafts(&self.model.drafts);
        self.backend.send(Action::Shutdown);
        std::thread::sleep(Duration::from_millis(120));
    }

    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        self.poll_events();
        self.model.tick();

        if (self.settings.ui_scale - self.legacy_scale).abs() > 0.0001 {
            self.legacy_scale = self.settings.ui_scale;
            self.settings.appearance.ui_scale = self.settings.ui_scale;
            self.apply_theme(&ctx);
            self.settings.save();
        }

        if !self.auto_open
            && self.model.active.is_none()
            && let Ok(id) = std::env::var("RCHAMBER_OPEN_SESSION")
            && !id.is_empty()
            && self.model.sessions.iter().any(|session| session.id == id)
            && !self.model.models.is_empty()
        {
            self.auto_open = true;
            self.open_session(id);
        }

        let system_dark = matches!(ctx.system_theme(), Some(egui::Theme::Dark));
        if system_dark != self.system_dark {
            self.system_dark = system_dark;
            if self.settings.theme == ThemeMode::System {
                let previous = if system_dark { 1.0 } else { 0.0 };
                if (self.settings.appearance.brightness - previous).abs() < 0.001 {
                    self.settings.appearance.brightness = if system_dark { 0.0 } else { 1.0 };
                    self.apply_theme(&ctx);
                }
            }
        }

        self.keyboard(&ctx);
        if !self.ui.context_open {
            self.ui.expanded = false;
        }

        let busy = self.model.active.as_ref().map(|c| c.busy).unwrap_or(false);
        if busy || matches!(self.model.connection, rchamber_app::Connection::Connecting) {
            ctx.request_repaint_after(Duration::from_millis(90));
        }

        let p = self.palette.clone();
        let has_workspace = self
            .model
            .workspace
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            || self
                .model
                .active
                .as_ref()
                .and_then(|chat| chat.session.directory())
                .is_some();
        let width = root.available_width();
        let sidebar_full = self.settings.sidebar_width.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
        let mut sidebar_wanted = !self.ui.sidebar_collapsed;
        let expanded = self.ui.expanded && self.ui.context_open;
        let right_zone = self.ui.context_open && self.ui.zone == PanelZone::Right && !expanded;
        let bottom_zone = self.ui.context_open && self.ui.zone == PanelZone::Bottom && !expanded;
        let mut target = solve_layout(
            width,
            sidebar_wanted,
            has_workspace,
            right_zone,
            sidebar_full,
            self.ui.zone_width,
        );
        if right_zone && sidebar_wanted && target.sidebar == 0.0 {
            self.ui.sidebar_collapsed = true;
            self.ui.sidebar_folded_by_zone = true;
            target = solve_layout(
                width,
                false,
                has_workspace,
                right_zone,
                sidebar_full,
                self.ui.zone_width,
            );
        } else if !right_zone && self.ui.sidebar_folded_by_zone {
            self.ui.sidebar_collapsed = false;
            self.ui.sidebar_folded_by_zone = false;
            target = solve_layout(
                width,
                true,
                has_workspace,
                right_zone,
                sidebar_full,
                self.ui.zone_width,
            );
        }
        sidebar_wanted = !self.ui.sidebar_collapsed;
        let expanded_right = expanded && self.ui.zone == PanelZone::Right;
        let expanded_bottom = expanded && self.ui.zone == PanelZone::Bottom;
        if expanded_right {
            let (sidebar, zone) = expanded_layout(width, target.rail, sidebar_wanted, sidebar_full);
            target.sidebar = sidebar;
            target.zone = zone;
        }
        let body_height = (ctx.content_rect().height() - header::HEADER_HEIGHT).max(0.0);

        let time = motion_time(&ctx, SHELL_MS);
        let sidebar_duration = if self.ui.sidebar_dragging { 0.0 } else { time };
        let sidebar = Motion::new("shell/sidebar-width")
            .with_duration(sidebar_duration)
            .with_easing(Curve::Standard)
            .value(&ctx, target.sidebar);
        let zone_duration = if self.ui.zone_dragging { 0.0 } else { time };
        let zone = Motion::new("shell/zone-width")
            .with_duration(zone_duration)
            .with_easing(Curve::Standard)
            .value(&ctx, target.zone);
        let bottom_target = if expanded_bottom {
            expanded_bottom_height(body_height)
        } else if bottom_zone {
            self.ui
                .bottom_height
                .clamp(BOTTOM_MIN, bottom_ceiling(body_height))
        } else {
            0.0
        };
        let bottom_duration = if self.ui.bottom_dragging { 0.0 } else { time };
        let bottom = Motion::new("shell/bottom-height")
            .with_duration(bottom_duration)
            .with_easing(Curve::Standard)
            .value(&ctx, bottom_target);
        let sidebar_exposure = motion::Animated::new("shell/sidebar-exposure")
            .with_duration(time)
            .with_easing(Curve::EaseOut)
            .bool(&ctx, target.sidebar > 0.0);
        let zone_exposure = motion::Animated::new("shell/zone-exposure")
            .with_duration(time)
            .with_easing(Curve::EaseOut)
            .bool(&ctx, target.zone > 0.0);
        let bottom_exposure = motion::Animated::new("shell/bottom-exposure")
            .with_duration(time)
            .with_easing(Curve::EaseOut)
            .bool(&ctx, bottom_zone || expanded_bottom);
        self.ui.sidebar_exposure = sidebar_exposure;
        self.ui.bottom_open = bottom_zone || expanded_bottom;
        self.ui.bottom_exposure = bottom_exposure;

        let layout = ShellLayout {
            sidebar,
            rail: target.rail,
            zone,
            chat: (width - sidebar - target.rail - zone).max(0.0),
        };
        if self.frames < 2 && std::env::var("RCHAMBER_LAYOUT_LOG").is_ok() {
            eprintln!(
                "layout sidebar={} rail={} zone={} chat={}",
                layout.sidebar, layout.rail, layout.zone, layout.chat
            );
        }

        let card_fill = zone_card_fill(&p);
        let ring = card_ring(&p);

        if layout.sidebar > 0.01 {
            egui::Panel::left("sidebar")
                .resizable(false)
                .exact_size(layout.sidebar)
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(p.bg_soft))
                .show(root, |ui| {
                    ui.set_opacity(sidebar_exposure);
                    let full = egui::Rect::from_min_size(
                        ui.max_rect().min,
                        Vec2::new(sidebar_full, ui.max_rect().height()),
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(full)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    sidebar::show(&mut child, self);
                });
        }

        egui::Panel::top("shell-header")
            .exact_size(header::HEADER_HEIGHT)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin {
                left: 12,
                right: 10,
                top: 0,
                bottom: 0,
            }))
            .show(root, |ui| {
                header::show(ui, self);
            });

        if layout.rail > 0.0 {
            egui::Panel::right("context-rail")
                .resizable(false)
                .exact_size(layout.rail)
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(p.bg))
                .show(root, |ui| {
                    rail::show(ui, self);
                });
        }

        if layout.zone > 0.01 {
            egui::Panel::right("context-panel")
                .resizable(false)
                .exact_size(layout.zone)
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin {
                    left: 8,
                    right: 6,
                    top: 0,
                    bottom: 8,
                }))
                .show(root, |ui| {
                    let card = ui.max_rect();
                    ui.painter().rect_filled(
                        card,
                        CornerRadius::same(CARD_RADIUS),
                        fade(card_fill, zone_exposure),
                    );
                    ui.set_opacity(zone_exposure);
                    if self.ui.zone == PanelZone::Right {
                        surfaces::set_panel_zone(ui.ctx(), PanelZone::Right);
                        match self.ui.surface {
                            Surface::Session => context_panel::show(ui, self),
                            Surface::Context => crate::context_dialog::show_panel(ui, self),
                            Surface::Changes => changes::show(ui, self),
                            Surface::Git => surfaces::git::show(ui, self),
                            Surface::Files => surfaces::files::show(ui, self),
                            Surface::Terminal => surfaces::terminal::show(ui, self),
                        }
                    }
                    ui.painter().rect_stroke(
                        card,
                        CornerRadius::same(CARD_RADIUS),
                        Stroke::new(0.5, fade(ring, zone_exposure)),
                        egui::StrokeKind::Inside,
                    );
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg))
            .show(root, |ui| {
                if bottom > 1.0 {
                    egui::Panel::bottom("zone-bottom")
                        .resizable(false)
                        .exact_size(bottom)
                        .show_separator_line(false)
                        .frame(egui::Frame::new().inner_margin(Margin {
                            left: 8,
                            right: 6,
                            top: 8,
                            bottom: 8,
                        }))
                        .show(ui, |ui| {
                            let card = ui.max_rect();
                            ui.painter().rect_filled(
                                card,
                                CornerRadius::same(CARD_RADIUS),
                                fade(card_fill, bottom_exposure),
                            );
                            ui.set_opacity(bottom_exposure);
                            if self.ui.zone == PanelZone::Bottom {
                                surfaces::set_panel_zone(ui.ctx(), PanelZone::Bottom);
                                match self.ui.surface {
                                    Surface::Session => context_panel::show(ui, self),
                                    Surface::Context => crate::context_dialog::show_panel(ui, self),
                                    Surface::Changes => changes::show(ui, self),
                                    Surface::Git => surfaces::git::show(ui, self),
                                    Surface::Files => surfaces::files::show(ui, self),
                                    Surface::Terminal => surfaces::terminal::show(ui, self),
                                }
                            }
                            ui.painter().rect_stroke(
                                card,
                                CornerRadius::same(CARD_RADIUS),
                                Stroke::new(0.5, fade(ring, bottom_exposure)),
                                egui::StrokeKind::Inside,
                            );
                        });
                }
                let remaining = ui.available_rect_before_wrap();
                if remaining.height() > 1.0 && !expanded {
                    let mut body = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(remaining)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    if self.model.active.is_some() {
                        chat::show(&mut body, self);
                    } else {
                        home::show(&mut body, self);
                    }
                }
            });

        if layout.sidebar > 0.5 && sidebar_exposure > 0.99 {
            let handle = egui::Rect::from_min_max(
                egui::pos2(sidebar - RESIZE_HANDLE * 0.5, 0.0),
                egui::pos2(sidebar + RESIZE_HANDLE * 0.5, ctx.content_rect().bottom()),
            );
            let response = egui::Area::new(Id::unique("shell/sidebar-resize"))
                .order(egui::Order::Middle)
                .fixed_pos(handle.min)
                .show(&ctx, |ui| {
                    let (rect, response) = ui.allocate_exact_size(handle.size(), Sense::drag());
                    if response.hovered() || response.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        ui.painter().rect_filled(
                            rect,
                            0.0,
                            if response.dragged() {
                                p.border_hover
                            } else {
                                fade(p.border_hover, 0.8)
                            },
                        );
                    }
                    response
                })
                .inner;
            if response.dragged() {
                self.settings.sidebar_width = (self.settings.sidebar_width
                    + response.drag_delta().x)
                    .clamp(SIDEBAR_MIN, SIDEBAR_MAX);
            }
            self.ui.sidebar_dragging = response.dragged();
            if response.drag_stopped() {
                self.settings.save();
            }
        } else {
            self.ui.sidebar_dragging = false;
        }

        if layout.zone > 0.5 && zone_exposure > 0.99 && !expanded {
            let zone_left = ctx.content_rect().right() - layout.rail - layout.zone;
            let handle = egui::Rect::from_min_max(
                egui::pos2(zone_left + 4.0, header::HEADER_HEIGHT),
                egui::pos2(
                    zone_left + 4.0 + ZONE_HANDLE,
                    ctx.content_rect().bottom() - 8.0,
                ),
            );
            let response = egui::Area::new(Id::unique("shell/zone-resize"))
                .order(egui::Order::Middle)
                .fixed_pos(handle.min)
                .show(&ctx, |ui| {
                    let (_, response) = ui.allocate_exact_size(handle.size(), Sense::drag());
                    if response.hovered() || response.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    response
                })
                .inner;
            if response.dragged() {
                let ceiling = zone_ceiling(width - layout.rail);
                self.ui.zone_width =
                    (self.ui.zone_width - response.drag_delta().x).clamp(ZONE_MIN, ceiling);
            }
            self.ui.zone_dragging = response.dragged();
            if response.drag_stopped() {
                self.settings.zone_width = self.ui.zone_width;
                self.settings.save();
            }
        } else {
            self.ui.zone_dragging = false;
        }

        if bottom_exposure > 0.99 && !expanded {
            let handle = egui::Rect::from_min_max(
                egui::pos2(layout.sidebar, ctx.content_rect().bottom() - bottom + 4.0),
                egui::pos2(
                    ctx.content_rect().right() - layout.rail,
                    ctx.content_rect().bottom() - bottom + 4.0 + BOTTOM_HANDLE,
                ),
            );
            let response = egui::Area::new(Id::unique("shell/bottom-resize"))
                .order(egui::Order::Middle)
                .fixed_pos(handle.min)
                .show(&ctx, |ui| {
                    let (_, response) = ui.allocate_exact_size(handle.size(), Sense::drag());
                    if response.hovered() || response.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }
                    response
                })
                .inner;
            if response.dragged() {
                let ceiling = bottom_ceiling(body_height);
                self.ui.bottom_height =
                    (self.ui.bottom_height - response.drag_delta().y).clamp(BOTTOM_MIN, ceiling);
            }
            self.ui.bottom_dragging = response.dragged();
            if response.drag_stopped() {
                self.settings.bottom_height = self.ui.bottom_height;
                self.settings.save();
            }
        } else {
            self.ui.bottom_dragging = false;
        }

        if let Some(zone) = surfaces::take_zone_request(&ctx) {
            self.ui.zone = zone;
            self.ui.context_open = true;
        }
        if surfaces::take_close_request(&ctx) {
            self.ui.context_open = false;
            self.ui.expanded = false;
        }

        if let Some(drag) = surfaces::tab_drag(&ctx) {
            let content = ctx.content_rect();
            let left = layout.sidebar + 12.0;
            let right = (content.right() - layout.rail - layout.zone - 12.0).max(left + 24.0);
            let target = egui::Rect::from_min_max(
                egui::pos2(left, content.bottom() - DROP_TARGET_HEIGHT - 12.0),
                egui::pos2(right, content.bottom() - 12.0),
            );
            let show_target = self.ui.zone == PanelZone::Right && !self.ui.expanded;
            if show_target && drag.outside && (drag.active || drag.released.is_some()) {
                panel_drop_overlay(&ctx, &p, &drag, target);
            }
            if let Some(pos) = drag.released {
                if show_target && target.contains(pos) {
                    self.ui.zone = PanelZone::Bottom;
                    self.ui.context_open = true;
                }
                surfaces::clear_tab_drag(&ctx);
            } else if !ctx.input(|input| input.pointer.any_down()) {
                surfaces::clear_tab_drag(&ctx);
            }
        }

        crate::chrome::titlebar_controls(&ctx, self, &p);

        if self.ui.show_settings {
            settings::show(&ctx, self);
        }
        if self.ui.palette_open {
            palette::show(&ctx, self);
        }
        crate::update::show(&ctx, self);
        self.rename_dialog(&ctx);
        crate::add_project::show(&ctx, self);
        self.toasts(&ctx, layout.rail);

        self.frames += 1;
        if !self.screenshot_taken
            && let Ok(path) = std::env::var("RCHAMBER_SCREENSHOT")
        {
            if self.frames >= 45 && self.started.elapsed() > Duration::from_millis(15000) {
                self.screenshot_taken = true;
                let _ = std::fs::remove_file(&path);
                let repaint = ctx.clone();
                ctx.request_screenshot(move |image| {
                    let [width, height] = image.size;
                    let mut rgba = Vec::with_capacity(width * height * 4);
                    for pixel in &image.pixels {
                        rgba.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
                    }
                    if let Some(buffer) =
                        image::RgbaImage::from_raw(width as u32, height as u32, rgba)
                    {
                        let _ = buffer.save(&path);
                    }
                    repaint.send_viewport_cmd(egui::ViewportCommand::Close);
                });
                ctx.request_repaint();
            } else {
                ctx.request_repaint();
            }
        }

        if self.model.drafts_changed_at > self.last_draft_save + 500.0 {
            let now = rchamber_app::now_ms();
            self.last_draft_save = now;
            self.model.drafts_changed_at = now;
            crate::persist::save_drafts(&self.model.drafts);
        }

        crate::chrome::place_traffic_lights(frame, &ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(width: f32, sidebar_open: bool, zone_open: bool) -> (f32, f32, f32, f32) {
        let layout = solve_layout(width, sidebar_open, true, zone_open, 280.0, 640.0);
        (layout.sidebar, layout.rail, layout.zone, layout.chat)
    }

    #[test]
    fn zone_closed_uses_full_residual() {
        assert_eq!(plan(1440.0, true, false), (280.0, 44.0, 0.0, 1116.0));
    }

    #[test]
    fn zone_folds_sidebar_before_displacing() {
        assert_eq!(plan(900.0, true, true), (0.0, 44.0, 536.0, 320.0));
        assert_eq!(plan(1024.0, true, true), (0.0, 44.0, 640.0, 340.0));
        assert_eq!(plan(1320.0, true, true), (280.0, 44.0, 640.0, 356.0));
    }

    #[test]
    fn zone_clamps_to_the_row_and_keeps_the_chat_minimum() {
        assert_eq!(plan(600.0, true, true), (0.0, 44.0, 236.0, 320.0));
        assert_eq!(plan(400.0, true, true), (0.0, 44.0, 36.0, 320.0));
    }

    #[test]
    fn reference_easing_hits_endpoints() {
        assert!(ease_shell(0.0).abs() < 1e-4);
        assert!((ease_shell(1.0) - 1.0).abs() < 1e-4);
        assert!(ease_shell(0.5) > 0.4);
        assert!(ease_shell(0.5) < 0.8);
    }

    #[test]
    fn toast_width_matches_reference() {
        assert_eq!(TOAST_WIDTH, 356.0);
        assert_eq!(TOAST_INSET, 24.0);
        assert_eq!(TOAST_PAD_X, 14);
        assert_eq!(TOAST_PAD_Y, 12);
    }

    #[test]
    fn zone_card_uses_the_muted_surface() {
        let dark = Palette::dark();
        assert_eq!(zone_card_fill(&dark), dark.bg_soft);
        let light = Palette::light();
        assert_eq!(zone_card_fill(&light), Color32::from_rgb(0xFA, 0xF9, 0xF7));
    }

    #[test]
    fn panel_size_bounds_match_the_reference() {
        assert_eq!(ZONE_MIN, 320.0);
        assert_eq!(CHAT_MIN, 320.0);
        assert_eq!(BOTTOM_MIN, 160.0);
        assert_eq!(BOTTOM_CHAT_MIN, 200.0);
        assert_eq!(zone_ceiling(1600.0), 1280.0);
        assert_eq!(zone_ceiling(500.0), 320.0);
        assert_eq!(bottom_ceiling(900.0), 700.0);
        assert_eq!(bottom_ceiling(300.0), 160.0);
    }

    #[test]
    fn zone_width_is_not_capped_below_the_chat_minimum() {
        let layout = solve_layout(2000.0, false, true, true, 280.0, 1000.0);
        assert_eq!(layout.zone, 1000.0);
        assert_eq!(layout.chat, 956.0);
        let layout = solve_layout(2600.0, false, true, true, 280.0, 1400.0);
        assert_eq!(layout.zone, 1400.0);
        assert_eq!(layout.chat, 1156.0);
    }

    #[test]
    fn expanded_zone_fills_the_row_right_of_the_sidebar() {
        assert_eq!(expanded_layout(1440.0, 44.0, true, 280.0), (280.0, 1116.0));
        assert_eq!(expanded_layout(1440.0, 44.0, false, 280.0), (0.0, 1396.0));
        assert_eq!(expanded_layout(900.0, 44.0, true, 280.0), (280.0, 576.0));
        assert_eq!(expanded_layout(600.0, 44.0, true, 280.0), (0.0, 556.0));
        assert_eq!(expanded_layout(300.0, 44.0, true, 280.0), (0.0, 256.0));
    }

    #[test]
    fn expanded_bottom_takes_the_whole_chat_column() {
        assert_eq!(expanded_bottom_height(900.0), 900.0);
        assert!(expanded_bottom_height(900.0) > bottom_ceiling(900.0));
        assert_eq!(expanded_bottom_height(0.0), 0.0);
    }

    #[test]
    fn toast_lifetime_is_one_value_for_every_kind() {
        assert_eq!(rchamber_app::state::TOAST_LIFETIME_MS, 4_000.0);
        for kind in [
            ToastKind::Info,
            ToastKind::Success,
            ToastKind::Warning,
            ToastKind::Error,
        ] {
            let mut model = AppModel::default();
            model.toast(kind, "message");
            model.toasts[0].created = rchamber_app::now_ms() - 3_900.0;
            model.expire_toasts();
            assert_eq!(model.toasts.len(), 1);
            model.toasts[0].created = rchamber_app::now_ms() - 4_200.0;
            model.expire_toasts();
            assert!(model.toasts.is_empty());
        }
    }

    #[test]
    fn toast_ring_runs_down_over_the_toast_lifetime() {
        let lifetime = (rchamber_app::state::TOAST_LIFETIME_MS / 1000.0) as f32;
        let full = std::f32::consts::TAU;
        assert!((toast_ring_sweep(0.0, lifetime) - full).abs() < 1e-4);
        assert!((toast_ring_sweep(lifetime * 0.5, lifetime) - full * 0.5).abs() < 1e-4);
        assert_eq!(toast_ring_sweep(lifetime, lifetime), 0.0);
        assert_eq!(toast_ring_sweep(lifetime + 1.0, lifetime), 0.0);
        assert_eq!(toast_ring_sweep(1.0, 0.0), 0.0);
    }

    #[test]
    fn toast_expiry_holds_while_paused() {
        let mut model = AppModel::default();
        model.toast(ToastKind::Error, "message");
        model.toasts[0].created = rchamber_app::now_ms() - 10_000.0;
        model.toasts_paused = true;
        model.expire_toasts();
        assert_eq!(model.toasts.len(), 1);
        model.toasts_paused = false;
        model.expire_toasts();
        assert!(model.toasts.is_empty());
    }

    #[test]
    fn panel_geometry_persists_in_settings() {
        let settings = Settings {
            zone_width: 512.0,
            bottom_height: 260.0,
            ..Settings::default()
        };
        let text = serde_json::to_string(&settings).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.zone_width, 512.0);
        assert_eq!(back.bottom_height, 260.0);
        let defaults: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults.zone_width, 640.0);
        assert_eq!(defaults.bottom_height, 320.0);
    }
}
