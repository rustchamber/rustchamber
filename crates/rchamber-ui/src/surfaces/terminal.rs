use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Event, EventFilter, FontId, Id, Key, Modifiers, Pos2,
    Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use rchamber_term::{Cell, Color};

use crate::app::RustChamber;
use crate::icons::Icon;
use crate::surfaces;
use crate::theme::{self, Palette};
const FONT_SIZE: f32 = 12.0;
const TAB_LABEL_MAX_CHARS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selection {
    anchor: (usize, usize),
    cursor: (usize, usize),
}

struct Tab {
    id: u64,
    title: String,
    terminal: Option<rchamber_term::Terminal>,
    error: Option<String>,
    selection: Option<Selection>,
    focus: bool,
    paste_pending: bool,
}

impl Tab {
    fn new(id: u64, title: String) -> Self {
        Self {
            id,
            title,
            terminal: None,
            error: None,
            selection: None,
            focus: false,
            paste_pending: false,
        }
    }

    fn label(&self, max: usize) -> String {
        let count = self.title.chars().count();
        if count <= max {
            self.title.clone()
        } else {
            let mut value: String = self.title.chars().take(max.saturating_sub(3)).collect();
            value.push_str("...");
            value
        }
    }

    fn selected_text(&self) -> Option<String> {
        let selection = self.selection?;
        let terminal = self.terminal.as_ref()?;
        let text = terminal
            .emulator
            .text_in_range(selection.anchor, selection.cursor);
        (!text.is_empty()).then_some(text)
    }

    fn visible_text(&self) -> Option<String> {
        let terminal = self.terminal.as_ref()?;
        let text = terminal.emulator.visible_text();
        (!text.is_empty()).then_some(text)
    }

    fn copy_text(&self) -> Option<String> {
        self.selected_text().or_else(|| self.visible_text())
    }
}

struct TerminalState {
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    rename: Option<(u64, String)>,
    rename_focus: bool,
    cwd: String,
    close_panel: bool,
}

impl Default for TerminalState {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            rename: None,
            rename_focus: false,
            cwd: String::new(),
            close_panel: false,
        }
    }
}

impl TerminalState {
    fn handle(ctx: &egui::Context, cwd: &str) -> Arc<Mutex<Self>> {
        ctx.data_mut(|data| {
            data.get_temp_mut_or_insert_with(Id::unique(("terminal-surface-state", cwd)), || {
                Arc::new(Mutex::new(Self::default()))
            })
            .clone()
        })
    }

    fn new_tab(&mut self) {
        let number = self.next_id;
        self.next_id += 1;
        self.tabs
            .push(Tab::new(number, format!("Terminal {number}")));
        self.active = self.tabs.len() - 1;
        self.tabs[self.active].focus = true;
    }

    fn ensure_tab(&mut self) {
        if self.tabs.is_empty() {
            self.new_tab();
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if let Some((id, _)) = self.rename.as_ref()
            && *id == self.tabs[index].id
        {
            self.rename = None;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.close_panel = true;
            return;
        }
        if self.active > index {
            self.active -= 1;
        } else if self.active == index {
            self.active = index.min(self.tabs.len() - 1);
            self.tabs[self.active].focus = true;
        }
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let index = self.active.min(self.tabs.len().saturating_sub(1));
        self.tabs.get_mut(index)
    }

    fn restart_active(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            tab.terminal = None;
            tab.error = None;
            tab.selection = None;
            tab.focus = true;
        }
    }

    fn copy_active(&self, ctx: &egui::Context) {
        if let Some(tab) = self.tabs.get(self.active)
            && let Some(text) = tab.copy_text()
        {
            ctx.copy_text(text);
        }
    }

    fn paste_active(&mut self, ctx: &egui::Context) {
        if let Some(tab) = self.active_tab_mut() {
            tab.paste_pending = true;
            tab.focus = true;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }

    fn kill_active(&mut self) {
        if let Some(tab) = self.active_tab_mut()
            && let Some(terminal) = tab.terminal.as_mut()
        {
            terminal.kill();
        }
    }

    fn pump(&mut self, ctx: &egui::Context) {
        for tab in &mut self.tabs {
            let Some(terminal) = tab.terminal.as_mut() else {
                continue;
            };
            let before = terminal.emulator.history_len();
            let changed = terminal.pump();
            let after = terminal.emulator.history_len();
            if after > before {
                let shift = after - before;
                if let Some(selection) = &mut tab.selection {
                    selection.anchor.0 += shift;
                    selection.cursor.0 += shift;
                }
            } else if after < before {
                tab.selection = None;
            }
            if changed {
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        }
    }

    fn header(&mut self, ui: &mut Ui, p: &Palette, expanded: bool) -> bool {
        let mut new_tab = false;
        let mut restart = false;
        let mut copy = false;
        let mut paste = false;
        let mut kill = false;
        let mut close_panel = false;
        let mut toggle_expanded = false;
        let has_selection = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.selection)
            .is_some();

        let tabs: Vec<surfaces::PanelTab> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                surfaces::PanelTab::new(tab.label(TAB_LABEL_MAX_CHARS), Icon::Terminal)
                    .active(index == self.active)
                    .closable(true)
            })
            .collect();

