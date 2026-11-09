use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use calamine::{Reader as CalamineReader, open_workbook_auto};
use eframe::egui::{
    self, Align, Align2, Id, Label, Layout, Margin, Pos2, Rect, RichText, ScrollArea, Sense,
    Stroke, Ui, Vec2,
};
use egui_extras::{Column, TableBuilder};
use parquet::basic::ConvertedType;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use quick_xml::{Reader as XmlReader, XmlVersion};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use zip::ZipArchive;

use crate::icons::{self, Icon};
use crate::markdown::Markdown;
use crate::surfaces;
use crate::theme::{self, Palette};
use crate::widgets;

const SQL_ROW_CAP: usize = 500;
const PARQUET_ROW_CAP: usize = 200;
const OFFICE_ROW_CAP: usize = 200;
const OFFICE_COL_CAP: usize = 60;
const OFFICE_XML_BYTES: usize = 16 * 1024 * 1024;
const ZIP_ENTRY_CAP: usize = 20_000;
const ZIP_PREVIEW_BYTES: usize = 1024 * 1024;
const NOTEBOOK_BYTES: usize = 8 * 1024 * 1024;
const NOTEBOOK_CELL_CAP: usize = 500;
const DOC_TEXT_CAP: usize = 2_000_000;
const CELL_CHARS: usize = 300;
const EXPORT_ROWS: usize = 1_048_576;
const EXPORT_COLS: usize = 16_384;

const TABLE_ROW_HEIGHT: f32 = 22.0;
const TABLE_HEADER_HEIGHT: f32 = 24.0;

pub fn supported(path: &str) -> bool {
    matches!(
        extension(path).as_str(),
        "db" | "sqlite"
            | "sqlite3"
            | "parquet"
            | "zip"
            | "ipynb"
            | "xlsx"
            | "xls"
            | "xlsb"
            | "ods"
            | "docx"
            | "pptx"
    )
}

pub fn is_csv(path: &str) -> bool {
    extension(path) == "csv"
}

pub fn show(ui: &mut Ui, p: &Palette, path: &str, local: &Path) {
    match extension(path).as_str() {
        "db" | "sqlite" | "sqlite3" => sqlite_view(ui, p, path, local),
        "parquet" => parquet_view(ui, p, path, local),
        "zip" => zip_view(ui, p, path, local),
        "ipynb" => notebook_view(ui, p, path, local),
        "xlsx" | "xls" | "xlsb" | "ods" => workbook_view(ui, p, path, local),
        "docx" => docx_view(ui, p, path, local),
        "pptx" => pptx_view(ui, p, path, local),
        _ => error_view(
            ui,
            p,
            "Preview unavailable",
            "This file type has no viewer.",
        ),
    }
}

pub fn export_csv(path: &Path) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let rows = parse_csv(&text);
    if rows.len() > EXPORT_ROWS {
        return Err(format!(
            "The CSV has more than {EXPORT_ROWS} rows and cannot be exported to Excel."
        ));
    }
    if rows.iter().any(|row| row.len() > EXPORT_COLS) {
        return Err(format!(
            "The CSV has more than {EXPORT_COLS} columns and cannot be exported to Excel."
        ));
    }
    let target = unique_export_path(path, |candidate| candidate.exists());
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, value) in row.iter().enumerate() {
            let row_index = row_index as u32;
            let column_index = column_index as u16;
            let trimmed = value.trim();
            if let Ok(number) = trimmed.parse::<f64>()
                && number.is_finite()
            {
                sheet
                    .write_number(row_index, column_index, number)
                    .map_err(|error| error.to_string())?;
            } else {
                sheet
                    .write_string(row_index, column_index, value)
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    workbook
        .save(&target)
        .map_err(|error| format!("Could not write {}: {error}", target.display()))?;
    Ok(target)
}

fn sqlite_view(ui: &mut Ui, p: &Palette, path: &str, local: &Path) {
    let stamp = file_stamp(local);
    let tables_id = Id::unique(("viewers-sqlite-tables", path, stamp));
    let tables = match load_cached(ui.ctx(), tables_id, || sqlite_tables(local)) {
        Loaded::Ready(tables) => tables,
        Loaded::Failed(error) => {
            error_view(ui, p, "Could not read database", &error);
            return;
        }
    };
    if tables.is_empty() {
        viewer_header(ui, p, "SQLite database", "no tables");
        empty_note(ui, p, "This database has no tables or views.");
        return;
    }
    let selected_id = Id::unique(("viewers-sqlite-selected", path));
    let mut selected = selected_index(ui.ctx(), selected_id, tables.len());
    let meta = format!("SQLite · {} tables", tables.len());
    if let Some(index) = section_strip(ui, p, ("sqlite", path), &tables, &mut selected, &meta) {
        selected = index;
        ui.ctx()
            .data_mut(|data| data.insert_temp(selected_id, selected));
    }
    let table = &tables[selected];
    let page_id = Id::unique(("viewers-sqlite-page", path, table, stamp));
    match load_cached(ui.ctx(), page_id, || sqlite_page(local, table)) {
        Loaded::Ready(page) => {
            if page.rows.is_empty() {
                empty_note(ui, p, "This table has no rows.");
            }
            data_table(
                ui,
                p,
                ("sqlite-page", path, table),
                &page.headers,
                &page.rows,
            );
        }
        Loaded::Failed(error) => error_view(ui, p, "Could not read table", &error),
    }
}

fn sqlite_open(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Could not open the database read-only: {error}"))
}

fn sqlite_tables(path: &Path) -> Result<Vec<String>, String> {
    let connection = sqlite_open(path)?;
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master \
             WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|error| format!("Could not list tables: {error}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("Could not list tables: {error}"))?;
    let mut tables = Vec::new();
    for row in rows {
        match row {
            Ok(name) => tables.push(name),
            Err(error) => return Err(format!("Could not list tables: {error}")),
        }
    }
    Ok(tables)
}

fn sqlite_page(path: &Path, table: &str) -> Result<TablePage, String> {
    let connection = sqlite_open(path)?;
    let sql = format!(
        "SELECT * FROM {} LIMIT {SQL_ROW_CAP}",
        quote_identifier(table)
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("Could not read {table}: {error}"))?;
    let headers: Vec<String> = statement
        .column_names()
        .map(|name| name.to_string())
        .collect();
    let column_count = statement.column_count();
    let mut query = statement
        .query([])
        .map_err(|error| format!("Could not read {table}: {error}"))?;
    let mut rows = Vec::new();
    while let Some(row) = query
        .next()
        .map_err(|error| format!("Could not read {table}: {error}"))?
    {
        let mut cells = Vec::with_capacity(column_count);
        for index in 0..column_count {
            let value = row
                .get_ref(index)
                .map_err(|error| format!("Could not read {table}: {error}"))?;
            cells.push(sqlite_cell(value));
        }
        rows.push(cells);
    }
    Ok(TablePage { headers, rows })
}

fn sqlite_cell(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(value) => clip_cell(&String::from_utf8_lossy(value)),
        ValueRef::Blob(value) => format!("<blob {} bytes>", value.len()),
    }
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn parquet_view(ui: &mut Ui, p: &Palette, path: &str, local: &Path) {
    let stamp = file_stamp(local);
    let id = Id::unique(("viewers-parquet", path, stamp));
    let data = match load_cached(ui.ctx(), id, || parquet_data(local)) {
        Loaded::Ready(data) => data,
        Loaded::Failed(error) => {
            error_view(ui, p, "Could not read Parquet file", &error);
            return;
        }
    };
    let meta = format!(
        "Parquet · {} columns · first {} rows",
        data.columns.len(),
        PARQUET_ROW_CAP
    );
    viewer_header(ui, p, "Parquet", &meta);
    if !data.columns.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(12.0);
            for column in &data.columns {
                let mut label = format!("{}: {}", column.name, column.type_name);
                if !column.converted.is_empty() {
                    label.push_str(&format!(" ({})", column.converted));
                }
                ui.add(
                    Label::new(
                        RichText::new(label)
                            .font(theme::font_mono(11.5))
                            .color(p.fg_muted),
                    )
                    .truncate(),
                );
                ui.add_space(12.0);
            }
        });
        surfaces::hairline(ui, p);
    }
    if data.rows.is_empty() {
        empty_note(ui, p, "This file has no rows.");
    }
    data_table(ui, p, ("parquet-page", path), &data.headers, &data.rows);
}

