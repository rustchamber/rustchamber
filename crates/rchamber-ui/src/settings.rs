use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, ColorImage, CornerRadius, EventFilter, Id, Key, Layout, Margin,
    Modifiers, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, TextureHandle, Ui, UiBuilder, Vec2,
};
use rchamber_api::models::{Agent, Command, Model, Provider, Skill};
use rchamber_app::format::compact_number;
use rchamber_app::{Action, Connection};

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::logo;
use crate::motion;
use crate::theme::{self, Palette};
use crate::theme_store::{
    self, CODE_FONTS, CODE_SIZE_MAX, CODE_SIZE_MIN, DEFAULT_CODE_SIZE, DEFAULT_TERMINAL_SIZE,
    SPACING_SCALE_MAX, SPACING_SCALE_MIN, TERMINAL_SIZE_MAX, TERMINAL_SIZE_MIN, UI_FONTS,
    UI_SCALE_MAX, UI_SCALE_MIN, UI_WEIGHTS,
};
use crate::widgets;

const NAV_WIDTH: f32 = 280.0;
const NAV_PAD: f32 = 12.0;
const NAV_ROW_HEIGHT: f32 = 40.0;
const NAV_ROW_GAP: f32 = 2.0;
const NAV_GROUP_HEIGHT: f32 = 19.0;
const NAV_GROUP_TOP: f32 = 20.0;
const NAV_GROUP_BOTTOM: f32 = 2.0;
const SEARCH_BAND_HEIGHT: f32 = 32.0;

const CONTENT_MAX_WIDTH: f32 = 840.0;
const CONTENT_WIDE: f32 = 768.0;
const CONTENT_PAD_X: f32 = 48.0;
const CONTENT_PAD_X_NARROW: f32 = 24.0;
const CONTENT_PAD_Y: f32 = 32.0;
const CONTENT_PAD_Y_NARROW: f32 = 24.0;
const PAGE_HEADER_GAP: f32 = 32.0;
const SECTION_GAP: f32 = 20.0;
const SECTION_PAD_Y: f32 = 32.0;
const FIELD_ROW_GAP: f32 = 32.0;
const FIELD_LABEL_WIDTH: f32 = 224.0;
const FIELD_ROW_STACK_BELOW: f32 = 576.0;
const CONTROL_MAX_WIDTH: f32 = 384.0;
const CARD_PADDING: f32 = 16.0;
const CARD_GAP: f32 = 12.0;
const CARD_MIN_HEIGHT: f32 = 132.0;
const CARD_COLUMNS_WIDE: f32 = 768.0;
const CARD_COLUMNS_MEDIUM: f32 = 576.0;
const LIST_ROW_HEIGHT: f32 = 34.0;
const LIST_ROW_TALL_HEIGHT: f32 = 46.0;
const KEYCAP_HEIGHT: f32 = 22.0;
const KEYCAP_MIN_WIDTH: f32 = 22.0;
const KEYCAP_PAD_X: f32 = 7.0;
const KEYCAP_GAP: f32 = 5.0;
const KEYCAP_FONT: f32 = 13.0;
const SHORTCUT_ROW_HEIGHT: f32 = 44.0;
const SHORTCUT_GROUP_GAP: f32 = 20.0;

const APPEARANCE_TITLE_GAP: f32 = 14.0;
const APPEARANCE_ROW_HEIGHT: f32 = 40.0;
const APPEARANCE_GUTTER: f32 = 32.0;
const APPEARANCE_TWO_COLUMN_MIN: f32 = 432.0;
const APPEARANCE_CONTROL_HEIGHT: f32 = 36.0;
const APPEARANCE_FIELD_GAP: f32 = 16.0;
const THEME_SEARCH_HINT: &str = "Search themes…";

const WINDOW_WIDTH_RATIO: f32 = 0.9;
const WINDOW_HEIGHT_RATIO: f32 = 0.85;
const WINDOW_MAX_WIDTH: f32 = 980.0;
const WINDOW_MAX_HEIGHT: f32 = 900.0;
const WINDOW_MIN_WIDTH: f32 = 320.0;
const WINDOW_MIN_HEIGHT: f32 = 300.0;
const WINDOW_EDGE_GAP: f32 = 24.0;
const MODAL_SCALE_FROM: f32 = 0.98;
const REOPEN_AFTER: f64 = 0.25;

const BACKDROP_DOWNSAMPLE: usize = 6;
const BACKDROP_BLUR_RADIUS: usize = 4;
const BACKDROP_BLUR_PASSES: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Section {
    #[default]
    General,
    Appearance,
    Shortcuts,
    About,
    Projects,
    Providers,
    Agents,
    Commands,
    Skills,
}

impl Section {
    fn icon(self) -> Icon {
        match self {
            Section::General => Icon::Settings,
            Section::Appearance => Icon::Sun,
            Section::Shortcuts => Icon::Command,
            Section::About => Icon::Info,
            Section::Projects => Icon::Folder,
            Section::Providers => Icon::Spark,
            Section::Agents => Icon::Bot,
            Section::Commands => Icon::Terminal,
            Section::Skills => Icon::File,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    General,
    Workspace,
    OpenCode,
}

impl Group {
    const ALL: [Group; 3] = [Group::General, Group::Workspace, Group::OpenCode];

    fn label(self) -> &'static str {
        match self {
            Group::General => "RustChamber",
            Group::Workspace => "Workspace",
            Group::OpenCode => "OpenCode",
        }
    }
}

pub struct Page {
    section: Section,
    pub label: &'static str,
    pub description: &'static str,
    group: Group,
    keywords: &'static [&'static str],
}

pub static PAGES: [Page; 9] = [
    Page {
        section: Section::General,
        label: "General",
        description: "Connection, server process, and the settings file.",
        group: Group::General,
        keywords: &[
            "general",
            "connection",
            "endpoint",
            "password",
            "server",
            "binary",
            "cli",
            "reconnect",
            "settings file",
        ],
    },
    Page {
        section: Section::Appearance,
        label: "Appearance",
        description: "Theme, type, spacing, and motion.",
        group: Group::General,
        keywords: &[
            "theme", "font", "spacing", "density", "radius", "terminal", "code", "dark", "light",
            "system", "scale", "motion", "accent", "color",
        ],
    },
    Page {
        section: Section::Shortcuts,
        label: "Shortcuts",
        description: "Keys used across the app.",
        group: Group::General,
        keywords: &["keyboard", "hotkeys", "shortcuts", "bindings", "keys"],
    },
    Page {
        section: Section::About,
        label: "About",
        description: "Version, links, and build details.",
        group: Group::General,
        keywords: &["about", "version", "release", "changelog", "build"],
    },
    Page {
        section: Section::Projects,
        label: "Projects",
        description: "Folders open in this window.",
        group: Group::Workspace,
        keywords: &[
            "project",
            "projects",
            "repo",
            "repository",
            "directory",
            "folder",
        ],
    },
    Page {
        section: Section::Providers,
        label: "Providers",
        description: "Model providers OpenCode can use. Open one to browse its models.",
        group: Group::OpenCode,
        keywords: &[
            "provider",
            "providers",
            "models",
            "model",
            "api key",
            "openai",
            "anthropic",
            "ollama",
            "credentials",
        ],
    },
    Page {
        section: Section::Agents,
        label: "Agents",
        description: "Agents exposed by the connected server.",
        group: Group::OpenCode,
        keywords: &[
            "agent",
            "agents",
            "prompts",
            "tools",
            "permissions",
            "subagent",
        ],
    },
    Page {
        section: Section::Commands,
        label: "Commands",
        description: "Slash commands exposed by the connected server.",
        group: Group::OpenCode,
        keywords: &["command", "commands", "slash", "macros", "automation"],
    },
    Page {
        section: Section::Skills,
        label: "Skills",
        description: "Skills reported by the connected server.",
        group: Group::OpenCode,
        keywords: &["skill", "skills", "instructions", "tools"],
    },
];

pub fn open_page(ctx: &egui::Context, index: usize) {
    if let Some(page) = PAGES.get(index) {
        set(ctx, Id::unique("settings.section"), page.section);
    }
}

fn current_section(ctx: &egui::Context) -> Section {
    let section: Section = get(ctx, Id::unique("settings.section"));
    if PAGES.iter().any(|page| page.section == section) {
        section
    } else {
        Section::default()
    }
}

fn matches(page: &Page, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut haystack = String::new();
    haystack.push_str(page.label);
    haystack.push(' ');
    haystack.push_str(page.description);
    haystack.push(' ');
    for keyword in page.keywords {
        haystack.push_str(keyword);
        haystack.push(' ');
    }
    let words: Vec<String> = haystack
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    needle
        .split_whitespace()
        .all(|token| words.iter().any(|word| word.starts_with(token)))
}

fn get<T: Clone + Send + Sync + Default + 'static>(ctx: &egui::Context, id: Id) -> T {
    ctx.data(|data| data.get_temp::<T>(id)).unwrap_or_default()
}

fn set<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: Id, value: T) {
    ctx.data_mut(|data| data.insert_temp(id, value));
}

fn overlay_id() -> Id {
    Id::unique("settings/overlay")
}

fn backdrop_image_id() -> Id {
    Id::unique("settings/backdrop.image")
}

fn backdrop_texture_id() -> Id {
    Id::unique("settings/backdrop.texture")
}

fn backdrop_generation_id() -> Id {
    Id::unique("settings/backdrop.generation")
}

fn clear_backdrop(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.remove::<Arc<ColorImage>>(backdrop_image_id());
        data.remove::<TextureHandle>(backdrop_texture_id());
        data.remove::<f64>(backdrop_generation_id());
    });
}

fn request_backdrop(ctx: &egui::Context, generation: f64) {
    set(ctx, backdrop_generation_id(), generation);
    let context = ctx.clone();
    ctx.request_screenshot(move |image| {
        let current = context.data(|data| data.get_temp::<f64>(backdrop_generation_id()));
        if current != Some(generation) {
            return;
        }
        let image = Arc::new(blur_backdrop(&image));
        context.data_mut(|data| data.insert_temp(backdrop_image_id(), image));
        context.request_repaint();
    });
}

fn backdrop_texture(ctx: &egui::Context) -> Option<TextureHandle> {
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(backdrop_texture_id())) {
        return Some(texture);
    }
    let image = ctx.data(|data| data.get_temp::<Arc<ColorImage>>(backdrop_image_id()))?;
    let texture = ctx.load_texture("settings/backdrop", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(backdrop_texture_id(), texture.clone()));
    Some(texture)
}

