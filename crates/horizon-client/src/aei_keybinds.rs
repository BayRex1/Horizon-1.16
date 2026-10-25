//! AEI's contextual shortcuts. They do not remap vanilla gameplay inputs.
use crate::{keybinds, text::Text};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Toggle,
    Recipes,
    Uses,
    Back,
    PreviousCategory,
    NextCategory,
    PreviousPage,
    NextPage,
    FocusSearch,
    PreviousAlternative,
    NextAlternative,
    PauseCycle,
}
/// (action, persistence name, English label, default chord).
pub const MAPPINGS: &[(Action, &str, &str, &str)] = &[
    (
        Action::Toggle,
        "horizon.aei.toggle",
        "Show / hide AEI",
        "ctrl+key.keyboard.r",
    ),
    (
        Action::Recipes,
        "horizon.aei.recipes",
        "Show recipes",
        "key.keyboard.r",
    ),
    (
        Action::Uses,
        "horizon.aei.uses",
        "Show uses",
        "key.keyboard.u",
    ),
    (
        Action::Back,
        "horizon.aei.back",
        "Previous recipe",
        "key.keyboard.backspace",
    ),
    (
        Action::PreviousCategory,
        "horizon.aei.previousCategory",
        "Previous category",
        "shift+key.keyboard.page.up",
    ),
    (
        Action::NextCategory,
        "horizon.aei.nextCategory",
        "Next category",
        "shift+key.keyboard.page.down",
    ),
    (
        Action::PreviousPage,
        "horizon.aei.previousPage",
        "Previous recipe page",
        "key.keyboard.page.up",
    ),
    (
        Action::NextPage,
        "horizon.aei.nextPage",
        "Next recipe page",
        "key.keyboard.page.down",
    ),
    (
        Action::FocusSearch,
        "horizon.aei.focusSearch",
        "Focus search",
        "ctrl+key.keyboard.f",
    ),
    (
        Action::PreviousAlternative,
        "horizon.aei.previousAlternative",
        "Previous ingredient",
        "key.keyboard.left",
    ),
    (
        Action::NextAlternative,
        "horizon.aei.nextAlternative",
        "Next ingredient",
        "key.keyboard.right",
    ),
    (
        Action::PauseCycle,
        "horizon.aei.pauseCycle",
        "Pause item cycle (hold)",
        "key.keyboard.left.shift",
    ),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}
