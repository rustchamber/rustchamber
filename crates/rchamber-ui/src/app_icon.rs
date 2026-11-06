use eframe::egui;

pub fn app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../../../packaging/macos/AppIcon.png"))
        .expect("bundled app icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_icon_decodes_to_rgba() {
        let icon = app_icon();
        assert_eq!(icon.width, 1024);
        assert_eq!(icon.height, 1024);
        assert_eq!(icon.rgba.len(), 1024 * 1024 * 4);
    }
}