fn blur_backdrop(image: &ColorImage) -> ColorImage {
    let [source_width, source_height] = image.size;
    if source_width == 0 || source_height == 0 {
        return ColorImage::filled([1, 1], Color32::BLACK);
    }
    let width = (source_width / BACKDROP_DOWNSAMPLE).max(1);
    let height = (source_height / BACKDROP_DOWNSAMPLE).max(1);
    let mut pixels = vec![[0.0_f32; 4]; width * height];
    for y in 0..height {
        let top = y * source_height / height;
        let bottom = (((y + 1) * source_height) / height)
            .max(top + 1)
            .min(source_height);
        for x in 0..width {
            let left = x * source_width / width;
            let right = (((x + 1) * source_width) / width)
                .max(left + 1)
                .min(source_width);
            let mut sum = [0.0_f32; 4];
            for row in top..bottom {
                for column in left..right {
                    let color = image.pixels[row * source_width + column];
                    sum[0] += color.r() as f32;
                    sum[1] += color.g() as f32;
                    sum[2] += color.b() as f32;
                    sum[3] += color.a() as f32;
                }
            }
            let count = ((bottom - top) * (right - left)) as f32;
            pixels[y * width + x] = [
                sum[0] / count,
                sum[1] / count,
                sum[2] / count,
                sum[3] / count,
            ];
        }
    }
    for _ in 0..BACKDROP_BLUR_PASSES {
        pixels = box_blur(&pixels, width, height, BACKDROP_BLUR_RADIUS);
    }
    let pixels = pixels
        .into_iter()
        .map(|color| {
            Color32::from_rgba_premultiplied(
                color[0].round().clamp(0.0, 255.0) as u8,
                color[1].round().clamp(0.0, 255.0) as u8,
                color[2].round().clamp(0.0, 255.0) as u8,
                color[3].round().clamp(0.0, 255.0) as u8,
            )
        })
        .collect();
    ColorImage::new([width, height], pixels)
}

fn box_blur(source: &[[f32; 4]], width: usize, height: usize, radius: usize) -> Vec<[f32; 4]> {
    let mut horizontal = vec![[0.0_f32; 4]; source.len()];
    let mut output = vec![[0.0_f32; 4]; source.len()];
    for y in 0..height {
        let row = y * width;
        for x in 0..width {
            let left = x.saturating_sub(radius);
            let right = (x + radius).min(width - 1);
            let mut sum = [0.0_f32; 4];
            for column in left..=right {
                for (channel, value) in sum.iter_mut().enumerate() {
                    *value += source[row + column][channel];
                }
            }
            let count = (right - left + 1) as f32;
            for (channel, value) in horizontal[row + x].iter_mut().enumerate() {
                *value = sum[channel] / count;
            }
        }
    }
    for y in 0..height {
        let top = y.saturating_sub(radius);
        let bottom = (y + radius).min(height - 1);
        let count = (bottom - top + 1) as f32;
        for x in 0..width {
            let mut sum = [0.0_f32; 4];
            for row in top..=bottom {
                for (channel, value) in sum.iter_mut().enumerate() {
                    *value += horizontal[row * width + x][channel];
                }
            }
            for (channel, value) in output[y * width + x].iter_mut().enumerate() {
                *value = sum[channel] / count;
            }
        }
    }
    output
}

#[derive(Clone)]
struct Overlay {
    open: bool,
    opened_at: f64,
    closed_at: Option<f64>,
    last_seen: f64,
    prev_focus: Option<Id>,
    search: String,
    search_index: usize,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            open: false,
            opened_at: 0.0,
            closed_at: None,
            last_seen: 0.0,
            prev_focus: None,
            search: String::new(),
            search_index: 0,
        }
    }
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

fn compact(ctx: &egui::Context) -> bool {
    get(ctx, Id::unique("settings.density.compact"))
}

pub fn show(ctx: &egui::Context, app: &mut RustChamber) {
    let now = ctx.input(|input| input.time);
    let mut overlay: Overlay = ctx
        .data(|data| data.get_temp::<Overlay>(overlay_id()))
        .unwrap_or_default();
    if !overlay.open || now - overlay.last_seen > REOPEN_AFTER {
        overlay = Overlay {
            open: true,
            opened_at: now,
            closed_at: None,
            last_seen: now,
            prev_focus: ctx.memory(|memory| memory.focused()),
            search: String::new(),
            search_index: 0,
        };
    }
    overlay.last_seen = now;

    if overlay.opened_at == now {
        clear_backdrop(ctx);
        request_backdrop(ctx, overlay.opened_at);
        set(
            ctx,
            Id::unique("settings.density.compact"),
            app.settings.compact_density,
        );
        set(
            ctx,
            Id::unique("settings.reduce_motion"),
            app.settings.reduce_motion,
        );
        set(
            ctx,
            Id::unique("settings.providers.selected"),
            String::new(),
        );
        set(
            ctx,
            Id::unique(("settings.search", "provider.models")),
            String::new(),
        );
    }

    let closing = overlay.closed_at.is_some();
    let duration = motion::duration_for(ctx, motion::FAST);
    let t = presence(now, overlay.opened_at, overlay.closed_at, duration);
    if t < 1.0 {
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    let search_id = Id::unique("settings/search");
    if !closing
        && !overlay.search.is_empty()
        && ctx.memory(|memory| memory.has_focus(search_id))
        && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
    {
        overlay.search.clear();
        overlay.search_index = 0;
    }

    let needle = overlay.search.trim().to_lowercase();
    let results: Vec<&Page> = PAGES.iter().filter(|page| matches(page, &needle)).collect();
    if results.is_empty() {
        overlay.search_index = 0;
    } else {
        overlay.search_index = overlay.search_index.min(results.len() - 1);
    }

    let mut open_result: Option<usize> = None;
    if !closing && !needle.is_empty() && ctx.memory(|memory| memory.has_focus(search_id)) {
        let down = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown));
        let up = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp));
        if down && !results.is_empty() {
            overlay.search_index = (overlay.search_index + 1) % results.len();
        }
        if up && !results.is_empty() {
            overlay.search_index = (overlay.search_index + results.len() - 1) % results.len();
        }
        if !results.is_empty()
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
        {
            open_result = Some(overlay.search_index);
        }
    }

    let mut p = app.palette.clone();
    if let Some(accent) = app.settings.accent.as_deref().and_then(parse_hex) {
        p.primary = accent;
    }
    let mut section = current_section(ctx);
    let mut next_section: Option<Section> = None;
    let mut close = false;

    let screen = ctx.content_rect();
    let max_width = (screen.width() - WINDOW_EDGE_GAP * 2.0).max(200.0);
    let max_height = (screen.height() - WINDOW_EDGE_GAP * 2.0).max(200.0);
    let width = (screen.width() * WINDOW_WIDTH_RATIO)
        .clamp(WINDOW_MIN_WIDTH.min(max_width), WINDOW_MAX_WIDTH)
        .min(max_width);
    let height = (screen.height() * WINDOW_HEIGHT_RATIO)
        .clamp(WINDOW_MIN_HEIGHT.min(max_height), WINDOW_MAX_HEIGHT)
        .min(max_height);

    let alpha = ((if p.dark { 102.0 } else { 64.0 }) * t)
        .round()
        .clamp(0.0, 255.0) as u8;
    let opacity = (255.0 * t).round().clamp(0.0, 255.0) as u8;
    let blur = backdrop_texture(ctx);
    egui::Area::new(Id::unique("settings/backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .interactable(false)
        .show(ctx, |ui| {
            if let Some(blur) = &blur {
                ui.painter().image(
                    blur.id(),
                    screen,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::from_white_alpha(opacity),
                );
            }
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha(alpha));
        });

    let shadow = egui::epaint::Shadow {
        offset: [0, 32],
        blur: 72,
        spread: 0,
        color: p.shadow,
    };

    let modal = egui::Modal::new(Id::unique("settings"))
        .frame(egui::Frame::NONE)
        .backdrop_color(Color32::TRANSPARENT)
        .show(ctx, |ui| {
            ui.set_opacity(if overlay.opened_at == now {
                0.0
            } else if closing {
                t
            } else {
                1.0
            });
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let (window, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
            paint_card(ui, window, &p, &shadow);

            let body = window;
            let nav_rect =
                Rect::from_min_max(body.min, Pos2::new(body.left() + NAV_WIDTH, body.bottom()));
            let content_rect =
                Rect::from_min_max(Pos2::new(nav_rect.right(), body.top()), body.max);
            let corner = theme::radius::XL.saturating_sub(1);
            ui.painter().rect_filled(
                nav_rect,
                CornerRadius {
                    nw: corner,
                    sw: corner,
                    ne: 0,
                    se: 0,
                },
                p.bg_soft,
            );
            ui.painter().vline(
                nav_rect.right(),
                nav_rect.top()..=nav_rect.bottom(),
                Stroke::new(theme::BORDER, p.border),
            );

            let field = Rect::from_min_size(
                Pos2::new(nav_rect.left() + NAV_PAD, nav_rect.top() + 12.0),
                Vec2::new(NAV_WIDTH - NAV_PAD * 2.0, SEARCH_BAND_HEIGHT),
            );
            let mut field_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(field)
                    .layout(Layout::top_down(Align::Min)),
            );
            if overlay.opened_at == now {
                ctx.memory_mut(|memory| memory.request_focus(search_id));
            }
            nav_search(&mut field_ui, &p, &mut overlay.search, search_id, field);

            let list_rect = Rect::from_min_max(
                Pos2::new(nav_rect.left() + NAV_PAD, field.bottom() + 16.0),
                Pos2::new(nav_rect.right() - NAV_PAD, nav_rect.bottom() - NAV_PAD),
            );
            let mut nav_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(list_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            nav_ui.set_clip_rect(list_rect);
            ScrollArea::vertical()
                .id_salt("settings/nav")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, false])
                .show(&mut nav_ui, |ui| {
                    ui.set_width(list_rect.width());
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    if needle.is_empty() {
                        for group in Group::ALL {
                            let pages: Vec<&Page> =
                                PAGES.iter().filter(|page| page.group == group).collect();
                            if pages.is_empty() {
                                continue;
                            }
                            nav_group_heading(ui, &p, group.label());
                            for page in pages {
                                if nav_row(ui, &p, page, page.section == section).clicked() {
                                    next_section = Some(page.section);
                                }
                                ui.add_space(NAV_ROW_GAP);
                            }
                        }
                    } else if results.is_empty() {
                        ui.add_space(24.0);
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), theme::LINE_META),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "No matching settings",
                            theme::font(theme::TEXT_META),
                            p.fg_muted,
                        );
                        ui.add_space(24.0);
                    } else {
                        for group in Group::ALL {
                            let group_results: Vec<(usize, &Page)> = results
                                .iter()
                                .enumerate()
                                .filter(|(_, page)| page.group == group)
                                .map(|(index, page)| (index, *page))
                                .collect();
                            if group_results.is_empty() {
                                continue;
                            }
                            nav_group_heading(ui, &p, group.label());
                            for (index, page) in group_results {
                                let active = index == overlay.search_index;
                                if search_row(ui, &p, page, active).clicked() {
                                    open_result = Some(index);
                                }
                                ui.add_space(NAV_ROW_GAP);
                            }
                        }
                    }
                    ui.add_space(8.0);
                });

            let mut content_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(content_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            content_ui.set_clip_rect(content_rect);
            ScrollArea::vertical()
                .id_salt(("settings/content", section as u8))
                .max_height(content_rect.height())
                .auto_shrink([false, false])
                .show(&mut content_ui, |ui| {
                    ui.set_width(content_rect.width());
                    page_column(ui, |ui| match section {
                        Section::General => general(ui, app, &p),
                        Section::Appearance => appearance(ui, app, &p),
                        Section::Shortcuts => shortcuts(ui, &p),
                        Section::About => about(ui, app, &p),
                        Section::Projects => projects(ui, app, &p),
                        Section::Providers => providers(ui, app, &p),
                        Section::Agents => agents(ui, app, &p),
                        Section::Commands => commands(ui, app, &p),
                        Section::Skills => skills(ui, app, &p),
                    });
                });

            let close_rect = Rect::from_min_size(
                Pos2::new(window.right() - 2.0 - 28.0, window.top() + 2.0),
                Vec2::splat(28.0),
            );
            let mut close_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(close_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            if widgets::icon_button_hit(&mut close_ui, Icon::Close, 14.0, 28.0, p.fg_muted)
                .on_hover_text("Close settings")
                .clicked()
            {
                close = true;
            }
        });

    let layer = modal.response.layer_id;
    let scale = MODAL_SCALE_FROM + (1.0 - MODAL_SCALE_FROM) * t;
    if t < 0.999 {
        let center = modal.response.rect.center().to_vec2();
        ctx.set_transform_layer(
            layer,
            egui::emath::TSTransform::new(center * (1.0 - scale), scale),
        );
    } else {
        ctx.set_transform_layer(layer, egui::emath::TSTransform::IDENTITY);
    }
    if !closing && modal.should_close() {
        close = true;
    }
    if let Some(index) = open_result
        && !closing
        && let Some(page) = PAGES.get(index)
    {
        next_section = Some(page.section);
    }
    if let Some(next) = next_section
        && !closing
    {
        section = next;
        set(ctx, Id::unique("settings.section"), section);
        if section != Section::Providers {
            set(
                ctx,
                Id::unique("settings.providers.selected"),
                String::new(),
            );
            set(
                ctx,
                Id::unique(("settings.search", "provider.models")),
                String::new(),
            );
        }
    }
    if close && !closing {
        overlay.closed_at = Some(now);
        ctx.request_repaint();
    }
    if closing && t <= 0.0001 {
        ctx.set_transform_layer(layer, egui::emath::TSTransform::IDENTITY);
        clear_backdrop(ctx);
        if !app.ui.palette_open
            && let Some(previous) = overlay.prev_focus
        {
            ctx.memory_mut(|memory| memory.request_focus(previous));
        }
        app.ui.show_settings = false;
        overlay = Overlay::default();
    }
    ctx.data_mut(|data| data.insert_temp(overlay_id(), overlay));
}

