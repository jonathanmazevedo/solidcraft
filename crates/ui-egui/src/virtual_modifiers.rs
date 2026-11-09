//! On-screen Shift and Ctrl toggles for touch and pen input with no keyboard.

use egui::{Align2, Event, Modifiers, RawInput, Rect, RichText, vec2};

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

/// Two latched buttons on the viewport's left edge.
pub(crate) fn show(app: &mut SolveApp, ui: &egui::Ui, rect: Rect) {
    egui::Area::new(egui::Id::unique("sc_virtual_modifiers"))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_CENTER)
        .fixed_pos(rect.left_center() + vec2(12.0, 0.0))
        .show(ui.ctx(), |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            for (label, on) in [("⇧ Shift", &mut app.virtual_shift), ("Ctrl", &mut app.virtual_ctrl)] {
                let button = egui::Button::new(RichText::new(label).size(18.0)).selected(*on).min_size(vec2(72.0, 48.0));
                if ui.add(button).clicked() {
                    *on = !*on;
                }
            }
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
