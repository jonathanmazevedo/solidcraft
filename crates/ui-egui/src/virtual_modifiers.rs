//! On-screen Shift and Ctrl toggles for touch and pen input with no keyboard.

use egui::{Align2, Event, Key, Modifiers, RawInput, Rect, vec2};

use crate::SolveApp;

fn toggled(shift: bool, ctrl: bool) -> Modifiers {
    Modifiers { shift, ctrl, command: ctrl, ..Modifiers::NONE }
}

/// Folds the latched toggles into the frame's input. egui keeps `InputState::modifiers` from the
/// last `ModifiersChanged` event and reads a click's modifiers from its own `PointerButton`, so
/// both carry the toggles. Key events are left alone so on-screen typing never becomes a shortcut.
/// `real` is the keyboard's own state, kept across frames; `was` is last frame's toggles.
pub fn apply(raw: &mut RawInput, real: &mut Modifiers, was: &mut Modifiers, shift: bool, ctrl: bool) {
    let v = toggled(shift, ctrl);
    for event in &mut raw.events {
        match event {
            Event::ModifiersChanged(m) => {
                *real = *m;
                *m = *m | v;
            }
            Event::WindowFocused(false) => *real = Modifiers::NONE,
            Event::PointerButton { modifiers, .. } | Event::MouseWheel { modifiers, .. } => *modifiers = *modifiers | v,
            _ => {}
        }
    }
    if !v.is_none() || !was.is_none() {
        raw.events.push(Event::ModifiersChanged(*real | v));
    }
    *was = v;
}

/// With the one-shot right-click armed, the next tap's primary press and release become secondary
/// (a touch press can arrive twice in a frame, from pointer and touch events). Disarms once the
/// tap is released.
pub fn right_click(raw: &mut RawInput, armed: &mut bool) {
    if !*armed {
        return;
    }
    let mut released = false;
    for event in &mut raw.events {
        if let Event::PointerButton { button, pressed, .. } = event
            && *button == egui::PointerButton::Primary
        {
            *button = egui::PointerButton::Secondary;
            released |= !*pressed;
        }
    }
    if released {
        *armed = false;
    }
}

/// On a touch screen a finger or pen tap wobbles and lingers past egui's mouse-sized click limits;
/// the click time is also how long a touch is held for a long press (the tool ring).
fn watch_input(app: &mut SolveApp, ctx: &egui::Context) {
    if app.touch_seen || !ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Touch { .. }))) {
        return;
    }
    app.touch_seen = true;
    ctx.options_mut(|o| {
        o.input_options.max_click_dist = 16.0;
        o.input_options.max_click_duration = 0.6;
    });
}

/// One key tap, through the synthetic queue so it arrives as a press and release next frame.
fn press(app: &mut SolveApp, key: Key) {
    for pressed in [true, false] {
        app.synthetic.push(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE });
    }
}

/// Size of the round button and of each key around it, and how far out the keys sit.
const BUTTON: f32 = 48.0;
const KEY: f32 = 46.0;
const RADIUS: f32 = 66.0;

/// A round button `size` across with `label` centred in it, filled with the selection colour
/// when `on`.
fn round_button(ui: &mut egui::Ui, label: &str, size: f32, text: f32, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
    let v = ui.visuals();
    let fill = if on {
        v.selection.bg_fill
    } else if response.hovered() {
        v.widgets.hovered.bg_fill
    } else {
        v.window_fill
    };
    let ink = if on { v.strong_text_color() } else { v.text_color() };
    let painter = ui.painter();
    painter.circle(rect.center(), size / 2.0, fill, v.window_stroke);
    if label == "≡" {
        // Drawn, not typed: the glyph sits off centre in the font.
        let c = rect.center();
        let (w, gap) = (size * 0.36, size * 0.14);
        for k in [-1.0, 0.0, 1.0] {
            let y = c.y + k * gap;
            painter.line_segment([egui::pos2(c.x - w / 2.0, y), egui::pos2(c.x + w / 2.0, y)], egui::Stroke::new(2.0, ink));
        }
    } else {
        painter.text(rect.center(), Align2::CENTER_CENTER, label, egui::FontId::proportional(text), ink);
    }
    response
}

/// What the round button shows: the latched keys, or a keyboard sign when none is.
fn button_label(app: &SolveApp) -> String {
    let on: Vec<&str> = [(app.virtual_shift, "⇧"), (app.virtual_ctrl, "Ctrl")]
        .into_iter()
        .filter_map(|(on, label)| on.then_some(label))
        .collect();
    if on.is_empty() { "≡".to_owned() } else { on.join(" ") }
}

