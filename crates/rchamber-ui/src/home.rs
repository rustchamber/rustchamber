use std::path::Path;
use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Galley, Id, Pos2, Rect, RichText, ScrollArea,
    Sense, Stroke, StrokeKind, Ui, Vec2,
};
use rchamber_app::{Action, Connection};

use crate::app::RustChamber;
use crate::composer;
use crate::icons::{self, Icon};
use crate::logo;
use crate::motion;
use crate::theme::{self, Palette};
use crate::widgets;

const COLUMN: f32 = 768.0;
const COLUMN_PAD_FRACTION: f32 = 0.025;
const COLUMN_PAD_MIN: f32 = 12.0;
const COLUMN_PAD_MAX: f32 = 16.0;
const COLUMN_MIN: f32 = 240.0;
const MD_WIDTH: f32 = 768.0;

const HEADING_SIZE: f32 = 24.0;
const HEADING_LINE: f32 = 32.0;
const HEADING_SIZE_MD: f32 = 30.0;
const HEADING_LINE_MD: f32 = 36.0;
const HEADING_TRACKING: f32 = -0.025;
const HEADING_MARGIN: f32 = 28.0;

const CHIP_HEIGHT: f32 = 24.0;
const CHIP_PAD_X: f32 = 10.0;
const CHIP_GAP: f32 = 6.0;
const CHIP_ICON: f32 = 12.0;
const CHIP_TEXT: f32 = theme::TEXT_META;
const CHIP_ROW_GAP: f32 = 6.0;
const CHIP_ICON_ALPHA: f32 = 0.7;
const CHIPS_MARGIN_TOP: f32 = 16.0;
const ADD_BUTTON: f32 = 24.0;
const ADD_ICON: f32 = 14.0;
const CHIP_REMOVE: f32 = 16.0;
const CHIP_REMOVE_ICON: f32 = 10.0;
const CHIP_REMOVE_OFFSET: f32 = 6.0;

const PICKER_WIDTH: f32 = 280.0;
const PICKER_MAX_HEIGHT: f32 = 320.0;
const PICKER_GAP: f32 = 6.0;
const PICKER_ID: &str = "home/starter-picker";

const EMPTY_LOGO: f32 = 140.0;
const EMPTY_LOGO_ALPHA: f32 = 0.2;
const EMPTY_GAP: f32 = 24.0;
const EMPTY_TEXT_GAP: f32 = 8.0;
const EMPTY_TEXT_WIDTH: f32 = 448.0;
const EMPTY_TEXT_PAD: f32 = 16.0;
const EMPTY_TITLE: f32 = 14.0;
const EMPTY_MESSAGE: f32 = 13.5;

const SUBMIT_GUARD: f64 = 0.6;

