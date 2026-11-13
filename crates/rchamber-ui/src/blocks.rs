use std::path::Path;
use std::sync::Arc;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Margin, Pos2, Rect, RichText,
    Sense, Stroke, StrokeKind, Ui, Vec2,
};
use rchamber_ext::{
    BlockResponse, ButtonVariant as ExtButtonVariant, Cell, ChoiceOption, Direction, ListItem,
    Series, StepItem, StepStatus, TabItem, Tone, Widget,
};
use serde_json::{Map, Value};

use crate::markdown::Markdown;
use crate::theme::{self, Palette};
use crate::widgets::{self, ButtonSize, TabVariant};

const TABLE_PAD_X: f32 = 10.0;
const TABLE_PAD_Y: f32 = 7.0;
const CHART_TICKS: usize = 4;

#[derive(Clone, Default)]
#[allow(clippy::type_complexity)]
pub struct BlockHost {
    pub submit: Option<Arc<dyn Fn(BlockResponse) + Send + Sync>>,
    pub request: Option<Arc<dyn Fn(&str, &[String]) + Send + Sync>>,
    pub tree: Option<Arc<dyn Fn(&str, &[String]) -> Option<Vec<Widget>> + Send + Sync>>,
}

impl BlockHost {
    pub fn none() -> Self {
        Self::default()
    }
}

#[derive(Clone)]
struct HostSlot(BlockHost);

pub fn set_host(ctx: &egui::Context, host: BlockHost) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique("block-host"), HostSlot(host)));
}

pub fn host(ctx: &egui::Context) -> BlockHost {
    ctx.data(|data| {
        data.get_temp::<HostSlot>(egui::Id::unique("block-host"))
            .map(|slot| slot.0)
            .unwrap_or_default()
    })
}

pub fn show(ui: &mut Ui, source: &str, p: &Palette, base_size: f32) {
    if source.len() > 128 * 1024 {
        ui.label(
            RichText::new("This block is too large to draw.")
                .font(theme::font(base_size))
                .color(p.error_text),
        );
        return;
    }
    let cache_id = egui::Id::unique(("openui-block", source));
    if let Some(widgets) = ui.ctx().data(|data| data.get_temp::<Vec<Widget>>(cache_id)) {
        show_widgets(ui, &widgets, p, base_size);
        return;
    }
    match rchamber_ext::parse_openui(source) {
        Ok(widgets) => {
            ui.ctx()
                .data_mut(|data| data.insert_temp(cache_id, widgets.clone()));
            show_widgets(ui, &widgets, p, base_size)
        }
        Err(error) => {
            egui::Frame::new()
                .fill(p.wash(p.red, 18))
                .stroke(Stroke::new(theme::HAIRLINE, p.wash(p.red, 70)))
                .corner_radius(CornerRadius::same(theme::radius::MD))
                .inner_margin(Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("Could not draw this block: {error}"))
                            .font(theme::font(base_size.max(theme::TEXT_MICRO) * 0.92))
                            .color(p.error_text),
                    );
                });
        }
    }
}

pub fn show_widgets(ui: &mut Ui, widgets: &[Widget], p: &Palette, base_size: f32) {
    if widgets.is_empty() {
        return;
    }
    let auto_id = ui.next_auto_id();
    let mut renderer = BlockRenderer::new(auto_id, p, base_size);
    renderer.widgets(ui, widgets, theme::PARAGRAPH_GAP);
    renderer.send_button(ui);
}

pub fn min_width(ui: &Ui, widgets: &[Widget], base_size: f32) -> f32 {
    WidthMetrics::new(ui, base_size).widgets(widgets)
}

pub fn row_width(ui: &Ui, children: &[Widget], base_size: f32) -> f32 {
    let metrics = WidthMetrics::new(ui, base_size);
    metrics.row(children)
}

struct WidthMetrics<'a> {
    ui: &'a Ui,
    base: f32,
}

impl<'a> WidthMetrics<'a> {
    fn new(ui: &'a Ui, base_size: f32) -> Self {
        let base = if base_size.is_finite() && base_size > 0.0 {
            base_size
        } else {
            theme::TEXT_MARKDOWN
        };
        Self { ui, base }
    }

    fn text(&self, text: &str, font: egui::FontId) -> f32 {
        self.ui
            .painter()
            .layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER)
            .size()
            .x
    }

    fn word(&self, text: &str, font: egui::FontId, cap: f32) -> f32 {
        text.split_whitespace()
            .map(|word| self.text(word, font.clone()))
            .fold(0.0_f32, f32::max)
            .min(cap)
    }

    fn widgets(&self, widgets: &[Widget]) -> f32 {
        widgets
            .iter()
            .map(|widget| self.widget(widget))
            .fold(0.0_f32, f32::max)
    }

    fn row(&self, children: &[Widget]) -> f32 {
        let gaps = theme::SPACE_2 * (children.len().saturating_sub(1)) as f32;
        children.iter().map(|child| self.widget(child)).sum::<f32>() + gaps
    }

    fn widget(&self, widget: &Widget) -> f32 {
        match widget {
            Widget::Stack { children, .. } => self.widgets(children),
            Widget::Row { children, .. } => {
                if children.is_empty() {
                    0.0
                } else {
                    self.row(children)
                }
            }
            Widget::Card { children, .. } => self.widgets(children) + 24.0,
            Widget::Tabs { tabs } => {
                let header = tabs
                    .iter()
                    .map(|tab| self.text(&tab.label, theme::font(theme::TEXT_MICRO)) + 24.0)
                    .sum::<f32>();
                let body = tabs
                    .iter()
                    .map(|tab| self.widgets(&tab.children))
                    .fold(0.0_f32, f32::max);
                header.max(body)
            }
            Widget::Text { markdown } => self.word(markdown, theme::font(self.base), 360.0),
            Widget::Heading { text, level } => {
                let scale = match level.unwrap_or(3).clamp(1, 6) {
                    1 => 1.46,
                    2 => 1.28,
                    3 => 1.13,
                    4 => 1.02,
                    5 => 0.95,
                    _ => 0.88,
                };
                self.word(text, theme::font_semibold(self.base * scale), 360.0)
            }
            Widget::Code { .. } => 0.0,
            Widget::Badge { label, .. } => {
                self.text(label, theme::font_medium(self.base * 0.78)) + 16.0
            }
            Widget::Callout {
                markdown, title, ..
            } => {
                let title = title
                    .as_deref()
                    .map(|title| self.text(title, theme::font_semibold(self.base * 0.94)))
                    .unwrap_or(0.0);
                title
                    .max(self.word(markdown, theme::font(self.base * 0.95), 320.0))
                    .max(120.0)
                    + 24.0
            }
            Widget::KeyValue { items } => items
                .iter()
                .map(|item| {
                    let label = self
                        .text(&item.label, theme::font(self.base * 0.88))
                        .clamp(80.0, 200.0);
                    let value = self.word(&item.value.text(), theme::font(self.base * 0.94), 240.0);
                    label + 8.0 + value.max(40.0)
                })
                .fold(0.0_f32, f32::max),
            Widget::Table { .. } => 0.0,
            Widget::List { items, .. } => items
                .iter()
                .map(|item| {
                    let title = self.word(&item.title, theme::font_medium(self.base * 0.96), 320.0);
                    let body = item
                        .markdown
                        .as_deref()
                        .map(|markdown| self.word(markdown, theme::font(self.base * 0.9), 320.0))
                        .unwrap_or(0.0);
                    theme::LIST_INDENT + title.max(body).max(80.0)
                })
                .fold(0.0_f32, f32::max),
            Widget::Steps { items, direction } => match direction {
                Direction::Row => items
                    .iter()
                    .map(|item| {
                        self.word(&item.title, theme::font_medium(self.base * 0.94), 240.0)
                            .max(110.0)
                    })
                    .fold(0.0_f32, f32::max),
                Direction::Column => {
                    24.0 + items
                        .iter()
                        .map(|item| {
                            self.word(&item.title, theme::font_medium(self.base * 0.96), 320.0)
                        })
                        .fold(80.0_f32, f32::max)
                }
            },
            Widget::Stat { label, value, .. } => {
                let value = self.text(&value.text(), theme::font_semibold(self.base * 1.65));
                let label = self.word(label, theme::font(self.base * 0.82), 200.0);
                value.max(label) + 24.0
            }
            Widget::Progress { label, .. } => {
                self.text(label, theme::font_medium(self.base * 0.9)) + 64.0
            }
            Widget::BarChart { series, labels, .. } | Widget::LineChart { series, labels, .. } => {
                let legend = series
                    .iter()
                    .map(|series| self.text(&series.name, theme::font(self.base * 0.8)) + 18.0)
                    .fold(0.0_f32, f32::max);
                let ticks = labels
                    .iter()
                    .map(|label| self.text(label, theme::font(self.base * 0.75)))
                    .fold(0.0_f32, f32::max);
                legend.max(ticks * 2.0).max(200.0)
            }
            Widget::Choice { options, .. } => {
                let described = options.iter().any(|option| {
                    option
                        .description()
                        .is_some_and(|value| !value.trim().is_empty())
                });
                if described {
                    180.0
                } else {
                    options
                        .iter()
                        .map(|option| {
                            self.text(option.label(), theme::font_medium(theme::TEXT_UI_LABEL))
                                + 20.0
                        })
                        .fold(80.0_f32, f32::max)
                }
            }
            Widget::Input { .. } => 140.0,
            Widget::DateInput { .. } => 170.0,
            Widget::Slider { .. } => 180.0,
            Widget::Details { children, .. } => self.widgets(children),
            Widget::Divider => 0.0,
            Widget::Button { label, .. } => {
                self.text(label, theme::font_medium(theme::TEXT_UI_LABEL)) + 24.0
            }
            Widget::Image { .. } => 120.0,
            Widget::ExtensionBlock { name, args } => host(self.ui.ctx())
                .tree
                .as_ref()
                .and_then(|tree| tree(name, args))
                .map(|widgets| self.widgets(&widgets))
                .unwrap_or(0.0),
        }
    }
}

struct BlockRenderer<'a> {
    p: &'a Palette,
    base: f32,
    auto_id: Id,
    markdown: Markdown,
    fields: Vec<FieldEntry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Text,
    Number,
    Single,
    Multiple,
}

struct FieldEntry {
    name: String,
    id: Id,
    kind: FieldKind,
}