fn parquet_data(path: &Path) -> Result<ParquetData, String> {
    let file = File::open(path).map_err(|error| format!("Could not open the file: {error}"))?;
    let reader = SerializedFileReader::new(file)
        .map_err(|error| format!("Could not read the Parquet metadata: {error}"))?;
    let schema = reader.metadata().file_metadata().schema_descr();
    let columns: Vec<ParquetColumn> = schema
        .columns()
        .iter()
        .map(|column| {
            let converted = column.converted_type();
            ParquetColumn {
                name: column.name().to_string(),
                type_name: format!("{:?}", column.physical_type()),
                converted: if converted == ConvertedType::NONE {
                    String::new()
                } else {
                    format!("{converted:?}")
                },
            }
        })
        .collect();
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    let iterator = reader
        .get_row_iter(None)
        .map_err(|error| format!("Could not read rows: {error}"))?;
    for row in iterator.take(PARQUET_ROW_CAP) {
        let row = row.map_err(|error| format!("Could not read rows: {error}"))?;
        if headers.is_empty() {
            headers = row
                .get_column_iter()
                .map(|(name, _)| name.clone())
                .collect();
        }
        rows.push(
            row.get_column_iter()
                .map(|(_, field)| parquet_cell(field))
                .collect(),
        );
    }
    if headers.is_empty() {
        headers = columns.iter().map(|column| column.name.clone()).collect();
    }
    Ok(ParquetData {
        columns,
        headers,
        rows,
    })
}

fn parquet_cell(field: &Field) -> String {
    match field {
        Field::Null => "NULL".to_string(),
        Field::Str(value) => clip_cell(value),
        Field::Bytes(value) => format!("<{} bytes>", value.data().len()),
        Field::Group(value) => format!("<group {} fields>", value.len()),
        Field::ListInternal(value) => format!("<list {} items>", value.len()),
        Field::MapInternal(value) => format!("<map {} entries>", value.len()),
        other => clip_cell(&other.to_string()),
    }
}

