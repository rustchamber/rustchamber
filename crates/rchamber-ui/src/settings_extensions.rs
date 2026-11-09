use eframe::egui::{
    self, Align, CornerRadius, Id, Layout, Margin, RichText, ScrollArea, Stroke, Ui, Vec2,
};
use rchamber_app::Action;
use rchamber_ext::InstalledExtension;

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::settings::{page_column, page_header};
use crate::theme::{self, Palette};
use crate::widgets;

pub fn show(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    page_column(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
        page_header(
            ui,
            p,
            "Extensions",
            "Native extensions add panels, status sections, commands, mentions, actions and chat blocks.",
        );
        ui.add_space(18.0);
        add_row(ui, app, p);
        ui.add_space(18.0);
        if app.model.extensions.installed.is_empty() {
            empty(ui, app, p);
        } else {
            let installed = app.model.extensions.installed.clone();
            for extension in &installed {
                card(ui, app, p, extension);
                ui.add_space(12.0);
            }
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "An extension runs in a sandbox with only the permissions you approve. Folder installs reload when you save; archives are copied into RustChamber.",
            )
            .font(theme::font(theme::TEXT_META))
            .color(p.fg_faint),
        );
    });
    approval_dialog(ui, app, p);
    remove_dialog(ui, app, p);
}

fn add_row(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let field = egui::TextEdit::singleline(&mut app.ui.extension_path)
            .hint_text("Folder or .zip path")
            .font(theme::font(theme::TEXT_UI_LABEL))
            .text_color(p.fg)
            .desired_width((ui.available_width() - 300.0).max(180.0))
            .margin(Margin::symmetric(9, 7));
        let response = ui.add(field);
        let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if widgets::primary_button(ui, "Add", Some(Icon::Plus), p).clicked() {
            install(app);
        }
        if enter {
            install(app);
        }
        if widgets::ghost_button(ui, "Folder", Some(Icon::Folder), p).clicked()
            && let Some(folder) = rfd::FileDialog::new().pick_folder()
        {
            app.ui.extension_path = folder.to_string_lossy().to_string();
            install(app);
        }
        if widgets::ghost_button(ui, "Archive", Some(Icon::Upload), p).clicked()
            && let Some(file) = rfd::FileDialog::new()
                .add_filter("Extension archive", &["zip"])
                .pick_file()
        {
            app.ui.extension_path = file.to_string_lossy().to_string();
            install(app);
        }
    });
}

fn install(app: &mut RustChamber) {
    let path = app.ui.extension_path.trim().to_string();
    if path.is_empty() {
        return;
    }
    if path.to_ascii_lowercase().starts_with("https://") {
        app.backend.send(Action::InstallExtensionUrl { url: path });
    } else {
        app.backend.send(Action::InstallExtension { path });
    }
    app.ui.extension_path.clear();
}

fn empty(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(theme::HAIRLINE, p.border))
        .corner_radius(CornerRadius::same(theme::radius::LG))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new("No extensions yet")
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Type /new-extension in a chat and describe what you want. The agent writes the folder and offers an Install button.",
                )
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg_muted),
            );
            ui.add_space(10.0);
            if widgets::ghost_button(ui, "New extension", Some(Icon::Puzzle), p).clicked() {
                app.begin_new_extension();
            }
        });
}

fn card(ui: &mut Ui, app: &mut RustChamber, p: &Palette, extension: &InstalledExtension) {
    let id = extension.manifest.id.clone();
    let status = if extension.needs_approval() {
        ("Needs approval", p.coral)
    } else if extension.enabled {
        ("On", p.green)
    } else {
        ("Off", p.fg_muted)
    };
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(theme::HAIRLINE, p.border))
        .corner_radius(CornerRadius::same(theme::radius::LG))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                icons::icon(
                    ui,
                    crate::icons::extension_icon(extension.manifest.icon.as_deref()),
                    18.0,
                    p.fg_muted,
                );
                ui.label(
                    RichText::new(&extension.manifest.name)
                        .font(theme::font_semibold(15.0))
                        .color(p.fg),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(format!("v{}", extension.manifest.version))
                        .font(theme::font(theme::TEXT_META))
                        .color(p.fg_faint),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(status.0)
                            .font(theme::font(theme::TEXT_META))
                            .color(status.1),
                    );
                });
            });
            if let Some(description) = &extension.manifest.description {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(description)
                        .font(theme::font(theme::TEXT_UI_LABEL))
                        .color(p.fg_muted),
                );
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    extension.source.label(),
                    extension.root.to_string_lossy()
                ))
                .font(theme::font_mono(11.0))
                .color(p.fg_faint),
            );
            let caps = extension.manifest.capability_summary();
            if !caps.is_empty() {
                ui.add_space(10.0);
                for line in caps {
                    ui.label(
                        RichText::new(format!("• {line}"))
                            .font(theme::font(theme::TEXT_META))
                            .color(p.fg_muted),
                    );
                }
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if extension.needs_approval() {
                    if widgets::primary_button(ui, "Review permissions", Some(Icon::Shield), p)
                        .clicked()
                    {
                        app.ui.extension_approval = Some(id.clone());
                    }
                } else if !extension.enabled
                    && widgets::ghost_button(ui, "Review", Some(Icon::Shield), p).clicked()
                {
                    app.ui.extension_approval = Some(id.clone());
                } else if extension.enabled {
                    if widgets::ghost_button(ui, "Disable", None, p).clicked() {
                        app.backend.send(Action::SetExtensionEnabled {
                            id: id.clone(),
                            enabled: false,
                        });
                    }
                } else if widgets::primary_button(ui, "Enable", Some(Icon::Check), p).clicked() {
                    app.backend.send(Action::SetExtensionEnabled {
                        id: id.clone(),
                        enabled: true,
                    });
                    if extension.manifest.contributes.panel.is_some() {
                        open_panel(app, &id);
                    }
                }
                if extension.enabled
                    && !extension.needs_approval()
                    && widgets::ghost_button(ui, "Review", Some(Icon::Shield), p).clicked()
                {
                    app.ui.extension_approval = Some(id.clone());
                }
                if extension.manifest.contributes.panel.is_some()
                    && extension.is_active()
                    && widgets::ghost_button(ui, "Open panel", Some(Icon::PanelRight), p).clicked()
                {
                    app.ui.surface = crate::surfaces::Surface::Extension;
                    app.ui.extension_panel = Some(id.clone());
                    app.ui.context_open = true;
                    app.ui.show_settings = false;
                    app.backend.send(Action::ExtensionCall {
                        id: id.clone(),
                        call_id: "panel".into(),
                        method: "panel".into(),
                        args: Vec::new(),
                    });
                }
                if widgets::ghost_button(ui, "Reload", Some(Icon::Refresh), p).clicked() {
                    app.backend.send(Action::ReloadExtension(id.clone()));
                }
                if widgets::ghost_button(ui, "Remove", Some(Icon::Trash), p).clicked() {
                    app.ui.extension_remove = Some(id.clone());
                }
            });
            if let Some(error) = app
                .model
                .extensions
                .errors
                .get(&id)
                .map(|error| &error.message)
            {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(error)
                        .font(theme::font(theme::TEXT_META))
                        .color(p.red),
                );
            }
        });
}