        let header = surfaces::panel_header(
            ui,
            p,
            &tabs,
            |_| {},
            |ui| {
                if surfaces::close_button(ui, p).clicked() {
                    close_panel = true;
                }
                if surfaces::expand_button(ui, p, expanded).clicked() {
                    toggle_expanded = true;
                }
                if surfaces::panel_button(ui, p, Icon::Stop, "Kill process").clicked() {
                    kill = true;
                }
                if surfaces::panel_button(ui, p, Icon::Paperclip, "Paste").clicked() {
                    paste = true;
                }
                if surfaces::panel_button(
                    ui,
                    p,
                    Icon::Copy,
                    if has_selection {
                        "Copy selection"
                    } else {
                        "Copy output"
                    },
                )
                .clicked()
                {
                    copy = true;
                }
                if surfaces::panel_action(ui, p, Icon::Refresh, true, "Restart shell").clicked() {
                    restart = true;
                }
                if surfaces::panel_button(ui, p, Icon::Plus, "New terminal").clicked() {
                    new_tab = true;
                }
            },
        );

        if let Some(index) = header.select
            && index < self.tabs.len()
        {
            self.active = index;
            self.tabs[index].focus = true;
        }
        if let Some(index) = header.close {
            self.close_tab(index);
        }
        if let Some(index) = header.double
            && let Some(tab) = self.tabs.get(index)
        {
            let id = tab.id;
            let title = tab.title.clone();
            self.rename = Some((id, title));
            self.rename_focus = true;
            self.tabs[index].focus = false;
        }
        if let Some((from, to)) = header.reorder {
            let active_id = self.tabs.get(self.active).map(|tab| tab.id);
            surfaces::move_item(&mut self.tabs, from, to);
            if let Some(id) = active_id
                && let Some(index) = self.tabs.iter().position(|tab| tab.id == id)
            {
                self.active = index;
            }
        }

        self.rename_row(ui, p);

        if close_panel {
            self.close_panel = true;
        }
        if new_tab {
            self.new_tab();
        }
        if restart {
            self.restart_active();
        }
        if copy {
            self.copy_active(ui.ctx());
        }
        if paste {
            self.paste_active(ui.ctx());
        }
        if kill {
            self.kill_active();
        }
        if (copy || kill)
            && let Some(tab) = self.active_tab_mut()
        {
            tab.focus = true;
        }
        toggle_expanded
    }

    fn rename_row(&mut self, ui: &mut Ui, p: &Palette) {
        let Some((id, mut name)) = self.rename.take() else {
            return;
        };
        let focus = self.rename_focus;
        self.rename_focus = false;
        let mut commit = false;
        let mut cancel = false;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, p.bg_soft);
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    egui::Pos2::new(rect.left() + 12.0, rect.top() + 4.0),
                    egui::Pos2::new(rect.right() - 12.0, rect.bottom() - 4.0),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.spacing_mut().item_spacing.x = 6.0;
        let width = (row.available_width() - 150.0).max(90.0);
        let response = row.add_sized(
            Vec2::new(width, 28.0),
            egui::TextEdit::singleline(&mut name)
                .id(Id::unique(("terminal-rename", id)))
                .font(theme::font(13.0))
                .text_color(p.fg)
                .margin(egui::Margin::symmetric(8, 5))
                .hint_text(
                    RichText::new("Tab name")
                        .font(theme::font(13.0))
                        .color(p.fg_muted),
                ),
        );
        if focus {
            response.request_focus();
        }
        if ui.input(|input| input.key_pressed(Key::Escape)) {
            cancel = true;
        } else if ui.input(|input| input.key_pressed(Key::Enter)) || response.lost_focus() {
            commit = true;
        }
        if surfaces::panel_button(&mut row, p, Icon::Check, "Rename tab").clicked() {
            commit = true;
        }
        if surfaces::panel_button(&mut row, p, Icon::Close, "Cancel").clicked() {
            cancel = true;
        }
        if cancel {
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                tab.focus = true;
            }
            self.rename = None;
        } else if commit {
            let name = name.trim().to_string();
            if !name.is_empty()
                && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id)
            {
                tab.title = name;
            }
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                tab.focus = true;
            }
            self.rename = None;
        } else {
            self.rename = Some((id, name));
        }
    }
    fn view(&mut self, ui: &mut Ui, p: &Palette, ctx: &egui::Context) {
        self.ensure_tab();
        let index = self.active.min(self.tabs.len() - 1);
        self.active = index;
        let cwd = self.cwd.clone();
        let tab = &mut self.tabs[index];
        render_tab(tab, ui, p, ctx, &cwd);
    }
}

