use eframe::egui::{
    self, Align, Color32, CornerRadius, Id, Layout, Margin, Modifiers, Rect, RichText, ScrollArea,
    Sense, Stroke, Ui, Vec2,
};
use rchamber_update::{InstallKind, Phase, Snapshot, UpdateInfo, Updater, Wake};

use crate::app::RustChamber;
use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets;

const WIDTH: f32 = 680.0;
const MAX_HEIGHT: f32 = 660.0;
const NARROW_GAP: f32 = 48.0;
const PAD: f32 = 20.0;
const HEADER_HEIGHT: f32 = 32.0;
const HEADER_GAP: f32 = 14.0;
const NOTES_GAP: f32 = 12.0;
const NOTES_MIN: f32 = 150.0;
const NOTES_MAX: f32 = 380.0;
const FOOTER_HEIGHT: f32 = 32.0;
const ERROR_HEIGHT: f32 = 48.0;
const PROGRESS_HEIGHT: f32 = 42.0;

pub fn app_version() -> &'static str {
    option_env!("RCHAMBER_VERSION")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub struct UpdateCenter {
    updater: Updater,
    dialog_open: bool,
    auto_open: bool,
    open_on_found: bool,
    open_requested: bool,
    local_error: Option<String>,
}

impl UpdateCenter {
    pub fn new(current_version: &str, wake: Wake) -> Self {
        let auto_open = std::env::var("RCHAMBER_UPDATE_DIALOG").is_ok_and(|value| value == "1");
        Self {
            updater: Updater::spawn(current_version, wake),
            dialog_open: false,
            auto_open,
            open_on_found: false,
            open_requested: false,
            local_error: None,
        }
    }

    pub fn poll(&mut self) {
        if !self.updater.poll() {
            return;
        }
        let snapshot = self.updater.snapshot();
        if snapshot.info.is_some() && (self.auto_open || self.open_on_found) {
            self.auto_open = false;
            self.open_on_found = false;
            self.dialog_open = true;
            self.open_requested = true;
        } else if snapshot.info.is_none() && !snapshot.busy() && snapshot.phase != Phase::Checking {
            self.open_on_found = false;
        }
    }

    pub fn take_open_request(&mut self) -> bool {
        std::mem::take(&mut self.open_requested)
    }

    pub fn check(&mut self) {
        self.open_on_found = self.updater.snapshot().info.is_none();
        self.updater.check();
    }

    pub fn snapshot(&self) -> &Snapshot {
        self.updater.snapshot()
    }

    pub fn available(&self) -> Option<&UpdateInfo> {
        self.snapshot().info.as_ref()
    }

    pub fn is_open(&self) -> bool {
        self.dialog_open
    }

    pub fn open(&mut self) {
        self.dialog_open = true;
        self.local_error = None;
    }

    pub fn close(&mut self) {
        self.dialog_open = false;
    }

    pub fn download(&mut self) {
        self.local_error = None;
        self.updater.download();
    }

    pub fn restart(&self) -> Result<(), String> {
        self.updater.restart()
    }

    pub fn fail(&mut self, message: String) {
        self.local_error = Some(message);
    }

    fn error(&self) -> Option<&str> {
        self.local_error
            .as_deref()
            .or(self.snapshot().error.as_deref())
    }
}