fn zip_view(ui: &mut Ui, p: &Palette, path: &str, local: &Path) {
    let stamp = file_stamp(local);
    let id = Id::unique(("viewers-zip", path, stamp));
    let data = match load_cached(ui.ctx(), id, || zip_entries(local)) {
        Loaded::Ready(data) => data,
        Loaded::Failed(error) => {
            error_view(ui, p, "Could not read ZIP archive", &error);
            return;
        }
    };
    let total: u64 = data.entries.iter().map(|entry| entry.size).sum();
    let mut meta = format!(
        "ZIP · {} entries · {}",
        data.entries.len(),
        human_bytes(total)
    );
    if data.truncated {
        meta.push_str(&format!(" · first {ZIP_ENTRY_CAP}"));
    }
    viewer_header(ui, p, "ZIP archive", &meta);
    if data.entries.is_empty() {
        empty_note(ui, p, "This archive is empty.");
        return;
    }
    let selected_id = Id::unique(("viewers-zip-selected", path));
    let mut selected: Option<(usize, String)> = ui
        .ctx()
        .data(|data| data.get_temp::<Option<(usize, String)>>(selected_id))
        .flatten();
    if let Some((index, _)) = &selected
        && *index >= data.entries.len()
    {
        selected = None;
    }
    let available = ui.available_height();
    let list_height = if selected.is_some() {
        (available * 0.45).clamp(120.0, (available - 100.0).max(120.0))
    } else {
        available.max(80.0)
    };
    let mut clicked = None;
    let mut close = false;
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), list_height),
        Layout::top_down(Align::Min),
        |ui| {
            let widths = [
                Column::remainder().at_least(160.0).clip(true),
                Column::initial(84.0).at_least(64.0).clip(true),
                Column::initial(84.0).at_least(64.0).clip(true),
                Column::initial(146.0).at_least(110.0).clip(true),
            ];
            TableBuilder::new(ui)
                .id_salt(("viewers-zip-table", path))
                .striped(true)
                .resizable(true)
                .sense(Sense::click())
                .cell_layout(Layout::left_to_right(Align::Center))
                .columns(widths[0], 1)
                .columns(widths[1], 1)
                .columns(widths[2], 1)
                .columns(widths[3], 1)
                .header(TABLE_HEADER_HEIGHT, |mut header| {
                    for label in ["Name", "Size", "Compressed", "Modified"] {
                        header.col(|ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(label)
                                        .font(theme::font_semibold(11.5))
                                        .color(p.fg_muted),
                                )
                                .truncate(),
                            );
                        });
                    }
                })
                .body(|body| {
                    body.rows(TABLE_ROW_HEIGHT, data.entries.len(), |mut row| {
                        let index = row.index();
                        let entry = &data.entries[index];
                        if let Some((selected_index, _)) = &selected {
                            row.set_selected(*selected_index == index);
                        }
                        row.col(|ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(&entry.name)
                                        .font(theme::font_mono(11.5))
                                        .color(if entry.is_dir { p.fg_muted } else { p.body }),
                                )
                                .truncate(),
                            );
                        });
                        row.col(|ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(if entry.is_dir {
                                        String::new()
                                    } else {
                                        human_bytes(entry.size)
                                    })
                                    .font(theme::font(11.5))
                                    .color(p.fg_muted),
                                )
                                .truncate(),
                            );
                        });
                        row.col(|ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(if entry.is_dir {
                                        String::new()
                                    } else {
                                        human_bytes(entry.compressed)
                                    })
                                    .font(theme::font(11.5))
                                    .color(p.fg_muted),
                                )
                                .truncate(),
                            );
                        });
                        row.col(|ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(entry.modified.clone().unwrap_or_default())
                                        .font(theme::font(11.5))
                                        .color(p.fg_muted),
                                )
                                .truncate(),
                            );
                        });
                        if row.response().clicked() && !entry.is_dir {
                            clicked = Some((index, entry.name.clone()));
                        }
                    });
                });
        },
    );
    if let Some((index, name)) = clicked {
        if selected
            .as_ref()
            .is_some_and(|(current, _)| *current == index)
        {
            selected = None;
        } else {
            selected = Some((index, name));
        }
        ui.ctx()
            .data_mut(|data| data.insert_temp(selected_id, selected.clone()));
    }
    let Some((index, name)) = selected else {
        return;
    };
    surfaces::hairline(ui, p);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
    let close_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 16.0, rect.center().y),
        Vec2::splat(22.0),
    );
    let close_response = ui.interact(
        close_rect,
        ui.scope_id().with("zip-preview-close"),
        Sense::click(),
    );
    icons::paint(
        ui,
        Icon::Close,
        Rect::from_center_size(close_rect.center(), Vec2::splat(11.0)),
        if close_response.hovered() {
            p.fg
        } else {
            p.fg_muted
        },
    );
    if close_response.clicked() {
        close = true;
    }
    let galley = surfaces::truncated_galley(
        ui,
        name,
        theme::font_mono(12.0),
        p.fg_muted,
        (rect.width() - 56.0).max(40.0),
    );
    ui.painter().galley(
        Pos2::new(rect.left() + 12.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.fg_muted,
    );
    if close {
        ui.ctx()
            .data_mut(|data| data.insert_temp(selected_id, Option::<(usize, String)>::None));
        return;
    }
    let archive_index = data
        .entries
        .get(index)
        .map(|entry| entry.index)
        .unwrap_or(index);
    let preview_id = Id::unique(("viewers-zip-preview", path, archive_index, stamp));
    match load_cached(ui.ctx(), preview_id, || zip_preview(local, archive_index)) {
        Loaded::Ready(preview) => {
            if preview.binary {
                empty_note(
                    ui,
                    p,
                    &format!("Binary entry · {}", human_bytes(preview.size)),
                );
            } else {
                ScrollArea::both()
                    .id_salt(("viewers-zip-preview-scroll", path, index))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.add(
                                Label::new(
                                    RichText::new(&preview.text)
                                        .font(theme::font_mono(11.5))
                                        .color(p.body),
                                )
                                .selectable(true),
                            );
                        });
                        if preview.truncated {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new(format!(
                                        "Showing the first {}",
                                        human_bytes(ZIP_PREVIEW_BYTES as u64)
                                    ))
                                    .font(theme::font(12.0))
                                    .color(p.fg_muted),
                                );
                            });
                        }
                    });
            }
        }
        Loaded::Failed(error) => error_view(ui, p, "Could not read archive entry", &error),
    }
}

fn zip_entries(path: &Path) -> Result<ZipData, String> {
    let file = File::open(path).map_err(|error| format!("Could not open the archive: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Could not read the archive: {error}"))?;
    let truncated = archive.len() > ZIP_ENTRY_CAP;
    let mut entries = Vec::with_capacity(archive.len().min(ZIP_ENTRY_CAP));
    for index in 0..archive.len().min(ZIP_ENTRY_CAP) {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        entries.push(ZipEntryInfo {
            index,
            name: zip_entry_name(entry.name(), entry.is_dir()),
            size: entry.size(),
            compressed: entry.compressed_size(),
            modified: entry.last_modified().map(|value| value.to_string()),
            is_dir: entry.is_dir(),
        });
    }
    Ok(ZipData { entries, truncated })
}

fn zip_entry_name(name: &str, is_dir: bool) -> String {
    if is_dir && !name.ends_with('/') {
        format!("{name}/")
    } else {
        name.to_string()
    }
}

fn zip_preview(path: &Path, index: usize) -> Result<ZipPreview, String> {
    let file = File::open(path).map_err(|error| format!("Could not open the archive: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Could not read the archive: {error}"))?;
    let entry = archive
        .by_index(index)
        .map_err(|error| format!("Could not read the entry: {error}"))?;
    let size = entry.size();
    let mut bytes = Vec::new();
    entry
        .take((ZIP_PREVIEW_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read the entry: {error}"))?;
    let truncated = bytes.len() > ZIP_PREVIEW_BYTES;
    bytes.truncate(ZIP_PREVIEW_BYTES);
    let binary = bytes.iter().take(1024).any(|byte| *byte == 0);
    let text = if binary {
        None
    } else {
        decode_preview(&bytes, truncated)
    };
    Ok(ZipPreview {
        text: text.clone().unwrap_or_default(),
        binary: binary || text.is_none(),
        truncated,
        size,
    })
}

fn decode_preview(bytes: &[u8], truncated: bool) -> Option<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Some(text.to_string());
    }
    if !truncated {
        return None;
    }
    for cut in 1..=3 {
        let end = bytes.len().saturating_sub(cut);
        if let Ok(text) = std::str::from_utf8(&bytes[..end]) {
            return Some(text.to_string());
        }
    }
    None
}