fn paint_card(ui: &Ui, rect: Rect, p: &Palette, shadow: &egui::epaint::Shadow) {
    let corner = CornerRadius::same(theme::radius::XL);
    ui.painter()
        .add(egui::Shape::Rect(shadow.as_shape(rect, corner)));
    ui.painter().rect_filled(rect, corner, p.bg);
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(theme::HAIRLINE, p.elevation_ring),
        egui::StrokeKind::Inside,
    );
}

fn nav_search(ui: &mut Ui, p: &Palette, query: &mut String, id: Id, rect: Rect) {
    let focused = ui.memory(|memory| memory.has_focus(id));
    let response = ui.interact(rect, id.with("band"), Sense::click());
    let corner = CornerRadius::same(theme::radius::MD);
    ui.painter().rect_filled(rect, corner, p.alpha(p.card, 178));
    let (width, color) = if focused {
        (theme::BORDER, p.alpha(p.fg, 66))
    } else {
        (theme::HAIRLINE, p.alpha(p.fg, 41))
    };
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(width, color),
        egui::StrokeKind::Inside,
    );
    icons::paint(
        ui,
        Icon::Search,
        Rect::from_center_size(
            Pos2::new(rect.left() + 16.0, rect.center().y),
            Vec2::splat(16.0),
        ),
        p.fg_muted,
    );
    let text_right = if query.is_empty() {
        rect.right() - 8.0
    } else {
        rect.right() - 30.0
    };
    let text_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 34.0, rect.top() + 4.0),
        Pos2::new(text_right, rect.bottom() - 4.0),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let edit = egui::TextEdit::singleline(query)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(Margin::ZERO)
        .hint_text(
            RichText::new("Search settings")
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg_muted),
        )
        .text_color(p.fg)
        .font(theme::font(theme::TEXT_UI_LABEL))
        .desired_width(f32::INFINITY);
    let edit_response = child.add(edit);
    if response.clicked() {
        edit_response.request_focus();
    }
    if !query.is_empty() {
        let clear = Rect::from_center_size(
            Pos2::new(rect.right() - 18.0, rect.center().y),
            Vec2::splat(20.0),
        );
        let clear_response = ui.interact(clear, id.with("clear"), Sense::click());
        icons::paint(
            ui,
            Icon::Close,
            Rect::from_center_size(clear.center(), Vec2::splat(14.0)),
            if clear_response.hovered() {
                p.fg
            } else {
                p.fg_muted
            },
        );
        if clear_response.clicked() {
            query.clear();
        }
    }
    ui.ctx().memory_mut(|memory| {
        memory.set_focus_lock_filter(
            id,
            EventFilter {
                tab: true,
                horizontal_arrows: false,
                vertical_arrows: false,
                escape: false,
            },
        )
    });
}

fn nav_group_heading(ui: &mut Ui, p: &Palette, label: &str) {
    ui.add_space(NAV_GROUP_TOP);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), NAV_GROUP_HEIGHT),
        Sense::hover(),
    );
    ui.painter().text(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label.to_uppercase(),
        theme::font_semibold(11.0),
        p.fg_muted,
    );
    ui.add_space(NAV_GROUP_BOTTOM);
}

fn nav_row(ui: &mut Ui, p: &Palette, page: &Page, active: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), NAV_ROW_HEIGHT),
        Sense::click(),
    );
    crate::widgets::pointer(ui, &response);
    let pressed = response.is_pointer_button_down_on();
    let fill = if active {
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
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), fill);
    }
    icons::paint(
        ui,
        page.section.icon(),
        Rect::from_center_size(
            Pos2::new(rect.left() + 16.0, rect.center().y),
            Vec2::splat(16.0),
        ),
        if active { p.fg } else { p.fg_muted },
    );
    let label_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 32.0, rect.top()),
        Pos2::new(rect.right() - 8.0, rect.bottom()),
    );
    ui.painter().with_clip_rect(label_rect).text(
        Pos2::new(rect.left() + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        page.label,
        theme::font(theme::TEXT_UI_LABEL),
        p.fg,
    );
    response
}

fn search_row(ui: &mut Ui, p: &Palette, page: &Page, active: bool) -> egui::Response {
    let width = ui.available_width();
    let text_width = (width - 16.0).max(0.0);
    let has_description = !page.description.is_empty();
    let description_font = theme::font(theme::TEXT_MICRO);
    let description_line = theme::row_height(ui, &description_font);
    let description = ui.painter().layout(
        page.description.to_string(),
        description_font,
        p.fg_muted,
        text_width,
    );
    let description_height = if has_description {
        description.size().y.min(description_line * 2.0)
    } else {
        0.0
    };
    let height = 12.0 + theme::LINE_LABEL + description_height;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    crate::widgets::pointer(ui, &response);
    let fill = if active {
        p.selection
    } else if response.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), fill);
    }
    let text_left = rect.left() + 8.0;
    let text_right = rect.right() - 8.0;
    ui.painter().text(
        Pos2::new(text_left, rect.top() + 6.0 + theme::LINE_LABEL / 2.0),
        Align2::LEFT_CENTER,
        page.label,
        theme::font(theme::TEXT_UI_LABEL),
        p.fg,
    );
    if has_description {
        let clip = Rect::from_min_max(
            Pos2::new(text_left, rect.top() + 6.0 + theme::LINE_LABEL),
            Pos2::new(
                text_right,
                rect.top() + 6.0 + theme::LINE_LABEL + description_height,
            ),
        );
        ui.painter().with_clip_rect(clip).galley(
            Pos2::new(text_left, clip.top()),
            description,
            p.fg_muted,
        );
    }
    response
}

fn section_pad(ui: &Ui) -> f32 {
    if compact(ui.ctx()) {
        SECTION_PAD_Y * 0.75
    } else {
        SECTION_PAD_Y
    }
}

fn page_column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let available = ui.available_width();
    let box_width = available.min(CONTENT_MAX_WIDTH);
    let wide = box_width >= CONTENT_WIDE;
    let pad_x = if wide {
        CONTENT_PAD_X
    } else {
        CONTENT_PAD_X_NARROW
    };
    let pad_y = if wide {
        CONTENT_PAD_Y
    } else {
        CONTENT_PAD_Y_NARROW
    };
    let inner = (box_width - pad_x * 2.0).max(0.0);
    let offset = ((available - box_width) / 2.0).max(0.0);
    set(ui.ctx(), Id::unique("settings.page.pad"), pad_x);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add_space(offset + pad_x);
        ui.allocate_ui_with_layout(Vec2::new(inner, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(inner);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.add_space(pad_y);
            add(ui);
            ui.add_space(pad_y);
        });
    });
}

fn pane_width(ui: &Ui) -> f32 {
    ui.available_width() + 2.0 * get::<f32>(ui.ctx(), Id::unique("settings.page.pad"))
}

fn description_text(ui: &mut Ui, p: &Palette, text: &str, font: egui::FontId) {
    let width = ui.available_width().max(1.0);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color: p.fg_muted,
            line_height: Some(theme::TEXT_META * 1.5),
            ..Default::default()
        },
    );
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, p.fg_muted);
}

fn page_header(ui: &mut Ui, p: &Palette, title: &str, description: &str) {
    ui.label(
        RichText::new(title)
            .font(theme::font_semibold(theme::TEXT_PAGE_TITLE))
            .color(p.fg),
    );
    if !description.is_empty() {
        ui.add_space(4.0);
        description_text(ui, p, description, theme::font(theme::TEXT_META));
    }
    ui.add_space(PAGE_HEADER_GAP);
}