#[derive(Clone, Debug, PartialEq)]
enum FieldValue {
    Text(String),
    Number(f64),
    Single(Option<String>),
    Multiple(Vec<String>),
}

#[derive(Clone, Copy)]
struct ToneColors {
    accent: Color32,
    text: Color32,
    fill: Color32,
    border: Color32,
}

impl<'a> BlockRenderer<'a> {
    fn new(auto_id: Id, p: &'a Palette, base_size: f32) -> Self {
        let base = if base_size.is_finite() && base_size > 0.0 {
            base_size
        } else {
            theme::TEXT_MARKDOWN
        };
        Self {
            p,
            base,
            auto_id,
            markdown: Markdown::default(),
            fields: Vec::new(),
        }
    }

    fn widgets(&mut self, ui: &mut Ui, widgets: &[Widget], gap: f32) {
        for (index, widget) in widgets.iter().enumerate() {
            if index > 0 {
                ui.add_space(gap);
            }
            self.widget(ui, widget);
        }
    }

    fn widget(&mut self, ui: &mut Ui, widget: &Widget) {
        match widget {
            Widget::Stack { gap, children } => {
                let gap = gap.unwrap_or(theme::PARAGRAPH_GAP);
                ui.vertical(|ui| {
                    self.widgets(ui, children, gap);
                });
            }
            Widget::Row { wrap, children } => {
                let wrap = *wrap || row_width(ui, children, self.base) > ui.available_width();
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme::SPACE_2, theme::SPACE_2);
                    if wrap {
                        ui.horizontal_wrapped(|ui| {
                            for child in children {
                                self.widget(ui, child);
                            }
                        });
                    } else {
                        ui.horizontal_top(|ui| {
                            for child in children {
                                self.widget(ui, child);
                            }
                        });
                    }
                });
            }
            Widget::Card { title, children } => {
                ui.vertical(|ui| self.card(ui, title.as_deref(), children));
            }
            Widget::Tabs { tabs } => {
                ui.vertical(|ui| self.tabs(ui, tabs));
            }
            Widget::Text { markdown } => {
                ui.vertical(|ui| {
                    let id = ui.next_auto_id();
                    let line = (self.base * 1.62).max(self.base + 4.0);
                    self.markdown.show_at_sized(
                        ui,
                        id,
                        markdown,
                        self.base,
                        line,
                        self.p.body,
                        self.p,
                    );
                });
            }
            Widget::Heading { text, level } => {
                self.heading(ui, text, *level);
            }
            Widget::Code { code, language } => {
                ui.vertical(|ui| self.code(ui, code, language.as_deref()));
            }
            Widget::Badge { label, tone } => {
                self.badge(ui, label, *tone);
            }
            Widget::Callout {
                markdown,
                tone,
                title,
            } => {
                ui.vertical(|ui| self.callout(ui, markdown, *tone, title.as_deref()));
            }
            Widget::KeyValue { items } => {
                ui.vertical(|ui| self.key_value(ui, items));
            }
            Widget::Table { columns, rows } => {
                ui.vertical(|ui| self.table(ui, columns, rows));
            }
            Widget::List { items, ordered } => {
                ui.vertical(|ui| self.list(ui, items, *ordered));
            }
            Widget::Steps { items, direction } => {
                ui.vertical(|ui| self.steps(ui, items, *direction));
            }
            Widget::Stat {
                label,
                value,
                delta,
                tone,
            } => {
                self.stat(ui, label, value, delta.as_deref(), *tone);
            }
            Widget::Progress { label, value, max } => {
                ui.vertical(|ui| self.progress(ui, label, *value, *max));
            }
            Widget::BarChart {
                labels,
                series,
                unit,
                whole,
            } => {
                ui.vertical(|ui| self.bar_chart(ui, labels, series, unit.as_deref(), *whole));
            }
            Widget::LineChart {
                labels,
                series,
                unit,
            } => {
                ui.vertical(|ui| self.line_chart(ui, labels, series, unit.as_deref()));
            }
            Widget::Choice {
                name,
                options,
                label,
                multiple,
            } => {
                ui.vertical(|ui| self.choice(ui, name, options, label.as_deref(), *multiple));
            }
            Widget::Input {
                name,
                label,
                placeholder,
                multiline,
            } => {
                ui.vertical(|ui| {
                    self.input(
                        ui,
                        name,
                        label.as_deref(),
                        placeholder.as_deref(),
                        *multiline,
                    );
                });
            }
            Widget::DateInput {
                name,
                label,
                time,
                value,
            } => {
                ui.vertical(|ui| {
                    self.date_input(ui, name, label.as_deref(), *time, value.as_deref());
                });
            }
            Widget::Slider {
                name,
                min,
                max,
                label,
                step,
                value,
                unit,
            } => {
                ui.vertical(|ui| {
                    self.slider(
                        ui,
                        name,
                        *min,
                        *max,
                        label.as_deref(),
                        *step,
                        *value,
                        unit.as_deref(),
                    );
                });
            }
            Widget::Details { title, children } => {
                ui.vertical(|ui| self.details(ui, title, children));
            }
            Widget::Divider => {
                theme::divider(ui, self.p);
            }
            Widget::Button {
                label,
                variant,
                value,
            } => {
                self.button(ui, label, *variant, value.as_deref());
            }
            Widget::Image { path, alt, height } => {
                self.image(ui, path, alt.as_deref(), *height);
            }
            Widget::ExtensionBlock { name, args } => {
                ui.vertical(|ui| self.extension(ui, name, args));
            }
        }
    }

    fn heading(&mut self, ui: &mut Ui, text: &str, level: Option<u8>) {
        let level = level.unwrap_or(3).clamp(1, 6);
        let scale = match level {
            1 => 1.46,
            2 => 1.28,
            3 => 1.13,
            4 => 1.02,
            5 => 0.95,
            _ => 0.88,
        };
        if level <= 2 {
            ui.add_space(self.base * 0.18);
        }
        ui.label(
            RichText::new(text)
                .font(theme::font_semibold(self.base * scale))
                .color(self.p.fg),
        );
    }

    fn code(&mut self, ui: &mut Ui, code: &str, language: Option<&str>) {
        let p = self.p;
        let language = language.unwrap_or("").trim();
        let runnable = matches!(
            language,
            "bash" | "sh" | "zsh" | "shell" | "console" | "cmd" | "fish" | "powershell"
        );
        let diffable = language == "diff" || looks_like_diff(code);
        let id = ui.next_auto_id();
        let run_id = egui::Id::unique(("block-run", code));
        let default_diff = language == "diff";
        let mut show_diff = ui
            .memory(|memory| memory.data.get_temp::<bool>(id.with("diff")))
            .unwrap_or(default_diff);
        let width = ui.available_width();
        egui::Frame::new()
            .fill(p.code_bg)
            .stroke(Stroke::new(theme::BORDER, p.border))
            .corner_radius(CornerRadius::same(theme::radius::TABLE))
            .show(ui, |ui| {
                ui.set_width(width.max(40.0));
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let inner = ui.available_width();
                let (header, _) = ui.allocate_exact_size(Vec2::new(inner, 30.0), Sense::hover());
                ui.painter().rect_filled(
                    header,
                    CornerRadius {
                        nw: theme::radius::TABLE,
                        ne: theme::radius::TABLE,
                        sw: 0,
                        se: 0,
                    },
                    p.bg_soft,
                );
                ui.painter().hline(
                    header.x_range(),
                    header.bottom(),
                    Stroke::new(theme::HAIRLINE, p.border),
                );
                let mut header_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(header.shrink2(Vec2::new(10.0, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                header_ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                let title = if language.is_empty() {
                    "code"
                } else {
                    language
                };
                header_ui.label(
                    RichText::new(title)
                        .font(theme::font_mono(theme::TEXT_CODE_HEADER))
                        .color(p.fg_muted),
                );
                header_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                    if runnable && code_action_button(ui, "Run", true, p).clicked() {
                        crate::shell_run::run(ui.ctx(), run_id, code);
                    }
                    if diffable {
                        let label = if show_diff { "Code" } else { "View diff" };
                        if code_action_button(ui, label, false, p).clicked() {
                            show_diff = !show_diff;
                        }
                    }
                    if code_action_button(ui, "Copy", false, p).clicked() {
                        ui.ctx().copy_text(code.to_string());
                    }
                });
                ui.memory_mut(|memory| {
                    memory.data.insert_temp(id.with("diff"), show_diff);
                });
                egui::Frame::new()
                    .inner_margin(Margin {
                        left: 12,
                        right: 12,
                        top: 10,
                        bottom: 10,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::horizontal()
                            .id_salt(id.with("body"))
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                if show_diff {
                                    code_diff(ui, p, code);
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(code)
                                                .font(theme::font_mono(theme::TEXT_CODE))
                                                .color(p.fg)
                                                .line_height(Some(theme::LINE_CODE_ROW)),
                                        )
                                        .selectable(true)
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                    );
                                }
                            });
                    });
            });
        if runnable {
            crate::shell_run::show(ui, run_id, p);
        }
    }

    fn badge(&mut self, ui: &mut Ui, label: &str, tone: Tone) {
        let colors = tone_colors(self.p, tone);
        let font = theme::font_medium(self.base * 0.78);
        let text = fit_label(ui, label, &font, (ui.available_width() - 16.0).max(20.0));
        let galley = ui
            .painter()
            .layout_no_wrap(text, font, Color32::PLACEHOLDER);
        let height = (self.base * 1.5).max(20.0);
        let width = galley.size().x + 16.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::radius::PILL), colors.fill);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::radius::PILL),
            Stroke::new(theme::HAIRLINE, colors.border),
            StrokeKind::Inside,
        );
        ui.painter().galley(
            Pos2::new(
                rect.center().x - galley.size().x / 2.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            colors.text,
        );
    }

    fn callout(&mut self, ui: &mut Ui, markdown: &str, tone: Tone, title: Option<&str>) {
        let p = self.p;
        let colors = tone_colors(p, tone);
        let inner = egui::Frame::new()
            .fill(colors.fill)
            .stroke(Stroke::new(theme::BORDER, colors.border))
            .corner_radius(CornerRadius::same(theme::radius::PANEL))
            .inner_margin(Margin {
                left: 14,
                right: 10,
                top: 9,
                bottom: 9,
            })
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
                    ui.label(
                        RichText::new(title)
                            .font(theme::font_semibold(self.base * 0.94))
                            .color(colors.text),
                    );
                    ui.add_space(3.0);
                }
                let id = ui.next_auto_id();
                let size = self.base * 0.95;
                self.markdown.show_at_sized(
                    ui,
                    id,
                    markdown,
                    size,
                    (size * 1.58).max(size + 4.0),
                    p.body,
                    p,
                );
            });
        let bar = Rect::from_min_size(
            inner.response.rect.min,
            Vec2::new(3.0, inner.response.rect.height()),
        );
        ui.painter()
            .rect_filled(bar, CornerRadius::same(1), colors.accent);
    }

    fn card(&mut self, ui: &mut Ui, title: Option<&str>, children: &[Widget]) {
        let p = self.p;
        let width = ui.available_width();
        theme::card_frame(p).show(ui, |ui| {
            ui.set_min_width((width - 24.0).max(0.0));
            if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
                ui.label(
                    RichText::new(title)
                        .font(theme::font_semibold(self.base))
                        .color(p.fg),
                );
                ui.add_space(6.0);
            }
            self.widgets(ui, children, theme::PARAGRAPH_GAP);
        });
    }

    fn tabs(&mut self, ui: &mut Ui, tabs: &[TabItem]) {
        if tabs.is_empty() {
            return;
        }
        let state_id = ui.next_auto_id();
        let labels: Vec<&str> = tabs.iter().map(|tab| tab.label.as_str()).collect();
        let mut selected = ui
            .memory(|memory| memory.data.get_temp::<usize>(state_id))
            .unwrap_or(0)
            .min(tabs.len() - 1);
        if widgets::tabs_ex(ui, &labels, &mut selected, TabVariant::Underline, self.p) {
            ui.memory_mut(|memory| memory.data.insert_temp(state_id, selected));
        }
        if let Some(tab) = tabs.get(selected)
            && !tab.children.is_empty()
        {
            ui.add_space(theme::SPACE_2);
            self.widgets(ui, &tab.children, theme::PARAGRAPH_GAP);
        }
    }

    fn key_value(&mut self, ui: &mut Ui, items: &[rchamber_ext::KeyValueItem]) {
        if items.is_empty() {
            return;
        }
        let p = self.p;
        let label_width = (ui.available_width() * 0.32).clamp(80.0, 200.0);
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                ui.add_space(5.0);
            }
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(label_width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&item.label)
                                    .font(theme::font(self.base * 0.88))
                                    .color(p.fg_muted),
                            )
                            .wrap(),
                        );
                    },
                );
                let value_width = ui.available_width().max(40.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(value_width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(item.value.text())
                                    .font(theme::font(self.base * 0.94))
                                    .color(p.fg),
                            )
                            .wrap(),
                        );
                    },
                );
            });
        }
    }

    fn table(&mut self, ui: &mut Ui, columns: &[String], rows: &[Vec<Cell>]) {
        let col_count = columns
            .len()
            .max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if col_count == 0 {
            return;
        }
        let p = self.p;
        let body_font = theme::font(self.base * 0.9);
        let head_font = theme::font_medium(self.base * 0.9);
        let available = ui.available_width().max(80.0);
        let min_col = (self.base * 4.6).max(62.0);
        let max_col = (available * 0.7).max(min_col);
        let mut widths = vec![min_col; col_count];
        for (index, width) in widths.iter_mut().enumerate() {
            let mut natural: f32 = 0.0;
            if let Some(column) = columns.get(index) {
                natural = natural.max(
                    ui.painter()
                        .layout_no_wrap(column.clone(), head_font.clone(), Color32::PLACEHOLDER)
                        .size()
                        .x,
                );
            }
            for row in rows.iter().take(200) {
                if let Some(cell) = row.get(index) {
                    natural = natural.max(
                        ui.painter()
                            .layout_no_wrap(cell.text(), body_font.clone(), Color32::PLACEHOLDER)
                            .size()
                            .x,
                    );
                }
            }
            *width = (natural + TABLE_PAD_X * 2.0).clamp(min_col, max_col);
        }
        let natural_total: f32 = widths.iter().sum();
        if natural_total < available {
            let extra = (available - natural_total) / col_count as f32;
            for width in &mut widths {
                *width += extra;
            }
        }
        let total: f32 = widths.iter().sum();

        struct RowLayout {
            galleys: Vec<Arc<egui::Galley>>,
            height: f32,
        }

        let header = if columns.is_empty() {
            None
        } else {
            let galleys: Vec<Arc<egui::Galley>> = (0..col_count)
                .map(|index| {
                    let text = columns.get(index).cloned().unwrap_or_default();
                    ui.fonts_mut(|fonts| {
                        fonts.layout(
                            text,
                            head_font.clone(),
                            p.fg_muted,
                            (widths[index] - TABLE_PAD_X * 2.0).max(8.0),
                        )
                    })
                })
                .collect();
            let height = galleys
                .iter()
                .map(|galley| galley.size().y)
                .fold(0.0_f32, f32::max)
                .max(self.base * 1.2)
                + TABLE_PAD_Y * 2.0;
            Some(RowLayout { galleys, height })
        };

        let laid_rows: Vec<RowLayout> = rows
            .iter()
            .take(200)
            .map(|row| {
                let galleys: Vec<Arc<egui::Galley>> = (0..col_count)
                    .map(|index| {
                        let text = row.get(index).map(Cell::text).unwrap_or_default();
                        ui.fonts_mut(|fonts| {
                            fonts.layout(
                                text,
                                body_font.clone(),
                                p.fg,
                                (widths[index] - TABLE_PAD_X * 2.0).max(8.0),
                            )
                        })
                    })
                    .collect();
                let height = galleys
                    .iter()
                    .map(|galley| galley.size().y)
                    .fold(0.0_f32, f32::max)
                    .max(self.base * 1.25)
                    + TABLE_PAD_Y * 2.0;
                RowLayout { galleys, height }
            })
            .collect();

        if rows.len() > laid_rows.len() {
            ui.label(
                egui::RichText::new(format!(
                    "Showing the first {} of {} rows.",
                    laid_rows.len(),
                    rows.len()
                ))
                .font(theme::font(self.base * 0.82))
                .color(self.p.fg_muted),
            );
            ui.add_space(3.0);
        }
        let total_height: f32 = header.as_ref().map(|row| row.height).unwrap_or(0.0)
            + laid_rows.iter().map(|row| row.height).sum::<f32>();

        egui::ScrollArea::horizontal()
            .id_salt(ui.next_auto_id())
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(total, total_height), Sense::hover());
                let mut y = rect.top();

                if let Some(header) = &header {
                    let header_rect = Rect::from_min_size(
                        Pos2::new(rect.left(), y),
                        Vec2::new(total, header.height),
                    );
                    ui.painter().rect_filled(
                        header_rect,
                        CornerRadius {
                            nw: theme::radius::MD,
                            ne: theme::radius::MD,
                            sw: 0,
                            se: 0,
                        },
                        p.tile_tone(),
                    );
                    let mut x = rect.left();
                    for (index, galley) in header.galleys.iter().enumerate() {
                        ui.painter().galley(
                            Pos2::new(x + TABLE_PAD_X, y + (header.height - galley.size().y) / 2.0),
                            galley.clone(),
                            p.fg_muted,
                        );
                        x += widths[index];
                    }
                    y += header.height;
                }

                for (row_index, row) in laid_rows.iter().enumerate() {
                    if row_index % 2 == 1 {
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(rect.left(), y),
                                Vec2::new(total, row.height),
                            ),
                            CornerRadius::ZERO,
                            p.wash(p.fg, if p.dark { 6 } else { 5 }),
                        );
                    }
                    let mut x = rect.left();
                    for (index, galley) in row.galleys.iter().enumerate() {
                        let right_aligned = rows[row_index]
                            .get(index)
                            .map(|cell| matches!(cell, Cell::Integer(_) | Cell::Number(_)))
                            .unwrap_or(false);
                        let text_x = if right_aligned {
                            x + widths[index] - TABLE_PAD_X - galley.size().x
                        } else {
                            x + TABLE_PAD_X
                        };
                        ui.painter().galley(
                            Pos2::new(text_x, y + (row.height - galley.size().y) / 2.0),
                            galley.clone(),
                            p.fg,
                        );
                        x += widths[index];
                    }
                    ui.painter().hline(
                        rect.left()..=rect.right(),
                        y + row.height,
                        Stroke::new(theme::HAIRLINE, p.edge_muted()),
                    );
                    y += row.height;
                }

                let mut x = rect.left();
                for width in widths.iter().take(col_count.saturating_sub(1)) {
                    x += *width;
                    ui.painter().line_segment(
                        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                        Stroke::new(theme::HAIRLINE, p.fade(p.border, 0.28)),
                    );
                }
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(theme::radius::MD),
                    Stroke::new(theme::HAIRLINE, p.edge_muted()),
                    StrokeKind::Inside,
                );
            });
    }

    fn list(&mut self, ui: &mut Ui, items: &[ListItem], ordered: bool) {
        let p = self.p;
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                ui.add_space(4.0);
            }
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(theme::LIST_INDENT, 0.0),
                    Layout::top_down(Align::Max),
                    |ui| {
                        let marker = if ordered {
                            format!("{}.", index + 1)
                        } else {
                            "•".to_string()
                        };
                        ui.label(
                            RichText::new(marker)
                                .font(theme::font_medium(self.base * 0.9))
                                .color(if ordered { p.fg_muted } else { p.list_marker }),
                        );
                    },
                );
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&item.title)
                            .font(theme::font_medium(self.base * 0.96))
                            .color(p.fg),
                    );
                    if let Some(markdown) = item.markdown.as_deref()
                        && !markdown.trim().is_empty()
                    {
                        ui.add_space(2.0);
                        let id = ui.next_auto_id();
                        let size = self.base * 0.9;
                        self.markdown.show_at_sized(
                            ui,
                            id,
                            markdown,
                            size,
                            (size * 1.55).max(size + 3.0),
                            p.fg_muted,
                            p,
                        );
                    }
                });
            });
        }
    }

    fn steps(&mut self, ui: &mut Ui, items: &[StepItem], direction: Direction) {
        if items.is_empty() {
            return;
        }
        match direction {
            Direction::Column => self.steps_column(ui, items),
            Direction::Row => self.steps_row(ui, items),
        }
    }

    fn steps_column(&mut self, ui: &mut Ui, items: &[StepItem]) {
        let p = self.p;
        let base = self.base;
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                ui.add_space(10.0);
            }
            let response = ui.horizontal_top(|ui| {
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&item.title)
                            .font(theme::font_medium(base * 0.96))
                            .color(match item.status {
                                StepStatus::Done => p.fg_muted,
                                StepStatus::Todo => p.fg_muted,
                                _ => p.fg,
                            }),
                    );
                    if let Some(markdown) = item.markdown.as_deref()
                        && !markdown.trim().is_empty()
                    {
                        ui.add_space(2.0);
                        let id = ui.next_auto_id();
                        let size = base * 0.88;
                        self.markdown.show_at_sized(
                            ui,
                            id,
                            markdown,
                            size,
                            (size * 1.5).max(size + 3.0),
                            p.fg_muted,
                            p,
                        );
                    }
                });
            });
            let rect = response.response.rect;
            let center = Pos2::new(rect.left() + 8.0, rect.top() + base * 0.62);
            if index + 1 < items.len() {
                ui.painter().line_segment(
                    [
                        Pos2::new(center.x, center.y + 7.0),
                        Pos2::new(center.x, rect.bottom() + 5.0),
                    ],
                    Stroke::new(theme::HAIRLINE, p.edge_muted()),
                );
            }
            paint_step_marker(ui, center, item.status, p);
        }
    }

    fn steps_row(&mut self, ui: &mut Ui, items: &[StepItem]) {
        let p = self.p;
        let base = self.base;
        let gap = theme::SPACE_3;
        let step_width = ((ui.available_width() - gap * (items.len().saturating_sub(1)) as f32)
            / items.len() as f32)
            .max(110.0);
        let mut rects = Vec::with_capacity(items.len());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(gap, theme::SPACE_2);
            for item in items {
                let inner = ui.allocate_ui_with_layout(
                    Vec2::new(step_width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_space(20.0);
                        ui.label(
                            RichText::new(&item.title)
                                .font(theme::font_medium(base * 0.94))
                                .color(match item.status {
                                    StepStatus::Done => p.fg_muted,
                                    StepStatus::Todo => p.fg_muted,
                                    _ => p.fg,
                                }),
                        );
                        if let Some(markdown) = item.markdown.as_deref()
                            && !markdown.trim().is_empty()
                        {
                            ui.add_space(2.0);
                            let id = ui.next_auto_id();
                            let size = base * 0.85;
                            self.markdown.show_at_sized(
                                ui,
                                id,
                                markdown,
                                size,
                                (size * 1.5).max(size + 3.0),
                                p.fg_muted,
                                p,
                            );
                        }
                    },
                );
                rects.push(inner.response.rect);
            }
        });
        for (index, rect) in rects.iter().enumerate() {
            let center = Pos2::new(rect.center().x, rect.top() + 10.0);
            if let Some(next) = rects.get(index + 1)
                && (next.top() - rect.top()).abs() < 4.0
            {
                ui.painter().line_segment(
                    [
                        Pos2::new(center.x + 8.0, center.y),
                        Pos2::new(next.center().x - 8.0, center.y),
                    ],
                    Stroke::new(theme::HAIRLINE, p.edge_muted()),
                );
            }
            paint_step_marker(ui, center, items[index].status, p);
        }
    }

    fn stat(
        &mut self,
        ui: &mut Ui,
        label: &str,
        value: &Cell,
        delta: Option<&str>,
        tone: Option<Tone>,
    ) {
        let p = self.p;
        egui::Frame::new()
            .fill(p.tile_tone())
            .stroke(Stroke::new(theme::HAIRLINE, p.edge_muted()))
            .corner_radius(CornerRadius::same(theme::radius::MD))
            .inner_margin(Margin::symmetric(12, 9))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(label)
                            .font(theme::font(self.base * 0.82))
                            .color(p.fg_muted),
                    );
                    ui.add_space(2.0);
                    let value_color = tone.map(|tone| tone_colors(p, tone).text).unwrap_or(p.fg);
                    ui.label(
                        RichText::new(value.text())
                            .font(theme::font_semibold(self.base * 1.65))
                            .color(value_color),
                    );
                    if let Some(delta) = delta.filter(|value| !value.trim().is_empty()) {
                        ui.add_space(1.0);
                        let color = if let Some(tone) = tone {
                            tone_colors(p, tone).text
                        } else if delta.trim_start().starts_with('-') {
                            p.red
                        } else if delta.trim_start().starts_with('+') {
                            p.green
                        } else {
                            p.fg_muted
                        };
                        ui.label(
                            RichText::new(delta)
                                .font(theme::font_medium(self.base * 0.82))
                                .color(color),
                        );
                    }
                });
            });
    }

    fn progress(&mut self, ui: &mut Ui, label: &str, value: f64, max: f64) {
        let p = self.p;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(label)
                    .font(theme::font_medium(self.base * 0.9))
                    .color(p.fg),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let ratio = if max > 0.0 {
                    (value / max).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                ui.label(
                    RichText::new(format!("{:.0}%", ratio * 100.0))
                        .font(theme::font(self.base * 0.82))
                        .color(p.fg_muted),
                );
            });
        });
        ui.add_space(3.0);
        let width = ui.available_width().max(20.0);
        let height = (self.base * 0.45).clamp(6.0, 9.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let radius = CornerRadius::same((height / 2.0) as u8);
        ui.painter().rect_filled(rect, radius, p.wash(p.fg, 18));
        let ratio = if max > 0.0 {
            (value / max).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        if ratio > 0.0 {
            let fill = Rect::from_min_size(
                rect.min,
                Vec2::new((rect.width() * ratio).max(2.0), rect.height()),
            );
            ui.painter().rect_filled(fill, radius, p.primary);
        }
    }

    fn button(&mut self, ui: &mut Ui, label: &str, variant: ExtButtonVariant, value: Option<&str>) {
        let variant = match variant {
            ExtButtonVariant::Primary => widgets::ButtonVariant::Primary,
            ExtButtonVariant::Secondary => widgets::ButtonVariant::Secondary,
        };
        let response = widgets::button(ui, label, None, variant, ButtonSize::Md, self.p);
        if response.clicked() {
            self.submit(
                ui,
                BlockResponse {
                    button: Some(label.to_string()),
                    value: value.map(str::to_string),
                    values: Map::new(),
                    note: None,
                },
            );
        }
    }

    fn image(&mut self, ui: &mut Ui, path: &str, alt: Option<&str>, height: Option<f32>) {
        let workspace = ui
            .ctx()
            .data(|data| data.get_temp::<String>(egui::Id::unique("shell-cwd")));
        let uri = image_uri(path, workspace.as_deref().map(Path::new));
        if uri.is_empty() {
            self.muted_line(ui, alt.unwrap_or("Image"));
            return;
        }
        let max_height = height
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or((self.base * 18.0).clamp(160.0, 360.0));
        let image = egui::Image::from_uri(uri)
            .alt_text(alt.unwrap_or(path))
            .fit_to_original_size(1.0)
            .max_width(ui.available_width().max(32.0))
            .max_height(max_height)
            .corner_radius(CornerRadius::same(theme::radius::MD))
            .show_loading_spinner(true);
        ui.add(image);
    }

    fn extension(&mut self, ui: &mut Ui, name: &str, args: &[String]) {
        let host = host(ui.ctx());
        if let Some(tree) = host.tree.as_ref()
            && let Some(widgets) = tree(name, args)
        {
            self.widgets(ui, &widgets, theme::PARAGRAPH_GAP);
            return;
        }
        let Some(request) = host.request.as_ref() else {
            self.muted_line(ui, &format!("Unknown component {name}"));
            return;
        };
        let guard = self.auto_id.with(("extension-request", name, args));
        let requested = ui
            .memory(|memory| memory.data.get_temp::<bool>(guard))
            .unwrap_or(false);
        if !requested {
            ui.memory_mut(|memory| memory.data.insert_temp(guard, true));
            request(name, args);
        }
        let p = self.p;
        ui.horizontal(|ui| {
            widgets::spinner(ui, self.base * 0.85, p.fg_muted);
            ui.label(
                RichText::new(format!("Loading {name}…"))
                    .font(theme::font(self.base * 0.9))
                    .color(p.fg_muted),
            );
        });
    }

    fn muted_line(&self, ui: &mut Ui, text: &str) {
        ui.label(
            RichText::new(text)
                .font(theme::font(self.base * 0.9))
                .color(self.p.fg_muted),
        );
    }

    fn field_label(&self, ui: &mut Ui, label: &str) {
        ui.label(
            RichText::new(label)
                .font(theme::font_medium(self.base * 0.86))
                .color(self.p.fg_muted),
        );
        ui.add_space(3.0);
    }

    fn field_id(&self, name: &str) -> Id {
        self.auto_id.with(("block-field", name))
    }

    fn register(&mut self, name: &str, id: Id, kind: FieldKind) {
        if let Some(entry) = self.fields.iter_mut().find(|entry| entry.id == id) {
            entry.name = name.to_string();
            entry.kind = kind;
        } else {
            self.fields.push(FieldEntry {
                name: name.to_string(),
                id,
                kind,
            });
        }
    }

    fn input(
        &mut self,
        ui: &mut Ui,
        name: &str,
        label: Option<&str>,
        placeholder: Option<&str>,
        multiline: bool,
    ) {
        if let Some(label) = label.filter(|value| !value.trim().is_empty()) {
            self.field_label(ui, label);
        }
        let id = self.field_id(name);
        let mut text = ui
            .memory(|memory| memory.data.get_temp::<String>(id))
            .unwrap_or_default();
        let hint = placeholder.unwrap_or("");
        let response = if multiline {
            widgets::textarea_ex(ui, &mut text, hint, true, self.p, id)
        } else {
            widgets::text_field_ex(ui, &mut text, hint, false, true, self.p, id)
        };
        ui.memory_mut(|memory| memory.data.insert_temp(id, text.clone()));
        self.register(name, id, FieldKind::Text);
        let enter = if multiline {
            response.has_focus()
                && ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter))
        } else {
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
        };
        if enter {
            self.submit(
                ui,
                BlockResponse {
                    button: None,
                    value: None,
                    values: single_value(name, &text),
                    note: None,
                },
            );
        }
    }

    fn date_input(
        &mut self,
        ui: &mut Ui,
        name: &str,
        label: Option<&str>,
        time: bool,
        value: Option<&str>,
    ) {
        if let Some(label) = label.filter(|value| !value.trim().is_empty()) {
            self.field_label(ui, label);
        }
        let id = self.field_id(name);
        let default = value.unwrap_or_default().to_string();
        let mut text = ui
            .memory(|memory| memory.data.get_temp::<String>(id))
            .unwrap_or(default);
        let hint = if time {
            "YYYY-MM-DD HH:MM"
        } else {
            "YYYY-MM-DD"
        };
        let response = widgets::text_field_ex(ui, &mut text, hint, false, true, self.p, id);
        ui.memory_mut(|memory| memory.data.insert_temp(id, text.clone()));
        self.register(name, id, FieldKind::Text);
        if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            self.submit(
                ui,
                BlockResponse {
                    button: None,
                    value: None,
                    values: single_value(name, &text),
                    note: None,
                },
            );
        }
    }

    fn slider(
        &mut self,
        ui: &mut Ui,
        name: &str,
        min: f64,
        max: f64,
        label: Option<&str>,
        step: Option<f64>,
        value: Option<f64>,
        unit: Option<&str>,
    ) {
        let p = self.p;
        let id = self.field_id(name);
        let mut lower = min.min(max);
        let mut upper = min.max(max);
        if (upper - lower).abs() < f64::EPSILON {
            upper = lower + 1.0;
        }
        if !lower.is_finite() || !upper.is_finite() {
            lower = 0.0;
            upper = 1.0;
        }
        let mut current = ui
            .memory(|memory| memory.data.get_temp::<f64>(id))
            .unwrap_or_else(|| value.unwrap_or(lower));
        current = current.clamp(lower, upper);
        let title = label.unwrap_or(name);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(title)
                    .font(theme::font_medium(self.base * 0.86))
                    .color(p.fg_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let display = match unit.filter(|value| !value.trim().is_empty()) {
                    Some(unit) => format!("{}{unit}", format_chart_number(current, false)),
                    None => format_chart_number(current, false),
                };
                ui.label(
                    RichText::new(display)
                        .font(theme::font_mono(self.base * 0.8))
                        .color(p.fg_muted),
                );
            });
        });
        ui.add_space(2.0);
        let mut knob = current as f32;
        let response = widgets::slider(ui, &mut knob, lower as f32, upper as f32, p);
        if response.changed() {
            let mut next = knob as f64;
            if let Some(step) = step.filter(|step| *step > 0.0) {
                next = lower + ((next - lower) / step).round() * step;
            }
            current = next.clamp(lower, upper);
            ui.memory_mut(|memory| memory.data.insert_temp(id, current));
        } else {
            ui.memory_mut(|memory| memory.data.insert_temp(id, current));
        }
        self.register(name, id, FieldKind::Number);
        if response.drag_stopped() || response.clicked() {
            let mut values = Map::new();
            values.insert(name.to_string(), number_value(current));
            self.submit(
                ui,
                BlockResponse {
                    button: None,
                    value: None,
                    values,
                    note: None,
                },
            );
        }
    }

    fn choice(
        &mut self,
        ui: &mut Ui,
        name: &str,
        options: &[ChoiceOption],
        label: Option<&str>,
        multiple: bool,
    ) {
        if let Some(label) = label.filter(|value| !value.trim().is_empty()) {
            self.field_label(ui, label);
        }
        let id = self.field_id(name);
        self.register(
            name,
            id,
            if multiple {
                FieldKind::Multiple
            } else {
                FieldKind::Single
            },
        );
        let has_description = options.iter().any(|option| {
            option
                .description()
                .is_some_and(|value| !value.trim().is_empty())
        });
        let mut clicked: Option<String> = None;

        if multiple {
            let selected = ui
                .memory(|memory| memory.data.get_temp::<Vec<String>>(id))
                .unwrap_or_default();
            if has_description {
                for option in options {
                    let current = selected.iter().any(|value| value == option.label());
                    let response = self.choice_row(ui, option, current, true);
                    if response.clicked() {
                        clicked = Some(option.label().to_string());
                    }
                }
            } else {
                let p = self.p;
                ui.horizontal_wrapped(|ui| {
                    for option in options {
                        let current = selected.iter().any(|value| value == option.label());
                        if widgets::chip_ex(ui, option.label(), None, current, true, p).clicked() {
                            clicked = Some(option.label().to_string());
                        }
                    }
                });
            }
            if let Some(choice) = clicked {
                let mut next = selected;
                if let Some(index) = next.iter().position(|value| value == &choice) {
                    next.remove(index);
                } else {
                    next.push(choice);
                }
                ui.memory_mut(|memory| memory.data.insert_temp(id, next.clone()));
                self.submit_choice(ui, name, true, &next);
            }
        } else {
            let selected = ui.memory(|memory| memory.data.get_temp::<String>(id));
            if has_description {
                for option in options {
                    let current = selected.as_deref() == Some(option.label());
                    let response = self.choice_row(ui, option, current, false);
                    if response.clicked() {
                        clicked = Some(option.label().to_string());
                    }
                }
            } else {
                let p = self.p;
                ui.horizontal_wrapped(|ui| {
                    for option in options {
                        let current = selected.as_deref() == Some(option.label());
                        if widgets::chip_ex(ui, option.label(), None, current, true, p).clicked() {
                            clicked = Some(option.label().to_string());
                        }
                    }
                });
            }
            if let Some(choice) = clicked
                && selected.as_deref() != Some(choice.as_str())
            {
                ui.memory_mut(|memory| memory.data.insert_temp(id, choice.clone()));
                self.submit_choice(ui, name, false, std::slice::from_ref(&choice));
            }
        }
    }

    fn choice_row(
        &self,
        ui: &mut Ui,
        option: &ChoiceOption,
        selected: bool,
        multiple: bool,
    ) -> egui::Response {
        let p = self.p;
        let width = ui.available_width().max(60.0);
        let text_width = (width - 48.0).max(30.0);
        let label_font = theme::font_medium(self.base * 0.94);
        let label_galley = ui.painter().layout(
            option.label().to_string(),
            label_font,
            Color32::PLACEHOLDER,
            text_width,
        );
        let description = option
            .description()
            .filter(|value| !value.trim().is_empty());
        let desc_font = theme::font(self.base * 0.82);
        let desc_galley = description.map(|value| {
            ui.painter().layout(
                value.to_string(),
                desc_font,
                Color32::PLACEHOLDER,
                text_width,
            )
        });
        let pad_y = 8.0;
        let mut height = label_galley.size().y + pad_y * 2.0;
        if let Some(description) = &desc_galley {
            height += 2.0 + description.size().y;
        }
        height = height.max(self.base * 2.1);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        let fill = if selected {
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
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::radius::MD),
            Stroke::new(
                if selected {
                    theme::BORDER
                } else {
                    theme::HAIRLINE
                },
                if selected {
                    p.wash(p.primary, 130)
                } else {
                    p.edge_muted()
                },
            ),
            StrokeKind::Inside,
        );
        let label_height = label_galley.size().y;
        ui.painter().galley(
            Pos2::new(rect.left() + 12.0, rect.top() + pad_y),
            label_galley,
            p.fg,
        );
        if let Some(description) = desc_galley {
            ui.painter().galley(
                Pos2::new(rect.left() + 12.0, rect.top() + pad_y + label_height + 2.0),
                description,
                p.fg_muted,
            );
        }
        let center = Pos2::new(rect.right() - 18.0, rect.center().y);
        if multiple {
            let box_rect = Rect::from_center_size(center, Vec2::splat(14.0));
            ui.painter().rect(
                box_rect,
                CornerRadius::same(3),
                if selected {
                    p.primary
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::new(
                    theme::BORDER,
                    if selected { p.primary } else { p.field_edge() },
                ),
                StrokeKind::Inside,
            );
            if selected {
                paint_check(ui, box_rect.shrink(3.0), p.on_primary);
            }
        } else {
            ui.painter().circle_stroke(
                center,
                7.0,
                Stroke::new(
                    theme::BORDER,
                    if selected { p.primary } else { p.field_edge() },
                ),
            );
            if selected {
                ui.painter().circle_filled(center, 3.5, p.primary);
            }
        }
        widgets::pointer(ui, &response);
        response
    }

    fn submit_choice(&self, ui: &Ui, name: &str, multiple: bool, labels: &[String]) {
        let mut values = Map::new();
        values.insert(name.to_string(), choice_value(multiple, labels));
        self.submit(
            ui,
            BlockResponse {
                button: None,
                value: None,
                values,
                note: None,
            },
        );
    }

    fn collect_values(&self, ui: &Ui) -> Map<String, Value> {
        let mut entries = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            let value = match field.kind {
                FieldKind::Text => FieldValue::Text(
                    ui.memory(|memory| memory.data.get_temp::<String>(field.id))
                        .unwrap_or_default(),
                ),
                FieldKind::Number => FieldValue::Number(
                    ui.memory(|memory| memory.data.get_temp::<f64>(field.id))
                        .unwrap_or(0.0),
                ),
                FieldKind::Single => {
                    FieldValue::Single(ui.memory(|memory| memory.data.get_temp::<String>(field.id)))
                }
                FieldKind::Multiple => FieldValue::Multiple(
                    ui.memory(|memory| memory.data.get_temp::<Vec<String>>(field.id))
                        .unwrap_or_default(),
                ),
            };
            entries.push((field.name.clone(), value));
        }
        form_values(&entries)
    }

    fn send_button(&mut self, ui: &mut Ui) {
        if self.fields.is_empty() {
            return;
        }
        let values = self.collect_values(ui);
        ui.add_space(theme::SPACE_3);
        let p = self.p;
        let response = ui
            .with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::button(
                    ui,
                    "Send",
                    None,
                    widgets::ButtonVariant::Primary,
                    ButtonSize::Md,
                    p,
                )
            })
            .inner;
        if response.clicked() {
            self.submit(
                ui,
                BlockResponse {
                    button: Some("Send".to_string()),
                    value: None,
                    values,
                    note: None,
                },
            );
        }
    }

    fn submit(&self, ui: &Ui, response: BlockResponse) {
        if let Some(submit) = host(ui.ctx()).submit {
            submit(response);
        }
    }

    fn muted_series_legend(&self, ui: &mut Ui, series: &[Series]) {
        if series.is_empty() {
            return;
        }
        let p = self.p;
        let font = theme::font(self.base * 0.8);
        let row_height = ui.ctx().fonts_mut(|fonts| fonts.row_height(&font));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 3.0);
            for (index, item) in series.iter().enumerate() {
                let color = series_color(index, p);
                let galley =
                    ui.painter()
                        .layout_no_wrap(item.name.clone(), font.clone(), p.fg_muted);
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(12.0 + galley.size().x, row_height.max(16.0)),
                    Sense::hover(),
                );
                ui.painter().circle_filled(
                    Pos2::new(rect.left() + 4.0, rect.center().y),
                    4.0,
                    color,
                );
                ui.painter().galley(
                    Pos2::new(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0),
                    galley,
                    p.fg_muted,
                );
            }
        });
    }

    fn bar_chart(
        &mut self,
        ui: &mut Ui,
        labels: &[String],
        series: &[Series],
        unit: Option<&str>,
        whole: bool,
    ) {
        if series.is_empty() {
            return;
        }
        let p = self.p;
        self.muted_series_legend(ui, series);
        ui.add_space(4.0);
        let groups = labels
            .len()
            .max(
                series
                    .iter()
                    .map(|item| item.values.len())
                    .max()
                    .unwrap_or(0),
            )
            .max(1);
        let (min, max) = chart_scale(series, true);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(120.0), chart_height(self.base)),
            Sense::hover(),
        );
        let tick_font = theme::font(self.base * 0.72);
        let tick_labels: Vec<String> = (0..=CHART_TICKS)
            .map(|index| {
                let value = min + (max - min) * index as f64 / CHART_TICKS as f64;
                format_chart_number(value, whole)
            })
            .collect();
        let gutter = tick_labels
            .iter()
            .map(|label| {
                ui.painter()
                    .layout_no_wrap(label.clone(), tick_font.clone(), Color32::PLACEHOLDER)
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max)
            .max(18.0)
            + 8.0;
        let plot = Rect::from_min_max(
            Pos2::new(rect.left() + gutter, rect.top() + 6.0),
            Pos2::new(rect.right() - 4.0, rect.bottom() - 20.0),
        );
        let plot = expand_plot(plot);
        ui.painter().rect_filled(
            plot,
            CornerRadius::same(4),
            p.wash(p.fg, if p.dark { 5 } else { 4 }),
        );

        for (index, label) in tick_labels.iter().enumerate() {
            let value = min + (max - min) * index as f64 / CHART_TICKS as f64;
            let y = map_y(value, min, max, plot);
            ui.painter().hline(
                plot.left()..=plot.right(),
                y,
                Stroke::new(
                    theme::HAIRLINE,
                    if value.abs() < f64::EPSILON {
                        p.edge_color()
                    } else {
                        p.fade(p.border, 0.34)
                    },
                ),
            );
            ui.painter().text(
                Pos2::new(plot.left() - 6.0, y),
                Align2::RIGHT_CENTER,
                label,
                tick_font.clone(),
                p.fg_muted,
            );
        }

        let group_width = plot.width() / groups as f32;
        let series_count = series.len().max(1);
        let inner = (group_width * 0.72).max(1.0);
        let bar_width = (inner / series_count as f32).max(1.0);
        let hovered = response
            .hover_pos()
            .filter(|position| plot.contains(*position))
            .map(|position| {
                (((position.x - plot.left()) / group_width).floor() as usize).min(groups - 1)
            });
        if let Some(group) = hovered {
            let highlight = Rect::from_min_max(
                Pos2::new(plot.left() + group_width * group as f32, plot.top()),
                Pos2::new(
                    plot.left() + group_width * (group + 1) as f32,
                    plot.bottom(),
                ),
            );
            ui.painter()
                .rect_filled(highlight, CornerRadius::ZERO, p.wash(p.fg, 10));
        }
        for group in 0..groups {
            let group_left = plot.left() + group_width * group as f32;
            for (series_index, item) in series.iter().enumerate() {
                let Some(Some(value)) = item.values.get(group) else {
                    continue;
                };
                if !value.is_finite() {
                    continue;
                }
                let x = group_left + (group_width - inner) / 2.0 + bar_width * series_index as f32;
                let top = map_y(*value, min, max, plot);
                let zero = map_y(0.0_f64.clamp(min, max), min, max, plot);
                let bar = Rect::from_min_max(
                    Pos2::new(x, top.min(zero)),
                    Pos2::new(x + (bar_width - 1.0).max(1.0), top.max(zero)),
                );
                ui.painter()
                    .rect_filled(bar, CornerRadius::same(2), series_color(series_index, p));
            }
        }

        let stride = chart_label_stride(groups, plot.width(), self.base);
        let label_font = theme::font(self.base * 0.72);
        for (index, label) in labels.iter().enumerate() {
            if index % stride != 0 {
                continue;
            }
            let text = fit_label(
                ui,
                label,
                &label_font,
                (group_width * stride as f32 - 4.0).max(12.0),
            );
            ui.painter().text(
                Pos2::new(
                    plot.left() + group_width * index as f32 + group_width / 2.0,
                    plot.bottom() + 4.0,
                ),
                Align2::CENTER_TOP,
                text,
                label_font.clone(),
                p.fg_muted,
            );
        }
        if let Some(unit) = unit.filter(|value| !value.trim().is_empty()) {
            ui.painter().text(
                Pos2::new(plot.right() - 3.0, rect.top() + 2.0),
                Align2::RIGHT_TOP,
                unit,
                theme::font(self.base * 0.72),
                p.fg_muted,
            );
        }

        if let Some(group) = hovered {
            let label = labels.get(group).cloned().unwrap_or_default();
            let values: Vec<(Color32, String)> = series
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    let value = item.values.get(group)?.as_ref()?;
                    Some((
                        series_color(index, p),
                        format!("{}: {}", item.name, value_unit(*value, whole, unit)),
                    ))
                })
                .collect();
            let text = theme::font(self.base * 0.86);
            let title = theme::font_medium(self.base * 0.9);
            response.on_hover_ui(|ui| {
                if !label.is_empty() {
                    ui.label(RichText::new(label).font(title).color(p.fg));
                }
                for (color, line) in values {
                    ui.colored_label(color, RichText::new(line).font(text.clone()));
                }
            });
        }
    }

    fn line_chart(
        &mut self,
        ui: &mut Ui,
        labels: &[String],
        series: &[Series],
        unit: Option<&str>,
    ) {
        if series.is_empty() {
            return;
        }
        let p = self.p;
        self.muted_series_legend(ui, series);
        ui.add_space(4.0);
        let points = labels
            .len()
            .max(
                series
                    .iter()
                    .map(|item| item.values.len())
                    .max()
                    .unwrap_or(0),
            )
            .max(1);
        let (min, max) = chart_scale(series, false);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(120.0), chart_height(self.base)),
            Sense::hover(),
        );
        let tick_font = theme::font(self.base * 0.72);
        let tick_labels: Vec<String> = (0..=CHART_TICKS)
            .map(|index| {
                let value = min + (max - min) * index as f64 / CHART_TICKS as f64;
                format_chart_number(value, false)
            })
            .collect();
        let gutter = tick_labels
            .iter()
            .map(|label| {
                ui.painter()
                    .layout_no_wrap(label.clone(), tick_font.clone(), Color32::PLACEHOLDER)
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max)
            .max(18.0)
            + 8.0;
        let plot = Rect::from_min_max(
            Pos2::new(rect.left() + gutter, rect.top() + 6.0),
            Pos2::new(rect.right() - 4.0, rect.bottom() - 20.0),
        );
        let plot = expand_plot(plot);
        ui.painter().rect_filled(
            plot,
            CornerRadius::same(4),
            p.wash(p.fg, if p.dark { 5 } else { 4 }),
        );

        for (index, label) in tick_labels.iter().enumerate() {
            let value = min + (max - min) * index as f64 / CHART_TICKS as f64;
            let y = map_y(value, min, max, plot);
            ui.painter().hline(
                plot.left()..=plot.right(),
                y,
                Stroke::new(
                    theme::HAIRLINE,
                    if value.abs() < f64::EPSILON {
                        p.edge_color()
                    } else {
                        p.fade(p.border, 0.34)
                    },
                ),
            );
            ui.painter().text(
                Pos2::new(plot.left() - 6.0, y),
                Align2::RIGHT_CENTER,
                label,
                tick_font.clone(),
                p.fg_muted,
            );
        }

        let x_at = |index: usize| -> f32 {
            if points <= 1 {
                plot.center().x
            } else {
                plot.left() + plot.width() * index as f32 / (points - 1) as f32
            }
        };

        for (series_index, item) in series.iter().enumerate() {
            let color = series_color(series_index, p);
            let mut previous: Option<Pos2> = None;
            for (index, value) in item.values.iter().enumerate() {
                let Some(value) = value.filter(|value| value.is_finite()) else {
                    previous = None;
                    continue;
                };
                let point = Pos2::new(x_at(index), map_y(value, min, max, plot));
                if let Some(previous) = previous {
                    ui.painter()
                        .line_segment([previous, point], Stroke::new(1.8, color));
                }
                ui.painter().circle_filled(point, 2.5, color);
                previous = Some(point);
            }
        }

        let hovered = response
            .hover_pos()
            .filter(|position| plot.contains(*position))
            .map(|position| {
                if points <= 1 {
                    0
                } else {
                    (((position.x - plot.left()) / plot.width() * (points - 1) as f32).round()
                        as usize)
                        .min(points - 1)
                }
            });
        if let Some(index) = hovered {
            let x = x_at(index);
            ui.painter().line_segment(
                [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
                Stroke::new(theme::HAIRLINE, p.edge_color()),
            );
        }

        let stride = chart_label_stride(points, plot.width(), self.base);
        let label_font = theme::font(self.base * 0.72);
        for (index, label) in labels.iter().enumerate() {
            if index % stride != 0 {
                continue;
            }
            let step = if points <= 1 {
                plot.width()
            } else {
                plot.width() / (points - 1) as f32
            };
            let text = fit_label(
                ui,
                label,
                &label_font,
                (step * stride as f32 - 4.0).max(12.0),
            );
            ui.painter().text(
                Pos2::new(x_at(index), plot.bottom() + 4.0),
                Align2::CENTER_TOP,
                text,
                label_font.clone(),
                p.fg_muted,
            );
        }
        if let Some(unit) = unit.filter(|value| !value.trim().is_empty()) {
            ui.painter().text(
                Pos2::new(plot.right() - 3.0, rect.top() + 2.0),
                Align2::RIGHT_TOP,
                unit,
                theme::font(self.base * 0.72),
                p.fg_muted,
            );
        }

        if let Some(index) = hovered {
            let label = labels.get(index).cloned().unwrap_or_default();
            let values: Vec<(Color32, String)> = series
                .iter()
                .enumerate()
                .filter_map(|(series_index, item)| {
                    let value = item.values.get(index)?.as_ref()?;
                    Some((
                        series_color(series_index, p),
                        format!("{}: {}", item.name, value_unit(*value, false, unit)),
                    ))
                })
                .collect();
            let text = theme::font(self.base * 0.86);
            let title = theme::font_medium(self.base * 0.9);
            response.on_hover_ui(|ui| {
                if !label.is_empty() {
                    ui.label(RichText::new(label).font(title).color(p.fg));
                }
                for (color, line) in values {
                    ui.colored_label(color, RichText::new(line).font(text.clone()));
                }
            });
        }
    }

    fn details(&mut self, ui: &mut Ui, title: &str, children: &[Widget]) {
        let p = self.p;
        let state_id = ui.next_auto_id();
        let mut open = ui
            .memory(|memory| memory.data.get_temp::<bool>(state_id))
            .unwrap_or(false);
        let height = (self.base * 2.0).max(28.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::radius::SM), p.hover);
        }
        let chevron = Pos2::new(rect.left() + 8.0, rect.center().y);
        let chevron_color = p.fg_muted;
        if open {
            ui.painter().line_segment(
                [
                    Pos2::new(chevron.x - 3.5, chevron.y - 1.5),
                    Pos2::new(chevron.x, chevron.y + 2.0),
                ],
                Stroke::new(1.4, chevron_color),
            );
            ui.painter().line_segment(
                [
                    Pos2::new(chevron.x, chevron.y + 2.0),
                    Pos2::new(chevron.x + 3.5, chevron.y - 1.5),
                ],
                Stroke::new(1.4, chevron_color),
            );
        } else {
            ui.painter().line_segment(
                [
                    Pos2::new(chevron.x - 1.5, chevron.y - 3.5),
                    Pos2::new(chevron.x + 2.0, chevron.y),
                ],
                Stroke::new(1.4, chevron_color),
            );
            ui.painter().line_segment(
                [
                    Pos2::new(chevron.x + 2.0, chevron.y),
                    Pos2::new(chevron.x - 1.5, chevron.y + 3.5),
                ],
                Stroke::new(1.4, chevron_color),
            );
        }
        let title_font = theme::font_medium(self.base * 0.96);
        let title_text = fit_label(ui, title, &title_font, (rect.width() - 30.0).max(20.0));
        ui.painter().text(
            Pos2::new(rect.left() + 18.0, rect.center().y),
            Align2::LEFT_CENTER,
            title_text,
            title_font,
            p.fg,
        );
        widgets::pointer(ui, &response);
        if response.clicked() {
            open = !open;
            ui.memory_mut(|memory| memory.data.insert_temp(state_id, open));
        }
        if open {
            ui.add_space(2.0);
            ui.horizontal_top(|ui| {
                ui.add_space(18.0);
                ui.vertical(|ui| {
                    self.widgets(ui, children, theme::PARAGRAPH_GAP);
                });
            });
        }
    }
}

