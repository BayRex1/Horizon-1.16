//! One shared ingredient index per recipe, with JEI's one-second interval.
//! Active time stops while the configured pause control is held.
use super::*;
use std::{collections::HashMap, time::Duration};
use winit::event::MouseButton;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum Key {
    Native(String, i32),
    Station(String, String),
    Plugin(String),
}
impl Key {
    pub fn native(entry: &Entry) -> Self {
        Self::Native(entry.id.clone(), entry.display_id)
    }
    pub fn station(recipe: &stations::StationRecipe) -> Self {
        Self::Station(recipe.category.into(), recipe.id.clone())
    }
}
impl ViewedRecipe {
    fn cycle_key(&self) -> Key {
        match self {
            Self::Native(entry) => Key::native(entry),
            Self::Station(recipe) => Key::station(recipe),
            Self::Plugin(recipe) => Key::Plugin(recipe.id.clone()),
        }
    }
    fn has_alternatives(&self) -> bool {
        match self {
            Self::Native(entry) => {
                entry.requirements().iter().any(|i| i.1.len() > 1)
                    || entry.stacks.iter().any(|(_, s)| s.len() > 1)
                    || entry.results().len() > 1
            }
            Self::Station(recipe) => recipe.slots.iter().any(|s| s.stacks.len() > 1),
            Self::Plugin(recipe) => recipe.slots.iter().any(|s| s.alternatives.len() > 1),
        }
    }
}
pub(super) fn at(index: i64, length: usize) -> usize {
    index.rem_euclid(length.max(1) as i64) as usize
}
struct Selection {
    index: i64,
    at: Duration,
}
#[derive(Default)]
pub(super) struct Cycles {
    last: Duration,
    elapsed: Duration,
    running: bool,
    initial: i64,
    selected: HashMap<Key, Selection>,
    wheel: f32,
    wheel_target: Option<Key>,
    buttons: std::collections::HashSet<MouseButton>,
}
impl Cycles {
    pub fn sync(&mut self, now: Duration, running: bool) {
        if self.running {
            self.elapsed += now.saturating_sub(self.last);
        }
        self.last = now;
        self.running = running;
    }
    pub fn reset(&mut self, now: Duration) {
        self.sync(now, false);
        self.elapsed = Duration::ZERO;
        self.initial = now.as_secs() as i64;
        self.selected.clear();
        self.wheel = 0.0;
        self.wheel_target = None;
    }
    pub fn value(&self, key: &Key) -> i64 {
        self.selected
            .get(key)
            .map_or(self.initial + self.elapsed.as_secs() as i64, |s| {
                s.index + self.elapsed.saturating_sub(s.at).as_secs() as i64
            })
    }
    fn step(&mut self, key: Key, delta: i64) {
        let index = self.value(&key) + delta;
        self.selected.insert(
            key,
            Selection {
                index,
                at: self.elapsed,
            },
        );
    }
    fn scroll(&mut self, key: Key, steps: f32) {
        if self.wheel_target.as_ref() != Some(&key) {
            self.wheel = 0.0;
            self.wheel_target = Some(key.clone());
        }
        self.wheel -= steps;
        let whole = self.wheel.trunc() as i64;
        if whole != 0 {
            self.wheel -= whole as f32;
            self.step(key, whole);
        }
    }
}
impl Game<'_> {
    pub(super) fn aei_cycle_paused(&self) -> bool {
        self.settings
            .extras
            .aei_keys
            .held(Action::PauseCycle, self.aei_modifiers(), |input| {
                horizon_client::keybinds::key_code(input)
                    .is_some_and(|key| self.physical_keys.contains(&key))
                    || self
                        .aei
                        .cycles
                        .buttons
                        .iter()
                        .any(|button| horizon_client::keybinds::mouse_name(*button) == input)
            })
    }
    pub(in crate::game) fn aei_cycle_sync(&mut self) {
        let active = self.aei_modal()
            && self.aei_container_screen()
            && self.settings.extras.aei_enabled
            && self.settings.extras.aei_visible
            && !self.aei_cycle_paused();
        self.aei.cycles.sync(self.started.elapsed(), active);
    }
    pub(in crate::game) fn aei_cycle_mouse(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            self.aei.cycles.buttons.insert(button);
        } else {
            self.aei.cycles.buttons.remove(&button);
        }
        self.aei_cycle_sync();
    }
    pub(in crate::game) fn aei_cycle_unfocus(&mut self) {
        self.aei.cycles.buttons.clear();
        self.aei_cycle_sync();
    }
    fn aei_cycle_target(&self, batch: &HudBatch) -> Option<Key> {
        self.aei_visible_recipes(batch)
            .into_iter()
            .find(|(recipe, area)| area.contains(self.cursor) && recipe.has_alternatives())
            .map(|(recipe, _)| recipe.cycle_key())
    }
    pub(super) fn aei_cycle_available(&self) -> bool {
        self.renderer.as_ref().is_some_and(|r| {
            self.aei_cycle_target(&HudBatch::new([r.config.width, r.config.height]))
                .is_some()
        })
    }
    pub(super) fn aei_cycle_step(&mut self, delta: i64) {
        self.aei_cycle_sync();
        let Some(renderer) = &self.renderer else {
            return;
        };
        let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
        if let Some(key) = self.aei_cycle_target(&batch) {
            self.aei.cycles.step(key, delta);
        }
    }
    pub(super) fn aei_cycle_scroll(&mut self, batch: &HudBatch, steps: f32) -> bool {
        if !self.aei_modal() || !self.aei_cycle_paused() {
            return false;
        }
        let Some(key) = self.aei_cycle_target(batch) else {
            return false;
        };
        self.aei_cycle_sync();
        self.aei.cycles.scroll(key, steps);
        true
    }
    pub(super) fn aei_cycle_hint(&self, batch: &HudBatch) -> Option<Vec<(String, u32)>> {
        self.aei_cycle_target(batch)?;
        let keys = &self.settings.extras.aei_keys;
        let shown = |name| horizon_client::aei_keybinds::display(keys.get(name)).plain();
        Some(vec![
            (
                format!(
                    "Cycle: {} / {}",
                    shown("horizon.aei.previousAlternative"),
                    shown("horizon.aei.nextAlternative")
                ),
                rgba(170, 170, 170, 255),
            ),
            (
                format!(
                    "Hold {} to pause; scroll while paused to cycle.",
                    shown("horizon.aei.pauseCycle")
                ),
                rgba(170, 170, 170, 255),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }
    #[test]
    fn held_pause_preserves_the_interval_and_resumes_without_catching_up() {
        let key = Key::Plugin("test:recipe".into());
        let mut clock = Cycles::default();
        clock.sync(time(0), true);
        clock.sync(time(700), false);
        clock.sync(time(8700), false);
        assert_eq!(clock.value(&key), 0);
        clock.sync(time(9000), true);
        clock.sync(time(9299), true);
        assert_eq!(clock.value(&key), 0);
        clock.sync(time(9300), true);
        assert_eq!(clock.value(&key), 1);
    }
    #[test]
    fn manual_steps_wrap_and_share_the_recipe_index_without_affecting_other_recipes() {
        let a = Key::Plugin("test:a".into());
        let b = Key::Plugin("test:b".into());
        let mut clock = Cycles::default();
        clock.step(a.clone(), -1);
        assert_eq!(at(clock.value(&a), 4), 3);
        assert_eq!(at(clock.value(&a), 2), 1);
        assert_eq!(clock.value(&b), 0);
        clock.scroll(a.clone(), -0.5);
        clock.scroll(a.clone(), -0.5);
        assert_eq!(at(clock.value(&a), 4), 0);
        clock.sync(time(0), true);
        clock.sync(time(990), true);
        clock.step(a.clone(), 1);
        clock.sync(time(1100), true);
        assert_eq!(clock.value(&a), 1);
        clock.sync(time(1990), true);
        assert_eq!(clock.value(&a), 2);
    }
}
