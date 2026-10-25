//! Remaining vanilla JEI categories, backed by Horizon's loaded recipes and
//! workstation rules. Never sample the live world's random source for a display.
use super::*;
use horizon_client::{
    crafting::Ingredient,
    stations::{self, Smithing},
};
use horizon_mods::aei::{Category, Role};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Slot {
    pub role: Role,
    pub x: f32,
    pub y: f32,
    pub stacks: Vec<ItemStack>,
    pub background: u8,
    pub required: bool,
}
impl Slot {
    fn new(role: Role, x: f32, y: f32, stacks: Vec<ItemStack>) -> Self {
        Self {
            role,
            x,
            y,
            stacks,
            background: 18,
            required: true,
        }
    }
}
#[derive(Clone, Copy, Default)]
pub(super) enum Decoration {
    #[default]
    Plain,
    Fire(u32),
    Fuel(u32),
    Brewing,
}
#[derive(Clone)]
pub(super) struct StationRecipe {
    pub id: String,
    pub category: &'static str,
    pub slots: Vec<Slot>,
    pub arrow: Option<(f32, f32)>,
    pub decoration: Decoration,
    pub text: Option<String>,
    pub costs: Vec<i32>,
    brewing_steps: usize,
}
impl StationRecipe {
    fn new(
        id: String,
        category: &'static str,
        slots: Vec<Slot>,
        arrow: Option<(f32, f32)>,
    ) -> Self {
        Self {
            id,
            category,
            slots,
            arrow,
            decoration: Decoration::Plain,
            text: None,
            costs: Vec::new(),
            brewing_steps: usize::MAX,
        }
    }
    pub fn matches(&self, focus: &ItemStack, uses: bool) -> bool {
        self.slots.iter().any(|slot| {
            matches!(
                (uses, slot.role),
                (true, Role::Input) | (false, Role::Output)
            ) && slot.stacks.iter().any(|stack| {
                stack.item == focus.item
                    && focus.potion().is_none_or(|potion| {
                        stack.potion() == Some(potion) || uses && stack.potion().is_none()
                    })
                    && (&*focus.item != "minecraft:enchanted_book"
                        || !focus.has_any_enchantments()
                        || stack.crafting_enchantments() == focus.crafting_enchantments())
            })
        })
    }
}
fn ingredient(value: &Ingredient) -> Vec<ItemStack> {
    value.1.iter().map(|id| ItemStack::new(id, 1)).collect()
}
fn potion_ingredient(value: &horizon_client::brewing::PotionIngredient) -> Vec<ItemStack> {
    let mut items: Vec<_> = value.items.iter().collect();
    items.sort();
    let mut out = Vec::new();
    for item in items {
        if let Some(potions) = &value.potions {
            let mut potions: Vec<_> = potions.iter().collect();
            potions.sort();
            out.extend(
                potions
                    .into_iter()
                    .map(|p| ItemStack::new(item, 1).with_potion(p)),
            );
        } else {
            out.push(ItemStack::new(item, 1));
        }
    }
    out
}
pub(super) fn catalysts(category: &str) -> &'static [&'static str] {
    match category {
        "minecraft:crafting" => &["minecraft:crafting_table", "minecraft:crafter"],
        "minecraft:stonecutting" => &["minecraft:stonecutter"],
        "minecraft:smelting" => &["minecraft:furnace"],
        "minecraft:blasting" => &["minecraft:blast_furnace"],
        "minecraft:smoking" => &["minecraft:smoker"],
        "minecraft:campfire_cooking" => &["minecraft:campfire", "minecraft:soul_campfire"],
        "minecraft:smithing" => &["minecraft:smithing_table"],
        "minecraft:compostable" => &["minecraft:composter"],
        "minecraft:fuel" => &[
            "minecraft:furnace",
            "minecraft:smoker",
            "minecraft:blast_furnace",
        ],
        "minecraft:brewing" => &["minecraft:brewing_stand"],
        "minecraft:anvil" => &[
            "minecraft:anvil",
            "minecraft:chipped_anvil",
            "minecraft:damaged_anvil",
        ],
        "minecraft:grindstone" => &["minecraft:grindstone"],
        _ => &[],
    }
}
impl Game<'_> {
    pub(super) fn aei_station_definitions(&self) -> Vec<Category> {
        [
            (
                "minecraft:stonecutting",
                "Stonecutting",
                "minecraft:stonecutter",
                82,
                34,
            ),
            (
                "minecraft:campfire_cooking",
                "Campfire Cooking",
                "minecraft:campfire",
                82,
                44,
            ),
            (
                "minecraft:smithing",
                "Smithing",
                "minecraft:smithing_table",
                108,
                28,
            ),
            (
                "minecraft:compostable",
                "Composting",
                "minecraft:composter",
                120,
                18,
            ),
            ("minecraft:fuel", "Fuel", "minecraft:coal", 160, 34),
            (
                "minecraft:brewing",
                "Brewing",
                "minecraft:brewing_stand",
                114,
                61,
            ),
            ("minecraft:anvil", "Anvil", "minecraft:anvil", 125, 38),
            (
                "minecraft:grindstone",
                "Grindstone",
                "minecraft:grindstone",
                125,
                52,
            ),
        ]
        .into_iter()
        .map(|(id, title, icon, width, height)| Category {
            id: id.into(),
            title: title.into(),
            icon: icon.into(),
            width,
            height,
        })
        .collect()
    }
    pub(super) fn aei_station_entries(&self) -> Vec<ViewedRecipe> {
        let mut out = Vec::<StationRecipe>::new();
        let input = |x, y, stacks| Slot::new(Role::Input, x, y, stacks);
        let output = |x, y, stack| Slot::new(Role::Output, x, y, vec![stack]);
        for recipe in &self.res.stations().recipes.stonecutting {
            let mut result = output(61.0, 9.0, recipe.result.clone());
            result.background = 26;
            out.push(StationRecipe::new(
                recipe.id.clone(),
                "minecraft:stonecutting",
                vec![input(1.0, 9.0, ingredient(&recipe.input)), result],
                Some((26.0, 9.0)),
            ));
        }
        for recipe in self
            .res
            .recipes()
            .cooking
            .iter()
            .filter(|r| r.kind == "minecraft:campfire_cooking")
        {
            let mut result = output(61.0, 9.0, recipe.result.clone());
            result.background = 26;
            let mut viewed = StationRecipe::new(
                format!("minecraft:{}", recipe.id.trim_start_matches("minecraft:")),
                "minecraft:campfire_cooking",
                vec![input(1.0, 1.0, ingredient(&recipe.ingredient)), result],
                Some((26.0, 7.0)),
            );
            viewed.decoration = Decoration::Fire(recipe.time.max(1));
            viewed.text = Some(format!("{}s", recipe.time / 20));
            out.push(viewed);
        }
        let brewing = &self.res.stations().brewing.recipes;
        let mut steps = std::collections::HashMap::from([(
            (
                "minecraft:potion".to_string(),
                "minecraft:water".to_string(),
            ),
            0usize,
        )]);
        for _ in 0..brewing.len() {
            let mut changed = false;
            for recipe in brewing {
                let distance = steps
                    .iter()
                    .filter(|((item, potion), _)| {
                        recipe
                            .input
                            .test(&ItemStack::new(item, 1).with_potion(potion))
                    })
                    .map(|(_, steps)| steps + 1)
                    .min();
                if let Some(distance) = distance {
                    let key = (
                        recipe.output.item.to_string(),
                        recipe.output.potion().unwrap_or("").to_string(),
                    );
                    if steps.get(&key).is_none_or(|old| distance < *old) {
                        steps.insert(key, distance);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let brewing_start = out.len();
        for recipe in brewing {
            let bottles = potion_ingredient(&recipe.input);
            let mut slots = vec![
                input(1.0, 37.0, bottles.clone()),
                input(24.0, 44.0, bottles.clone()),
                input(47.0, 37.0, bottles),
                input(24.0, 3.0, potion_ingredient(&recipe.reagent)),
                output(81.0, 3.0, recipe.output.clone()),
            ];
            // Brewing accepts one to three bottles; the other two positions
            // illustrate the batch, rather than requiring three for availability.
            for slot in &mut slots[..3] {
                slot.background = 0;
            }
            slots[1].required = false;
            slots[2].required = false;
            let mut viewed =
                StationRecipe::new(recipe.id.clone(), "minecraft:brewing", slots, None);
            viewed.decoration = Decoration::Brewing;
            viewed.brewing_steps = steps
                .get(&(
                    recipe.output.item.to_string(),
                    recipe.output.potion().unwrap_or("").to_string(),
                ))
                .copied()
                .unwrap_or(usize::MAX);
            viewed.text = Some(format!(
                "Steps: {}",
                steps
                    .get(&(
                        recipe.output.item.to_string(),
                        recipe.output.potion().unwrap_or("").to_string()
                    ))
                    .map(usize::to_string)
                    .unwrap_or_else(|| "?".into())
            ));
            out.push(viewed);
        }
        out[brewing_start..].sort_by_key(|recipe| recipe.brewing_steps);
        for (index, recipe) in self.res.stations().recipes.smithing.iter().enumerate() {
            let (template, base, addition) = match recipe {
                Smithing::Transform {
                    template,
                    base,
                    addition,
                    ..
                }
                | Smithing::Trim {
                    template,
                    base,
                    addition,
                    ..
                } => (template, base, addition),
            };
            for base in ingredient(base) {
                let mut additions = Vec::new();
                let mut results = Vec::new();
                let Some(template_stack) = ingredient(template).into_iter().next() else {
                    continue;
                };
                for addition in ingredient(addition) {
                    if let Some(result) = self.res.stations().recipes.smithing_result(
                        &self.res.stations().enchantments,
                        Some(&template_stack),
                        Some(&base),
                        Some(&addition),
                    ) {
                        additions.push(addition);
                        results.push(result);
                    }
                }
                if results.is_empty() {
                    continue;
                }
                let mut viewed = StationRecipe::new(
                    format!(
                        "minecraft:aei/smithing/{index}/{}",
                        base.item.replace(':', "/")
                    ),
                    "minecraft:smithing",
                    vec![
                        input(1.0, 6.0, ingredient(template)),
                        input(19.0, 6.0, vec![base]),
                        input(37.0, 6.0, additions),
                    ],
                    Some((61.0, 6.0)),
                );
                viewed
                    .slots
                    .push(Slot::new(Role::Output, 91.0, 6.0, results));
                out.push(viewed);
            }
        }
        let items = self.res.items();
        let enchantments = &self.res.stations().enchantments;
        let mut ids: Vec<_> = items.items.keys().collect();
        ids.sort();
        for id in &ids {
            let info = &items.items[*id];
            let item = ItemStack::new(id, 1);
            let burn = self.res.providers().burn_time(items, &item, false);
            if burn > 0 {
                let mut recipe = StationRecipe::new(
                    format!("minecraft:aei/fuel/{}", id.replace(':', "/")),
                    "minecraft:fuel",
                    vec![input(1.0, 17.0, vec![item.clone()])],
                    None,
                );
                recipe.decoration = Decoration::Fuel(burn as u32);
                recipe.text = Some(if burn == 200 {
                    "Smelts 1 item".into()
                } else {
                    let count = format!("{:.2}", f64::from(burn) / 200.0);
                    format!(
                        "Smelts {} items",
                        count.trim_end_matches('0').trim_end_matches('.')
                    )
                });
                out.push(recipe);
            }
            if let Some(provider) = &info.compostable {
                let mut recipe = StationRecipe::new(
                    format!("minecraft:aei/compost/{}", id.replace(':', "/")),
                    "minecraft:compostable",
                    vec![input(1.0, 1.0, vec![item.clone()])],
                    None,
                );
                recipe.text = Some(
                    self.res
                        .providers()
                        .positive_chance(provider)
                        .map(|chance| format!("{}% chance", (chance * 100.0).floor() as u32))
                        .unwrap_or_else(|| "Adds compost layers".into()),
                );
                out.push(recipe);
            }
            if info.max_damage > 0 {
                let mut damaged = item.clone();
                damaged.damage = info.max_damage * 3 / 4;
                let anvil = stations::anvil_result(
                    enchantments,
                    items,
                    Some(&damaged),
                    Some(&damaged),
                    None,
                    id,
                    false,
                );
                if let Some(result) = anvil.result {
                    let mut recipe = StationRecipe::new(
                        format!("minecraft:aei/anvil/repair/{}", id.replace(':', "/")),
                        "minecraft:anvil",
                        vec![
                            input(1.0, 1.0, vec![damaged.clone()]),
                            input(50.0, 1.0, vec![damaged.clone()]),
                            output(108.0, 1.0, result),
                        ],
                        Some((76.0, 1.0)),
                    );
                    recipe.costs = vec![anvil.cost];
                    out.push(recipe);
                }
                if let Some(result) =
                    stations::grindstone_result(enchantments, items, Some(&damaged), Some(&damaged))
                {
                    let mut slot = output(52.0, 13.0, result);
                    slot.background = 26;
                    out.push(StationRecipe::new(
                        format!("minecraft:aei/grindstone/repair/{}", id.replace(':', "/")),
                        "minecraft:grindstone",
                        vec![
                            input(1.0, 1.0, vec![damaged.clone()]),
                            input(1.0, 24.0, vec![damaged.clone()]),
                            slot,
                        ],
                        Some((20.0, 12.0)),
                    ));
                }
                let mut materials = Vec::new();
                let mut results = Vec::new();
                let mut costs = Vec::new();
                for material in &ids {
                    let material = ItemStack::new(material, 1);
                    if !stations::is_valid_repair_item(enchantments, items, &damaged, &material) {
                        continue;
                    }
                    let result = stations::anvil_result(
                        enchantments,
                        items,
                        Some(&damaged),
                        Some(&material),
                        None,
                        id,
                        false,
                    );
                    if let Some(result_stack) = result.result {
                        materials.push(material);
                        results.push(result_stack);
                        costs.push(result.cost);
                    }
                }
                if !results.is_empty() {
                    let mut recipe = StationRecipe::new(
                        format!("minecraft:aei/anvil/material/{}", id.replace(':', "/")),
                        "minecraft:anvil",
                        vec![
                            input(1.0, 1.0, vec![damaged]),
                            input(50.0, 1.0, materials),
                            Slot::new(Role::Output, 108.0, 1.0, results),
                        ],
                        Some((76.0, 1.0)),
                    );
                    recipe.costs = costs;
                    out.push(recipe);
                }
            }
            if enchantments.enchantable(id) == 0 && &***id != "minecraft:enchanted_book" {
                continue;
            }
            for enchantment in &enchantments.list {
                if &***id != "minecraft:enchanted_book"
                    && !enchantments.can_enchant(items, &enchantment.id, id)
                {
                    continue;
                }
                let mut bases = Vec::new();
                let mut books = Vec::new();
                let mut results = Vec::new();
                let mut costs = Vec::new();
                let mut enchanted = Vec::new();
                let mut disenchanted = Vec::new();
                for level in 1..=enchantment.max_level.min(10) {
                    let mut book = ItemStack::new("minecraft:enchanted_book", 1);
                    book.enchant(&enchantment.id, level);
                    let mut base = item.clone();
                    if &*base.item == "minecraft:enchanted_book" {
                        base.enchant(&enchantment.id, level);
                    }
                    let result = stations::anvil_result(
                        enchantments,
                        items,
                        Some(&base),
                        Some(&book),
                        None,
                        id,
                        false,
                    );
                    if let Some(result_stack) = result.result {
                        bases.push(base);
                        books.push(book);
                        results.push(result_stack);
                        costs.push(result.cost);
                    }
                    if !enchantments.is_curse(&enchantment.id) {
                        let mut stack = item.clone();
                        stack.enchant(&enchantment.id, level);
                        if let Some(result) =
                            stations::grindstone_result(enchantments, items, Some(&stack), None)
                        {
                            enchanted.push(stack);
                            disenchanted.push(result);
                        }
                    }
                }
                let suffix = format!(
                    "{}/{}",
                    id.replace(':', "/"),
                    enchantment.id.replace(':', "/")
                );
                if !results.is_empty() {
                    let mut recipe = StationRecipe::new(
                        format!("minecraft:aei/anvil/enchantment/{suffix}"),
                        "minecraft:anvil",
                        vec![
                            input(1.0, 1.0, bases),
                            input(50.0, 1.0, books),
                            Slot::new(Role::Output, 108.0, 1.0, results),
                        ],
                        Some((76.0, 1.0)),
                    );
                    recipe.costs = costs;
                    out.push(recipe);
                }
                if !disenchanted.is_empty() {
                    let mut result = Slot::new(Role::Output, 52.0, 13.0, disenchanted);
                    result.background = 26;
                    out.push(StationRecipe::new(
                        format!("minecraft:aei/grindstone/disenchant/{suffix}"),
                        "minecraft:grindstone",
                        vec![
                            input(1.0, 1.0, enchanted),
                            Slot {
                                required: false,
                                ..input(1.0, 24.0, Vec::new())
                            },
                            result,
                        ],
                        Some((20.0, 12.0)),
                    ));
                }
            }
        }
        out.into_iter()
            .map(|recipe| ViewedRecipe::Station(Arc::new(recipe)))
            .collect()
    }
    pub(super) fn aei_station_slots(
        &self,
        recipe: &StationRecipe,
        area: Rect,
    ) -> Vec<(Option<ItemStack>, Rect)> {
        let cycle = self.aei.cycles.value(&cycle::Key::station(recipe));
        recipe
            .slots
            .iter()
            .map(|slot| {
                (
                    slot.stacks
                        .get(cycle::at(cycle, slot.stacks.len()))
                        .cloned(),
                    Rect::new(area.x + 4.0 + slot.x, area.y + 4.0 + slot.y, 16.0, 16.0),
                )
            })
            .collect()
    }
    pub(super) fn draw_aei_station(
        &mut self,
        batch: &mut HudBatch,
        recipe: &StationRecipe,
        area: Rect,
    ) {
        panel(batch, &self.gui.atlas, area, false);
        let atlas = &self.gui.atlas;
        let (x, y) = (area.x + 4.0, area.y + 4.0);
        let elapsed = self.aei.recipe_started.elapsed().as_millis();
        if matches!(recipe.decoration, Decoration::Brewing) {
            batch.sprite(atlas, "horizon/aei/brewing_stand_background", x, y + 1.0);
            batch.sprite(
                atlas,
                "horizon/aei/brewing_stand_blaze_heat",
                x + 5.0,
                y + 30.0,
            );
            let arrow = ((elapsed % 20000) * 28 / 20000) as f32;
            if arrow > 0.0 {
                part(
                    batch,
                    atlas,
                    "brewing_stand_arrow",
                    Rect::new(0.0, 0.0, 7.0, arrow),
                    Rect::new(x + 43.0, y + 3.0, 7.0, arrow),
                    u32::MAX,
                );
            }
            let lengths = [29.0, 23.0, 18.0, 13.0, 9.0, 5.0, 0.0];
            let mask = lengths[((elapsed % 700) * 7 / 700) as usize];
            if mask < 28.0 {
                part(
                    batch,
                    atlas,
                    "brewing_stand_bubbles",
                    Rect::new(0.0, mask, 11.0, 28.0 - mask),
                    Rect::new(x + 9.0, y + 1.0 + mask, 11.0, 28.0 - mask),
                    u32::MAX,
                );
            }
        }
        if let Some((ax, ay)) = recipe.arrow {
            batch.sprite(atlas, "horizon/aei/recipe_arrow", x + ax, y + ay);
            if let Decoration::Fire(ticks) = recipe.decoration {
                let width =
                    ((elapsed % (u128::from(ticks) * 50)) * 23 / (u128::from(ticks) * 50)) as f32;
                if width > 0.0 {
                    part(
                        batch,
                        atlas,
                        "recipe_arrow_filled",
                        Rect::new(0.0, 0.0, width, 16.0),
                        Rect::new(x + ax, y + ay, width, 16.0),
                        u32::MAX,
                    );
                }
            }
        }
        match recipe.decoration {
            Decoration::Fire(_) | Decoration::Fuel(_) => {
                let (fy, ticks) = match recipe.decoration {
                    Decoration::Fuel(t) => (0.0, t),
                    _ => (20.0, 300),
                };
                batch.sprite(atlas, "horizon/aei/icons/flame_empty", x + 1.0, y + fy);
                let mask =
                    ((elapsed % (u128::from(ticks) * 50)) * 15 / (u128::from(ticks) * 50)) as f32;
                if mask < 14.0 {
                    part(
                        batch,
                        atlas,
                        "icons/flame",
                        Rect::new(0.0, mask, 14.0, 14.0 - mask),
                        Rect::new(x + 1.0, y + fy + mask, 14.0, 14.0 - mask),
                        u32::MAX,
                    );
                }
            }
            _ => {}
        }
        if recipe.category == "minecraft:anvil" {
            batch.sprite(atlas, "horizon/aei/recipe_plus_sign", x + 27.0, y + 3.0);
        }
        if let Some(text) = &recipe.text {
            let (tx, ty) = match recipe.decoration {
                Decoration::Fire(_) => (
                    area.x + area.w - 4.0 - HudBatch::text_width(atlas, text),
                    area.y + area.h - 12.0,
                ),
                Decoration::Brewing => (x + 70.0, y + 28.0),
                Decoration::Fuel(_) => (
                    x + 20.0 + (area.w - 28.0 - HudBatch::text_width(atlas, text)) / 2.0,
                    y + 13.0,
                ),
                _ => (
                    x + 24.0 + (area.w - 32.0 - HudBatch::text_width(atlas, text)) / 2.0,
                    y + 5.0,
                ),
            };
            batch.text(
                atlas,
                text,
                tx.round(),
                ty.round(),
                rgba(128, 128, 128, 255),
                false,
            );
        }
        if !recipe.costs.is_empty() {
            let cost = recipe.costs[cycle::at(
                self.aei.cycles.value(&cycle::Key::station(recipe)),
                recipe.costs.len(),
            )];
            let text = horizon_client::text::format_translation(
                &self.translate("container.repair.cost", "Enchantment Cost: %s"),
                &[horizon_client::text::Text::literal(cost.to_string())],
            )
            .plain();
            let color = if self.creative || (cost < 40 && self.experience.level >= cost) {
                rgba(128, 255, 32, 255)
            } else {
                rgba(255, 96, 96, 255)
            };
            batch.text(
                atlas,
                &text,
                x + 123.0 - HudBatch::text_width(atlas, &text),
                y + 27.0,
                color,
                true,
            );
        }
        for (slot, (stack, rect)) in recipe
            .slots
            .iter()
            .zip(self.aei_station_slots(recipe, area))
        {
            if slot.background > 0 {
                let border = (f32::from(slot.background) - 16.0) / 2.0;
                panel(
                    batch,
                    &self.gui.atlas,
                    Rect::new(
                        rect.x - border,
                        rect.y - border,
                        f32::from(slot.background),
                        f32::from(slot.background),
                    ),
                    true,
                );
            }
            if let Some(stack) = stack {
                self.draw_item(batch, &stack, rect.x, rect.y);
            }
            if rect.contains(self.cursor) {
                rect.fill(batch, &self.gui.atlas, rgba(255, 255, 255, 128));
            }
        }
    }
}