#[derive(Clone, Debug)]
struct Starter {
    label: String,
    icon: Icon,
    prompt: String,
    skill: bool,
    pin: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct BuiltinStarter {
    label: &'static str,
    icon: Icon,
    prompt: &'static str,
}

const BUILTIN_STARTERS: [BuiltinStarter; 8] = [
    BuiltinStarter {
        label: "Explore the codebase",
        icon: Icon::Folder,
        prompt: "/explore",
    },
    BuiltinStarter {
        label: "Catch me up",
        icon: Icon::Refresh,
        prompt: "/catch-up",
    },
    BuiltinStarter {
        label: "Weigh my options",
        icon: Icon::Diff,
        prompt: "/weigh",
    },
    BuiltinStarter {
        label: "Start feature planning",
        icon: Icon::Check,
        prompt: "/plan-feature",
    },
    BuiltinStarter {
        label: "Craft a Goal",
        icon: Icon::Spark,
        prompt: "/craft-goal",
    },
    BuiltinStarter {
        label: "Schedule a Task",
        icon: Icon::Clock,
        prompt: "/schedule-task",
    },
    BuiltinStarter {
        label: "Debug an issue",
        icon: Icon::Warning,
        prompt: "/debug",
    },
    BuiltinStarter {
        label: "Review my changes",
        icon: Icon::Search,
        prompt: "/workspace-review",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StarterSection {
    Command,
    Skill,
}

impl StarterSection {
    fn label(self) -> &'static str {
        match self {
            StarterSection::Command => "Commands",
            StarterSection::Skill => "Skills",
        }
    }
}

#[derive(Clone, Debug)]
struct PickerItem {
    label: String,
    prompt: String,
    icon: Icon,
    pin: String,
    section: StarterSection,
}

struct ChipLayout {
    widths: Vec<f32>,
    rows: Vec<Vec<usize>>,
}

impl ChipLayout {
    fn height(&self) -> f32 {
        if self.rows.is_empty() {
            return 0.0;
        }
        self.rows.len() as f32 * CHIP_HEIGHT + (self.rows.len() - 1) as f32 * CHIP_ROW_GAP
    }

    fn row_width(&self, row: &[usize]) -> f32 {
        let chips: f32 = row.iter().map(|index| self.widths[*index]).sum();
        chips + CHIP_GAP * (row.len().saturating_sub(1)) as f32
    }
}

struct Heading {
    before: Arc<Galley>,
    project: Option<Arc<Galley>>,
    after: Arc<Galley>,
    wrapped: Option<Arc<Galley>>,
    project_width: f32,
    line: f32,
}

impl Heading {
    fn height(&self) -> f32 {
        self.wrapped
            .as_ref()
            .map_or(self.line, |galley| galley.size().y)
    }

    fn paint(&self, ui: &Ui, p: &Palette, center_x: f32, top: f32) {
        if let Some(galley) = &self.wrapped {
            ui.painter()
                .galley(Pos2::new(center_x, top), galley.clone(), p.fg);
            return;
        }
        let project_width = match &self.project {
            Some(_) => motion::Motion::from_id(Id::unique("home/heading-project-width"))
                .with_duration(motion::LAYOUT)
                .with_easing(motion::EASE_LAYOUT)
                .value(ui.ctx(), self.project_width),
            None => 0.0,
        };
        let total = self.before.size().x + project_width + self.after.size().x;
        let left = center_x - total * 0.5;
        let painter = ui.painter();
        painter.galley(Pos2::new(left, top), self.before.clone(), p.fg);
        if let Some(project) = &self.project {
            let clip = Rect::from_min_size(
                Pos2::new(left + self.before.size().x, top),
                Vec2::new(project_width.max(0.0), self.line),
            );
            painter
                .with_clip_rect(clip)
                .galley(clip.min, project.clone(), p.fg);
        }
        painter.galley(
            Pos2::new(left + self.before.size().x + project_width, top),
            self.after.clone(),
            p.fg,
        );
    }
}

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let area = ui.max_rect();
    if area.width() < 40.0 || area.height() < 40.0 {
        return;
    }
    if matches!(app.model.connection, Connection::Failed(_)) {
        connection_state(ui, app, &p, area);
        return;
    }

    let starters = starter_set(app);
    let column = column_width(area.width());
    let layout = chip_layout(ui, &starters, column);
    let project = workspace_label(app);
    let heading = prepare_heading(
        ui,
        project.as_deref(),
        column,
        ui.ctx().content_rect().width(),
        &p,
    );
    let composer_height = composer::reserved_height(ui.ctx(), "home", true).max(96.0);
    let block =
        heading.height() + HEADING_MARGIN + composer_height + CHIPS_MARGIN_TOP + layout.height();
    let block_top = area.top() + ((area.height() - block) * 0.5).max(24.0);
    let composer_top = block_top + heading.height() + HEADING_MARGIN;
    let bounds = Rect::from_min_max(
        area.min,
        Pos2::new(
            area.right(),
            (composer_top + composer::FORM_PAD_BOTTOM + composer_height).min(area.bottom()),
        ),
    );
    composer::floating(ui, app, None, true, bounds);

    let composer_rect = ui
        .ctx()
        .memory(|memory| memory.area_rect(Id::unique(("composer-area", "home", true))))
        .unwrap_or_else(|| Rect::from_center_size(area.center(), Vec2::new(column, 0.0)));
    let center_x = composer_rect.center().x;

    let top = (composer_rect.top() - HEADING_MARGIN - heading.height()).max(area.top());
    heading.paint(ui, &p, center_x, top);

    let chips_rect = Rect::from_min_size(
        Pos2::new(
            center_x - column * 0.5,
            composer_rect.bottom() + CHIPS_MARGIN_TOP,
        ),
        Vec2::new(column, layout.height()),
    );
    chips(ui, app, &p, chips_rect, &starters, &layout);
}

fn starter_label(name: &str) -> String {
    let base: String = name
        .trim_start_matches('/')
        .chars()
        .map(|ch| if ch == '-' || ch == '_' { ' ' } else { ch })
        .collect();
    let base = base.split_whitespace().collect::<Vec<_>>().join(" ");
    match base.chars().next() {
        Some(first) => {
            let mut out = first.to_uppercase().collect::<String>();
            out.push_str(&base[first.len_utf8()..]);
            out
        }
        None => name.to_string(),
    }
}

fn builtin_starters() -> Vec<Starter> {
    BUILTIN_STARTERS
        .iter()
        .map(|starter| Starter {
            label: starter.label.to_owned(),
            icon: starter.icon,
            prompt: starter.prompt.to_owned(),
            skill: false,
            pin: None,
        })
        .collect()
}

fn merge_starters(base: &mut Vec<Starter>, extra: Vec<Starter>) {
    for starter in extra {
        if base
            .iter()
            .any(|existing| existing.prompt == starter.prompt)
        {
            continue;
        }
        base.push(starter);
    }
}

fn starter_set(app: &RustChamber) -> Vec<Starter> {
    let mut starters = builtin_starters();
    let pinned = pinned_starters(
        &app.settings.draft_starters,
        &app.model.commands,
        &app.model.skills,
    );
    merge_starters(&mut starters, pinned);
    starters
}

fn pinned_starters(
    pins: &[String],
    commands: &[rchamber_api::models::Command],
    skills: &[rchamber_api::models::Skill],
) -> Vec<Starter> {
    let mut starters: Vec<Starter> = Vec::new();
    for pin in pins {
        let Some(starter) = pinned_starter(pin, commands, skills) else {
            continue;
        };
        if starters
            .iter()
            .any(|existing| existing.prompt == starter.prompt)
        {
            continue;
        }
        starters.push(starter);
    }
    starters
}

fn pinned_starter(
    pin: &str,
    commands: &[rchamber_api::models::Command],
    skills: &[rchamber_api::models::Skill],
) -> Option<Starter> {
    let (kind, name) = pin.split_once(':')?;
    let item = match kind {
        "command" => {
            let command = commands.iter().find(|command| command.name == name)?;
            command_item(&command.name, command.source.as_deref())?
        }
        "skill" => {
            let skill = skills.iter().find(|skill| skill.name == name)?;
            skill_item(&skill.name)?
        }
        _ => return None,
    };
    if item.pin != pin {
        return None;
    }
    Some(Starter {
        label: item.label,
        icon: item.icon,
        prompt: item.prompt,
        skill: item.section == StarterSection::Skill,
        pin: Some(pin.to_owned()),
    })
}

fn pin_starter(app: &mut RustChamber, pin: &str) {
    if app.settings.draft_starters.iter().any(|entry| entry == pin) {
        return;
    }
    app.settings.draft_starters.push(pin.to_owned());
    app.settings.save();
}

fn unpin_starter(app: &mut RustChamber, pin: &str) {
    app.settings
        .draft_starters
        .retain(|entry| entry.as_str() != pin);
    app.settings.save();
}

fn workspace_label(app: &RustChamber) -> Option<String> {
    let path = app.model.workspace.as_deref()?.trim();
    if path.is_empty() {
        return None;
    }
    let name = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty());
    Some(name.unwrap_or_else(|| path.to_string()))
}

