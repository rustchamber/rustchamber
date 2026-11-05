use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, Galley, Label, Layout, Pos2, Rect, Response, ScrollArea, Sense,
    Stroke, StrokeKind, TextFormat, TextStyle, Ui, Vec2, WidgetText,
};
use egui_commonmark::CommonMarkCache;

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets;

const CODE_HEADER: f32 = theme::CODE_HEADER_HEIGHT;
const TABLE_MIN_COLUMN: f32 = 120.0;
const TABLE_MAX_COLUMN: f32 = 280.0;
const TABLE_PAD_X: f32 = 16.0;
const TABLE_PAD_Y: f32 = 10.0;
const TABLE_MIN_ROW: f32 = 36.0;
const QUOTE_BAR: f32 = 2.0;
const MARKER_WIDTH: f32 = theme::LIST_INDENT;
const MAX_INLINE_DEPTH: usize = 8;

#[derive(Default)]
pub struct Markdown {
    _cache: CommonMarkCache,
}

#[derive(Clone, Copy, Debug)]
struct InlineBase {
    size: f32,
    line_height: f32,
    color: Color32,
    bold: bool,
    italic: bool,
}

impl InlineBase {
    fn body(p: &Palette) -> Self {
        Self {
            size: theme::TEXT_MARKDOWN,
            line_height: theme::LINE_PROSE,
            color: p.body,
            bold: false,
            italic: false,
        }
    }

    fn meta(p: &Palette) -> Self {
        Self {
            size: theme::TEXT_META,
            line_height: theme::LINE_META,
            color: p.fg_muted,
            bold: false,
            italic: false,
        }
    }

