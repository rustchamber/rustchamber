use std::collections::HashMap;

use crate::widget::{
    ButtonVariant, Cell, ChoiceOption, Direction, KeyValueItem, ListItem, Series, StepItem,
    StepStatus, TabItem, Tone, Widget,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ParseError {
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
    Semantic(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Syntax {
                line,
                column,
                message,
            } => write!(formatter, "line {line}, column {column}: {message}"),
            ParseError::Semantic(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
    Call(String, Vec<Node>),
    Ident(String),
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Equals,
    Colon,
    Newline,
    Eof,
}

struct Lexer {
    chars: Vec<char>,
    index: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            index: 0,
            line: 1,
            column: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.index + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.index += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn skip_inline(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') => {
                    self.bump();
                }
                Some('/') if self.peek_next() == Some('/') => {
                    while let Some(ch) = self.peek() {
                        if ch == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    fn token(&mut self) -> Result<Token, ParseError> {
        self.skip_inline();
        let Some(ch) = self.peek() else {
            return Ok(Token::Eof);
        };
        let line = self.line;
        let column = self.column;
        match ch {
            '\n' => {
                self.bump();
                Ok(Token::Newline)
            }
            '(' => {
                self.bump();
                Ok(Token::LParen)
            }
            ')' => {
                self.bump();
                Ok(Token::RParen)
            }
            '[' => {
                self.bump();
                Ok(Token::LBracket)
            }
            ']' => {
                self.bump();
                Ok(Token::RBracket)
            }
            '{' => {
                self.bump();
                Ok(Token::LBrace)
            }
            '}' => {
                self.bump();
                Ok(Token::RBrace)
            }
            ',' => {
                self.bump();
                Ok(Token::Comma)
            }
            '=' => {
                self.bump();
                Ok(Token::Equals)
            }
            ':' => {
                self.bump();
                Ok(Token::Colon)
            }
            '"' | '\'' => self.string(ch, line, column),
            '-' | '0'..='9' => self.number(line, column),
            _ if ch.is_alphabetic() || ch == '_' => {
                let start = self.index;
                while let Some(ch) = self.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let text: String = self.chars[start..self.index].iter().collect();
                Ok(match text.as_str() {
                    "true" => Token::Bool(true),
                    "false" => Token::Bool(false),
                    "null" | "nil" => Token::Null,
                    _ => Token::Ident(text),
                })
            }
            other => Err(ParseError::Syntax {
                line,
                column,
                message: format!("unexpected character {other:?}"),
            }),
        }
    }

    fn string(&mut self, quote: char, line: usize, column: usize) -> Result<Token, ParseError> {
        self.bump();
        let mut value = String::new();
        loop {
            let Some(ch) = self.bump() else {
                return Err(ParseError::Syntax {
                    line,
                    column,
                    message: "unterminated string".into(),
                });
            };
            if ch == quote {
                break;
            }
            if ch == '\\' {
                let Some(escape) = self.bump() else {
                    return Err(ParseError::Syntax {
                        line,
                        column,
                        message: "unterminated escape".into(),
                    });
                };
                match escape {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
                    'u' => {
                        let mut hex = String::new();
                        for _ in 0..4 {
                            if let Some(ch) = self.bump() {
                                hex.push(ch);
                            }
                        }
                        if let Ok(code) = u32::from_str_radix(&hex, 16)
                            && let Some(ch) = char::from_u32(code)
                        {
                            value.push(ch);
                        }
                    }
                    other => value.push(other),
                }
            } else {
                value.push(ch);
            }
        }
        Ok(Token::Str(value))
    }

    fn number(&mut self, line: usize, column: usize) -> Result<Token, ParseError> {
        let start = self.index;
        if self.peek() == Some('-') {
            self.bump();
        }
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' || ch == '+' || ch == '-'
            {
                self.bump();
            } else {
                break;
            }
        }
        let text: String = self.chars[start..self.index].iter().collect();
        text.parse::<f64>()
            .map(Token::Num)
            .map_err(|_| ParseError::Syntax {
                line,
                column,
                message: format!("invalid number {text:?}"),
            })
    }
}

struct Parser {
    lexer: Lexer,
    lookahead: Option<(Token, usize, usize)>,
    nodes: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            lexer: Lexer::new(input),
            lookahead: None,
            nodes: 0,
        }
    }

    fn next(&mut self) -> Result<(Token, usize, usize), ParseError> {
        if let Some(token) = self.lookahead.take() {
            return Ok(token);
        }
        let line = self.lexer.line;
        let column = self.lexer.column;
        let token = self.lexer.token()?;
        Ok((token, line, column))
    }

    fn peek(&mut self) -> Result<&Token, ParseError> {
        if self.lookahead.is_none() {
            let line = self.lexer.line;
            let column = self.lexer.column;
            let token = self.lexer.token()?;
            self.lookahead = Some((token, line, column));
        }
        Ok(&self.lookahead.as_ref().unwrap().0)
    }

    fn skip_newlines(&mut self) -> Result<(), ParseError> {
        while matches!(self.peek()?, Token::Newline) {
            self.next()?;
        }
        Ok(())
    }

    fn program(&mut self) -> Result<HashMap<String, Node>, ParseError> {
        let mut statements = HashMap::new();
        let mut count = 0usize;
        loop {
            count += 1;
            if count > 2_000 {
                return Err(ParseError::Semantic(
                    "the program has more than 2,000 statements".into(),
                ));
            }
            self.skip_newlines()?;
            let (token, line, column) = self.next()?;
            match token {
                Token::Eof => break,
                Token::Ident(name) => {
                    let (equals, _, _) = self.next()?;
                    if !matches!(equals, Token::Equals) {
                        return Err(ParseError::Syntax {
                            line,
                            column,
                            message: format!("expected \"=\" after {name}"),
                        });
                    }
                    let node = self.expr(0)?;
                    statements.insert(name, node);
                    match self.peek()? {
                        Token::Newline | Token::Eof => {}
                        _ => {
                            let (token, line, column) = self.next()?;
                            return Err(ParseError::Syntax {
                                line,
                                column,
                                message: format!("unexpected token after statement: {token:?}"),
                            });
                        }
                    }
                }
                other => {
                    return Err(ParseError::Syntax {
                        line,
                        column,
                        message: format!("expected a statement name, got {other:?}"),
                    });
                }
            }
        }
        Ok(statements)
    }

    fn expr(&mut self, depth: usize) -> Result<Node, ParseError> {
        self.nodes += 1;
        if self.nodes > 20_000 {
            return Err(ParseError::Semantic(
                "the block has more than 20,000 values".into(),
            ));
        }
        if depth > 32 {
            return Err(ParseError::Semantic(
                "the block is nested more than 32 levels".into(),
            ));
        }
        let (token, line, column) = self.next()?;
        match token {
            Token::Str(value) => Ok(Node::Str(value)),
            Token::Num(value) => Ok(Node::Num(value)),
            Token::Bool(value) => Ok(Node::Bool(value)),
            Token::Null => Ok(Node::Null),
            Token::Ident(name) => {
                if matches!(self.peek()?, Token::LParen) {
                    self.next()?;
                    let args = self.args(depth + 1)?;
                    Ok(Node::Call(name, args))
                } else {
                    Ok(Node::Ident(name))
                }
            }
            Token::LBracket => {
                let mut items = Vec::new();
                loop {
                    self.skip_newlines()?;
                    if matches!(self.peek()?, Token::RBracket) {
                        self.next()?;
                        break;
                    }
                    items.push(self.expr(depth + 1)?);
                    self.skip_newlines()?;
                    let token = self.peek()?.clone();
                    match token {
                        Token::Comma => {
                            self.next()?;
                        }
                        Token::RBracket => {}
                        other => {
                            return Err(ParseError::Syntax {
                                line: self.lexer.line,
                                column: self.lexer.column,
                                message: format!(
                                    "expected \",\" or \"]\", got {}",
                                    other_short(&other)
                                ),
                            });
                        }
                    }
                }
                Ok(Node::Array(items))
            }
            Token::LBrace => {
                let mut pairs = Vec::new();
                loop {
                    self.skip_newlines()?;
                    if matches!(self.peek()?, Token::RBrace) {
                        self.next()?;
                        break;
                    }
                    let (key, key_line, key_column) = self.next()?;
                    let key = match key {
                        Token::Ident(name) => name,
                        Token::Str(name) => name,
                        other => {
                            return Err(ParseError::Syntax {
                                line: key_line,
                                column: key_column,
                                message: format!(
                                    "object keys must be names, got {}",
                                    other_short(&other)
                                ),
                            });
                        }
                    };
                    let (colon, _, _) = self.next()?;
                    if !matches!(colon, Token::Colon) {
                        return Err(ParseError::Syntax {
                            line: key_line,
                            column: key_column,
                            message: format!("expected \":\" after {key}"),
                        });
                    }
                    pairs.push((key, self.expr(depth + 1)?));
                    self.skip_newlines()?;
                    let token = self.peek()?.clone();
                    match token {
                        Token::Comma => {
                            self.next()?;
                        }
                        Token::RBrace => {}
                        other => {
                            return Err(ParseError::Syntax {
                                line: self.lexer.line,
                                column: self.lexer.column,
                                message: format!(
                                    "expected \",\" or \"}}\", got {}",
                                    other_short(&other)
                                ),
                            });
                        }
                    }
                }
                Ok(Node::Object(pairs))
            }
            other => Err(ParseError::Syntax {
                line,
                column,
                message: format!("expected a value, got {}", other_short(&other)),
            }),
        }
    }

    fn args(&mut self, depth: usize) -> Result<Vec<Node>, ParseError> {
        if depth > 32 {
            return Err(ParseError::Semantic(
                "the block is nested more than 32 levels".into(),
            ));
        }
        let mut args = Vec::new();
        loop {
            self.skip_newlines()?;
            if matches!(self.peek()?, Token::RParen) {
                self.next()?;
                break;
            }
            args.push(self.expr(depth + 1)?);
            self.skip_newlines()?;
            let token = self.peek()?.clone();
            match token {
                Token::Comma => {
                    self.next()?;
                }
                Token::RParen => {}
                other => {
                    return Err(ParseError::Syntax {
                        line: self.lexer.line,
                        column: self.lexer.column,
                        message: format!("expected \",\" or \")\", got {}", other_short(&other)),
                    });
                }
            }
        }
        Ok(args)
    }
}

fn other_short(token: &Token) -> String {
    match token {
        Token::Ident(name) => format!("\"{name}\""),
        Token::Str(_) => "a string".into(),
        Token::Num(_) => "a number".into(),
        Token::Eof => "the end".into(),
        other => format!("{other:?}"),
    }
}

struct Resolver<'a> {
    statements: &'a HashMap<String, Node>,
    stack: Vec<String>,
}

impl<'a> Resolver<'a> {
    fn node(&mut self, node: &Node) -> Result<Node, ParseError> {
        match node {
            Node::Ident(name) => {
                if self.stack.iter().any(|item| item == name) {
                    return Err(ParseError::Semantic(format!(
                        "reference cycle through {name}"
                    )));
                }
                let Some(value) = self.statements.get(name) else {
                    return Err(ParseError::Semantic(format!("unknown name {name}")));
                };
                self.stack.push(name.clone());
                let resolved = self.node(value);
                self.stack.pop();
                resolved
            }
            Node::Array(items) => Ok(Node::Array(
                items
                    .iter()
                    .map(|item| self.node(item))
                    .collect::<Result<_, _>>()?,
            )),
            Node::Object(pairs) => Ok(Node::Object(
                pairs
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), self.node(value)?)))
                    .collect::<Result<_, ParseError>>()?,
            )),
            Node::Call(name, args) => Ok(Node::Call(
                name.clone(),
                args.iter()
                    .map(|arg| self.node(arg))
                    .collect::<Result<_, _>>()?,
            )),
            other => Ok(other.clone()),
        }
    }
}

