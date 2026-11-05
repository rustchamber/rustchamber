use eframe::egui::{self, RichText, ScrollArea};
use rchamber_ui::markdown::Markdown;
use rchamber_ui::theme;

const SAMPLE: &str = r#"# Heading

This is a paragraph with a long line that should wrap across the column width and not overlap anything below it. It has `inline code` and **bold** text.

```bash
cd /Users/felipesilvamachado/Projects/lipecode/rustchamber
pkill -f "opencode serve --hostname 127.0.0.1" 2>/dev/null
cargo build 2>&1 | tail -3
RCHAMBER_THEME=dark RCHAMBER_OPEN_SESSION=ses_e5dca984effekIWW6De8HWS0qh cargo run
```

After the code block, this paragraph must appear below it.

Another code block:

```rust
fn main() {
    println!("hello");
    let x = 1 + 2;
    assert_eq!(x, 3);
}
```

The end.
"#;

struct Probe {
    md: Markdown,
    palette: theme::Palette,
    frames: u64,
}

impl eframe::App for Probe {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let p = self.palette.clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg))
            .show(root, |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let width = ui.available_width();
                        let column = width.min(760.0);
                        let margin = ((width - column) / 2.0).max(0.0);
                        ui.horizontal(|ui| {
                            ui.add_space(margin);
                            ui.vertical(|ui| {
                                ui.set_width(column);
                                ui.label(RichText::new("probe").font(theme::font_semibold(16.0)));
                                self.md
                                    .show_at(ui, egui::Id::unique("probe-md"), SAMPLE, &p);
                                ui.label("after markdown");
                            });
                        });
                    });
            });
        self.frames += 1;
        if self.frames == 40
            && let Ok(path) = std::env::var("PROBE_SHOT")
        {
            let ctx = root.ctx().clone();
            ctx.request_screenshot(move |image| {
                let [width, height] = image.size;
                let mut rgba = Vec::with_capacity(width * height * 4);
                for pixel in &image.pixels {
                    rgba.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
                }
                if let Some(buffer) = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
                {
                    let _ = buffer.save(&path);
                }
            });
        }
        root.ctx().request_repaint();
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "md-probe",
        options,
        Box::new(|cc| {
            theme::install_fonts(&cc.egui_ctx);
            let palette = theme::Palette::dark();
            theme::apply(&cc.egui_ctx, &palette);
            Ok(Box::new(Probe {
                md: Markdown::default(),
                palette,
                frames: 0,
            }))
        }),
    )
}