    fn heading(p: &Palette, level: usize) -> Self {
        Self {
            size: match level {
                1 => 20.0,
                2 => 18.0,
                3 => 16.0,
                _ => theme::TEXT_MARKDOWN,
            },
            line_height: theme::heading_line_height(level),
            color: p.fade(p.fg, 0.95),
            bold: true,
            italic: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct InlineFlags {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
    depth: usize,
}

#[derive(Clone, Debug)]
struct LinkSpan {
    chars: std::ops::Range<usize>,
    url: String,
}

struct InlineBuilder<'a> {
    job: egui::text::LayoutJob,
    links: Vec<LinkSpan>,
    code: Vec<std::ops::Range<usize>>,
    p: &'a Palette,
    base: InlineBase,
}

impl<'a> InlineBuilder<'a> {
    fn new(p: &'a Palette, base: InlineBase) -> Self {
        Self {
            job: egui::text::LayoutJob::default(),
            links: Vec::new(),
            code: Vec::new(),
            p,
            base,
        }
    }

    fn push(&mut self, text: &str, flags: &InlineFlags) {
        if text.is_empty() {
            return;
        }
        let start = self.job.text.chars().count();
        let format = self.format(flags);
        self.job.append(text, 0.0, format);
        let end = start + text.chars().count();
        if let Some(url) = &flags.link {
            self.links.push(LinkSpan {
                chars: start..end,
                url: url.clone(),
            });
        }
        if flags.code {
            self.code.push(start..end);
        }
    }

    fn format(&self, flags: &InlineFlags) -> TextFormat {
        let p = self.p;
        let bold = flags.bold || self.base.bold;
        let italic = flags.italic || self.base.italic;
        let color = if flags.link.is_some() {
            p.markdown_link
        } else if flags.code {
            p.fade(p.inline_code, 0.9)
        } else if bold {
            p.bold
        } else if italic {
            p.italic
        } else {
            self.base.color
        };
        let font_id = if flags.code {
            theme::font_mono(theme::TEXT_CODE)
        } else if bold {
            theme::font_semibold(self.base.size)
        } else {
            theme::font(self.base.size)
        };
        TextFormat {
            font_id,
            color,
            background: Color32::TRANSPARENT,
            italics: italic,
            strikethrough: if flags.strike {
                Stroke::new(theme::BORDER, p.fg_muted)
            } else {
                Stroke::NONE
            },
            line_height: Some(self.base.line_height),
            ..Default::default()
        }
    }

    fn markup(&mut self, text: &str, flags: &InlineFlags) {
        if text.is_empty() {
            return;
        }
        if flags.depth > MAX_INLINE_DEPTH {
            self.push(text, flags);
            return;
        }
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut plain = 0;
        let mut i = 0;
        while i < len {
            match bytes[i] {
                b'`' => {
                    let mut ticks = 0;
                    while i + ticks < len && bytes[i + ticks] == b'`' {
                        ticks += 1;
                    }
                    let open = i + ticks;
                    let fence = "`".repeat(ticks);
                    if let Some(close) = find_from(text, &fence, open) {
                        self.push(&text[plain..i], flags);
                        let mut inner = flags.clone();
                        inner.code = true;
                        inner.bold = false;
                        inner.italic = false;
                        inner.depth += 1;
                        let code = text[open..close].trim();
                        self.push(code, &inner);
                        i = close + ticks;
                        plain = i;
                    } else {
                        i += ticks;
                    }
                }
                b'*' | b'_' => {
                    let marker = bytes[i];
                    let double = i + 1 < len && bytes[i + 1] == marker;
                    let needle = if double {
                        if marker == b'*' { "**" } else { "__" }
                    } else if marker == b'*' {
                        "*"
                    } else {
                        "_"
                    };
                    let start = i + needle.len();
                    if marker == b'_' {
                        let prev = text[..i].chars().next_back();
                        let after = text[start..].chars().next();
                        if prev.is_some_and(|c| c.is_alphanumeric())
                            || after.is_none_or(|c| c.is_whitespace())
                        {
                            i += needle.len();
                            continue;
                        }
                    }
                    if let Some(close) = find_from(text, needle, start) {
                        if close > start {
                            self.push(&text[plain..i], flags);
                            let mut inner = flags.clone();
                            if double {
                                inner.bold = true;
                            } else {
                                inner.italic = true;
                            }
                            inner.depth += 1;
                            self.markup(&text[start..close], &inner);
                            i = close + needle.len();
                            plain = i;
                        } else {
                            i += needle.len();
                        }
                    } else {
                        i += needle.len();
                    }
                }
                b'~' if i + 1 < len && bytes[i + 1] == b'~' => {
                    if let Some(close) = find_from(text, "~~", i + 2) {
                        if close > i + 2 {
                            self.push(&text[plain..i], flags);
                            let mut inner = flags.clone();
                            inner.strike = true;
                            inner.depth += 1;
                            self.markup(&text[i + 2..close], &inner);
                            i = close + 2;
                            plain = i;
                        } else {
                            i += 2;
                        }
                    } else {
                        i += 2;
                    }
                }
                b'[' => {
                    if let Some((label, url, end)) = parse_link(text, i) {
                        self.push(&text[plain..i], flags);
                        let mut inner = flags.clone();
                        inner.link = Some(url);
                        inner.depth += 1;
                        self.markup(label, &inner);
                        i = end;
                        plain = i;
                    } else {
                        i += 1;
                    }
                }
                b'\\' if i + 1 < len => {
                    self.push(&text[plain..i], flags);
                    let ch = text[i + 1..].chars().next().unwrap_or('\\');
                    self.push(&ch.to_string(), flags);
                    i += 1 + ch.len_utf8();
                    plain = i;
                }
                _ => {
                    i += 1;
                }
            }
        }
        self.push(&text[plain..], flags);
    }

    fn finish(
        mut self,
        width: f32,
    ) -> (
        egui::text::LayoutJob,
        Vec<LinkSpan>,
        Vec<std::ops::Range<usize>>,
    ) {
        self.job.wrap.max_width = width.max(16.0);
        self.job.wrap.max_rows = usize::MAX;
        (self.job, self.links, self.code)
    }
}

fn find_from(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if from > haystack.len() {
        return None;
    }
    haystack.get(from..)?.find(needle).map(|index| index + from)
}

fn parse_link(text: &str, start: usize) -> Option<(&str, String, usize)> {
    let rest = text.get(start + 1..)?;
    let close = rest.find(']')?;
    let label = &rest[..close];
    let after = &rest[close + 1..];
    if !after.starts_with('(') {
        return None;
    }
    let url_end = after.find(')')?;
    let mut url = after[1..url_end].trim().to_string();
    if let Some((target, _)) = url.split_once(char::is_whitespace) {
        url = target.to_string();
    }
    let end = start + 1 + close + 1 + url_end + 1;
    Some((label, url, end))
}

impl Markdown {
    pub fn show(&mut self, ui: &mut Ui, text: &str, p: &Palette) {
        let id = ui.next_auto_id();
        self.show_at(ui, id, text, p);
    }

    pub fn show_at(&mut self, ui: &mut Ui, id: egui::Id, text: &str, p: &Palette) {
        self.show_sized(ui, id, text, InlineBase::body(p), p);
    }

    pub fn show_meta(&mut self, ui: &mut Ui, id: egui::Id, text: &str, p: &Palette) {
        self.show_sized(ui, id, text, InlineBase::meta(p), p);
    }

    pub fn show_at_sized(
        &mut self,
        ui: &mut Ui,
        id: egui::Id,
        text: &str,
        size: f32,
        line_height: f32,
        color: Color32,
        p: &Palette,
    ) {
        let base = InlineBase {
            size,
            line_height,
            color,
            bold: false,
            italic: false,
        };
        self.show_sized(ui, id, text, base, p);
    }

    fn show_sized(&mut self, ui: &mut Ui, id: egui::Id, text: &str, base: InlineBase, p: &Palette) {
        if text.trim().is_empty() {
            return;
        }
        let rect = ui.max_rect();
        if !rect.top().is_finite() || !rect.bottom().is_finite() || !rect.width().is_finite() {
            ui.ctx().request_repaint();
            return;
        }
        ui.push_id(id, |ui| {
            let mut style: egui::Style = (*ui.style()).as_ref().clone();
            style.visuals.hyperlink_color = p.markdown_link;
            style.visuals.code_bg_color = p.code_bg;
            style.visuals.extreme_bg_color = p.code_bg;
            style.visuals.override_text_color = Some(base.color);
            style.visuals.weak_text_color = Some(p.blockquote);
            style.spacing.item_spacing = egui::vec2(0.0, 0.0);
            style
                .text_styles
                .insert(TextStyle::Body, theme::font(base.size));
            style
                .text_styles
                .insert(TextStyle::Monospace, theme::font_mono(theme::TEXT_CODE));
            ui.scope(|ui| {
                ui.set_style(style);
                let lines: Vec<&str> = text.split('\n').collect();
                render_blocks(ui, &lines, p, base, 0);
            });
        });
    }

    pub fn show_plain(&mut self, ui: &mut Ui, text: &str, p: &Palette) {
        self.show(ui, text, p);
    }
}

fn render_blocks(ui: &mut Ui, lines: &[&str], p: &Palette, base: InlineBase, depth: usize) -> bool {
    let mut index = 0;
    let mut rendered = false;
    let mut gap = 0.0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            index += 1;
            continue;
        }
        if let Some((fence, lang)) = fence_start(line) {
            if rendered {
                ui.add_space(gap);
            }
            index = code_block(ui, lines, index, fence, &lang, p, base);
            gap = theme::PARAGRAPH_GAP;
            rendered = true;
            continue;
        }
        if index + 1 < lines.len()
            && let Some(level) = setext_underline(lines[index + 1])
            && !is_block_start(line)
        {
            if rendered {
                ui.add_space(theme::HEADING_TOP);
            }
            heading(ui, level, line.trim(), p);
            gap = theme::PARAGRAPH_GAP;
            rendered = true;
            index += 2;
            continue;
        }
        if let Some(level) = heading_start(line) {
            if rendered {
                ui.add_space(theme::HEADING_TOP);
            }
            heading(ui, level, line.trim(), p);
            gap = theme::PARAGRAPH_GAP;
            rendered = true;
            index += 1;
            continue;
        }
        if is_hrule(line) {
            if rendered {
                ui.add_space(gap);
            }
            hrule(ui, p);
            gap = theme::PARAGRAPH_GAP;
            rendered = true;
            index += 1;
            continue;
        }
        if line.trim_start().starts_with('>') {
            if rendered {
                ui.add_space(gap);
            }
            let (next, inner) = collect_quote(lines, index);
            blockquote(ui, &inner, p, base, depth);
            gap = theme::PARAGRAPH_GAP;
            rendered = true;
            index = next;
            continue;
        }
        if is_table(lines, index) {
            if rendered {
                ui.add_space(gap);
            }
            index = table(ui, lines, index, p, base);
            gap = theme::PARAGRAPH_GAP;
            rendered = true;
            continue;
        }
        if let Some((alt, url)) = standalone_image(line) {
            if rendered {
                ui.add_space(gap);
            }
            image_block(ui, &alt, &url);
            gap = theme::PARAGRAPH_GAP;
            rendered = true;
            index += 1;
            continue;
        }
        if list_marker(line).is_some() {
            if rendered {
                ui.add_space(gap);
            }
            index = list(ui, lines, index, p, base, depth);
            gap = theme::PARAGRAPH_GAP;
            rendered = true;
            continue;
        }
        if rendered {
            ui.add_space(gap);
        }
        let mut end = index;
        let mut paragraph: Vec<&str> = Vec::new();
        while end < lines.len() && !lines[end].trim().is_empty() {
            if end > index && is_block_start(lines[end]) {
                break;
            }
            if end > index && is_table(lines, end) {
                break;
            }
            paragraph.push(lines[end]);
            end += 1;
        }
        paragraph_view(ui, &paragraph, p, base);
        gap = theme::PARAGRAPH_GAP;
        rendered = true;
        index = end.max(index + 1);
    }
    rendered
}