pub fn parse(input: &str) -> Result<Vec<Widget>, ParseError> {
    let mut parser = Parser::new(input);
    let statements = parser.program()?;
    let Some(root) = statements.get("root") else {
        return Err(ParseError::Semantic(
            "the program needs a `root = ...` statement".into(),
        ));
    };
    let mut resolver = Resolver {
        statements: &statements,
        stack: vec!["root".into()],
    };
    let node = resolver.node(root)?;
    let widgets = widgets(&node)?;
    Ok(if widgets.len() == 1 {
        match widgets.into_iter().next().unwrap() {
            Widget::Stack { children, .. } => children,
            other => vec![other],
        }
    } else {
        widgets
    })
}

fn widgets(node: &Node) -> Result<Vec<Widget>, ParseError> {
    match node {
        Node::Array(items) => items.iter().map(widget).collect(),
        other => Ok(vec![widget(other)?]),
    }
}

fn widget(node: &Node) -> Result<Widget, ParseError> {
    let Node::Call(name, args) = node else {
        return Err(ParseError::Semantic(format!(
            "expected a component, got {}",
            describe(node)
        )));
    };
    let widget = match name.as_str() {
        "Stack" => Widget::Stack {
            gap: optional_number(args, 1)?.map(|value| value as f32),
            children: children(arg(args, 0)?)?,
        },
        "Row" => Widget::Row {
            wrap: optional_bool(args, 1)?.unwrap_or(false),
            children: children(arg(args, 0)?)?,
        },
        "Card" => Widget::Card {
            title: optional_string(args, 0)?,
            children: children(arg(args, 1)?)?,
        },
        "Tabs" => Widget::Tabs {
            tabs: tab_items(arg(args, 0)?)?,
        },
        "Text" => Widget::Text {
            markdown: string(arg(args, 0)?)?,
        },
        "Heading" => Widget::Heading {
            text: string(arg(args, 0)?)?,
            level: optional_number(args, 1)?.map(|value| value.clamp(1.0, 6.0) as u8),
        },
        "Code" => Widget::Code {
            code: string(arg(args, 0)?)?,
            language: optional_string(args, 1)?,
        },
        "Badge" => Widget::Badge {
            label: string(arg(args, 0)?)?,
            tone: tone(args, 1)?,
        },
        "Callout" => Widget::Callout {
            markdown: string(arg(args, 0)?)?,
            tone: tone(args, 1)?,
            title: optional_string(args, 2)?,
        },
        "KeyValue" => Widget::KeyValue {
            items: key_value_items(arg(args, 0)?)?,
        },
        "Table" => Widget::Table {
            columns: strings(arg(args, 0)?)?,
            rows: rows(arg(args, 1)?)?,
        },
        "List" => Widget::List {
            items: list_items(arg(args, 0)?)?,
            ordered: optional_bool(args, 1)?.unwrap_or(false),
        },
        "Steps" => Widget::Steps {
            items: step_items(arg(args, 0)?)?,
            direction: match optional_string(args, 1)?.as_deref() {
                Some("row") => Direction::Row,
                _ => Direction::Column,
            },
        },
        "Stat" => Widget::Stat {
            label: string(arg(args, 0)?)?,
            value: cell(arg(args, 1)?)?,
            delta: optional_string(args, 2)?,
            tone: optional_tone(args, 3)?,
        },
        "Progress" => Widget::Progress {
            label: string(arg(args, 0)?)?,
            value: number(arg(args, 1)?)?,
            max: optional_number(args, 2)?.unwrap_or(100.0),
        },
        "BarChart" => Widget::BarChart {
            labels: strings(arg(args, 0)?)?,
            series: series(arg(args, 1)?)?,
            unit: optional_string(args, 2)?,
            whole: optional_bool(args, 3)?.unwrap_or(false),
        },
        "LineChart" => Widget::LineChart {
            labels: strings(arg(args, 0)?)?,
            series: series(arg(args, 1)?)?,
            unit: optional_string(args, 2)?,
        },
        "Choice" => Widget::Choice {
            name: string(arg(args, 0)?)?,
            options: choice_options(arg(args, 1)?)?,
            label: optional_string(args, 2)?,
            multiple: optional_bool(args, 3)?.unwrap_or(false),
        },
        "Input" => Widget::Input {
            name: string(arg(args, 0)?)?,
            label: optional_string(args, 1)?,
            placeholder: optional_string(args, 2)?,
            multiline: optional_bool(args, 3)?.unwrap_or(false),
        },
        "DateInput" => Widget::DateInput {
            name: string(arg(args, 0)?)?,
            label: optional_string(args, 1)?,
            time: optional_bool(args, 2)?.unwrap_or(false),
            value: optional_string(args, 3)?,
        },
        "Slider" => Widget::Slider {
            name: string(arg(args, 0)?)?,
            min: number(arg(args, 1)?)?,
            max: number(arg(args, 2)?)?,
            label: optional_string(args, 3)?,
            step: optional_number(args, 4)?,
            value: optional_number(args, 5)?,
            unit: optional_string(args, 6)?,
        },
        "Details" => Widget::Details {
            title: string(arg(args, 0)?)?,
            children: children(arg(args, 1)?)?,
        },
        "Divider" => Widget::Divider,
        "Button" => Widget::Button {
            label: string(arg(args, 0)?)?,
            variant: match optional_string(args, 1)?.as_deref() {
                Some("secondary") => ButtonVariant::Secondary,
                _ => ButtonVariant::Primary,
            },
            value: optional_string(args, 2)?,
        },
        "Tab" => {
            return Err(ParseError::Semantic("Tab is only valid inside Tabs".into()));
        }
        "Series" => {
            return Err(ParseError::Semantic(
                "Series is only valid inside BarChart or LineChart".into(),
            ));
        }
        other => Widget::ExtensionBlock {
            name: other.to_string(),
            args: args
                .iter()
                .map(scalar_text)
                .collect::<Result<Vec<_>, _>>()?,
        },
    };
    Ok(widget)
}

