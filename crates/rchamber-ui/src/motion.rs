use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Context, Id, Ui};

pub const INSTANT: f32 = 0.0;
pub const FAST: f32 = 0.150;
pub const HOVER: f32 = 0.150;
pub const NORMAL: f32 = 0.250;
pub const SLOW: f32 = 0.350;
pub const POPUP: f32 = 0.251;
pub const POPUP_EXIT: f32 = 0.110;
pub const LAYOUT: f32 = 0.294;
pub const COLUMN: f32 = 0.240;
pub const TAB: f32 = 0.280;
pub const TAB_SIZE: f32 = 0.260;
pub const PICKER: f32 = 0.090;
pub const MESSAGE: f32 = 0.320;
pub const REVEAL: f32 = 0.100;
pub const TOAST: f32 = 4.0;
pub const SPIN_PERIOD: f32 = 0.250;
pub const BUSY_PERIOD: f32 = 1.0;

pub const POPUP_SCALE: f32 = 0.95;
pub const POPUP_RISE: f32 = 4.0;
pub const TAB_PRESS_SCALE: f32 = 0.97;
pub const TAB_NUDGE: f32 = 4.0;

pub const EASE_STANDARD: Easing = Easing::Standard;
pub const EASE_DECELERATE: Easing = Easing::Decelerate;
pub const EASE_OUT: Easing = Easing::EaseOut;
pub const EASE_IN_OUT: Easing = Easing::EaseInOut;
pub const EASE_POPUP: Easing = Easing::PopupSpring;
pub const EASE_LAYOUT: Easing = Easing::LayoutSpring;

pub const SPINNER_BARS: usize = 8;
pub const SPINNER_TAIL: [f32; 8] = [1.0, 0.82, 0.66, 0.52, 0.4, 0.32, 0.26, 0.22];
pub const BUSY_PHASES: [[bool; 3]; 5] = [
    [true, false, false],
    [true, true, false],
    [true, true, true],
    [false, true, true],
    [false, false, true],
];

static REDUCED: AtomicBool = AtomicBool::new(false);

pub fn set_reduced_motion(enabled: bool) {
    REDUCED.store(enabled, Ordering::Relaxed);
}

pub fn reduced_motion_flag() -> bool {
    if REDUCED.load(Ordering::Relaxed) {
        return true;
    }
    static ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENV.get_or_init(|| {
        ["RCHAMBER_REDUCED_MOTION", "RCHAMBER_REDUCE_MOTION"]
            .iter()
            .filter_map(|key| std::env::var(key).ok())
            .any(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
    })
}

pub fn reduced_motion(ctx: &Context) -> bool {
    if reduced_motion_flag() {
        return true;
    }
    if ctx.global_style().animation_time <= f32::EPSILON {
        return true;
    }
    ctx.data(|data| data.get_temp::<bool>(Id::unique("settings.reduce_motion")))
        .unwrap_or(false)
}

pub fn duration(base: f32) -> f32 {
    if reduced_motion_flag() { INSTANT } else { base }
}

pub fn duration_for(ctx: &Context, base: f32) -> f32 {
    if reduced_motion(ctx) { INSTANT } else { base }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    EaseOut,
    EaseIn,
    EaseOutCubic,
    EaseOutQuint,
    EaseInOut,
    Standard,
    Decelerate,
    Spring,
    PopupSpring,
    LayoutSpring,
}

const POPUP_STOPS: [f32; 33] = [
    0.0, 0.0186, 0.0657, 0.1307, 0.2056, 0.2848, 0.364, 0.4406, 0.5127, 0.5791, 0.6394, 0.6933,
    0.741, 0.7828, 0.819, 0.8501, 0.8767, 0.8992, 0.9182, 0.9341, 0.9473, 0.9581, 0.9671, 0.9743,
    0.9802, 0.9849, 0.9887, 0.9917, 0.994, 0.9958, 0.9972, 0.9982, 1.0,
];

const LAYOUT_STOPS: [f32; 33] = [
    0.0, 0.0344, 0.1145, 0.2151, 0.3209, 0.423, 0.5166, 0.5995, 0.6712, 0.7322, 0.7832, 0.8255,
    0.8602, 0.8885, 0.9114, 0.9298, 0.9445, 0.9563, 0.9657, 0.9731, 0.9789, 0.9835, 0.9872, 0.99,
    0.9922, 0.994, 0.9953, 0.9964, 0.9972, 0.9978, 0.9983, 0.9987, 1.0,
];

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseOut => cubic_bezier(0.0, 0.0, 0.2, 1.0, t),
            Easing::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, t),
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::EaseOutQuint => 1.0 - (1.0 - t).powi(5),
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::Standard => cubic_bezier(0.4, 0.0, 0.2, 1.0, t),
            Easing::Decelerate => cubic_bezier(0.22, 1.0, 0.36, 1.0, t),
            Easing::Spring => ease_out_back(t, 0.14),
            Easing::PopupSpring => linear_stops(t, &POPUP_STOPS),
            Easing::LayoutSpring => linear_stops(t, &LAYOUT_STOPS),
        }
    }
}

