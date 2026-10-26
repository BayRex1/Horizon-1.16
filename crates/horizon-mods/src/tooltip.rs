//! Bounded retained tooltip rules, owned by each running module instance.
pub use crate::runtime::tooltip_bindings::horizon::mods::tooltip::{
    EnchantmentSelector, EnchantmentSource, Rule as TooltipRule, Selector as TooltipSelector,
};
use std::collections::{HashMap, HashSet};
pub const MAX_RULES: usize = 256;
/// Exact typed selector indexes, retaining registration order.
#[derive(Default)]
pub struct TooltipRules {
    items: HashMap<String, Vec<TooltipRule>>,
    applied: HashMap<String, Vec<TooltipRule>>,
    stored: HashMap<String, Vec<TooltipRule>>,
}
impl TooltipRules {
    /// Validate before replacing: an error leaves the previous set intact.
    pub fn replace(&mut self, rules: Vec<TooltipRule>) -> Result<(), String> {
        if rules.len() > MAX_RULES {
            return Err("at most 256 tooltip rules per module".into());
        }
        let mut ids = HashSet::new();
        for rule in &rules {
            if !bounded(&rule.id) || !ids.insert(&rule.id) {
                return Err("tooltip rule IDs must be unique and 1..128 characters".into());
            }
            if !bounded(&rule.translation_key) {
                return Err("tooltip translation keys must be 1..128 characters".into());
            }
            if rule.color.is_some_and(|rgb| rgb > 0xFFFFFF) {
                return Err("tooltip color must be RGB (0..0xFFFFFF)".into());
            }
            let id = match &rule.selector {
                TooltipSelector::ItemId(id) => id,
                TooltipSelector::Enchantment(e) => &e.id,
            };
            if !resource_id(id) {
                return Err(
                    "tooltip selector requires a namespaced resource ID of 1..128 characters"
                        .into(),
                );
            }
        }
        let mut next = Self::default();
        for rule in rules {
            let (index, id) = match &rule.selector {
                TooltipSelector::ItemId(id) => (&mut next.items, id.clone()),
                TooltipSelector::Enchantment(e) => (
                    match e.source {
                        EnchantmentSource::Applied => &mut next.applied,
                        EnchantmentSource::Stored => &mut next.stored,
                    },
                    e.id.clone(),
                ),
            };
            index.entry(id).or_default().push(rule);
        }
        *self = next;
        Ok(())
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn item(&self, id: &str) -> &[TooltipRule] {
        self.items.get(id).map_or(&[], Vec::as_slice)
    }
    pub fn enchantment(&self, source: EnchantmentSource, id: &str) -> &[TooltipRule] {
        let index = match source {
            EnchantmentSource::Applied => &self.applied,
            EnchantmentSource::Stored => &self.stored,
        };
        index.get(id).map_or(&[], Vec::as_slice)
    }
}
fn bounded(s: &str) -> bool {
    !s.is_empty() && s.chars().take(129).count() <= 128
}
fn resource_id(s: &str) -> bool {
    let Some((namespace, path)) = s.split_once(':') else {
        return false;
    };
    bounded(s)
        && !namespace.is_empty()
        && !path.is_empty()
        && namespace
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c))
        && path
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-/".contains(&c))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rule(id: &str) -> TooltipRule {
        TooltipRule {
            id: id.into(),
            selector: TooltipSelector::ItemId("example:wand".into()),
            translation_key: "example.hint".into(),
            color: None,
        }
    }
    #[test]
    fn atomic_ordered_replacement_and_local_clear() {
        let mut rules = TooltipRules::default();
        rules.replace(vec![rule("first"), rule("second")]).unwrap();
        assert!(
            rules
                .replace(vec![rule("duplicate"), rule("duplicate")])
                .is_err()
        );
        assert_eq!(rules.item("example:wand")[0].id, "first");
        assert_eq!(rules.item("example:wand")[1].id, "second");
        let mut other = TooltipRules::default();
        other.replace(vec![rule("other")]).unwrap();
        rules.clear();
        assert!(rules.item("example:wand").is_empty());
        assert_eq!(other.item("example:wand").len(), 1);
    }
    #[test]
    fn input_bounds_and_resource_ids() {
        let mut rules = TooltipRules::default();
        assert!(
            rules
                .replace((0..257).map(|i| rule(&i.to_string())).collect())
                .is_err()
        );
        for id in [
            "wand",
            "Example:wand",
            ":wand",
            "example:",
            "example:with space",
            "example:a:b",
        ] {
            let mut r = rule("id");
            r.selector = TooltipSelector::ItemId(id.into());
            assert!(rules.replace(vec![r]).is_err(), "{id}");
        }
        let mut r = rule(&"é".repeat(128));
        rules.replace(vec![r.clone()]).unwrap();
        r.id.push('é');
        assert!(rules.replace(vec![r]).is_err());
        let mut r = rule("id");
        r.color = Some(0x1000000);
        assert!(rules.replace(vec![r]).is_err());
        let mut r = rule("id");
        r.translation_key.clear();
        assert!(rules.replace(vec![r]).is_err());
    }
    #[test]
    fn typed_selectors_do_not_cross_match() {
        let mut r = rule("applied");
        r.selector = TooltipSelector::Enchantment(EnchantmentSelector {
            source: EnchantmentSource::Applied,
            id: "example:wand".into(),
        });
        let mut rules = TooltipRules::default();
        rules.replace(vec![r]).unwrap();
        assert!(rules.item("example:wand").is_empty());
        assert!(
            rules
                .enchantment(EnchantmentSource::Stored, "example:wand")
                .is_empty()
        );
        assert_eq!(
            rules
                .enchantment(EnchantmentSource::Applied, "example:wand")
                .len(),
            1
        );
    }
}