fn arg(args: &[Node], index: usize) -> Result<&Node, ParseError> {
    args.get(index)
        .ok_or_else(|| ParseError::Semantic(format!("missing argument {}", index + 1)))
}

fn describe(node: &Node) -> String {
    match node {
        Node::Str(value) => format!("{value:?}"),
        Node::Num(value) => value.to_string(),
        Node::Bool(value) => value.to_string(),
        Node::Null => "null".into(),
        Node::Array(_) => "an array".into(),
        Node::Object(_) => "an object".into(),
        Node::Call(name, _) => format!("{name}(...)"),
        Node::Ident(name) => name.clone(),
    }
}

fn scalar_text(node: &Node) -> Result<String, ParseError> {
    match node {
        Node::Str(value) => Ok(value.clone()),
        Node::Num(value) => Ok(Cell::Number(*value).text()),
        Node::Bool(value) => Ok(value.to_string()),
        Node::Null => Ok(String::new()),
        other => Err(ParseError::Semantic(format!(
            "extension block arguments must be strings or numbers, got {}",
            describe(other)
        ))),
    }
}

fn string(node: &Node) -> Result<String, ParseError> {
    match node {
        Node::Str(value) => Ok(value.clone()),
        Node::Num(value) => Ok(Cell::Number(*value).text()),
        Node::Bool(value) => Ok(value.to_string()),
        other => Err(ParseError::Semantic(format!(
            "expected a string, got {}",
            describe(other)
        ))),
    }
}