fn notebook_view(ui: &mut Ui, p: &Palette, path: &str, local: &Path) {
    let stamp = file_stamp(local);
    let id = Id::unique(("viewers-notebook", path, stamp));
    let data = match load_cached(ui.ctx(), id, || notebook_data(local)) {
        Loaded::Ready(data) => data,
        Loaded::Failed(error) => {
            error_view(ui, p, "Could not read notebook", &error);
            return;
        }
    };
    let meta = format!("Jupyter · {} cells", data.len());
    viewer_header(ui, p, "Notebook", &meta);
    let mut markdown = Markdown::default();
    ScrollArea::vertical()
        .id_salt(("viewers-notebook-scroll", path))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let available = ui.available_width();
            let inner = available.clamp(240.0, 820.0);
            let side = ((available - inner) * 0.5).max(14.0);
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_max_width(inner);
                    ui.add_space(10.0);
                    for (index, cell) in data.iter().enumerate() {
                        match cell {
                            NotebookCell::Markdown(text) => {
                                markdown.show_at(
                                    ui,
                                    Id::unique(("viewers-notebook-md", path, index)),
                                    text,
                                    p,
                                );
                                ui.add_space(10.0);
                            }
                            NotebookCell::Code { source, outputs } => {
                                code_cell(ui, p, path, index, source, outputs);
                                ui.add_space(12.0);
                            }
                            NotebookCell::Raw(text) => {
                                ui.add(
                                    Label::new(
                                        RichText::new(text)
                                            .font(theme::font_mono(11.5))
                                            .color(p.fg_muted),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(10.0);
                            }
                        }
                    }
                    ui.add_space(20.0);
                });
            });
        });
}

fn code_cell(ui: &mut Ui, p: &Palette, path: &str, index: usize, source: &str, outputs: &[String]) {
    egui::Frame::new()
        .fill(p.code_bg)
        .corner_radius(theme::radius::TABLE)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                Label::new(
                    RichText::new(source)
                        .font(theme::font_mono(theme::TEXT_CODE))
                        .color(p.body),
                )
                .wrap()
                .selectable(true),
            );
        });
    ui.push_id(("viewers-notebook-output", path, index), |ui| {
        for output in outputs {
            egui::Frame::new()
                .fill(p.wash(p.fg, if p.dark { 7 } else { 6 }))
                .corner_radius(theme::radius::SM)
                .inner_margin(Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(
                        Label::new(
                            RichText::new(output)
                                .font(theme::font_mono(11.5))
                                .color(p.fg_muted),
                        )
                        .wrap()
                        .selectable(true),
                    );
                });
            ui.add_space(4.0);
        }
    });
}

fn notebook_data(path: &Path) -> Result<Vec<NotebookCell>, String> {
    let text = read_text_file(path, NOTEBOOK_BYTES)?;
    notebook_cells(&text)
}

fn notebook_cells(text: &str) -> Result<Vec<NotebookCell>, String> {
    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("Invalid notebook JSON: {error}"))?;
    let cells = root
        .get("cells")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "The notebook has no cells.".to_string())?;
    let mut result = Vec::new();
    for cell in cells.iter().take(NOTEBOOK_CELL_CAP) {
        let kind = cell
            .get("cell_type")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let source = truncate_text(
            &cell.get("source").and_then(json_text).unwrap_or_default(),
            400_000,
        );
        match kind {
            "markdown" => {
                if !source.trim().is_empty() {
                    result.push(NotebookCell::Markdown(source));
                }
            }
            "code" => {
                let outputs = cell
                    .get("outputs")
                    .and_then(|value| value.as_array())
                    .map(|outputs| {
                        outputs
                            .iter()
                            .filter_map(notebook_output)
                            .filter(|output| !output.trim().is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                result.push(NotebookCell::Code { source, outputs });
            }
            _ => {
                if !source.trim().is_empty() {
                    result.push(NotebookCell::Raw(source));
                }
            }
        }
    }
    Ok(result)
}

fn notebook_output(output: &serde_json::Value) -> Option<String> {
    output
        .get("text")
        .and_then(json_text)
        .or_else(|| {
            output
                .get("data")
                .and_then(|data| data.get("text/plain"))
                .and_then(json_text)
        })
        .map(|text| truncate_text(&text, DOC_TEXT_CAP))
}

fn json_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
                .join(""),
        ),
        _ => None,
    }
}

fn workbook_view(ui: &mut Ui, p: &Palette, path: &str, local: &Path) {
    let stamp = file_stamp(local);
    let names_id = Id::unique(("viewers-workbook-sheets", path, stamp));
    let names = match load_cached(ui.ctx(), names_id, || workbook_sheets(local)) {
        Loaded::Ready(names) => names,
        Loaded::Failed(error) => {
            error_view(ui, p, "Could not read spreadsheet", &error);
            return;
        }
    };
    if names.is_empty() {
        viewer_header(ui, p, "Spreadsheet", "no sheets");
        empty_note(ui, p, "This workbook has no sheets.");
        return;
    }
    let selected_id = Id::unique(("viewers-workbook-selected", path));
    let mut selected = selected_index(ui.ctx(), selected_id, names.len());
    let meta = format!("Spreadsheet · {} sheets", names.len());
    if let Some(index) = section_strip(ui, p, ("workbook", path), &names, &mut selected, &meta) {
        selected = index;
        ui.ctx()
            .data_mut(|data| data.insert_temp(selected_id, selected));
    }
    let sheet = &names[selected];
    let page_id = Id::unique(("viewers-workbook-page", path, sheet, stamp));
    match load_cached(ui.ctx(), page_id, || workbook_page(local, sheet)) {
        Loaded::Ready(page) => {
            if page.headers.is_empty() || page.rows.is_empty() {
                empty_note(ui, p, "This sheet is empty.");
            }
            data_table(
                ui,
                p,
                ("workbook-page", path, sheet),
                &page.headers,
                &page.rows,
            );
        }
        Loaded::Failed(error) => error_view(ui, p, "Could not read sheet", &error),
    }
}

fn workbook_sheets(path: &Path) -> Result<Vec<String>, String> {
    let workbook = open_workbook_auto(path)
        .map_err(|error| format!("Could not open the workbook: {error}"))?;
    Ok(workbook.sheet_names())
}

fn workbook_page(path: &Path, sheet: &str) -> Result<TablePage, String> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|error| format!("Could not open the workbook: {error}"))?;
    let range = workbook
        .worksheet_range(sheet)
        .map_err(|error| format!("Could not read {sheet}: {error}"))?;
    let width = range.width().min(OFFICE_COL_CAP);
    let headers: Vec<String> = (0..width).map(column_label).collect();
    let mut rows = Vec::new();
    for row in range.rows().take(OFFICE_ROW_CAP) {
        let mut cells = Vec::with_capacity(width);
        for column in 0..width {
            cells.push(
                row.get(column)
                    .map(|value| clip_cell(&value.to_string()))
                    .unwrap_or_default(),
            );
        }
        rows.push(cells);
    }
    Ok(TablePage { headers, rows })
}