fn paragraph_view(ui: &mut Ui, lines: &[&str], p: &Palette, base: InlineBase) {
    let text = join_lines(lines);
    if text.trim().is_empty() {
        return;
    }
    inline_view(ui, &text, p, base);
}

fn join_lines(lines: &[&str]) -> String {
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            let hard = lines[index - 1].ends_with("  ");
            text.push(if hard { '\n' } else { ' ' });
        }
        text.push_str(line.trim());
    }
    text
}

fn inline_view(ui: &mut Ui, text: &str, p: &Palette, base: InlineBase) -> Response {
    let width = ui.available_width();
    let mut builder = InlineBuilder::new(p, base);
    builder.markup(text, &InlineFlags::default());
    place_inline(ui, builder, width, p).0
}

fn heading(ui: &mut Ui, level: usize, text: &str, p: &Palette) {
    let base = InlineBase::heading(p, level);
    let width = ui.available_width();
    let text = text.trim_start_matches('#').trim();
    let mut builder = InlineBuilder::new(p, base);
    builder.markup(text, &InlineFlags::default());
    place_inline(ui, builder, width, p);
}

fn paint_code_chips(
    ui: &Ui,
    origin: Pos2,
    galley: &Galley,
    spans: &[std::ops::Range<usize>],
    p: &Palette,
) {
    if spans.is_empty() {
        return;
    }
    let fill = p.inline_code_chip();
    let mut char_index = 0usize;
    for row in &galley.rows {
        let mut segment: Option<(f32, f32)> = None;
        let mut segments: Vec<(f32, f32)> = Vec::new();
        let mut row_top = f32::INFINITY;
        let mut row_height = row.row.size.y;
        for glyph in &row.row.glyphs {
            let inside = spans.iter().any(|span| span.contains(&char_index));
            char_index += 1;
            if !inside {
                if let Some((start, end)) = segment.take() {
                    segments.push((start, end));
                }
                continue;
            }
            let left = row.pos.x + glyph.pos.x;
            let right = left + glyph.advance_width;
            row_top = row_top.min(row.pos.y + glyph.pos.y - glyph.font_ascent);
            row_height = row_height.min(glyph.line_height);
            match &mut segment {
                Some((_, end)) => *end = right,
                None => segment = Some((left, right)),
            }
        }
        if let Some((start, end)) = segment.take() {
            segments.push((start, end));
        }
        if segments.is_empty() || !row_top.is_finite() {
            continue;
        }
        for (start, end) in segments {
            let rect = Rect::from_min_max(
                Pos2::new(
                    origin.x + start - theme::INLINE_CODE_PAD_X,
                    origin.y + row_top - theme::INLINE_CODE_PAD_Y,
                ),
                Pos2::new(
                    origin.x + end + theme::INLINE_CODE_PAD_X,
                    origin.y + row_top + row_height + theme::INLINE_CODE_PAD_Y,
                ),
            );
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::radius::INLINE_CODE), fill);
        }
    }
}