pub fn linear_stops(t: f32, stops: &[f32]) -> f32 {
    if stops.len() < 2 {
        return t.clamp(0.0, 1.0);
    }
    let t = t.clamp(0.0, 1.0);
    let scaled = t * (stops.len() - 1) as f32;
    let index = scaled.floor() as usize;
    let next = (index + 1).min(stops.len() - 1);
    let local = scaled - index as f32;
    stops[index] + (stops[next] - stops[index]) * local
}

pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let mut u = t;
    for _ in 0..8 {
        let x = bezier_axis(x1, x2, u) - t;
        if x.abs() < 1e-5 {
            break;
        }
        let derivative = bezier_derivative(x1, x2, u);
        if derivative.abs() < 1e-5 {
            break;
        }
        u = (u - x / derivative).clamp(0.0, 1.0);
    }
    bezier_axis(y1, y2, u)
}

fn bezier_axis(a: f32, b: f32, t: f32) -> f32 {
    let one = 1.0 - t;
    3.0 * one * one * t * a + 3.0 * one * t * t * b + t * t * t
}

fn bezier_derivative(a: f32, b: f32, t: f32) -> f32 {
    let one = 1.0 - t;
    3.0 * one * one * a + 6.0 * one * t * (b - a) + 3.0 * t * t * (1.0 - b)
}

pub fn ease_out_back(t: f32, overshoot: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = overshoot;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

pub fn ease_out_cubic(t: f32) -> f32 {
    Easing::EaseOutCubic.apply(t)
}

pub fn ease_out_quint(t: f32) -> f32 {
    Easing::EaseOutQuint.apply(t)
}

pub fn ease_in_out_cubic(t: f32) -> f32 {
    Easing::EaseInOut.apply(t)
}

pub fn ease_standard(t: f32) -> f32 {
    Easing::Standard.apply(t)
}

pub fn ease_decelerate(t: f32) -> f32 {
    Easing::Decelerate.apply(t)
}

pub fn ease_popup(t: f32) -> f32 {
    Easing::PopupSpring.apply(t)
}

pub fn spring(t: f32) -> f32 {
    Easing::Spring.apply(t)
}

#[derive(Clone, Copy, Debug)]
struct State {
    from: f32,
    target: f32,
    start: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Motion {
    id: Id,
    duration: f32,
    easing: Easing,
}

impl Motion {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: Id::unique(id),
            duration: FAST,
            easing: Easing::EaseOutCubic,
        }
    }

    pub fn from_id(id: Id) -> Self {
        Self {
            id,
            duration: FAST,
            easing: Easing::EaseOutCubic,
        }
    }

    pub fn with_duration(mut self, duration: f32) -> Self {
        self.duration = duration;
        self
    }

    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    pub fn id(&self) -> Id {
        self.id
    }

    pub fn value(&self, ctx: &Context, target: f32) -> f32 {
        self.value_at(ctx, target, self.duration, self.easing)
    }

    pub fn value_at(&self, ctx: &Context, target: f32, duration: f32, easing: Easing) -> f32 {
        if reduced_motion(ctx) || duration <= f32::EPSILON {
            return target;
        }
        let now = ctx.input(|input| input.time);
        let mut state = ctx
            .data(|data| data.get_temp::<State>(self.id))
            .unwrap_or(State {
                from: target,
                target,
                start: now,
            });

        if (state.target - target).abs() > f32::EPSILON {
            let current = eval(&state, now, duration, easing);
            state = State {
                from: current,
                target,
                start: now,
            };
            ctx.data_mut(|data| data.insert_temp(self.id, state));
        }

        let remaining = duration as f64 - (now - state.start);
        if remaining > 0.0 {
            ctx.request_repaint();
        }
        eval(&state, now, duration, easing)
    }

    pub fn bool(&self, ctx: &Context, target: bool) -> f32 {
        self.value(ctx, if target { 1.0 } else { 0.0 })
    }
}