pub fn has_focus(ctx: &egui::Context) -> bool {
    let cwd = ctx
        .data(|data| data.get_temp::<String>(Id::unique("terminal-current-directory")))
        .unwrap_or_default();
    let handle = ctx.data(|data| {
        data.get_temp::<Arc<Mutex<TerminalState>>>(Id::unique(("terminal-surface-state", cwd)))
    });
    let Some(handle) = handle else {
        return false;
    };
    let state = handle.lock().unwrap_or_else(|error| error.into_inner());
    let Some(tab) = state.tabs.get(state.active) else {
        return false;
    };
    ctx.memory(|memory| memory.has_focus(Id::unique(("terminal-input", tab.id))))
}

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let palette = app.palette.clone();
    let cwd = app.workspace_directory();
    let ctx = ui.ctx().clone();
    ctx.data_mut(|data| data.insert_temp(Id::unique("terminal-current-directory"), cwd.clone()));
    let handle = TerminalState::handle(&ctx, &cwd);
    let mut state = handle.lock().unwrap_or_else(|error| error.into_inner());
    if state.cwd != cwd {
        state.cwd = cwd;
        state.tabs.clear();
        state.active = 0;
        state.rename = None;
    }
    if state.tabs.is_empty() {
        state.ensure_tab();
    }
    let toggle_expanded = state.header(ui, &palette, app.ui.expanded);
    if toggle_expanded {
        app.ui.expanded = !app.ui.expanded;
    }
    if state.close_panel {
        state.close_panel = false;
        app.ui.context_open = false;
        return;
    }
    state.pump(&ctx);
    state.view(ui, &palette, &ctx);
}

