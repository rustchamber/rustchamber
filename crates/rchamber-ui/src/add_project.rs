use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, Key, Layout, Margin, Modifiers, Pos2, Rect,
    RichText, ScrollArea, Sense, Stroke, TextStyle, Ui, UiBuilder, Vec2, WidgetText,
};
use rchamber_api::models::Project;
use rchamber_app::ToastKind;
use rchamber_app::workspace::{ProjectEntry, paths_match};
use rchamber_git::repo;

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets::{self, ButtonSize, ButtonVariant};

const MODAL_WIDTH: f32 = 760.0;
const MODAL_HEIGHT: f32 = 520.0;
const MODAL_GAP: f32 = 24.0;
const RAIL_WIDTH: f32 = 180.0;
const HEADER_HEIGHT: f32 = 58.0;
const FOOTER_HEIGHT: f32 = 56.0;
const CONTENT_PAD: f32 = 20.0;
const RAIL_PAD: f32 = 12.0;
const ROW_HEIGHT: f32 = 34.0;
const ROW_TALL: f32 = 52.0;
const RAIL_ITEM: f32 = 32.0;
const RAIL_GLYPH: f32 = 15.0;
const CONTENT_GLYPH: f32 = 16.0;
const GAP: f32 = 6.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Home,
    Projects,
    Clone,
    OpenCode,
}

impl Tab {
    fn subtitle(self) -> &'static str {
        match self {
            Tab::Home => "Choose a folder to add to the sidebar.",
            Tab::Projects => "Projects already in this window.",
            Tab::Clone => "Clone a Git repository and add it.",
            Tab::OpenCode => "Import projects registered in OpenCode.",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CloneStatus {
    #[default]
    Idle,
    Running,
    Done(String),
    Failed(String),
}

#[derive(Clone, Debug)]
enum CloneMsg {
    Done(PathBuf),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub added: bool,
}

pub struct State {
    pub open: bool,
    pub tab: Tab,
    pub path: PathBuf,
    pub path_input: String,
    pub listing: Vec<DirEntry>,
    pub error: Option<String>,
    pub clone_url: String,
    pub clone_target: String,
    pub clone_name: String,
    pub clone_listing: Vec<DirEntry>,
    pub clone_status: CloneStatus,
    pub query: String,
    pub selected: Option<usize>,
    pub focus_path: bool,
    pub focus_clone: bool,
    clone_base: PathBuf,
    clone_derived: String,
    receiver: Option<Receiver<CloneMsg>>,
}

impl Default for State {
    fn default() -> Self {
        let path = home_dir();
        let listing = load_listing(&path, &[]);
        Self {
            open: false,
            tab: Tab::Home,
            path_input: display_path(&path),
            path,
            listing: listing.clone(),
            error: None,
            clone_url: String::new(),
            clone_target: display_path(&home_dir()),
            clone_name: String::new(),
            clone_listing: listing,
            clone_status: CloneStatus::Idle,
            query: String::new(),
            selected: None,
            focus_path: false,
            focus_clone: false,
            clone_base: home_dir(),
            clone_derived: String::new(),
            receiver: None,
        }
    }
}

impl State {
    pub fn begin(&mut self, directory: Option<&str>, projects: &[ProjectEntry]) {
        let path = directory
            .map(normalize_dir)
            .filter(|path| path.is_dir())
            .unwrap_or_else(home_dir);
        self.open = true;
        self.tab = Tab::Home;
        self.query.clear();
        self.selected = None;
        self.error = None;
        self.focus_path = true;
        self.path = path;
        self.path_input = display_path(&self.path);
        self.listing = load_listing(&self.path, projects);
        if !matches!(self.clone_status, CloneStatus::Running) {
            self.clone_status = CloneStatus::Idle;
            self.clone_target = display_path(&self.path);
            self.clone_base = self.path.clone();
            self.clone_listing = self.listing.clone();
            self.clone_name.clear();
            self.clone_derived.clear();
        }
    }

    fn navigate(&mut self, path: PathBuf, projects: &[ProjectEntry]) {
        if !path.is_dir() {
            self.error = Some(format!("Folder not found: {}", display_path(&path)));
            return;
        }
        self.path = path.canonicalize().unwrap_or(path);
        self.path_input = display_path(&self.path);
        self.listing = load_listing(&self.path, projects);
        self.selected = None;
        self.error = None;
    }

    fn refresh(&mut self, projects: &[ProjectEntry]) {
        if self.path.is_dir() {
            self.listing = load_listing(&self.path, projects);
            self.error = None;
        } else {
            self.listing.clear();
            self.error = Some(format!("Folder not found: {}", display_path(&self.path)));
        }
    }

    fn up(&mut self, projects: &[ProjectEntry]) {
        if let Some(parent) = parent_dir(&self.path) {
            self.navigate(parent, projects);
        }
    }

    fn commit_path(&mut self, projects: &[ProjectEntry]) {
        let target = normalize_dir(&self.path_input);
        if target.is_dir() {
            self.navigate(target, projects);
        } else {
            self.error = Some(format!("Folder not found: {}", self.path_input.trim()));
        }
    }

    fn target_path(&self) -> PathBuf {
        let target = normalize_dir(&self.path_input);
        if target.is_dir() {
            target
        } else {
            self.path.clone()
        }
    }

    fn sync_clone_listing(&mut self, projects: &[ProjectEntry]) {
        let target = normalize_dir(&self.clone_target);
        if target != self.clone_base {
            self.clone_base = target.clone();
            self.clone_listing = if target.is_dir() {
                load_listing(&target, projects)
            } else {
                Vec::new()
            };
        }
    }
}

#[derive(Default)]
struct Outcome {
    add: Option<PathBuf>,
    select: Option<String>,
    close_project: Option<String>,
    opencode: Option<(String, Option<String>)>,
    clone: bool,
    close: bool,
    open_failed: Option<String>,
}

pub fn expand_home(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if trimmed == "~" {
        return home_dir();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("~\\") {
        return home_dir().join(rest);
    }
    PathBuf::from(trimmed)
}

pub fn normalize_dir(input: &str) -> PathBuf {
    let path = expand_home(input);
    if path.as_os_str().is_empty() {
        return path;
    }
    path.canonicalize().unwrap_or(path)
}

pub fn parent_dir(path: &Path) -> Option<PathBuf> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

pub fn display_path(path: &Path) -> String {
    let home = home_dir();
    if path == home {
        return "~".into();
    }
    match path.strip_prefix(&home) {
        Ok(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

pub fn added(projects: &[ProjectEntry], path: &Path) -> bool {
    let path = path.to_string_lossy();
    projects
        .iter()
        .any(|project| paths_match(&project.canonical, &path))
}

pub fn is_valid_repo_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty()
        || url.contains('\0')
        || url.contains('\n')
        || url.contains('\r')
        || url.chars().any(char::is_whitespace)
    {
        return false;
    }
    if let Some(rest) = url.strip_prefix("https://") {
        let rest = rest.trim_end_matches('/');
        let Some((host, path)) = rest.split_once('/') else {
            return false;
        };
        return !host.is_empty() && !host.contains('@') && !path.trim_matches('/').is_empty();
    }
    if let Some(rest) = url.strip_prefix("ssh://") {
        let rest = rest.trim_end_matches('/');
        let Some((host, path)) = rest.split_once('/') else {
            return false;
        };
        return !host.is_empty() && !path.trim_matches('/').is_empty();
    }
    if let Some(rest) = url.strip_prefix("git@") {
        let Some((host, path)) = rest.split_once(':') else {
            return false;
        };
        return !host.is_empty() && !path.trim_matches('/').is_empty();
    }
    false
}

pub fn repo_name_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if !is_valid_repo_url(trimmed) {
        return None;
    }
    let last = trimmed.rsplit(['/', ':']).next()?.trim();
    let name = last.strip_suffix(".git").unwrap_or(last);
    if valid_folder_name(name) {
        Some(name.to_string())
    } else {
        None
    }
}

pub fn valid_folder_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|character| matches!(character, '/' | '\\' | '\0' | '\n' | '\r'))
}

fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn load_listing(path: &Path, projects: &[ProjectEntry]) -> Vec<DirEntry> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut directories: Vec<DirEntry> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            Some(DirEntry {
                name,
                added: added(projects, &path),
                path,
            })
        })
        .collect();
    directories.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    directories.truncate(500);
    directories
}

