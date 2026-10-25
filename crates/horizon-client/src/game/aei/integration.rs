//! JEI-style category, catalyst and container integrations for the native SDK.
use super::*;
use horizon_mods::aei as api;

#[derive(Clone)]
pub(super) enum ViewedRecipe {
    Native(Entry),
    Plugin(api::Recipe),
    Station(std::sync::Arc<super::stations::StationRecipe>),
}
impl ViewedRecipe {
    pub fn category(&self) -> &str {
        match self {
            Self::Native(entry) => RecipeKind::of(entry).id(),
            Self::Plugin(recipe) => &recipe.category,
            Self::Station(recipe) => recipe.category,
        }
    }
    pub fn id(&self) -> &str {
        match self {
            Self::Native(entry) => &entry.id,
            Self::Plugin(recipe) => &recipe.id,
            Self::Station(recipe) => &recipe.id,
        }
    }
}
impl RecipeKind {
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Crafting => "minecraft:crafting",
            Self::Smelting => "minecraft:smelting",
            Self::Blasting => "minecraft:blasting",
            Self::Smoking => "minecraft:smoking",
        }
    }
}

impl Game<'_> {
    pub(in crate::game) fn aei_container_screen(&self) -> bool {
        matches!(self.screen, Screen::Inventory | Screen::Creative)
    }
    pub(super) fn aei_definitions(&self) -> Vec<api::Category> {
        let mut categories: Vec<_> = [
            RecipeKind::Crafting,
            RecipeKind::Smelting,
            RecipeKind::Blasting,
            RecipeKind::Smoking,
        ]
        .into_iter()
        .map(|kind| api::Category {
            id: kind.id().into(),
            title: self.translate(kind.title().0, kind.title().1),
            icon: kind.icon().into(),
            width: if kind == RecipeKind::Crafting {
                116
            } else {
                82
            },
            height: 54,
        })
        .chain(self.aei_station_definitions())
        .chain(
            self.aei
                .plugins
                .iter()
                .flat_map(|p| p.categories.iter().cloned()),
        )
        .collect();
        let order = [
            "minecraft:crafting",
            "minecraft:stonecutting",
            "minecraft:smelting",
            "minecraft:smoking",
            "minecraft:blasting",
            "minecraft:campfire_cooking",
            "minecraft:smithing",
            "minecraft:compostable",
            "minecraft:fuel",
            "minecraft:brewing",
            "minecraft:anvil",
            "minecraft:grindstone",
        ];
        categories.sort_by_key(|category| {
            order
                .iter()
                .position(|id| *id == category.id)
                .unwrap_or(usize::MAX)
        });
        categories
    }
    #[cfg(feature = "mods")]
    pub(in crate::game) fn aei_replace_plugins(&mut self, plugins: Vec<api::Plugin>) {
        self.aei.plugins = plugins;
        self.aei.all_entries = None;
        self.aei.ranking = Default::default();
        // A history entry may refer to a category that no longer exists.
        if self.aei_modal() {
            self.aei_open_catalogue();
        }
    }
    pub(super) fn aei_all_entries(&mut self) -> Vec<ViewedRecipe> {
        if self.aei.all_entries.is_none() {
            let entries = self
                .aei_all_recipe_entries()
                .into_iter()
                .map(ViewedRecipe::Native)
                .chain(self.aei_station_entries())
                .chain(
                    self.aei
                        .plugins
                        .iter()
                        .flat_map(|p| p.recipes.iter().cloned().map(ViewedRecipe::Plugin)),
                )
                .collect();
            self.aei.all_entries = Some(entries);
        }
        self.aei.all_entries.clone().unwrap_or_default()
    }
    pub(super) fn aei_show_all_categories(&mut self, preserve_category: bool) {
        let selected = preserve_category
            .then(|| {
                self.aei_categories()
                    .get(self.aei.category)
                    .map(|c| c.id.clone())
            })
            .flatten();
        let entries = self.aei_all_entries();
        self.aei_show_entries(entries, AeiView::Recipes);
        if let Some(selected) = selected
            && let Some(index) = self.aei_categories().iter().position(|c| c.id == selected)
        {
            self.aei.category = index;
        }
    }
    pub(super) fn aei_category_title_area(&self, batch: &HudBatch) -> Rect {
        let nav = self.aei_recipe_layout(batch).nav(true);
        let Some(category) = self.aei_categories().get(self.aei.category).cloned() else {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        };
        let width = HudBatch::text_width(&self.gui.atlas, &category.title).min(nav.w - 54.0);
        Rect::new(
            nav.x + ((nav.w - width) / 2.0).round(),
            nav.y + 2.0,
            width,
            9.0,
        )
    }
    pub(super) fn aei_catalyst_categories(&self, item: &str) -> Vec<String> {
        let mut categories: Vec<String> = api::NATIVE_CATEGORIES
            .iter()
            .filter(|category| super::stations::catalysts(category).contains(&item))
            .map(|category| category.to_string())
            .collect();
        categories.extend(
            self.aei
                .plugins
                .iter()
                .flat_map(|p| &p.catalysts)
                .filter(|c| c.item == item)
                .flat_map(|c| c.categories.iter().cloned()),
        );
        categories
    }
    #[cfg(feature = "mods")]
    pub(super) fn aei_lookup_entries(&mut self, item: &str, uses: bool) -> Vec<ViewedRecipe> {
        self.aei_lookup_stack(&ItemStack::new(item, 1), uses)
    }
    pub(super) fn aei_lookup_stack(&mut self, focus: &ItemStack, uses: bool) -> Vec<ViewedRecipe> {
        let item = &*focus.item;
        let catalysts = if uses {
            self.aei_catalyst_categories(item)
        } else {
            Vec::new()
        };
        let native_ids: std::collections::HashSet<String> = self
            .aei_recipe_entries(item, uses)
            .iter()
            .map(|e| e.id.clone())
            .collect();
        self.aei_all_entries()
            .into_iter()
            .filter(|recipe| {
                catalysts
                    .iter()
                    .any(|category| category == recipe.category())
                    || match recipe {
                        ViewedRecipe::Native(entry) => native_ids.contains(&entry.id),
                        ViewedRecipe::Plugin(recipe) => plugin_matches(recipe, item, uses),
                        ViewedRecipe::Station(recipe) => recipe.matches(focus, uses),
                    }
            })
            .collect()
    }
    pub(super) fn aei_show_categories(&mut self, categories: &[String]) {
        let entries = self
            .aei_all_entries()
            .into_iter()
            .filter(|r| categories.iter().any(|c| c == r.category()))
            .collect();
        self.aei_show_entries(entries, AeiView::Recipes);
    }
    pub(super) fn aei_recipe_layout(&self, batch: &HudBatch) -> Recipes {
        let mut layout = Recipes::new(batch.width, batch.height);
        if let Some(category) = self.aei_categories().get(self.aei.category) {
            layout = layout.with_recipe_size(
                batch.width,
                category.width as f32 + 8.0,
                category.height as f32 + 8.0,
            );
        }
        layout
    }
    pub(super) fn aei_tabs(&self, batch: &HudBatch) -> (usize, usize, usize, Rect) {
        let layout = self.aei_recipe_layout(batch);
        let count = self.aei_categories().len();
        let per_page = ((layout.area.w - 4.0) / 24.0).floor().max(1.0) as usize;
        let pages = count.div_ceil(per_page).max(1);
        let first = self.aei.tab_page.min(pages - 1) * per_page;
        (
            first,
            (first + per_page).min(count),
            pages,
            Rect::new(
                layout.area.x + 2.0,
                layout.area.y - 43.0,
                layout.area.w - 4.0,
                20.0,
            ),
        )
    }
    pub(super) fn aei_catalysts(&self) -> Vec<String> {
        let categories = self.aei_categories();
        let Some(category) = categories.get(self.aei.category) else {
            return Vec::new();
        };
        let mut items: Vec<String> = super::stations::catalysts(&category.id)
            .iter()
            .map(|item| item.to_string())
            .collect();
        for item in self
            .aei
            .plugins
            .iter()
            .flat_map(|p| &p.catalysts)
            .filter(|c| c.categories.contains(&category.id))
            .map(|c| &c.item)
        {
            if !items.contains(item) {
                items.push(item.clone());
            }
        }
        items
    }
    fn aei_plugin_slots(&self, recipe: &api::Recipe, area: Rect) -> Vec<(Option<ItemStack>, Rect)> {
        let cycle = self
            .aei
            .cycles
            .value(&cycle::Key::Plugin(recipe.id.clone()));
        recipe
            .slots
            .iter()
            .map(|slot| {
                (
                    slot.alternatives
                        .get(cycle::at(cycle, slot.alternatives.len()))
                        .map(|stack| ItemStack::new(&stack.item, stack.count)),
                    Rect::new(
                        area.x + 4.0 + slot.x as f32,
                        area.y + 4.0 + slot.y as f32,
                        16.0,
                        16.0,
                    ),
                )
            })
            .collect()
    }
    pub(super) fn aei_viewed_slots(
        &self,
        entry: &ViewedRecipe,
        area: Rect,
    ) -> Vec<(Option<ItemStack>, Rect)> {
        match entry {
            ViewedRecipe::Native(entry) => self.aei_recipe_slots(entry, area),
            ViewedRecipe::Plugin(recipe) => self.aei_plugin_slots(recipe, area),
            ViewedRecipe::Station(recipe) => self.aei_station_slots(recipe, area),
        }
    }
    pub(super) fn draw_aei_viewed_recipe(
        &mut self,
        batch: &mut HudBatch,
        entry: &ViewedRecipe,
        area: Rect,
    ) {
        match entry {
            ViewedRecipe::Native(entry) => self.draw_aei_entry(batch, entry, area),
            ViewedRecipe::Station(recipe) => self.draw_aei_station(batch, recipe, area),
            ViewedRecipe::Plugin(recipe) => {
                panel(batch, &self.gui.atlas, area, false);
                for (stack, slot) in self.aei_plugin_slots(recipe, area) {
                    panel(
                        batch,
                        &self.gui.atlas,
                        Rect::new(slot.x - 1.0, slot.y - 1.0, 18.0, 18.0),
                        true,
                    );
                    if let Some(stack) = stack {
                        self.draw_item(batch, &stack, slot.x, slot.y);
                    }
                    if slot.contains(self.cursor) {
                        slot.fill(batch, &self.gui.atlas, rgba(255, 255, 255, 128));
                    }
                }
            }
        }
    }
    pub(super) fn aei_screen_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Some(key) = self.container {
            ids.push(self.block_name([key.0, key.1, key.2]).to_string());
        }
        if let Some(station) = &self.station {
            ids.push(self.block_name(station.pos).to_string());
            let menu = match station.kind {
                super::super::stations::Kind::Anvil => "anvil",
                super::super::stations::Kind::Stonecutter => "stonecutter",
                super::super::stations::Kind::Smithing => "smithing",
                super::super::stations::Kind::Grindstone => "grindstone",
                super::super::stations::Kind::Loom => "loom",
                super::super::stations::Kind::Cartography => "cartography_table",
                super::super::stations::Kind::Enchanting => "enchanting_table",
                super::super::stations::Kind::Merchant => "merchant",
                super::super::stations::Kind::Beacon => "beacon",
            };
            ids.push(format!("minecraft:{menu}"));
        }
        let native = if self.screen == Screen::Creative {
            "minecraft:creative"
        } else if self.container.is_none() && self.station.is_none() {
            if self.craft_size == 3 {
                "minecraft:crafting"
            } else {
                "minecraft:inventory"
            }
        } else if let Some(horizon_client::containers::BlockEntity::Furnace(furnace)) =
            self.container_kind()
        {
            match furnace.kind {
                horizon_client::containers::FurnaceKind::BlastFurnace => "minecraft:blast_furnace",
                horizon_client::containers::FurnaceKind::Smoker => "minecraft:smoker",
                _ => "minecraft:furnace",
            }
        } else {
            "minecraft:container"
        };
        ids.push(native.into());
        ids
    }
    pub(super) fn aei_click_categories(&self, batch: &HudBatch) -> Option<Vec<String>> {
        if self.aei_modal() {
            return None;
        }
        let (x, y) = self.inventory_origin(batch);
        let cursor = (self.cursor.0 - x, self.cursor.1 - y);
        let screens = self.aei_screen_ids();
        // Explicit SDK areas can override a native area's category selection.
        if let Some(area) = self
            .aei
            .plugins
            .iter()
            .flat_map(|p| &p.click_areas)
            .find(|area| {
                screens.contains(&area.screen)
                    && Rect::new(
                        area.x as f32,
                        area.y as f32,
                        area.width as f32,
                        area.height as f32,
                    )
                    .contains(cursor)
            })
        {
            return Some(area.categories.clone());
        }
        let areas: [(&[&str], Rect, &[&str]); 9] = [
            (
                &["minecraft:crafting", "minecraft:crafter"],
                Rect::new(88.0, 32.0, 28.0, 23.0),
                &["minecraft:crafting"],
            ),
            (
                &["minecraft:inventory"],
                Rect::new(137.0, 29.0, 10.0, 13.0),
                &["minecraft:crafting"],
            ),
            (
                &["minecraft:furnace"],
                Rect::new(78.0, 32.0, 28.0, 23.0),
                &["minecraft:smelting", "minecraft:fuel"],
            ),
            (
                &["minecraft:blast_furnace"],
                Rect::new(78.0, 32.0, 28.0, 23.0),
                &["minecraft:blasting", "minecraft:fuel"],
            ),
            (
                &["minecraft:smoker"],
                Rect::new(78.0, 32.0, 28.0, 23.0),
                &["minecraft:smoking", "minecraft:fuel"],
            ),
            (
                &["minecraft:brewing_stand"],
                Rect::new(97.0, 16.0, 14.0, 30.0),
                &["minecraft:brewing"],
            ),
            (
                &["minecraft:anvil"],
                Rect::new(102.0, 48.0, 22.0, 15.0),
                &["minecraft:anvil"],
            ),
            (
                &["minecraft:grindstone"],
                Rect::new(92.0, 31.0, 28.0, 21.0),
                &["minecraft:grindstone"],
            ),
            (
                &["minecraft:smithing"],
                Rect::new(68.0, 49.0, 22.0, 15.0),
                &["minecraft:smithing"],
            ),
        ];
        areas
            .iter()
            .find(|(ids, area, _)| {
                area.contains(cursor)
                    && ids
                        .iter()
                        .any(|id| screens.iter().any(|screen| screen == id))
            })
            .map(|(_, _, categories)| categories.iter().map(|id| id.to_string()).collect())
    }

    #[cfg(feature = "mods")]
    pub(in crate::game) fn aei_apply_request(&mut self, kind: api::Lookup, value: &str) {
        if !self.settings.extras.aei_enabled
            || !self.settings.extras.aei_visible
            || self.player.is_none()
            || self.book.is_some()
            || self.sign_edit.is_some()
            || !matches!(
                self.screen,
                Screen::Playing | Screen::Inventory | Screen::Creative | Screen::Chat
            )
        {
            return;
        }
        let entries = match kind {
            api::Lookup::Category => self
                .aei_all_entries()
                .into_iter()
                .filter(|r| r.category() == value)
                .collect(),
            _ => self.aei_lookup_entries(value, matches!(kind, api::Lookup::Uses)),
        };
        if entries.is_empty() {
            return;
        }
        if !self.aei_container_screen() {
            self.ui.chat_screen = None;
            self.screen = if self.creative {
                Screen::Creative
            } else {
                Screen::Inventory
            };
            self.capture(false);
        }
        self.aei_show_entries(
            entries,
            if matches!(kind, api::Lookup::Uses) {
                AeiView::Uses
            } else {
                AeiView::Recipes
            },
        );
    }
}
fn plugin_matches(recipe: &api::Recipe, item: &str, uses: bool) -> bool {
    recipe.slots.iter().any(|slot| {
        matches!(
            (uses, slot.role),
            (true, api::Role::Input) | (false, api::Role::Output)
        ) && slot.alternatives.iter().any(|stack| stack.item == item)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plugin_lookups_obey_slot_roles_and_item_alternatives() {
        let recipe = api::Recipe {
            id: "test:recipe".into(),
            category: "test:machine".into(),
            slots: vec![
                api::Slot {
                    role: api::Role::Input,
                    x: 0,
                    y: 0,
                    alternatives: vec![
                        api::Stack::new("minecraft:iron_ingot", 9),
                        api::Stack::new("minecraft:gold_ingot", 9),
                    ],
                },
                api::Slot {
                    role: api::Role::Output,
                    x: 18,
                    y: 0,
                    alternatives: vec![api::Stack::new("minecraft:iron_block", 1)],
                },
                api::Slot {
                    role: api::Role::RenderOnly,
                    x: 36,
                    y: 0,
                    alternatives: vec![api::Stack::new("minecraft:coal", 1)],
                },
            ],
        };
        assert!(plugin_matches(&recipe, "minecraft:iron_ingot", true));
        assert!(plugin_matches(&recipe, "minecraft:gold_ingot", true));
        assert!(!plugin_matches(&recipe, "minecraft:iron_ingot", false));
        assert!(plugin_matches(&recipe, "minecraft:iron_block", false));
        assert!(!plugin_matches(&recipe, "minecraft:iron_block", true));
        assert!(!plugin_matches(&recipe, "minecraft:coal", false));
        assert!(!plugin_matches(&recipe, "minecraft:coal", true));
    }
}