fn column_label(index: usize) -> String {
    let mut value = index + 1;
    let mut label = String::new();
    while value > 0 {
        let remainder = (value - 1) % 26;
        label.insert(0, (b'A' + remainder as u8) as char);
        value = (value - 1) / 26;
    }
    label
}

fn docx_view(ui: &mut Ui, p: &Palette, path: &str, local: &Path) {
    let stamp = file_stamp(local);
    let id = Id::unique(("viewers-docx", path, stamp));
    let data = match load_cached(ui.ctx(), id, || docx_data(local)) {
        Loaded::Ready(data) => data,
        Loaded::Failed(error) => {
            error_view(ui, p, "Could not read Word document", &error);
            return;
        }
    };
    let meta = format!("Word · {} paragraphs", data.paragraphs.len());
    viewer_header(ui, p, "Word document", &meta);
    prose_view(ui, p, ("docx", path), &data.paragraphs, data.truncated);
}

fn docx_data(path: &Path) -> Result<DocData, String> {
    let bytes = zip_entry_bytes(path, "word/document.xml", OFFICE_XML_BYTES)?;
    let xml = String::from_utf8_lossy(&bytes);
    let paragraphs = extract_paragraphs(&xml)?;
    Ok(cap_document(paragraphs))
}

fn pptx_view(ui: &mut Ui, p: &Palette, path: &str, local: &Path) {
    let stamp = file_stamp(local);
    let id = Id::unique(("viewers-pptx", path, stamp));
    let data = match load_cached(ui.ctx(), id, || pptx_data(local)) {
        Loaded::Ready(data) => data,
        Loaded::Failed(error) => {
            error_view(ui, p, "Could not read presentation", &error);
            return;
        }
    };
    let meta = format!("PowerPoint · {} slides", data.slides.len());
    viewer_header(ui, p, "Presentation", &meta);
    ScrollArea::vertical()
        .id_salt(("viewers-pptx-scroll", path))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let available = ui.available_width();
            let inner = available.clamp(240.0, 820.0);
            let side = ((available - inner) * 0.5).max(14.0);
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_max_width(inner);
                    ui.add_space(12.0);
                    for (index, slide) in data.slides.iter().enumerate() {
                        ui.label(
                            RichText::new(format!("Slide {}", index + 1))
                                .font(theme::font_semibold(13.0))
                                .color(p.fg_muted),
                        );
                        ui.add_space(6.0);
                        if slide.is_empty() {
                            ui.label(
                                RichText::new("No text on this slide.")
                                    .font(theme::font(13.0))
                                    .color(p.fg_muted),
                            );
                        }
                        for paragraph in slide {
                            ui.add(
                                Label::new(
                                    RichText::new(paragraph)
                                        .font(theme::font(theme::TEXT_MARKDOWN))
                                        .color(p.body),
                                )
                                .wrap()
                                .selectable(true),
                            );
                            ui.add_space(5.0);
                        }
                        ui.add_space(14.0);
                    }
                    if data.truncated {
                        ui.label(
                            RichText::new("Showing the first slides that fit the preview limit.")
                                .font(theme::font(12.0))
                                .color(p.fg_muted),
                        );
                    }
                    ui.add_space(20.0);
                });
            });
        });
}

fn pptx_data(path: &Path) -> Result<PptxData, String> {
    let file = File::open(path).map_err(|error| format!("Could not open the file: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Could not read the archive: {error}"))?;
    let mut slides: Vec<(u32, usize)> = Vec::new();
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        if let Some(number) = slide_number(entry.name()) {
            slides.push((number, index));
        }
    }
    slides.sort_by_key(|(number, _)| *number);
    let mut result = Vec::new();
    let mut used = 0;
    let mut truncated = false;
    for (_, index) in slides {
        let Ok(mut entry) = archive.by_index(index) else {
            continue;
        };
        if entry.size() as usize > OFFICE_XML_BYTES {
            truncated = true;
            break;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read a slide: {error}"))?;
        let xml = String::from_utf8_lossy(&bytes);
        let paragraphs = extract_paragraphs(&xml)?;
        used += paragraphs
            .iter()
            .map(|paragraph| paragraph.len())
            .sum::<usize>();
        result.push(paragraphs);
        if used > DOC_TEXT_CAP {
            truncated = true;
            break;
        }
    }
    Ok(PptxData {
        slides: result,
        truncated,
    })
}

fn slide_number(name: &str) -> Option<u32> {
    name.strip_prefix("ppt/slides/slide")?
        .strip_suffix(".xml")?
        .parse()
        .ok()
}

fn prose_view(
    ui: &mut Ui,
    p: &Palette,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    paragraphs: &[String],
    truncated: bool,
) {
    if paragraphs.is_empty() {
        empty_note(ui, p, "This document has no visible text.");
        return;
    }
    ScrollArea::vertical()
        .id_salt(("viewers-prose-scroll", id_salt))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let available = ui.available_width();
            let inner = available.clamp(240.0, 760.0);
            let side = ((available - inner) * 0.5).max(14.0);
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_max_width(inner);
                    ui.add_space(14.0);
                    for paragraph in paragraphs {
                        ui.add(
                            Label::new(
                                RichText::new(paragraph)
                                    .font(theme::font(theme::TEXT_MARKDOWN))
                                    .color(p.body),
                            )
                            .wrap()
                            .selectable(true),
                        );
                        ui.add_space(10.0);
                    }
                    if truncated {
                        ui.label(
                            RichText::new("Showing the first part of the document.")
                                .font(theme::font(12.0))
                                .color(p.fg_muted),
                        );
                    }
                    ui.add_space(24.0);
                });
            });
        });
}

fn extract_paragraphs(xml: &str) -> Result<Vec<String>, String> {
    let mut reader = XmlReader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut paragraphs = Vec::new();
    let mut paragraph = String::new();
    let mut in_text = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "t" => in_text += 1,
                "tab" => paragraph.push('\t'),
                "br" => paragraph.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(event)) => match event.local_name().as_ref() {
                "tab" => paragraph.push('\t'),
                "br" => paragraph.push('\n'),
                _ => {}
            },
            Ok(Event::Text(event)) if in_text > 0 => {
                let content = event.xml_content(XmlVersion::Implicit1_0);
                match unescape(&content) {
                    Ok(text) => paragraph.push_str(&text),
                    Err(_) => paragraph.push_str(&content),
                }
            }
            Ok(Event::GeneralRef(event)) if in_text > 0 => {
                if let Ok(Some(character)) = event.resolve_char_ref() {
                    paragraph.push(character);
                } else if let Some(character) = named_entity(&event) {
                    paragraph.push(character);
                }
            }
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "t" => in_text = in_text.saturating_sub(1),
                "p" => {
                    let text = paragraph.trim();
                    if !text.is_empty() {
                        paragraphs.push(text.to_string());
                    }
                    paragraph.clear();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("Could not parse the document XML: {error}")),
            _ => {}
        }
    }
    let text = paragraph.trim();
    if !text.is_empty() {
        paragraphs.push(text.to_string());
    }
    Ok(paragraphs)
}