fn section<R>(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    description: &str,
    suffix: Option<&str>,
    divider: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let pad = section_pad(ui);
    if divider {
        let rect = ui.available_rect_before_wrap();
        ui.painter().hline(
            rect.left()..=rect.right(),
            rect.top(),
            Stroke::new(theme::BORDER, p.edge_color()),
        );
        ui.add_space(pad);
    }
    if !title.is_empty() || !description.is_empty() {
        if !title.is_empty() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(
                    RichText::new(title)
                        .font(theme::font_semibold(theme::TEXT_UI_LABEL))
                        .color(p.fg),
                );
                if let Some(suffix) = suffix {
                    ui.label(
                        RichText::new(suffix)
                            .font(theme::font(theme::TEXT_MICRO))
                            .color(p.fg_muted),
                    );
                }
            });
        }
        if !description.is_empty() {
            ui.add_space(4.0);
            description_text(ui, p, description, theme::font(theme::TEXT_META));
        }
        ui.add_space(SECTION_GAP);
    }
    let inner = add(ui);
    ui.add_space(pad);
    inner
}

fn field_row(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    description: Option<&str>,
    add: impl FnOnce(&mut Ui),
) {
    ui.add_space(4.0);
    if pane_width(ui) >= FIELD_ROW_STACK_BELOW {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                Vec2::new(FIELD_LABEL_WIDTH, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(FIELD_LABEL_WIDTH);
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(label)
                                .font(theme::font(theme::TEXT_UI_LABEL))
                                .color(p.fg),
                        )
                        .truncate(),
                    );
                    if let Some(description) = description {
                        ui.add(
                            egui::Label::new(
                                RichText::new(description)
                                    .font(theme::font(theme::TEXT_META))
                                    .color(p.fg_muted),
                            )
                            .wrap(),
                        );
                    }
                },
            );
            ui.add_space(FIELD_ROW_GAP);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                add(ui);
            });
        });
    } else {
        ui.label(
            RichText::new(label)
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg),
        );
        if let Some(description) = description {
            ui.add_space(2.0);
            ui.add(
                egui::Label::new(
                    RichText::new(description)
                        .font(theme::font(theme::TEXT_META))
                        .color(p.fg_muted),
                )
                .wrap(),
            );
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            add(ui);
        });
    }
    ui.add_space(4.0);
}

fn stacked_field(ui: &mut Ui, p: &Palette, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.label(
        RichText::new(label)
            .font(theme::font(theme::TEXT_UI_LABEL))
            .color(p.fg),
    );
    ui.add_space(8.0);
    let width = ui.available_width().min(CONTROL_MAX_WIDTH);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.allocate_ui_with_layout(
            Vec2::new(width, 0.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_max_width(width);
                add(ui);
            },
        );
    });
}

fn draw_minus(ui: &Ui, center: Pos2, color: Color32) {
    ui.painter().hline(
        (center.x - 4.0)..=(center.x + 4.0),
        center.y,
        Stroke::new(theme::BORDER, color),
    );
}

fn draw_plus(ui: &Ui, center: Pos2, color: Color32) {
    ui.painter().hline(
        (center.x - 4.0)..=(center.x + 4.0),
        center.y,
        Stroke::new(theme::BORDER, color),
    );
    ui.painter().vline(
        center.x,
        (center.y - 4.0)..=(center.y + 4.0),
        Stroke::new(theme::BORDER, color),
    );
}

fn stepper_sized(
    ui: &mut Ui,
    p: &Palette,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    width: f32,
) -> Option<f32> {
    let width = width.max(96.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::hover());
    let id = response.id;
    let corner = CornerRadius::same(theme::radius::MD);
    ui.painter().rect_filled(rect, corner, p.alpha(p.card, 178));
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(theme::HAIRLINE, p.alpha(p.fg, 41)),
        egui::StrokeKind::Inside,
    );
    let minus = Rect::from_min_size(rect.min, Vec2::new(30.0, rect.height()));
    let plus = Rect::from_min_size(
        Pos2::new(rect.right() - 30.0, rect.top()),
        Vec2::new(30.0, rect.height()),
    );
    let minus_response = ui.interact(minus, id.with("minus"), Sense::click());
    let plus_response = ui.interact(plus, id.with("plus"), Sense::click());
    let mut next = None;
    if minus_response.clicked() && value > min + 0.0001 {
        next = Some((value - step).max(min));
    }
    if plus_response.clicked() && value < max - 0.0001 {
        next = Some((value + step).min(max));
    }
    let enabled = p.fg;
    draw_minus(
        ui,
        minus.center(),
        if value > min + 0.0001 {
            enabled
        } else {
            p.disabled
        },
    );
    draw_plus(
        ui,
        plus.center(),
        if value < max - 0.0001 {
            enabled
        } else {
            p.disabled
        },
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!("{value:.0}"),
        theme::font_mono(theme::TEXT_META),
        p.fg,
    );
    if minus_response.hovered() || plus_response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    next
}

fn unit_stepper(
    ui: &mut Ui,
    p: &Palette,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    unit: &str,
    reset: f32,
) -> Option<f32> {
    let unit_width = ui
        .painter()
        .layout_no_wrap(unit.to_string(), theme::font(theme::TEXT_META), p.fg_muted)
        .size()
        .x;
    let available = ui.available_width();
    let width = (available - unit_width - 28.0 - 16.0).clamp(96.0, 180.0);
    let mut next = stepper_sized(ui, p, value, min, max, step, width);
    ui.add_space(8.0);
    let (unit_rect, _) =
        ui.allocate_exact_size(Vec2::new(unit_width.max(8.0), 18.0), Sense::hover());
    ui.painter().text(
        unit_rect.center(),
        Align2::CENTER_CENTER,
        unit,
        theme::font(theme::TEXT_META),
        p.fg_muted,
    );
    ui.add_space(8.0);
    let at_reset = (value - reset).abs() < 0.0001;
    let reset_response = widgets::icon_button_hit(
        ui,
        Icon::Refresh,
        13.0,
        28.0,
        if at_reset { p.disabled } else { p.fg_muted },
    )
    .on_hover_text("Reset");
    if !at_reset && reset_response.clicked() {
        next = Some(reset);
    }
    next
}

fn percent_stepper(
    ui: &mut Ui,
    p: &Palette,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    reset: f32,
) -> Option<f32> {
    unit_stepper(ui, p, value, min, max, step, "%", reset)
}

fn text_field(ui: &mut Ui, text: &mut String, hint: &str, password: bool, p: &Palette) {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, widgets::INPUT_HEIGHT), Sense::click());
    let id = response.id.with("field");
    let focused = ui.memory(|memory| memory.has_focus(id));
    let corner = CornerRadius::same(theme::radius::MD);
    ui.painter().rect_filled(rect, corner, p.alpha(p.card, 178));
    let (width, color) = if focused {
        (theme::BORDER, p.alpha(p.fg, 66))
    } else {
        (theme::HAIRLINE, p.alpha(p.fg, 41))
    };
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(width, color),
        egui::StrokeKind::Inside,
    );
    let text_rect = rect.shrink2(Vec2::new(10.0, 5.0));
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let edit = egui::TextEdit::singleline(text)
        .id(id)
        .password(password)
        .frame(egui::Frame::NONE)
        .margin(Margin::ZERO)
        .hint_text(
            RichText::new(hint)
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg_muted),
        )
        .font(theme::font(theme::TEXT_UI_LABEL))
        .text_color(p.fg)
        .desired_width(f32::INFINITY);
    let edit_response = child.add(edit);
    if response.clicked() {
        edit_response.request_focus();
    }
}

fn matches_query(needle: &str, fields: &[&str]) -> bool {
    needle.is_empty()
        || fields
            .iter()
            .any(|field| field.to_lowercase().contains(needle))
}

fn search_state(ui: &mut Ui, key: &str, hint: &str, p: &Palette) -> String {
    let data_id = Id::unique(("settings.search", key));
    let field_id = Id::unique(("settings.search.field", key));
    let mut query: String = get(ui.ctx(), data_id);
    let width = ui.available_width().min(CONTROL_MAX_WIDTH);
    ui.allocate_ui_with_layout(
        Vec2::new(width, 0.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(width);
            widgets::search_field(ui, &mut query, hint, p, field_id);
        },
    );
    set(ui.ctx(), data_id, query.clone());
    query
}

fn muted(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(text)
                .font(theme::font(theme::TEXT_META))
                .color(p.fg_muted),
        )
        .wrap(),
    );
}

fn fit_text(ui: &Ui, text: &str, font: &egui::FontId, width: f32) -> String {
    let measured =
        ui.painter()
            .layout_no_wrap(text.to_string(), font.clone(), Color32::PLACEHOLDER);
    if measured.size().x <= width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let mut candidate: String = chars.iter().collect();
        candidate.push('…');
        let measured =
            ui.painter()
                .layout_no_wrap(candidate.clone(), font.clone(), Color32::PLACEHOLDER);
        if measured.size().x <= width {
            return candidate;
        }
    }
    String::new()
}

fn paint_badge(ui: &Ui, p: &Palette, left: f32, center_y: f32, text: &str) -> f32 {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_string(), theme::font(theme::TEXT_MICRO), p.fg_muted);
    let size = Vec2::new(galley.size().x + 8.0, 15.0);
    let rect = Rect::from_min_size(Pos2::new(left, center_y - size.y / 2.0), size);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::radius::XS),
        p.alpha(p.fg, 10),
    );
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        p.fg_muted,
    );
    rect.right()
}

fn list_row(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    badge: Option<&str>,
    description: Option<&str>,
    selected: bool,
) -> egui::Response {
    let has_description = description.is_some();
    let height = if has_description {
        LIST_ROW_TALL_HEIGHT
    } else {
        LIST_ROW_HEIGHT
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    crate::widgets::pointer(ui, &response);
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.selection);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::MD), p.hover);
    }
    let left = rect.left() + 8.0;
    let right = rect.right() - 8.0;
    let title_y = if has_description {
        rect.top() + 13.0
    } else {
        rect.center().y
    };
    let font = theme::font(theme::TEXT_UI_LABEL);
    let badge_width = badge
        .map(|label| {
            ui.painter()
                .layout_no_wrap(
                    label.to_string(),
                    theme::font(theme::TEXT_MICRO),
                    p.fg_muted,
                )
                .size()
                .x
                + 14.0
        })
        .unwrap_or(0.0);
    let title = fit_text(ui, title, &font, (right - left - badge_width).max(0.0));
    let galley = ui.painter().layout_no_wrap(title, font, p.fg);
    let clip = Rect::from_min_max(
        Pos2::new(left, title_y - 10.0),
        Pos2::new(right, title_y + 10.0),
    );
    let title_width = galley.size().x;
    ui.painter().with_clip_rect(clip).galley(
        Pos2::new(left, title_y - galley.size().y / 2.0),
        galley,
        p.fg,
    );
    if let Some(badge) = badge {
        paint_badge(ui, p, left + title_width + 6.0, title_y, badge);
    }
    if let Some(description) = description {
        let description = fit_text(
            ui,
            description,
            &theme::font(theme::TEXT_MICRO),
            (right - left).max(0.0),
        );
        let clip = Rect::from_min_max(
            Pos2::new(left, rect.top() + 24.0),
            Pos2::new(right, rect.bottom() - 2.0),
        );
        ui.painter().with_clip_rect(clip).text(
            Pos2::new(left, rect.top() + 33.0),
            Align2::LEFT_CENTER,
            description,
            theme::font(theme::TEXT_MICRO),
            p.fg_muted,
        );
    }
    response
}