impl Modifiers {
    fn contained_in(self, actual: Self) -> bool {
        (!self.control || actual.control)
            && (!self.shift || actual.shift)
            && (!self.alt || actual.alt)
            && (!self.super_key || actual.super_key)
    }
    pub fn chord(self, input: &str) -> String {
        let mut out = String::new();
        for (held, name) in [
            (self.control, "ctrl+"),
            (self.shift, "shift+"),
            (self.alt, "alt+"),
            (self.super_key, "super+"),
        ] {
            if held {
                out.push_str(name);
            }
        }
        out.push_str(input);
        out
    }
}
fn parse(value: &str) -> Option<(Modifiers, &str)> {
    let mut modifiers = Modifiers::default();
    let mut parts = value.split('+').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return (keybinds::key_code(part).is_some()
                || keybinds::mouse_button(part).is_some()
                || part
                    .strip_prefix("key.mouse.")
                    .is_some_and(|n| n.parse::<u16>().is_ok_and(|n| n > 0))
                || part == "key.keyboard.unknown")
                .then_some((modifiers, part));
        }
        let flag = match part {
            "ctrl" => &mut modifiers.control,
            "shift" => &mut modifiers.shift,
            "alt" => &mut modifiers.alt,
            "super" => &mut modifiers.super_key,
            _ => return None,
        };
        if *flag {
            return None;
        }
        *flag = true;
    }
    None
}
pub fn definition(
    name: &str,
) -> Option<&'static (Action, &'static str, &'static str, &'static str)> {
    MAPPINGS.iter().find(|m| m.1 == name)
}
pub fn display(value: &str) -> Text {
    let Some((modifiers, input)) = parse(value) else {
        return Text::literal(value);
    };
    let mut text = String::new();
    for (held, label) in [
        (modifiers.control, "Ctrl + "),
        (modifiers.shift, "Shift + "),
        (modifiers.alt, "Alt + "),
        (modifiers.super_key, "Super + "),
    ] {
        if held {
            text.push_str(label);
        }
    }
    Text::literal(text).append(keybinds::display_name(input))
}
#[derive(Clone, Debug, PartialEq)]
pub struct Bindings(BTreeMap<&'static str, String>);
impl Default for Bindings {
    fn default() -> Self {
        Self(MAPPINGS.iter().map(|m| (m.1, m.3.to_owned())).collect())
    }
}
impl Bindings {
    pub fn get(&self, name: &str) -> &str {
        self.0
            .get(name)
            .map_or("key.keyboard.unknown", String::as_str)
    }
    pub fn set(&mut self, name: &str, value: &str) -> bool {
        let Some(definition) = definition(name) else {
            return false;
        };
        if let Some((modifiers, input)) = parse(value) {
            self.0.insert(
                definition.1,
                if input == "key.keyboard.unknown" {
                    input.to_owned()
                } else {
                    modifiers.chord(input)
                },
            );
        }
        true
    }
    pub fn action(&self, input: &str, modifiers: Modifiers) -> Option<Action> {
        MAPPINGS
            .iter()
            .find(|m| {
                parse(self.get(m.1)).is_some_and(|(required, key)| {
                    key != "key.keyboard.unknown" && key == input && required == modifiers
                })
            })
            .map(|m| m.0)
    }
    /// A held shortcut accepts extra modifiers, including its own modifier key.
    pub fn held(&self, action: Action, modifiers: Modifiers, down: impl Fn(&str) -> bool) -> bool {
        let Some(mapping) = MAPPINGS.iter().find(|m| m.0 == action) else {
            return false;
        };
        let Some((required, key)) = parse(self.get(mapping.1)) else {
            return false;
        };
        let pressed = match key {
            "key.keyboard.left.shift" | "key.keyboard.right.shift" => modifiers.shift,
            "key.keyboard.left.control" | "key.keyboard.right.control" => modifiers.control,
            "key.keyboard.left.alt" | "key.keyboard.right.alt" => modifiers.alt,
            "key.keyboard.left.win" | "key.keyboard.right.win" => modifiers.super_key,
            "key.keyboard.unknown" => false,
            _ => down(key),
        };
        pressed && required.contained_in(modifiers)
    }
    /// Manual cycling remains usable while a pause chord's modifiers are held.
    pub fn cycle_action(&self, input: &str, modifiers: Modifiers) -> Option<Action> {
        MAPPINGS
            .iter()
            .filter(|m| matches!(m.0, Action::PreviousAlternative | Action::NextAlternative))
            .find(|m| {
                parse(self.get(m.1))
                    .is_some_and(|(required, key)| key == input && required.contained_in(modifiers))
            })
            .map(|m| m.0)
    }
    pub fn serialize(&self) -> String {
        MAPPINGS
            .iter()
            .map(|m| format!("{}:{}\n", m.1, self.get(m.1)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_cycle_controls_accept_modifiers_chords_and_unbinding() {
        let mut keys = Bindings::default();
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert!(keys.held(Action::PauseCycle, shift, |_| false));
        assert!(!keys.held(Action::PauseCycle, Modifiers::default(), |_| true));
        assert_eq!(keys.action("key.keyboard.right", shift), None);
        assert_eq!(
            keys.cycle_action("key.keyboard.right", shift),
            Some(Action::NextAlternative)
        );
        keys.set("horizon.aei.pauseCycle", "ctrl+key.keyboard.space");
        assert!(!keys.held(Action::PauseCycle, shift, |_| true));
        let ctrl = Modifiers {
            control: true,
            ..shift
        };
        assert!(!keys.held(Action::PauseCycle, ctrl, |_| false));
        assert!(keys.held(Action::PauseCycle, ctrl, |key| key == "key.keyboard.space"));
        keys.set("horizon.aei.pauseCycle", "key.mouse.6");
        assert!(keys.held(Action::PauseCycle, ctrl, |key| key == "key.mouse.6"));
        keys.set("horizon.aei.pauseCycle", "key.keyboard.unknown");
        assert!(!keys.held(Action::PauseCycle, ctrl, |_| true));
    }
    #[test]
    fn chords_are_distinct_rebindable_and_can_be_unbound() {
        let mut keys = Bindings::default();
        let ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        assert_eq!(keys.action("key.keyboard.r", ctrl), Some(Action::Toggle));
        assert_eq!(
            keys.action("key.keyboard.r", Modifiers::default()),
            Some(Action::Recipes)
        );
        keys.set("horizon.aei.toggle", "shift+ctrl+key.mouse.4");
        assert_eq!(keys.get("horizon.aei.toggle"), "ctrl+shift+key.mouse.4");
        assert_eq!(
            keys.action(
                "key.mouse.4",
                Modifiers {
                    shift: true,
                    ..ctrl
                }
            ),
            Some(Action::Toggle)
        );
        keys.set("horizon.aei.toggle", "ctrl+ctrl+key.keyboard.t");
        assert_eq!(keys.get("horizon.aei.toggle"), "ctrl+shift+key.mouse.4");
        keys.set("horizon.aei.toggle", "key.keyboard.unknown");
        assert_eq!(keys.action("key.keyboard.r", ctrl), None);
        keys.set("horizon.aei.toggle", "key.mouse.middle");
        assert_eq!(
            keys.action("key.mouse.middle", Modifiers::default()),
            Some(Action::Toggle)
        );
    }
}
