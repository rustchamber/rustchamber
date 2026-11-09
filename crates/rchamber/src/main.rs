use rchamber_ui::RustChamber;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|arg| arg == "--version" || arg == "-V")
    {
        println!("RustChamber {}", app_version());
        return Ok(());
    }
    if args.get(1).is_some_and(|arg| arg == "ext") && args.get(2).is_some_and(|arg| arg == "check")
    {
        let Some(path) = args.get(3) else {
            eprintln!("usage: rchamber ext check <folder>");
            std::process::exit(2);
        };
        match rchamber_ext::check_package(std::path::Path::new(path)) {
            Ok(report) => {
                println!("{} {} ({})", report.name, report.version, report.id);
                if report.capabilities.is_empty() {
                    println!("permissions: none");
                } else {
                    println!("permissions: {}", report.capabilities.join(", "));
                }
                if report.methods.is_empty() {
                    println!("methods: none");
                } else {
                    println!("methods: {}", report.methods.join(", "));
                }
                for warning in report.warnings {
                    println!("warning: {warning}");
                }
                println!("ready");
            }
            Err(error) => {
                eprintln!("invalid extension: {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(
                "warn,rchamber=debug,rchamber_app=debug,rchamber_ui=debug,rchamber_api=debug",
            )
        }))
        .with_target(false)
        .compact()
        .init();

    let screenshot = std::env::var_os("RCHAMBER_SCREENSHOT").is_some();
    let mut size = [1320.0, 860.0];
    if let Ok(value) = std::env::var("RCHAMBER_WINDOW")
        && let Some((width, height)) = value.split_once('x')
        && let (Ok(width), Ok(height)) = (width.parse(), height.parse())
    {
        size = [width, height];
    }
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("RustChamber")
        .with_icon(rchamber_ui::app_icon::app_icon())
        .with_inner_size(size)
        .with_min_inner_size([900.0, 600.0])
        .with_app_id("dev.rustchamber.RustChamber");
    if cfg!(target_os = "macos") {
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false);
    }
    if screenshot {
        viewport = viewport
            .with_position([-12000.0, -12000.0])
            .with_active(false)
            .with_taskbar(false);
    }
    let options = eframe::NativeOptions {
        viewport,
        persist_window: !screenshot,
        ..Default::default()
    };

    eframe::run_native(
        "RustChamber",
        options,
        Box::new(|cc| Ok(Box::new(RustChamber::new(cc)))),
    )
}

fn app_version() -> &'static str {
    option_env!("RCHAMBER_VERSION")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}