fn eval(state: &State, now: f64, duration: f32, easing: Easing) -> f32 {
    if duration <= f32::EPSILON {
        return state.target;
    }
    let t = (((now - state.start) as f32) / duration).clamp(0.0, 1.0);
    state.from + (state.target - state.from) * easing.apply(t)
}

#[derive(Clone, Copy, Debug)]
pub struct Animated {
    motion: Motion,
}

impl Animated {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            motion: Motion::new(id),
        }
    }

    pub fn from_id(id: Id) -> Self {
        Self {
            motion: Motion::from_id(id),
        }
    }

    pub fn with_duration(mut self, duration: f32) -> Self {
        self.motion = self.motion.with_duration(duration);
        self
    }

    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.motion = self.motion.with_easing(easing);
        self
    }

    pub fn value(&self, ctx: &Context, target: f32) -> f32 {
        self.motion.value(ctx, target)
    }

    pub fn bool(&self, ctx: &Context, target: bool) -> f32 {
        self.motion.bool(ctx, target)
    }

    pub fn duration(&self) -> f32 {
        self.motion.duration
    }

    pub fn eased(&self, ctx: &Context, target: f32, easing: Easing) -> f32 {
        self.motion
            .value_at(ctx, target, self.motion.duration, easing)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopupT {
    pub opacity: f32,
    pub scale: f32,
    pub offset: egui::Vec2,
}

pub fn presence(ctx: &Context, id: Id, open: bool, enter: f32, exit: f32) -> Option<f32> {
    let now = ctx.input(|input| input.time);
    if ctx.data(|data| data.get_temp::<State>(id)).is_none() {
        ctx.data_mut(|data| {
            data.insert_temp(
                id,
                State {
                    from: 0.0,
                    target: 0.0,
                    start: now,
                },
            )
        });
    }
    let (duration, easing) = if open {
        (enter, Easing::PopupSpring)
    } else {
        (exit, Easing::EaseIn)
    };
    let value = Motion::from_id(id)
        .with_duration(duration)
        .with_easing(easing)
        .bool(ctx, open);
    if open || value > 0.001 {
        Some(value.clamp(0.0, 1.0))
    } else {
        None
    }
}

pub fn popup(ctx: &Context, id: Id, open: bool) -> Option<PopupT> {
    let t = presence(ctx, id.with("presence"), open, POPUP, POPUP_EXIT)?;
    if reduced_motion(ctx) {
        return Some(PopupT {
            opacity: t,
            scale: 1.0,
            offset: egui::Vec2::ZERO,
        });
    }
    Some(PopupT {
        opacity: t,
        scale: POPUP_SCALE + (1.0 - POPUP_SCALE) * t,
        offset: egui::vec2(0.0, (1.0 - t) * POPUP_RISE),
    })
}

pub fn popup_rect(rect: egui::Rect, t: &PopupT) -> egui::Rect {
    let inset = egui::vec2(rect.width(), rect.height()) * ((1.0 - t.scale) * 0.5);
    rect.translate(t.offset).shrink2(inset)
}

pub fn animate_bool(ctx: &Context, id: Id, value: bool) -> f32 {
    Animated::from_id(id).bool(ctx, value)
}

pub fn animate_bool_with_time(ctx: &Context, id: Id, value: bool, time: f32) -> f32 {
    Animated::from_id(id).with_duration(time).bool(ctx, value)
}

pub fn animate_value(ctx: &Context, id: Id, target: f32, time: f32) -> f32 {
    Motion::from_id(id)
        .with_duration(time)
        .with_easing(Easing::EaseOutCubic)
        .value(ctx, target)
}

pub fn hover_fade(ui: &Ui, id: Id, hovered: bool) -> f32 {
    Animated::from_id(id)
        .with_duration(HOVER)
        .with_easing(Easing::EaseOut)
        .bool(ui.ctx(), hovered)
}

pub fn hover_fade_response(ui: &Ui, response: &egui::Response) -> f32 {
    hover_fade(ui, response.id.with("hover"), response.hovered())
}

pub fn second(ctx: &Context) -> i64 {
    ctx.input(|input| input.time) as i64
}

pub fn beat(ctx: &Context) -> i64 {
    (ctx.input(|input| input.time) / SPIN_PERIOD as f64) as i64
}

pub fn spinner_lit(ctx: &Context) -> usize {
    if reduced_motion(ctx) {
        return 0;
    }
    beat(ctx).rem_euclid(SPINNER_BARS as i64) as usize
}

pub fn busy_phase(ctx: &Context) -> usize {
    second(ctx).rem_euclid(BUSY_PHASES.len() as i64) as usize
}

pub fn busy_dots(ctx: &Context) -> [bool; 3] {
    BUSY_PHASES[busy_phase(ctx)]
}

pub fn waiting_dim(ctx: &Context) -> bool {
    if reduced_motion(ctx) {
        return false;
    }
    second(ctx).rem_euclid(2) == 1
}

pub fn waiting_alpha(ctx: &Context, _id: Id) -> f32 {
    if waiting_dim(ctx) { 0.4 } else { 1.0 }
}

pub fn spin_angle(ctx: &Context, period: f32) -> f32 {
    if reduced_motion(ctx) {
        return 0.0;
    }
    let time = ctx.input(|input| input.time) as f32;
    let period = period.max(0.05);
    (time / period * std::f32::consts::TAU) % std::f32::consts::TAU
}

pub fn spin_angle_for(ctx: &Context, id: Id, period: f32) -> f32 {
    let time = ctx.input(|input| input.time) as f32;
    let offset = (id.value() % 1_000) as f32 / 1_000.0;
    let period = period.max(0.05);
    if reduced_motion(ctx) {
        return 0.0;
    }
    ((time / period + offset) * std::f32::consts::TAU) % std::f32::consts::TAU
}

pub fn pulse(ctx: &Context, id: Id, period: f32) -> f32 {
    if reduced_motion(ctx) {
        return 1.0;
    }
    let time = ctx.input(|input| input.time) as f32;
    let offset = (id.value() % 1_000) as f32 / 1_000.0;
    let period = period.max(0.05);
    let phase = (time / period + offset) * std::f32::consts::TAU;
    0.5 + 0.5 * phase.sin()
}

pub fn pulse_alpha(ctx: &Context, id: Id, period: f32, base: u8, amount: f32) -> u8 {
    let value = pulse(ctx, id, period);
    let alpha = base as f32 + amount * (value - 0.5) * 2.0;
    alpha.clamp(0.0, 255.0) as u8
}

fn with_alpha(color: egui::Color32, alpha: f32) -> egui::Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    egui::Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * alpha.clamp(0.0, 1.0)) as u8)
}