pub fn show(ctx: &egui::Context, app: &mut RustChamber) {
    if !app.updates.is_open() {
        return;
    }
    let snapshot = app.updates.snapshot().clone();
    let Some(info) = snapshot.info.clone() else {
        return;
    };
    let p = app.palette.clone();
    let e = app.updates.error().map(str::to_string);
    let mut close = false;
    let mut download = false;
    let mut restart = false;
    let mut open_url: Option<String> = None;

    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, egui::Key::Escape)) {
        close = true;
    }

    let screen = ctx.content_rect();
    let width = (screen.width() - NARROW_GAP).clamp(320.0, WIDTH);
    let max_height = (screen.height() - 48.0).clamp(320.0, MAX_HEIGHT);
    let progress = snapshot.phase == Phase::Downloading || snapshot.phase == Phase::Updating;
    let error_space = if e.is_some() { ERROR_HEIGHT + 8.0 } else { 0.0 };
    let progress_space = if progress {
        PROGRESS_HEIGHT + 10.0
    } else {
        0.0
    };
    let detail_space = if info.detail.is_some() { 26.0 } else { 0.0 };
    let fixed = PAD * 2.0
        + HEADER_HEIGHT
        + HEADER_GAP
        + NOTES_GAP
        + FOOTER_HEIGHT
        + error_space
        + progress_space
        + detail_space;
    let notes_height = (max_height - fixed).clamp(NOTES_MIN, NOTES_MAX);
    let height = fixed + notes_height;
    let card = Rect::from_center_size(screen.center(), Vec2::new(width, height));

    let shadow = egui::epaint::Shadow {
        offset: [0, 28],
        blur: 64,
        spread: 0,
        color: p.shadow,
    };
    let scrim = if p.dark {
        Color32::from_black_alpha(120)
    } else {
        Color32::from_black_alpha(52)
    };

    let modal = egui::Modal::new(Id::unique("update-dialog"))
        .frame(egui::Frame::NONE)
        .backdrop_color(scrim)
        .show(ctx, |ui| {
            let (card_rect, _) = ui.allocate_exact_size(card.size(), Sense::hover());
            paint_card(ui, card_rect, &p, &shadow);
            let inner = card_rect.shrink(PAD);
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::top_down(Align::Min)),
            );
            body.spacing_mut().item_spacing = Vec2::ZERO;

            body.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (badge, _) =
                    ui.allocate_exact_size(Vec2::new(28.0, HEADER_HEIGHT), Sense::hover());
                ui.painter()
                    .circle_filled(badge.center(), 14.0, p.alpha(p.blue, 30));
                icons::paint(
                    ui,
                    Icon::Download,
                    Rect::from_center_size(badge.center(), Vec2::splat(15.0)),
                    p.blue,
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new("Update available")
                        .font(theme::font_semibold(15.0))
                        .color(p.fg),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(&info.current_version)
                        .font(theme::font_mono(12.0))
                        .color(p.fg_muted),
                );
                ui.add_space(6.0);
                ui.label(RichText::new("→").font(theme::font(13.0)).color(p.fg_faint));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(&info.version)
                        .font(theme::font_mono(12.0))
                        .color(p.blue),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::Close, 15.0, p.fg_muted).clicked() {
                        close = true;
                    }
                });
            });
            body.add_space(HEADER_GAP);

            egui::Frame::new()
                .fill(if p.dark { p.bg_soft } else { p.card })
                .stroke(Stroke::new(theme::HAIRLINE, p.elevation_ring))
                .corner_radius(CornerRadius::same(theme::radius::LG))
                .inner_margin(Margin::same(16))
                .show(&mut body, |ui| {
                    ui.set_width((width - PAD * 2.0 - 32.0).max(0.0));
                    ui.set_height((notes_height - 32.0).max(0.0));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        egui::Frame::new()
                            .fill(p.alpha(p.blue, 26))
                            .corner_radius(CornerRadius::same(theme::radius::XS))
                            .inner_margin(Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("v{}", info.version))
                                        .font(theme::font_mono(11.5))
                                        .color(p.blue),
                                );
                            });
                        if let Some(date) = &info.published_label {
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(date)
                                    .font(theme::font(12.0))
                                    .color(p.fg_muted),
                            );
                        }
                        if info.install == InstallKind::Homebrew {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new("Homebrew")
                                        .font(theme::font(11.5))
                                        .color(p.fg_faint),
                                );
                            });
                        }
                    });
                    ui.add_space(8.0);
                    ScrollArea::vertical()
                        .id_salt("update-notes")
                        .max_height((notes_height - 32.0 - 34.0).max(0.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            app.md
                                .show_at(ui, Id::unique("update-notes-body"), &info.notes, &p);
                        });
                });
            body.add_space(NOTES_GAP);

            if let Some(detail) = &info.detail {
                body.label(
                    RichText::new(detail)
                        .font(theme::font(12.0))
                        .color(p.fg_muted),
                );
                body.add_space(8.0);
            }

            if let Some(error) = &e {
                let lines: Vec<&str> = error
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .collect();
                let text = lines[lines.len().saturating_sub(3)..].join("\n");
                body.label(
                    RichText::new(text)
                        .font(theme::font(12.0))
                        .color(p.error_text),
                );
                body.add_space(8.0);
            }

            if progress {
                progress_block(&mut body, &snapshot, &p, width - PAD * 2.0);
                body.add_space(10.0);
            }

            body.with_layout(Layout::bottom_up(Align::Min), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.horizontal(|ui| {
                    if widgets::link_button(ui, "GitHub", Some(Icon::ArrowUpRight), &p).clicked() {
                        open_url = Some(info.release_url.clone());
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        actions(
                            ui,
                            &snapshot,
                            &info,
                            &p,
                            &mut download,
                            &mut restart,
                            &mut open_url,
                        );
                    });
                });
            });
        });

    if modal.backdrop_response.clicked() {
        close = true;
    }
    if close {
        app.updates.close();
    }
    if download {
        app.updates.download();
    }
    if restart {
        match app.updates.restart() {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(error) => app.updates.fail(error),
        }
    }
    if let Some(url) = open_url {
        ctx.open_url(egui::OpenUrl::new_tab(url));
    }
}