fn paint_step_marker(ui: &Ui, center: Pos2, status: StepStatus, p: &Palette) {
    match status {
        StepStatus::Done => {
            ui.painter().circle_filled(center, 6.0, p.green);
            paint_check(
                ui,
                Rect::from_center_size(center, Vec2::splat(6.0)),
                p.on_primary,
            );
        }
        StepStatus::Current => {
            ui.painter().circle_filled(center, 6.5, p.primary);
            ui.painter().circle_filled(center, 2.5, p.bg);
        }
        StepStatus::Blocked => {
            ui.painter().circle_filled(center, 6.0, p.red);
            let arm = 2.6;
            let color = p.on_primary;
            ui.painter().line_segment(
                [
                    Pos2::new(center.x - arm, center.y - arm),
                    Pos2::new(center.x + arm, center.y + arm),
                ],
                Stroke::new(1.3, color),
            );
            ui.painter().line_segment(
                [
                    Pos2::new(center.x + arm, center.y - arm),
                    Pos2::new(center.x - arm, center.y + arm),
                ],
                Stroke::new(1.3, color),
            );
        }
        StepStatus::Todo => {
            ui.painter()
                .circle_stroke(center, 5.5, Stroke::new(1.5, p.edge_color()));
        }
    }
}

fn paint_check(ui: &Ui, rect: Rect, color: Color32) {
    let start = Pos2::new(rect.left() + rect.width() * 0.18, rect.center().y);
    let middle = Pos2::new(
        rect.left() + rect.width() * 0.42,
        rect.bottom() - rect.height() * 0.22,
    );
    let end = Pos2::new(
        rect.right() - rect.width() * 0.14,
        rect.top() + rect.height() * 0.22,
    );
    ui.painter()
        .line_segment([start, middle], Stroke::new(1.6, color));
    ui.painter()
        .line_segment([middle, end], Stroke::new(1.6, color));
}