fn heading_text(project: Option<&str>) -> String {
    match project.filter(|label| !label.is_empty()) {
        Some(label) => format!("What are we working on in {label}?"),
        None => "What are we working on?".into(),
    }
}

fn heading_pieces<'a>(text: &'a str, label: &str) -> (&'a str, Option<&'a str>, &'a str) {
    if label.is_empty() {
        return (text, None, "");
    }
    match text.rfind(label) {
        Some(index) => (
            &text[..index],
            Some(&text[index..index + label.len()]),
            &text[index + label.len()..],
        ),
        None => (text, None, ""),
    }
}

fn heading_metrics(viewport: f32) -> (f32, f32) {
    if viewport >= MD_WIDTH {
        (HEADING_SIZE_MD, HEADING_LINE_MD)
    } else {
        (HEADING_SIZE, HEADING_LINE)
    }
}

fn heading_format(size: f32, line: f32, font: egui::FontId, p: &Palette) -> TextFormat {
    TextFormat {
        font_id: font,
        color: p.fg,
        extra_letter_spacing: size * HEADING_TRACKING,
        line_height: Some(line),
        ..Default::default()
    }
}

fn heading_part(
    ui: &Ui,
    text: &str,
    size: f32,
    line: f32,
    font: egui::FontId,
    p: &Palette,
) -> Arc<Galley> {
    if text.is_empty() {
        return ui
            .painter()
            .layout_no_wrap(String::new(), theme::font(size), p.fg);
    }
    let mut job = LayoutJob::default();
    job.append(text, 0.0, heading_format(size, line, font, p));
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

fn heading_wrapped(
    ui: &Ui,
    before: &str,
    project: Option<&str>,
    after: &str,
    size: f32,
    line: f32,
    width: f32,
    p: &Palette,
) -> Arc<Galley> {
    let mut job = LayoutJob {
        halign: Align::Center,
        wrap: egui::text::TextWrapping {
            max_width: width,
            ..Default::default()
        },
        ..LayoutJob::default()
    };
    if !before.is_empty() {
        job.append(
            before,
            0.0,
            heading_format(size, line, theme::font(size), p),
        );
    }
    if let Some(project) = project {
        job.append(
            project,
            0.0,
            heading_format(size, line, theme::font_medium(size), p),
        );
    }
    if !after.is_empty() {
        job.append(after, 0.0, heading_format(size, line, theme::font(size), p));
    }
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

fn prepare_heading(
    ui: &Ui,
    project: Option<&str>,
    width: f32,
    viewport: f32,
    p: &Palette,
) -> Heading {
    let (size, line) = heading_metrics(viewport);
    let text = heading_text(project);
    let label = project.filter(|label| !label.is_empty()).unwrap_or("");
    let (before_text, project_text, after_text) = heading_pieces(&text, label);
    let before = heading_part(ui, before_text, size, line, theme::font(size), p);
    let after = heading_part(ui, after_text, size, line, theme::font(size), p);
    let project_galley =
        project_text.map(|label| heading_part(ui, label, size, line, theme::font_medium(size), p));
    let project_width = project_galley
        .as_ref()
        .map_or(0.0, |galley| galley.size().x);
    let natural = before.size().x + project_width + after.size().x;
    let wrapped = if natural > width {
        Some(heading_wrapped(
            ui,
            before_text,
            project_text,
            after_text,
            size,
            line,
            width,
            p,
        ))
    } else {
        None
    };
    Heading {
        before,
        project: project_galley,
        after,
        wrapped,
        project_width,
        line,
    }
}

fn column_width(bounds: f32) -> f32 {
    let column = bounds.min(COLUMN);
    let pad = if bounds >= 1024.0 {
        (bounds * 0.03).clamp(16.0, 24.0)
    } else {
        (bounds * COLUMN_PAD_FRACTION).clamp(COLUMN_PAD_MIN, COLUMN_PAD_MAX)
    };
    (column - pad * 2.0).max(COLUMN_MIN)
}

fn chip_layout(ui: &Ui, starters: &[Starter], width: f32) -> ChipLayout {
    let mut widths: Vec<f32> = starters
        .iter()
        .map(|starter| {
            let galley = ui.painter().layout_no_wrap(
                starter.label.clone(),
                theme::font(CHIP_TEXT),
                Color32::PLACEHOLDER,
            );
            CHIP_PAD_X * 2.0 + CHIP_ICON + CHIP_GAP + galley.size().x
        })
        .collect();
    widths.push(ADD_BUTTON);
    let rows = wrap_rows(&widths, width);
    ChipLayout { widths, rows }
}

fn wrap_rows(widths: &[f32], max: f32) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut row: Vec<usize> = Vec::new();
    let mut used = 0.0;
    for (index, width) in widths.iter().copied().enumerate() {
        let advance = if row.is_empty() {
            width
        } else {
            width + CHIP_GAP
        };
        if !row.is_empty() && used + advance > max {
            rows.push(std::mem::take(&mut row));
            used = width;
        } else {
            used += advance;
        }
        row.push(index);
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn chips(
    ui: &Ui,
    app: &mut RustChamber,
    p: &Palette,
    rect: Rect,
    starters: &[Starter],
    layout: &ChipLayout,
) {
    let mut clicked: Option<usize> = None;
    let mut removed: Option<String> = None;
    let mut add = false;
    let mut add_response: Option<egui::Response> = None;
    for (row_index, row) in layout.rows.iter().enumerate() {
        let mut x = rect.center().x - layout.row_width(row) * 0.5;
        let y = rect.top() + row_index as f32 * (CHIP_HEIGHT + CHIP_ROW_GAP);
        for index in row {
            let width = layout.widths[*index];
            let chip_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, CHIP_HEIGHT));
            match starters.get(*index) {
                Some(starter) => {
                    let id = Id::unique(("home/starter-chip", starter.prompt.as_str()));
                    let hit = paint_chip(ui, chip_rect, starter, p, id);
                    if hit.submit {
                        clicked = Some(*index);
                    }
                    if hit.remove {
                        removed = starter.pin.clone();
                    }
                }
                None => {
                    let response = paint_add(ui, chip_rect, p);
                    if response.clicked() {
                        add = true;
                    }
                    add_response = Some(response);
                }
            }
            x += width + CHIP_GAP;
        }
    }

    let mut picker_open = ui
        .memory(|memory| memory.data.get_temp::<bool>(Id::unique(PICKER_ID)))
        .unwrap_or(false);
    if add {
        picker_open = !picker_open;
    }
    if picker_open && let Some(anchor) = add_response.as_ref() {
        starter_picker(ui, app, p, anchor, &mut picker_open);
    }
    ui.memory_mut(|memory| memory.data.insert_temp(Id::unique(PICKER_ID), picker_open));

    if let Some(pin) = removed {
        unpin_starter(app, &pin);
    }
    if let Some(index) = clicked {
        let starter = &starters[index];
        submit(ui, app, &starter.prompt, starter.skill);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ChipHit {
    submit: bool,
    remove: bool,
}

fn paint_chip(ui: &Ui, rect: Rect, starter: &Starter, p: &Palette, id: Id) -> ChipHit {
    let remove_rect = starter.pin.as_ref().map(|_| {
        Rect::from_min_size(
            Pos2::new(
                rect.right() + CHIP_REMOVE_OFFSET - CHIP_REMOVE,
                rect.top() - CHIP_REMOVE_OFFSET,
            ),
            Vec2::splat(CHIP_REMOVE),
        )
    });
    let response = ui.interact(rect, id, Sense::click());
    let remove_response =
        remove_rect.map(|remove| ui.interact(remove, id.with("remove"), Sense::click()));
    let remove_hovered = remove_response
        .as_ref()
        .is_some_and(|remove| remove.hovered());
    let t = motion::hover_fade(ui, id.with("hover"), response.hovered() || remove_hovered);
    let painter = ui.painter();
    let radius = CornerRadius::same(theme::radius::PILL);
    painter.rect_filled(rect, radius, p.card);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(theme::BORDER, p.border),
        StrokeKind::Inside,
    );
    let icon = Rect::from_center_size(
        Pos2::new(rect.left() + CHIP_PAD_X + CHIP_ICON * 0.5, rect.center().y),
        Vec2::splat(CHIP_ICON),
    );
    icons::paint(
        ui,
        starter.icon,
        icon,
        p.mix(p.fade(p.fg_muted, CHIP_ICON_ALPHA), p.fg, t),
    );
    painter.text(
        Pos2::new(
            rect.left() + CHIP_PAD_X + CHIP_ICON + CHIP_GAP,
            rect.center().y,
        ),
        Align2::LEFT_CENTER,
        &starter.label,
        theme::font(CHIP_TEXT),
        p.mix(p.fg_muted, p.fg, t),
    );
    let mut hit = ChipHit {
        submit: response.clicked(),
        remove: false,
    };
    if let (Some(remove), Some(remove_response)) = (remove_rect, remove_response) {
        let rt = motion::hover_fade(ui, id.with("remove-hover"), remove_response.hovered());
        if t > 0.01 {
            ui.painter().rect_filled(remove, radius, p.card);
            ui.painter().rect_stroke(
                remove,
                radius,
                Stroke::new(theme::BORDER, p.border),
                StrokeKind::Inside,
            );
            icons::paint(
                ui,
                Icon::Close,
                Rect::from_center_size(remove.center(), Vec2::splat(CHIP_REMOVE_ICON)),
                p.mix(p.fade(p.fg_muted, t), p.fg, rt),
            );
        }
        hit.submit &= !remove_response.clicked();
        hit.remove = remove_response.clicked();
        widgets::tooltip(remove_response, "Remove");
    }
    hit
}

fn paint_add(ui: &Ui, rect: Rect, p: &Palette) -> egui::Response {
    let id = Id::unique("home/add-starter");
    let response = ui.interact(rect, id, Sense::click());
    let t = motion::hover_fade(ui, id.with("hover"), response.hovered());
    let painter = ui.painter();
    let radius = CornerRadius::same(theme::radius::PILL);
    painter.rect_filled(rect, radius, p.card);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(theme::BORDER, p.border),
        StrokeKind::Inside,
    );
    icons::paint(
        ui,
        Icon::Plus,
        Rect::from_center_size(rect.center(), Vec2::splat(ADD_ICON)),
        p.mix(p.fg_muted, p.fg, t),
    );
    widgets::tooltip(response, "Add a starter")
}

fn command_item(name: &str, source: Option<&str>) -> Option<PickerItem> {
    let name = name.trim();
    if name.is_empty() || source == Some("skill") {
        return None;
    }
    let prompt = format!("/{name}");
    if BUILTIN_STARTERS
        .iter()
        .any(|starter| starter.prompt == prompt)
    {
        return None;
    }
    Some(PickerItem {
        label: starter_label(name),
        prompt,
        icon: Icon::Terminal,
        pin: format!("command:{name}"),
        section: StarterSection::Command,
    })
}

fn skill_item(name: &str) -> Option<PickerItem> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(PickerItem {
        label: starter_label(name),
        prompt: format!("${name}"),
        icon: Icon::File,
        pin: format!("skill:{name}"),
        section: StarterSection::Skill,
    })
}

