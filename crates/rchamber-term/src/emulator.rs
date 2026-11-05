use std::collections::VecDeque;

use vte::{Params, Perform};

pub const SCROLLBACK_LIMIT: usize = 2000;
pub const MIN_COLS: usize = 2;
pub const MIN_ROWS: usize = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub dim: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
            dim: false,
        }
    }
}

impl Cell {
    fn blank_with_bg(bg: Color) -> Self {
        Self {
            bg,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Style {
    fg: Color,
    bg: Color,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

#[derive(Clone)]
struct Grid {
    cols: usize,
    rows: usize,
    lines: VecDeque<Vec<Cell>>,
    scrollback_limit: usize,
}

impl Grid {
    fn new(cols: usize, rows: usize, scrollback_limit: usize) -> Self {
        let mut lines = VecDeque::with_capacity(rows);
        for _ in 0..rows {
            lines.push_back(Self::blank_line(cols));
        }
        Self {
            cols,
            rows,
            lines,
            scrollback_limit,
        }
    }

    fn blank_line(cols: usize) -> Vec<Cell> {
        vec![Cell::default(); cols]
    }

    fn base(&self) -> usize {
        self.lines.len().saturating_sub(self.rows)
    }

    fn screen_mut(&mut self, row: usize) -> Option<&mut Vec<Cell>> {
        let index = self.base() + row;
        self.lines.get_mut(index)
    }

    fn push_line(&mut self, line: Vec<Cell>) {
        self.lines.push_back(line);
        self.trim();
    }

    fn trim(&mut self) {
        let capacity = self.rows + self.scrollback_limit;
        while self.lines.len() > capacity {
            self.lines.pop_front();
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        if cols != self.cols {
            for line in &mut self.lines {
                line.resize(cols, Cell::default());
            }
            self.cols = cols;
        }
        if rows > self.rows {
            while self.lines.len() < rows {
                self.lines.push_back(Self::blank_line(cols));
            }
        }
        self.rows = rows;
        self.trim();
    }

    fn clear_history(&mut self) {
        let base = self.base();
        for _ in 0..base {
            self.lines.pop_front();
        }
    }
}

struct SavedScreen {
    grid: Grid,
    cursor_row: usize,
    cursor_col: usize,
}

pub struct Emulator {
    grid: Grid,
    alt_saved: Option<SavedScreen>,
    cursor_row: usize,
    cursor_col: usize,
    saved_cursor: (usize, usize),
    style: Style,
    cursor_visible: bool,
    auto_wrap: bool,
    app_cursor: bool,
    bracketed_paste: bool,
    origin_mode: bool,
    wrap_pending: bool,
    scroll_top: usize,
    scroll_bottom: usize,
    parser: vte::Parser,
    responses: Vec<u8>,
    title: String,
    view_offset: usize,
    generation: u64,
    last_char: Option<char>,
}

impl Emulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(MIN_COLS);
        let rows = rows.max(MIN_ROWS);
        Self {
            grid: Grid::new(cols, rows, SCROLLBACK_LIMIT),
            alt_saved: None,
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor: (0, 0),
            style: Style::default(),
            cursor_visible: true,
            auto_wrap: true,
            app_cursor: false,
            bracketed_paste: false,
            origin_mode: false,
            wrap_pending: false,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            parser: vte::Parser::new(),
            responses: Vec::new(),
            title: String::new(),
            view_offset: 0,
            generation: 1,
            last_char: None,
        }
    }

    pub fn cols(&self) -> usize {
        self.grid.cols
    }

    pub fn rows(&self) -> usize {
        self.grid.rows
    }

    pub fn total_lines(&self) -> usize {
        self.grid.lines.len()
    }

    pub fn history_len(&self) -> usize {
        if self.alt_saved.is_some() {
            0
        } else {
            self.grid.lines.len().saturating_sub(self.grid.rows)
        }
    }

    pub fn line(&self, index: usize) -> Option<&[Cell]> {
        self.grid.lines.get(index).map(Vec::as_slice)
    }

    pub fn cursor_row(&self) -> usize {
        self.cursor_row
    }

    pub fn cursor_col(&self) -> usize {
        self.cursor_col
    }

    pub fn cursor_line(&self) -> usize {
        self.history_len() + self.cursor_row
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn application_cursor(&self) -> bool {
        self.app_cursor
    }

    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    pub fn is_alt_screen(&self) -> bool {
        self.alt_saved.is_some()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn view_offset(&self) -> usize {
        self.view_offset.min(self.history_len())
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let max = self.history_len();
        let current = self.view_offset.min(max);
        let next = if delta >= 0 {
            current.saturating_add(delta as usize).min(max)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        if next != self.view_offset {
            self.view_offset = next;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn set_view_offset(&mut self, offset: usize) {
        let next = offset.min(self.history_len());
        if next != self.view_offset {
            self.view_offset = next;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.set_view_offset(0);
    }

    pub fn take_responses(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.responses)
    }

    pub fn feed(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let before = self.history_len();
        let mut parser = std::mem::take(&mut self.parser);
        for &byte in data {
            parser.advance(self, &[byte]);
        }
        self.parser = parser;
        let after = self.history_len();
        if self.view_offset > 0 && after > before {
            self.view_offset = (self.view_offset + (after - before)).min(after);
        }
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(MIN_COLS);
        let rows = rows.max(MIN_ROWS);
        if cols == self.grid.cols && rows == self.grid.rows {
            return;
        }
        let old_rows = self.grid.rows;
        let old_len = self.grid.lines.len();
        if rows > old_rows {
            if old_len >= rows {
                self.cursor_row = self.cursor_row.saturating_add(rows - old_rows);
            }
        } else if rows < old_rows {
            self.cursor_row = self.cursor_row.saturating_sub(old_rows - rows);
        }
        self.grid.resize(cols, rows);
        if let Some(saved) = self.alt_saved.as_mut() {
            let saved_len = saved.grid.lines.len();
            if rows > old_rows {
                if saved_len >= rows {
                    saved.cursor_row = saved.cursor_row.saturating_add(rows - old_rows);
                }
            } else if rows < old_rows {
                saved.cursor_row = saved.cursor_row.saturating_sub(old_rows - rows);
            }
            saved.grid.resize(cols, rows);
            saved.cursor_row = saved.cursor_row.min(rows - 1);
            saved.cursor_col = saved.cursor_col.min(cols - 1);
        }
        self.cursor_row = self.cursor_row.min(rows - 1);
        self.cursor_col = self.cursor_col.min(cols - 1);
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.wrap_pending = false;
        self.view_offset = self.view_offset.min(self.history_len());
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn visible_text(&self) -> String {
        let total = self.grid.lines.len();
        let start = total.saturating_sub(self.grid.rows + self.view_offset());
        let mut lines = Vec::with_capacity(self.grid.rows);
        for index in start..start + self.grid.rows {
            let text = self
                .grid
                .lines
                .get(index)
                .map(|line| Self::line_text(line))
                .unwrap_or_default();
            lines.push(text);
        }
        while lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    pub fn text_in_range(&self, start: (usize, usize), end: (usize, usize)) -> String {
        let (first, last) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let total = self.total_lines();
        if total == 0 || first.0 >= total {
            return String::new();
        }
        let mut lines = Vec::new();
        let last_line = last.0.min(total - 1);
        for index in first.0..=last_line {
            let Some(cells) = self.line(index) else {
                continue;
            };
            let from = if index == first.0 {
                first.1.min(cells.len())
            } else {
                0
            };
            let to = if index == last_line {
                (last.1 + 1).min(cells.len())
            } else {
                cells.len()
            };
            let text: String = cells[from..to].iter().map(|cell| cell.ch).collect();
            lines.push(text.trim_end().to_string());
        }
        lines.join("\n")
    }

    fn line_text(cells: &[Cell]) -> String {
        let text: String = cells.iter().map(|cell| cell.ch).collect();
        text.trim_end().to_string()
    }

    fn blank(&self) -> Cell {
        Cell::blank_with_bg(self.style.bg)
    }

    fn put(&mut self, row: usize, col: usize, cell: Cell) {
        if let Some(line) = self.grid.screen_mut(row)
            && let Some(slot) = line.get_mut(col)
        {
            *slot = cell;
        }
    }

    fn print(&mut self, ch: char) {
        if self.wrap_pending && self.auto_wrap {
            self.cursor_col = 0;
            self.linefeed();
        }
        self.wrap_pending = false;
        if self.cursor_col >= self.grid.cols {
            self.cursor_col = self.grid.cols - 1;
        }
        let cell = Cell {
            ch,
            fg: self.style.fg,
            bg: self.style.bg,
            bold: self.style.bold,
            italic: self.style.italic,
            underline: self.style.underline,
            inverse: self.style.inverse,
            dim: self.style.dim,
        };
        let row = self.cursor_row;
        let col = self.cursor_col;
        self.put(row, col, cell);
        self.last_char = Some(ch);
        if self.cursor_col + 1 >= self.grid.cols {
            if self.auto_wrap {
                self.wrap_pending = true;
            }
        } else {
            self.cursor_col += 1;
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn linefeed(&mut self) {
        self.wrap_pending = false;
        let top = if self.cursor_row < self.scroll_top {
            0
        } else {
            self.scroll_top
        };
        let bottom = if self.cursor_row > self.scroll_bottom {
            self.grid.rows - 1
        } else {
            self.scroll_bottom
        };
        if self.cursor_row >= bottom {
            self.scroll_region(top, bottom, 1, true);
        } else {
            self.cursor_row += 1;
        }
    }

    fn reverse_index(&mut self) {
        self.wrap_pending = false;
        let top = if self.cursor_row < self.scroll_top {
            0
        } else {
            self.scroll_top
        };
        let bottom = if self.cursor_row > self.scroll_bottom {
            self.grid.rows - 1
        } else {
            self.scroll_bottom
        };
        if self.cursor_row <= top {
            self.scroll_region(top, bottom, 1, false);
        } else {
            self.cursor_row -= 1;
        }
    }

    fn scroll_region(&mut self, top: usize, bottom: usize, count: usize, up: bool) {
        let rows = self.grid.rows;
        let top = top.min(rows - 1);
        let bottom = bottom.min(rows - 1).max(top);
        if up {
            if top == 0 && bottom == rows - 1 {
                for _ in 0..count {
                    let line = Grid::blank_line(self.grid.cols);
                    self.grid.push_line(line);
                }
            } else {
                for _ in 0..count {
                    let base = self.grid.base();
                    self.grid.lines.remove(base + top);
                    let line = Grid::blank_line(self.grid.cols);
                    self.grid.lines.insert(base + bottom, line);
                }
            }
        } else {
            for _ in 0..count {
                let base = self.grid.base();
                self.grid.lines.remove(base + bottom);
                let line = Grid::blank_line(self.grid.cols);
                self.grid.lines.insert(base + top, line);
            }
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn clear_row(&mut self, row: usize) {
        let blank = self.blank();
        if let Some(line) = self.grid.screen_mut(row) {
            line.fill(blank);
        }
    }

    fn erase_line(&mut self, mode: u16) {
        let blank = self.blank();
        let row = self.cursor_row;
        let cols = self.grid.cols;
        let cursor = self.cursor_col;
        if let Some(line) = self.grid.screen_mut(row) {
            match mode {
                0 => {
                    for cell in line.iter_mut().skip(cursor) {
                        *cell = blank;
                    }
                }
                1 => {
                    let end = (cursor + 1).min(cols);
                    for cell in line.iter_mut().take(end) {
                        *cell = blank;
                    }
                }
                _ => line.fill(blank),
            }
        }
    }

    fn erase_display(&mut self, mode: u16) {
        match mode {
            0 => {
                self.erase_line(0);
                for row in self.cursor_row + 1..self.grid.rows {
                    self.clear_row(row);
                }
            }
            1 => {
                for row in 0..self.cursor_row {
                    self.clear_row(row);
                }
                self.erase_line(1);
            }
            2 => {
                for row in 0..self.grid.rows {
                    self.clear_row(row);
                }
            }
            3 => {
                for row in 0..self.grid.rows {
                    self.clear_row(row);
                }
                self.grid.clear_history();
                self.view_offset = 0;
            }
            _ => {}
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn insert_lines(&mut self, count: usize) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        for _ in 0..count {
            let base = self.grid.base();
            self.grid.lines.remove(base + bottom);
            let line = Grid::blank_line(self.grid.cols);
            self.grid.lines.insert(base + top, line);
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn delete_lines(&mut self, count: usize) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        for _ in 0..count {
            let base = self.grid.base();
            self.grid.lines.remove(base + top);
            let line = Grid::blank_line(self.grid.cols);
            self.grid.lines.insert(base + bottom, line);
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn insert_chars(&mut self, count: usize) {
        let blank = self.blank();
        let cursor = self.cursor_col;
        let row = self.cursor_row;
        if let Some(line) = self.grid.screen_mut(row)
            && cursor < line.len()
        {
            let tail = &mut line[cursor..];
            let count = count.min(tail.len());
            tail.rotate_right(count);
            for cell in tail.iter_mut().take(count) {
                *cell = blank;
            }
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn delete_chars(&mut self, count: usize) {
        let blank = self.blank();
        let cursor = self.cursor_col;
        let row = self.cursor_row;
        if let Some(line) = self.grid.screen_mut(row)
            && cursor < line.len()
        {
            let tail = &mut line[cursor..];
            let count = count.min(tail.len());
            tail.rotate_left(count);
            let len = tail.len();
            for cell in tail.iter_mut().skip(len - count) {
                *cell = blank;
            }
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn erase_chars(&mut self, count: usize) {
        let blank = self.blank();
        let cursor = self.cursor_col;
        let row = self.cursor_row;
        if let Some(line) = self.grid.screen_mut(row) {
            let end = (cursor + count).min(line.len());
            for cell in line.iter_mut().take(end).skip(cursor) {
                *cell = blank;
            }
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn cursor_up(&mut self, count: usize) {
        let limit = if self.cursor_row >= self.scroll_top {
            self.scroll_top
        } else {
            0
        };
        self.cursor_row = self.cursor_row.saturating_sub(count).max(limit);
        self.wrap_pending = false;
    }

    fn cursor_down(&mut self, count: usize) {
        let limit = if self.cursor_row <= self.scroll_bottom {
            self.scroll_bottom
        } else {
            self.grid.rows - 1
        };
        self.cursor_row = (self.cursor_row + count).min(limit);
        self.wrap_pending = false;
    }

    fn cursor_forward(&mut self, count: usize) {
        self.cursor_col = (self.cursor_col + count).min(self.grid.cols - 1);
        self.wrap_pending = false;
    }

    fn cursor_back(&mut self, count: usize) {
        self.cursor_col = self.cursor_col.saturating_sub(count);
        self.wrap_pending = false;
    }

    fn set_column(&mut self, col: usize) {
        self.cursor_col = col.min(self.grid.cols - 1);
        self.wrap_pending = false;
    }

    fn set_row(&mut self, row: usize) {
        let row = if self.origin_mode {
            self.scroll_top + row
        } else {
            row
        };
        self.cursor_row = row.min(self.grid.rows - 1);
        self.wrap_pending = false;
    }

    fn save_cursor(&mut self) {
        self.saved_cursor = (self.cursor_row, self.cursor_col);
    }

    fn restore_cursor(&mut self) {
        self.cursor_row = self.saved_cursor.0.min(self.grid.rows - 1);
        self.cursor_col = self.saved_cursor.1.min(self.grid.cols - 1);
        self.wrap_pending = false;
    }

    fn enter_alt(&mut self) {
        if self.alt_saved.is_some() {
            return;
        }
        let cols = self.grid.cols;
        let rows = self.grid.rows;
        let alt = Grid::new(cols, rows, 0);
        let primary = std::mem::replace(&mut self.grid, alt);
        self.alt_saved = Some(SavedScreen {
            grid: primary,
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.wrap_pending = false;
        self.view_offset = 0;
        self.generation = self.generation.wrapping_add(1);
    }

    fn leave_alt(&mut self) {
        let Some(saved) = self.alt_saved.take() else {
            return;
        };
        self.grid = saved.grid;
        self.cursor_row = saved.cursor_row.min(self.grid.rows - 1);
        self.cursor_col = saved.cursor_col.min(self.grid.cols - 1);
        self.scroll_top = 0;
        self.scroll_bottom = self.grid.rows - 1;
        self.wrap_pending = false;
        self.generation = self.generation.wrapping_add(1);
    }

    fn reset(&mut self) {
        let cols = self.grid.cols;
        let rows = self.grid.rows;
        self.grid = Grid::new(cols, rows, SCROLLBACK_LIMIT);
        self.alt_saved = None;
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.saved_cursor = (0, 0);
        self.style = Style::default();
        self.cursor_visible = true;
        self.auto_wrap = true;
        self.app_cursor = false;
        self.origin_mode = false;
        self.wrap_pending = false;
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.title.clear();
        self.view_offset = 0;
        self.last_char = None;
        self.generation = self.generation.wrapping_add(1);
    }

    fn set_fg(&mut self, color: Color) {
        self.style.fg = color;
    }

    fn set_bg(&mut self, color: Color) {
        self.style.bg = color;
    }

    fn sgr_simple(&mut self, code: u16) {
        match code {
            0 => self.style = Style::default(),
            1 => self.style.bold = true,
            2 => self.style.dim = true,
            3 => self.style.italic = true,
            4 => self.style.underline = true,
            7 => self.style.inverse = true,
            21 => self.style.underline = true,
            22 => {
                self.style.bold = false;
                self.style.dim = false;
            }
            23 => self.style.italic = false,
            24 => self.style.underline = false,
            27 => self.style.inverse = false,
            30..=37 => self.set_fg(Color::Indexed((code - 30) as u8)),
            39 => self.set_fg(Color::Default),
            40..=47 => self.set_bg(Color::Indexed((code - 40) as u8)),
            49 => self.set_bg(Color::Default),
            90..=97 => self.set_fg(Color::Indexed((code - 90 + 8) as u8)),
            100..=107 => self.set_bg(Color::Indexed((code - 100 + 8) as u8)),
            _ => {}
        }
    }

    fn sgr(&mut self, params: &Params) {
        let groups: Vec<&[u16]> = params.iter().collect();
        if groups.is_empty() || groups.iter().all(|group| group.is_empty()) {
            self.style = Style::default();
            return;
        }
        let mut index = 0;
        while index < groups.len() {
            let group = groups[index];
            if group.is_empty() {
                index += 1;
                continue;
            }
            let code = group[0];
            if group.len() > 1 {
                match code {
                    38 => {
                        if let Some(color) = extended_color(&group[1..]) {
                            self.set_fg(color);
                        }
                    }
                    48 => {
                        if let Some(color) = extended_color(&group[1..]) {
                            self.set_bg(color);
                        }
                    }
                    _ => {
                        for &part in group {
                            self.sgr_simple(part);
                        }
                    }
                }
                index += 1;
                continue;
            }
            match code {
                38 | 48 => {
                    let Some(mode) = groups.get(index + 1).and_then(|group| group.first()) else {
                        index += 1;
                        continue;
                    };
                    if *mode == 5 {
                        if let Some(value) = groups.get(index + 2).and_then(|group| group.first()) {
                            let color = Color::Indexed((*value).min(255) as u8);
                            if code == 38 {
                                self.set_fg(color);
                            } else {
                                self.set_bg(color);
                            }
                        }
                        index += 3;
                    } else if *mode == 2 {
                        if index + 4 < groups.len() {
                            let r = groups[index + 2].first().copied().unwrap_or(0) as u8;
                            let g = groups[index + 3].first().copied().unwrap_or(0) as u8;
                            let b = groups[index + 4].first().copied().unwrap_or(0) as u8;
                            let color = Color::Rgb(r, g, b);
                            if code == 38 {
                                self.set_fg(color);
                            } else {
                                self.set_bg(color);
                            }
                            index += 5;
                        } else {
                            index += 1;
                        }
                    } else {
                        index += 1;
                    }
                }
                _ => {
                    self.sgr_simple(code);
                    index += 1;
                }
            }
        }
    }

    fn set_mode(&mut self, params: &Params, private: bool, enable: bool) {
        for group in params.iter() {
            let Some(&mode) = group.first() else {
                continue;
            };
            if !private {
                continue;
            }
            match mode {
                1 => self.app_cursor = enable,
                6 => self.origin_mode = enable,
                7 => self.auto_wrap = enable,
                25 => self.cursor_visible = enable,
                47 | 1047 | 1049 => {
                    if enable {
                        self.enter_alt();
                    } else {
                        self.leave_alt();
                    }
                }
                2004 => self.bracketed_paste = enable,
                _ => {}
            }
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn respond(&mut self, bytes: &[u8]) {
        self.responses.extend_from_slice(bytes);
    }

    fn tab_forward(&mut self, count: usize) {
        for _ in 0..count {
            let next = (self.cursor_col / 8 + 1) * 8;
            self.cursor_col = next.min(self.grid.cols - 1);
        }
        self.wrap_pending = false;
    }

    fn tab_back(&mut self, count: usize) {
        for _ in 0..count {
            if self.cursor_col == 0 {
                break;
            }
            self.cursor_col = ((self.cursor_col - 1) / 8) * 8;
        }
        self.wrap_pending = false;
    }
}

fn extended_color(params: &[u16]) -> Option<Color> {
    let mode = *params.first()?;
    match mode {
        5 => params
            .get(1)
            .map(|value| Color::Indexed((*value).min(255) as u8)),
        2 => {
            if params.len() >= 5 {
                Some(Color::Rgb(
                    params[2] as u8,
                    params[3] as u8,
                    params[4] as u8,
                ))
            } else if params.len() == 4 {
                Some(Color::Rgb(
                    params[1] as u8,
                    params[2] as u8,
                    params[3] as u8,
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

impl Perform for Emulator {
    fn print(&mut self, ch: char) {
        self.print(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {}
            0x08 => {
                self.cursor_col = self.cursor_col.saturating_sub(1);
                self.wrap_pending = false;
            }
            0x09 => self.tab_forward(1),
            0x0A..=0x0C => self.linefeed(),
            0x0D => {
                self.cursor_col = 0;
                self.wrap_pending = false;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }
        let groups: Vec<&[u16]> = params.iter().collect();
        let value = |index: usize, default: usize| -> usize {
            groups
                .get(index)
                .and_then(|group| group.first())
                .copied()
                .map(|value| if value == 0 { default } else { value as usize })
                .unwrap_or(default)
        };
        let private = intermediates.contains(&b'?');
        match action {
            'A' => self.cursor_up(value(0, 1)),
            'B' => self.cursor_down(value(0, 1)),
            'C' => self.cursor_forward(value(0, 1)),
            'D' => self.cursor_back(value(0, 1)),
            'E' => {
                let count = value(0, 1);
                self.cursor_down(count);
                self.cursor_col = 0;
            }
            'F' => {
                let count = value(0, 1);
                self.cursor_up(count);
                self.cursor_col = 0;
            }
            'G' | '`' => self.set_column(value(0, 1) - 1),
            'd' => self.set_row(value(0, 1) - 1),
            'H' | 'f' => {
                let row = value(0, 1) - 1;
                let col = value(1, 1) - 1;
                self.set_row(row);
                self.set_column(col);
            }
            'J' => self.erase_display(value(0, 0) as u16),
            'K' => self.erase_line(value(0, 0) as u16),
            'L' => self.insert_lines(value(0, 1)),
            'M' => self.delete_lines(value(0, 1)),
            'P' => self.delete_chars(value(0, 1)),
            '@' => self.insert_chars(value(0, 1)),
            'X' => self.erase_chars(value(0, 1)),
            'S' => {
                let top = self.scroll_top;
                let bottom = self.scroll_bottom;
                self.scroll_region(top, bottom, value(0, 1), true);
            }
            'T' => {
                let top = self.scroll_top;
                let bottom = self.scroll_bottom;
                self.scroll_region(top, bottom, value(0, 1), false);
            }
            'm' => self.sgr(params),
            'h' => self.set_mode(params, private, true),
            'l' => self.set_mode(params, private, false),
            'r' => {
                let top = value(0, 1) - 1;
                let bottom = value(1, self.grid.rows) - 1;
                let top = top.min(self.grid.rows - 1);
                let bottom = bottom.min(self.grid.rows - 1).max(top);
                self.scroll_top = top;
                self.scroll_bottom = bottom;
                self.set_row(0);
                self.set_column(0);
            }
            's' => self.save_cursor(),
            'u' => self.restore_cursor(),
            'Z' => self.tab_back(value(0, 1)),
            'b' => {
                if let Some(ch) = self.last_char {
                    let count = value(0, 1);
                    for _ in 0..count {
                        self.print(ch);
                    }
                }
            }
            'n' => match value(0, 0) {
                5 => self.respond(b"\x1b[0n"),
                6 => {
                    let row = self.cursor_row + 1;
                    let col = self.cursor_col + 1;
                    let report = format!("\x1b[{row};{col}R");
                    self.respond(report.as_bytes());
                }
                _ => {}
            },
            'c' => {
                if intermediates.contains(&b'>') {
                    self.respond(b"\x1b[>0;1;0c");
                } else {
                    self.respond(b"\x1b[?1;2c");
                }
            }
            't' => match value(0, 0) {
                18 | 19 => {
                    let rows = self.grid.rows;
                    let cols = self.grid.cols;
                    let report = format!("\x1b[8;{rows};{cols}t");
                    self.respond(report.as_bytes());
                }
                _ => {}
            },
            'g' | 'q' | 'I' | 'W' => {}
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        match byte {
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'D' => self.linefeed(),
            b'E' => {
                self.cursor_col = 0;
                self.linefeed();
            }
            b'M' => self.reverse_index(),
            b'c' => self.reset(),
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let Some(command) = params.first() else {
            return;
        };
        if (*command == b"0" || *command == b"2") && params.len() > 1 {
            let title = String::from_utf8_lossy(params[1]).trim().to_string();
            if !title.is_empty() {
                self.title = title;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(emulator: &Emulator, row: usize) -> String {
        emulator
            .line(row)
            .unwrap_or_default()
            .iter()
            .map(|cell| cell.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn prints_and_wraps() {
        let mut emulator = Emulator::new(4, 3);
        emulator.feed(b"abcdef");
        assert_eq!(text(&emulator, 0), "abcd");
        assert_eq!(text(&emulator, 1), "ef");
        assert_eq!(emulator.cursor_row(), 1);
        assert_eq!(emulator.cursor_col(), 2);
    }

    #[test]
    fn cursor_motion_and_erase() {
        let mut emulator = Emulator::new(10, 3);
        emulator.feed(b"hello\x1b[2D\x1b[K");
        assert_eq!(text(&emulator, 0), "hel");
        emulator.feed(b"\x1b[2J");
        assert_eq!(text(&emulator, 0), "");
    }

    #[test]
    fn colors() {
        let mut emulator = Emulator::new(10, 3);
        emulator.feed(b"\x1b[31mA\x1b[38;5;196mB\x1b[38;2;1;2;3mC\x1b[0mD");
        let line = emulator.line(0).unwrap();
        assert_eq!(line[0].fg, Color::Indexed(1));
        assert_eq!(line[1].fg, Color::Indexed(196));
        assert_eq!(line[2].fg, Color::Rgb(1, 2, 3));
        assert_eq!(line[3].fg, Color::Default);
        emulator.feed(b"\x1b[48;5;22m\x1b[1;4mE");
        let cell = emulator.line(0).unwrap()[4];
        assert_eq!(cell.bg, Color::Indexed(22));
        assert!(cell.bold);
        assert!(cell.underline);
        emulator.feed(b"\x1b[38:5:123mF\x1b[48:2::9:8:7mG");
        let line = emulator.line(0).unwrap();
        assert_eq!(line[5].fg, Color::Indexed(123));
        assert_eq!(line[6].bg, Color::Rgb(9, 8, 7));
    }

    #[test]
    fn scrollback() {
        let mut emulator = Emulator::new(10, 3);
        for _ in 0..8 {
            emulator.feed(b"row\r\n");
        }
        assert!(emulator.history_len() >= 6);
        emulator.scroll_by(3);
        assert_eq!(emulator.view_offset(), 3);
        emulator.scroll_to_bottom();
        assert_eq!(emulator.view_offset(), 0);
    }

    #[test]
    fn alternate_screen() {
        let mut emulator = Emulator::new(10, 3);
        emulator.feed(b"main");
        emulator.feed(b"\x1b[?1049h");
        assert!(emulator.is_alt_screen());
        emulator.feed(b"alternate");
        emulator.feed(b"\x1b[?1049l");
        assert!(!emulator.is_alt_screen());
        assert_eq!(text(&emulator, 0), "main");
    }

    #[test]
    fn resize_keeps_content() {
        let mut emulator = Emulator::new(10, 3);
        emulator.feed(b"one\r\ntwo");
        emulator.resize(20, 5);
        assert_eq!(text(&emulator, 0), "one");
        assert_eq!(text(&emulator, 1), "two");
        assert_eq!(emulator.cursor_row(), 1);
        emulator.resize(20, 2);
        assert_eq!(text(&emulator, 1), "two");
    }
}