pub fn show(ctx: &egui::Context, app: &mut RustChamber) {
    poll_clone(ctx, app);
    if !app.ui.add_project.open {
        return;
    }
    let projects: Vec<ProjectEntry> = app
        .model
        .workspace_projects()
        .into_iter()
        .cloned()
        .collect();
    let opencode: Vec<Project> = app.model.projects.clone();
    let active_directory = app
        .model
        .workspace
        .clone()
        .or_else(|| {
            app.model
                .active
                .as_ref()
                .and_then(|chat| chat.session.directory().map(str::to_string))
        })
        .map(PathBuf::from)
        .map(|path| path.canonicalize().unwrap_or(path));
    let p = app.palette.clone();
    let screen = ctx.content_rect();
    let width = MODAL_WIDTH.min((screen.width() - MODAL_GAP * 2.0).max(260.0));
    let height = MODAL_HEIGHT.min((screen.height() - MODAL_GAP * 2.0).max(300.0));
    let rail_width = RAIL_WIDTH.min((width * 0.32).max(120.0));
    let frame = egui::Frame::new()
        .fill(p.bg)
        .corner_radius(theme::radius::XL)
        .stroke(Stroke::new(theme::HAIRLINE, p.elevation_ring))
        .shadow(p.modal_shadow())
        .inner_margin(Margin::ZERO);
    let mut outcome = Outcome::default();
    let modal = egui::Modal::new(Id::unique("add-project-modal"))
        .frame(frame)
        .backdrop_color(p.scrim())
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let (window, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
            let rail_rect = Rect::from_min_max(
                window.min,
                Pos2::new(window.left() + rail_width, window.bottom()),
            );
            let content_rect =
                Rect::from_min_max(Pos2::new(rail_rect.right(), window.top()), window.max);
            paint_rail(ui, rail_rect, &p);

            let mut rail_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(rail_rect.shrink2(Vec2::new(RAIL_PAD, 0.0)))
                    .layout(Layout::top_down(Align::Min)),
            );
            rail_ui.set_clip_rect(rail_rect);
            let state = &mut app.ui.add_project;
            rail(
                &mut rail_ui,
                state,
                &projects,
                active_directory.as_deref(),
                &p,
            );

            let header_rect = Rect::from_min_size(
                content_rect.min,
                Vec2::new(content_rect.width(), HEADER_HEIGHT),
            );
            let footer_rect = Rect::from_min_size(
                Pos2::new(content_rect.left(), content_rect.bottom() - FOOTER_HEIGHT),
                Vec2::new(content_rect.width(), FOOTER_HEIGHT),
            );
            let body_rect = Rect::from_min_max(
                Pos2::new(content_rect.left() + CONTENT_PAD, header_rect.bottom()),
                Pos2::new(content_rect.right() - CONTENT_PAD, footer_rect.top()),
            );
            header(ui, header_rect, state, &p, &mut outcome);

            let mut body_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(body_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            body_ui.set_clip_rect(body_rect);
            match state.tab {
                Tab::Home => home_tab(&mut body_ui, state, &projects, &p, &mut outcome),
                Tab::Projects => projects_tab(
                    &mut body_ui,
                    state,
                    &projects,
                    active_directory.as_deref(),
                    &p,
                    &mut outcome,
                ),
                Tab::Clone => clone_tab(&mut body_ui, state, &projects, &p),
                Tab::OpenCode => {
                    opencode_tab(&mut body_ui, state, &projects, &opencode, &p, &mut outcome)
                }
            }

            footer(
                ui,
                footer_rect,
                state,
                &projects,
                active_directory.as_deref(),
                &p,
                &mut outcome,
            );
        });
    if modal.should_close() {
        outcome.close = true;
    }
    if let Some(path) = outcome.add {
        register_project(app, &path, None, true);
        outcome.close = true;
    }
    if let Some((directory, name)) = outcome.opencode {
        register_project(app, Path::new(&directory), name.as_deref(), false);
        refresh_listings(app);
    }
    if let Some(id) = outcome.select {
        app.select_project(&id);
        outcome.close = true;
    }
    if let Some(id) = outcome.close_project {
        app.ui.confirm_close_project = Some(id);
        outcome.close = true;
    }
    if outcome.clone {
        start_clone(ctx, app);
    }
    if let Some(error) = outcome.open_failed {
        app.model.toast(ToastKind::Error, error);
    }
    if outcome.close {
        app.ui.add_project.open = false;
    }
}

