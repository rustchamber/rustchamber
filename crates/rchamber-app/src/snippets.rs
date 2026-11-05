use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Snippet {
    pub name: String,
    pub content: String,
    pub aliases: Vec<String>,
    pub description: Option<String>,
    pub file_path: String,
    pub source: String,
}

pub fn load(workspace: Option<&str>) -> Vec<Snippet> {
    let mut canonical: HashMap<String, Snippet> = HashMap::new();
    for (directory, source) in directories(workspace) {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            let Some(snippet) = read_snippet(&path, source) else {
                continue;
            };
            canonical.insert(snippet.name.to_lowercase(), snippet);
        }
    }
    let mut snippets: Vec<Snippet> = canonical.into_values().collect();
    snippets.sort_by(|left, right| left.name.cmp(&right.name));
    snippets
}

pub fn expand(text: &str, workspace: Option<&str>) -> String {
    let snippets = load(workspace);
    expand_with(text, &snippets)
}

pub fn expand_with(text: &str, snippets: &[Snippet]) -> String {
    let registry = registry(snippets);
    let mut counts = HashMap::new();
    let mut prepend = Vec::new();
    let mut append = Vec::new();
    let inline = expand_text(text, &registry, &mut counts, &mut prepend, &mut append, 0)
        .trim()
        .to_string();
    let mut parts = prepend;
    if !inline.is_empty() {
        parts.push(inline);
    }
    parts.extend(append);
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn directories(workspace: Option<&str>) -> Vec<(PathBuf, &'static str)> {
    let home = directories::BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .unwrap_or_default();
    let config = std::env::var_os("OPENCODE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| Some(home.join(".config")))
                .map(|base| base.join("opencode"))
        })
        .unwrap_or_else(|| home.join(".config").join("opencode"));
    let mut result = vec![
        (config.join("snippets"), "global"),
        (config.join("snippet"), "global"),
    ];
    if let Some(workspace) = workspace.filter(|value| !value.is_empty()) {
        result.push((Path::new(workspace).join(".opencode/snippets"), "project"));
        result.push((Path::new(workspace).join(".opencode/snippet"), "project"));
    }
    result
}

fn read_snippet(path: &Path, source: &'static str) -> Option<Snippet> {
    let name = path.file_stem()?.to_str()?.to_string();
    if name.is_empty()
        || name.len() > 80
        || !name
            .chars()
            .next()
            .is_some_and(|value| value.is_ascii_alphanumeric())
        || !name
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '_' || value == '-')
    {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let (frontmatter, body) = split_frontmatter(&content);
    let mut aliases = Vec::new();
    let mut description = None;
    let mut in_aliases = false;
    for raw in frontmatter.lines() {
        let line = raw.trim();
        if let Some(value) = line
            .strip_prefix("aliases:")
            .or_else(|| line.strip_prefix("alias:"))
        {
            in_aliases = true;
            let value = value.trim();
            if value.starts_with('[') {
                for alias in value.trim_matches(['[', ']']).split(',') {
                    push_alias(&mut aliases, alias);
                }
            } else {
                push_alias(&mut aliases, value);
            }
            continue;
        }
        if in_aliases {
            if let Some(value) = line.strip_prefix("- ") {
                push_alias(&mut aliases, value);
                continue;
            }
            if line.is_empty() {
                continue;
            }
            in_aliases = false;
        }
        if let Some(value) = line.strip_prefix("description:") {
            let value = clean(value);
            if !value.is_empty() {
                description = Some(value);
            }
        }
    }
    Some(Snippet {
        name,
        content: body.trim().to_string(),
        aliases,
        description,
        file_path: path.display().to_string(),
        source: source.to_string(),
    })
}

fn split_frontmatter(content: &str) -> (&str, &str) {
    let Some(rest) = content.strip_prefix("---") else {
        return ("", content);
    };
    let rest = rest.trim_start_matches(['\r', '\n']);
    let Some(end) = rest.find("\n---") else {
        return ("", content);
    };
    let frontmatter = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    (frontmatter, body)
}

fn clean(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn push_alias(aliases: &mut Vec<String>, value: &str) {
    let value = clean(value);
    if !value.is_empty()
        && !aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(&value))
    {
        aliases.push(value);
    }
}

fn registry(snippets: &[Snippet]) -> HashMap<String, Snippet> {
    let mut registry = HashMap::new();
    for snippet in snippets {
        registry.insert(snippet.name.to_lowercase(), snippet.clone());
    }
    for snippet in snippets {
        for alias in &snippet.aliases {
            registry
                .entry(alias.to_lowercase())
                .or_insert_with(|| snippet.clone());
        }
    }
    registry
}