fn render_tab(tab: &mut Tab, ui: &mut Ui, p: &Palette, ctx: &egui::Context, cwd: &str) {
    let font = theme::font_mono(FONT_SIZE);
    let probe = ui
        .painter()
        .layout_no_wrap("M".to_string(), font.clone(), p.fg);
    let cell_width = probe.size().x.max(5.0);
    let cell_height = (probe.size().y + 2.0).ceil().max(12.0);

    let available = ui.available_size();
    let (body, _) = ui.allocate_exact_size(
        Vec2::new(available.x.max(120.0), available.y.max(60.0)),
        Sense::hover(),
    );
    ui.painter().rect_filled(body, 0.0, p.bg);
    let inner = Rect::from_min_max(
        Pos2::new(body.left() + 16.0, body.top() + 12.0),
        Pos2::new(body.right() - 6.0, body.bottom() - 16.0),
    );

    let columns = (((inner.width() - 8.0) / cell_width).floor() as usize).max(20);
    let rows = ((inner.height() / cell_height).floor() as usize).max(5);

    if tab.terminal.is_none() && tab.error.is_none() {
        let wake_ctx = ctx.clone();
        let wake: rchamber_term::Wake =
            Arc::new(move || wake_ctx.request_repaint_after(Duration::from_millis(33)));
        match rchamber_term::Terminal::spawn_with(
            Path::new(cwd),
            tab.title.clone(),
            columns,
            rows,
            wake,
        ) {
            Ok(terminal) => {
                tab.terminal = Some(terminal);
            }
            Err(error) => tab.error = Some(error.to_string()),
        }
    }

    if let Some(terminal) = tab.terminal.as_mut() {
        terminal.resize(columns, rows);
    }

    let focus_id = Id::unique(("terminal-input", tab.id));
    let response = ui.interact(inner, focus_id, Sense::click_and_drag());
    if tab.focus {
        response.request_focus();
        tab.focus = false;
    }
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    let focused = response.has_focus();
    if focused {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                focus_id,
                EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
    }

    if let Some(terminal) = tab.terminal.as_mut() {
        if response.hovered() {
            let delta = ui.input(|input| input.smooth_scroll_delta.y);
            if delta.abs() > 0.5 {
                let steps = (delta / cell_height).round() as isize;
                let steps = if steps == 0 {
                    delta.signum() as isize
                } else {
                    steps
                };
                terminal.emulator.scroll_by(steps);
            }
        }
        let max_offset = terminal.emulator.history_len();
        if max_offset > 0 {
            let track = Rect::from_min_max(
                Pos2::new(inner.right() - 5.0, inner.top()),
                Pos2::new(inner.right(), inner.bottom()),
            );
            let scrollbar = ui.interact(
                track,
                Id::unique(("terminal-scrollbar", tab.id)),
                Sense::click_and_drag(),
            );
            if (scrollbar.dragged() || scrollbar.clicked())
                && let Some(pointer) = scrollbar.interact_pointer_pos()
            {
                let fraction =
                    ((pointer.y - inner.top()) / inner.height().max(1.0)).clamp(0.0, 1.0);
                let offset = ((1.0 - fraction) * max_offset as f32).round() as usize;
                terminal.emulator.set_view_offset(offset);
            }
        }
    }

    let view_top = {
        let terminal = tab.terminal.as_ref();
        terminal
            .map(|terminal| {
                let emulator = &terminal.emulator;
                emulator
                    .total_lines()
                    .saturating_sub(emulator.rows() + emulator.view_offset())
            })
            .unwrap_or(0)
    };

    if let Some(terminal) = tab.terminal.as_mut() {
        if response.drag_started() {
            let origin = ui.input(|input| input.pointer.press_origin());
            if let Some(position) = origin.or_else(|| response.interact_pointer_pos())
                && position.x < inner.right() - 7.0
            {
                let cell = pointer_cell(
                    position,
                    inner,
                    cell_width,
                    cell_height,
                    view_top,
                    terminal.emulator.cols(),
                    terminal.emulator.rows(),
                );
                tab.selection = Some(Selection {
                    anchor: cell,
                    cursor: cell,
                });
            }
        }
        if response.dragged()
            && let Some(position) = response.interact_pointer_pos()
            && position.x < inner.right() - 7.0
        {
            let cell = pointer_cell(
                position,
                inner,
                cell_width,
                cell_height,
                view_top,
                terminal.emulator.cols(),
                terminal.emulator.rows(),
            );
            if let Some(selection) = &mut tab.selection {
                selection.cursor = cell;
            } else {
                tab.selection = Some(Selection {
                    anchor: cell,
                    cursor: cell,
                });
            }
        }
        if response.clicked() {
            tab.selection = None;
        }
    }

    if focused || tab.paste_pending {
        handle_input(ui, tab, focused);
    }

    if let Some(error) = tab.error.clone() {
        ui.painter().rect_filled(body, 0.0, p.bg);
        let overlay =
            Rect::from_center_size(inner.center(), Vec2::new(inner.width().min(420.0), 160.0));
        ui.painter().text(
            Pos2::new(overlay.center().x, overlay.center().y - 34.0),
            Align2::CENTER_CENTER,
            "Terminal couldn't connect",
            theme::font_semibold(13.5),
            p.status_text(p.red),
        );
        let galley = surfaces::truncated_galley(
            ui,
            error,
            theme::font(12.0),
            p.fg_muted,
            overlay.width() - 24.0,
        );
        ui.painter().galley(
            Pos2::new(
                overlay.center().x - galley.size().x * 0.5,
                overlay.center().y - 12.0,
            ),
            galley,
            p.fg_muted,
        );
        let button_rect = Rect::from_center_size(
            Pos2::new(overlay.center().x, overlay.center().y + 30.0),
            Vec2::new(96.0, 28.0),
        );
        let retry = ui.interact(
            button_rect,
            Id::unique(("terminal-retry", tab.id)),
            Sense::click(),
        );
        ui.painter().rect_filled(
            button_rect,
            CornerRadius::same(theme::radius::MD),
            if retry.hovered() { p.hover } else { p.card },
        );
        ui.painter().rect_stroke(
            button_rect,
            CornerRadius::same(theme::radius::MD),
            Stroke::new(0.5, surfaces::edge(p)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            button_rect.center(),
            Align2::CENTER_CENTER,
            "Try again",
            theme::font_medium(13.0),
            p.fg,
        );
        if retry.on_hover_text("Restart the shell").clicked() {
            tab.error = None;
            tab.terminal = None;
            tab.focus = true;
        }
        return;
    }

    let Some(terminal) = tab.terminal.as_ref() else {
        return;
    };
    let emulator = &terminal.emulator;
    let selection = tab.selection;
    let origin = Pos2::new(inner.left(), inner.top());
    let painter = ui.painter().with_clip_rect(inner);

    for row in 0..emulator.rows() {
        let line = emulator.line(view_top + row);
        let top = origin.y + row as f32 * cell_height;
        let bottom = top + cell_height;
        let at = |column: usize| -> Cell {
            line.and_then(|cells| cells.get(column))
                .copied()
                .unwrap_or_default()
        };

        let mut column = 0;
        while column < emulator.cols() {
            let background = effective_background(&at(column), p);
            if let Some(color) = background {
                let start = column;
                column += 1;
                while column < emulator.cols()
                    && effective_background(&at(column), p) == Some(color)
                {
                    column += 1;
                }
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(origin.x + start as f32 * cell_width, top),
                        Pos2::new(origin.x + column as f32 * cell_width, bottom),
                    ),
                    CornerRadius::ZERO,
                    color,
                );
            } else {
                column += 1;
            }
        }

        if let Some((start, end)) = selection_columns(selection, view_top + row, emulator.cols()) {
            let color = p.selection;
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(origin.x + start as f32 * cell_width, top),
                    Pos2::new(origin.x + end as f32 * cell_width, bottom),
                ),
                CornerRadius::ZERO,
                color,
            );
        }

        let mut run = String::new();
        let mut run_start = 0;
        let mut run_key: Option<(Color32, bool, bool)> = None;
        for column in 0..=emulator.cols() {
            let key = if column < emulator.cols() {
                let cell = at(column);
                Some((effective_foreground(&cell, p), cell.bold, cell.underline))
            } else {
                None
            };
            if key != run_key {
                if let Some((color, bold, underline)) = run_key {
                    paint_run(
                        &painter,
                        origin,
                        cell_width,
                        cell_height,
                        top,
                        run_start,
                        &run,
                        color,
                        bold,
                        underline,
                        &font,
                    );
                }
                run.clear();
                run_start = column;
                run_key = key;
            }
            if column < emulator.cols() {
                run.push(at(column).ch);
            }
        }
    }

    if emulator.cursor_visible()
        && emulator.view_offset() == 0
        && emulator.cursor_line() >= view_top
        && emulator.cursor_line() < view_top + emulator.rows()
    {
        let row = emulator.cursor_line() - view_top;
        let column = emulator.cursor_col().min(emulator.cols() - 1);
        let top = origin.y + row as f32 * cell_height;
        let rect = Rect::from_min_max(
            Pos2::new(origin.x + column as f32 * cell_width, top),
            Pos2::new(
                origin.x + (column + 1) as f32 * cell_width,
                top + cell_height,
            ),
        );
        if focused {
            painter.rect_filled(rect, CornerRadius::same(2), p.primary);
            let cell = emulator
                .line(emulator.cursor_line())
                .and_then(|cells| cells.get(column))
                .copied()
                .unwrap_or_default();
            if cell.ch != ' ' {
                let galley = painter.layout_no_wrap(cell.ch.to_string(), font.clone(), p.code_bg);
                painter.galley(
                    Pos2::new(rect.left(), top + (cell_height - galley.size().y) * 0.5),
                    galley,
                    p.code_bg,
                );
            }
        } else {
            painter.rect_stroke(
                rect,
                CornerRadius::same(2),
                Stroke::new(1.0, p.fg_faint),
                StrokeKind::Inside,
            );
        }
    }

    let history = emulator.history_len();
    if history > 0 {
        let track = Rect::from_min_max(
            Pos2::new(inner.right() - 5.0, inner.top()),
            Pos2::new(inner.right(), inner.bottom()),
        );
        painter.rect_filled(
            track,
            CornerRadius::same(3),
            p.border_soft.gamma_multiply(0.7),
        );
        let total = emulator.total_lines().max(1);
        let visible = emulator.rows().min(total);
        let thumb_height = (track.height() * visible as f32 / total as f32)
            .max(24.0)
            .min(track.height());
        let max_top = total.saturating_sub(visible).max(1);
        let progress = view_top as f32 / max_top as f32;
        let y = track.top() + (track.height() - thumb_height) * progress;
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(track.left(), y),
                Vec2::new(track.width(), thumb_height),
            ),
            CornerRadius::same(3),
            p.fg_faint.gamma_multiply(0.85),
        );
    }

    let status = if let Some(code) = terminal.exit_code() {
        Some(format!("process exited ({code})"))
    } else if terminal.killed() {
        Some("process stopped".to_string())
    } else {
        None
    };
    if let Some(status) = status {
        painter.text(
            Pos2::new(inner.right() - 12.0, inner.bottom() - 4.0),
            Align2::RIGHT_BOTTOM,
            status,
            theme::font(10.5),
            p.fg_faint,
        );
    }
}