fn paint_rail(ui: &Ui, rect: Rect, p: &Palette) {
    let corner = CornerRadius {
        nw: theme::radius::XL.saturating_sub(1),
        sw: theme::radius::XL.saturating_sub(1),
        ne: 0,
        se: 0,
    };
    ui.painter().rect_filled(rect, corner, p.bg_soft);
    ui.painter().vline(
        rect.right(),
        rect.y_range(),
        Stroke::new(theme::BORDER, p.border),
    );
}

fn rail(
    ui: &mut Ui,
    state: &mut State,
    projects: &[ProjectEntry],
    active_directory: Option<&Path>,
    p: &Palette,
) {
    ui.add_space(18.0);
    if rail_item(ui, p, Icon::Home, "Home", state.tab == Tab::Home).clicked() {
        state.tab = Tab::Home;
        state.query.clear();
        state.error = None;
        state.navigate(home_dir(), projects);
        state.focus_path = true;
    }
    ui.add_space(10.0);
    widgets::section_label(ui, "Recent", p);
    ui.add_space(4.0);
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(active) = active_directory
        && let Some(parent) = parent_dir(active)
        && parent != home_dir()
    {
        roots.push(parent);
    }
    if !roots.iter().any(|root| root == &home_dir()) {
        roots.push(home_dir());
    }
    for root in &roots {
        let label = if root == &home_dir() {
            "~".to_string()
        } else {
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| display_path(root))
        };
        if rail_root(ui, p, &label, &display_path(root)).clicked() {
            state.tab = Tab::Home;
            state.query.clear();
            state.error = None;
            state.navigate(root.clone(), projects);
            state.focus_path = true;
        }
    }
    ui.add_space(8.0);
    if rail_item(ui, p, Icon::Folder, "Projects", state.tab == Tab::Projects).clicked() {
        state.tab = Tab::Projects;
        state.query.clear();
        state.error = None;
    }
    ui.add_space(2.0);
    if rail_item(
        ui,
        p,
        Icon::Link,
        "Clone repository",
        state.tab == Tab::Clone,
    )
    .clicked()
    {
        state.tab = Tab::Clone;
        state.query.clear();
        state.error = None;
        state.focus_clone = true;
        state.sync_clone_listing(projects);
    }
    ui.add_space(2.0);
    if rail_item(
        ui,
        p,
        Icon::Bot,
        "From OpenCode",
        state.tab == Tab::OpenCode,
    )
    .clicked()
    {
        state.tab = Tab::OpenCode;
        state.query.clear();
        state.error = None;
    }
}

fn rail_item(ui: &mut Ui, p: &Palette, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), RAIL_ITEM), Sense::click());
    widgets::pointer(ui, &response);
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.selection);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.hover);
    }
    let tint = if selected { p.selection_fg } else { p.fg };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 8.0 + RAIL_GLYPH * 0.5, rect.center().y),
            Vec2::splat(RAIL_GLYPH),
        ),
        if selected { tint } else { p.fg_muted },
    );
    let galley = WidgetText::from(
        RichText::new(label)
            .font(theme::font_medium(theme::TEXT_UI_LABEL))
            .color(tint),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (rect.right() - rect.left() - 40.0).max(20.0),
        TextStyle::Body,
    );
    ui.painter().galley(
        Pos2::new(
            rect.left() + 8.0 + RAIL_GLYPH + 8.0,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        tint,
    );
    widgets::pointer(ui, &response);
    response
}

fn rail_root(ui: &mut Ui, p: &Palette, label: &str, path: &str) -> egui::Response {
    let response = rail_item(ui, p, Icon::FolderOpen, label, false);
    response.on_hover_text(path)
}