fn optional_string(args: &[Node], index: usize) -> Result<Option<String>, ParseError> {
    match args.get(index) {
        None | Some(Node::Null) => Ok(None),
        Some(node) => Ok(Some(string(node)?)),
    }
}

fn number(node: &Node) -> Result<f64, ParseError> {
    match node {
        Node::Num(value) => Ok(*value),
        other => Err(ParseError::Semantic(format!(
            "expected a number, got {}",
            describe(other)
        ))),
    }
}

fn optional_number(args: &[Node], index: usize) -> Result<Option<f64>, ParseError> {
    match args.get(index) {
        None | Some(Node::Null) => Ok(None),
        Some(node) => Ok(Some(number(node)?)),
    }
}

fn optional_bool(args: &[Node], index: usize) -> Result<Option<bool>, ParseError> {
    match args.get(index) {
        None | Some(Node::Null) => Ok(None),
        Some(Node::Bool(value)) => Ok(Some(*value)),
        Some(other) => Err(ParseError::Semantic(format!(
            "expected true or false, got {}",
            describe(other)
        ))),
    }
}

fn tone(args: &[Node], index: usize) -> Result<Tone, ParseError> {
    Ok(optional_tone(args, index)?.unwrap_or_default())
}

fn optional_tone(args: &[Node], index: usize) -> Result<Option<Tone>, ParseError> {
    let Some(value) = optional_string(args, index)? else {
        return Ok(None);
    };
    Ok(Some(match value.as_str() {
        "neutral" => Tone::Neutral,
        "info" => Tone::Info,
        "success" => Tone::Success,
        "warning" => Tone::Warning,
        "error" => Tone::Error,
        other => {
            return Err(ParseError::Semantic(format!(
                "unknown tone {other:?}; use neutral, info, success, warning or error"
            )));
        }
    }))
}

