//! Desktop color-scheme detection for Linux.
//!
//! winit returns `None` from `system_theme()` on Linux, so egui cannot tell
//! whether the desktop prefers a dark appearance. The XDG desktop portal
//! exposes the same preference as `org.freedesktop.appearance color-scheme`,
//! and works both for native apps and inside a Flatpak sandbox.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const UNKNOWN: u8 = 0;
const DARK: u8 = 1;
const LIGHT: u8 = 2;

#[derive(Clone)]
pub struct SystemTheme {
    dark: Arc<AtomicU8>,
}

impl SystemTheme {
    pub fn spawn(wake: impl Fn() + Send + Sync + 'static) -> Self {
        #[cfg(target_os = "linux")]
        {
            let dark = Arc::new(AtomicU8::new(UNKNOWN));
            let flag = Arc::clone(&dark);
            let _ = std::thread::Builder::new()
                .name("rchamber-system-theme".into())
                .spawn(move || {
                    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    else {
                        return;
                    };
                    runtime.block_on(async move {
                        let Ok(settings) = ashpd::desktop::settings::Settings::new().await else {
                            return;
                        };
                        if let Ok(scheme) = settings.color_scheme().await
                            && apply(&flag, scheme)
                        {
                            wake();
                        }
                        let Ok(mut stream) = settings.receive_color_scheme_changed().await else {
                            return;
                        };
                        use futures_util::StreamExt as _;
                        while let Some(scheme) = stream.next().await {
                            if apply(&flag, scheme) {
                                wake();
                            }
                        }
                    });
                });
            Self { dark }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = wake;
            Self {
                dark: Arc::new(AtomicU8::new(UNKNOWN)),
            }
        }
    }

    /// `None` until the portal answers; callers can fall back to egui's value.
    pub fn dark(&self) -> Option<bool> {
        match self.dark.load(Ordering::Relaxed) {
            DARK => Some(true),
            LIGHT => Some(false),
            _ => None,
        }
    }
}

#[cfg(target_os = "linux")]
fn apply(flag: &AtomicU8, scheme: ashpd::desktop::settings::ColorScheme) -> bool {
    use ashpd::desktop::settings::ColorScheme;
    let value = match scheme {
        ColorScheme::PreferDark => DARK,
        ColorScheme::PreferLight | ColorScheme::NoPreference => LIGHT,
    };
    flag.swap(value, Ordering::Relaxed) != value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_until_a_value_arrives() {
        let theme = SystemTheme {
            dark: Arc::new(AtomicU8::new(UNKNOWN)),
        };
        assert_eq!(theme.dark(), None);
        theme.dark.store(DARK, Ordering::Relaxed);
        assert_eq!(theme.dark(), Some(true));
        theme.dark.store(LIGHT, Ordering::Relaxed);
        assert_eq!(theme.dark(), Some(false));
    }
}
