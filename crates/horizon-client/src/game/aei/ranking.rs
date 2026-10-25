//! Station-local recipe priorities, following JEI's transfer compatibility and
//! missing-input ordering. Catalogue result prioritization is an AEI extension.
use super::*;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Availability {
    matched: usize,
    total: usize,
}
impl Availability {
    pub fn craftable(self) -> bool {
        self.total > 0 && self.matched == self.total
    }
    pub fn priority(self) -> (bool, usize) {
        (
            self.craftable(),
            (100 * self.matched).checked_div(self.total).unwrap_or(0),
        )
    }
}
#[derive(Default)]
pub(super) struct ContextRanking {
    fingerprint: Option<u64>,
    // Retain the old component Arcs so pointer identities cannot be reused
    // while they serve as the snapshot's inexpensive change keys.
    stock: Vec<ItemStack>,
    pub categories: Vec<String>,
    recipes: HashMap<String, HashMap<i32, Availability>>,
    results: HashMap<String, Availability>,
}
impl ContextRanking {
    pub fn availability(&self, recipe: &ViewedRecipe) -> Availability {
        let display = match recipe {
            ViewedRecipe::Native(entry) => entry.display_id,
            ViewedRecipe::Plugin(_) | ViewedRecipe::Station(_) => i32::MIN,
        };
        self.recipes
            .get(recipe.id())
            .and_then(|r| r.get(&display))
            .copied()
            .unwrap_or_default()
    }
}

type Requirement = Vec<(String, u32)>;

