use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use rchamber_api::PromptFile;

pub const MAX_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_INLINE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_OFFICE_TEXT_CHARS: usize = 500_000;
const MAX_OFFICE_IMAGES: usize = 50;
const MAX_OFFICE_IMAGE_TOTAL: u64 = 40 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct AttachmentItem {
    pub file: PromptFile,
    pub source_document_id: Option<String>,
    pub size: u64,
}

pub fn prepare_path(path: &Path, _workspace: Option<&str>) -> Result<Vec<AttachmentItem>, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.is_dir() {
        return Err(format!(
            "{} is a folder; mention it instead",
            path.display()
        ));
    }
    if metadata.len() > MAX_ATTACHMENT_BYTES {
        return Err(format!(
            "{} is larger than 20 MB",
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string())
        ));
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".into());
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();

    if matches!(
        extension.as_str(),
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp"
    ) {
        return prepare_office(path, &name, &extension);
    }

    if matches!(extension.as_str(), "heic" | "heif") {
        return prepare_heic(path, &name, metadata.len());
    }

    let mime = mime_for(path);
    prepare_regular(path, &name, mime, metadata.len())
}

pub fn prepare_paths(
    paths: &[PathBuf],
    workspace: Option<&str>,
) -> (Vec<AttachmentItem>, Vec<String>) {
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for path in paths {
        match prepare_path(path, workspace) {
            Ok(mut prepared) => items.append(&mut prepared),
            Err(error) => errors.push(error),
        }
    }
    (items, errors)
}

fn prepare_regular(
    path: &Path,
    name: &str,
    mime: &str,
    size: u64,
) -> Result<Vec<AttachmentItem>, String> {
    if size <= MAX_INLINE_BYTES {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        return Ok(vec![AttachmentItem {
            file: PromptFile::inline(data, mime, Some(name.to_string()), None),
            source_document_id: None,
            size,
        }]);
    }
    let uri = file_url(path)?;
    Ok(vec![AttachmentItem {
        file: PromptFile::path(uri, Some(name.to_string()), mime, None),
        source_document_id: None,
        size,
    }])
}

fn prepare_heic(path: &Path, name: &str, size: u64) -> Result<Vec<AttachmentItem>, String> {
    let converted = convert_heic(path);
    let jpeg_name = name
        .strip_suffix(".heic")
        .or_else(|| name.strip_suffix(".HEIC"))
        .or_else(|| name.strip_suffix(".heif"))
        .or_else(|| name.strip_suffix(".HEIF"))
        .unwrap_or(name)
        .to_string()
        + ".jpg";
    match converted {
        Ok(bytes) if !bytes.is_empty() && bytes.len() as u64 <= MAX_ATTACHMENT_BYTES => {
            let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(vec![AttachmentItem {
                file: PromptFile::inline(
                    data,
                    "image/jpeg",
                    Some(jpeg_name),
                    Some("Converted from HEIC".into()),
                ),
                source_document_id: None,
                size: bytes.len() as u64,
            }])
        }
        _ => {
            let uri = file_url(path)?;
            Ok(vec![AttachmentItem {
                file: PromptFile::path(uri, Some(name.to_string()), "image/heic", None),
                source_document_id: None,
                size,
            }])
        }
    }
}

fn convert_heic(path: &Path) -> Result<Vec<u8>, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let output =
        std::env::temp_dir().join(format!("rchamber-heic-{}-{stamp}.jpg", std::process::id()));
    let attempts: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[(
            "sips",
            &["-s", "format", "jpeg", "-s", "formatOptions", "90"],
        )]
    } else {
        &[
            ("magick", &["-quality", "90"]),
            ("convert", &["-quality", "90"]),
            ("heif-convert", &[]),
            ("ffmpeg", &["-y", "-loglevel", "error"]),
        ]
    };
    for (program, args) in attempts {
        let mut command = std::process::Command::new(program);
        if *program == "sips" {
            for arg in *args {
                command.arg(arg);
            }
            command.arg(path).arg("--out").arg(&output);
        } else {
            for arg in *args {
                command.arg(arg);
            }
            command.arg(path).arg(&output);
        }
        let ok = command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if ok && let Ok(bytes) = std::fs::read(&output) {
            let _ = std::fs::remove_file(&output);
            return Ok(bytes);
        }
    }
    let _ = std::fs::remove_file(&output);
    Err("HEIC conversion failed".into())
}