fn place_inline(
    ui: &mut Ui,
    builder: InlineBuilder,
    width: f32,
    p: &Palette,
) -> (Response, Arc<Galley>, Vec<LinkSpan>) {
    let (job, links, code) = builder.finish(width);
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let size = galley.size();
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    paint_code_chips(ui, rect.min, &galley, &code, p);
    let response = ui.put(
        rect,
        Label::new(WidgetText::Galley(galley.clone())).selectable(true),
    );
    link_interaction(ui, &response, &galley, &links);
    (response, galley, links)
}

fn hrule(ui: &mut Ui, p: &Palette) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, theme::BORDER), Sense::hover());
    ui.painter().hline(
        rect.left()..=rect.right(),
        rect.center().y,
        Stroke::new(theme::HAIRLINE, p.hr),
    );
}

fn blockquote(ui: &mut Ui, lines: &[&str], p: &Palette, base: InlineBase, depth: usize) {
    let bar_x = ui.cursor().left();
    let quote_base = InlineBase {
        color: p.blockquote,
        ..base
    };
    let inner = ui.horizontal_top(|ui| {
        ui.add_space(theme::QUOTE_PAD);
        ui.vertical(|ui| {
            render_blocks(ui, lines, p, quote_base, depth + 1);
        });
    });
    let rect = inner.response.rect;
    if rect.height() > 2.0 {
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(bar_x, rect.top() + 1.0),
                Pos2::new(bar_x + QUOTE_BAR, rect.bottom() - 1.0),
            ),
            CornerRadius::same(1),
            p.blockquote_border,
        );
    }
}

