use eframe::egui::epaint::{PathShape, PathStroke};
use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Shape, Vec2};

pub fn paint(painter: &eframe::egui::Painter, rect: Rect, color: Color32) {
    let size = rect.width().min(rect.height());
    if size <= 0.0 || color.a() == 0 {
        return;
    }
    let origin = rect.center() - Vec2::splat(size * 0.5);
    let stroke = (size * 0.093).max(1.0);
    let radius = stroke * 0.5;
    let left = origin.x + size * 0.20;
    let right = origin.x + size * 0.80;
    let top = origin.y + size * 0.16;
    let bottom = origin.y + size * 0.84;
    let arm = size * 0.13;

    for (x, direction) in [(left, 1.0), (right, -1.0)] {
        let points = vec![
            Pos2::new(x + direction * arm, top),
            Pos2::new(x, top),
            Pos2::new(x, bottom),
            Pos2::new(x + direction * arm, bottom),
        ];
        painter.add(Shape::Path(PathShape {
            points: points.clone(),
            closed: false,
            fill: Color32::TRANSPARENT,
            stroke: PathStroke::new(stroke, color),
        }));
        for point in points {
            painter.circle_filled(point, radius, color);
        }
    }

    let caret_width = (size * 0.075).max(1.0);
    let caret = Rect::from_center_size(
        Pos2::new(origin.x + size * 0.5, origin.y + size * 0.5),
        Vec2::new(caret_width, size * 0.30),
    );
    painter.rect_filled(
        caret,
        CornerRadius::same((caret_width * 0.5).round() as u8),
        color,
    );
}