/// Count fully satisfied input slots, consuming each available item at most once.
/// Alternatives are whole stacks: 5 iron + 4 gold cannot satisfy "9 iron OR 9 gold".
fn availability(requirements: &[Requirement], stock: &HashMap<String, u32>) -> Availability {
    let items: Vec<_> = stock.iter().filter(|(_, count)| **count > 0).collect();
    let mut amounts: Vec<u32> = items.iter().map(|(_, count)| **count).collect();
    let mut choices: Vec<Vec<(usize, u32)>> = requirements
        .iter()
        .map(|requirement| {
            requirement
                .iter()
                .filter_map(|(item, count)| {
                    items
                        .iter()
                        .position(|(id, available)| *id == item && **available >= *count)
                        .map(|i| (i, *count))
                })
                .collect()
        })
        .collect();
    choices.sort_by_key(Vec::len);
    // Bound pathological mod recipes. Exhaustion can only lower a priority,
    // never mark an impossible recipe as craftable or change any inventory.
    let mut budget = 4096;
    let matched = best_match(0, &choices, &mut amounts, &mut HashMap::new(), &mut budget);
    Availability {
        matched,
        total: requirements.len(),
    }
}
fn best_match(
    index: usize,
    choices: &[Vec<(usize, u32)>],
    amounts: &mut Vec<u32>,
    cache: &mut HashMap<(usize, Vec<u32>), usize>,
    budget: &mut usize,
) -> usize {
    if index == choices.len() || *budget == 0 {
        return 0;
    }
    let key = (index, amounts.clone());
    if let Some(found) = cache.get(&key) {
        return *found;
    }
    *budget -= 1;
    let mut best = 0;
    for &(item, count) in &choices[index] {
        if amounts[item] >= count {
            amounts[item] -= count;
            best = best.max(1 + best_match(index + 1, choices, amounts, cache, budget));
            amounts[item] += count;
            if best == choices.len() - index {
                return best;
            }
        }
    }
    best = best.max(best_match(index + 1, choices, amounts, cache, budget));
    cache.insert(key, best);
    best
}
pub(super) fn stack_matches(expected: &ItemStack, actual: &ItemStack) -> bool {
    expected.item == actual.item
        && expected
            .potion()
            .is_none_or(|potion| actual.potion() == Some(potion))
        && (expected.damage == 0 || actual.damage > 0)
        && expected.crafting_enchantments().iter().all(|(id, level)| {
            actual
                .crafting_enchantments()
                .iter()
                .any(|(actual_id, actual_level)| actual_id == id && actual_level >= level)
        })
}
fn stack_choices(expected: &[ItemStack], stock: &[ItemStack]) -> Requirement {
    let mut choices: Requirement = expected
        .iter()
        .flat_map(|expected| {
            stock
                .iter()
                .enumerate()
                .filter(move |(_, actual)| stack_matches(expected, actual))
                .map(move |(index, _)| (index.to_string(), expected.count))
        })
        .collect();
    choices.sort();
    choices.dedup();
    choices
}
fn requirements(recipe: &ViewedRecipe, stock: &[ItemStack]) -> Vec<Requirement> {
    match recipe {
        ViewedRecipe::Native(entry) => entry
            .requirements()
            .iter()
            .enumerate()
            .map(|(index, ingredient)| {
                if let Some((_, variants)) = entry.stacks.iter().find(|(i, _)| *i == index) {
                    return stack_choices(variants, stock);
                }
                stock
                    .iter()
                    .enumerate()
                    .filter(|(_, stack)| ingredient.0.contains(&*stack.item))
                    .map(|(index, _)| (index.to_string(), 1))
                    .collect()
            })
            .collect(),
        ViewedRecipe::Station(recipe) => recipe
            .slots
            .iter()
            .filter(|slot| slot.required && matches!(slot.role, horizon_mods::aei::Role::Input))
            .map(|slot| stack_choices(&slot.stacks, stock))
            .collect(),
        ViewedRecipe::Plugin(recipe) => recipe
            .slots
            .iter()
            .filter(|slot| matches!(slot.role, horizon_mods::aei::Role::Input))
            .map(|slot| {
                stack_choices(
                    &slot
                        .alternatives
                        .iter()
                        .map(|stack| ItemStack::new(&stack.item, stack.count))
                        .collect::<Vec<_>>(),
                    stock,
                )
            })
            .collect(),
    }
}
impl Game<'_> {
    fn aei_context_categories(&self) -> Vec<String> {
        if self.screen != Screen::Inventory {
            return Vec::new();
        }
        let screens = self.aei_screen_ids();
        let mut categories = Vec::new();
        for screen in &screens {
            if matches!(
                screen.as_str(),
                "minecraft:inventory" | "minecraft:creative" | "minecraft:container"
            ) {
                continue;
            }
            if screen == "minecraft:crafting" {
                categories.push("minecraft:crafting".into());
            }
            categories.extend(self.aei_catalyst_categories(screen));
            categories.extend(
                self.aei
                    .plugins
                    .iter()
                    .flat_map(|plugin| &plugin.click_areas)
                    .filter(|area| area.screen == *screen)
                    .flat_map(|area| area.categories.iter().cloned()),
            );
        }
        categories.sort();
        categories.dedup();
        categories
    }
    pub(super) fn aei_refresh_context(&mut self) {
        let categories = self.aei_context_categories();
        let mut stacks: Vec<ItemStack> = self.inventory.iter().flatten().cloned().collect();
        if !categories.is_empty() {
            if self.craft_size == 3 && self.container.is_none() && self.station.is_none() {
                stacks.extend((100..109).filter_map(|slot| self.slot_mut(slot).clone()));
            } else if matches!(
                self.container_kind(),
                Some(horizon_client::containers::BlockEntity::Furnace(_))
            ) {
                stacks.extend((300..302).filter_map(|slot| self.slot_mut(slot).clone()));
            } else if matches!(
                self.container_kind(),
                Some(horizon_client::containers::BlockEntity::Crafter(_))
            ) {
                stacks.extend((300..309).filter_map(|slot| self.slot_mut(slot).clone()));
            } else if matches!(
                self.container_kind(),
                Some(horizon_client::containers::BlockEntity::Brewing(_))
            ) {
                stacks.extend((300..304).filter_map(|slot| self.slot_mut(slot).clone()));
            } else if let Some(station) = &self.station {
                stacks.extend(station.inputs.iter().flatten().cloned());
            }
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        categories.hash(&mut hash);
        if !categories.is_empty() {
            for stack in &stacks {
                (
                    &stack.item,
                    stack.count,
                    stack.damage,
                    stack.data().map(std::ptr::from_ref),
                )
                    .hash(&mut hash);
            }
        }
        let fingerprint = hash.finish();
        if self.aei.ranking.fingerprint == Some(fingerprint)
            && (categories.is_empty() || self.aei.ranking.stock.len() == stacks.len())
        {
            return;
        }
        let snapshot = if categories.is_empty() {
            Vec::new()
        } else {
            stacks.clone()
        };
        let mut grouped: Vec<ItemStack> = Vec::new();
        for stack in stacks {
            if let Some(existing) = grouped
                .iter_mut()
                .find(|existing| existing.same_item_same_components(&stack))
            {
                existing.count = existing.count.saturating_add(stack.count);
            } else {
                grouped.push(stack);
            }
        }
        let stock: HashMap<String, u32> = grouped
            .iter()
            .enumerate()
            .map(|(index, stack)| (index.to_string(), stack.count))
            .collect();
        let selected = self
            .aei_categories()
            .get(self.aei.category)
            .map(|c| c.id.clone());
        let context_changed = self.aei.ranking.categories != categories;
        let mut ranking = ContextRanking {
            fingerprint: Some(fingerprint),
            stock: snapshot,
            categories,
            ..Default::default()
        };
        if !ranking.categories.is_empty() {
            for recipe in self.aei_all_entries() {
                if !ranking
                    .categories
                    .iter()
                    .any(|category| category == recipe.category())
                {
                    continue;
                }
                let available = availability(&requirements(&recipe, &grouped), &stock);
                let display = match &recipe {
                    ViewedRecipe::Native(entry) => entry.display_id,
                    ViewedRecipe::Plugin(_) | ViewedRecipe::Station(_) => i32::MIN,
                };
                ranking
                    .recipes
                    .entry(recipe.id().into())
                    .or_default()
                    .insert(display, available);
                let results: Vec<String> = match &recipe {
                    ViewedRecipe::Native(entry) => entry
                        .results()
                        .iter()
                        .map(|stack| stack.item.to_string())
                        .collect(),
                    ViewedRecipe::Station(recipe) => recipe
                        .slots
                        .iter()
                        .filter(|slot| matches!(slot.role, horizon_mods::aei::Role::Output))
                        .flat_map(|slot| slot.stacks.iter().map(|stack| stack.item.to_string()))
                        .collect(),
                    ViewedRecipe::Plugin(recipe) => recipe
                        .slots
                        .iter()
                        .filter(|slot| matches!(slot.role, horizon_mods::aei::Role::Output))
                        .flat_map(|slot| slot.alternatives.iter().map(|stack| stack.item.clone()))
                        .collect(),
                };
                for item in results {
                    ranking
                        .results
                        .entry(item)
                        .and_modify(|old| {
                            if available.priority() > old.priority() {
                                *old = available;
                            }
                        })
                        .or_insert(available);
                }
            }
        }
        self.aei.ranking = ranking;
        if let Some(selected) = selected
            && let Some(index) = self
                .aei_categories()
                .iter()
                .position(|category| category.id == selected)
        {
            self.aei.category = index;
        }
        if context_changed {
            self.aei.catalogue_page = 0;
        }
        self.aei_refresh_catalogue();
    }
    pub(super) fn aei_sort_catalogue(&mut self) {
        if self.aei.ranking.categories.is_empty() {
            return;
        }
        let results = &self.aei.ranking.results;
        self.aei.catalogue.sort_by_cached_key(|item| {
            let rank = results.get(item);
            std::cmp::Reverse((rank.is_some(), rank.copied().unwrap_or_default().priority()))
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn requirement(items: &[(&str, u32)]) -> Requirement {
        items
            .iter()
            .map(|(id, count)| (id.to_string(), *count))
            .collect()
    }
    #[test]
    fn station_matching_distinguishes_potions_damage_and_enchanted_books() {
        let water = ItemStack::new("minecraft:potion", 1).with_potion("minecraft:water");
        let awkward = ItemStack::new("minecraft:potion", 1).with_potion("minecraft:awkward");
        assert!(stack_matches(&water, &water));
        assert!(!stack_matches(&water, &awkward));
        let mut damaged = ItemStack::new("minecraft:iron_pickaxe", 1);
        damaged.damage = 100;
        assert!(stack_matches(&damaged, &damaged));
        assert!(!stack_matches(
            &damaged,
            &ItemStack::new("minecraft:iron_pickaxe", 1)
        ));
        let mut book = ItemStack::new("minecraft:enchanted_book", 1);
        book.enchant("minecraft:sharpness", 1);
        assert!(stack_matches(&book, &book));
        assert!(!stack_matches(
            &book,
            &ItemStack::new("minecraft:enchanted_book", 1)
        ));
    }
    #[test]
    fn repeated_inputs_consume_counts_and_overlapping_tags_choose_a_valid_assignment() {
        let stock = HashMap::from([("oak".into(), 1), ("birch".into(), 1)]);
        let tagged = requirement(&[("oak", 1), ("birch", 1)]);
        let oak = requirement(&[("oak", 1)]);
        assert!(availability(&[tagged, oak.clone()], &stock).craftable());
        assert_eq!(
            availability(&[oak.clone(), oak], &stock).priority(),
            (false, 50)
        );
    }
    #[test]
    fn counted_alternatives_cannot_mix_or_reuse_inventory_items() {
        let counted = requirement(&[("iron", 9), ("gold", 9)]);
        assert!(
            !availability(
                std::slice::from_ref(&counted),
                &HashMap::from([("iron".into(), 5), ("gold".into(), 4)])
            )
            .craftable()
        );
        let stock = HashMap::from([("iron".into(), 9)]);
        assert!(availability(std::slice::from_ref(&counted), &stock).craftable());
        assert_eq!(
            availability(&[counted.clone(), counted], &stock).priority(),
            (false, 50)
        );
        assert!(!availability(&[], &stock).craftable());
    }
}