fn collect_quote<'a>(lines: &[&'a str], start: usize) -> (usize, Vec<&'a str>) {
    let mut collected: Vec<&str> = Vec::new();
    let mut index = start;
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if let Some(rest) = trimmed.strip_prefix('>') {
            collected.push(rest.strip_prefix(' ').unwrap_or(rest));
            index += 1;
        } else if trimmed.is_empty() {
            let mut look = index + 1;
            while look < lines.len() && lines[look].trim().is_empty() {
                look += 1;
            }
            if look < lines.len() && lines[look].trim_start().starts_with('>') {
                collected.push("");
                index += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    (index, collected)
}

fn list(
    ui: &mut Ui,
    lines: &[&str],
    start: usize,
    p: &Palette,
    base: InlineBase,
    depth: usize,
) -> usize {
    let Some((base_indent, ordered, mut number, _)) = list_marker(lines[start]) else {
        return start + 1;
    };
    let mut index = start;
    let mut first = true;
    while index < lines.len() {
        if lines[index].trim().is_empty() {
            index += 1;
            continue;
        }
        let Some((indent, item_ordered, item_number, content)) = list_marker(lines[index]) else {
            break;
        };
        let (content, task) = if item_ordered {
            (content, None)
        } else {
            task_content(content)
        };
        if indent + 1 < base_indent {
            break;
        }
        if indent > base_indent + 1 {
            index = list(ui, lines, index, p, base, depth + 1);
            continue;
        }
        if item_ordered != ordered {
            break;
        }
        if !first {
            ui.add_space(theme::LIST_GAP);
        }
        let mut item_lines: Vec<&str> = vec![content];
        let mut next = index + 1;
        while next < lines.len() {
            let candidate = lines[next];
            if candidate.trim().is_empty() {
                let mut look = next + 1;
                while look < lines.len() && lines[look].trim().is_empty() {
                    look += 1;
                }
                if look < lines.len() {
                    let cont_indent = indent_of(lines[look]);
                    if cont_indent > base_indent || list_marker(lines[look]).is_some() {
                        item_lines.push("");
                        next += 1;
                        continue;
                    }
                }
                break;
            }
            if let Some((indent, _, _, _)) = list_marker(candidate) {
                if indent > base_indent {
                    item_lines.push(candidate.trim_start());
                    next += 1;
                    continue;
                }
                break;
            }
            if indent_of(candidate) > base_indent {
                item_lines.push(candidate.trim_start());
                next += 1;
                continue;
            }
            if !is_block_start(candidate) {
                item_lines.push(candidate.trim_start());
                next += 1;
                continue;
            }
            break;
        }
        let marker = if ordered {
            format!("{}.", if item_number > 0 { item_number } else { number })
        } else {
            match depth % 3 {
                0 => "•".to_string(),
                1 => "◦".to_string(),
                _ => "▪".to_string(),
            }
        };
        let marker_color = p.list_marker;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(MARKER_WIDTH, base.line_height), Sense::hover());
            if let Some(done) = task {
                let box_rect = Rect::from_center_size(
                    Pos2::new(rect.left() + 9.0, rect.center().y),
                    Vec2::splat(12.0),
                );
                let fill = if done {
                    p.primary
                } else {
                    Color32::TRANSPARENT
                };
                let border = if done { p.primary } else { p.edge_strong() };
                ui.painter()
                    .rect_filled(box_rect, CornerRadius::same(theme::radius::XS), fill);
                ui.painter().rect_stroke(
                    box_rect,
                    CornerRadius::same(theme::radius::XS),
                    Stroke::new(theme::HAIRLINE, border),
                    StrokeKind::Inside,
                );
                if done {
                    icons::paint(ui, Icon::Check, box_rect.shrink(2.0), p.on_primary);
                }
            } else {
                ui.painter().text(
                    Pos2::new(rect.left() + 3.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    marker,
                    theme::font(base.size),
                    marker_color,
                );
            }
            ui.vertical(|ui| {
                render_blocks(ui, &item_lines, p, base, depth + 1);
            });
        });
        number += 1;
        first = false;
        index = next;
    }
    index
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn task_content(content: &str) -> (&str, Option<bool>) {
    if let Some(rest) = content.strip_prefix("[ ] ") {
        (rest, Some(false))
    } else if let Some(rest) = content
        .strip_prefix("[x] ")
        .or_else(|| content.strip_prefix("[X] "))
    {
        (rest, Some(true))
    } else if content == "[ ]" {
        ("", Some(false))
    } else if content == "[x]" || content == "[X]" {
        ("", Some(true))
    } else {
        (content, None)
    }
}

fn standalone_image(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("![") {
        return None;
    }
    let (alt, url, end) = parse_link(trimmed, 1)?;
    if end != trimmed.len() {
        return None;
    }
    Some((alt.to_string(), url))
}

fn image_block(ui: &mut Ui, alt: &str, url: &str) {
    let uri = if url.contains("://") || url.starts_with("data:") {
        url.to_string()
    } else {
        format!("file://{url}")
    };
    let width = ui.available_width().max(16.0);
    let image = egui::Image::from_uri(uri)
        .max_width(width)
        .max_height(400.0)
        .maintain_aspect_ratio(true)
        .alt_text(alt);
    ui.add(image);
}

fn list_marker(line: &str) -> Option<(usize, bool, usize, &str)> {
    let indent = indent_of(line);
    let rest = &line[indent..];
    for bullet in ['-', '*', '+'] {
        if let Some(after) = rest.strip_prefix(bullet) {
            if after.is_empty() {
                return Some((indent, false, 0, ""));
            }
            if after.starts_with(' ') {
                return Some((indent, false, 0, after.trim_start()));
            }
        }
    }
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 9 {
        let after = &rest[digits.len()..];
        if let Some(content) = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))
        {
            let number = digits.parse().unwrap_or(1);
            return Some((indent, true, number, content));
        }
        if after == "." || after == ")" {
            return Some((indent, true, digits.parse().unwrap_or(1), ""));
        }
    }
    None
}

fn fence_start(line: &str) -> Option<(char, String)> {
    let trimmed = line.trim_start();
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let ticks = trimmed.chars().take_while(|c| *c == ch).count();
    if ticks < 3 {
        return None;
    }
    let info = trimmed[ticks..].trim();
    let language = info.split_whitespace().next().unwrap_or("").to_string();
    Some((ch, language))
}

fn heading_start(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    if rest.is_empty() || rest.starts_with(' ') {
        Some(level)
    } else {
        None
    }
}

fn setext_underline(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().all(|c| c == '=') {
        return Some(1);
    }
    if trimmed.chars().all(|c| c == '-') && trimmed.chars().count() >= 3 {
        return Some(2);
    }
    None
}

fn is_hrule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let mut marker = None;
    let mut count = 0;
    for ch in trimmed.chars() {
        if ch == ' ' {
            continue;
        }
        if !matches!(ch, '-' | '*' | '_') {
            return false;
        }
        match marker {
            None => marker = Some(ch),
            Some(existing) if existing == ch => {}
            _ => return false,
        }
        count += 1;
    }
    count >= 3
}