fn capsule(center: egui::Pos2, half_len: f32, radius: f32, angle: f32) -> Vec<egui::Pos2> {
    let direction = egui::vec2(angle.sin(), -angle.cos());
    let normal = egui::vec2(direction.y, -direction.x);
    let mut points = Vec::with_capacity(16);
    for i in 0..=6 {
        let theta = std::f32::consts::PI * i as f32 / 6.0;
        let offset = normal * (theta.cos() * radius) + direction * (theta.sin() * radius);
        points.push(center + direction * half_len + offset);
    }
    for i in 0..=6 {
        let theta = std::f32::consts::PI * i as f32 / 6.0;
        let offset = normal * (-theta.cos() * radius) + direction * (-theta.sin() * radius);
        points.push(center - direction * half_len + offset);
    }
    points
}

pub fn paint_spinner(ui: &Ui, center: egui::Pos2, size: f32, color: egui::Color32) {
    let scale = (size / 24.0).max(0.05);
    let lit = spinner_lit(ui.ctx());
    let half_len = 2.5 * scale;
    let radius = 1.1 * scale;
    let distance = 7.5 * scale;
    for bar in 0..SPINNER_BARS {
        let opacity = SPINNER_TAIL[(lit + SPINNER_BARS - bar) % SPINNER_BARS];
        let angle = bar as f32 * std::f32::consts::TAU / SPINNER_BARS as f32;
        let bar_center = center + egui::vec2(angle.sin(), -angle.cos()) * distance;
        let points = capsule(bar_center, half_len, radius, angle);
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            with_alpha(color, opacity),
            egui::Stroke::NONE,
        ));
    }
    let phase = (ui.ctx().input(|input| input.time) as f32) % SPIN_PERIOD;
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f32(
            (SPIN_PERIOD - phase).max(0.001),
        ));
}