fn handle_input(ui: &mut Ui, tab: &mut Tab, focused: bool) {
    let Some(terminal) = tab.terminal.as_mut() else {
        tab.paste_pending = false;
        return;
    };
    let modifiers = ui.input(|input| input.modifiers);
    let events = ui.input(|input| input.events.clone());
    let selection = tab.selection;
    let mut bytes = Vec::new();
    let mut copy_text = None;
    let mut paste_text = None;

    for event in &events {
        match event {
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if !focused {
                    continue;
                }
                if let Some(encoded) =
                    encode_key(*key, *modifiers, terminal.emulator.application_cursor())
                {
                    bytes.extend(encoded);
                }
            }
            Event::Text(text) => {
                if focused && !modifiers.ctrl && !modifiers.command && !modifiers.alt {
                    bytes.extend_from_slice(text.as_bytes());
                }
            }
            Event::Paste(text) => {
                if focused || tab.paste_pending {
                    paste_text = Some(text.clone());
                }
            }
            Event::Copy if focused => {
                if let Some(selection) = selection {
                    let text = terminal
                        .emulator
                        .text_in_range(selection.anchor, selection.cursor);
                    if text.is_empty() {
                        bytes.push(0x03);
                    } else {
                        copy_text = Some(text);
                    }
                } else {
                    bytes.push(0x03);
                }
            }
            _ => {}
        }
    }

    if let Some(text) = copy_text {
        ui.ctx().copy_text(text);
    }
    if let Some(text) = paste_text {
        let mut data = text.replace("\r\n", "\n").into_bytes();
        if terminal.emulator.bracketed_paste() {
            let mut wrapped = Vec::with_capacity(data.len() + 12);
            wrapped.extend_from_slice(b"\x1b[200~");
            wrapped.extend_from_slice(&data);
            wrapped.extend_from_slice(b"\x1b[201~");
            data = wrapped;
        }
        bytes.extend(data);
    }
    if !bytes.is_empty() {
        let _ = terminal.write(&bytes);
    }
    if focused {
        tab.paste_pending = false;
    }
}