fn header(ui: &mut Ui, rect: Rect, state: &State, p: &Palette, outcome: &mut Outcome) {
    ui.painter().text(
        Pos2::new(rect.left() + CONTENT_PAD, rect.top() + 13.0),
        Align2::LEFT_TOP,
        "Add project",
        theme::font_semibold(15.0),
        p.fg,
    );
    ui.painter().text(
        Pos2::new(rect.left() + CONTENT_PAD, rect.top() + 34.0),
        Align2::LEFT_TOP,
        state.tab.subtitle(),
        theme::font(theme::TEXT_META),
        p.fg_muted,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - theme::HAIRLINE,
        Stroke::new(theme::HAIRLINE, p.edge_muted()),
    );
    let close_rect = Rect::from_min_size(
        Pos2::new(rect.right() - CONTENT_PAD - 28.0, rect.top() + 11.0),
        Vec2::splat(28.0),
    );
    let mut close_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(close_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    if widgets::icon_button_hit(&mut close_ui, Icon::Close, 14.0, 28.0, p.fg_muted)
        .on_hover_text("Close")
        .clicked()
    {
        outcome.close = true;
    }
}

fn home_tab(
    ui: &mut Ui,
    state: &mut State,
    projects: &[ProjectEntry],
    p: &Palette,
    outcome: &mut Outcome,
) {
    let path_id = Id::unique("add-project/path");
    if state.focus_path {
        ui.memory_mut(|memory| memory.request_focus(path_id));
        state.focus_path = false;
    }
    let focused = ui.memory(|memory| memory.has_focus(path_id));
    if !state.listing.is_empty() {
        state.selected = state
            .selected
            .map(|index| index.min(state.listing.len() - 1));
        let down = ui
            .ctx()
            .input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown));
        let up = ui
            .ctx()
            .input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp));
        if down {
            state.selected = Some(
                state
                    .selected
                    .map_or(0, |index| (index + 1).min(state.listing.len() - 1)),
            );
        } else if up {
            state.selected = Some(
                state
                    .selected
                    .map_or(state.listing.len() - 1, |index| index.saturating_sub(1)),
            );
        }
    }
    let command_enter = ui
        .ctx()
        .input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter))
        || ui
            .ctx()
            .input_mut(|input| input.consume_key(Modifiers::CTRL, Key::Enter));
    let enter = !command_enter
        && ui
            .ctx()
            .input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter));

    ui.add_space(4.0);
    let field_width = (ui.available_width() - 2.0 * (28.0 + GAP)).max(120.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.allocate_ui_with_layout(
            Vec2::new(field_width, widgets::INPUT_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                field_with_icon(
                    ui,
                    &mut state.path_input,
                    "~/Projects/",
                    Icon::FolderOpen,
                    false,
                    true,
                    p,
                    path_id,
                );
            },
        );
        if widgets::icon_button_hit(ui, Icon::Refresh, 14.0, 28.0, p.fg_muted)
            .on_hover_text("Refresh")
            .clicked()
        {
            state.refresh(projects);
        }
        if widgets::icon_button_hit(ui, Icon::ArrowUp, 14.0, 28.0, p.fg_muted)
            .on_hover_text("Up")
            .clicked()
        {
            state.up(projects);
        }
    });
    if command_enter {
        outcome.add = Some(state.target_path());
    } else if enter {
        let typed = state.path_input.trim() != display_path(&state.path);
        if focused && (typed || state.selected.is_none()) {
            state.commit_path(projects);
        } else if let Some(index) = state.selected
            && let Some(entry) = state.listing.get(index).cloned()
        {
            state.navigate(entry.path, projects);
        }
    }
    if let Some(error) = state.error.clone() {
        ui.add_space(3.0);
        error_line(ui, p, &error);
    }
    ui.add_space(GAP);
    ui.horizontal(|ui| {
        widgets::section_label(ui, "Folders", p);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::ghost_button(ui, "Browse…", Some(Icon::FolderOpen), p).clicked()
                && let Some(folder) = rfd::FileDialog::new().pick_folder()
            {
                state.navigate(folder, projects);
                state.focus_path = true;
            }
        });
    });
    ui.add_space(4.0);
    let entries = state.listing.clone();
    let added_current = added(projects, &state.path);
    let list_height = ui.available_height().max(60.0);
    ScrollArea::vertical()
        .id_salt(("add-project/home", &state.path))
        .max_height(list_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if entries.is_empty() {
                empty_state(
                    ui,
                    p,
                    Icon::FolderOpen,
                    "No folders here",
                    "Choose another folder or add this one.",
                );
            } else {
                for (index, entry) in entries.iter().enumerate() {
                    let selected = state.selected == Some(index);
                    let response = folder_row(ui, p, entry, selected);
                    if response.clicked() {
                        state.selected = Some(index);
                    }
                    if response.double_clicked() {
                        state.navigate(entry.path.clone(), projects);
                        break;
                    }
                }
                if added_current {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("This folder is already a project.")
                            .font(theme::font(theme::TEXT_MICRO))
                            .color(p.fg_muted),
                    );
                }
            }
        });
}

fn projects_tab(
    ui: &mut Ui,
    state: &mut State,
    projects: &[ProjectEntry],
    active_directory: Option<&Path>,
    p: &Palette,
    outcome: &mut Outcome,
) {
    ui.add_space(4.0);
    widgets::search_field(
        ui,
        &mut state.query,
        "Search projects…",
        p,
        Id::unique("add-project/projects-search"),
    );
    ui.add_space(GAP);
    let needle = state.query.trim().to_lowercase();
    let filtered: Vec<&ProjectEntry> = projects
        .iter()
        .filter(|project| {
            needle.is_empty()
                || project.display_name().to_lowercase().contains(&needle)
                || project.canonical.to_lowercase().contains(&needle)
        })
        .collect();
    let list_height = ui.available_height().max(80.0);
    ScrollArea::vertical()
        .id_salt("add-project/projects")
        .max_height(list_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if filtered.is_empty() {
                let text = if projects.is_empty() {
                    "No projects yet."
                } else {
                    "No matching projects."
                };
                empty_state(ui, p, Icon::Folder, "No projects", text);
                return;
            }
            for project in filtered {
                let active = active_directory.is_some_and(|directory| {
                    paths_match(&project.canonical, &directory.to_string_lossy())
                });
                let action = project_row(ui, p, project, active);
                if action.open {
                    outcome.select = Some(project.id.clone());
                    outcome.close = true;
                } else if action.close {
                    outcome.close_project = Some(project.id.clone());
                    outcome.close = true;
                } else if action.navigate {
                    state.tab = Tab::Home;
                    state.navigate(PathBuf::from(&project.canonical), projects);
                    state.focus_path = true;
                    break;
                }
            }
        });
}

fn opencode_tab(
    ui: &mut Ui,
    state: &mut State,
    projects: &[ProjectEntry],
    opencode: &[Project],
    p: &Palette,
    outcome: &mut Outcome,
) {
    ui.add_space(4.0);
    widgets::search_field(
        ui,
        &mut state.query,
        "Search OpenCode projects…",
        p,
        Id::unique("add-project/opencode-search"),
    );
    ui.add_space(GAP);
    let needle = state.query.trim().to_lowercase();
    let filtered: Vec<&Project> = opencode
        .iter()
        .filter(|project| {
            needle.is_empty()
                || project.display_name().to_lowercase().contains(&needle)
                || project.canonical.to_lowercase().contains(&needle)
        })
        .collect();
    let list_height = ui.available_height().max(80.0);
    ScrollArea::vertical()
        .id_salt("add-project/opencode")
        .max_height(list_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if filtered.is_empty() {
                let text = if opencode.is_empty() {
                    "Connect to OpenCode to see its projects."
                } else {
                    "No matching projects."
                };
                empty_state(ui, p, Icon::Bot, "No projects", text);
                return;
            }
            for project in filtered {
                let path = Path::new(&project.canonical);
                let is_added = added(projects, path);
                let available = path.is_dir();
                if opencode_row(ui, p, project, is_added, available) {
                    outcome.opencode = Some((project.canonical.clone(), project.name.clone()));
                }
            }
        });
}