pub fn paint_pulse_dot(ui: &Ui, center: egui::Pos2, radius: f32, color: egui::Color32, _id: Id) {
    let alpha = waiting_alpha(ui.ctx(), Id::unique("waiting"));
    ui.painter()
        .circle_filled(center, radius, with_alpha(color, alpha));
}

pub fn paint_busy_dots(
    ui: &Ui,
    font: &egui::FontId,
    color: egui::Color32,
    origin: egui::Pos2,
) -> f32 {
    let phase = busy_dots(ui.ctx());
    let mut x = origin.x;
    let mut width = 0.0;
    for shown in phase {
        let galley = ui.painter().layout_no_wrap(
            ".".to_string(),
            font.clone(),
            if shown {
                color
            } else {
                egui::Color32::TRANSPARENT
            },
        );
        ui.painter()
            .galley(egui::pos2(x, origin.y), galley.clone(), color);
        x += galley.size().x;
        width += galley.size().x;
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(1000));
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_are_clamped() {
        for easing in [
            Easing::Linear,
            Easing::EaseOut,
            Easing::EaseIn,
            Easing::EaseOutCubic,
            Easing::EaseOutQuint,
            Easing::EaseInOut,
            Easing::Standard,
            Easing::Decelerate,
            Easing::Spring,
            Easing::PopupSpring,
            Easing::LayoutSpring,
        ] {
            assert!((easing.apply(0.0)).abs() < 0.001);
            assert!((easing.apply(1.0) - 1.0).abs() < 0.001);
        }
    }

    #[test]
    fn bezier_matches_endpoints() {
        assert!((cubic_bezier(0.4, 0.0, 0.2, 1.0, 0.0)).abs() < 0.001);
        assert!((cubic_bezier(0.4, 0.0, 0.2, 1.0, 1.0) - 1.0).abs() < 0.001);
        let mid = cubic_bezier(0.4, 0.0, 0.2, 1.0, 0.5);
        assert!(mid > 0.5 && mid < 1.0);
    }

    #[test]
    fn popup_spring_traces_the_reference_stops() {
        assert!(Easing::PopupSpring.apply(0.1) < 0.2);
        assert!((Easing::PopupSpring.apply(0.5) - 0.8767).abs() < 0.02);
        assert!((Easing::PopupSpring.apply(0.25) - 0.5127).abs() < 0.02);
        assert!((Easing::LayoutSpring.apply(0.5) - 0.9445).abs() < 0.02);
    }

    #[test]
    fn reference_durations_are_stable() {
        assert_eq!(FAST, 0.150);
        assert_eq!(POPUP, 0.251);
        assert_eq!(POPUP_EXIT, 0.110);
        assert_eq!(LAYOUT, 0.294);
        assert_eq!(COLUMN, 0.240);
        assert_eq!(TAB, 0.280);
        assert_eq!(TAB_SIZE, 0.260);
        assert_eq!(SPIN_PERIOD, 0.250);
        assert_eq!(BUSY_PERIOD, 1.0);
    }
}
