use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Column,
    Row,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    #[default]
    Todo,
    Done,
    Current,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cell {
    Text(String),
    Bool(bool),
    Integer(i64),
    Number(f64),
}

impl Cell {
    pub fn text(&self) -> String {
        match self {
            Cell::Text(value) => value.clone(),
            Cell::Bool(value) => value.to_string(),
            Cell::Integer(value) => value.to_string(),
            Cell::Number(value) => {
                if value.fract().abs() < f64::EPSILON {
                    format!("{value:.0}")
                } else {
                    format!("{value}")
                }
            }
        }
    }
}

impl From<&str> for Cell {
    fn from(value: &str) -> Self {
        Cell::Text(value.to_string())
    }
}

impl From<String> for Cell {
    fn from(value: String) -> Self {
        Cell::Text(value)
    }
}

impl From<f64> for Cell {
    fn from(value: f64) -> Self {
        Cell::Number(value)
    }
}

impl From<i64> for Cell {
    fn from(value: i64) -> Self {
        Cell::Integer(value)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyValueItem {
    pub label: String,
    pub value: Cell,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepItem {
    pub title: String,
    #[serde(default)]
    pub markdown: Option<String>,
    #[serde(default)]
    pub status: StepStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChoiceOption {
    Label(String),
    Full {
        label: String,
        #[serde(default)]
        description: Option<String>,
    },
}

impl ChoiceOption {
    pub fn label(&self) -> &str {
        match self {
            ChoiceOption::Label(label) => label,
            ChoiceOption::Full { label, .. } => label,
        }
    }

    pub fn description(&self) -> Option<&str> {
        match self {
            ChoiceOption::Label(_) => None,
            ChoiceOption::Full { description, .. } => description.as_deref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TabItem {
    pub label: String,
    #[serde(default)]
    pub children: Vec<Widget>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub title: String,
    #[serde(default)]
    pub markdown: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    #[serde(default)]
    pub values: Vec<Option<f64>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Widget {
    Stack {
        #[serde(default)]
        gap: Option<f32>,
        #[serde(default)]
        children: Vec<Widget>,
    },
    Row {
        #[serde(default)]
        wrap: bool,
        #[serde(default)]
        children: Vec<Widget>,
    },
    Card {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        children: Vec<Widget>,
    },
    Tabs {
        #[serde(default)]
        tabs: Vec<TabItem>,
    },
    Text {
        markdown: String,
    },
    Heading {
        text: String,
        #[serde(default)]
        level: Option<u8>,
    },
    Code {
        code: String,
        #[serde(default)]
        language: Option<String>,
    },
    Badge {
        label: String,
        #[serde(default)]
        tone: Tone,
    },
    Callout {
        markdown: String,
        #[serde(default)]
        tone: Tone,
        #[serde(default)]
        title: Option<String>,
    },
    KeyValue {
        #[serde(default)]
        items: Vec<KeyValueItem>,
    },
    Table {
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        rows: Vec<Vec<Cell>>,
    },
    List {
        #[serde(default)]
        items: Vec<ListItem>,
        #[serde(default)]
        ordered: bool,
    },
    Steps {
        #[serde(default)]
        items: Vec<StepItem>,
        #[serde(default)]
        direction: Direction,
    },
    Stat {
        label: String,
        value: Cell,
        #[serde(default)]
        delta: Option<String>,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Progress {
        label: String,
        value: f64,
        #[serde(default = "default_max")]
        max: f64,
    },
    BarChart {
        #[serde(default)]
        labels: Vec<String>,
        #[serde(default)]
        series: Vec<Series>,
        #[serde(default)]
        unit: Option<String>,
        #[serde(default)]
        whole: bool,
    },
    LineChart {
        #[serde(default)]
        labels: Vec<String>,
        #[serde(default)]
        series: Vec<Series>,
        #[serde(default)]
        unit: Option<String>,
    },
    Choice {
        name: String,
        #[serde(default)]
        options: Vec<ChoiceOption>,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        multiple: bool,
    },
    Input {
        name: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        multiline: bool,
    },
    DateInput {
        name: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        time: bool,
        #[serde(default)]
        value: Option<String>,
    },
    Slider {
        name: String,
        min: f64,
        max: f64,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        value: Option<f64>,
        #[serde(default)]
        unit: Option<String>,
    },
    Details {
        title: String,
        #[serde(default)]
        children: Vec<Widget>,
    },
    Divider,
    Button {
        label: String,
        #[serde(default)]
        variant: ButtonVariant,
        #[serde(default)]
        value: Option<String>,
    },
    Image {
        path: String,
        #[serde(default)]
        alt: Option<String>,
        #[serde(default)]
        height: Option<f32>,
    },
    ExtensionBlock {
        name: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

fn default_max() -> f64 {
    100.0
}

impl Widget {
    pub fn text(markdown: impl Into<String>) -> Self {
        Widget::Text {
            markdown: markdown.into(),
        }
    }

    pub fn stack(children: Vec<Widget>) -> Self {
        Widget::Stack {
            gap: None,
            children,
        }
    }

    pub fn card(title: impl Into<String>, children: Vec<Widget>) -> Self {
        Widget::Card {
            title: Some(title.into()),
            children,
        }
    }

    pub fn is_interactive(&self) -> bool {
        match self {
            Widget::Choice { .. }
            | Widget::Input { .. }
            | Widget::DateInput { .. }
            | Widget::Slider { .. }
            | Widget::Button { .. } => true,
            Widget::Stack { children, .. }
            | Widget::Row { children, .. }
            | Widget::Card { children, .. }
            | Widget::Details { children, .. } => children.iter().any(Widget::is_interactive),
            Widget::Tabs { tabs } => tabs
                .iter()
                .any(|tab| tab.children.iter().any(Widget::is_interactive)),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockResponse {
    #[serde(default)]
    pub button: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub values: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub note: Option<String>,
}

impl BlockResponse {
    pub fn summary(&self) -> String {
        if let Some(value) = &self.value {
            return value.clone();
        }
        if !self.values.is_empty() {
            let mut parts = Vec::new();
            for (key, value) in &self.values {
                let text = match value {
                    serde_json::Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                parts.push(format!("{key}: {text}"));
            }
            return parts.join(", ");
        }
        self.button.clone().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_accept_strings_numbers_and_bools() {
        let widget: Widget = serde_json::from_value(serde_json::json!({
            "kind": "table",
            "columns": ["a", "b"],
            "rows": [["x", 2], [true, 3.5]]
        }))
        .unwrap();
        let Widget::Table { rows, .. } = widget else {
            panic!("expected table");
        };
        assert_eq!(rows[0][0].text(), "x");
        assert_eq!(rows[0][1].text(), "2");
        assert_eq!(rows[1][0].text(), "true");
        assert_eq!(rows[1][1].text(), "3.5");
    }

    #[test]
    fn choice_options_accept_labels_and_objects() {
        let widget: Widget = serde_json::from_value(serde_json::json!({
            "kind": "choice",
            "name": "fruit",
            "options": ["Apple", {"label": "Pear", "description": "Green"}]
        }))
        .unwrap();
        let Widget::Choice { options, .. } = widget else {
            panic!("expected choice");
        };
        assert_eq!(options[0].label(), "Apple");
        assert_eq!(options[1].description(), Some("Green"));
    }
}
