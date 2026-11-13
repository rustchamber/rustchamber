use std::sync::Arc;

use eframe::egui::{self, Align, Layout, Margin, ScrollArea, Stroke, Ui, Vec2};
use rchamber_app::Action;
use rchamber_ext::{BlockResponse, InstalledExtension, Widget};

use crate::app::RustChamber;
use crate::blocks::{self, BlockHost};
use crate::icons::Icon;
use crate::surfaces::{self, PanelTab};
use crate::theme::{self, Palette};

pub fn show(ui: &mut Ui, app: &mut RustChamber) {
    let p = app.palette.clone();
    let id = app.ui.extension_panel.clone();
    let extension = id
        .as_ref()
        .and_then(|id| {
            app.model
                .extensions
                .installed
                .iter()
                .find(|extension| extension.manifest.id == *id)
        })
        .cloned();
    let active = extension
        .as_ref()
        .is_some_and(InstalledExtension::is_active);
    let title = extension
        .as_ref()
        .map(|extension| extension.manifest.panel_title().to_string())
        .or_else(|| id.clone())
        .unwrap_or_else(|| "Extension".into());
    let icon = extension
        .as_ref()
        .map(|extension| crate::icons::extension_icon(extension.manifest.icon.as_deref()))
        .unwrap_or(Icon::Puzzle);

    if let (Some(id), true) = (id.as_ref(), active) {
        let loaded = app.model.extensions.panels.contains_key(id);
        if !loaded {
            let requested = ui.ctx().data(|data| {
                data.get_temp::<bool>(egui::Id::unique(("ext-panel-requested", id.as_str())))
                    .unwrap_or(false)
            });
            if !requested {
                app.backend.send(Action::ExtensionCall {
                    id: id.clone(),
                    call_id: "panel".into(),
                    method: "panel".into(),
                    args: Vec::new(),
                });
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique(("ext-panel-requested", id.as_str())), true)
                });
            }
        }
    }

    let tab = PanelTab::new(title, icon).active(true);
    let expanded = app.ui.expanded;
    let mut close = false;
    let mut toggle_expanded = false;
    surfaces::panel_header(
        ui,
        &p,
        &[tab],
        |_| {},
        |ui| {
            if surfaces::close_button(ui, &p).clicked() {
                close = true;
            }
            if surfaces::expand_button(ui, &p, expanded).clicked() {
                toggle_expanded = true;
            }
            if active
                && id.is_some()
                && crate::surfaces::panel_action(ui, &p, Icon::Refresh, true, "Reload the panel")
                    .clicked()
            {
                app.backend.send(Action::ExtensionCall {
                    id: id.clone().unwrap_or_default(),
                    call_id: "panel".into(),
                    method: "panel".into(),
                    args: Vec::new(),
                });
            }
        },
    );
    if close {
        app.ui.extension_panel = None;
        app.ui.surface = crate::surfaces::Surface::Session;
        app.ui.context_open = false;
        return;
    }
    if toggle_expanded {
        app.ui.expanded = !app.ui.expanded;
    }

    let Some(id) = id else {
        surfaces::empty_state(ui, &p, Icon::Puzzle, "No extension panel", "");
        return;
    };
    let Some(extension) = extension else {
        surfaces::empty_state(ui, &p, Icon::Puzzle, "Extension removed", "");
        return;
    };
    if !extension.is_active() {
        surfaces::empty_state(
            ui,
            &p,
            Icon::Puzzle,
            "Extension is off",
            "Enable it in Settings → Extensions.",
        );
        return;
    }

    ui.add_space(6.0);
    let host = host_for(app, &id);
    blocks::set_host(ui.ctx(), host);
    ScrollArea::vertical()
        .id_salt(("ext-panel", id.as_str()))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(16.0));
            let error = app
                .model
                .extensions
                .errors
                .get(&id)
                .map(|error| error.message.clone());
            if let Some(error) = &error {
                egui::Frame::new()
                    .fill(p.error_soft)
                    .stroke(Stroke::new(theme::HAIRLINE, p.error_border))
                    .corner_radius(egui::CornerRadius::same(theme::radius::MD))
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(error)
                                .font(theme::font(theme::TEXT_META))
                                .color(p.error_text),
                        );
                    });
                ui.add_space(6.0);
            }
            let Some(widgets) = app.model.extensions.panels.get(&id).cloned() else {
                if error.is_none() {
                    loading(ui, &p);
                }
                return;
            };
            if app.ui.zone == crate::surfaces::PanelZone::Right && !app.ui.expanded {
                fit_width(ui, app, &id, &widgets);
            }
            egui::Frame::new()
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    blocks::show_widgets(ui, &widgets, &p, theme::TEXT_MARKDOWN);
                });
        });
}

const PANEL_CHROME: f32 = 50.0;

fn fit_width(ui: &Ui, app: &mut RustChamber, id: &str, widgets: &[Widget]) {
    if app.ui.extension_user_widths.contains(id) {
        return;
    }
    let revision = app.model.extensions.panel_revision(id);
    let slot = egui::Id::unique(("ext-panel-fit", id));
    if ui
        .ctx()
        .data(|data| data.get_temp::<u64>(slot))
        .is_some_and(|fitted| fitted == revision)
    {
        return;
    }
    let desired = (blocks::min_width(ui, widgets, theme::TEXT_MARKDOWN) + PANEL_CHROME)
        .max(crate::app::ZONE_MIN);
    app.ui.extension_widths.insert(id.to_string(), desired);
    ui.ctx().data_mut(|data| data.insert_temp(slot, revision));
    ui.ctx().request_repaint();
}

pub(crate) fn host_for(app: &RustChamber, id: &str) -> BlockHost {
    let actions = app.backend.actions.clone();
    let panel_id = id.to_string();
    let submit_actions = actions.clone();
    let submit_id = panel_id.clone();
    let tree_blocks = app.model.extensions.blocks.clone();
    let tree_actions = actions.clone();
    BlockHost {
        submit: Some(Arc::new(move |response: BlockResponse| {
            let value = response
                .value
                .clone()
                .or_else(|| response.button.clone())
                .unwrap_or_default();
            let values = serde_json::Value::Object(response.values.clone());
            let _ = submit_actions.send(Action::ExtensionCall {
                id: submit_id.clone(),
                call_id: "panel-action".into(),
                method: "command".into(),
                args: vec![serde_json::Value::String(value), values],
            });
        })),
        request: Some(Arc::new(move |name: &str, args: &[String]| {
            let _ = tree_actions.send(Action::ExtensionBlock {
                name: name.to_string(),
                args: args.to_vec(),
            });
        })),
        tree: Some(Arc::new(move |name: &str, args: &[String]| {
            let key = format!("{name}:{}", args.join("\u{1f}"));
            tree_blocks.get(&key).cloned()
        })),
    }
}

fn loading(ui: &mut Ui, p: &Palette) {
    ui.add_space(18.0);
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add(egui::Spinner::new().size(16.0).color(p.fg_muted));
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Loading panel…")
                .font(theme::font(theme::TEXT_META))
                .color(p.fg_muted),
        );
        let _ = Vec2::splat(0.0);
    });
}
