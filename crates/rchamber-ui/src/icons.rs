use eframe::egui::{self, Color32, Response, Sense, Ui, Vec2};

mod data {
    include!("icons_svg.rs");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Plus,
    Search,
    Settings,
    Folder,
    ChevronDown,
    ChevronRight,
    ChevronLeft,
    ArrowUp,
    ArrowDown,
    Sidebar,
    Send,
    Stop,
    Branch,
    Terminal,
    File,
    Check,
    Close,
    Warning,
    Shield,
    Spark,
    Command,
    Sun,
    Moon,
    Refresh,
    More,
    PanelRight,
    Diff,
    Clock,
    Bot,
    Copy,
    ArrowUpRight,
    Trash,
    Pencil,
    Archive,
    Layers,
    Paperclip,
    Menu,
    Info,
    Circle,
}

impl Icon {
    fn uri(self) -> &'static str {
        match self {
            Icon::Plus => "bytes://rust-ui/plus.svg",
            Icon::Search => "bytes://rust-ui/search.svg",
            Icon::Settings => "bytes://rust-ui/settings.svg",
            Icon::Folder => "bytes://rust-ui/folder.svg",
            Icon::ChevronDown => "bytes://rust-ui/chevron-down.svg",
            Icon::ChevronRight => "bytes://rust-ui/chevron-right.svg",
            Icon::ChevronLeft => "bytes://rust-ui/chevron-left.svg",
            Icon::ArrowUp => "bytes://rust-ui/arrow-up.svg",
            Icon::ArrowDown => "bytes://rust-ui/arrow-down.svg",
            Icon::Sidebar => "bytes://rust-ui/panel-left.svg",
            Icon::Send => "bytes://rust-ui/send.svg",
            Icon::Stop => "bytes://rust-ui/square.svg",
            Icon::Branch => "bytes://rust-ui/git-branch.svg",
            Icon::Terminal => "bytes://rust-ui/terminal.svg",
            Icon::File => "bytes://rust-ui/file.svg",
            Icon::Check => "bytes://rust-ui/check.svg",
            Icon::Close => "bytes://rust-ui/x.svg",
            Icon::Warning => "bytes://rust-ui/triangle-alert.svg",
            Icon::Shield => "bytes://rust-ui/shield.svg",
            Icon::Spark => "bytes://rust-ui/sparkles.svg",
            Icon::Command => "bytes://rust-ui/command.svg",
            Icon::Sun => "bytes://rust-ui/sun.svg",
            Icon::Moon => "bytes://rust-ui/moon.svg",
            Icon::Refresh => "bytes://rust-ui/refresh-cw.svg",
            Icon::More => "bytes://rust-ui/ellipsis.svg",
            Icon::PanelRight => "bytes://rust-ui/panel-right.svg",
            Icon::Diff => "bytes://rust-ui/git-compare.svg",
            Icon::Clock => "bytes://rust-ui/clock.svg",
            Icon::Bot => "bytes://rust-ui/bot.svg",
            Icon::Copy => "bytes://rust-ui/copy.svg",
            Icon::ArrowUpRight => "bytes://rust-ui/arrow-up-right.svg",
            Icon::Trash => "bytes://rust-ui/trash-2.svg",
            Icon::Pencil => "bytes://rust-ui/pencil.svg",
            Icon::Archive => "bytes://rust-ui/archive.svg",
            Icon::Layers => "bytes://rust-ui/layers.svg",
            Icon::Paperclip => "bytes://rust-ui/paperclip.svg",
            Icon::Menu => "bytes://rust-ui/menu.svg",
            Icon::Info => "bytes://rust-ui/info.svg",
            Icon::Circle => "bytes://rust-ui/circle.svg",
        }
    }

    fn svg(self) -> &'static str {
        match self {
            Icon::Plus => data::PLUS,
            Icon::Search => data::SEARCH,
            Icon::Settings => data::SETTINGS,
            Icon::Folder => data::FOLDER,
            Icon::ChevronDown => data::CHEVRONDOWN,
            Icon::ChevronRight => data::CHEVRONRIGHT,
            Icon::ChevronLeft => data::CHEVRONLEFT,
            Icon::ArrowUp => data::ARROWUP,
            Icon::ArrowDown => data::ARROWDOWN,
            Icon::Sidebar => data::PANELLEFT,
            Icon::Send => data::SEND,
            Icon::Stop => data::SQUARE,
            Icon::Branch => data::GITBRANCH,
            Icon::Terminal => data::TERMINAL,
            Icon::File => data::FILE,
            Icon::Check => data::CHECK,
            Icon::Close => data::X,
            Icon::Warning => data::TRIANGLEALERT,
            Icon::Shield => data::SHIELD,
            Icon::Spark => data::SPARKLES,
            Icon::Command => data::COMMAND,
            Icon::Sun => data::SUN,
            Icon::Moon => data::MOON,
            Icon::Refresh => data::REFRESHCW,
            Icon::More => data::ELLIPSIS,
            Icon::PanelRight => data::PANELRIGHT,
            Icon::Diff => data::GITCOMPARE,
            Icon::Clock => data::CLOCK,
            Icon::Bot => data::BOT,
            Icon::Copy => data::COPY,
            Icon::ArrowUpRight => data::ARROWUPRIGHT,
            Icon::Trash => data::TRASH2,
            Icon::Pencil => data::PENCIL,
            Icon::Archive => data::ARCHIVE,
            Icon::Layers => data::LAYERS,
            Icon::Paperclip => data::PAPERCLIP,
            Icon::Menu => data::MENU,
            Icon::Info => data::INFO,
            Icon::Circle => data::CIRCLE,
        }
    }
}

pub fn icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint(ui, icon, rect, color);
    }
    response
}

pub fn paint(ui: &Ui, icon: Icon, rect: egui::Rect, color: Color32) {
    egui::Image::from_bytes(icon.uri(), icon.svg().as_bytes())
        .tint(color)
        .paint_at(ui, rect);
}