fn children(node: &Node) -> Result<Vec<Widget>, ParseError> {
    match node {
        Node::Array(items) => items.iter().map(widget).collect(),
        other => Err(ParseError::Semantic(format!(
            "expected an array of components, got {}",
            describe(other)
        ))),
    }
}

fn strings(node: &Node) -> Result<Vec<String>, ParseError> {
    match node {
        Node::Array(items) => items.iter().map(string).collect(),
        other => Err(ParseError::Semantic(format!(
            "expected an array of strings, got {}",
            describe(other)
        ))),
    }
}

fn rows(node: &Node) -> Result<Vec<Vec<Cell>>, ParseError> {
    let Node::Array(items) = node else {
        return Err(ParseError::Semantic("expected an array of rows".into()));
    };
    items
        .iter()
        .map(|row| match row {
            Node::Array(cells) => cells.iter().map(cell).collect(),
            other => Err(ParseError::Semantic(format!(
                "each row must be an array, got {}",
                describe(other)
            ))),
        })
        .collect()
}

fn cell(node: &Node) -> Result<Cell, ParseError> {
    match node {
        Node::Str(value) => Ok(Cell::Text(value.clone())),
        Node::Bool(value) => Ok(Cell::Bool(*value)),
        Node::Num(value) => {
            if value.fract().abs() < f64::EPSILON && value.abs() < i64::MAX as f64 {
                Ok(Cell::Integer(*value as i64))
            } else {
                Ok(Cell::Number(*value))
            }
        }
        other => Err(ParseError::Semantic(format!(
            "table cells must be strings or numbers, got {}",
            describe(other)
        ))),
    }
}

