use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, CornerRadius, Margin, RichText, ScrollArea, Sense, Stroke, Ui, Vec2};
use rchamber_app::format::duration;

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets;

#[derive(Clone, Debug, Default)]
pub struct RunState {
    pub running: bool,
    pub output: String,
    pub status: Option<i32>,
    pub elapsed_ms: f64,
}

type Store = HashMap<egui::Id, Arc<Mutex<RunState>>>;

fn store_id() -> egui::Id {
    egui::Id::unique("shell-run-store")
}

pub fn run(ctx: &egui::Context, id: egui::Id, code: &str) {
    let state = Arc::new(Mutex::new(RunState {
        running: true,
        ..RunState::default()
    }));
    ctx.data_mut(|data| {
        let mut store: Store = data.get_temp(store_id()).unwrap_or_default();
        store.insert(id, state.clone());
        data.insert_temp(store_id(), store);
    });
    let cwd = ctx
        .data(|data| data.get_temp::<String>(egui::Id::unique("shell-cwd")))
        .filter(|value| !value.trim().is_empty());
    let code = code.to_string();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.arg("/C").arg(&code);
            command
        } else {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
            let mut command = Command::new(shell);
            command.arg("-lc").arg(&code);
            command
        };
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        let output = command.stdin(Stdio::null()).output();
        let mut state = state.lock().unwrap();
        state.elapsed_ms = started.elapsed().as_millis() as f64;
        state.running = false;
        match output {
            Ok(output) => {
                state.status = output.status.code();
                let mut text = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.trim().is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&stderr);
                }
                if text.len() > 200_000 {
                    text.truncate(200_000);
                    text.push_str("\n… truncated");
                }
                state.output = text;
            }
            Err(error) => {
                state.output = format!("Could not run the command: {error}");
                state.status = Some(-1);
            }
        }
        ctx.request_repaint();
    });
}

pub fn show(ui: &mut Ui, id: egui::Id, p: &Palette) {
    let Some(state) = ui.ctx().data(|data| {
        data.get_temp::<Store>(store_id())
            .and_then(|store| store.get(&id).cloned())
    }) else {
        return;
    };
    let state = state.lock().unwrap().clone();
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(p.code_bg)
        .stroke(Stroke::new(theme::BORDER, p.border))
        .corner_radius(CornerRadius::same(theme::radius::TABLE))
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                let (label, ink, icon) = if state.running {
                    ("Running", p.primary_text, Some(Icon::Clock))
                } else {
                    match state.status {
                        Some(0) => ("Finished", p.green, Some(Icon::Check)),
                        Some(_) => ("Failed", p.error_text, Some(Icon::Warning)),
                        None => ("Finished", p.fg_muted, None),
                    }
                };
                if let Some(icon) = icon {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                    icons::paint(ui, icon, icon_rect, ink);
                }
                ui.add_space(2.0);
                ui.label(
                    RichText::new(label)
                        .font(theme::font_mono(11.5))
                        .color(ink),
                );
                if !state.running {
                    match state.status {
                        Some(code) if code != 0 => {
                            ui.label(
                                RichText::new(format!("exit {code}"))
                                    .font(theme::font_mono(11.5))
                                    .color(p.error_text),
                            );
                        }
                        _ => {}
                    }
                }
                if !state.running && state.elapsed_ms > 0.0 {
                    ui.label(
                        RichText::new(duration(state.elapsed_ms))
                            .font(theme::font_mono(11.5))
                            .color(p.fg_faint),
                    );
                }
            });
            if !state.output.trim().is_empty() {
                ui.add_space(6.0);
                ScrollArea::vertical()
                    .id_salt(id.with("output"))
                    .max_height(260.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(state.output.clone())
                                    .font(theme::font_mono(theme::TEXT_CODE))
                                    .color(p.fg)
                                    .line_height(Some(theme::LINE_CODE_ROW)),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
            }
        });
    if state.running {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(120));
    }
}

pub fn run_button(ui: &mut Ui, p: &Palette, id: egui::Id, code: &str) -> bool {
    if widgets::icon_button_hit(ui, Icon::Terminal, 13.0, 24.0, p.fg_muted)
        .on_hover_text("Run this command")
        .clicked()
    {
        run(ui.ctx(), id, code);
        return true;
    }
    false
}
