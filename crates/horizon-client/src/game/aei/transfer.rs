//! JEI-style transfer through ordinary, server-validated container clicks.
//! A complete plan is checked before any click is applied; results are never taken.
use super::*;
use horizon_client::{containers::BlockEntity, menu::Input};
use std::collections::BTreeMap;

#[derive(Clone)]
struct Ingredient {
    target: usize,
    display: usize,
    alternatives: Vec<ItemStack>,
}
struct Spec {
    clear: Vec<usize>,
    ingredients: Vec<Ingredient>,
    error: Option<&'static str>,
}
#[derive(Clone)]
pub(super) struct Press {
    pub area: Rect,
    pub recipe: ViewedRecipe,
}
#[derive(Debug)]
struct Plan {
    clicks: Vec<(usize, i32)>,
    expected: BTreeMap<usize, Option<ItemStack>>,
}
struct Prepared {
    spec: Spec,
    missing: Vec<usize>,
    error: Option<&'static str>,
    plan: Option<Plan>,
}
fn same(a: &Option<ItemStack>, b: &Option<ItemStack>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.count == b.count && a.same_item_same_components(b),
        _ => false,
    }
}
fn groups(slots: &BTreeMap<usize, Option<ItemStack>>) -> Vec<ItemStack> {
    let mut out: Vec<ItemStack> = Vec::new();
    for stack in slots.values().flatten() {
        if let Some(group) = out
            .iter_mut()
            .find(|group| group.same_item_same_components(stack))
        {
            group.count = group.count.saturating_add(stack.count);
        } else {
            out.push(stack.clone());
        }
    }
    out
}
/// Match complete input slots, reserving each physical stack component group once.
fn assignment(
    ingredients: &[Ingredient],
    stock: &[ItemStack],
    quantity: u32,
    max: &impl Fn(&ItemStack) -> u32,
) -> Vec<Option<usize>> {
    let mut choices: Vec<_> = ingredients
        .iter()
        .enumerate()
        .map(|(index, ingredient)| {
            let candidates: Vec<_> = stock
                .iter()
                .enumerate()
                .filter_map(|(group, actual)| {
                    ingredient
                        .alternatives
                        .iter()
                        .find(|expected| {
                            ranking::stack_matches(expected, actual)
                                && expected.count.saturating_mul(quantity) <= actual.count
                                && expected.count.saturating_mul(quantity) <= max(actual)
                        })
                        .map(|expected| (group, expected.count * quantity))
                })
                .collect();
            (index, candidates)
        })
        .collect();
    choices.sort_by_key(|(_, options)| options.len());
    let mut amounts: Vec<_> = stock.iter().map(|stack| stack.count).collect();
    let mut current = vec![None; ingredients.len()];
    let mut best = current.clone();
    let mut budget = 8192;
    fn visit(
        index: usize,
        choices: &[(usize, Vec<(usize, u32)>)],
        amounts: &mut [u32],
        current: &mut [Option<usize>],
        best: &mut Vec<Option<usize>>,
        budget: &mut usize,
    ) {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        if current.iter().flatten().count() > best.iter().flatten().count() {
            best.clone_from_slice(current);
        }
        if index == choices.len() || best.iter().all(Option::is_some) {
            return;
        }
        let (ingredient, candidates) = &choices[index];
        for &(group, count) in candidates {
            if amounts[group] >= count {
                amounts[group] -= count;
                current[*ingredient] = Some(group);
                visit(index + 1, choices, amounts, current, best, budget);
                current[*ingredient] = None;
                amounts[group] += count;
                if best.iter().all(Option::is_some) {
                    return;
                }
            }
        }
        visit(index + 1, choices, amounts, current, best, budget);
    }
    visit(
        0,
        &choices,
        &mut amounts,
        &mut current,
        &mut best,
        &mut budget,
    );
    best
}
/// Record a pickup, an exact deposit and returning any remainder to its source.
fn move_items(
    slots: &mut BTreeMap<usize, Option<ItemStack>>,
    clicks: &mut Vec<(usize, i32)>,
    from: usize,
    to: usize,
    count: u32,
) {
    let mut source = slots[&from].clone().expect("planned source");
    let whole = count == source.count;
    clicks.push((from, 0));
    if whole {
        clicks.push((to, 0));
    } else {
        clicks.extend((0..count).map(|_| (to, 1)));
        clicks.push((from, 0));
    }
    let mut taken = source.clone();
    taken.count = count;
    source.count -= count;
    slots.insert(from, (source.count > 0).then_some(source));
    match slots.get_mut(&to).expect("planned destination") {
        Some(stack) => stack.count += count,
        empty => *empty = Some(taken),
    }
}
fn plan(
    mut slots: BTreeMap<usize, Option<ItemStack>>,
    clear: &[usize],
    desired: BTreeMap<usize, Option<ItemStack>>,
    max: &impl Fn(&ItemStack) -> u32,
) -> Result<Plan, &'static str> {
    if desired
        .iter()
        .all(|(slot, stack)| same(&slots[slot], stack))
    {
        return Ok(Plan {
            clicks: Vec::new(),
            expected: desired,
        });
    }
    let mut clicks = Vec::new();
    for &slot in clear {
        if slots[&slot].as_ref().is_some_and(|current| {
            desired[&slot].as_ref().is_some_and(|expected| {
                current.same_item_same_components(expected) && current.count <= expected.count
            })
        }) {
            continue;
        }
        while let Some(stack) = slots[&slot].clone() {
            let destination = (0..36)
                .find(|id| {
                    slots[id].as_ref().is_some_and(|other| {
                        other.same_item_same_components(&stack) && other.count < max(other)
                    })
                })
                .or_else(|| (0..36).find(|id| slots[id].is_none()))
                .ok_or("Inventory is too full to return the current ingredients.")?;
            let room =
                max(&stack).saturating_sub(slots[&destination].as_ref().map_or(0, |s| s.count));
            if room == 0 {
                return Err("Inventory is too full to return the current ingredients.");
            }
            move_items(
                &mut slots,
                &mut clicks,
                slot,
                destination,
                room.min(stack.count),
            );
        }
    }
    for (&target, expected) in &desired {
        let Some(expected) = expected else {
            continue;
        };
        let mut remaining = expected.count - slots[&target].as_ref().map_or(0, |stack| stack.count);
        while remaining > 0 {
            let source = (0..36)
                .find(|id| {
                    slots[id]
                        .as_ref()
                        .is_some_and(|stack| stack.same_item_same_components(expected))
                })
                .ok_or("The available ingredients changed. Try again.")?;
            let count = remaining.min(slots[&source].as_ref().expect("source").count);
            move_items(&mut slots, &mut clicks, source, target, count);
            remaining -= count;
        }
    }
    Ok(Plan {
        clicks,
        expected: desired,
    })
}
impl Game<'_> {
    fn aei_transfer_stack(&self, id: usize) -> Option<ItemStack> {
        match id {
            0..36 => self.inventory.get(id).cloned().flatten(),
            100..109 => self.craft_grid.get(id - 100).cloned().flatten(),
            600..603 => self
                .station
                .as_ref()?
                .inputs
                .get(id - 600)
                .cloned()
                .flatten(),
            300..309 => self
                .container_kind()?
                .slots()
                .get(id - 300)
                .cloned()
                .flatten(),
            _ => None,
        }
    }
    fn aei_transfer_spec(&self, recipe: &ViewedRecipe) -> Option<Spec> {
        let mut spec = Spec {
            clear: Vec::new(),
            ingredients: Vec::new(),
            error: None,
        };
        match recipe {
            ViewedRecipe::Native(entry) => {
                let (width, first) = if matches!(entry.display, Display::Furnace(_)) {
                    let Some(BlockEntity::Furnace(furnace)) = self.container_kind() else {
                        return None;
                    };
                    if furnace.kind.recipe_type() != recipe.category() {
                        return None;
                    }
                    spec.clear = vec![300];
                    (1, 300)
                } else if matches!(self.container_kind(), Some(BlockEntity::Crafter(_))) {
                    spec.clear = (300..309).collect();
                    (3, 300)
                } else if self.screen == Screen::Inventory
                    && self.container.is_none()
                    && self.station.is_none()
                    && !self.mount_active()
                {
                    let width = self.craft_size;
                    spec.clear = (100..100 + width * width).collect();
                    (width, 100)
                } else {
                    return None;
                };
                if !entry.fits(width, width) {
                    spec.error = Some("This recipe needs a larger crafting grid.");
                    return Some(spec);
                }
                let visual: Vec<usize> = match &entry.display {
                    Display::Shaped {
                        width,
                        height,
                        cells,
                    } => cells
                        .iter()
                        .enumerate()
                        .filter_map(|(i, c)| c.as_ref().map(|_| crafting_index(i, *width, *height)))
                        .collect(),
                    Display::Shapeless(items) => {
                        let size = if items.len() > 4 {
                            3
                        } else if items.len() > 1 {
                            2
                        } else {
                            1
                        };
                        (0..items.len())
                            .map(|i| crafting_index(i, size, size))
                            .collect()
                    }
                    Display::Furnace(_) => vec![0],
                };
                let requirements = entry.requirements();
                let cells = if let Display::Shapeless(items) = &entry.display {
                    let size = if items.len() > 4 {
                        3
                    } else if items.len() > 1 {
                        2
                    } else {
                        1
                    };
                    horizon_client::recipe_book::place_cells(
                        width,
                        width,
                        size,
                        size,
                        &(0..items.len()).map(Some).collect::<Vec<_>>(),
                    )
                } else {
                    entry.grid_cells(width, width)
                };
                for (ingredient, cell) in cells {
                    let Some(index) = ingredient else {
                        continue;
                    };
                    let alternatives = entry
                        .stacks
                        .iter()
                        .find(|(i, _)| *i == index)
                        .map(|(_, stacks)| stacks.clone())
                        .unwrap_or_else(|| {
                            requirements[index]
                                .1
                                .iter()
                                .map(|id| ItemStack::new(id, 1))
                                .collect()
                        });
                    spec.ingredients.push(Ingredient {
                        target: first + cell,
                        display: visual[index],
                        alternatives,
                    });
                }
            }
            ViewedRecipe::Station(recipe) => {
                let kind = self.station.as_ref().map(|s| s.kind);
                use super::super::stations::Kind;
                let first = match recipe.category {
                    "minecraft:stonecutting" if kind == Some(Kind::Stonecutter) => 600,
                    "minecraft:smithing" if kind == Some(Kind::Smithing) => 600,
                    "minecraft:anvil" if kind == Some(Kind::Anvil) => 600,
                    "minecraft:grindstone" if kind == Some(Kind::Grindstone) => 600,
                    "minecraft:brewing"
                        if matches!(self.container_kind(), Some(BlockEntity::Brewing(_))) =>
                    {
                        300
                    }
                    "minecraft:fuel"
                        if matches!(self.container_kind(), Some(BlockEntity::Furnace(_))) =>
                    {
                        301
                    }
                    _ => return None,
                };
                spec.clear = if recipe.category == "minecraft:brewing" {
                    (300..304).collect()
                } else if first == 600 {
                    (600..600 + self.station.as_ref()?.kind.inputs()).collect()
                } else {
                    vec![first]
                };
                for (index, slot) in
                    recipe.slots.iter().enumerate().filter(|(_, s)| {
                        s.required && matches!(s.role, horizon_mods::aei::Role::Input)
                    })
                {
                    let offset = if recipe.category == "minecraft:brewing" && index == 3 {
                        3
                    } else {
                        spec.ingredients.len()
                    };
                    spec.ingredients.push(Ingredient {
                        target: first + offset,
                        display: index,
                        alternatives: slot.stacks.clone(),
                    });
                }
            }
            ViewedRecipe::Plugin(_) => return None,
        }
        if let Some(BlockEntity::Crafter(crafter)) = self.container_kind()
            && spec.ingredients.iter().any(|ingredient| {
                ingredient
                    .target
                    .checked_sub(300)
                    .and_then(|i| crafter.disabled.get(i))
                    .copied()
                    .unwrap_or(false)
            })
        {
            spec.error = Some("Enable the required crafter slots before transferring this recipe.");
        }
        if spec.ingredients.is_empty() && spec.error.is_none() {
            return None;
        }
        Some(spec)
    }
    fn aei_prepare_transfer(&self, recipe: &ViewedRecipe, maximum: bool) -> Option<Prepared> {
        let spec = self.aei_transfer_spec(recipe)?;
        let mut prepared = Prepared {
            error: spec.error,
            spec,
            missing: Vec::new(),
            plan: None,
        };
        if prepared.error.is_some() {
            return Some(prepared);
        }
        if self.spectator() {
            prepared.error = Some("Spectator mode is read-only.");
            return Some(prepared);
        }
        if self.carried.is_some() {
            prepared.error = Some("Put down the carried stack before transferring a recipe.");
            return Some(prepared);
        }
        let slots: BTreeMap<_, _> = (0..36)
            .chain(prepared.spec.clear.iter().copied())
            .map(|id| (id, self.aei_transfer_stack(id)))
            .collect();
        let stock = groups(&slots);
        let max = |stack: &ItemStack| {
            self.res
                .items()
                .get(&stack.item)
                .map_or(64, |item| item.max_stack)
                .min(99)
        };
        let mut picked = assignment(&prepared.spec.ingredients, &stock, 1, &max);
        prepared.missing = picked
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.is_none().then_some(i))
            .collect();
        if !prepared.missing.is_empty() {
            return Some(prepared);
        }
        let mut quantity = 1;
        if maximum {
            let (mut low, mut high) = (1_u32, 99_u32);
            while low < high {
                let middle = (low + high).div_ceil(2);
                let candidate = assignment(&prepared.spec.ingredients, &stock, middle, &max);
                if candidate.iter().all(Option::is_some) {
                    low = middle;
                    picked = candidate;
                } else {
                    high = middle - 1;
                }
            }
            quantity = low;
        }
        let mut desired: BTreeMap<_, _> =
            prepared.spec.clear.iter().map(|id| (*id, None)).collect();
        for (ingredient, group) in prepared.spec.ingredients.iter().zip(picked) {
            let mut stack = stock[group?].clone();
            let count = ingredient
                .alternatives
                .iter()
                .find(|s| ranking::stack_matches(s, &stack))?
                .count;
            stack.count = count * quantity;
            desired.insert(ingredient.target, Some(stack));
        }
        match plan(slots, &prepared.spec.clear, desired, &max) {
            Ok(plan) => prepared.plan = Some(plan),
            Err(error) => prepared.error = Some(error),
        }
        Some(prepared)
    }
    pub(super) fn aei_transfer_area(area: Rect) -> Rect {
        Rect::new(area.x + area.w + 2.0, area.y + area.h - 13.0, 13.0, 13.0)
    }
    pub(super) fn aei_transfer_value(
        &self,
        recipe: &ViewedRecipe,
        area: Rect,
    ) -> serde_json::Value {
        let Some(prepared) = self.aei_prepare_transfer(recipe, false) else {
            return serde_json::Value::Null;
        };
        let button = Self::aei_transfer_area(area);
        serde_json::json!({"area":[button.x,button.y,button.w,button.h],"enabled":prepared.plan.is_some(),"error":prepared.error,
            "missing":prepared.missing.iter().map(|i| {let ingredient=&prepared.spec.ingredients[*i];serde_json::json!({"slot":ingredient.display,"alternatives":ingredient.alternatives.iter().map(|s| self.stack_value(s)).collect::<Vec<_>>()})}).collect::<Vec<_>>()})
    }
    pub(super) fn draw_aei_transfer(
        &self,
        batch: &mut HudBatch,
        recipe: &ViewedRecipe,
        area: Rect,
    ) {
        let Some(prepared) = self.aei_prepare_transfer(recipe, false) else {
            return;
        };
        let button = Self::aei_transfer_area(area);
        let enabled = prepared.plan.is_some();
        let hovered = button.contains(self.cursor);
        let pressed = enabled
            && self
                .aei
                .transfer_press
                .as_ref()
                .is_some_and(|p| p.area == button);
        let texture = if !enabled {
            "button_disabled"
        } else if pressed {
            if hovered {
                "button_pressed_highlight"
            } else {
                "button_pressed"
            }
        } else if hovered {
            "button_highlight"
        } else {
            "button_enabled"
        };
        nine_slice(batch, &self.gui.atlas, texture, button, [6.0; 4]);
        let tint = if enabled { 255 } else { 160 };
        let offset = if pressed { 0.5 } else { 0.0 };
        part(
            batch,
            &self.gui.atlas,
            "icons/recipe_transfer",
            Rect::new(0.0, 0.0, 7.0, 7.0),
            Rect::new(button.x + 3.0 + offset, button.y + 3.0 + offset, 7.0, 7.0),
            rgba(tint, tint, tint, 255),
        );
        horizon_client::widgets::record(
            "Transfer recipe",
            [button.x, button.y, button.w, button.h],
            enabled,
        );
        if hovered {
            let slots = self.aei_viewed_slots(recipe, area);
            for index in &prepared.missing {
                if let Some((_, rect)) = slots.get(prepared.spec.ingredients[*index].display) {
                    rect.fill(batch, &self.gui.atlas, rgba(255, 0, 0, 102));
                }
            }
        }
    }
    pub(super) fn aei_transfer_tooltip(&self, batch: &mut HudBatch) -> bool {
        if !self.aei_modal() {
            return false;
        }
        for (recipe, area) in self.aei_visible_recipes(batch) {
            if !Self::aei_transfer_area(area).contains(self.cursor) {
                continue;
            }
            let Some(prepared) = self.aei_prepare_transfer(&recipe, false) else {
                continue;
            };
            let missing_items = prepared.error.is_none() && !prepared.missing.is_empty();
            let mut lines = Vec::new();
            if let Some(error) = prepared.error {
                lines.push(error.to_owned());
            } else if prepared.missing.is_empty() {
                lines.push("Move items".into());
                lines.push("Shift-click to move the maximum amount.".into());
            } else {
                lines.push("Missing items".into());
                let mut missing = BTreeMap::<String, u32>::new();
                for i in prepared.missing {
                    let ingredient = &prepared.spec.ingredients[i];
                    let names = ingredient
                        .alternatives
                        .iter()
                        .take(3)
                        .map(|s| {
                            self.stack_tooltip(s)
                                .first()
                                .map(|(name, _)| name.plain())
                                .unwrap_or_else(|| self.pack.names.item(&s.item))
                        })
                        .collect::<Vec<_>>()
                        .join(" / ");
                    let label = if ingredient.alternatives.len() > 3 {
                        format!("{names} / …")
                    } else {
                        names
                    };
                    *missing.entry(label).or_default() +=
                        ingredient.alternatives.first().map_or(1, |s| s.count);
                }
                lines.extend(
                    missing
                        .into_iter()
                        .map(|(name, count)| format!("{count} × {name}")),
                );
            }
            let lines: Vec<_> = lines
                .iter()
                .map(|line| {
                    if missing_items {
                        horizon_client::text::Text::colored(line, horizon_client::text::RED)
                    } else {
                        horizon_client::text::Text::literal(line)
                    }
                })
                .collect();
            horizon_client::ui::tooltip(batch, &self.gui.atlas, &lines, self.cursor);
            return true;
        }
        false
    }
    pub(super) fn aei_transfer_click(&mut self, batch: &HudBatch, right: bool) -> bool {
        self.aei.transfer_press = None;
        if !self.aei_modal() {
            return false;
        }
        for (recipe, area) in self.aei_visible_recipes(batch) {
            let button = Self::aei_transfer_area(area);
            if button.contains(self.cursor)
                && let Some(prepared) = self.aei_prepare_transfer(&recipe, false)
            {
                if !right && prepared.plan.is_some() {
                    self.aei.transfer_press = Some(Press {
                        area: button,
                        recipe,
                    });
                }
                return true;
            }
        }
        false
    }
    pub(super) fn aei_transfer_release(&mut self, batch: &HudBatch, right: bool) -> bool {
        let Some(press) = self.aei.transfer_press.take() else {
            return false;
        };
        if right || !self.aei_modal() || !press.area.contains(self.cursor) {
            return true;
        }
        let still_visible = self
            .aei_visible_recipes(batch)
            .iter()
            .any(|(recipe, area)| {
                let same_display = match (recipe, &press.recipe) {
                    (ViewedRecipe::Native(a), ViewedRecipe::Native(b)) => {
                        a.display_id == b.display_id
                    }
                    _ => true,
                };
                recipe.id() == press.recipe.id()
                    && same_display
                    && Self::aei_transfer_area(*area) == press.area
            });
        if !still_visible {
            return true;
        }
        let Some(prepared) = self.aei_prepare_transfer(&press.recipe, self.has_shift_down()) else {
            return true;
        };
        let Some(plan) = prepared.plan else {
            return true;
        };
        let ops: Vec<_> = plan
            .clicks
            .iter()
            .map(|(slot, button)| (Some(*slot), *button, Input::Pickup))
            .collect();
        let mut valid = false;
        self.menu_session(|menu, env, to_menu| {
            for (slot, button, _) in &ops {
                let Some(slot) = slot.and_then(to_menu) else {
                    return Some(false);
                };
                if menu
                    .click(env, slot as i32, *button, Input::Pickup)
                    .is_err()
                {
                    return Some(false);
                }
            }
            valid = menu.carried.is_none()
                && menu.drops.is_empty()
                && plan.expected.iter().all(|(id, expected)| {
                    to_menu(*id).is_some_and(|id| same(&menu.slot(id).cloned(), expected))
                });
            Some(false) // Preview only; ordinary menu operations apply and send the actual clicks.
        });
        if !valid || !self.menu_ops(&ops) {
            return true;
        }
        if press.recipe.category() == "minecraft:stonecutting" {
            let id = press.recipe.id();
            let index = self.station.as_ref().and_then(|s| {
                s.stone
                    .recipes
                    .iter()
                    .position(|i| self.res.stations().recipes.stonecutting[*i].id == id)
            });
            if let Some(index) = index {
                self.menu_session(|menu, env, _| Some(menu.button(env, index as i32)));
                self.remote_station_button(index as i32);
            }
        }
        self.aei_open_catalogue();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(target: usize, items: &[&str]) -> Ingredient {
        Ingredient {
            target,
            display: target,
            alternatives: items.iter().map(|item| ItemStack::new(item, 1)).collect(),
        }
    }
    #[test]
    fn overlapping_alternatives_reserve_counts_and_report_missing_slots() {
        let ingredients = vec![
            input(100, &["minecraft:oak_planks", "minecraft:birch_planks"]),
            input(101, &["minecraft:oak_planks"]),
        ];
        let stock = vec![
            ItemStack::new("minecraft:oak_planks", 1),
            ItemStack::new("minecraft:birch_planks", 1),
        ];
        assert_eq!(
            assignment(&ingredients, &stock, 1, &|_| 64),
            vec![Some(1), Some(0)]
        );
        let repeated = vec![input(100, &["minecraft:oak_planks"]); 4];
        assert_eq!(
            assignment(&repeated, &stock, 1, &|_| 64)
                .iter()
                .flatten()
                .count(),
            1
        );
        assert_eq!(
            assignment(&ingredients, &stock, 2, &|_| 64),
            vec![None, None]
        );
    }
    #[test]
    fn potion_types_and_component_groups_remain_distinct() {
        let water = ItemStack::new("minecraft:potion", 1).with_potion("minecraft:water");
        let awkward = ItemStack::new("minecraft:potion", 1).with_potion("minecraft:awkward");
        let ingredient = Ingredient {
            target: 300,
            display: 0,
            alternatives: vec![water.clone()],
        };
        assert_eq!(
            assignment(
                std::slice::from_ref(&ingredient),
                std::slice::from_ref(&awkward),
                1,
                &|_| 1
            ),
            vec![None]
        );
        let slots = [
            (0, Some(water.clone())),
            (1, Some(awkward)),
            (2, Some(water)),
        ]
        .into_iter()
        .collect();
        let grouped = groups(&slots);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].count, 2);
        assert_eq!(assignment(&[ingredient], &grouped, 2, &|_| 1), vec![None]);
    }
    #[test]
    fn full_inventory_refuses_clearing_and_an_already_correct_grid_is_a_noop() {
        let mut slots: BTreeMap<_, _> = (0..36)
            .map(|i| (i, Some(ItemStack::new("minecraft:stone", 64))))
            .collect();
        slots.insert(100, Some(ItemStack::new("minecraft:dirt", 1)));
        let desired = [(100, Some(ItemStack::new("minecraft:stone", 1)))]
            .into_iter()
            .collect();
        assert!(
            plan(slots.clone(), &[100], desired, &|_| 64)
                .unwrap_err()
                .contains("too full")
        );
        let desired = [(100, slots[&100].clone())].into_iter().collect();
        assert!(
            plan(slots, &[100], desired, &|_| 64)
                .unwrap()
                .clicks
                .is_empty()
        );
    }

    #[test]
    fn topping_up_a_matching_input_does_not_require_an_empty_inventory_slot() {
        let mut slots: BTreeMap<_, _> = (0..36)
            .map(|i| (i, Some(ItemStack::new("minecraft:stone", 64))))
            .collect();
        slots.insert(0, Some(ItemStack::new("minecraft:oak_planks", 64)));
        slots.insert(100, Some(ItemStack::new("minecraft:oak_planks", 1)));
        let desired = [(100, Some(ItemStack::new("minecraft:oak_planks", 4)))]
            .into_iter()
            .collect();
        let plan = plan(slots, &[100], desired, &|_| 64).unwrap();
        assert!(!plan.clicks.is_empty());
        assert!(plan.clicks.iter().all(|(slot, _)| matches!(slot, 0 | 100)));
    }
}