fn key_value_items(node: &Node) -> Result<Vec<KeyValueItem>, ParseError> {
    let Node::Array(items) = node else {
        return Err(ParseError::Semantic(
            "KeyValue expects an array of objects".into(),
        ));
    };
    items
        .iter()
        .map(|item| {
            let Node::Object(pairs) = item else {
                return Err(ParseError::Semantic(
                    "each KeyValue item must be an object {label, value}".into(),
                ));
            };
            let mut label = None;
            let mut value = None;
            for (key, node) in pairs {
                match key.as_str() {
                    "label" => label = Some(string(node)?),
                    "value" => value = Some(cell(node)?),
                    _ => {}
                }
            }
            Ok(KeyValueItem {
                label: label
                    .ok_or_else(|| ParseError::Semantic("KeyValue item needs label".into()))?,
                value: value
                    .ok_or_else(|| ParseError::Semantic("KeyValue item needs value".into()))?,
            })
        })
        .collect()
}

fn list_items(node: &Node) -> Result<Vec<ListItem>, ParseError> {
    let Node::Array(items) = node else {
        return Err(ParseError::Semantic("List expects an array".into()));
    };
    items
        .iter()
        .map(|item| match item {
            Node::Str(title) => Ok(ListItem {
                title: title.clone(),
                markdown: None,
            }),
            Node::Object(pairs) => {
                let mut title = None;
                let mut markdown = None;
                for (key, node) in pairs {
                    match key.as_str() {
                        "title" => title = Some(string(node)?),
                        "markdown" => markdown = Some(string(node)?),
                        _ => {}
                    }
                }
                Ok(ListItem {
                    title: title.unwrap_or_default(),
                    markdown,
                })
            }
            other => Err(ParseError::Semantic(format!(
                "list items must be strings or objects, got {}",
                describe(other)
            ))),
        })
        .collect()
}

fn step_items(node: &Node) -> Result<Vec<StepItem>, ParseError> {
    let Node::Array(items) = node else {
        return Err(ParseError::Semantic("Steps expects an array".into()));
    };
    items
        .iter()
        .map(|item| {
            let Node::Object(pairs) = item else {
                return Err(ParseError::Semantic(
                    "each step must be an object {title, markdown, status}".into(),
                ));
            };
            let mut title = None;
            let mut markdown = None;
            let mut status = StepStatus::Todo;
            for (key, node) in pairs {
                match key.as_str() {
                    "title" => title = Some(string(node)?),
                    "markdown" => markdown = Some(string(node)?),
                    "status" => {
                        status = match string(node)?.as_str() {
                            "done" => StepStatus::Done,
                            "current" => StepStatus::Current,
                            "blocked" => StepStatus::Blocked,
                            _ => StepStatus::Todo,
                        };
                    }
                    _ => {}
                }
            }
            Ok(StepItem {
                title: title.ok_or_else(|| ParseError::Semantic("step needs title".into()))?,
                markdown,
                status,
            })
        })
        .collect()
}

