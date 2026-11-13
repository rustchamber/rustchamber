use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;
use crate::theme_store::ThemeSettings;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: ThemeMode,
    #[serde(default)]
    pub appearance: ThemeSettings,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub compact_density: bool,
    #[serde(default)]
    pub reduce_motion: bool,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    #[serde(default = "default_zone_width")]
    pub zone_width: f32,
    #[serde(default = "default_bottom_height")]
    pub bottom_height: f32,
    #[serde(default)]
    pub draft_starters: Vec<String>,
    #[serde(default)]
    pub keybindings: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub hidden_providers: Vec<String>,
    #[serde(skip)]
    pub theme_migrated: bool,
}

fn default_theme() -> ThemeMode {
    ThemeMode::System
}

fn default_scale() -> f32 {
    1.0
}

fn default_sidebar_width() -> f32 {
    248.0
}

fn default_zone_width() -> f32 {
    340.0
}

fn default_bottom_height() -> f32 {
    210.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            appearance: ThemeSettings::default(),
            workspace: None,
            endpoint: None,
            password: None,
            ui_scale: 1.0,
            compact_density: false,
            reduce_motion: false,
            accent: None,
            sidebar_width: 248.0,
            zone_width: 340.0,
            bottom_height: 210.0,
            draft_starters: Vec::new(),
            keybindings: std::collections::BTreeMap::new(),
            hidden_providers: Vec::new(),
            theme_migrated: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let (mut settings, legacy) = match path() {
            Some(path) => match std::fs::read_to_string(path) {
                Ok(text) => {
                    let has_appearance = serde_json::from_str::<serde_json::Value>(&text)
                        .ok()
                        .is_some_and(|value| value.get("appearance").is_some());
                    (
                        serde_json::from_str::<Self>(&text).unwrap_or_default(),
                        !has_appearance,
                    )
                }
                Err(_) => (Self::default(), true),
            },
            None => (Self::default(), true),
        };
        settings.theme_migrated = legacy;
        settings.migrate_shell_widths();
        crate::motion::set_reduced_motion(settings.reduce_motion);
        settings
    }

    fn migrate_shell_widths(&mut self) {
        if (self.sidebar_width - 280.0).abs() < f32::EPSILON {
            self.sidebar_width = default_sidebar_width();
        }
        if (self.zone_width - 640.0).abs() < f32::EPSILON {
            self.zone_width = default_zone_width();
        }
        if (self.bottom_height - 320.0).abs() < f32::EPSILON {
            self.bottom_height = default_bottom_height();
        }
    }

    pub fn resolve_theme_migration(&mut self, system_dark: bool) {
        if !self.theme_migrated {
            return;
        }
        self.theme_migrated = false;
        self.appearance.brightness = match self.theme {
            ThemeMode::Dark => 0.0,
            ThemeMode::Light => 1.0,
            ThemeMode::System => {
                if system_dark {
                    0.0
                } else {
                    1.0
                }
            }
        };
    }

    pub fn save(&self) {
        let Some(path) = path() else {
            return;
        };
        if let Ok(text) = serde_json::to_string_pretty(self) {
            write_private(&path, &text);
        }
    }
}

fn write_private(path: &std::path::Path, text: &str) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let tmp = path.with_extension(format!("tmp-{}-{stamp}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let write = options.open(&tmp).and_then(|mut file| {
        use std::io::Write;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    if write.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    let _ = std::fs::rename(tmp, path);
}

pub fn path() -> Option<PathBuf> {
    let base = directories::BaseDirs::new()?;
    Some(base.config_dir().join("rustchamber").join("settings.json"))
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DraftStore {
    #[serde(default)]
    pub drafts: std::collections::HashMap<String, String>,
}

pub fn load_drafts() -> std::collections::HashMap<String, String> {
    let Some(path) = drafts_path() else {
        return Default::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<DraftStore>(&text).ok())
        .map(|store| store.drafts)
        .unwrap_or_default()
}

pub fn save_drafts(drafts: &std::collections::HashMap<String, String>) {
    let Some(path) = drafts_path() else {
        return;
    };
    let store = DraftStore {
        drafts: drafts.clone(),
    };
    if let Ok(text) = serde_json::to_string_pretty(&store) {
        write_private(&path, &text);
    }
}

fn drafts_path() -> Option<PathBuf> {
    let base = directories::BaseDirs::new()?;
    Some(base.config_dir().join("rustchamber").join("drafts.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_starters_persist_and_default_to_empty() {
        let mut settings = Settings::default();
        assert!(settings.draft_starters.is_empty());
        settings.draft_starters = vec!["command:deploy".into(), "skill:code-review".into()];
        let text = serde_json::to_string(&settings).unwrap();
        let restored: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(restored.draft_starters, settings.draft_starters);
        let legacy: Settings = serde_json::from_str("{}").unwrap();
        assert!(legacy.draft_starters.is_empty());
    }

    #[test]
    fn appearance_persists_without_the_migration_marker() {
        let mut settings = Settings::default();
        settings.appearance.brightness = 0.25;
        settings.appearance.dark_theme = "nord-dark".into();
        let text = serde_json::to_string(&settings).unwrap();
        assert!(!text.contains("theme_migrated"));
        let restored: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(restored.appearance, settings.appearance);
        assert!(!restored.theme_migrated);
    }

    #[test]
    fn legacy_theme_mode_seeds_brightness_once() {
        let mut settings = Settings {
            theme: ThemeMode::Light,
            theme_migrated: true,
            ..Settings::default()
        };
        settings.resolve_theme_migration(true);
        assert_eq!(settings.appearance.brightness, 1.0);
        assert!(!settings.theme_migrated);
        settings.theme = ThemeMode::Dark;
        settings.resolve_theme_migration(false);
        assert_eq!(settings.appearance.brightness, 1.0);
        settings.theme_migrated = true;
        settings.resolve_theme_migration(true);
        assert_eq!(settings.appearance.brightness, 0.0);
        settings.theme = ThemeMode::System;
        settings.theme_migrated = true;
        settings.resolve_theme_migration(false);
        assert_eq!(settings.appearance.brightness, 1.0);
    }
}
