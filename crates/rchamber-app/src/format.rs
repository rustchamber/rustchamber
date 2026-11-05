use chrono::{DateTime, Local, TimeZone};

pub fn relative_time(ms: f64) -> String {
    if ms <= 0.0 {
        return String::new();
    }
    let seconds = (crate::now_ms() - ms) / 1000.0;
    if seconds < 45.0 {
        "just now".into()
    } else if seconds < 90.0 {
        "1 min ago".into()
    } else if seconds < 3600.0 {
        format!("{} min ago", (seconds / 60.0).round() as i64)
    } else if seconds < 86_400.0 {
        let hours = (seconds / 3600.0).round() as i64;
        if hours <= 1 {
            "1 hour ago".into()
        } else {
            format!("{hours} hours ago")
        }
    } else if seconds < 172_800.0 {
        "Yesterday".into()
    } else if seconds < 604_800.0 {
        format!("{} days ago", (seconds / 86_400.0).round() as i64)
    } else {
        date_label(ms)
    }
}

pub fn date_label(ms: f64) -> String {
    let Some(datetime) = datetime(ms) else {
        return String::new();
    };
    datetime.format("%b %-d").to_string()
}

pub fn time_label(ms: f64) -> String {
    let Some(datetime) = datetime(ms) else {
        return String::new();
    };
    datetime.format("%H:%M").to_string()
}

pub fn day_bucket(ms: f64) -> &'static str {
    if ms <= 0.0 {
        return "Older";
    }
    let Some(datetime) = datetime(ms) else {
        return "Older";
    };
    let now = Local::now();
    let today = now.date_naive();
    let date = datetime.date_naive();
    let delta = (today - date).num_days();
    match delta {
        i64::MIN..=-2 => "Older",
        -1 => "Yesterday",
        0 => "Today",
        _ => "Today",
    }
}

fn datetime(ms: f64) -> Option<DateTime<Local>> {
    let seconds = ms / 1000.0;
    let secs = seconds.trunc() as i64;
    let nanos = ((seconds - seconds.trunc()) * 1_000_000_000.0) as u32;
    Local.timestamp_opt(secs, nanos).single()
}

pub fn compact_number(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}k", value / 1_000.0)
    } else {
        format!("{}", value.round() as i64)
    }
}

pub fn money(value: f64) -> String {
    if value <= 0.0 {
        return "$0.00".into();
    }
    if value < 0.01 {
        format!("${value:.4}")
    } else if value < 1.0 {
        format!("${value:.3}")
    } else {
        format!("${value:.2}")
    }
}

pub fn duration(ms: f64) -> String {
    if ms <= 0.0 {
        return String::new();
    }
    let seconds = ms / 1000.0;
    if seconds < 1.0 {
        format!("{}ms", ms.round() as i64)
    } else if seconds < 60.0 {
        format!("{seconds:.1}s")
    } else {
        let minutes = (seconds / 60.0).floor() as i64;
        let rest = (seconds % 60.0).round() as i64;
        format!("{minutes}m {rest}s")
    }
}

pub fn shorten_path(path: &str, max: usize) -> String {
    if path.chars().count() <= max {
        return path.to_string();
    }
    let tail: String = path
        .chars()
        .rev()
        .take(max.saturating_sub(1))
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("…{tail}")
}

pub fn tool_label(name: &str) -> &str {
    match name {
        "bash" | "shell" => "Shell",
        "read" | "read_file" => "Read",
        "write" | "write_file" => "Write",
        "edit" | "edit_file" | "multi_edit" => "Edit",
        "grep" | "search" => "Search",
        "glob" | "list" => "Files",
        "webfetch" | "fetch" => "Fetch",
        "websearch" | "search_web" => "Web search",
        "task" | "agent" => "Subagent",
        "todowrite" | "todo" => "Plan",
        "patch" => "Patch",
        _ => name,
    }
}

pub fn tool_detail(name: &str, input: Option<&serde_json::Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };
    let pick = |keys: &[&str]| -> Option<String> {
        for key in keys {
            if let Some(value) = input.get(*key) {
                match value {
                    serde_json::Value::String(text) if !text.is_empty() => {
                        return Some(text.clone());
                    }
                    serde_json::Value::Array(items) if !items.is_empty() => {
                        return Some(
                            items
                                .iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(" "),
                        );
                    }
                    _ => {}
                }
            }
        }
        None
    };
    match name {
        "bash" | "shell" => pick(&["command", "cmd"]).unwrap_or_default(),
        "read" | "read_file" | "write" | "write_file" | "edit" | "edit_file" | "patch" => {
            pick(&["filePath", "file_path", "path", "file"]).unwrap_or_default()
        }
        "grep" | "search" | "glob" | "list" => {
            pick(&["pattern", "query", "path"]).unwrap_or_default()
        }
        "webfetch" | "fetch" => pick(&["url"]).unwrap_or_default(),
        "websearch" | "search_web" => pick(&["query"]).unwrap_or_default(),
        "task" | "agent" => pick(&["description", "prompt", "agent"]).unwrap_or_default(),
        _ => pick(&["description", "command", "query", "path", "file", "url"]).unwrap_or_default(),
    }
}

pub fn diff_line_style(line: &str) -> DiffLine {
    if line.starts_with("@@") {
        DiffLine::Hunk
    } else if line.starts_with('+') {
        DiffLine::Added
    } else if line.starts_with('-') {
        DiffLine::Removed
    } else if line.starts_with("diff ")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
    {
        DiffLine::Meta
    } else {
        DiffLine::Context
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLine {
    Added,
    Removed,
    Hunk,
    Meta,
    Context,
}

pub fn plural(value: usize, singular: &str, plural: &str) -> String {
    if value == 1 {
        format!("{value} {singular}")
    } else {
        format!("{value} {plural}")
    }
}