fn clone_tab(ui: &mut Ui, state: &mut State, projects: &[ProjectEntry], p: &Palette) {
    state.sync_clone_listing(projects);
    let url_id = Id::unique("add-project/clone-url");
    if state.focus_clone {
        ui.memory_mut(|memory| memory.request_focus(url_id));
        state.focus_clone = false;
    }
    let url_valid = is_valid_repo_url(&state.clone_url);
    let url_empty = state.clone_url.trim().is_empty();
    let mut url_changed = false;
    let mut target_changed = false;
    let list_height = ui.available_height().max(80.0);
    ScrollArea::vertical()
        .id_salt("add-project/clone")
        .max_height(list_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_space(4.0);
            widgets::section_label(ui, "Repository URL (HTTPS or SSH)", p);
            ui.add_space(3.0);
            url_changed = field_with_icon(
                ui,
                &mut state.clone_url,
                "https://github.com/owner/repo.git",
                Icon::Link,
                !url_valid && !url_empty,
                true,
                p,
                url_id,
            )
            .changed();
            if !url_valid && !url_empty {
                ui.add_space(2.0);
                error_line(ui, p, "Enter a valid HTTPS or SSH repository URL.");
            }
            if url_changed
                && let Some(name) = repo_name_from_url(&state.clone_url)
                && (state.clone_name.trim().is_empty() || state.clone_name == state.clone_derived)
            {
                state.clone_name = name.clone();
                state.clone_derived = name;
            }
            ui.add_space(10.0);
            widgets::section_label(ui, "Clone into", p);
            ui.add_space(3.0);
            let field_width = (ui.available_width() - 74.0).max(100.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.allocate_ui_with_layout(
                    Vec2::new(field_width, widgets::INPUT_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        target_changed = field_with_icon(
                            ui,
                            &mut state.clone_target,
                            "~/Projects",
                            Icon::FolderOpen,
                            false,
                            true,
                            p,
                            Id::unique("add-project/clone-target"),
                        )
                        .changed();
                    },
                );
                if widgets::ghost_button(ui, "Browse…", None, p).clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    state.clone_target = display_path(&folder);
                    state.sync_clone_listing(projects);
                }
            });
            ui.add_space(10.0);
            widgets::section_label(ui, "Folder name", p);
            ui.add_space(3.0);
            let name_changed = field_with_icon(
                ui,
                &mut state.clone_name,
                "repository",
                Icon::Folder,
                false,
                false,
                p,
                Id::unique("add-project/clone-name"),
            )
            .changed();
            if !matches!(state.clone_status, CloneStatus::Running)
                && (url_changed || target_changed || name_changed)
            {
                state.clone_status = CloneStatus::Idle;
                state.error = None;
            }
            ui.add_space(10.0);
            clone_status(ui, state, p);
            if let Some(error) = state.error.clone() {
                ui.add_space(2.0);
                error_line(ui, p, &error);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                widgets::section_label(ui, "Folders", p);
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(display_path(&state.clone_base))
                            .font(theme::font_mono(11.0))
                            .color(p.fg_faint),
                    )
                    .truncate(),
                );
            });
            ui.add_space(3.0);
            let entries = state.clone_listing.clone();
            if entries.is_empty() {
                ui.label(
                    RichText::new("No folders here.")
                        .font(theme::font(theme::TEXT_MICRO))
                        .color(p.fg_muted),
                );
                return;
            }
            for entry in entries {
                let response = folder_row(ui, p, &entry, false);
                if response.double_clicked() {
                    state.clone_target = display_path(&entry.path);
                    state.clone_base = entry.path.clone();
                    state.clone_listing = load_listing(&entry.path, projects);
                    break;
                }
            }
        });
}

fn clone_status(ui: &mut Ui, state: &State, p: &Palette) {
    match &state.clone_status {
        CloneStatus::Idle => {}
        CloneStatus::Running => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Cloning repository…")
                        .font(theme::font(theme::TEXT_UI_LABEL))
                        .color(p.fg_muted),
                );
            });
        }
        CloneStatus::Done(name) => {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                icons::paint(ui, Icon::Check, rect, p.success_text);
                ui.label(
                    RichText::new(format!("Cloned and added '{name}'"))
                        .font(theme::font(theme::TEXT_UI_LABEL))
                        .color(p.success_text),
                );
            });
        }
        CloneStatus::Failed(message) => {
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                icons::paint(ui, Icon::Warning, rect, p.error_text);
                ui.add(
                    egui::Label::new(
                        RichText::new(message)
                            .font(theme::font(theme::TEXT_MICRO))
                            .color(p.error_text),
                    )
                    .wrap(),
                );
            });
        }
    }
}