fn expand_text(
    text: &str,
    registry: &HashMap<String, Snippet>,
    counts: &mut HashMap<String, usize>,
    prepend: &mut Vec<String>,
    append: &mut Vec<String>,
    depth: usize,
) -> String {
    if depth > 16 {
        return text.to_string();
    }
    let mut output = String::new();
    let mut index = 0;
    while index < text.len() {
        let Some(character) = text[index..].chars().next() else {
            break;
        };
        if character != '#' {
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        let start = index;
        let mut end = index + 1;
        let mut name = String::new();
        while end < text.len() {
            let Some(character) = text[end..].chars().next() else {
                break;
            };
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                name.push(character);
                end += character.len_utf8();
            } else {
                break;
            }
        }
        if name.is_empty() {
            output.push('#');
            index += 1;
            continue;
        }
        if name.eq_ignore_ascii_case("skill") && text[end..].starts_with('(') {
            output.push('#');
            index += 1;
            continue;
        }
        let key = name.to_lowercase();
        let Some(snippet) = registry.get(&key) else {
            output.push_str(&text[start..end]);
            index = end;
            continue;
        };
        let count = counts.entry(key.clone()).or_insert(0);
        *count += 1;
        if *count > 15 {
            output.push_str(&text[start..end]);
            index = end;
            continue;
        }
        let (inline, mut pre, mut post) = parse_blocks(&snippet.content);
        for block in pre.drain(..) {
            let expanded = expand_text(&block, registry, counts, prepend, append, depth + 1);
            prepend.push(expanded);
        }
        for block in post.drain(..) {
            let expanded = expand_text(&block, registry, counts, prepend, append, depth + 1);
            append.push(expanded);
        }
        output.push_str(&expand_text(
            &inline,
            registry,
            counts,
            prepend,
            append,
            depth + 1,
        ));
        index = end;
    }
    output
}

fn parse_blocks(content: &str) -> (String, Vec<String>, Vec<String>) {
    let mut inline = content.to_string();
    let mut prepend = Vec::new();
    let mut append = Vec::new();
    extract_tag(&mut inline, "prepend", &mut prepend);
    extract_tag(&mut inline, "append", &mut append);
    remove_tag(&mut inline, "inject");
    (inline.trim().to_string(), prepend, append)
}

fn extract_tag(text: &mut String, tag: &str, out: &mut Vec<String>) {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    while let Some(start) = text.find(&open) {
        let value_start = start + open.len();
        let Some(relative_end) = text[value_start..].find(&close) else {
            let value = text[value_start..].trim().to_string();
            if !value.is_empty() {
                out.push(value);
            }
            text.truncate(start);
            break;
        };
        let value = text[value_start..value_start + relative_end]
            .trim()
            .to_string();
        if !value.is_empty() {
            out.push(value);
        }
        let end = value_start + relative_end + close.len();
        text.replace_range(start..end, "");
    }
}

fn remove_tag(text: &mut String, tag: &str) {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    while let Some(start) = text.find(&open) {
        if let Some(relative_end) = text[start + open.len()..].find(&close) {
            let end = start + open.len() + relative_end + close.len();
            text.replace_range(start..end, "");
        } else {
            text.truncate(start);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(name: &str, content: &str) -> Snippet {
        Snippet {
            name: name.into(),
            content: content.into(),
            aliases: Vec::new(),
            description: None,
            file_path: String::new(),
            source: "project".into(),
        }
    }

    #[test]
    fn expands_names_aliases_and_keeps_unknown_tokens() {
        let mut review = snippet("review", "Review the change");
        review.aliases.push("rev".into());
        let snippets = vec![review];
        assert_eq!(
            expand_with("Please #review and #rev but not #12", &snippets),
            "Please Review the change and Review the change but not #12"
        );
    }

    #[test]
    fn expands_prepend_and_append_blocks() {
        let snippets = vec![snippet(
            "review",
            "Base text<prepend>Before</prepend><append>After</append>",
        )];
        assert_eq!(
            expand_with("Please #review", &snippets),
            "Before\n\nPlease Base text\n\nAfter"
        );
    }

    #[test]
    fn loads_project_frontmatter_and_aliases() {
        let workspace =
            std::env::temp_dir().join(format!("rchamber-snippet-test-{}", std::process::id()));
        let directory = workspace.join(".opencode/snippets");
        let _ = std::fs::remove_dir_all(&workspace);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("release.md"),
            "---\naliases:\n  - ship\ndescription: Release checklist\n---\nShip it",
        )
        .unwrap();
        let snippets = load(Some(workspace.to_str().unwrap()));
        let snippet = snippets
            .iter()
            .find(|snippet| snippet.name == "release")
            .unwrap();
        assert_eq!(snippet.aliases, vec!["ship"]);
        assert_eq!(snippet.description.as_deref(), Some("Release checklist"));
        assert_eq!(snippet.source, "project");
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn recurses_but_stops_cycles() {
        let snippets = vec![snippet("a", "A #b"), snippet("b", "B #a")];
        let output = expand_with("#a", &snippets);
        assert!(output.contains('A'));
        assert!(output.contains('B'));
    }
}