fn pointer_cell(
    position: Pos2,
    inner: Rect,
    cell_width: f32,
    cell_height: f32,
    view_top: usize,
    columns: usize,
    rows: usize,
) -> (usize, usize) {
    let column = (((position.x - inner.left()) / cell_width).floor() as isize)
        .clamp(0, columns.saturating_sub(1) as isize) as usize;
    let row = (((position.y - inner.top()) / cell_height).floor() as isize)
        .clamp(0, rows.saturating_sub(1) as isize) as usize;
    (view_top + row, column)
}

fn selection_columns(
    selection: Option<Selection>,
    line: usize,
    columns: usize,
) -> Option<(usize, usize)> {
    let selection = selection?;
    let (start, end) = if selection.anchor <= selection.cursor {
        (selection.anchor, selection.cursor)
    } else {
        (selection.cursor, selection.anchor)
    };
    if line < start.0 || line > end.0 {
        return None;
    }
    let from = if line == start.0 {
        start.1.min(columns.saturating_sub(1))
    } else {
        0
    };
    let to = if line == end.0 {
        (end.1 + 1).min(columns)
    } else {
        columns
    };
    (to > from).then_some((from, to))
}

fn paint_run(
    painter: &egui::Painter,
    origin: Pos2,
    cell_width: f32,
    cell_height: f32,
    top: f32,
    start: usize,
    text: &str,
    color: Color32,
    bold: bool,
    underline: bool,
    font: &FontId,
) {
    if text.trim().is_empty() && !underline {
        return;
    }
    let x = origin.x + start as f32 * cell_width;
    let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
    let y = top + (cell_height - galley.size().y) * 0.5;
    painter.galley(Pos2::new(x, y), galley.clone(), color);
    if bold {
        painter.galley(Pos2::new(x + 0.35, y), galley, color);
    }
    if underline {
        let width = text.chars().count() as f32 * cell_width;
        painter.hline(
            x..=(x + width),
            top + cell_height - 1.5,
            Stroke::new(1.0, color),
        );
    }
}

fn effective_foreground(cell: &Cell, p: &Palette) -> Color32 {
    let color = if cell.inverse {
        resolve_color(cell.bg, p, false, false)
    } else {
        resolve_color(cell.fg, p, true, cell.bold)
    };
    if cell.dim {
        color.gamma_multiply(0.65)
    } else {
        color
    }
}