fn footer(
    ui: &mut Ui,
    rect: Rect,
    state: &State,
    projects: &[ProjectEntry],
    active_directory: Option<&Path>,
    p: &Palette,
    outcome: &mut Outcome,
) {
    ui.painter().hline(
        rect.x_range(),
        rect.top() + theme::HAIRLINE,
        Stroke::new(theme::HAIRLINE, p.edge_muted()),
    );
    let inner = rect.shrink2(Vec2::new(CONTENT_PAD, 9.0));
    let mut footer_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    footer_ui.set_clip_rect(inner);
    footer_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        match state.tab {
            Tab::Home => {
                let target = state.target_path();
                let current_added = added(projects, &target);
                if current_added {
                    widgets::button_ex(
                        ui,
                        "Added",
                        Some(Icon::Check),
                        ButtonVariant::Outline,
                        ButtonSize::Md,
                        false,
                        p,
                    );
                } else {
                    let name = target
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| target.display().to_string());
                    let name = short_name(&name, 28);
                    if widgets::button_ex(
                        ui,
                        &format!("Add '{name}'"),
                        Some(Icon::Plus),
                        ButtonVariant::Primary,
                        ButtonSize::Md,
                        target.is_dir(),
                        p,
                    )
                    .clicked()
                    {
                        outcome.add = Some(target);
                    }
                }
            }
            Tab::Clone => {
                if matches!(state.clone_status, CloneStatus::Running) {
                    widgets::button_ex(
                        ui,
                        "Cloning…",
                        None,
                        ButtonVariant::Outline,
                        ButtonSize::Md,
                        false,
                        p,
                    );
                } else if widgets::primary_button(ui, "Clone and add", Some(Icon::Download), p)
                    .clicked()
                {
                    outcome.clone = true;
                }
            }
            Tab::Projects | Tab::OpenCode => {
                if widgets::ghost_button(ui, "Done", None, p).clicked() {
                    outcome.close = true;
                }
            }
        }
        let finder = match state.tab {
            Tab::Home => Some(state.path.clone()),
            Tab::Clone => Some(state.clone_base.clone()),
            Tab::Projects | Tab::OpenCode => active_directory
                .map(Path::to_path_buf)
                .or_else(|| Some(state.path.clone())),
        };
        if let Some(path) = finder
            && widgets::button_ex(
                ui,
                if cfg!(target_os = "macos") {
                    "Open in Finder"
                } else {
                    "Open in folder"
                },
                Some(Icon::FolderOpen),
                ButtonVariant::Outline,
                ButtonSize::Sm,
                path.is_dir(),
                p,
            )
            .clicked()
            && let Err(error) = widgets::reveal_in_finder(&path)
        {
            outcome.open_failed = Some(format!("Could not open folder: {error}"));
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new("↑↓ Navigate   ↵ Open   Esc Close")
                        .font(theme::font(theme::TEXT_MICRO))
                        .color(p.fg_muted),
                )
                .truncate(),
            );
        });
    });
}

fn folder_row(ui: &mut Ui, p: &Palette, entry: &DirEntry, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.selection);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.hover);
    }
    icons::paint(
        ui,
        Icon::Folder,
        Rect::from_center_size(
            Pos2::new(rect.left() + 8.0 + CONTENT_GLYPH * 0.5, rect.center().y),
            Vec2::splat(CONTENT_GLYPH),
        ),
        if selected { p.selection_fg } else { p.fg_muted },
    );
    let name_x = rect.left() + 8.0 + CONTENT_GLYPH + 9.0;
    let right = if entry.added {
        paint_added(ui, p, rect.right() - 10.0, rect.center().y)
    } else {
        icons::paint(
            ui,
            Icon::ChevronRight,
            Rect::from_center_size(
                Pos2::new(rect.right() - 16.0, rect.center().y),
                Vec2::splat(13.0),
            ),
            p.fg_muted,
        );
        rect.right() - 30.0
    };
    paint_truncated(
        ui,
        Pos2::new(name_x, rect.center().y),
        &entry.name,
        theme::font(theme::TEXT_UI_LABEL),
        if selected { p.selection_fg } else { p.fg },
        (right - name_x - 8.0).max(20.0),
    );
    widgets::pointer(ui, &response);
    response
}

#[derive(Default)]
struct ProjectAction {
    navigate: bool,
    open: bool,
    close: bool,
}

fn project_row(ui: &mut Ui, p: &Palette, project: &ProjectEntry, active: bool) -> ProjectAction {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_TALL), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.hover);
    }
    icons::paint(
        ui,
        Icon::FolderOpen,
        Rect::from_center_size(
            Pos2::new(rect.left() + 8.0 + CONTENT_GLYPH * 0.5, rect.center().y),
            Vec2::splat(CONTENT_GLYPH),
        ),
        p.fg_muted,
    );
    let open_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 36.0, rect.center().y),
        Vec2::new(56.0, 26.0),
    );
    let open = ui.interact(open_rect, response.id.with("open"), Sense::click());
    let badge_left = paint_added(ui, p, open_rect.left() - 10.0, rect.center().y);
    let name_x = rect.left() + 8.0 + CONTENT_GLYPH + 9.0;
    let text_width = (badge_left - name_x - 12.0).max(30.0);
    paint_truncated(
        ui,
        Pos2::new(name_x, rect.center().y - 9.0),
        project.display_name(),
        theme::font_medium(theme::TEXT_UI_LABEL),
        p.fg,
        text_width,
    );
    paint_truncated(
        ui,
        Pos2::new(name_x, rect.center().y + 9.0),
        &project.canonical,
        theme::font_mono(11.0),
        p.fg_muted,
        text_width,
    );
    if open.hovered() {
        ui.painter()
            .rect_filled(open_rect, CornerRadius::same(theme::radius::MD), p.active);
    }
    ui.painter().text(
        open_rect.center(),
        Align2::CENTER_CENTER,
        if active { "Close" } else { "Open" },
        theme::font_medium(theme::TEXT_MICRO),
        if active {
            if open.hovered() {
                p.error_text
            } else {
                p.fg_muted
            }
        } else if open.hovered() {
            p.fg
        } else {
            p.primary_text
        },
    );
    widgets::pointer(ui, &response);
    widgets::pointer(ui, &open);
    ProjectAction {
        navigate: response.clicked() && !open.hovered(),
        open: open.clicked() && !active,
        close: open.clicked() && active,
    }
}