fn picker_items(app: &RustChamber) -> Vec<PickerItem> {
    let mut items: Vec<PickerItem> = Vec::new();
    for command in &app.model.commands {
        let Some(item) = command_item(&command.name, command.source.as_deref()) else {
            continue;
        };
        if app
            .settings
            .draft_starters
            .iter()
            .any(|pin| pin == &item.pin)
        {
            continue;
        }
        items.push(item);
    }
    for skill in &app.model.skills {
        let Some(item) = skill_item(&skill.name) else {
            continue;
        };
        if app
            .settings
            .draft_starters
            .iter()
            .any(|pin| pin == &item.pin)
        {
            continue;
        }
        items.push(item);
    }
    items
}

fn starter_picker(
    ui: &Ui,
    app: &mut RustChamber,
    p: &Palette,
    anchor: &egui::Response,
    open: &mut bool,
) {
    let items = picker_items(app);
    let id = Id::unique(PICKER_ID);
    let Some(t) = motion::popup(ui.ctx(), id, *open) else {
        *open = false;
        return;
    };
    let mut held = true;
    let mut chosen: Option<usize> = None;
    let shown = egui::Popup::from_response(anchor)
        .id(id)
        .open_bool(&mut held)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(theme::popover_frame_t(p, t.opacity))
        .gap(PICKER_GAP)
        .align(egui::RectAlign::TOP_END)
        .show(|ui| {
            ui.set_opacity(t.opacity);
            ui.set_width(PICKER_WIDTH);
            ui.spacing_mut().item_spacing.y = 1.0;
            ScrollArea::vertical()
                .id_salt("home/starter-picker-scroll")
                .max_height(PICKER_MAX_HEIGHT)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if items.is_empty() {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("Nothing to add")
                                .font(theme::font(theme::TEXT_META))
                                .color(p.fg_muted),
                        );
                        return;
                    }
                    let mut section: Option<StarterSection> = None;
                    for (index, item) in items.iter().enumerate() {
                        if section != Some(item.section) {
                            if section.is_some() {
                                ui.add_space(6.0);
                            }
                            section = Some(item.section);
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(item.section.label())
                                    .font(theme::font_medium(11.0))
                                    .color(p.fg_muted),
                            );
                            ui.add_space(2.0);
                        }
                        if widgets::menu_item(ui, &item.label, None, None, p).clicked() {
                            chosen = Some(index);
                        }
                    }
                });
        });
    if let Some(inner) = &shown {
        let rect = inner.response.rect;
        let pivot = egui::pos2(rect.center().x, rect.top());
        let transform =
            egui::emath::TSTransform::new(pivot.to_vec2() * (1.0 - t.scale) + t.offset, t.scale);
        ui.ctx()
            .transform_layer_shapes_after_rounding(inner.response.layer_id, transform);
    }
    if let Some(index) = chosen {
        *open = false;
        pin_starter(app, &items[index].pin);
    } else if !held {
        *open = false;
    }
}