fn effective_background(cell: &Cell, p: &Palette) -> Option<Color32> {
    if cell.inverse {
        Some(resolve_color(cell.fg, p, true, cell.bold))
    } else if cell.bg == Color::Default {
        None
    } else {
        Some(resolve_color(cell.bg, p, false, false))
    }
}

fn resolve_color(color: Color, p: &Palette, foreground: bool, bold: bool) -> Color32 {
    match color {
        Color::Default => {
            if foreground {
                p.fg
            } else {
                p.code_bg
            }
        }
        Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        Color::Indexed(index) => ansi_color(p, index, bold && foreground),
    }
}

fn ansi_color(p: &Palette, index: u8, bright: bool) -> Color32 {
    let index = if bright && index < 8 {
        index + 8
    } else {
        index
    };
    match index {
        0 => {
            if p.dark {
                Color32::from_rgb(0x1C, 0x1B, 0x1A)
            } else {
                Color32::from_rgb(0x4A, 0x42, 0x38)
            }
        }
        1 => p.red,
        2 => p.green,
        3 => {
            if p.dark {
                p.primary
            } else {
                Color32::from_rgb(0x9A, 0x62, 0x12)
            }
        }
        4 => p.blue,
        5 => p.purple,
        6 => {
            if p.dark {
                Color32::from_rgb(0x68, 0xB0, 0xB8)
            } else {
                Color32::from_rgb(0x2F, 0x7C, 0x86)
            }
        }
        7 => {
            if p.dark {
                p.fg
            } else {
                Color32::from_rgb(0x6B, 0x61, 0x56)
            }
        }
        8 => p.fg_faint,
        9 => brighten(p.red, 18),
        10 => brighten(p.green, 18),
        11 => brighten(p.primary, 12),
        12 => brighten(p.blue, 18),
        13 => brighten(p.purple, 18),
        14 => {
            if p.dark {
                Color32::from_rgb(0x8E, 0xD4, 0xD8)
            } else {
                Color32::from_rgb(0x46, 0x9A, 0xA2)
            }
        }
        15 => {
            if p.dark {
                Color32::from_rgb(0xF2, 0xF1, 0xE8)
            } else {
                p.fg
            }
        }
        value if value < 232 => {
            const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            let value = (value - 16) as usize;
            let r = LEVELS[value / 36];
            let g = LEVELS[(value / 6) % 6];
            let b = LEVELS[value % 6];
            Color32::from_rgb(r, g, b)
        }
        value => {
            let level = 8u8.saturating_add(value.saturating_sub(232).saturating_mul(10));
            Color32::from_rgb(level, level, level)
        }
    }
}

fn brighten(color: Color32, amount: u8) -> Color32 {
    Color32::from_rgb(
        color.r().saturating_add(amount),
        color.g().saturating_add(amount),
        color.b().saturating_add(amount),
    )
}

fn encode_key(key: Key, modifiers: Modifiers, application_cursor: bool) -> Option<Vec<u8>> {
    if modifiers.command && !modifiers.ctrl {
        return None;
    }
    if modifiers.ctrl
        && let Some(byte) = control_byte(key, modifiers.shift)
    {
        let mut bytes = Vec::with_capacity(2);
        if modifiers.alt {
            bytes.push(0x1b);
        }
        bytes.push(byte);
        return Some(bytes);
    }
    if let Some(sequence) = special_key(key, modifiers.shift, application_cursor) {
        let mut bytes = Vec::with_capacity(sequence.len() + 1);
        if modifiers.alt {
            bytes.push(0x1b);
        }
        bytes.extend_from_slice(sequence);
        return Some(bytes);
    }
    if modifiers.alt
        && let Some(ch) = alt_char(key, modifiers.shift)
    {
        let mut bytes = Vec::with_capacity(5);
        bytes.push(0x1b);
        let mut buffer = [0u8; 4];
        bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
        return Some(bytes);
    }
    None
}