fn total_row(ui: &mut Ui, p: &Palette, label: &str) {
    ui.label(
        RichText::new(label)
            .font(theme::font(theme::TEXT_META))
            .color(p.fg_muted),
    );
}

fn provider_name(provider: &Provider) -> &str {
    if provider.name.trim().is_empty() {
        &provider.id
    } else {
        &provider.name
    }
}

fn provider_initial(provider: &Provider) -> String {
    provider_name(provider)
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_default()
}

fn parse_hex(text: &str) -> Option<Color32> {
    let trimmed = text.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        return Some(Color32::from_rgb(r, g, b));
    }
    None
}

fn hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

fn accent_field(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let base = if p.dark {
        Palette::dark()
    } else {
        Palette::light()
    };
    let swatches = [
        ("Amber", base.primary),
        ("Blue", base.blue),
        ("Green", base.green),
        ("Purple", base.purple),
        ("Coral", base.coral),
    ];
    let current = app.settings.accent.as_deref().and_then(parse_hex);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (name, color) in swatches {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::radius::INLINE_CODE), color);
            let selected = current
                .map(|accent| accent.to_array() == color.to_array())
                .unwrap_or(color.to_array() == base.primary.to_array());
            if selected {
                ui.painter().rect_stroke(
                    rect.expand(2.0),
                    CornerRadius::same(theme::radius::MD),
                    Stroke::new(theme::BORDER, p.fg),
                    egui::StrokeKind::Inside,
                );
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.on_hover_text(name).clicked() {
                app.settings.accent = Some(hex(color));
                app.settings.save();
                app.apply_theme(ui.ctx());
            }
        }
        if app.settings.accent.is_some() {
            let reset = widgets::icon_button_hit(ui, Icon::Refresh, 13.0, 28.0, p.fg_muted);
            if reset.on_hover_text("Reset").clicked() {
                app.settings.accent = None;
                app.settings.save();
                app.apply_theme(ui.ctx());
            }
        }
    });
}

fn appearance_options(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(id, label)| ((*id).to_string(), (*label).to_string()))
        .collect()
}

fn appearance_section(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    divider: bool,
    add: impl FnOnce(&mut Ui),
) {
    if divider {
        ui.add_space(12.0);
        let rect = ui.available_rect_before_wrap();
        let y = ui.cursor().top();
        ui.painter().hline(
            rect.left()..=rect.right(),
            y,
            Stroke::new(theme::BORDER, p.edge_color()),
        );
        ui.add_space(12.0 + theme::BORDER);
    }
    ui.label(
        RichText::new(title)
            .font(theme::font_semibold(theme::TEXT_UI_LABEL))
            .color(p.fg),
    );
    ui.add_space(APPEARANCE_TITLE_GAP);
    add(ui);
}

fn appearance_columns(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    if width >= APPEARANCE_TWO_COLUMN_MIN {
        let column = ((width - APPEARANCE_GUTTER) / 2.0).max(0.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = APPEARANCE_GUTTER;
            ui.allocate_ui_with_layout(
                Vec2::new(column, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(column);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    left(ui);
                },
            );
            ui.allocate_ui_with_layout(
                Vec2::new(column, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(column);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    right(ui);
                },
            );
        });
    } else {
        left(ui);
        ui.add_space(12.0);
        right(ui);
    }
}

fn appearance_field(ui: &mut Ui, p: &Palette, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.add(
        egui::Label::new(
            RichText::new(label)
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg),
        )
        .truncate(),
    );
    ui.add_space(6.0);
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        Vec2::new(width, APPEARANCE_CONTROL_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(width);
            add(ui);
        },
    );
    ui.add_space(APPEARANCE_FIELD_GAP);
}

fn appearance_inline_row(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    control_width: f32,
    add: impl FnOnce(&mut Ui),
) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        Vec2::new(width, APPEARANCE_ROW_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_height(APPEARANCE_ROW_HEIGHT);
            ui.add(
                egui::Label::new(
                    RichText::new(label)
                        .font(theme::font(theme::TEXT_UI_LABEL))
                        .color(p.fg),
                )
                .truncate(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let control = control_width.min(ui.available_width().max(0.0));
                ui.allocate_ui_with_layout(
                    Vec2::new(control, APPEARANCE_ROW_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_width(control);
                        add(ui);
                    },
                );
            });
        },
    );
}

fn appearance_select(
    ui: &mut Ui,
    selected: &mut String,
    options: &[(String, String)],
    placeholder: &str,
    searchable: bool,
    p: &Palette,
) -> bool {
    if searchable {
        widgets::searchable_select(ui, selected, options, placeholder, THEME_SEARCH_HINT, p)
    } else {
        widgets::select(ui, selected, options, placeholder, p)
    }
}

fn commit_appearance(app: &mut RustChamber, ctx: &egui::Context) {
    app.apply_theme(ctx);
    app.settings.save();
}

fn brightness_control(ui: &mut Ui, value: &mut f32, p: &Palette) -> egui::Response {
    let width = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(width, widgets::SLIDER_HEIGHT), Sense::hover());
    let center_y = rect.center().y;
    icons::paint(
        ui,
        Icon::Sun,
        Rect::from_center_size(Pos2::new(rect.left() + 8.0, center_y), Vec2::splat(16.0)),
        if *value >= 0.5 { p.fg } else { p.fg_muted },
    );
    let percent_width = 42.0;
    let moon_center = Pos2::new(rect.right() - percent_width - 14.0, center_y);
    icons::paint(
        ui,
        Icon::Moon,
        Rect::from_center_size(moon_center, Vec2::splat(16.0)),
        if *value < 0.5 { p.fg } else { p.fg_muted },
    );
    let slider_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 24.0, rect.top()),
        Pos2::new(moon_center.x - 18.0, rect.bottom()),
    );
    let mut slider_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(slider_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = widgets::slider(&mut slider_ui, value, 0.0, 1.0, p);
    ui.painter().text(
        Pos2::new(rect.right(), center_y),
        Align2::RIGHT_CENTER,
        format!("{:.0}%", *value * 100.0),
        theme::font_mono(theme::TEXT_META),
        p.fg,
    );
    response
}