/// A round button the user drags anywhere; a tap opens Shift, Ctrl, Esc and Del in a ring around
/// it. Shift and Ctrl stay latched until tapped again.
pub(crate) fn show(app: &mut SolveApp, ui: &egui::Ui, rect: Rect) {
    let ctx = ui.ctx().clone();
    watch_input(app, &ctx);
    let reach = RADIUS + KEY / 2.0 + 4.0;
    let latched = app.virtual_shift || app.virtual_ctrl;
    let label = button_label(app);
    let size = if label.chars().count() > 2 { 14.0 } else { 20.0 };
    let main = egui::Area::new(egui::Id::unique("sc_virtual_modifiers"))
        .order(egui::Order::Foreground)
        .movable(true)
        .constrain_to(ctx.content_rect().shrink(reach - BUTTON / 2.0))
        .pivot(Align2::CENTER_CENTER)
        .default_pos(rect.right_bottom() - vec2(reach + 8.0, reach + 64.0))
        .show(&ctx, |ui| {
            round_button(ui, &label, BUTTON, size, latched || app.virtual_ring).clicked()
        });
    if main.inner {
        app.virtual_ring = !app.virtual_ring;
    }
    if !app.virtual_ring {
        return;
    }
    let centre = main.response.rect.center();
    let mut keep = vec![main.response.rect];
    let keys: [(&str, Option<Key>); 4] = [("Shift", None), ("Ctrl", None), ("Esc", Some(Key::Escape)), ("Del", Some(Key::Delete))];
    let n = keys.len() as f32;
    for (k, (label, key)) in keys.into_iter().enumerate() {
        let angle = k as f32 / n * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let at = centre + vec2(angle.cos(), angle.sin()) * RADIUS;
        let on = match k {
            0 => app.virtual_shift,
            1 => app.virtual_ctrl,
            _ => false,
        };
        let shown = egui::Area::new(egui::Id::unique(("sc_virtual_modifiers_key", k)))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(at)
            .show(&ctx, |ui| {
                round_button(ui, label, KEY, 14.0, on).clicked()
            });
        keep.push(shown.response.rect);
        if !shown.inner {
            continue;
        }
        match (k, key) {
            (0, _) => app.virtual_shift = !app.virtual_shift,
            (1, _) => app.virtual_ctrl = !app.virtual_ctrl,
            (_, Some(key)) => {
                press(app, key);
                ctx.request_repaint();
            }
            _ => {}
        }
        app.virtual_ring = false;
    }
    // A tap anywhere else closes the ring.
    let elsewhere = ctx.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| !keep.iter().any(|r| r.contains(p))));
    if elsewhere {
        app.virtual_ring = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, PointerButton, pos2};

    fn press() -> Event {
        Event::PointerButton { pos: pos2(1.0, 1.0), button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }
    }

    fn key() -> Event {
        Event::Key { key: Key::S, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    fn run(events: Vec<Event>, shift: bool, ctrl: bool) -> RawInput {
        let mut raw = RawInput { events, ..Default::default() };
        let (mut real, mut was) = (Modifiers::NONE, Modifiers::NONE);
        apply(&mut raw, &mut real, &mut was, shift, ctrl);
        raw
    }

    #[test]
    fn toggles_reach_clicks_and_held_state_but_not_keys() {
        let raw = run(vec![press(), key()], true, true);
        let Event::PointerButton { modifiers, .. } = raw.events[0] else { panic!() };
        assert!(modifiers.shift && modifiers.ctrl && modifiers.command);
        let Event::Key { modifiers, .. } = raw.events[1] else { panic!() };
        assert!(modifiers.is_none());
        let Some(Event::ModifiersChanged(held)) = raw.events.last() else { panic!() };
        assert!(held.shift && held.command);
    }

    #[test]
    fn armed_right_click_turns_one_tap_secondary() {
        let release = Event::PointerButton { pos: pos2(1.0, 1.0), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE };
        let mut raw = RawInput { events: vec![press(), release.clone()], ..Default::default() };
        let mut armed = true;
        right_click(&mut raw, &mut armed);
        assert!(!armed);
        assert!(raw.events.iter().all(|e| matches!(e, Event::PointerButton { button: PointerButton::Secondary, .. })));
        let mut raw = RawInput { events: vec![press(), release], ..Default::default() };
        right_click(&mut raw, &mut armed);
        assert!(matches!(raw.events[0], Event::PointerButton { button: PointerButton::Primary, .. }));
    }

    #[test]
    fn untouched_when_off() {
        let raw = run(vec![press(), key()], false, false);
        assert_eq!(raw.events.len(), 2);
        let Event::PointerButton { modifiers, .. } = raw.events[0] else { panic!() };
        assert!(modifiers.is_none());
    }

    #[test]
    fn turning_off_restores_the_keyboard_state() {
        let mut raw = RawInput { events: vec![Event::ModifiersChanged(Modifiers::ALT)], ..Default::default() };
        let (mut real, mut was) = (Modifiers::NONE, Modifiers::NONE);
        apply(&mut raw, &mut real, &mut was, true, false);
        let mut raw = RawInput::default();
        apply(&mut raw, &mut real, &mut was, false, false);
        assert_eq!(raw.events, vec![Event::ModifiersChanged(Modifiers::ALT)]);
    }
}