fn series(node: &Node) -> Result<Vec<Series>, ParseError> {
    let Node::Array(items) = node else {
        return Err(ParseError::Semantic(
            "charts expect an array of Series".into(),
        ));
    };
    items
        .iter()
        .map(|item| {
            let Node::Call(name, args) = item else {
                return Err(ParseError::Semantic("expected Series(...)".into()));
            };
            if name != "Series" {
                return Err(ParseError::Semantic(format!(
                    "charts expect Series(...), got {name}"
                )));
            }
            let name = string(arg(args, 0)?)?;
            let values = match arg(args, 1)? {
                Node::Array(items) => items
                    .iter()
                    .map(|value| match value {
                        Node::Null => Ok(None),
                        other => Ok(Some(number(other)?)),
                    })
                    .collect::<Result<Vec<_>, ParseError>>()?,
                other => {
                    return Err(ParseError::Semantic(format!(
                        "Series values must be an array, got {}",
                        describe(other)
                    )));
                }
            };
            Ok(Series { name, values })
        })
        .collect()
}

fn tab_items(node: &Node) -> Result<Vec<TabItem>, ParseError> {
    let Node::Array(items) = node else {
        return Err(ParseError::Semantic("Tabs expects an array".into()));
    };
    items
        .iter()
        .map(|item| {
            let Node::Call(name, args) = item else {
                return Err(ParseError::Semantic("expected Tab(...)".into()));
            };
            if name != "Tab" {
                return Err(ParseError::Semantic(format!(
                    "Tabs expect Tab(...), got {name}"
                )));
            }
            Ok(TabItem {
                label: string(arg(args, 0)?)?,
                children: children(arg(args, 1)?)?,
            })
        })
        .collect()
}

fn choice_options(node: &Node) -> Result<Vec<ChoiceOption>, ParseError> {
    let Node::Array(items) = node else {
        return Err(ParseError::Semantic(
            "Choice options must be an array".into(),
        ));
    };
    items
        .iter()
        .map(|item| match item {
            Node::Str(label) => Ok(ChoiceOption::Label(label.clone())),
            Node::Object(pairs) => {
                let mut label = None;
                let mut description = None;
                for (key, node) in pairs {
                    match key.as_str() {
                        "label" => label = Some(string(node)?),
                        "description" => description = Some(string(node)?),
                        _ => {}
                    }
                }
                Ok(ChoiceOption::Full {
                    label: label
                        .ok_or_else(|| ParseError::Semantic("choice option needs label".into()))?,
                    description,
                })
            }
            other => Err(ParseError::Semantic(format!(
                "choice options must be strings or objects, got {}",
                describe(other)
            ))),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_standard_example() {
        let source = r#"
root = Stack([note, flow])
note = Callout("Routers rewrite the MAC header, keep the IP header.", "info")
flow = Steps([{title: "Sender", status: "done"}, {title: "Router", status: "current"}, {title: "Receiver"}], "row")
"#;
        let widgets = parse(source).unwrap();
        assert_eq!(widgets.len(), 2);
        assert!(matches!(widgets[0], Widget::Callout { .. }));
        assert!(matches!(
            widgets[1],
            Widget::Steps {
                direction: Direction::Row,
                ..
            }
        ));
    }

    #[test]
    fn resolves_references_and_reports_cycles() {
        let good = "root = Stack([a])\na = Text(\"hi\")";
        assert!(parse(good).is_ok());
        let cycle = "root = Stack([a])\na = Stack([root])";
        assert!(parse(cycle).unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn parses_tables_and_charts() {
        let source = r#"
root = Stack([t, c])
t = Table(["a", "b"], [["x", 2], ["y", 3]])
c = BarChart(["a", "b"], [Series("one", [1, 2])], "ms", false)
"#;
        let widgets = parse(source).unwrap();
        assert!(matches!(widgets[0], Widget::Table { .. }));
        assert!(matches!(widgets[1], Widget::BarChart { .. }));
    }

    #[test]
    fn unknown_components_become_extension_blocks() {
        let widgets = parse(r#"root = Stack([Deploys("api", "prod")])"#).unwrap();
        assert!(
            matches!(&widgets[0], Widget::ExtensionBlock { name, args } if name == "Deploys" && args.len() == 2)
        );
    }

    #[test]
    fn rejects_tab_outside_tabs() {
        assert!(parse("root = Tab(\"a\", [])").is_err());
    }
}