fn named_entity(reference: &str) -> Option<char> {
    match reference {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}

fn cap_document(paragraphs: Vec<String>) -> DocData {
    let mut result = Vec::new();
    let mut used = 0;
    let mut truncated = false;
    for paragraph in paragraphs {
        used += paragraph.len();
        if used > DOC_TEXT_CAP {
            truncated = true;
            break;
        }
        result.push(paragraph);
    }
    DocData {
        paragraphs: result,
        truncated,
    }
}

fn zip_entry_bytes(path: &Path, name: &str, limit: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| format!("Could not open the file: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Could not read the archive: {error}"))?;
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("The archive does not contain {name}."))?;
    if entry.size() as usize > limit {
        return Err(format!("{name} is too large to preview."));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {name}: {error}"))?;
    Ok(bytes)
}

fn read_text_file(path: &Path, limit: usize) -> Result<String, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("Could not read the file: {error}"))?;
    if metadata.len() as usize > limit {
        return Err(format!(
            "The file is {}; this preview supports up to {}.",
            human_bytes(metadata.len()),
            human_bytes(limit as u64)
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("Could not read the file: {error}"))?;
    String::from_utf8(bytes).map_err(|_| "The file is not valid UTF-8.".to_string())
}

fn viewer_header(ui: &mut Ui, p: &Palette, title: &str, meta: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
    ui.painter().text(
        Pos2::new(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        theme::font_semibold(13.0),
        p.fg,
    );
    if !meta.is_empty() {
        let galley = surfaces::truncated_galley(
            ui,
            meta.to_string(),
            theme::font(11.5),
            p.fg_muted,
            (rect.width() * 0.55).max(40.0),
        );
        ui.painter().galley(
            Pos2::new(
                rect.right() - 12.0 - galley.size().x,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            p.fg_muted,
        );
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.25,
        Stroke::new(0.5, surfaces::edge_muted(p)),
    );
}

fn section_strip(
    ui: &mut Ui,
    p: &Palette,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    labels: &[String],
    selected: &mut usize,
    meta: &str,
) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.25,
        Stroke::new(0.5, surfaces::edge_muted(p)),
    );
    let meta_galley = surfaces::truncated_galley(
        ui,
        meta.to_string(),
        theme::font(11.5),
        p.fg_muted,
        (rect.width() * 0.5).max(40.0),
    );
    let meta_width = if meta.is_empty() {
        0.0
    } else {
        meta_galley.size().x + 24.0
    };
    let strip_rect = Rect::from_min_max(
        rect.min,
        Pos2::new(
            (rect.right() - meta_width).max(rect.left() + 40.0),
            rect.bottom(),
        ),
    );
    let mut strip_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(strip_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    strip_ui.spacing_mut().item_spacing.x = 6.0;
    let mut chosen = None;
    ScrollArea::horizontal()
        .id_salt(("viewers-strip", id_salt))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, true])
        .show(&mut strip_ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                for (index, label) in labels.iter().enumerate() {
                    if widgets::chip(ui, label, None, *selected == index, p).clicked() {
                        chosen = Some(index);
                    }
                }
            });
        });
    if meta_width > 0.0 {
        ui.painter().galley(
            Pos2::new(
                rect.right() - meta_width + 12.0,
                rect.center().y - meta_galley.size().y * 0.5,
            ),
            meta_galley,
            p.fg_muted,
        );
    }
    chosen
}

fn data_table(
    ui: &mut Ui,
    p: &Palette,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    headers: &[String],
    rows: &[Vec<String>],
) {
    if headers.is_empty() {
        if rows.is_empty() {
            empty_note(ui, p, "No rows to show.");
        }
        return;
    }
    let salt = Id::unique(("viewers-data-table", id_salt));
    let column_width = if headers.len() > 8 { 132.0 } else { 168.0 };
    let content_width = (headers.len() as f32 * (column_width + 8.0)).max(ui.available_width());
    ScrollArea::horizontal()
        .id_salt(salt.with("scroll-x"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(content_width);
            TableBuilder::new(ui)
                .id_salt(salt)
                .striped(true)
                .resizable(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .columns(
                    Column::initial(column_width).at_least(72.0).clip(true),
                    headers.len(),
                )
                .header(TABLE_HEADER_HEIGHT, |mut header| {
                    for label in headers {
                        header.col(|ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(label)
                                        .font(theme::font_semibold(11.5))
                                        .color(p.fg_muted),
                                )
                                .truncate(),
                            );
                        });
                    }
                })
                .body(|body| {
                    body.rows(TABLE_ROW_HEIGHT, rows.len(), |mut row| {
                        let index = row.index();
                        let cells = &rows[index];
                        for column in 0..headers.len() {
                            let value = cells.get(column).map(String::as_str).unwrap_or_default();
                            row.col(|ui| {
                                ui.add(
                                    Label::new(
                                        RichText::new(value)
                                            .font(theme::font_mono(11.5))
                                            .color(p.body),
                                    )
                                    .truncate(),
                                );
                            });
                        }
                    });
                });
        });
}

fn empty_note(ui: &mut Ui, p: &Palette, message: &str) {
    surfaces::inline_empty(ui, p, message);
}

fn error_view(ui: &mut Ui, p: &Palette, title: &str, message: &str) {
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                icons::icon(ui, Icon::Warning, 14.0, p.status_text(p.red));
                ui.label(
                    RichText::new(title)
                        .font(theme::font_semibold(13.0))
                        .color(p.fg),
                );
            });
            ui.add_space(6.0);
            ui.add(
                Label::new(
                    RichText::new(message)
                        .font(theme::font(12.5))
                        .color(p.fg_muted),
                )
                .wrap(),
            );
        });
    });
}

fn human_bytes(bytes: u64) -> String {
    let value = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if value < 1024.0 * 1024.0 {
        format!("{:.1} KB", value / 1024.0)
    } else if value < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", value / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", value / (1024.0 * 1024.0 * 1024.0))
    }
}

fn clip_cell(value: &str) -> String {
    clip_text(value, CELL_CHARS)
}

fn clip_text(value: &str, limit: usize) -> String {
    let cleaned = value.replace(['\n', '\r', '\t'], " ");
    truncate_text(&cleaned, limit)
}