fn appearance(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    page_header(ui, p, "Appearance", "Theme, type, spacing, and motion.");

    let light_options = theme_store::theme_options(false);
    let dark_options = theme_store::theme_options(true);
    let mut light_theme = app.settings.appearance.light_theme.clone();
    let mut dark_theme = app.settings.appearance.dark_theme.clone();
    appearance_section(ui, p, "Themes", false, |ui| {
        appearance_columns(
            ui,
            |ui| {
                appearance_field(ui, p, "Light theme", |ui| {
                    appearance_select(
                        ui,
                        &mut light_theme,
                        &light_options,
                        "Select theme",
                        true,
                        p,
                    );
                });
            },
            |ui| {
                appearance_field(ui, p, "Dark theme", |ui| {
                    appearance_select(ui, &mut dark_theme, &dark_options, "Select theme", true, p);
                });
            },
        );
        let mut theme_changed = false;
        if light_theme != app.settings.appearance.light_theme {
            app.settings.appearance.light_theme = light_theme;
            theme_changed = true;
        }
        if dark_theme != app.settings.appearance.dark_theme {
            app.settings.appearance.dark_theme = dark_theme;
            theme_changed = true;
        }
        if theme_changed {
            commit_appearance(app, ui.ctx());
        }
        ui.add_space(2.0);
        let accent_width = if app.settings.accent.is_some() {
            188.0
        } else {
            152.0
        };
        appearance_inline_row(ui, p, "Accent", accent_width, |ui| accent_field(ui, app, p));
    });

    appearance_section(ui, p, "Brightness", true, |ui| {
        let mut brightness = app.settings.appearance.brightness;
        let response = brightness_control(ui, &mut brightness, p);
        if response.changed() {
            app.settings.appearance.brightness = brightness;
            app.apply_theme(ui.ctx());
        }
        if response.drag_stopped() || response.clicked() {
            app.settings.appearance.brightness = brightness;
            app.settings.save();
        }
        ui.add_space(4.0);
        muted(ui, p, "0 = dark theme, 100 = light theme");
    });

    let languages = appearance_options(&[("en", "English"), ("pt-BR", "Português (Brasil)")]);
    let time_formats =
        appearance_options(&[("auto", "Auto"), ("12h", "12-hour"), ("24h", "24-hour")]);
    let week_starts =
        appearance_options(&[("auto", "Auto"), ("sunday", "Sunday"), ("monday", "Monday")]);
    let mut language = app.settings.appearance.language.clone();
    let mut time_format = app.settings.appearance.time_format.clone();
    let mut week_start = app.settings.appearance.week_start.clone();
    appearance_section(ui, p, "Localization", true, |ui| {
        appearance_columns(
            ui,
            |ui| {
                appearance_field(ui, p, "Language", |ui| {
                    appearance_select(ui, &mut language, &languages, "English", false, p);
                });
            },
            |ui| {
                appearance_field(ui, p, "Time format", |ui| {
                    appearance_select(ui, &mut time_format, &time_formats, "Auto", false, p);
                });
                appearance_field(ui, p, "Week starts on", |ui| {
                    appearance_select(ui, &mut week_start, &week_starts, "Auto", false, p);
                });
            },
        );
        let mut changed = false;
        if language != app.settings.appearance.language {
            app.settings.appearance.language = language;
            changed = true;
        }
        if time_format != app.settings.appearance.time_format {
            app.settings.appearance.time_format = time_format;
            changed = true;
        }
        if week_start != app.settings.appearance.week_start {
            app.settings.appearance.week_start = week_start;
            changed = true;
        }
        if changed {
            app.settings.save();
        }
    });

    let ui_fonts = appearance_options(&UI_FONTS);
    let code_fonts = appearance_options(&CODE_FONTS);
    let weight_labels: Vec<&str> = UI_WEIGHTS.iter().map(|(_, label)| *label).collect();
    let appearance_settings = &app.settings.appearance;
    let mut ui_font = appearance_settings.ui_font.clone();
    let mut code_font = appearance_settings.code_font.clone();
    let mut ui_scale = appearance_settings.ui_scale;
    let mut code_size = appearance_settings.code_size;
    let mut terminal_size = appearance_settings.terminal_size;
    let mut spacing_scale = appearance_settings.spacing_scale;
    let mut weight = UI_WEIGHTS
        .iter()
        .position(|(id, _)| *id == appearance_settings.weight())
        .unwrap_or(0);
    let mut animated = appearance_settings.animated;
    appearance_section(ui, p, "Density and typography", true, |ui| {
        appearance_columns(
            ui,
            |ui| {
                appearance_field(ui, p, "Interface font", |ui| {
                    appearance_select(ui, &mut ui_font, &ui_fonts, "Inter", false, p);
                });
                appearance_field(ui, p, "Interface base size", |ui| {
                    let percent = ui_scale * 100.0;
                    if let Some(next) = percent_stepper(
                        ui,
                        p,
                        percent,
                        UI_SCALE_MIN * 100.0,
                        UI_SCALE_MAX * 100.0,
                        5.0,
                        100.0,
                    ) {
                        ui_scale = next / 100.0;
                    }
                });
                appearance_field(ui, p, "Terminal base size", |ui| {
                    if let Some(next) = unit_stepper(
                        ui,
                        p,
                        terminal_size,
                        TERMINAL_SIZE_MIN,
                        TERMINAL_SIZE_MAX,
                        1.0,
                        "px",
                        DEFAULT_TERMINAL_SIZE,
                    ) {
                        terminal_size = next;
                    }
                });
                appearance_field(ui, p, "Spacing density", |ui| {
                    let percent = spacing_scale * 100.0;
                    if let Some(next) = percent_stepper(
                        ui,
                        p,
                        percent,
                        SPACING_SCALE_MIN * 100.0,
                        SPACING_SCALE_MAX * 100.0,
                        5.0,
                        100.0,
                    ) {
                        spacing_scale = next / 100.0;
                    }
                });
            },
            |ui| {
                appearance_field(ui, p, "Code font", |ui| {
                    appearance_select(ui, &mut code_font, &code_fonts, "JetBrains Mono", false, p);
                });
                appearance_field(ui, p, "UI font weight", |ui| {
                    let width = ui.available_width();
                    widgets::segmented_fixed(
                        ui,
                        &weight_labels,
                        &mut weight,
                        widgets::SEGMENT_HEIGHT,
                        width,
                        p,
                    );
                });
                appearance_field(ui, p, "Editor base size", |ui| {
                    if let Some(next) = unit_stepper(
                        ui,
                        p,
                        code_size,
                        CODE_SIZE_MIN,
                        CODE_SIZE_MAX,
                        1.0,
                        "px",
                        DEFAULT_CODE_SIZE,
                    ) {
                        code_size = next;
                    }
                });
            },
        );
        ui.add_space(6.0);
        muted(
            ui,
            p,
            "Interface scale and spacing apply across the app; code sizes follow code and terminals.",
        );
        ui.add_space(10.0);
        appearance_inline_row(ui, p, "Animated activity indicators", 14.0, |ui| {
            widgets::checkbox(ui, &mut animated, p);
        });
        ui.add_space(4.0);
        muted(
            ui,
            p,
            "Show moving activity markers while a response is streaming.",
        );
    });

    let mut changed = false;
    if ui_font != app.settings.appearance.ui_font {
        app.settings.appearance.ui_font = ui_font;
        changed = true;
    }
    if code_font != app.settings.appearance.code_font {
        app.settings.appearance.code_font = code_font;
        changed = true;
    }
    if (ui_scale - app.settings.appearance.ui_scale).abs() > 0.0001 {
        app.settings.appearance.ui_scale = ui_scale;
        app.settings.ui_scale = ui_scale;
        changed = true;
    }
    let weight_id = UI_WEIGHTS[weight.min(UI_WEIGHTS.len() - 1)].0;
    if weight_id != app.settings.appearance.ui_weight {
        app.settings.appearance.ui_weight = weight_id.to_string();
        changed = true;
    }
    if (code_size - app.settings.appearance.code_size).abs() > 0.0001 {
        app.settings.appearance.code_size = code_size;
        changed = true;
    }
    if (terminal_size - app.settings.appearance.terminal_size).abs() > 0.0001 {
        app.settings.appearance.terminal_size = terminal_size;
        changed = true;
    }
    if (spacing_scale - app.settings.appearance.spacing_scale).abs() > 0.0001 {
        app.settings.appearance.spacing_scale = spacing_scale;
        changed = true;
    }
    if animated != app.settings.appearance.animated {
        app.settings.appearance.animated = animated;
        app.settings.reduce_motion = !animated;
        motion::set_reduced_motion(!animated);
        changed = true;
    }
    if changed {
        commit_appearance(app, ui.ctx());
    }
}

fn provider_cards(
    ui: &mut Ui,
    app: &RustChamber,
    providers: &[&Provider],
    p: &Palette,
) -> Option<String> {
    let width = ui.available_width();
    let pane = pane_width(ui);
    let columns = if pane >= CARD_COLUMNS_WIDE {
        3
    } else if pane >= CARD_COLUMNS_MEDIUM {
        2
    } else {
        1
    };
    let card_width = ((width - CARD_GAP * (columns - 1) as f32) / columns as f32).max(0.0);
    let mut opened = None;
    for chunk in providers.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            for provider in chunk {
                let models = app
                    .model
                    .models
                    .iter()
                    .filter(|model| model.provider_id == provider.id)
                    .count();
                if provider_card(ui, provider, models, card_width, p).clicked() {
                    opened = Some(provider.id.clone());
                }
            }
        });
        ui.add_space(CARD_GAP);
    }
    opened
}

fn provider_card(
    ui: &mut Ui,
    provider: &Provider,
    models: usize,
    width: f32,
    p: &Palette,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, CARD_MIN_HEIGHT), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::radius::XL),
        if response.hovered() {
            p.tile_hover()
        } else {
            p.tile_tone()
        },
    );
    let icon_rect = Rect::from_min_size(
        Pos2::new(rect.left() + CARD_PADDING, rect.top() + CARD_PADDING),
        Vec2::splat(40.0),
    );
    ui.painter()
        .rect_filled(icon_rect, CornerRadius::same(10), p.tile_inset());
    let initial = provider_initial(provider);
    ui.painter().text(
        icon_rect.center(),
        Align2::CENTER_CENTER,
        initial,
        theme::font_semibold(16.0),
        p.fg_muted,
    );
    let title_rect = Rect::from_min_max(
        Pos2::new(rect.left() + CARD_PADDING, rect.top() + CARD_PADDING + 52.0),
        Pos2::new(
            rect.right() - CARD_PADDING,
            rect.top() + CARD_PADDING + 72.0,
        ),
    );
    let card_title = fit_text(
        ui,
        provider_name(provider),
        &theme::font_semibold(theme::TEXT_UI_HEADER),
        title_rect.width(),
    );
    ui.painter().with_clip_rect(title_rect).text(
        title_rect.left_center(),
        Align2::LEFT_CENTER,
        card_title,
        theme::font_semibold(theme::TEXT_UI_HEADER),
        p.fg,
    );
    let icon_left = rect.left() + CARD_PADDING;
    let footer_y = rect.bottom() - CARD_PADDING - 7.0;
    icons::paint(
        ui,
        Icon::Layers,
        Rect::from_center_size(Pos2::new(icon_left + 7.0, footer_y), Vec2::splat(14.0)),
        p.fg_muted,
    );
    ui.painter().text(
        Pos2::new(icon_left + 18.0, footer_y),
        Align2::LEFT_CENTER,
        models.to_string(),
        theme::font_mono(theme::TEXT_MICRO),
        p.fg_muted,
    );
    if response.hovered() {
        icons::paint(
            ui,
            Icon::ChevronRight,
            Rect::from_center_size(
                Pos2::new(rect.right() - CARD_PADDING - 8.0, footer_y),
                Vec2::splat(16.0),
            ),
            p.alpha(p.fg, 178),
        );
    }
    if models == 0 {
        let galley = ui.painter().layout_no_wrap(
            "No models".to_string(),
            theme::font_medium(10.0),
            p.warning_text,
        );
        let size = Vec2::new(galley.size().x + 12.0, 16.0);
        let pill = Rect::from_min_size(
            Pos2::new(
                rect.right() - CARD_PADDING - size.x,
                rect.top() + CARD_PADDING,
            ),
            size,
        );
        ui.painter().rect_filled(
            pill,
            CornerRadius::same(theme::radius::PILL),
            p.tint(p.warning_text, 0.15),
        );
        ui.painter().galley(
            Pos2::new(pill.left() + 6.0, pill.center().y - galley.size().y / 2.0),
            galley,
            p.warning_text,
        );
    }
    response
}

fn providers(ui: &mut Ui, app: &RustChamber, p: &Palette) {
    let mut rows = app.model.providers.clone();
    if rows.is_empty() {
        for model in &app.model.models {
            if !rows.iter().any(|provider| provider.id == model.provider_id) {
                rows.push(Provider {
                    id: model.provider_id.clone(),
                    name: model.provider_id.clone(),
                    activation: None,
                    package: None,
                    integration_id: None,
                });
            }
        }
    }
    rows.sort_by(|a, b| {
        provider_name(a)
            .to_lowercase()
            .cmp(&provider_name(b).to_lowercase())
    });
    let selection_id = Id::unique("settings.providers.selected");
    let mut selected: String = get(ui.ctx(), selection_id);
    if !selected.is_empty() && !rows.iter().any(|provider| provider.id == selected) {
        selected.clear();
        set(ui.ctx(), selection_id, selected.clone());
    }
    if !selected.is_empty() {
        if let Some(provider) = rows.iter().find(|provider| provider.id == selected) {
            if provider_detail(ui, app, provider, p) {
                selected.clear();
                set(ui.ctx(), selection_id, selected.clone());
                set(
                    ui.ctx(),
                    Id::unique(("settings.search", "provider.models")),
                    String::new(),
                );
            }
            return;
        }
        selected.clear();
        set(ui.ctx(), selection_id, selected.clone());
    }
    page_header(
        ui,
        p,
        "Providers",
        "Model providers OpenCode can use. Open one to browse its models.",
    );
    if rows.is_empty() {
        muted(ui, p, "No providers reported yet.");
        return;
    }
    let query = search_state(ui, "providers", "Search providers", p);
    let needle = query.trim().to_lowercase();
    let filtered: Vec<&Provider> = rows
        .iter()
        .filter(|provider| matches_query(&needle, &[provider_name(provider), &provider.id]))
        .collect();
    if filtered.is_empty() {
        ui.add_space(16.0);
        muted(ui, p, &format!("No providers match “{}”.", query.trim()));
        return;
    }
    ui.add_space(16.0);
    if let Some(opened) = provider_cards(ui, app, &filtered, p) {
        set(ui.ctx(), selection_id, opened);
        set(
            ui.ctx(),
            Id::unique(("settings.search", "provider.models")),
            String::new(),
        );
    }
}