fn opencode_row(
    ui: &mut Ui,
    p: &Palette,
    project: &Project,
    is_added: bool,
    available: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_TALL), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.hover);
    }
    icons::paint(
        ui,
        Icon::Bot,
        Rect::from_center_size(
            Pos2::new(rect.left() + 8.0 + CONTENT_GLYPH * 0.5, rect.center().y),
            Vec2::splat(CONTENT_GLYPH),
        ),
        p.fg_muted,
    );
    let add_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 34.0, rect.center().y),
        Vec2::new(52.0, 26.0),
    );
    let add = ui.interact(
        add_rect,
        response.id.with("add"),
        if !is_added && available {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let right = if is_added {
        paint_added(ui, p, add_rect.right(), rect.center().y)
    } else if !available {
        ui.painter().text(
            Pos2::new(add_rect.right(), rect.center().y),
            Align2::RIGHT_CENTER,
            "Missing",
            theme::font_medium(theme::TEXT_MICRO),
            p.fg_faint,
        );
        add_rect.left() - 4.0
    } else {
        if add.hovered() {
            ui.painter()
                .rect_filled(add_rect, CornerRadius::same(theme::radius::MD), p.active);
        }
        ui.painter().text(
            add_rect.center(),
            Align2::CENTER_CENTER,
            "Add",
            theme::font_medium(theme::TEXT_MICRO),
            if add.hovered() { p.fg } else { p.primary_text },
        );
        add_rect.left() - 4.0
    };
    let name_x = rect.left() + 8.0 + CONTENT_GLYPH + 9.0;
    let text_width = (right - name_x - 10.0).max(30.0);
    paint_truncated(
        ui,
        Pos2::new(name_x, rect.center().y - 9.0),
        &project.display_name(),
        theme::font_medium(theme::TEXT_UI_LABEL),
        p.fg,
        text_width,
    );
    paint_truncated(
        ui,
        Pos2::new(name_x, rect.center().y + 9.0),
        &project.canonical,
        theme::font_mono(11.0),
        p.fg_muted,
        text_width,
    );
    widgets::pointer(ui, &response);
    widgets::pointer(ui, &add);
    add.clicked()
}

fn short_name(name: &str, limit: usize) -> String {
    if name.chars().count() <= limit {
        return name.to_string();
    }
    let mut short: String = name.chars().take(limit.saturating_sub(1)).collect();
    short.push('…');
    short
}

fn paint_added(ui: &Ui, p: &Palette, right: f32, center_y: f32) -> f32 {
    let check = 11.0;
    let gap = 4.0;
    let font = theme::font_medium(11.0);
    let galley = ui
        .painter()
        .layout_no_wrap("Added".into(), font, p.success_text);
    let left = right - (check + gap + galley.size().x);
    icons::paint(
        ui,
        Icon::Check,
        Rect::from_center_size(Pos2::new(left + check * 0.5, center_y), Vec2::splat(check)),
        p.success_text,
    );
    ui.painter().galley(
        Pos2::new(left + check + gap, center_y - galley.size().y * 0.5),
        galley,
        p.success_text,
    );
    left
}

fn paint_truncated(
    ui: &Ui,
    center: Pos2,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_width: f32,
) {
    let galley = WidgetText::from(RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        max_width.max(1.0),
        TextStyle::Body,
    );
    ui.painter().galley(
        Pos2::new(center.x, center.y - galley.size().y * 0.5),
        galley,
        color,
    );
}

fn field_with_icon(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    icon: Icon,
    invalid: bool,
    mono: bool,
    p: &Palette,
    id: Id,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, widgets::INPUT_HEIGHT), Sense::click());
    let focused = ui.memory(|memory| memory.has_focus(id));
    let hover = response.hovered() && !focused;
    let mut fill = p.field_fill();
    if hover {
        fill = p.over(fill, p.hover);
    }
    let corner = CornerRadius::same(theme::radius::MD);
    ui.painter().rect_filled(rect, corner, fill);
    let (highlight_width, highlight) = p.field_highlight();
    ui.painter().hline(
        (rect.left() + theme::radius::MD as f32)..=(rect.right() - theme::radius::MD as f32),
        rect.top() + highlight_width * 0.5,
        Stroke::new(highlight_width, highlight),
    );
    let (edge_width, edge) = if invalid {
        (theme::BORDER, p.red)
    } else if focused {
        (theme::BORDER, p.field_edge_focus())
    } else {
        (theme::HAIRLINE, p.field_edge())
    };
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(edge_width, edge),
        egui::StrokeKind::Inside,
    );
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 18.0, rect.center().y),
            Vec2::splat(16.0),
        ),
        if focused { p.fg } else { p.fg_muted },
    );
    let text_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 36.0, rect.top() + 4.0),
        Pos2::new(rect.right() - 10.0, rect.bottom() - 4.0),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let font = if mono {
        theme::font_mono(12.0)
    } else {
        theme::font(theme::TEXT_UI_LABEL)
    };
    let edit = child.add(
        egui::TextEdit::singleline(value)
            .id(id)
            .frame(egui::Frame::NONE)
            .margin(Margin::ZERO)
            .hint_text(
                RichText::new(hint)
                    .font(theme::font(theme::TEXT_UI_LABEL))
                    .color(p.fg_muted),
            )
            .text_color(p.fg)
            .font(font)
            .desired_width(f32::INFINITY),
    );
    if response.clicked() {
        edit.request_focus();
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    edit
}

fn error_line(ui: &mut Ui, p: &Palette, message: &str) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
        icons::paint(ui, Icon::Warning, rect, p.error_text);
        ui.add(
            egui::Label::new(
                RichText::new(message)
                    .font(theme::font(theme::TEXT_MICRO))
                    .color(p.error_text),
            )
            .wrap(),
        );
    });
}

fn empty_state(ui: &mut Ui, p: &Palette, icon: Icon, title: &str, description: &str) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
        icons::paint(ui, icon, rect, p.fg_muted);
        ui.add_space(8.0);
        ui.label(
            RichText::new(title)
                .font(theme::font_medium(theme::TEXT_UI_LABEL))
                .color(p.fg),
        );
        ui.add_space(3.0);
        ui.label(
            RichText::new(description)
                .font(theme::font(theme::TEXT_MICRO))
                .color(p.fg_muted),
        );
    });
}

fn register_project(app: &mut RustChamber, directory: &Path, name: Option<&str>, activate: bool) {
    let directory = directory.to_string_lossy().to_string();
    let id = app.model.add_project(&directory, name);
    app.sync_tracked_projects();
    if activate {
        app.select_project(&id);
    }
    app.model.toast(ToastKind::Success, "Project added");
}

