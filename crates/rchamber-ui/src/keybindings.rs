use std::collections::BTreeMap;

use eframe::egui::{self, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Command {
    NewSession,
    Palette,
    Settings,
    ToggleSidebar,
    Terminal,
    Search,
    ModelMenu,
    PreviousSession,
    NextSession,
}

impl Command {
    pub const ALL: [Command; 9] = [
        Command::NewSession,
        Command::Palette,
        Command::Settings,
        Command::ToggleSidebar,
        Command::Terminal,
        Command::Search,
        Command::ModelMenu,
        Command::PreviousSession,
        Command::NextSession,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Command::NewSession => "new-session",
            Command::Palette => "palette",
            Command::Settings => "settings",
            Command::ToggleSidebar => "toggle-sidebar",
            Command::Terminal => "terminal",
            Command::Search => "search",
            Command::ModelMenu => "model-menu",
            Command::PreviousSession => "previous-session",
            Command::NextSession => "next-session",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Command::NewSession => "New session",
            Command::Palette => "Open command palette",
            Command::Settings => "Open settings",
            Command::ToggleSidebar => "Toggle sidebar",
            Command::Terminal => "Toggle terminal",
            Command::Search => "Search sessions",
            Command::ModelMenu => "Open model selector",
            Command::PreviousSession => "Previous session",
            Command::NextSession => "Next session",
        }
    }

    pub fn default_binding(self) -> Binding {
        match self {
            Command::NewSession => Binding::new(Key::N, true, false, false, false),
            Command::Palette => Binding::new(Key::K, true, false, false, false),
            Command::Settings => Binding::new(Key::Comma, true, false, false, false),
            Command::ToggleSidebar => Binding::new(Key::B, true, false, false, false),
            Command::Terminal => Binding::new(Key::J, true, false, false, false),
            Command::Search => Binding::new(Key::F, true, false, false, false),
            Command::ModelMenu => Binding::new(Key::M, true, false, true, false),
            Command::PreviousSession => Binding::new(Key::ArrowLeft, true, false, false, true),
            Command::NextSession => Binding::new(Key::ArrowRight, true, false, false, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: Key,
    pub command: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Binding {
    pub const fn new(key: Key, command: bool, ctrl: bool, shift: bool, alt: bool) -> Self {
        Self {
            key,
            command,
            ctrl,
            shift,
            alt,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut key = None;
        let mut command = false;
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        for part in text.split('+') {
            match part.trim().to_ascii_lowercase().as_str() {
                "" => {}
                "mod" | "cmd" | "command" => command = true,
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" | "option" | "opt" => alt = true,
                other => {
                    if key.is_some() {
                        return None;
                    }
                    key = Key::from_name(other);
                    key?;
                }
            }
        }
        Some(Self {
            key: key?,
            command,
            ctrl,
            shift,
            alt,
        })
    }

    pub fn text(self) -> String {
        let mut parts = Vec::new();
        if self.command {
            parts.push(if cfg!(target_os = "macos") {
                "Cmd"
            } else {
                "Ctrl"
            });
        }
        if self.ctrl && !self.command {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push(if cfg!(target_os = "macos") {
                "Option"
            } else {
                "Alt"
            });
        }
        parts.push(self.key.name());
        parts.join("+")
    }

    pub fn matches(self, ctx: &egui::Context) -> bool {
        ctx.input(|input| {
            if !input.key_pressed(self.key) {
                return false;
            }
            let modifiers = input.modifiers;
            modifiers.command == self.command
                && modifiers.ctrl == self.ctrl
                && modifiers.shift == self.shift
                && modifiers.alt == self.alt
        })
    }

    pub fn from_modifiers(modifiers: Modifiers, key: Key) -> Self {
        Self {
            key,
            command: modifiers.command,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Keymap {
    overrides: BTreeMap<String, String>,
}

impl Keymap {
    pub fn from_settings(settings: &BTreeMap<String, String>) -> Self {
        Self {
            overrides: settings.clone(),
        }
    }

    pub fn binding(&self, command: Command) -> Binding {
        self.overrides
            .get(command.id())
            .and_then(|text| Binding::parse(text))
            .unwrap_or_else(|| command.default_binding())
    }

    pub fn matches(&self, ctx: &egui::Context, command: Command) -> bool {
        self.binding(command).matches(ctx)
    }

    pub fn set(&mut self, command: Command, binding: Option<Binding>) {
        match binding {
            Some(binding) => {
                self.overrides
                    .insert(command.id().to_string(), binding.text());
            }
            None => {
                self.overrides
                    .insert(command.id().to_string(), String::new());
            }
        }
    }
}

pub fn capture(ctx: &egui::Context) -> Option<Capture> {
    ctx.input(|input| {
        for event in &input.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if *key == Key::Escape {
                    return Some(Capture::Cancel);
                }
                if *key == Key::Backspace || *key == Key::Delete {
                    return Some(Capture::Clear);
                }
                if matches!(key, Key::Tab | Key::Enter) {
                    continue;
                }
                return Some(Capture::Binding(Binding::from_modifiers(*modifiers, *key)));
            }
        }
        None
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    Binding(Binding),
    Clear,
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_bindings() {
        let binding = Binding::parse("Cmd+Shift+M").unwrap();
        assert_eq!(binding.key, Key::M);
        assert!(binding.command);
        assert!(binding.shift);
        assert!(binding.text().contains('M'));
        assert!(Binding::parse("Nope+Q").is_none());
    }

    #[test]
    fn defaults_are_stable() {
        let keymap = Keymap::default();
        assert_eq!(keymap.binding(Command::NewSession).key, Key::N);
        assert!(keymap.binding(Command::NewSession).command);
    }
}