fn is_block_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    fence_start(line).is_some()
        || heading_start(line).is_some()
        || is_hrule(line)
        || trimmed.starts_with('>')
        || list_marker(line).is_some()
        || standalone_image(line).is_some()
}

fn is_table(lines: &[&str], index: usize) -> bool {
    if index + 1 >= lines.len() {
        return false;
    }
    if !lines[index].contains('|') || !lines[index + 1].contains('|') {
        return false;
    }
    let cells = split_row(lines[index + 1]);
    if cells.is_empty() {
        return false;
    }
    cells.iter().all(|cell| {
        let cell = cell.trim();
        let core = cell.trim_matches(':');
        !core.is_empty() && core.chars().all(|c| c == '-') && !core.is_empty()
    })
}

fn split_row(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    trimmed
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn alignment(line: &str) -> Vec<bool> {
    split_row(line)
        .iter()
        .map(|cell| {
            let cell = cell.trim();
            cell.ends_with(':')
        })
        .collect()
}

fn table(ui: &mut Ui, lines: &[&str], start: usize, p: &Palette, base: InlineBase) -> usize {
    let header = split_row(lines[start]);
    let aligns = alignment(lines[start + 1]);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut index = start + 2;
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() || !line.contains('|') {
            break;
        }
        if is_block_start(line) {
            break;
        }
        rows.push(split_row(line));
        index += 1;
    }
    let columns = header.len().max(1);
    let available = ui.available_width();
    let column_width =
        ((available - 2.0) / columns as f32).clamp(TABLE_MIN_COLUMN, TABLE_MAX_COLUMN);
    let total = column_width * columns as f32;
    let scroll = total > available + 0.5;
    if scroll {
        let salt = ui.next_auto_id();
        ScrollArea::horizontal().id_salt(salt).show(ui, |ui| {
            paint_table(
                ui,
                &header,
                &rows,
                &aligns,
                p,
                base,
                column_width,
                total,
                columns,
            );
        });
    } else {
        paint_table(
            ui,
            &header,
            &rows,
            &aligns,
            p,
            base,
            column_width,
            total,
            columns,
        );
    }
    index
}

struct TableCell {
    galley: Arc<Galley>,
    links: Vec<LinkSpan>,
    code: Vec<std::ops::Range<usize>>,
}

fn table_cell(ui: &mut Ui, text: &str, p: &Palette, base: InlineBase, width: f32) -> TableCell {
    let mut builder = InlineBuilder::new(p, base);
    builder.markup(text, &InlineFlags::default());
    let (job, links, code) = builder.finish((width - TABLE_PAD_X * 2.0).max(16.0));
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    TableCell {
        galley,
        links,
        code,
    }
}

fn paint_table(
    ui: &mut Ui,
    header: &[String],
    rows: &[Vec<String>],
    aligns: &[bool],
    p: &Palette,
    base: InlineBase,
    column_width: f32,
    total: f32,
    columns: usize,
) {
    let head_base = InlineBase {
        color: p.fg,
        ..base
    };
    let body_base = InlineBase {
        color: p.body,
        ..base
    };
    let head_cells: Vec<TableCell> = (0..columns)
        .map(|column| {
            let text = header.get(column).map(String::as_str).unwrap_or("");
            table_cell(ui, text, p, head_base, column_width)
        })
        .collect();
    let head_height = head_cells
        .iter()
        .map(|cell| cell.galley.size().y + TABLE_PAD_Y * 2.0)
        .fold(0.0_f32, f32::max)
        .max(TABLE_MIN_ROW);
    let body_cells: Vec<Vec<TableCell>> = rows
        .iter()
        .map(|row| {
            (0..columns)
                .map(|column| {
                    let text = row.get(column).map(String::as_str).unwrap_or("");
                    table_cell(ui, text, p, body_base, column_width)
                })
                .collect()
        })
        .collect();
    let body_heights: Vec<f32> = body_cells
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.galley.size().y + TABLE_PAD_Y * 2.0)
                .fold(0.0_f32, f32::max)
                .max(TABLE_MIN_ROW)
        })
        .collect();
    let table_height = head_height + body_heights.iter().sum::<f32>();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total, table_height), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(theme::radius::TABLE), p.card);
    painter.rect_stroke(
        rect,
        CornerRadius::same(theme::radius::TABLE),
        Stroke::new(theme::BORDER, p.fade(p.border, 0.8)),
        StrokeKind::Inside,
    );
    let rule = p.fade(p.border, 0.6);
    let mut y = rect.top() + head_height;
    painter.hline(
        rect.left()..=rect.right(),
        y,
        Stroke::new(theme::BORDER, rule),
    );
    for height in body_heights
        .iter()
        .take(body_heights.len().saturating_sub(1))
    {
        y += height;
        painter.hline(
            rect.left()..=rect.right(),
            y,
            Stroke::new(theme::BORDER, rule),
        );
    }
    for column in 1..columns {
        let x = rect.left() + column_width * column as f32;
        painter.vline(
            x,
            rect.top()..=rect.bottom(),
            Stroke::new(theme::BORDER, rule),
        );
    }
    let mut y = rect.top();
    for (column, cell) in head_cells.iter().enumerate() {
        let cell_rect = Rect::from_min_size(
            Pos2::new(
                rect.left() + column_width * column as f32 + TABLE_PAD_X,
                y + TABLE_PAD_Y,
            ),
            Vec2::new(column_width - TABLE_PAD_X * 2.0, cell.galley.size().y),
        );
        place_cell(
            ui,
            cell,
            cell_rect,
            aligns.get(column).copied().unwrap_or(false),
            p,
        );
    }
    y += head_height;
    for (row, cells) in body_cells.iter().enumerate() {
        let height = body_heights[row];
        for (column, cell) in cells.iter().enumerate() {
            let cell_rect = Rect::from_min_size(
                Pos2::new(
                    rect.left() + column_width * column as f32 + TABLE_PAD_X,
                    y + TABLE_PAD_Y,
                ),
                Vec2::new(column_width - TABLE_PAD_X * 2.0, cell.galley.size().y),
            );
            place_cell(
                ui,
                cell,
                cell_rect,
                aligns.get(column).copied().unwrap_or(false),
                p,
            );
        }
        y += height;
    }
}