fn provider_detail(ui: &mut Ui, app: &RustChamber, provider: &Provider, p: &Palette) -> bool {
    let mut back = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if widgets::icon_button_hit(ui, Icon::ChevronLeft, 14.0, 28.0, p.fg_muted)
            .on_hover_text("All providers")
            .clicked()
        {
            back = true;
        }
        let (initial_rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
        ui.painter().text(
            initial_rect.center(),
            Align2::CENTER_CENTER,
            provider_initial(provider),
            theme::font_semibold(12.0),
            p.fg_muted,
        );
        ui.label(
            RichText::new(provider_name(provider))
                .font(theme::font_semibold(theme::TEXT_PAGE_TITLE))
                .color(p.fg_muted),
        );
    });
    ui.add_space(4.0);
    description_text(ui, p, &provider.id, theme::font_mono(theme::TEXT_META));
    ui.add_space(PAGE_HEADER_GAP);
    let models: Vec<&Model> = app
        .model
        .models
        .iter()
        .filter(|model| model.provider_id == provider.id)
        .collect();
    section(ui, p, "Details", "", None, false, |ui| {
        value_row(ui, p, "Provider ID", &provider.id);
        if let Some(activation) = &provider.activation {
            value_row(ui, p, "Activation", activation);
        }
        if let Some(package) = &provider.package {
            value_row(ui, p, "Package", package);
        }
        if let Some(integration) = &provider.integration_id {
            value_row(ui, p, "Integration", integration);
        }
        value_row(ui, p, "Models", &models.len().to_string());
    });
    let suffix = format!("({})", models.len());
    section(ui, p, "Models", "", Some(&suffix), true, |ui| {
        if models.is_empty() {
            muted(ui, p, "OpenCode lists no models for this provider.");
            return;
        }
        let query = search_state(ui, "provider.models", "Filter models", p);
        ui.add_space(8.0);
        let needle = query.trim().to_lowercase();
        let filtered: Vec<&&Model> = models
            .iter()
            .filter(|model| {
                matches_query(
                    &needle,
                    &[
                        model.display_name(),
                        &model.id,
                        &model.model_id,
                        model.status.as_deref().unwrap_or(""),
                    ],
                )
            })
            .collect();
        if filtered.is_empty() {
            muted(ui, p, "No models match this filter.");
            return;
        }
        for (index, model) in filtered.iter().enumerate() {
            if index > 0 {
                let rect = ui.available_rect_before_wrap();
                ui.painter().hline(
                    rect.left()..=rect.right(),
                    rect.top(),
                    Stroke::new(theme::HAIRLINE, p.edge_muted()),
                );
            }
            model_row(ui, model, p);
        }
    });
    back
}

fn model_row(ui: &mut Ui, model: &Model, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), LIST_ROW_HEIGHT),
        Sense::hover(),
    );
    let clip = Rect::from_min_max(rect.min, Pos2::new(rect.right() - 200.0, rect.bottom()));
    let name = fit_text(
        ui,
        model.display_name(),
        &theme::font_medium(theme::TEXT_META),
        clip.width(),
    );
    ui.painter().with_clip_rect(clip).text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        name,
        theme::font_medium(theme::TEXT_META),
        p.fg,
    );
    let mut summary: Vec<String> = Vec::new();
    if let Some(limit) = &model.limit {
        if limit.context > 0 {
            summary.push(format!("{} context", compact_number(limit.context as f64)));
        }
        if limit.output > 0 {
            summary.push(format!("{} output", compact_number(limit.output as f64)));
        }
    }
    if summary.is_empty() {
        return;
    }
    let text = summary.join(" · ");
    let galley = ui
        .painter()
        .layout_no_wrap(text, theme::font(theme::TEXT_MICRO), p.fg_muted);
    let size = Vec2::new(galley.size().x + 12.0, 18.0);
    let chip = Rect::from_min_size(
        Pos2::new(rect.right() - size.x, rect.center().y - size.y / 2.0),
        size,
    );
    ui.painter().rect_filled(
        chip,
        CornerRadius::same(theme::radius::XS),
        p.alpha(p.fg, 10),
    );
    ui.painter().galley(
        Pos2::new(chip.left() + 6.0, chip.center().y - galley.size().y / 2.0),
        galley,
        p.fg_muted,
    );
}

fn agents(ui: &mut Ui, app: &RustChamber, p: &Palette) {
    page_header(ui, p, "Agents", "Agents exposed by the connected server.");
    total_row(ui, p, &format!("Total {}", app.model.agents.len()));
    ui.add_space(12.0);
    let query = search_state(ui, "agents", "Search", p);
    ui.add_space(8.0);
    if app.model.agents.is_empty() {
        muted(ui, p, "No agents configured");
        return;
    }
    let needle = query.trim().to_lowercase();
    let filtered: Vec<&Agent> = app
        .model
        .agents
        .iter()
        .filter(|agent| {
            matches_query(
                &needle,
                &[
                    &agent.id,
                    &agent.name,
                    agent.description.as_deref().unwrap_or(""),
                    &agent.mode,
                ],
            )
        })
        .collect();
    if filtered.is_empty() {
        muted(ui, p, &format!("Nothing matches “{}”.", query.trim()));
        return;
    }
    for agent in filtered {
        let mode = if agent.mode.trim().is_empty() {
            "default"
        } else {
            agent.mode.as_str()
        };
        list_row(
            ui,
            p,
            if agent.name.trim().is_empty() {
                &agent.id
            } else {
                &agent.name
            },
            Some(mode),
            agent.description.as_deref(),
            false,
        );
        ui.add_space(2.0);
    }
}

fn skill_badge(skill: &Skill) -> &'static str {
    let path = skill.path.as_deref().unwrap_or("");
    if path.contains(".claude") || path.contains("claude") {
        "claude"
    } else if path.contains(".agents") || path.contains("/agents/") {
        "agents"
    } else {
        "opencode"
    }
}

fn skills(ui: &mut Ui, app: &RustChamber, p: &Palette) {
    page_header(ui, p, "Skills", "Skills reported by the connected server.");
    total_row(ui, p, &format!("Total {}", app.model.skills.len()));
    ui.add_space(12.0);
    let query = search_state(ui, "skills", "Search", p);
    ui.add_space(8.0);
    if app.model.skills.is_empty() {
        muted(ui, p, "No skills configured");
        return;
    }
    let needle = query.trim().to_lowercase();
    let filtered: Vec<&Skill> = app
        .model
        .skills
        .iter()
        .filter(|skill| {
            matches_query(
                &needle,
                &[
                    &skill.id,
                    &skill.name,
                    skill.description.as_deref().unwrap_or(""),
                    skill.path.as_deref().unwrap_or(""),
                ],
            )
        })
        .collect();
    if filtered.is_empty() {
        muted(ui, p, &format!("Nothing matches “{}”.", query.trim()));
        return;
    }
    for skill in filtered {
        list_row(
            ui,
            p,
            if skill.name.trim().is_empty() {
                &skill.id
            } else {
                &skill.name
            },
            Some(skill_badge(skill)),
            skill.description.as_deref(),
            false,
        );
        ui.add_space(2.0);
    }
}

fn commands(ui: &mut Ui, app: &RustChamber, p: &Palette) {
    page_header(
        ui,
        p,
        "Commands",
        "Slash commands exposed by the connected server.",
    );
    total_row(ui, p, &format!("Total {}", app.model.commands.len()));
    ui.add_space(12.0);
    let query = search_state(ui, "commands", "Search", p);
    ui.add_space(8.0);
    if app.model.commands.is_empty() {
        muted(ui, p, "No commands configured");
        return;
    }
    let needle = query.trim().to_lowercase();
    let filtered: Vec<&Command> = app
        .model
        .commands
        .iter()
        .filter(|command| {
            matches_query(
                &needle,
                &[
                    &command.name,
                    command.description.as_deref().unwrap_or(""),
                    command.source.as_deref().unwrap_or(""),
                ],
            )
        })
        .collect();
    if filtered.is_empty() {
        muted(ui, p, &format!("Nothing matches “{}”.", query.trim()));
        return;
    }
    for command in filtered {
        list_row(
            ui,
            p,
            &format!("/{}", command.name),
            command.source.as_deref(),
            command.description.as_deref(),
            false,
        );
        ui.add_space(2.0);
    }
}

fn same_path(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

fn home_relative(path: &std::path::Path) -> String {
    let Some(home) = directories::BaseDirs::new().map(|base| base.home_dir().to_path_buf()) else {
        return path.display().to_string();
    };
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn projects(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    page_header(ui, p, "Projects", "Folders open in this window.");
    let path = app.workspace_directory();
    let mut change = false;
    section(ui, p, "Folder", "", None, false, |ui| {
        field_row(ui, p, "Path", Some(&path), |ui| {
            if widgets::outline_button(ui, "Change folder…", Some(Icon::Folder), p).clicked() {
                change = true;
            }
        });
    });
    if change && let Some(folder) = rfd::FileDialog::new().pick_folder() {
        app.set_workspace(folder.to_string_lossy().to_string());
    }
    let projects = app.model.projects.clone();
    let mut switch: Option<String> = None;
    section(
        ui,
        p,
        "Projects",
        &format!("Total {}", projects.len()),
        None,
        true,
        |ui| {
            if projects.is_empty() {
                muted(ui, p, "No projects yet.");
                return;
            }
            for project in &projects {
                let active = same_path(&project.canonical, &path);
                let badge = if active { Some("active") } else { None };
                let response = list_row(
                    ui,
                    p,
                    &project.display_name(),
                    badge,
                    Some(&project.canonical),
                    active,
                );
                if response.hovered() && !active {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if response.clicked() && !active {
                    switch = Some(project.canonical.clone());
                }
                ui.add_space(2.0);
            }
        },
    );
    if let Some(directory) = switch {
        app.set_workspace(directory);
    }
}

fn value_row(ui: &mut Ui, p: &Palette, label: &str, value: &str) {
    field_row(ui, p, label, None, |ui| {
        ui.add(
            egui::Label::new(
                RichText::new(value)
                    .font(theme::font_mono(theme::TEXT_META))
                    .color(p.fg_muted),
            )
            .truncate(),
        );
    });
}

fn general(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    page_header(
        ui,
        p,
        "General",
        "Connection, server process, and the settings file.",
    );
    let (label, color) = match &app.model.connection {
        Connection::Ready => ("Connected", p.green),
        Connection::Connecting => ("Starting", p.primary),
        Connection::Reconnecting(_) => ("Reconnecting", p.coral),
        Connection::Failed(_) => ("Offline", p.red),
        Connection::Disconnected => ("Disconnected", p.fg_muted),
    };
    let version = app
        .model
        .server
        .as_ref()
        .map(|server| server.version.clone());
    let detail = app.model.connection_detail.clone();
    section(ui, p, "Connection", "", None, false, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            widgets::status_dot(ui, color);
            ui.label(
                RichText::new(label)
                    .font(theme::font_medium(theme::TEXT_META))
                    .color(color),
            );
            if let Some(version) = &version {
                ui.label(
                    RichText::new(format!("OpenCode {version}"))
                        .font(theme::font_mono(11.5))
                        .color(p.fg_muted),
                );
            }
        });
        if let Some(detail) = &detail {
            ui.add_space(4.0);
            muted(ui, p, detail);
        }
    });
    section(ui, p, "Server connection", "", None, true, |ui| {
        stacked_field(ui, p, "Endpoint", |ui| {
            text_field(ui, &mut app.ui.settings_endpoint, "auto", false, p);
        });
        ui.add_space(16.0);
        stacked_field(ui, p, "Password", |ui| {
            text_field(
                ui,
                &mut app.ui.settings_password,
                "OPENCODE_PASSWORD",
                true,
                p,
            );
        });
        ui.add_space(8.0);
        let managed = app.ui.settings_endpoint.trim().is_empty();
        muted(
            ui,
            p,
            if managed {
                "Managed server — RustChamber starts and supervises a local OpenCode process."
            } else {
                "External server — RustChamber only connects and never spawns a process."
            },
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if widgets::primary_button(ui, "Apply & reconnect", Some(Icon::Refresh), p).clicked() {
                app.apply_connection_settings();
            }
            if widgets::ghost_button(ui, "Restart", Some(Icon::Refresh), p).clicked() {
                app.backend.send(Action::Reconnect);
            }
        });
    });
}