fn start_clone(ctx: &egui::Context, app: &mut RustChamber) {
    let state = &mut app.ui.add_project;
    if matches!(state.clone_status, CloneStatus::Running) {
        return;
    }
    state.error = None;
    let url = state.clone_url.trim().to_string();
    if !is_valid_repo_url(&url) {
        state.error = Some("Enter a valid HTTPS or SSH repository URL.".into());
        return;
    }
    let target = normalize_dir(&state.clone_target);
    if !target.is_dir() {
        state.error = Some("Choose an existing folder to clone into.".into());
        return;
    }
    let mut name = state.clone_name.trim().to_string();
    if name.is_empty() {
        name = repo_name_from_url(&url).unwrap_or_default();
    }
    if !valid_folder_name(&name) {
        state.error = Some("Enter a folder name for the clone.".into());
        return;
    }
    state.clone_name = name.clone();
    let destination = target.join(&name);
    if destination.exists() {
        if !destination.is_dir() {
            state.error = Some(format!(
                "Destination exists and is not a folder: {}",
                destination.display()
            ));
            return;
        }
        let non_empty = std::fs::read_dir(&destination)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true);
        if non_empty {
            state.error = Some(format!(
                "Destination already exists and is not empty: {}",
                destination.display()
            ));
            return;
        }
    }
    let (sender, receiver) = mpsc::channel();
    state.receiver = Some(receiver);
    state.clone_status = CloneStatus::Running;
    let context = ctx.clone();
    std::thread::spawn(move || {
        let message = match repo::clone_repo(&url, &destination) {
            Ok(()) => CloneMsg::Done(destination),
            Err(error) => CloneMsg::Failed(error.to_string()),
        };
        let _ = sender.send(message);
        context.request_repaint();
    });
}

fn poll_clone(ctx: &egui::Context, app: &mut RustChamber) {
    let received = {
        let state = &app.ui.add_project;
        let Some(receiver) = state.receiver.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(message) => Ok(message),
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(120));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("Clone process ended unexpectedly.".to_string()),
        }
    };
    let completed = {
        let state = &mut app.ui.add_project;
        state.receiver = None;
        match received {
            Ok(CloneMsg::Done(path)) => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "repository".into());
                state.clone_status = CloneStatus::Done(name);
                Some(path)
            }
            Ok(CloneMsg::Failed(message)) => {
                state.clone_status = CloneStatus::Failed(message);
                None
            }
            Err(message) => {
                state.clone_status = CloneStatus::Failed(message);
                None
            }
        }
    };
    if let Some(path) = completed {
        let directory = path.to_string_lossy().to_string();
        let id = app.model.add_project(&directory, None);
        app.sync_tracked_projects();
        app.select_project(&id);
        app.model
            .toast(ToastKind::Success, "Repository cloned and added");
        refresh_listings(app);
    }
}

fn refresh_listings(app: &mut RustChamber) {
    let projects: Vec<ProjectEntry> = app
        .model
        .workspace_projects()
        .into_iter()
        .cloned()
        .collect();
    let state = &mut app.ui.add_project;
    state.listing = load_listing(&state.path, &projects);
    state.clone_listing = load_listing(&state.clone_base, &projects);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, canonical: &str) -> ProjectEntry {
        ProjectEntry {
            id: id.into(),
            canonical: canonical.into(),
            name: canonical.into(),
            name_override: None,
            color: None,
            hidden: false,
            added_at: 0.0,
        }
    }

    #[test]
    fn home_expansion_handles_tilde_forms() {
        let home = home_dir();
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/Projects"), home.join("Projects"));
        assert_eq!(expand_home(" /tmp/example "), PathBuf::from("/tmp/example"));
        assert_eq!(
            normalize_dir("/tmp/../tmp"),
            Path::new("/tmp")
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from("/tmp"))
        );
    }

    #[test]
    fn display_path_uses_the_home_shorthand() {
        let home = home_dir();
        assert_eq!(display_path(&home), "~");
        assert_eq!(
            display_path(&home.join("Projects")),
            "~/Projects".to_string()
        );
        assert_eq!(display_path(Path::new("/tmp/example")), "/tmp/example");
    }

    #[test]
    fn parent_dir_stops_at_the_root() {
        let path = Path::new("/tmp/example/child");
        assert_eq!(parent_dir(path), Some(PathBuf::from("/tmp/example")));
        assert_eq!(parent_dir(Path::new("/")), None);
    }

    #[test]
    fn repository_urls_accept_https_ssh_and_git() {
        assert!(is_valid_repo_url("https://github.com/owner/repo.git"));
        assert!(!is_valid_repo_url("http://example.com/owner/repo"));
        assert!(!is_valid_repo_url("https://token@github.com/owner/repo"));
        assert!(is_valid_repo_url("ssh://git@github.com/owner/repo.git"));
        assert!(is_valid_repo_url("git@github.com:owner/repo.git"));
        assert!(!is_valid_repo_url(""));
        assert!(!is_valid_repo_url("github.com/owner/repo"));
        assert!(!is_valid_repo_url("https://"));
        assert!(!is_valid_repo_url("https://github.com/"));
        assert!(!is_valid_repo_url("git@github.com"));
        assert!(!is_valid_repo_url("ftp://github.com/owner/repo"));
        assert!(!is_valid_repo_url(
            "https://github.com/owner/repo with space"
        ));
    }

    #[test]
    fn repository_name_comes_from_the_last_segment() {
        assert_eq!(
            repo_name_from_url("https://github.com/owner/repo.git"),
            Some("repo".into())
        );
        assert_eq!(
            repo_name_from_url("git@github.com:owner/repo.git"),
            Some("repo".into())
        );
        assert_eq!(
            repo_name_from_url("ssh://git@github.com/owner/repo/"),
            Some("repo".into())
        );
        assert_eq!(repo_name_from_url("https://github.com"), None);
    }

    #[test]
    fn added_matches_normalized_paths() {
        let projects = vec![entry("p1", "/tmp/example/")];
        assert!(added(&projects, Path::new("/tmp/example")));
        assert!(!added(&projects, Path::new("/tmp/other")));
    }

    #[test]
    fn folder_names_reject_separators_and_dots() {
        assert!(valid_folder_name("repo"));
        assert!(!valid_folder_name(""));
        assert!(!valid_folder_name(".."));
        assert!(!valid_folder_name("a/b"));
        assert!(!valid_folder_name("a\\b"));
        assert!(!valid_folder_name("a\nb"));
    }

    #[test]
    fn listing_keeps_sorted_directories_and_hides_dot_entries() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "rchamber-add-project-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("beta")).unwrap();
        std::fs::create_dir_all(root.join("Alpha")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("note.txt"), "content").unwrap();
        let entries = load_listing(&root, &[]);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "beta"]);
        assert!(entries.iter().all(|entry| !entry.added));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