fn approval_dialog(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let Some(id) = app.ui.extension_approval.clone() else {
        return;
    };
    let Some(extension) = app
        .model
        .extensions
        .installed
        .iter()
        .find(|extension| extension.manifest.id == id)
        .cloned()
    else {
        app.ui.extension_approval = None;
        return;
    };
    let mut close = false;
    let mut allow = false;
    let frame = egui::Frame::new()
        .fill(p.bg)
        .corner_radius(theme::radius::XL)
        .inner_margin(Margin::same(20));
    let modal = egui::Modal::new(Id::unique("extension-approval"))
        .frame(frame)
        .show(ui.ctx(), |ui| {
            ui.set_width(480.0);
            ui.label(
                RichText::new(format!("Allow {}?", extension.manifest.name))
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "The extension asks for the following. Allow the whole list or remove the extension.",
                )
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg_muted),
            );
            ui.add_space(10.0);
            ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    for line in extension.manifest.review_lines() {
                        ui.label(
                            RichText::new(format!("• {line}"))
                                .font(theme::font(theme::TEXT_UI_LABEL))
                                .color(p.fg),
                        );
                        ui.add_space(4.0);
                    }
                });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, "Allow and enable", Some(Icon::ShieldCheck), p)
                    .clicked()
                {
                    allow = true;
                }
                if widgets::ghost_button(ui, "Not now", None, p).clicked() {
                    close = true;
                }
            });
        });
    if modal.should_close() {
        close = true;
    }
    if allow {
        app.backend.send(Action::ApproveExtension {
            id: id.clone(),
            grants: extension.manifest.requested_capabilities(),
        });
        app.ui.extension_approval = None;
        if extension.manifest.contributes.panel.is_some() {
            open_panel(app, &id);
        }
    } else if close {
        app.ui.extension_approval = None;
    }
}

fn remove_dialog(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let Some(id) = app.ui.extension_remove.clone() else {
        return;
    };
    let name = app
        .model
        .extensions
        .installed
        .iter()
        .find(|extension| extension.manifest.id == id)
        .map(|extension| extension.manifest.name.clone())
        .unwrap_or_else(|| id.clone());
    let mut close = false;
    let mut remove = false;
    let frame = egui::Frame::new()
        .fill(p.bg)
        .corner_radius(theme::radius::XL)
        .inner_margin(Margin::same(20));
    let modal = egui::Modal::new(Id::unique("extension-remove"))
        .frame(frame)
        .show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.label(
                RichText::new(format!("Remove {name}?"))
                    .font(theme::font_semibold(15.0))
                    .color(p.fg),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Its saved data and approved permissions are deleted. A folder install is not touched on disk.",
                )
                .font(theme::font(theme::TEXT_UI_LABEL))
                .color(p.fg_muted),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if widgets::destructive_button(ui, "Remove", Some(Icon::Trash), p).clicked() {
                    remove = true;
                }
                if widgets::ghost_button(ui, "Cancel", None, p).clicked() {
                    close = true;
                }
            });
        });
    if modal.should_close() {
        close = true;
    }
    if remove {
        app.backend.send(Action::RemoveExtension(id));
        app.ui.extension_remove = None;
    } else if close {
        app.ui.extension_remove = None;
    }
}

fn open_panel(app: &mut RustChamber, id: &str) {
    app.ui.surface = crate::surfaces::Surface::Extension;
    app.ui.extension_panel = Some(id.to_string());
    app.ui.context_open = true;
    app.ui.show_settings = false;
    app.backend.send(Action::ExtensionCall {
        id: id.to_string(),
        call_id: "panel".into(),
        method: "panel".into(),
        args: Vec::new(),
    });
}