fn prepare_office(
    path: &Path,
    original_name: &str,
    extension: &str,
) -> Result<Vec<AttachmentItem>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut names = Vec::new();
    for index in 0..archive.len() {
        if let Ok(entry) = archive.by_index(index) {
            names.push(entry.name().to_string());
        }
    }
    let text = match extension {
        "docx" | "odt" => read_office_text(&mut archive, &["word/document.xml", "content.xml"]),
        "xlsx" | "ods" => {
            let mut entries: Vec<String> = vec![
                "xl/sharedStrings.xml".to_string(),
                "content.xml".to_string(),
            ];
            let mut sheets: Vec<String> = names
                .iter()
                .filter(|name| name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
                .cloned()
                .collect();
            sheets.sort();
            entries.extend(sheets);
            read_office_text(
                &mut archive,
                &entries.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        }
        "pptx" | "odp" => {
            let mut entries: Vec<String> = names
                .iter()
                .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
                .cloned()
                .collect();
            entries.sort();
            read_office_text(
                &mut archive,
                &entries.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        }
        _ => String::new(),
    };
    let text = truncate_chars(&text, MAX_OFFICE_TEXT_CHARS);
    let mut items = Vec::new();
    let source_document_id = format!(
        "doc-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default(),
        original_name
    );
    let mut image_names = Vec::new();
    let mut total_images = 0u64;
    for name in &names {
        let lower = name.to_lowercase();
        let is_image = lower.starts_with("word/media/")
            || lower.starts_with("xl/media/")
            || lower.starts_with("ppt/media/")
            || lower.starts_with("pictures/");
        if !is_image {
            continue;
        }
        let image_mime = mime_for(Path::new(name));
        if !image_mime.starts_with("image/") {
            continue;
        }
        if items.len() >= MAX_OFFICE_IMAGES || total_images >= MAX_OFFICE_IMAGE_TOTAL {
            break;
        }
        let Ok(mut entry) = archive.by_name(name) else {
            continue;
        };
        if entry.size() > MAX_ATTACHMENT_BYTES {
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        if entry.read_to_end(&mut bytes).is_err() {
            continue;
        }
        if total_images + bytes.len() as u64 > MAX_OFFICE_IMAGE_TOTAL {
            continue;
        }
        total_images += bytes.len() as u64;
        let image_name = Path::new(name)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".into());
        image_names.push(image_name.clone());
        items.push(AttachmentItem {
            file: PromptFile::inline(
                base64::engine::general_purpose::STANDARD.encode(&bytes),
                image_mime,
                Some(image_name),
                Some(format!("Embedded in {original_name}")),
            ),
            source_document_id: Some(source_document_id.clone()),
            size: bytes.len() as u64,
        });
    }
    if text.trim().is_empty() && items.is_empty() {
        return Err(format!(
            "{original_name} does not contain readable text or images"
        ));
    }
    let mut composed = format!("[{original_name}]\n{text}");
    if !image_names.is_empty() {
        composed.push_str("\n\n[Embedded images]\n");
        for name in &image_names {
            composed.push_str(&format!("- [{name}]\n"));
        }
    }
    let text_bytes = composed.into_bytes();
    items.insert(
        0,
        AttachmentItem {
            file: PromptFile::inline(
                base64::engine::general_purpose::STANDARD.encode(&text_bytes),
                "text/plain",
                Some(original_name.to_string()),
                Some("Extracted document text".into()),
            ),
            source_document_id: Some(source_document_id),
            size: text_bytes.len() as u64,
        },
    );
    Ok(items)
}

fn read_office_text(archive: &mut zip::ZipArchive<File>, names: &[&str]) -> String {
    let mut output = String::new();
    for name in names {
        let Ok(mut entry) = archive.by_name(name) else {
            continue;
        };
        if entry.size() > 8 * 1024 * 1024 {
            continue;
        }
        let mut xml = String::new();
        if entry.read_to_string(&mut xml).is_err() {
            continue;
        }
        let text = xml_text(&xml);
        if !text.trim().is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(text.trim());
        }
    }
    output
}

pub fn xml_text(xml: &str) -> String {
    let mut output = String::new();
    let mut tag = String::new();
    let mut text = String::new();
    let mut in_tag = false;
    for character in xml.chars() {
        if in_tag {
            if character == '>' {
                let tag_name = tag
                    .trim()
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or_default();
                let closing = tag.trim_start().starts_with('/');
                if closing
                    && matches!(
                        tag_name,
                        "w:p" | "a:p" | "w:tr" | "row" | "si" | "text:p" | "w:tc" | "c"
                    )
                {
                    output.push(
                        if tag_name == "w:tc" || tag_name == "c" || tag_name == "row" {
                            '\t'
                        } else {
                            '\n'
                        },
                    );
                } else if tag_name == "w:tab" {
                    output.push('\t');
                } else if tag_name == "w:br" || tag_name == "a:br" {
                    output.push('\n');
                }
                in_tag = false;
                tag.clear();
            } else {
                tag.push(character);
            }
        } else if character == '<' {
            flush_text(&mut output, &mut text);
            in_tag = true;
            tag.clear();
        } else {
            text.push(character);
        }
    }
    flush_text(&mut output, &mut text);
    normalize_text(&output)
}

fn flush_text(output: &mut String, text: &mut String) {
    if text.is_empty() {
        return;
    }
    output.push_str(&decode_entities(text));
    text.clear();
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn normalize_text(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.replace('\t', " ");
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() && lines.last().is_some_and(|last: &String| last.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut output: String = text.chars().take(max).collect();
    output.push_str("\n[Document text truncated]");
    output
}

fn file_url(path: &Path) -> Result<String, String> {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| format!("Could not build a file URL for {}", path.display()))
}

pub fn mime_for(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "odp" => "application/vnd.oasis.opendocument.presentation",
        "json" | "jsonc" => "application/json",
        "xml" => "application/xml",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "md" | "markdown" | "txt" | "log" | "ini" | "cfg" | "conf" | "env" | "rs" | "py" | "js"
        | "jsx" | "ts" | "tsx" | "go" | "java" | "c" | "h" | "cpp" | "hpp" | "cs" | "rb"
        | "php" | "sh" | "bash" | "zsh" | "fish" | "sql" | "graphql" | "gql" | "lock"
        | "gitignore" | "dockerfile" | "makefile" => "text/plain",
        _ => {
            let path = path.to_path_buf();
            if let Ok(mut file) = File::open(&path) {
                let mut buffer = [0u8; 4096];
                let count = file.read(&mut buffer).unwrap_or(0);
                let slice = &buffer[..count];
                if !slice.contains(&0) && std::str::from_utf8(slice).is_ok() {
                    return "text/plain";
                }
            }
            "application/octet-stream"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_docx_text() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:p><w:r><w:t>World</w:t></w:r></w:p></w:body></w:document>"#;
        assert_eq!(xml_text(xml), "Hello\nWorld");
    }

    #[test]
    fn inlines_small_files_and_uses_paths_for_large_files() {
        use rchamber_api::PromptFileSource;

        let directory =
            std::env::temp_dir().join(format!("rchamber-attachment-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let small = directory.join("note.txt");
        std::fs::write(&small, b"hello").unwrap();
        let items = prepare_path(&small, None).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].file.source, PromptFileSource::Inline));
        assert!(!items[0].file.data.is_empty());

        let large = directory.join("large.txt");
        std::fs::write(&large, vec![b'a'; (MAX_INLINE_BYTES + 1) as usize]).unwrap();
        let items = prepare_path(&large, None).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].file.source, PromptFileSource::Uri { .. }));
        assert!(items[0].file.data.is_empty());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn extracts_a_docx_archive_with_images() {
        use rchamber_api::PromptFileSource;

        let directory =
            std::env::temp_dir().join(format!("rchamber-office-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("note.docx");
        let file = File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("word/document.xml", options).unwrap();
        writer
            .write_all(
                br#"<w:document><w:body><w:p><w:r><w:t>Hello Office</w:t></w:r></w:p></w:body></w:document>"#,
            )
            .unwrap();
        writer.start_file("word/media/pixel.png", options).unwrap();
        writer
            .write_all(&[
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
                0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
                0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
                0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
                0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
            ])
            .unwrap();
        writer.finish().unwrap();

        let items = prepare_path(&path, None).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].source_document_id.as_deref(),
            items[1].source_document_id.as_deref()
        );
        let text = base64::engine::general_purpose::STANDARD
            .decode(&items[0].file.data)
            .unwrap();
        assert!(String::from_utf8_lossy(&text).contains("Hello Office"));
        assert!(matches!(items[0].file.source, PromptFileSource::Inline));
        assert!(matches!(items[1].file.source, PromptFileSource::Inline));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn serializes_open_code_file_attachments() {
        let inline = PromptFile::inline(
            "aGVsbG8=".into(),
            "text/plain",
            Some("note.txt".into()),
            None,
        );
        let value = serde_json::to_value(&inline).unwrap();
        assert_eq!(value["data"], "aGVsbG8=");
        assert_eq!(value["source"]["type"], "inline");

        let path = PromptFile::path(
            "file:///tmp/note.txt",
            Some("note.txt".into()),
            "text/plain",
            None,
        );
        let value = serde_json::to_value(&path).unwrap();
        assert_eq!(value["data"], "");
        assert_eq!(value["source"]["type"], "uri");
        assert_eq!(value["source"]["uri"], "file:///tmp/note.txt");
    }

    #[test]
    fn maps_office_mimes() {
        assert_eq!(
            mime_for(Path::new("report.docx")),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(mime_for(Path::new("photo.heic")), "image/heic");
    }
}