fn actions(
    ui: &mut Ui,
    snapshot: &Snapshot,
    info: &UpdateInfo,
    p: &Palette,
    download: &mut bool,
    restart: &mut bool,
    open_url: &mut Option<String>,
) {
    match snapshot.phase {
        Phase::Ready => {
            if widgets::button(
                ui,
                "Restart to update",
                Some(Icon::Restart),
                widgets::ButtonVariant::Success,
                widgets::ButtonSize::Md,
                p,
            )
            .clicked()
            {
                *restart = true;
            }
        }
        Phase::Downloading => {
            widgets::spinner(ui, 14.0, p.fg_muted);
            let _ = widgets::button_ex(
                ui,
                "Downloading…",
                None,
                widgets::ButtonVariant::Primary,
                widgets::ButtonSize::Md,
                false,
                p,
            );
        }
        Phase::Updating => {
            widgets::spinner(ui, 14.0, p.fg_muted);
            let _ = widgets::button_ex(
                ui,
                "Updating…",
                None,
                widgets::ButtonVariant::Primary,
                widgets::ButtonSize::Md,
                false,
                p,
            );
        }
        _ => match info.install {
            InstallKind::External => {
                if widgets::button(
                    ui,
                    "Open in GitHub",
                    Some(Icon::ArrowUpRight),
                    widgets::ButtonVariant::Primary,
                    widgets::ButtonSize::Md,
                    p,
                )
                .clicked()
                {
                    *open_url = Some(info.release_url.clone());
                }
            }
            InstallKind::Homebrew => {
                let label = if snapshot.error.is_some() {
                    "Try again"
                } else {
                    "Update with Homebrew"
                };
                if widgets::button(
                    ui,
                    label,
                    Some(Icon::Download),
                    widgets::ButtonVariant::Primary,
                    widgets::ButtonSize::Md,
                    p,
                )
                .clicked()
                {
                    *download = true;
                }
            }
            InstallKind::Native => {
                let label = if snapshot.error.is_some() {
                    "Try again"
                } else {
                    "Download update"
                };
                if widgets::button(
                    ui,
                    label,
                    Some(Icon::Download),
                    widgets::ButtonVariant::Primary,
                    widgets::ButtonSize::Md,
                    p,
                )
                .clicked()
                {
                    *download = true;
                }
            }
        },
    }
}

fn progress_block(ui: &mut Ui, snapshot: &Snapshot, p: &Palette, width: f32) {
    ui.horizontal(|ui| {
        if snapshot.phase == Phase::Updating {
            widgets::spinner(ui, 13.0, p.fg_muted);
            ui.add_space(6.0);
            ui.label(
                RichText::new("Updating with Homebrew…")
                    .font(theme::font(12.0))
                    .color(p.fg_muted),
            );
            return;
        }
        ui.label(
            RichText::new("Downloading update…")
                .font(theme::font(12.0))
                .color(p.fg_muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{}%", (snapshot.progress() * 100.0).round() as i32))
                    .font(theme::font_mono(12.0))
                    .color(p.fg),
            );
        });
    });
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 5.0), Sense::hover());
    let radius = CornerRadius::same(3);
    ui.painter().rect_filled(rect, radius, p.subtle);
    let filled = Rect::from_min_size(
        rect.min,
        Vec2::new(rect.width() * snapshot.progress(), rect.height()),
    );
    if filled.width() > 0.0 {
        ui.painter().rect_filled(filled, radius, p.primary);
    }
}

fn paint_card(ui: &Ui, rect: Rect, p: &Palette, shadow: &egui::epaint::Shadow) {
    let corner = CornerRadius::same(theme::radius::XL);
    ui.painter()
        .add(egui::Shape::Rect(shadow.as_shape(rect, corner)));
    ui.painter().rect_filled(rect, corner, p.bg);
    ui.painter().rect_stroke(
        rect,
        corner,
        Stroke::new(theme::HAIRLINE, p.elevation_ring),
        egui::StrokeKind::Inside,
    );
}

pub fn sidebar_badge(ui: &mut Ui, app: &mut RustChamber, p: &Palette) {
    let Some(info) = app.updates.available().cloned() else {
        return;
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    widgets::pointer(ui, &response);
    let hovered = crate::app::hover_amount(ui.ctx(), response.id.with("hover"), response.hovered());
    if hovered > 0.0 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::radius::MD),
            p.scale_alpha(p.blue, 0.14 * hovered),
        );
    }
    icons::paint(
        ui,
        Icon::Download,
        Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
        p.blue.lerp_to_gamma(p.fg, hovered * 0.15),
    );
    if response
        .on_hover_text(format!("Update available · v{}", info.version))
        .clicked()
    {
        app.updates.open();
    }
}