fn truncate_text(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        let mut clipped: String = value.chars().take(limit).collect();
        clipped.push('…');
        clipped
    }
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn file_stamp(path: &Path) -> (u64, u64) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos() as u64)
        .unwrap_or(0);
    (metadata.len(), modified)
}

enum Loaded<T> {
    Ready(Arc<T>),
    Failed(String),
}

impl<T> Clone for Loaded<T> {
    fn clone(&self) -> Self {
        match self {
            Loaded::Ready(value) => Loaded::Ready(value.clone()),
            Loaded::Failed(error) => Loaded::Failed(error.clone()),
        }
    }
}

fn load_cached<T: Send + Sync + 'static>(
    ctx: &egui::Context,
    id: Id,
    load: impl FnOnce() -> Result<T, String>,
) -> Loaded<T> {
    if let Some(value) = ctx.data(|data| data.get_temp::<Loaded<T>>(id)) {
        return value;
    }
    let value = match load() {
        Ok(value) => Loaded::Ready(Arc::new(value)),
        Err(error) => Loaded::Failed(error),
    };
    ctx.data_mut(|data| data.insert_temp(id, value.clone()));
    value
}

fn selected_index(ctx: &egui::Context, id: Id, count: usize) -> usize {
    let value = ctx.data(|data| data.get_temp::<usize>(id)).unwrap_or(0);
    if count == 0 { 0 } else { value.min(count - 1) }
}

struct TablePage {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

struct ParquetData {
    columns: Vec<ParquetColumn>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

struct ParquetColumn {
    name: String,
    type_name: String,
    converted: String,
}

struct ZipData {
    entries: Vec<ZipEntryInfo>,
    truncated: bool,
}

struct ZipEntryInfo {
    index: usize,
    name: String,
    size: u64,
    compressed: u64,
    modified: Option<String>,
    is_dir: bool,
}

struct ZipPreview {
    text: String,
    binary: bool,
    truncated: bool,
    size: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum NotebookCell {
    Markdown(String),
    Code {
        source: String,
        outputs: Vec<String>,
    },
    Raw(String),
}

struct DocData {
    paragraphs: Vec<String>,
    truncated: bool,
}

struct PptxData {
    slides: Vec<Vec<String>>,
    truncated: bool,
}

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(character);
            }
            continue;
        }
        match character {
            '"' if cell.is_empty() => quoted = true,
            ',' => row.push(std::mem::take(&mut cell)),
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            _ => cell.push(character),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

fn unique_export_path(path: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("export");
    let mut candidate = parent.join(format!("{stem}.xlsx"));
    let mut suffix = 1;
    while exists(&candidate) {
        candidate = parent.join(format!("{stem}-{suffix}.xlsx"));
        suffix += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rchamber-viewers-{label}-{}-{stamp}-{count}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn extension_detection_supports_native_formats() {
        assert!(supported("data.db"));
        assert!(supported("DATA.SQLITE3"));
        assert!(supported("rows.parquet"));
        assert!(supported("bundle.zip"));
        assert!(supported("analysis.ipynb"));
        assert!(supported("book.xlsx"));
        assert!(supported("slides.pptx"));
        assert!(!supported("notes.md"));
        assert!(!supported("data.csv"));
    }

    #[test]
    fn zip_entry_label_parts_stay_readable() {
        let entry = ZipEntryInfo {
            index: 3,
            name: "docs/readme.txt".to_string(),
            size: 1536,
            compressed: 512,
            modified: Some("2024-05-01 12:30:00".to_string()),
            is_dir: false,
        };
        assert_eq!(entry.name, "docs/readme.txt");
        assert_eq!(human_bytes(entry.size), "1.5 KB");
        assert_eq!(human_bytes(entry.compressed), "512 B");
        assert_eq!(entry.modified.as_deref(), Some("2024-05-01 12:30:00"));
        assert_eq!(zip_entry_name("docs", true), "docs/");
        assert_eq!(zip_entry_name("docs/", true), "docs/");
        assert_eq!(zip_entry_name("docs/readme.txt", false), "docs/readme.txt");
    }

    #[test]
    fn notebook_extraction_reads_markdown_code_and_outputs() {
        let notebook = r##"{
            "cells": [
                {"cell_type": "markdown", "source": ["# Title\n", "Body"]},
                {"cell_type": "code", "source": ["print('hi')\n"], "outputs": [
                    {"output_type": "stream", "text": ["hi\n", "there"]},
                    {"output_type": "display_data", "data": {"text/plain": ["<obj>"]}}
                ]}
            ]
        }"##;
        let cells = notebook_cells(notebook).unwrap();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0], NotebookCell::Markdown("# Title\nBody".into()));
        match &cells[1] {
            NotebookCell::Code { source, outputs } => {
                assert_eq!(source, "print('hi')\n");
                assert_eq!(outputs, &vec!["hi\nthere".to_string(), "<obj>".to_string()]);
            }
            _ => panic!("expected a code cell"),
        }
    }

    #[test]
    fn docx_extraction_keeps_paragraphs_and_entities() {
        let xml = r#"<w:document><w:body>
            <w:p><w:r><w:t>Hello &amp; world</w:t></w:r></w:p>
            <w:p><w:r><w:t>Second</w:t></w:r><w:r><w:tab/><w:t>line</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let paragraphs = extract_paragraphs(xml).unwrap();
        assert_eq!(paragraphs, vec!["Hello & world", "Second\tline"]);
    }

    #[test]
    fn csv_export_targets_avoid_overwriting() {
        let path = Path::new("/tmp/report.csv");
        let fresh = unique_export_path(path, |_| false);
        assert_eq!(fresh, PathBuf::from("/tmp/report.xlsx"));
        let next = unique_export_path(path, |candidate| candidate == Path::new("/tmp/report.xlsx"));
        assert_eq!(next, PathBuf::from("/tmp/report-1.xlsx"));
        let later = unique_export_path(path, |candidate| {
            candidate == Path::new("/tmp/report.xlsx")
                || candidate == Path::new("/tmp/report-1.xlsx")
        });
        assert_eq!(later, PathBuf::from("/tmp/report-2.xlsx"));
    }