fn code_action_button(ui: &mut Ui, label: &str, primary: bool, p: &Palette) -> egui::Response {
    let font = theme::font(theme::TEXT_TAG);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 16.0, 22.0), Sense::click());
    crate::widgets::pointer(ui, &response);
    let (fill, border, ink) = if primary {
        (
            if response.hovered() {
                p.primary_hover
            } else {
                p.primary
            },
            Color32::TRANSPARENT,
            p.on_primary,
        )
    } else {
        (
            if response.hovered() { p.hover } else { p.card },
            p.border,
            if response.hovered() { p.fg } else { p.fg_muted },
        )
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::radius::SM), fill);
    if border != Color32::TRANSPARENT {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::radius::SM),
            Stroke::new(theme::BORDER, border),
            StrokeKind::Inside,
        );
    }
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        ink,
    );
    response
}

fn looks_like_diff(code: &str) -> bool {
    let mut hunk = false;
    let mut header = false;
    for line in code.lines() {
        if line.starts_with("@@") {
            hunk = true;
        }
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            header = true;
        }
        if hunk && header {
            return true;
        }
    }
    false
}

fn code_diff(ui: &mut Ui, p: &Palette, code: &str) {
    let font = theme::font_mono(theme::TEXT_CODE);
    for line in code.lines() {
        let (color, fill) = if line.starts_with("+++") || line.starts_with("---") {
            (p.fg_faint, None)
        } else if line.starts_with('+') {
            (p.diff_added, Some(p.diff_added_bg))
        } else if line.starts_with('-') {
            (p.diff_removed, Some(p.diff_removed_bg))
        } else if line.starts_with("@@") {
            (p.fg_muted, None)
        } else {
            (p.fg_muted, None)
        };
        let galley = ui
            .painter()
            .layout_no_wrap(line.to_string(), font.clone(), color);
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(
                ui.available_width().max(galley.size().x + 8.0),
                theme::LINE_CODE_ROW,
            ),
            Sense::hover(),
        );
        if let Some(fill) = fill {
            ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
        }
        ui.painter().galley(
            Pos2::new(rect.left() + 2.0, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
    }
}

fn tone_colors(p: &Palette, tone: Tone) -> ToneColors {
    let (accent, text) = match tone {
        Tone::Neutral => (p.fg_muted, p.fg),
        Tone::Info => (p.blue, p.info_text),
        Tone::Success => (p.green, p.success_text),
        Tone::Warning => (p.coral, p.warning_text),
        Tone::Error => (p.red, p.error_text),
    };
    let fill = match tone {
        Tone::Neutral => p.tile_tone(),
        _ => p.wash(accent, if p.dark { 26 } else { 20 }),
    };
    let border = match tone {
        Tone::Neutral => p.edge_muted(),
        _ => p.wash(accent, if p.dark { 82 } else { 64 }),
    };
    ToneColors {
        accent,
        text,
        fill,
        border,
    }
}

fn series_color(index: usize, p: &Palette) -> Color32 {
    match index % 6 {
        0 => p.primary,
        1 => p.blue,
        2 => p.green,
        3 => p.coral,
        4 => p.purple,
        _ => p.red,
    }
}

fn single_value(name: &str, text: &str) -> Map<String, Value> {
    let mut values = Map::new();
    values.insert(name.to_string(), Value::String(text.to_string()));
    values
}

fn number_value(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn choice_value(multiple: bool, labels: &[String]) -> Value {
    if multiple {
        Value::Array(
            labels
                .iter()
                .map(|label| Value::String(label.clone()))
                .collect(),
        )
    } else {
        Value::String(labels.first().cloned().unwrap_or_default())
    }
}

fn form_values(entries: &[(String, FieldValue)]) -> Map<String, Value> {
    let mut values = Map::new();
    for (name, value) in entries {
        match value {
            FieldValue::Text(text) => {
                values.insert(name.clone(), Value::String(text.clone()));
            }
            FieldValue::Number(number) => {
                values.insert(name.clone(), number_value(*number));
            }
            FieldValue::Single(label) => {
                if let Some(label) = label {
                    values.insert(name.clone(), Value::String(label.clone()));
                }
            }
            FieldValue::Multiple(labels) => {
                values.insert(
                    name.clone(),
                    Value::Array(
                        labels
                            .iter()
                            .map(|label| Value::String(label.clone()))
                            .collect(),
                    ),
                );
            }
        }
    }
    values
}

fn chart_bounds(series: &[Series], include_zero: bool) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut any = false;
    for value in series.iter().flat_map(|item| item.values.iter().flatten()) {
        if !value.is_finite() {
            continue;
        }
        any = true;
        min = min.min(*value);
        max = max.max(*value);
    }
    if !any {
        return None;
    }
    if include_zero {
        min = min.min(0.0);
        max = max.max(0.0);
    }
    Some((min, max))
}

fn chart_scale(series: &[Series], include_zero: bool) -> (f64, f64) {
    let Some((mut min, mut max)) = chart_bounds(series, include_zero) else {
        return (0.0, 1.0);
    };
    if include_zero {
        min = min.min(0.0);
        max = max.max(0.0);
    }
    let span = max - min;
    let pad = if span.abs() < f64::EPSILON {
        (max.abs() * 0.1).max(1.0)
    } else {
        span * 0.08
    };
    if include_zero {
        if min < 0.0 {
            min -= pad;
        }
        if max > 0.0 {
            max += pad;
        }
    } else {
        min -= pad;
        max += pad;
    }
    if (max - min).abs() < f64::EPSILON {
        max = min + 1.0;
    }
    (min, max)
}

fn chart_height(base: f32) -> f32 {
    (base * 9.2).clamp(126.0, 188.0)
}

fn chart_label_stride(groups: usize, width: f32, base: f32) -> usize {
    if groups <= 1 {
        return 1;
    }
    let estimated = (base * 3.6).max(26.0);
    let needed = (groups as f32 * estimated / width.max(1.0)).ceil() as usize;
    needed.max(1)
}

fn expand_plot(rect: Rect) -> Rect {
    if rect.width() < 4.0 || rect.height() < 4.0 {
        Rect::from_min_size(rect.min, Vec2::splat(4.0))
    } else {
        rect
    }
}

fn map_y(value: f64, min: f64, max: f64, plot: Rect) -> f32 {
    let ratio = if (max - min).abs() < f64::EPSILON {
        0.5
    } else {
        (value - min) / (max - min)
    };
    plot.bottom() - (ratio as f32).clamp(-0.2, 1.2) * plot.height()
}

fn format_chart_number(value: f64, whole: bool) -> String {
    if !value.is_finite() {
        return "—".to_string();
    }
    if whole {
        return format!("{value:.0}");
    }
    let abs = value.abs();
    if abs >= 1_000_000_000.0 {
        trim_number(format!("{:.1}", value / 1_000_000_000.0), "B")
    } else if abs >= 1_000_000.0 {
        trim_number(format!("{:.1}", value / 1_000_000.0), "M")
    } else if abs >= 1_000.0 {
        trim_number(format!("{:.1}", value / 1_000.0), "k")
    } else if abs >= 10.0 {
        trim_number(format!("{value:.1}"), "")
    } else if abs >= 1.0 {
        trim_number(format!("{value:.2}"), "")
    } else if abs > 0.0 {
        trim_number(format!("{value:.3}"), "")
    } else {
        "0".to_string()
    }
}

fn trim_number(text: String, suffix: &str) -> String {
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    format!("{text}{suffix}")
}

fn value_unit(value: f64, whole: bool, unit: Option<&str>) -> String {
    let number = format_chart_number(value, whole);
    match unit.filter(|value| !value.trim().is_empty()) {
        Some(unit) => format!("{number}{unit}"),
        None => number,
    }
}

fn fit_label(ui: &Ui, text: &str, font: &FontId, max_width: f32) -> String {
    let width = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), Color32::PLACEHOLDER)
        .size()
        .x;
    if width <= max_width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate = format!("{}…", chars.iter().collect::<String>());
        let width = ui
            .painter()
            .layout_no_wrap(candidate.clone(), font.clone(), Color32::PLACEHOLDER)
            .size()
            .x;
        if width <= max_width {
            return candidate;
        }
    }
    "…".to_string()
}