fn shortcut(parts: &[&str], key: &str) -> Vec<String> {
    let mut keys = Vec::with_capacity(parts.len() + 1);
    if cfg!(target_os = "macos") {
        for part in parts {
            let glyph = match *part {
                "mod" => "⌘",
                "alt" => "⌥",
                "shift" => "⇧",
                "ctrl" => "⌃",
                _ => "",
            };
            if !glyph.is_empty() {
                keys.push(glyph.to_string());
            }
        }
    } else {
        for part in parts {
            let name = match *part {
                "mod" | "ctrl" => "Ctrl",
                "alt" => "Alt",
                "shift" => "Shift",
                _ => "",
            };
            if !name.is_empty() {
                keys.push(name.to_string());
            }
        }
    }
    keys.push(key.to_string());
    keys
}

fn keycap_width(ui: &Ui, p: &Palette, key: &str) -> f32 {
    let galley = ui
        .painter()
        .layout_no_wrap(key.to_string(), theme::font_mono(KEYCAP_FONT), p.fg);
    (galley.size().x + KEYCAP_PAD_X * 2.0).max(KEYCAP_MIN_WIDTH)
}

fn shortcut_row(ui: &mut Ui, p: &Palette, label: &str, keys: &[String]) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), SHORTCUT_ROW_HEIGHT),
        Sense::hover(),
    );
    ui.painter().text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::font(theme::TEXT_UI_LABEL),
        p.fg,
    );
    let font = theme::font_mono(KEYCAP_FONT);
    let widths: Vec<f32> = keys.iter().map(|key| keycap_width(ui, p, key)).collect();
    let total = widths.iter().sum::<f32>() + KEYCAP_GAP * keys.len().saturating_sub(1) as f32;
    let mut left = rect.right() - total;
    let corner = CornerRadius::same(theme::radius::SM);
    let fill = p.alpha(p.fg, if p.dark { 10 } else { 6 });
    let border = Stroke::new(theme::HAIRLINE, p.edge_muted());
    for (key, width) in keys.iter().zip(widths) {
        let cap = Rect::from_min_size(
            Pos2::new(left, rect.center().y - KEYCAP_HEIGHT / 2.0),
            Vec2::new(width, KEYCAP_HEIGHT),
        );
        ui.painter().rect_filled(cap, corner, fill);
        ui.painter()
            .rect_stroke(cap, corner, border, egui::StrokeKind::Inside);
        ui.painter()
            .text(cap.center(), Align2::CENTER_CENTER, key, font.clone(), p.fg);
        left += width + KEYCAP_GAP;
    }
}

type ShortcutGroup = (&'static str, Vec<(&'static str, Vec<String>)>);

fn shortcuts(ui: &mut Ui, p: &Palette) {
    page_header(ui, p, "Shortcuts", "Keys used across the app.");
    let groups: [ShortcutGroup; 4] = [
        (
            "Session controls",
            vec![
                ("New session", shortcut(&["mod"], "N")),
                ("Previous session", shortcut(&["mod", "alt"], "←")),
                ("Next session", shortcut(&["mod", "alt"], "→")),
            ],
        ),
        (
            "Models & agents",
            vec![("Open model selector", shortcut(&["mod", "shift"], "M"))],
        ),
        (
            "Panels & tools",
            vec![
                ("Toggle terminal dock", shortcut(&["mod"], "J")),
                ("Toggle sidebar", shortcut(&["mod"], "B")),
                ("Close tab in Files and Terminal", shortcut(&["mod"], "W")),
            ],
        ),
        (
            "Navigation",
            vec![
                ("Open command palette", shortcut(&["mod"], "K")),
                ("Open settings", shortcut(&["mod"], ",")),
                ("Close overlay", vec!["Esc".to_string()]),
            ],
        ),
    ];
    for (index, (title, rows)) in groups.iter().enumerate() {
        if index > 0 {
            ui.add_space(SHORTCUT_GROUP_GAP);
            let rect = ui.available_rect_before_wrap();
            ui.painter().hline(
                rect.left()..=rect.right(),
                rect.top(),
                Stroke::new(theme::BORDER, p.edge_color()),
            );
            ui.add_space(SHORTCUT_GROUP_GAP);
        }
        ui.label(
            RichText::new(*title)
                .font(theme::font_semibold(theme::TEXT_UI_LABEL))
                .color(p.fg),
        );
        ui.add_space(10.0);
        for (label, keys) in rows {
            shortcut_row(ui, p, label, keys);
        }
    }
}

fn text_link(ui: &mut Ui, p: &Palette, label: &str, url: &str, icon: Icon) {
    let font = theme::font(theme::TEXT_META);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, p.fg_muted);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 22.0, 20.0), Sense::click());
    crate::widgets::pointer(ui, &response);
    let color = if response.hovered() { p.fg } else { p.fg_muted };
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 7.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        color,
    );
    ui.painter().galley(
        Pos2::new(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        let _ = open::that(url);
    }
}

fn about(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    page_header(ui, p, "About", "Version, links, and build details.");
    section(ui, p, "", "", None, false, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
            let corner = CornerRadius::same(theme::radius::XL);
            ui.painter().rect_filled(rect, corner, p.bg_soft);
            ui.painter().rect_stroke(
                rect,
                corner,
                Stroke::new(theme::BORDER, p.edge_color()),
                egui::StrokeKind::Inside,
            );
            logo::paint(ui.painter(), rect.shrink(10.0), p.primary);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(16.0, 4.0);
                ui.label(
                    RichText::new("RustChamber")
                        .font(theme::font_semibold(theme::TEXT_UI_HEADER))
                        .color(p.fg),
                );
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    text_link(
                        ui,
                        p,
                        "Source",
                        env!("CARGO_PKG_REPOSITORY"),
                        Icon::ArrowUpRight,
                    );
                    text_link(
                        ui,
                        p,
                        "OpenCode docs",
                        "https://opencode.ai/docs",
                        Icon::ArrowUpRight,
                    );
                });
            });
        });
    });
    let opencode = app
        .model
        .server
        .as_ref()
        .map(|server| server.version.clone())
        .unwrap_or_else(|| "Unknown".into());
    section(ui, p, "", "", None, true, |ui| {
        value_row(ui, p, "Version", crate::update::app_version());
        value_row(ui, p, "OpenCode version", &opencode);
        value_row(ui, p, "UI toolkit", "egui 0.37");
        value_row(
            ui,
            p,
            "Platform",
            &format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        );
        let settings_path = crate::persist::path()
            .map(|path| home_relative(&path))
            .unwrap_or_else(|| "unavailable".into());
        value_row(ui, p, "Settings file", &settings_path);
        ui.add_space(12.0);
        if app.updates.is_flatpak() {
            let id = app
                .updates
                .flatpak_id()
                .unwrap_or_else(|| "dev.rustchamber.RustChamber".into());
            ui.label(
                RichText::new(format!(
                    "Updates are managed by Flathub. Run `flatpak update {id}` or use your software center."
                ))
                .font(theme::font(theme::TEXT_META))
                .color(p.fg_muted),
            );
        } else {
            ui.horizontal(|ui| {
                if widgets::button(
                    ui,
                    "Check for updates",
                    Some(Icon::Refresh),
                    widgets::ButtonVariant::Outline,
                    widgets::ButtonSize::Sm,
                    p,
                )
                .clicked()
                {
                    app.check_for_updates();
                }
                if let Some(info) = app.updates.available() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Version {} is available", info.version))
                            .font(theme::font(theme::TEXT_META))
                            .color(p.blue),
                    );
                } else if app.updates.snapshot().phase == rchamber_update::Phase::Checking {
                    ui.add_space(8.0);
                    widgets::spinner(ui, 13.0, p.fg_muted);
                    ui.label(
                        RichText::new("Checking…")
                            .font(theme::font(theme::TEXT_META))
                            .color(p.fg_muted),
                    );
                } else if app.updates.snapshot().phase == rchamber_update::Phase::Idle
                    && app.updates.snapshot().error.is_none()
                {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Up to date")
                            .font(theme::font(theme::TEXT_META))
                            .color(p.fg_muted),
                    );
                }
            });
        }
        if let Some(error) = app.updates.snapshot().error.clone() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(error.lines().next().unwrap_or(&error))
                    .font(theme::font(theme::TEXT_META))
                    .color(p.error_text),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Color32, ColorImage};

    use super::{blur_backdrop, shortcut};

    #[test]
    fn shortcut_puts_each_modifier_in_its_own_cap() {
        let keys = shortcut(&["mod", "shift"], "M");
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[2], "M");
        assert_ne!(keys[0], keys[1]);
    }

    #[test]
    fn backdrop_blur_downsamples_and_keeps_a_flat_color() {
        let source = ColorImage::filled([60, 36], Color32::from_rgb(120, 60, 30));
        let blurred = blur_backdrop(&source);
        assert_eq!(blurred.size, [10, 6]);
        assert!(blurred.pixels.iter().all(|pixel| {
            pixel.r().abs_diff(120) <= 1
                && pixel.g().abs_diff(60) <= 1
                && pixel.b().abs_diff(30) <= 1
                && pixel.a() == 255
        }));
    }
}