fn special_key(key: Key, shift: bool, application_cursor: bool) -> Option<&'static [u8]> {
    Some(match key {
        Key::Enter => b"\r",
        Key::Backspace => b"\x7f",
        Key::Tab => {
            if shift {
                b"\x1b[Z"
            } else {
                b"\t"
            }
        }
        Key::Escape => b"\x1b",
        Key::ArrowUp => {
            if application_cursor {
                b"\x1bOA"
            } else {
                b"\x1b[A"
            }
        }
        Key::ArrowDown => {
            if application_cursor {
                b"\x1bOB"
            } else {
                b"\x1b[B"
            }
        }
        Key::ArrowRight => {
            if application_cursor {
                b"\x1bOC"
            } else {
                b"\x1b[C"
            }
        }
        Key::ArrowLeft => {
            if application_cursor {
                b"\x1bOD"
            } else {
                b"\x1b[D"
            }
        }
        Key::Home => {
            if application_cursor {
                b"\x1bOH"
            } else {
                b"\x1b[H"
            }
        }
        Key::End => {
            if application_cursor {
                b"\x1bOF"
            } else {
                b"\x1b[F"
            }
        }
        Key::Insert => b"\x1b[2~",
        Key::Delete => b"\x1b[3~",
        Key::PageUp => b"\x1b[5~",
        Key::PageDown => b"\x1b[6~",
        Key::F1 => b"\x1bOP",
        Key::F2 => b"\x1bOQ",
        Key::F3 => b"\x1bOR",
        Key::F4 => b"\x1bOS",
        Key::F5 => b"\x1b[15~",
        Key::F6 => b"\x1b[17~",
        Key::F7 => b"\x1b[18~",
        Key::F8 => b"\x1b[19~",
        Key::F9 => b"\x1b[20~",
        Key::F10 => b"\x1b[21~",
        Key::F11 => b"\x1b[23~",
        Key::F12 => b"\x1b[24~",
        _ => return None,
    })
}

fn control_byte(key: Key, shift: bool) -> Option<u8> {
    let name = key.name();
    let mut chars = name.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        let upper = ch.to_ascii_uppercase();
        if upper.is_ascii_uppercase() {
            return Some(upper as u8 - b'A' + 1);
        }
    }
    match key {
        Key::Space | Key::Num2 => Some(0),
        Key::OpenBracket => Some(0x1b),
        Key::Backslash => Some(0x1c),
        Key::CloseBracket => Some(0x1d),
        Key::Num6 if shift => Some(0x1e),
        Key::Minus | Key::Slash if shift => Some(0x1f),
        Key::Questionmark => Some(0x7f),
        _ => None,
    }
}

fn alt_char(key: Key, shift: bool) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        if ch.is_ascii_alphabetic() {
            return Some(if shift {
                ch.to_ascii_uppercase()
            } else {
                ch.to_ascii_lowercase()
            });
        }
        if ch.is_ascii_digit() {
            return Some(ch);
        }
    }
    Some(match key {
        Key::Space => ' ',
        Key::Colon => ':',
        Key::Comma => ',',
        Key::Minus => '-',
        Key::Period => '.',
        Key::Plus => '+',
        Key::Equals => '=',
        Key::Semicolon => ';',
        Key::Quote => '\'',
        Key::Backslash => '\\',
        Key::Slash => '/',
        Key::Pipe => '|',
        Key::Questionmark => '?',
        Key::Exclamationmark => '!',
        Key::OpenBracket => '[',
        Key::CloseBracket => ']',
        Key::OpenCurlyBracket => '{',
        Key::CloseCurlyBracket => '}',
        Key::Backtick => '`',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_encoding() {
        let none = Modifiers::default();
        assert_eq!(encode_key(Key::Enter, none, false).unwrap(), b"\r".to_vec());
        assert_eq!(
            encode_key(Key::Backspace, none, false).unwrap(),
            b"\x7f".to_vec()
        );
        assert_eq!(encode_key(Key::Tab, none, false).unwrap(), b"\t".to_vec());
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(encode_key(Key::C, ctrl, false).unwrap(), vec![0x03]);
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        assert_eq!(encode_key(Key::A, alt, false).unwrap(), vec![0x1b, b'a']);
        assert_eq!(
            encode_key(Key::ArrowUp, none, true).unwrap(),
            b"\x1bOA".to_vec()
        );
        let command = Modifiers {
            command: true,
            ..Modifiers::default()
        };
        assert!(encode_key(Key::A, command, false).is_none());
    }

    #[test]
    #[ignore]
    fn paints_a_terminal() {
        let cwd = std::env::current_dir().unwrap();
        let mut terminal =
            rchamber_term::Terminal::spawn_with(&cwd, "test", 80, 24, Arc::new(|| {})).unwrap();
        terminal
            .emulator
            .feed(b"\x1b[31mhello\x1b[0m\r\n\x1b[?1049halternate\x1b[?1049l");
        let mut tab = Tab::new(1, "test".into());
        tab.terminal = Some(terminal);
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let palette = Palette::dark();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let ctx = ui.ctx().clone();
            render_tab(&mut tab, ui, &palette, &ctx, "/");
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