fn place_cell(ui: &mut Ui, cell: &TableCell, rect: Rect, align_right: bool, p: &Palette) {
    let galley = cell.galley.clone();
    let place = if align_right {
        Rect::from_min_size(
            Pos2::new(rect.right() - galley.size().x, rect.top()),
            galley.size(),
        )
    } else {
        Rect::from_min_size(rect.min, galley.size())
    };
    paint_code_chips(ui, place.min, &galley, &cell.code, p);
    let response = ui.put(
        place,
        Label::new(WidgetText::Galley(galley.clone())).selectable(true),
    );
    link_interaction(ui, &response, &galley, &cell.links);
}

fn code_block(
    ui: &mut Ui,
    lines: &[&str],
    start: usize,
    fence: char,
    lang: &str,
    p: &Palette,
    base: InlineBase,
) -> usize {
    let mut index = start + 1;
    let mut code = String::new();
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        let ticks = trimmed.chars().take_while(|c| *c == fence).count();
        if ticks >= 3 && trimmed[ticks..].trim().is_empty() {
            index += 1;
            break;
        }
        if !code.is_empty() {
            code.push('\n');
        }
        code.push_str(lines[index]);
        index += 1;
    }
    let width = ui.available_width();
    let frame = egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(theme::BORDER, p.fade(p.border, 0.8)))
        .corner_radius(CornerRadius::same(theme::radius::CODE_BLOCK));
    frame.show(ui, |ui| {
        ui.set_width((width - 2.0).max(16.0));
        let inner_width = (width - 2.0).max(16.0);
        let (header_rect, _) =
            ui.allocate_exact_size(Vec2::new(inner_width, CODE_HEADER), Sense::hover());
        if !lang.is_empty() {
            ui.painter().text(
                Pos2::new(
                    header_rect.left() + theme::CODE_HEADER_PAD_X,
                    header_rect.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                lang,
                theme::font(theme::TEXT_CODE_HEADER),
                p.fg_muted,
            );
        }
        let action_rect = Rect::from_center_size(
            Pos2::new(header_rect.right() - 18.0, header_rect.center().y),
            Vec2::splat(widgets::HIT_XS),
        );
        let mut action_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(action_rect)
                .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
        );
        widgets::copy_button(&mut action_ui, &code, p);
        ui.painter().hline(
            header_rect.left()..=header_rect.right(),
            header_rect.bottom(),
            Stroke::new(theme::BORDER, p.fade(p.border, 0.7)),
        );
        let text = if code.is_empty() {
            " ".to_string()
        } else {
            code.clone()
        };
        let mut job = egui::text::LayoutJob::default();
        job.append(
            &text,
            0.0,
            TextFormat {
                font_id: theme::font_mono(theme::TEXT_CODE),
                color: base.color,
                line_height: Some(theme::LINE_CODE),
                ..Default::default()
            },
        );
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        let salt = ui.next_auto_id();
        ScrollArea::horizontal()
            .id_salt(salt)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let body_size = Vec2::new(
                    galley.size().x + theme::CODE_PAD_X * 2.0,
                    galley.size().y + theme::CODE_PAD_Y * 2.0,
                );
                let (body_rect, _) = ui.allocate_exact_size(body_size, Sense::hover());
                let galley_rect = Rect::from_min_size(
                    Pos2::new(
                        body_rect.left() + theme::CODE_PAD_X,
                        body_rect.top() + theme::CODE_PAD_Y,
                    ),
                    galley.size(),
                );
                ui.put(
                    galley_rect,
                    Label::new(WidgetText::Galley(galley.clone())).selectable(true),
                );
            });
    });
    index
}

fn link_interaction(ui: &Ui, response: &Response, galley: &Galley, links: &[LinkSpan]) {
    if links.is_empty() {
        return;
    }
    let Some(position) = response.hover_pos() else {
        return;
    };
    let cursor = galley.cursor_from_pos(position - response.rect.min);
    let index = cursor.index.0;
    let Some(link) = links.iter().find(|link| link.chars.contains(&index)) else {
        return;
    };
    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    if response.clicked() {
        open_link(ui.ctx(), &link.url);
    }
}

