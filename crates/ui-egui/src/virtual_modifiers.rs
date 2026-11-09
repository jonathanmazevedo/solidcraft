//! On-screen Shift and Ctrl toggles for touch and pen input with no keyboard.

use egui::{Align2, Event, Key, Modifiers, RawInput, Rect, RichText, vec2};

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

/// On a touch screen a pen tap wobbles and lingers past egui's mouse-sized click limits, so the
/// S Pen button's right-click never counted as a click. Also keeps a short log of button events
/// shown above the strip, to see what the pen sends.
fn watch_input(app: &mut SolveApp, ctx: &egui::Context) {
    let (touch, lines, secondary_click) = ctx.input(|i| {
        let touch = i.events.iter().any(|e| matches!(e, Event::Touch { .. }));
        let lines: Vec<String> = i
            .events
            .iter()
            .filter_map(|e| match e {
                Event::PointerButton { button, pressed, pos, .. } => {
                    Some(format!("{button:?} {} @{:.0},{:.0} t={:.2}", if *pressed { "down" } else { "up" }, pos.x, pos.y, i.time))
                }
                _ => None,
            })
            .collect();
        (touch, lines, i.pointer.secondary_clicked())
    });
    if touch && !app.touch_seen {
        app.touch_seen = true;
        ctx.options_mut(|o| {
            o.input_options.max_click_dist = 16.0;
            o.input_options.max_click_duration = 1.5;
        });
    }
    app.pen_log.extend(lines);
    if secondary_click {
        app.pen_log.push("=> secondary click".to_owned());
    }
    let excess = app.pen_log.len().saturating_sub(4);
    app.pen_log.drain(..excess);
}

/// One key tap, through the synthetic queue so it arrives as a press and release next frame.
fn press(app: &mut SolveApp, key: Key) {
    for pressed in [true, false] {
        app.synthetic.push(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE });
    }
}

/// Latched Shift/Ctrl and one-tap Esc/Del in a strip the user drags anywhere by its grip; it starts above the
/// viewport's bottom toolbar.
pub(crate) fn show(app: &mut SolveApp, ui: &egui::Ui, rect: Rect) {
    watch_input(app, ui.ctx());
    egui::Area::new(egui::Id::unique("sc_virtual_modifiers"))
        .order(egui::Order::Foreground)
        .movable(true)
        .constrain(true)
        .pivot(Align2::CENTER_BOTTOM)
        .default_pos(rect.center_bottom() - vec2(0.0, 72.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                for line in &app.pen_log {
                    ui.label(RichText::new(line).size(11.0).monospace());
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                    ui.add(egui::Label::new(RichText::new("≡").size(24.0)).selectable(false));
                    for (label, on) in [("⇧ Shift", &mut app.virtual_shift), ("Ctrl", &mut app.virtual_ctrl)] {
                        let button = egui::Button::new(RichText::new(label).size(16.0)).selected(*on).min_size(vec2(64.0, 40.0));
                        if ui.add(button).clicked() {
                            *on = !*on;
                        }
                    }
                    for (label, key) in [("Esc", Key::Escape), ("Del", Key::Delete)] {
                        if ui.add(egui::Button::new(RichText::new(label).size(16.0)).min_size(vec2(56.0, 40.0))).clicked() {
                            press(app, key);
                            ui.ctx().request_repaint();
                        }
                    }
                });
            });
        });
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