    #[test]
    fn csv_parser_handles_quotes_and_newlines() {
        let rows = parse_csv("a,b\n\"one, two\",\"line\nthree\"\n");
        assert_eq!(
            rows,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["one, two".to_string(), "line\nthree".to_string()],
            ]
        );
    }

    #[test]
    fn column_labels_follow_sheet_order() {
        assert_eq!(column_label(0), "A");
        assert_eq!(column_label(25), "Z");
        assert_eq!(column_label(26), "AA");
        assert_eq!(column_label(59), "BH");
    }

    #[test]
    fn data_tables_render_without_panicking() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let headers: Vec<String> = (0..60).map(column_label).collect();
        let rows: Vec<Vec<String>> = (0..300)
            .map(|row| {
                (0..60)
                    .map(|column| format!("cell {row}/{column}"))
                    .collect()
            })
            .collect();
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            ui.set_min_height(620.0);
            data_table(ui, &p, ("smoke", 0), &headers, &rows);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn viewer_entry_points_render_a_frame() {
        let directory = temp_dir("show");
        let database = directory.join("data.db");
        {
            let connection = Connection::open(&database).unwrap();
            connection
                .execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT);")
                .unwrap();
        }
        let archive = directory.join("bundle.zip");
        {
            let file = File::create(&archive).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("hello.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"hello").unwrap();
            writer.finish().unwrap();
        }

        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let p = Palette::dark();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        for path in [&database, &archive] {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let name = path.to_string_lossy().to_string();
            let mut output = ctx.run_ui(input, |ui| {
                ui.set_min_height(620.0);
                show(ui, &p, &name, path);
            });
            output.textures_delta.clear();
        }
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn parquet_reader_reads_schema_and_rows() {
        use parquet::data_type::{ByteArray, ByteArrayType, Int32Type};
        use parquet::file::writer::SerializedFileWriter;
        use parquet::schema::parser::parse_message_type;

        let directory = temp_dir("parquet");
        let path = directory.join("rows.parquet");
        let schema = Arc::new(
            parse_message_type(
                "message schema { REQUIRED INT32 id; REQUIRED BYTE_ARRAY name (UTF8); }",
            )
            .unwrap(),
        );
        let file = File::create(&path).unwrap();
        let mut writer = SerializedFileWriter::new(file, schema, Default::default()).unwrap();
        let mut group = writer.next_row_group().unwrap();
        let mut column = group.next_column().unwrap().unwrap();
        column
            .typed::<Int32Type>()
            .write_batch(&[1, 2], None, None)
            .unwrap();
        column.close().unwrap();
        let mut column = group.next_column().unwrap().unwrap();
        column
            .typed::<ByteArrayType>()
            .write_batch(
                &[ByteArray::from("alpha"), ByteArray::from("beta")],
                None,
                None,
            )
            .unwrap();
        column.close().unwrap();
        group.close().unwrap();
        writer.close().unwrap();

        let data = parquet_data(&path).unwrap();
        assert_eq!(data.columns.len(), 2);
        assert_eq!(data.columns[0].name, "id");
        assert_eq!(data.headers, vec!["id", "name"]);
        assert_eq!(
            data.rows,
            vec![
                vec!["1".to_string(), "alpha".to_string()],
                vec!["2".to_string(), "beta".to_string()],
            ]
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn csv_export_writes_sibling_workbooks_without_overwriting() {
        let directory = temp_dir("csv");
        let path = directory.join("report.csv");
        std::fs::write(&path, "name,count\nalpha,42\n").unwrap();
        let first = export_csv(&path).unwrap();
        assert_eq!(first, directory.join("report.xlsx"));
        assert!(first.is_file());
        let second = export_csv(&path).unwrap();
        assert_eq!(second, directory.join("report-1.xlsx"));
        assert!(second.is_file());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn sqlite_reader_lists_tables_and_rows() {
        let directory = temp_dir("sqlite");
        let path = directory.join("data.db");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT);\
                     INSERT INTO notes (body) VALUES ('one'), ('two');",
                )
                .unwrap();
        }
        let tables = sqlite_tables(&path).unwrap();
        assert_eq!(tables, vec!["notes"]);
        let page = sqlite_page(&path, "notes").unwrap();
        assert_eq!(page.headers, vec!["id", "body"]);
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[1][1], "two");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn zip_reader_lists_and_previews_entries() {
        let directory = temp_dir("zip");
        let path = directory.join("bundle.zip");
        {
            let file = File::create(&path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("docs/hello.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"hello from the archive").unwrap();
            writer.finish().unwrap();
        }
        let data = zip_entries(&path).unwrap();
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].name, "docs/hello.txt");
        let preview = zip_preview(&path, data.entries[0].index).unwrap();
        assert!(!preview.binary);
        assert_eq!(preview.text, "hello from the archive");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn workbook_reader_reads_sheets_and_cells() {
        let directory = temp_dir("xlsx");
        let path = directory.join("book.xlsx");
        {
            let mut workbook = rust_xlsxwriter::Workbook::new();
            let sheet = workbook.add_worksheet();
            sheet.write_string(0, 0, "Name").unwrap();
            sheet.write_string(0, 1, "Count").unwrap();
            sheet.write_string(1, 0, "alpha").unwrap();
            sheet.write_number(1, 1, 42).unwrap();
            workbook.save(&path).unwrap();
        }
        let sheets = workbook_sheets(&path).unwrap();
        assert_eq!(sheets.len(), 1);
        let page = workbook_page(&path, &sheets[0]).unwrap();
        assert_eq!(page.headers, vec!["A", "B"]);
        assert_eq!(page.rows[0], vec!["Name", "Count"]);
        assert_eq!(page.rows[1], vec!["alpha", "42"]);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn pptx_reader_orders_slides_numerically() {
        let directory = temp_dir("pptx");
        let path = directory.join("slides.pptx");
        {
            let file = File::create(&path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            for (slide, text) in [(10, "Ten"), (2, "Two")] {
                writer
                    .start_file(
                        format!("ppt/slides/slide{slide}.xml"),
                        zip::write::SimpleFileOptions::default(),
                    )
                    .unwrap();
                writer
                    .write_all(
                        format!(
                            "<p:sld><p:cSld><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:cSld></p:sld>"
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            }
            writer.finish().unwrap();
        }
        let data = pptx_data(&path).unwrap();
        assert_eq!(
            data.slides,
            vec![vec!["Two".to_string()], vec!["Ten".to_string()]]
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn docx_reader_reads_visible_text_from_the_archive() {
        let directory = temp_dir("docx");
        let path = directory.join("document.docx");
        {
            let file = File::create(&path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file(
                    "word/document.xml",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer
                .write_all(
                    br#"<w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:p><w:r><w:t>World</w:t></w:r></w:p></w:body></w:document>"#,
                )
                .unwrap();
            writer.finish().unwrap();
        }
        let data = docx_data(&path).unwrap();
        assert_eq!(data.paragraphs, vec!["Hello", "World"]);
        let _ = std::fs::remove_dir_all(directory);
    }
}