fn open_link(ctx: &egui::Context, url: &str) {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
        ctx.open_url(egui::OpenUrl::new_tab(url));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bullets_and_ordered_lists_parse() {
        assert_eq!(list_marker("- item").unwrap().0, 0);
        assert!(!list_marker("- item").unwrap().1);
        assert_eq!(list_marker("  * item").unwrap().0, 2);
        let ordered = list_marker("12. step").unwrap();
        assert!(ordered.1);
        assert_eq!(ordered.2, 12);
        assert_eq!(ordered.3, "step");
        assert!(list_marker("---").is_none());
        assert!(list_marker("plain").is_none());
    }

    #[test]
    fn fenced_blocks_parse() {
        assert_eq!(fence_start("```rust title=x").unwrap().1, "rust");
        assert_eq!(fence_start("   ~~~").unwrap().0, '~');
        assert!(fence_start("``").is_none());
    }

    #[test]
    fn tables_need_a_delimiter_row() {
        let lines = ["| a | b |", "| --- | :-: |", "| 1 | 2 |"];
        assert!(is_table(&lines, 0));
        let plain = ["| a | b |", "| 1 | 2 |"];
        assert!(!is_table(&plain, 0));
    }

    #[test]
    fn table_rows_split_and_trim() {
        let cells = split_row("| alpha | beta |");
        assert_eq!(cells, vec!["alpha", "beta"]);
    }

    #[test]
    fn links_parse_to_label_and_target() {
        let (label, url, end) = parse_link("see [docs](https://example.com/a) now", 4).unwrap();
        assert_eq!(label, "docs");
        assert_eq!(url, "https://example.com/a");
        assert_eq!(end, 33);
    }

    #[test]
    fn setext_and_rules_are_distinguished() {
        assert_eq!(setext_underline("==="), Some(1));
        assert_eq!(setext_underline("---"), Some(2));
        assert!(is_hrule("***"));
        assert!(is_hrule("- - -"));
        assert!(!is_hrule("--"));
    }

    #[test]
    fn headings_require_a_space() {
        assert_eq!(heading_start("## Title"), Some(2));
        assert_eq!(heading_start("##Title"), None);
        assert_eq!(heading_start("####### Title"), None);
    }

    #[test]
    fn heading_line_heights_match_the_reference() {
        assert_eq!(theme::heading_line_height(1), 26.0);
        assert_eq!(theme::heading_line_height(2), 23.4);
        assert_eq!(theme::heading_line_height(3), 20.8);
        assert_eq!(theme::heading_line_height(4), 18.2);
    }

    #[test]
    fn inline_builder_tracks_link_ranges() {
        let palette = Palette::dark();
        let mut builder = InlineBuilder::new(&palette, InlineBase::body(&palette));
        builder.markup(
            "get **bold** and [docs](https://example.com)",
            &InlineFlags::default(),
        );
        let links = builder.links.clone();
        let (job, _, _) = builder.finish(400.0);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://example.com");
        let text = &job.text;
        let chars: Vec<char> = text.chars().collect();
        let linked: String = chars[links[0].chars.clone()].iter().collect();
        assert_eq!(linked, "docs");
        assert!(text.contains("bold"));
    }

    #[test]
    fn inline_builder_tracks_code_ranges() {
        let palette = Palette::light();
        let mut builder = InlineBuilder::new(&palette, InlineBase::body(&palette));
        builder.markup("a `code` b", &InlineFlags::default());
        let (job, _, code) = builder.finish(400.0);
        assert_eq!(code.len(), 1);
        let chars: Vec<char> = job.text.chars().collect();
        let text: String = chars[code[0].clone()].iter().collect();
        assert_eq!(text, "code");
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;

    const SAMPLE: &str = "# Findings\n\nA paragraph with **bold**, *italic*, `inline code` and [a link](https://example.com).\n\n- first item\n- second item with a longer sentence that should wrap inside the column when the width is narrow\n  - nested item\n\n1. one\n2. two\n\n> A quote that wraps across its own column and keeps its bar.\n\n| Name | Value | Note |\n| --- | --- | --- |\n| alpha | 1 | a long cell value that needs to wrap inside the table column |\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n---\n\nAfter the rule.";

    #[test]
    fn document_renders_without_panic() {
        let palette = Palette::dark();
        egui::__run_test_ui(|ui| {
            ui.set_width(560.0);
            let mut md = Markdown::default();
            md.show(ui, SAMPLE, &palette);
            assert!(ui.min_rect().height() > 200.0);
        });
    }

    #[test]
    fn narrow_document_renders_without_panic() {
        let palette = Palette::light();
        egui::__run_test_ui(|ui| {
            ui.set_width(260.0);
            let mut md = Markdown::default();
            md.show_at(ui, egui::Id::unique("narrow"), SAMPLE, &palette);
            assert!(ui.min_rect().height() > 200.0);
        });
    }
}