fn image_uri(path: &str, workspace: Option<&Path>) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    if path.starts_with("data:image/") {
        return path.to_string();
    }
    if path.starts_with("http://") || path.starts_with("https://") || path.starts_with("bytes://") {
        return String::new();
    }
    let raw = path.strip_prefix("file://").unwrap_or(path);
    let requested = Path::new(raw);
    if requested.is_absolute() && !requested.starts_with("/") {
        return String::new();
    }
    let Some(workspace) = workspace else {
        return String::new();
    };
    let Ok(base) = workspace.canonicalize() else {
        return String::new();
    };
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        base.join(requested)
    };
    let Ok(canonical) = candidate.canonicalize() else {
        return String::new();
    };
    if !canonical.starts_with(&base) {
        return String::new();
    }
    let normalized = canonical.to_string_lossy().replace('\\', "/");
    let prefix = if normalized.starts_with('/') {
        "file://"
    } else {
        "file:///"
    };
    let mut uri = format!("{prefix}{normalized}");
    for (from, to) in [("%", "%25"), (" ", "%20"), ("#", "%23"), ("?", "%3F")] {
        uri = uri.replace(from, to);
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tone_colors_follow_the_palette() {
        let dark = Palette::dark();
        assert_eq!(tone_colors(&dark, Tone::Error).accent, dark.red);
        assert_eq!(tone_colors(&dark, Tone::Success).text, dark.success_text);
        assert_eq!(tone_colors(&dark, Tone::Info).accent, dark.blue);
        assert_eq!(tone_colors(&dark, Tone::Neutral).accent, dark.fg_muted);
        let light = Palette::light();
        assert_eq!(tone_colors(&light, Tone::Warning).text, light.warning_text);
        assert_eq!(tone_colors(&light, Tone::Error).accent, light.red);
    }

    #[test]
    fn form_values_collect_text_numbers_and_choices() {
        let entries = vec![
            ("name".to_string(), FieldValue::Text("Ada".to_string())),
            ("count".to_string(), FieldValue::Number(3.0)),
            ("none".to_string(), FieldValue::Single(None)),
            (
                "fruit".to_string(),
                FieldValue::Single(Some("Pear".to_string())),
            ),
            (
                "tags".to_string(),
                FieldValue::Multiple(vec!["a".to_string(), "b".to_string()]),
            ),
        ];
        let values = form_values(&entries);
        assert_eq!(values["name"], json!("Ada"));
        assert_eq!(values["count"], json!(3.0));
        assert!(!values.contains_key("none"));
        assert_eq!(values["fruit"], json!("Pear"));
        assert_eq!(values["tags"], json!(["a", "b"]));
    }

    #[test]
    fn choice_values_are_strings_or_arrays() {
        assert_eq!(choice_value(false, &["Apple".to_string()]), json!("Apple"));
        assert_eq!(
            choice_value(true, &["Apple".to_string(), "Pear".to_string()]),
            json!(["Apple", "Pear"])
        );
    }

    #[test]
    fn chart_bounds_skip_missing_points_and_optionally_include_zero() {
        let series = vec![
            Series {
                name: "a".to_string(),
                values: vec![Some(2.0), None, Some(-1.0)],
            },
            Series {
                name: "b".to_string(),
                values: vec![None, Some(4.0)],
            },
        ];
        assert_eq!(chart_bounds(&series, false), Some((-1.0, 4.0)));
        assert_eq!(chart_bounds(&series, true), Some((-1.0, 4.0)));
        let positive = vec![Series {
            name: "p".to_string(),
            values: vec![Some(3.0), Some(5.0)],
        }];
        assert_eq!(chart_bounds(&positive, true), Some((0.0, 5.0)));
        assert_eq!(chart_bounds(&[], true), None);
    }

    #[test]
    fn chart_scale_pads_single_values_and_zero_ranges() {
        let flat = vec![Series {
            name: "flat".to_string(),
            values: vec![Some(5.0)],
        }];
        let (min, max) = chart_scale(&flat, false);
        assert!(min < 5.0);
        assert!(5.0 < max);
        let zeros = vec![Series {
            name: "zero".to_string(),
            values: vec![Some(0.0), Some(0.0)],
        }];
        let (min, max) = chart_scale(&zeros, true);
        assert_eq!(min, 0.0);
        assert!(max > 0.0);
    }

    #[test]
    fn chart_numbers_stay_compact() {
        assert_eq!(format_chart_number(0.0, false), "0");
        assert_eq!(format_chart_number(12.0, true), "12");
        assert_eq!(format_chart_number(12.5, false), "12.5");
        assert_eq!(format_chart_number(1_500.0, false), "1.5k");
        assert_eq!(format_chart_number(2_400_000.0, false), "2.4M");
    }

    fn button(label: &str) -> Widget {
        Widget::Button {
            label: label.to_string(),
            variant: ExtButtonVariant::Primary,
            value: None,
        }
    }

    #[test]
    fn a_row_measures_every_button_regardless_of_wrap() {
        egui::__run_test_ui(|ui| {
            let buttons = vec![button("Iniciar deploy"), button("Marcar sucesso")];
            let single = min_width(ui, &buttons[..1], theme::TEXT_MARKDOWN);
            let row = Widget::Row {
                wrap: false,
                children: buttons.clone(),
            };
            let row_width = min_width(ui, std::slice::from_ref(&row), theme::TEXT_MARKDOWN);
            assert!(row_width > single * 1.8);

            let wrapped = Widget::Row {
                wrap: true,
                children: buttons,
            };
            assert_eq!(
                min_width(ui, std::slice::from_ref(&wrapped), theme::TEXT_MARKDOWN),
                row_width
            );
        });
    }

    #[test]
    fn panel_width_grows_with_a_button_row() {
        egui::__run_test_ui(|ui| {
            let plain = vec![button("Zerar")];
            let row = vec![Widget::Row {
                wrap: false,
                children: vec![
                    button("Iniciar deploy"),
                    button("Marcar sucesso"),
                    button("Marcar falha"),
                    button("Zerar"),
                ],
            }];
            assert!(
                min_width(ui, &row, theme::TEXT_MARKDOWN)
                    > min_width(ui, &plain, theme::TEXT_MARKDOWN) * 3.0
            );
        });
    }

    #[test]
    fn a_button_row_reflows_at_the_panel_minimum_width() {
        egui::__run_test_ui(|ui| {
            let bounds = Rect::from_min_size(ui.min_rect().min, Vec2::new(300.0, 500.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bounds));
            let widgets = vec![Widget::Row {
                wrap: false,
                children: vec![
                    button("Iniciar deploy"),
                    button("Marcar sucesso"),
                    button("Marcar falha"),
                    button("Zerar"),
                ],
            }];
            show_widgets(
                &mut child,
                &widgets,
                &Palette::light(),
                theme::TEXT_MARKDOWN,
            );
            assert!(
                child.min_rect().width() <= 301.0,
                "row used {} points",
                child.min_rect().width()
            );
        });
    }

    #[test]
    fn image_paths_stay_inside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("a b.png");
        std::fs::write(&image, b"png").unwrap();
        let uri = image_uri("a b.png", Some(dir.path()));
        assert!(uri.starts_with("file://"));
        assert!(uri.ends_with("a%20b.png"));
        assert_eq!(image_uri("https://example.com/a.png", Some(dir.path())), "");
        assert_eq!(image_uri("/etc/hosts", Some(dir.path())), "");
        assert_eq!(image_uri("", None), "");
    }
}