fn submit(ui: &Ui, app: &mut RustChamber, text: &str, skill: bool) {
    let now = ui.input(|input| input.time);
    let guard = Id::unique("home/starter-submitted-at");
    if let Some(at) = ui.ctx().memory(|memory| memory.data.get_temp::<f64>(guard))
        && now - at < SUBMIT_GUARD
    {
        return;
    }
    ui.ctx()
        .memory_mut(|memory| memory.data.insert_temp(guard, now));

    let draft = app.ui.home_draft.trim().to_string();
    let prompt = starter_prompt(text, &draft, skill);
    let model = composer::current_model(app);
    let agent = composer::current_agent(app);
    let files = app.model.attachments_for("home");
    app.backend.send(Action::CreateSession {
        directory: app.workspace_directory(),
        prompt: Some(prompt),
        files,
        model,
        agent,
    });
    app.ui.home_draft.clear();
    app.model.clear_attachments("home");
    app.model.save_draft();
}

fn starter_prompt(submit: &str, draft: &str, skill: bool) -> String {
    let draft = draft.trim();
    if draft.is_empty() {
        submit.to_string()
    } else if skill {
        format!("{submit}\n{draft}")
    } else {
        format!("{submit} {draft}")
    }
}

fn empty_galley(
    ui: &Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    wrap: Option<f32>,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    if let Some(width) = wrap {
        job.halign = Align::Center;
        job.wrap.max_width = width;
        job.wrap.break_anywhere = true;
    }
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

fn connection_state(ui: &Ui, app: &RustChamber, p: &Palette, area: Rect) {
    let Connection::Failed(reason) = app.model.connection.clone() else {
        return;
    };
    if area.width() < 80.0 || area.height() < 80.0 {
        return;
    }
    let width = (area.width() - EMPTY_TEXT_PAD * 2.0).clamp(160.0, EMPTY_TEXT_WIDTH);
    let title = empty_galley(
        ui,
        "OpenCode is not reachable",
        theme::font_medium(EMPTY_TITLE),
        p.red,
        None,
    );
    let message = empty_galley(
        ui,
        &reason,
        theme::font(EMPTY_MESSAGE),
        p.fg_muted,
        Some(width),
    );
    let chrome = EMPTY_GAP + title.size().y + EMPTY_TEXT_GAP + message.size().y;
    let available = area.height() - EMPTY_TEXT_PAD - chrome;
    let logo = if available >= 32.0 {
        available.min(EMPTY_LOGO)
    } else {
        0.0
    };
    let content = logo + chrome;
    let y = if content + EMPTY_TEXT_PAD <= area.height() {
        area.center().y
    } else {
        area.bottom() - EMPTY_TEXT_PAD - content * 0.5
    };
    let band = area;
    let popup = egui::Area::new(Id::unique("home/connection"))
        .order(egui::Order::Middle)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(Pos2::new(band.center().x, y))
        .constrain(false);
    popup.show(ui.ctx(), |ui| {
        ui.set_clip_rect(band);
        ui.set_width(width);
        ui.vertical_centered(|ui| {
            if logo > 0.0 {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(logo), Sense::hover());
                logo::paint(
                    ui.painter(),
                    rect.shrink(logo * 0.08),
                    p.fade(p.fg, EMPTY_LOGO_ALPHA),
                );
                ui.add_space(EMPTY_GAP);
            }
            let (rect, _) = ui.allocate_exact_size(title.size(), Sense::hover());
            ui.painter().galley(rect.min, title.clone(), p.red);
            ui.add_space(EMPTY_TEXT_GAP);
            let (rect, _) = ui.allocate_exact_size(message.size(), Sense::hover());
            ui.painter().galley(
                Pos2::new(rect.center().x, rect.top()),
                message.clone(),
                p.fg_muted,
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_names_the_workspace_when_present() {
        assert_eq!(
            heading_text(Some("rustchamber")),
            "What are we working on in rustchamber?"
        );
        assert_eq!(heading_text(None), "What are we working on?");
        assert_eq!(heading_text(Some("")), "What are we working on?");
    }

    #[test]
    fn heading_splits_the_project_segment() {
        let (before, project, after) =
            heading_pieces("What are we working on in rustchamber?", "rustchamber");
        assert_eq!(before, "What are we working on in ");
        assert_eq!(project, Some("rustchamber"));
        assert_eq!(after, "?");
        assert_eq!(
            heading_pieces("What are we working on?", ""),
            ("What are we working on?", None, "")
        );
    }

    #[test]
    fn heading_metrics_follow_the_md_breakpoint() {
        assert_eq!(heading_metrics(767.0), (24.0, 32.0));
        assert_eq!(heading_metrics(768.0), (30.0, 36.0));
        assert_eq!(heading_metrics(1600.0), (30.0, 36.0));
    }

    #[test]
    fn default_starters_match_the_reference_catalogue() {
        let pairs: Vec<(&str, &str)> = BUILTIN_STARTERS
            .iter()
            .map(|starter| (starter.label, starter.prompt))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("Explore the codebase", "/explore"),
                ("Catch me up", "/catch-up"),
                ("Weigh my options", "/weigh"),
                ("Start feature planning", "/plan-feature"),
                ("Craft a Goal", "/craft-goal"),
                ("Schedule a Task", "/schedule-task"),
                ("Debug an issue", "/debug"),
                ("Review my changes", "/workspace-review"),
            ]
        );
    }

    #[test]
    fn starter_labels_follow_the_slash_command_name() {
        assert_eq!(starter_label("/simplify-code"), "Simplify code");
        assert_eq!(starter_label("workspace_review"), "Workspace review");
        assert_eq!(starter_label("init"), "Init");
        assert_eq!(starter_label("/"), "/");
    }

    fn command(name: &str, source: Option<&str>) -> rchamber_api::models::Command {
        rchamber_api::models::Command {
            name: name.to_owned(),
            description: None,
            agent: None,
            model: None,
            template: None,
            source: source.map(str::to_owned),
        }
    }

    fn skill(name: &str) -> rchamber_api::models::Skill {
        rchamber_api::models::Skill {
            id: name.to_owned(),
            name: name.to_owned(),
            description: None,
            path: None,
            content: String::new(),
        }
    }

    #[test]
    fn picker_hides_built_ins_and_skill_commands() {
        assert!(command_item("explore", None).is_none());
        assert!(command_item("craft-goal", Some("command")).is_none());
        assert!(command_item("deploy", Some("skill")).is_none());
        assert!(command_item("  ", None).is_none());
        let item = command_item("deploy", Some("command")).unwrap();
        assert_eq!(item.label, "Deploy");
        assert_eq!(item.prompt, "/deploy");
        assert_eq!(item.pin, "command:deploy");
        assert_eq!(item.section, StarterSection::Command);
    }

    #[test]
    fn picker_skills_pin_with_the_skill_prefix() {
        let item = skill_item("code-review").unwrap();
        assert_eq!(item.label, "Code review");
        assert_eq!(item.prompt, "$code-review");
        assert_eq!(item.pin, "skill:code-review");
        assert_eq!(item.section, StarterSection::Skill);
        assert!(skill_item("").is_none());
    }

    #[test]
    fn pinned_starters_keep_the_stored_order_and_dedupe() {
        let commands = vec![
            command("deploy", Some("command")),
            command("explore", Some("command")),
        ];
        let skills = vec![skill("code-review")];
        let pins = vec![
            "skill:code-review".to_owned(),
            "command:deploy".to_owned(),
            "command:deploy".to_owned(),
            "command:explore".to_owned(),
            "command:missing".to_owned(),
            "bogus:value".to_owned(),
        ];
        let starters = pinned_starters(&pins, &commands, &skills);
        let prompts: Vec<&str> = starters.iter().map(|s| s.prompt.as_str()).collect();
        assert_eq!(prompts, vec!["$code-review", "/deploy"]);
        assert!(starters.iter().all(|starter| starter.pin.is_some()));
        assert!(
            starters
                .iter()
                .all(|starter| starter.skill == starter.prompt.starts_with('$'))
        );
    }

    #[test]
    fn pinned_starters_follow_the_built_ins() {
        let mut starters = builtin_starters();
        let pinned = vec![Starter {
            label: "Deploy".into(),
            icon: Icon::Terminal,
            prompt: "/deploy".into(),
            skill: false,
            pin: Some("command:deploy".into()),
        }];
        merge_starters(&mut starters, pinned.clone());
        merge_starters(&mut starters, pinned);
        let prompts: Vec<&str> = starters.iter().map(|s| s.prompt.as_str()).collect();
        assert_eq!(prompts.len(), 9);
        assert_eq!(prompts[0], "/explore");
        assert_eq!(prompts[8], "/deploy");
        assert!(starters[..8].iter().all(|starter| starter.pin.is_none()));
    }

    #[test]
    fn chips_wrap_with_the_reference_gap() {
        assert_eq!(wrap_rows(&[300.0, 300.0, 24.0], 640.0), vec![vec![0, 1, 2]]);
        assert_eq!(
            wrap_rows(&[300.0, 300.0, 24.0], 600.0),
            vec![vec![0], vec![1, 2]]
        );
        assert_eq!(wrap_rows(&[24.0], 640.0), vec![vec![0]]);
        assert_eq!(wrap_rows(&[], 640.0), Vec::<Vec<usize>>::new());
    }

    #[test]
    fn starter_prompt_keeps_the_draft_as_arguments() {
        assert_eq!(starter_prompt("/review", "", false), "/review");
        assert_eq!(starter_prompt("/review", " focus ", false), "/review focus");
        assert_eq!(
            starter_prompt("$skill-name", "explain this", true),
            "$skill-name\nexplain this"
        );
    }

    #[test]
    fn chip_row_width_adds_the_gap() {
        let layout = ChipLayout {
            widths: vec![100.0, 50.0, 24.0],
            rows: vec![vec![0, 1, 2]],
        };
        assert_eq!(layout.row_width(&[0, 1, 2]), 186.0);
        assert_eq!(layout.height(), 24.0);
    }

    #[test]
    fn column_matches_the_composer_width() {
        assert!((column_width(1440.0) - 720.0).abs() < 0.01);
        assert!((column_width(1320.0) - 720.0).abs() < 0.01);
        assert!((column_width(768.0) - 736.0).abs() < 0.01);
        assert!((column_width(480.0) - 456.0).abs() < 0.01);
    }
}
