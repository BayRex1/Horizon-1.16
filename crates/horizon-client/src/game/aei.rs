//! AEI (All Enough Items), Horizon's native recipe and usage lookup.
//! Layout reference: JEI 19.27's IngredientListOverlay, RecipesGui and
//! CraftingGridHelper. All widgets are drawn natively; no JEI jar is loaded.
mod index;
mod integration;
use integration::ViewedRecipe;
mod cycle;
mod layout;
mod ranking;
mod search;
mod stations;
mod transfer;
use super::{Game, Screen};
use horizon_client::aei_keybinds::{Action, Modifiers};
use horizon_client::{
    hud::{HudBatch, rgba},
    items::ItemStack,
    recipe_book::{Category, Display, Entry},
};
use layout::{
    Catalogue, Recipes, Rect, change_page, crafting_index, nav_buttons, navigation, nine_slice,
    panel, part,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum AeiView {
    #[default]
    Items,
    Recipes,
    Uses,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecipeKind {
    Crafting,
    Smelting,
    Blasting,
    Smoking,
}
impl RecipeKind {
    fn of(entry: &Entry) -> Self {
        match entry.category {
            Category::FurnaceFood | Category::FurnaceBlocks | Category::FurnaceMisc => {
                Self::Smelting
            }
            Category::BlastBlocks | Category::BlastMisc => Self::Blasting,
            Category::SmokerFood => Self::Smoking,
            _ => Self::Crafting,
        }
    }
    fn title(self) -> (&'static str, &'static str) {
        match self {
            Self::Crafting => ("container.crafting", "Crafting"),
            Self::Smelting => ("jei.category.smelting", "Smelting"),
            Self::Blasting => ("jei.category.blasting", "Blasting"),
            Self::Smoking => ("jei.category.smoking", "Smoking"),
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Crafting => "minecraft:crafting_table",
            Self::Smelting => "minecraft:furnace",
            Self::Blasting => "minecraft:blast_furnace",
            Self::Smoking => "minecraft:smoker",
        }
    }
}

#[derive(Clone, Copy)]
enum Navigation {
    Catalogue,
    Category,
    CategoryTab(usize),
    Tabs,
    AllCategories,
    CheatMode,
    CheatUnavailable,
    AllRecipes,
    Recipes,
}
#[derive(Clone, Copy)]
struct NavigationPress {
    area: Rect,
    target: Navigation,
    backwards: bool,
}

pub(super) struct AeiState {
    pub(super) open: bool,
    view: AeiView,
    entries: Vec<ViewedRecipe>,
    entry_categories: std::collections::HashSet<String>,
    category: usize,
    page: usize,
    catalogue_page: usize,
    tab_page: usize,
    tabs_for_category: usize,
    search: horizon_client::ui::EditBox,
    search_dragging: bool,
    search_error: Option<&'static str>,
    search_index: Vec<search::Document>,
    search_names: Option<std::sync::Arc<horizon_client::creative::Names>>,
    search_language: String,
    search_advanced: bool,
    catalogue: Vec<String>,
    history: Vec<(AeiView, Vec<ViewedRecipe>, usize, usize)>,
    toggle_notice: Option<horizon_client::toasts::Slide>,
    plugins: Vec<horizon_mods::aei::Plugin>,
    all_entries: Option<Vec<ViewedRecipe>>,
    ranking: ranking::ContextRanking,
    navigation_press: Option<NavigationPress>,
    recipe_started: std::time::Instant,
    transfer_press: Option<transfer::Press>,
    cycles: cycle::Cycles,
    cheat_mode: bool,
}

impl Default for AeiState {
    fn default() -> Self {
        let mut search = horizon_client::ui::EditBox::new(0.0, 0.0, 0.0, 8.0);
        search.max_length = 512;
        search.bordered = false;
        search.focused = false;
        Self {
            open: false,
            view: AeiView::Items,
            entries: Vec::new(),
            entry_categories: Default::default(),
            category: 0,
            page: 0,
            catalogue_page: 0,
            tab_page: 0,
            tabs_for_category: usize::MAX,
            search,
            search_dragging: false,
            search_error: None,
            search_index: Vec::new(),
            search_names: None,
            search_language: String::new(),
            search_advanced: false,
            catalogue: Vec::new(),
            history: Vec::new(),
            toggle_notice: None,
            plugins: Vec::new(),
            all_entries: None,
            ranking: Default::default(),
            navigation_press: None,
            recipe_started: std::time::Instant::now(),
            transfer_press: None,
            cycles: Default::default(),
            cheat_mode: false,
        }
    }
}

impl Game<'_> {
    /// The visible browser state for `horizon-ctl screen` and UI automation.
    pub(super) fn aei_screen_value(&self) -> serde_json::Value {
        let page_size = self
            .renderer
            .as_ref()
            .map(|r| {
                self.aei_catalogue_layout(&HudBatch::new([r.config.width, r.config.height]))
                    .page_size()
            })
            .unwrap_or(0);
        let geometry = self.renderer.as_ref().map(|renderer| {
            let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
            let catalogue = self.aei_catalogue_layout(&batch);
            let rect = |r: Rect| [r.x, r.y, r.w, r.h];
            let (x, y) = self.inventory_origin(&batch);
            serde_json::json!({
                "search":rect(catalogue.search),"catalogue":rect(catalogue.grid),
                "categories_button":rect(catalogue.categories),
                "container_origin":[x,y],"recipes":rect(self.aei_recipe_layout(&batch).area),
                "screens":self.aei_screen_ids(),
                "visible_recipes":self.aei_visible_recipes(&batch).iter().map(|(entry,area)|serde_json::json!({
                    "id":entry.id(),"transfer":self.aei_transfer_value(entry,*area),"slots":self.aei_viewed_slots(entry,*area).iter().map(|(stack,area)|serde_json::json!({"stack":stack.as_ref().map(|stack|self.stack_value(stack)),"area":rect(*area)})).collect::<Vec<_>>()
                })).collect::<Vec<_>>(),
            })
        });
        serde_json::json!({
            "visible":self.aei.open,
            "layout":geometry,
            "registered_categories":self.aei_definitions().iter().map(|c|&c.id).collect::<Vec<_>>(),
            "view":match self.aei.view { AeiView::Items=>"items", AeiView::Recipes=>"recipes", AeiView::Uses=>"uses" },
            "history_depth":self.aei.history.len(),
            "recipe_category":self.aei_categories().get(self.aei.category).map(|k|&k.title),
            "category_id":self.aei_categories().get(self.aei.category).map(|k|&k.id),
            "categories":self.aei_categories().iter().map(|k|&k.id).collect::<Vec<_>>(),
            "recipe_ids":self.aei_category_entries().iter().map(|r| r.id()).collect::<Vec<_>>(),
            "read_only":self.spectator(),
            "context_categories":self.aei.ranking.categories,
            "craftable_recipes":self.aei_category_entries().iter().filter(|r|self.aei.ranking.availability(r).craftable()).map(|r|r.id()).collect::<Vec<_>>(),
            "recipe_page":self.aei.page+1,
            "cycle_paused":self.aei_cycle_paused(),
            "cheat_mode":self.aei_cheat_mode(),
            "cheat_allowed":self.aei_cheat_available(),
            "enabled":self.settings.extras.aei_enabled,
            "recipe_book_enabled":self.settings.extras.recipe_book_enabled,
            "keybinds":horizon_client::aei_keybinds::MAPPINGS.iter().map(|m| (m.1,self.settings.extras.aei_keys.get(m.1))).collect::<std::collections::BTreeMap<_,_>>(),
            "query":self.aei.search.value,
            "focused":self.aei_search_focused(),
            "matches":self.aei.catalogue.len(),
            "items":self.aei.catalogue.iter().skip(self.aei.catalogue_page*page_size).take(page_size).collect::<Vec<_>>(),
            "error":self.aei.search_error,
        })
    }
    pub(super) fn aei_sync_visibility(&mut self) {
        if !self.aei_cheat_available() {
            self.aei.cheat_mode = false;
        }
        if !self.settings.extras.aei_enabled || !self.settings.extras.aei_visible {
            self.aei_close();
            if !self.settings.extras.aei_enabled {
                self.aei.toggle_notice = None;
            }
        } else if self.aei_container_screen() && !self.aei.open {
            self.aei_open_catalogue();
        }
    }

    pub(super) fn aei_modifiers(&self) -> Modifiers {
        use winit::keyboard::KeyCode as K;
        Modifiers {
            control: self.has_control_down(),
            shift: self.has_shift_down(),
            alt: [K::AltLeft, K::AltRight]
                .iter()
                .any(|key| self.physical_keys.contains(key)),
            super_key: [K::SuperLeft, K::SuperRight]
                .iter()
                .any(|key| self.physical_keys.contains(key)),
        }
    }
    fn aei_cheat_available(&self) -> bool {
        if let Some(remote) = &self.remote {
            return remote.commands.as_ref().is_some_and(|dispatcher| {
                dispatcher.nodes.first().is_some_and(|root| {
                    root.children.iter().any(|&child| {
                        dispatcher.nodes.get(child).is_some_and(|node| {
                            matches!(
                                &node.kind,
                                horizon_client::command::NodeKind::Literal(name) if name == "give"
                            )
                        })
                    })
                })
            });
        }
        self.allow_commands()
    }
    fn aei_cheat_mode(&self) -> bool {
        self.aei.cheat_mode && self.aei_cheat_available()
    }
    fn aei_give_stack(&mut self, stack: &ItemStack) {
        if !self.aei_cheat_mode() {
            return;
        }
        let count = self
            .res
            .items()
            .get(&stack.item)
            .map_or(64, |item| item.max_stack);
        let components = match horizon_client::item_data::encode_components(stack) {
            Some(horizon_world::nbt::Tag::Compound(entries)) if !entries.is_empty() => {
                let values = entries
                    .iter()
                    .map(|(key, value)| {
                        format!(
                            "{}={}",
                            key.to_string_lossy(),
                            horizon_client::snbt::to_snbt(value)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("[{values}]")
            }
            _ => String::new(),
        };
        let command = format!("give @s {}{components} {count}", stack.item);
        if self.remote.is_some() {
            self.send_chat(&format!("/{command}"));
        } else {
            self.run_command(&command);
        }
    }
    pub(super) fn aei_shortcut_action(&self, input: &str) -> Option<Action> {
        if !self.settings.extras.aei_enabled
            || self.title.is_some()
            || self.sign_edit.is_some()
            || self.book.is_some()
            || !matches!(
                self.screen,
                Screen::Playing | Screen::Inventory | Screen::Creative
            )
        {
            return None;
        }
        let action = self
            .settings
            .extras
            .aei_keys
            .action(input, self.aei_modifiers())
            .or_else(|| {
                self.aei_cycle_paused()
                    .then(|| {
                        self.settings
                            .extras
                            .aei_keys
                            .cycle_action(input, self.aei_modifiers())
                    })
                    .flatten()
            })?;
        if action == Action::Toggle {
            return Some(action);
        }
        if !self.aei_container_screen() || !self.aei.open || !self.settings.extras.aei_visible {
            return None;
        }
        if matches!(
            action,
            Action::PreviousAlternative | Action::NextAlternative
        ) && !self.aei_cycle_available()
        {
            return None;
        }
        if action == Action::FocusSearch {
            return Some(action);
        }
        if self.aei_search_focused() || self.recipe_search_focused() && !self.aei_modal() {
            return None;
        }
        if !self.aei_modal()
            && (self.station_typing()
                || self.screen == Screen::Creative && self.creative_screen.tab().search)
            && !self.renderer.as_ref().is_some_and(|r| {
                self.aei_hovered(&HudBatch::new([r.config.width, r.config.height]))
                    .is_some()
            })
        {
            return None;
        }
        if matches!(action, Action::Recipes | Action::Uses) || self.aei_modal() {
            Some(action)
        } else {
            None
        }
    }
    pub(super) fn aei_shortcut(&mut self, input: &str) -> bool {
        let Some(action) = self.aei_shortcut_action(input) else {
            return false;
        };
        match action {
            Action::Toggle => self.aei_toggle(),
            Action::Recipes | Action::Uses => self.aei_open_hovered(action == Action::Uses),
            Action::Back => self.aei_back(),
            Action::PreviousAlternative => self.aei_cycle_step(-1),
            Action::NextAlternative => self.aei_cycle_step(1),
            Action::PauseCycle => {}
            Action::FocusSearch => {
                self.aei.search.focused = true;
                self.aei.search.reset_blink();
            }
            action => self.aei_page_action(action),
        }
        true
    }
    fn aei_toggle(&mut self) {
        if self.settings.extras.toggle_aei() {
            self.aei.toggle_notice = Some(Default::default());
        }
        self.aei_close();
        if self.aei_container_screen() {
            self.aei_sync_visibility();
        }
        if let Some(dir) = Self::options_path().parent() {
            self.settings.extras.save(dir);
        }
    }

    /// The first toggle explains how to bring the browser back, even while hidden.
    pub(super) fn draw_aei_notice(&mut self, batch: &mut HudBatch) -> bool {
        let Some(slide) = self.aei.toggle_notice.as_mut() else {
            return false;
        };
        let duration = 8000.0 * f64::from(self.settings.notification_display_time);
        let (sound, finished) = slide.update_now(|fully| (fully as f64) < duration);
        let portion = slide.portion;
        if finished {
            self.aei.toggle_notice = None;
            return false;
        }
        if let Some(show) = sound {
            self.audio.play(
                if show {
                    "minecraft:ui.toast.in"
                } else {
                    "minecraft:ui.toast.out"
                },
                None,
                1.0,
                1.0,
            );
        }
        let shortcut = horizon_client::aei_keybinds::display(
            self.settings.extras.aei_keys.get("horizon.aei.toggle"),
        )
        .plain();
        let (title, message) = if self.settings.extras.aei_visible {
            ("AEI visible", format!("Press {shortcut} to hide AEI."))
        } else {
            ("AEI hidden", format!("Press {shortcut} to show AEI."))
        };
        let atlas = &self.gui.atlas;
        let width = HudBatch::text_width(atlas, title)
            .max(HudBatch::text_width(atlas, &message))
            .ceil()
            .max(160.0)
            + 30.0;
        let x = batch.width - width * portion;
        horizon_client::toasts::background(atlas, batch, x, 0.0, width);
        batch.text(atlas, title, x + 18.0, 7.0, rgba(255, 255, 0, 255), false);
        batch.text(atlas, &message, x + 18.0, 19.0, u32::MAX, false);
        true
    }

    fn aei_catalogue_layout(&self, batch: &HudBatch) -> Catalogue {
        let right = if self.aei_modal() {
            let area = self.aei_recipe_layout(batch).area;
            area.x + area.w
        } else {
            let (shift, width) = if self.screen == Screen::Creative {
                (0.0, horizon_client::creative::Creative::WIDTH)
            } else {
                match self.station.as_ref().map(|s| s.kind) {
                    Some(super::stations::Kind::Merchant) => (-50.0, 276.0),
                    Some(super::stations::Kind::Beacon) => (super::beacon::SHIFT, 230.0),
                    _ => (0.0, 176.0),
                }
            };
            self.inventory_origin(batch).0 + shift + width
        };
        Catalogue::new(batch.width, batch.height, right)
    }
    pub(super) fn aei_modal(&self) -> bool {
        self.aei.open && self.aei.view != AeiView::Items
    }

    /// R/U also work over the browser and recipe ingredients.
    pub(super) fn aei_open_hovered(&mut self, uses: bool) {
        if !self.aei_container_screen()
            || !self.settings.extras.aei_enabled
            || !self.settings.extras.aei_visible
        {
            return;
        }
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return;
        };
        let batch = HudBatch::new(size);
        let item = self.aei_hovered(&batch).or_else(|| {
            if self.aei_modal() {
                return None;
            }
            self.slot_at(&batch, self.cursor.0, self.cursor.1)
                .and_then(|slot| {
                    if self.creative_tab_open()
                        && slot >= horizon_client::creative::Creative::DISPLAY
                    {
                        let first = self.creative_screen.first_row(&self.pack.names);
                        self.creative_screen
                            .items(&self.pack.names)
                            .get(first * 9 + slot - horizon_client::creative::Creative::DISPLAY)
                            .cloned()
                    } else {
                        self.slot_view(slot)
                    }
                })
        });
        if let Some(stack) = item {
            self.aei_show_stack(&stack, uses);
        }
    }
    fn aei_show_stack(&mut self, stack: &ItemStack, uses: bool) {
        let entries = self.aei_lookup_stack(stack, uses);
        self.aei_show_entries(
            entries,
            if uses {
                AeiView::Uses
            } else {
                AeiView::Recipes
            },
        );
    }
    fn aei_show_entries(&mut self, entries: Vec<ViewedRecipe>, view: AeiView) {
        self.aei_refresh_context();
        if entries.is_empty() {
            return;
        }
        if !self.aei_modal() {
            self.aei.cycles.reset(self.started.elapsed());
        }
        if self.aei_modal() {
            self.aei.history.push((
                self.aei.view,
                self.aei.entries.clone(),
                self.aei.category,
                self.aei.page,
            ));
        }
        self.aei.entries = entries;
        self.aei.entry_categories = self
            .aei
            .entries
            .iter()
            .map(|entry| entry.category().to_string())
            .collect();
        self.aei.recipe_started = std::time::Instant::now();
        self.aei.view = view;
        self.aei.navigation_press = None;
        self.aei.category = 0;
        self.aei.page = 0;
        self.aei.tab_page = 0;
        self.aei.tabs_for_category = usize::MAX;
        self.aei.search.focused = false;
        self.aei.open = true;
        if self.aei.catalogue.is_empty() {
            self.aei_refresh_catalogue();
        }
    }
    pub(super) fn aei_open_catalogue(&mut self) {
        self.aei.cycles.reset(self.started.elapsed());
        self.aei.view = AeiView::Items;
        self.aei.navigation_press = None;
        self.aei.entries.clear();
        self.aei.entry_categories.clear();
        self.aei.history.clear();
        self.aei.search.focused = false;
        self.aei.open = true;
        self.aei_refresh_catalogue();
    }
    fn aei_refresh_catalogue(&mut self) {
        self.aei_ensure_search_index();
        match search::Query::parse(&self.aei.search.value) {
            Ok(query) => {
                self.aei.search_error = None;
                self.aei.catalogue = self
                    .aei
                    .search_index
                    .iter()
                    .filter(|document| query.matches(document))
                    .map(|document| document.id.clone())
                    .collect();
                self.aei_sort_catalogue();
            }
            Err(error) => {
                self.aei.search_error = Some(error);
                self.aei.catalogue.clear();
            }
        }
    }
    pub(super) fn aei_search_focused(&self) -> bool {
        self.aei.open && self.aei.search.focused
    }
    pub(super) fn aei_search_blur(&mut self) {
        self.aei.search.focused = false;
        self.aei.search_dragging = false;
    }
    fn aei_search_changed(&mut self) {
        self.aei.catalogue_page = 0;
        self.aei_refresh_catalogue();
    }
    pub(super) fn aei_search_text(&mut self, text: &str) {
        if text == "\\b" {
            self.aei.search.delete_chars(-1);
        } else {
            self.aei.search.insert_text(text);
        }
        self.aei.search.reset_blink();
        self.aei_search_changed();
    }
    fn aei_search_shortcut(&mut self, shortcut: horizon_client::clipboard::EditShortcut) {
        if self.aei.search.shortcut(shortcut, &mut self.clipboard) {
            self.aei_search_changed();
        }
    }
    /// Shared by native keyboard events and the control socket.
    pub(super) fn aei_search_key_code(&mut self, key: winit::keyboard::KeyCode) -> bool {
        use horizon_client::clipboard::edit_shortcut;
        use winit::keyboard::KeyCode as K;
        if key == K::Escape {
            if self.aei_modal() {
                self.aei_open_catalogue();
            } else {
                self.aei_search_blur();
            }
            return true;
        }
        if matches!(key, K::Enter | K::NumpadEnter) {
            self.aei_search_blur();
            return true;
        }
        let ctrl = self.has_control_down();
        let shift = self.has_shift_down();
        let held = |keys: [K; 2]| keys.iter().any(|k| self.physical_keys.contains(k));
        let edit = if cfg!(target_os = "macos") {
            held([K::SuperLeft, K::SuperRight])
        } else {
            ctrl
        };
        let letter = match key {
            K::KeyA => Some('a'),
            K::KeyC => Some('c'),
            K::KeyV => Some('v'),
            K::KeyX => Some('x'),
            _ => None,
        };
        if let Some(shortcut) = edit_shortcut(letter, edit, shift, held([K::AltLeft, K::AltRight]))
        {
            self.aei_search_shortcut(shortcut);
            return true;
        }
        let before = self.aei.search.value.clone();
        let handled = self.aei.search.key(key, ctrl, shift);
        self.aei.search.reset_blink();
        if self.aei.search.value != before {
            self.aei_search_changed();
        }
        handled
    }
    pub(super) fn aei_search_key(&mut self, event: &winit::event::KeyEvent) {
        use winit::{event::ElementState, keyboard::PhysicalKey};
        if event.state != ElementState::Pressed {
            return;
        }
        if let Some(shortcut) = self.edit_shortcut(event) {
            self.aei_search_shortcut(shortcut);
            return;
        }
        let handled = match event.physical_key {
            PhysicalKey::Code(key) => self.aei_search_key_code(key),
            _ => false,
        };
        if !handled
            && !self.has_control_down()
            && !self.modifiers.super_key()
            && let Some(text) = event.text.as_deref()
        {
            self.aei_search_text(text);
        }
    }
    pub(super) fn aei_search_drag(&mut self) {
        if self.aei.search_dragging && self.aei_search_focused() {
            self.aei.search.drag_to(&self.gui.atlas, self.cursor.0);
        }
    }
    pub(super) fn aei_search_release(&mut self) {
        self.aei.search_dragging = false;
    }
    /// IconButton's EXECUTE phase runs on left release, not its SIMULATE press.
    pub(super) fn aei_navigation_release(&mut self, batch: &HudBatch, right: bool) -> bool {
        if self.aei_transfer_release(batch, right) {
            return true;
        }
        if right {
            return false;
        }
        let Some(press) = self.aei.navigation_press.take() else {
            return false;
        };
        if press.area.contains(self.cursor) {
            match press.target {
                Navigation::AllCategories => self.aei_show_all_categories(false),
                Navigation::CheatMode => {
                    if self.aei_cheat_available() {
                        self.aei.cheat_mode = !self.aei.cheat_mode;
                    } else {
                        self.aei.cheat_mode = false;
                    }
                }
                Navigation::CheatUnavailable => {}
                Navigation::AllRecipes => self.aei_show_all_categories(true),
                Navigation::Catalogue => {
                    let pages = self
                        .aei_catalogue_layout(batch)
                        .pages(self.aei.catalogue.len());
                    self.aei.catalogue_page =
                        change_page(self.aei.catalogue_page, pages, press.backwards);
                }
                Navigation::Category => {
                    let pages = self.aei_categories().len();
                    self.aei.category = change_page(self.aei.category, pages, press.backwards);
                    self.aei.page = 0;
                }
                Navigation::Tabs => {
                    let (_, _, pages, _) = self.aei_tabs(batch);
                    self.aei.tab_page = change_page(self.aei.tab_page, pages, press.backwards);
                }
                Navigation::CategoryTab(category) => {
                    if self.aei.category != category {
                        self.aei.category = category;
                        self.aei.page = 0;
                    }
                }
                Navigation::Recipes => {
                    let pages = self
                        .aei_category_entries()
                        .len()
                        .div_ceil(self.aei_recipe_layout(batch).per_page)
                        .max(1);
                    self.aei.page = change_page(self.aei.page, pages, press.backwards);
                }
            }
            self.audio
                .play("minecraft:ui.button.click", None, 0.25, 1.0);
        }
        true
    }
    fn aei_page_action(&mut self, action: Action) {
        let backwards = matches!(action, Action::PreviousPage | Action::PreviousCategory);
        if matches!(action, Action::PreviousCategory | Action::NextCategory) {
            self.aei.category =
                change_page(self.aei.category, self.aei_categories().len(), backwards);
            self.aei.page = 0;
        } else if let Some(renderer) = &self.renderer {
            let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
            let pages = self
                .aei_category_entries()
                .len()
                .div_ceil(self.aei_recipe_layout(&batch).per_page)
                .max(1);
            self.aei.page = change_page(self.aei.page, pages, backwards);
        }
    }
    pub(super) fn aei_back(&mut self) {
        if let Some((view, entries, category, page)) = self.aei.history.pop() {
            self.aei.view = view;
            self.aei.entries = entries;
            self.aei.entry_categories = self
                .aei
                .entries
                .iter()
                .map(|entry| entry.category().to_string())
                .collect();
            self.aei.category = category;
            self.aei.page = page;
        }
    }
    pub(super) fn aei_close(&mut self) {
        self.aei.open = false;
        self.aei.cycles.reset(self.started.elapsed());
        self.aei.transfer_press = None;
        self.aei.navigation_press = None;
        self.aei.entries.clear();
        self.aei.entry_categories.clear();
        self.aei.history.clear();
        self.aei.search.focused = false;
    }
    fn aei_categories(&self) -> Vec<horizon_mods::aei::Category> {
        let mut categories: Vec<_> = self
            .aei_definitions()
            .into_iter()
            .filter(|category| self.aei.entry_categories.contains(&category.id))
            .collect();
        categories.sort_by_key(|category| !self.aei.ranking.categories.contains(&category.id));
        categories
    }
    fn aei_category_entries(&self) -> Vec<&ViewedRecipe> {
        let kinds = self.aei_categories();
        let category = kinds.get(self.aei.category).map(|kind| kind.id.as_str());
        let mut entries: Vec<_> = self
            .aei
            .entries
            .iter()
            .filter(|entry| Some(entry.category()) == category)
            .collect();
        if category.is_some_and(|id| self.aei.ranking.categories.iter().any(|c| c == id)) {
            entries.sort_by_cached_key(|entry| {
                std::cmp::Reverse(self.aei.ranking.availability(entry).priority())
            });
        }
        entries
    }
    fn aei_visible_recipes(&self, batch: &HudBatch) -> Vec<(ViewedRecipe, Rect)> {
        let layout = self.aei_recipe_layout(batch);
        let entries = self.aei_category_entries();
        let pages = entries.len().div_ceil(layout.per_page).max(1);
        entries
            .into_iter()
            .skip(self.aei.page.min(pages - 1) * layout.per_page)
            .take(layout.per_page)
            .enumerate()
            .map(|(i, entry)| (entry.clone(), layout.recipe(i)))
            .collect()
    }
    /// A shared stack/position list keeps drawing, tooltips and clicks aligned.
    fn aei_recipe_slots(&self, entry: &Entry, r: Rect) -> Vec<(Option<ItemStack>, Rect)> {
        let cycle = self.aei.cycles.value(&cycle::Key::native(entry));
        let mut slots = Vec::new();
        let cooking = matches!(entry.display, Display::Furnace(_));
        let cells = match &entry.display {
            Display::Shaped {
                width,
                height,
                cells,
            } => {
                let mut padded = vec![None; 9];
                for (i, cell) in cells.iter().enumerate() {
                    if let Some(dst) = padded.get_mut(crafting_index(i, *width, *height)) {
                        *dst = cell.as_ref();
                    }
                }
                padded
            }
            Display::Shapeless(items) => {
                let size = if items.len() > 4 {
                    3
                } else if items.len() > 1 {
                    2
                } else {
                    1
                };
                let mut padded = vec![None; 9];
                for (i, item) in items.iter().enumerate() {
                    if let Some(dst) = padded.get_mut(crafting_index(i, size, size)) {
                        *dst = Some(item);
                    }
                }
                padded
            }
            Display::Furnace(item) => vec![Some(item)],
        };
        let mut ingredient_index = 0;
        for (i, ingredient) in cells.into_iter().enumerate() {
            let stack = ingredient.and_then(|ingredient| {
                let index = ingredient_index;
                ingredient_index += 1;
                if let Some((_, variants)) = entry.stacks.iter().find(|(n, _)| *n == index) {
                    return variants.get(cycle::at(cycle, variants.len())).cloned();
                }
                ingredient
                    .1
                    .get(cycle::at(cycle, ingredient.1.len()))
                    .map(|id| ItemStack::new(id, 1))
            });
            let (dx, dy) = if cooking {
                (5.0, 5.0)
            } else {
                (5.0 + (i % 3) as f32 * 18.0, 5.0 + (i / 3) as f32 * 18.0)
            };
            slots.push((stack, Rect::new(r.x + dx, r.y + dy, 16.0, 16.0)));
        }
        let variants = entry.results();
        slots.push((
            Some(variants[cycle::at(cycle, variants.len())].clone()),
            Rect::new(
                r.x + if cooking { 65.0 } else { 99.0 },
                r.y + 23.0,
                16.0,
                16.0,
            ),
        ));
        slots
    }
    pub(super) fn aei_hovered(&self, batch: &HudBatch) -> Option<ItemStack> {
        if !self.aei.open {
            return None;
        }
        let c = self.aei_catalogue_layout(batch);
        let page = self
            .aei
            .catalogue_page
            .min(c.pages(self.aei.catalogue.len()) - 1);
        if let Some(id) = c
            .item_at(self.cursor, page)
            .and_then(|i| self.aei.catalogue.get(i))
        {
            return Some(ItemStack::new(id, 1));
        }
        if self.aei_modal() {
            let area = self.aei_recipe_layout(batch).area;
            for (i, item) in self.aei_catalysts().iter().enumerate() {
                if Rect::new(area.x - 16.0, area.y + 6.0 + i as f32 * 18.0, 16.0, 16.0)
                    .contains(self.cursor)
                {
                    return Some(ItemStack::new(item, 1));
                }
            }
            for (entry, r) in self.aei_visible_recipes(batch) {
                for (stack, slot) in self.aei_viewed_slots(&entry, r) {
                    if slot.contains(self.cursor) {
                        return stack;
                    }
                }
            }
        }
        None
    }
    pub(super) fn draw_aei_panel(&mut self, batch: &mut HudBatch) {
        if !self.aei.open || !self.aei_container_screen() {
            return;
        }
        self.aei_refresh_context();
        if self.aei_search_index_stale() {
            self.aei_refresh_catalogue();
        }
        if self.aei_modal() {
            self.draw_aei_recipes(batch);
        }
        self.draw_aei_catalogue(batch);
        if self.aei_transfer_tooltip(batch) {
            return;
        }
        if let Some(stack) = self.aei_hovered(batch) {
            let mut lines = self.stack_tooltip(&stack);
            if let Some(hint) = self.aei_cycle_hint(batch) {
                lines.extend(
                    hint.into_iter()
                        .map(|(text, color)| (horizon_client::text::Text::literal(text), color)),
                );
            }
            self.draw_tooltip_lines(batch, &lines, self.cursor.0, self.cursor.1);
        } else if self
            .aei_catalogue_layout(batch)
            .categories
            .contains(self.cursor)
        {
            let mut lines = vec![horizon_client::text::Text::literal(
                "Show all recipe categories",
            )];
            lines.push(horizon_client::text::Text::literal(
                if self.aei_cheat_mode() {
                    "Ctrl-click to turn cheat mode off"
                } else if self.aei_cheat_available() {
                    "Ctrl-click to turn cheat mode on"
                } else {
                    "Cheat mode requires world cheats"
                },
            ));
            horizon_client::ui::tooltip(batch, &self.gui.atlas, &lines, self.cursor);
        } else if self.aei_modal() && self.aei_category_title_area(batch).contains(self.cursor) {
            self.tooltip(batch, "Show all recipes", self.cursor.0, self.cursor.1);
        } else if let Some(categories) = self.aei_click_categories(batch) {
            let mut lines = vec![horizon_client::text::Text::literal("Show recipes")];
            lines.extend(
                self.aei_definitions()
                    .iter()
                    .filter(|c| categories.contains(&c.id))
                    .map(|c| horizon_client::text::Text::literal(&c.title)),
            );
            horizon_client::ui::tooltip(batch, &self.gui.atlas, &lines, self.cursor);
        } else if self
            .aei_catalogue_layout(batch)
            .search
            .contains(self.cursor)
            && let Some(error) = self.aei.search_error
        {
            horizon_client::ui::tooltip(
                batch,
                &self.gui.atlas,
                &[horizon_client::text::Text::literal(error)],
                self.cursor,
            );
        }
    }
    fn draw_aei_catalogue(&mut self, batch: &mut HudBatch) {
        let c = self.aei_catalogue_layout(batch);
        if c.page_size() == 0 {
            return;
        }
        let pages = c.pages(self.aei.catalogue.len());
        self.aei.catalogue_page = self.aei.catalogue_page.min(pages - 1);
        navigation(
            batch,
            &self.gui.atlas,
            c.navigation,
            &format!("{}/{}", self.aei.catalogue_page + 1, pages),
            true,
            self.cursor,
            self.aei.navigation_press.map(|p| p.area),
        );
        let first = self.aei.catalogue_page * c.page_size();
        let items = self
            .aei
            .catalogue
            .iter()
            .skip(first)
            .take(c.page_size())
            .cloned()
            .collect::<Vec<_>>();
        for (i, id) in items.iter().enumerate() {
            let slot = c.slot(i);
            self.draw_item(batch, &ItemStack::new(id, 1), slot.x + 1.0, slot.y + 1.0);
            if slot.contains(self.cursor) {
                slot.fill(batch, &self.gui.atlas, rgba(255, 255, 255, 128));
            }
        }
        let button = c.categories;
        nine_slice(
            batch,
            &self.gui.atlas,
            if self.aei.navigation_press.is_some_and(|p| p.area == button) {
                if button.contains(self.cursor) {
                    "button_pressed_highlight"
                } else {
                    "button_pressed"
                }
            } else if button.contains(self.cursor) {
                "button_highlight"
            } else {
                "button_enabled"
            },
            button,
            [6.0; 4],
        );
        if self.aei_cheat_mode() {
            button.fill(batch, &self.gui.atlas, rgba(255, 0, 0, 170));
        }
        self.draw_item(
            batch,
            &ItemStack::new("minecraft:crafting_table", 1),
            button.x + 2.0,
            button.y + 2.0,
        );
        let atlas = &self.gui.atlas;
        let border = if self.aei.search.focused { 255 } else { 96 };
        c.search
            .fill(batch, atlas, rgba(border, border, border, 255));
        Rect::new(
            c.search.x + 1.0,
            c.search.y + 1.0,
            c.search.w - 2.0,
            c.search.h - 2.0,
        )
        .fill(batch, atlas, rgba(0, 0, 0, 255));
        let edit = &mut self.aei.search;
        edit.x = c.search.x + 4.0;
        edit.y = c.search.y + 6.0;
        edit.width = (c.search.w - 14.0).max(1.0);
        edit.height = 8.0;
        edit.text_color = if self.aei.catalogue.is_empty() {
            rgba(255, 85, 85, 255)
        } else {
            rgba(224, 224, 224, 255)
        };
        edit.draw(batch, atlas, None);
    }
    fn draw_aei_recipes(&mut self, batch: &mut HudBatch) {
        let layout = self.aei_recipe_layout(batch);
        let kinds = self.aei_categories();
        let tabs_per_page = ((layout.area.w - 4.0) / 24.0).floor().max(1.0) as usize;
        if self.aei.tabs_for_category != self.aei.category {
            self.aei.tab_page = self.aei.category / tabs_per_page;
            self.aei.tabs_for_category = self.aei.category;
        }
        let (first_tab, last_tab, tab_pages, tab_navigation) = self.aei_tabs(batch);
        let atlas = &self.gui.atlas;
        nine_slice(batch, atlas, "gui_background", layout.area, [16.0; 4]);
        if tab_pages > 1 {
            navigation(
                batch,
                atlas,
                tab_navigation,
                &format!("{}/{}", self.aei.tab_page + 1, tab_pages),
                true,
                self.cursor,
                self.aei.navigation_press.map(|p| p.area),
            );
        }
        for i in first_tab..last_tab {
            let tab = layout.tab(i - first_tab);
            let sprite = if i == self.aei.category {
                "tab_selected"
            } else {
                "tab_unselected"
            };
            batch.sprite(atlas, &format!("horizon/aei/{sprite}"), tab.x, tab.y);
        }
        let Some(kind) = kinds.get(self.aei.category).cloned() else {
            return;
        };
        let title = kind.title.clone();
        let pressed = self.aei.navigation_press.map(|p| p.area);
        navigation(
            batch,
            atlas,
            layout.nav(true),
            &title,
            kinds.len() > 1,
            self.cursor,
            pressed,
        );
        let count = self.aei_category_entries().len();
        let pages = count.div_ceil(layout.per_page).max(1);
        self.aei.page = self.aei.page.min(pages - 1);
        navigation(
            batch,
            atlas,
            layout.nav(false),
            &format!("{}/{}", self.aei.page + 1, pages),
            pages > 1,
            self.cursor,
            pressed,
        );
        for (i, kind) in kinds.iter().enumerate().take(last_tab).skip(first_tab) {
            let tab = layout.tab(i - first_tab);
            if kind.id == "minecraft:fuel" {
                batch.sprite(
                    &self.gui.atlas,
                    "horizon/aei/icons/flame",
                    tab.x + 5.0,
                    tab.y + 5.0,
                );
            } else {
                self.draw_item(
                    batch,
                    &ItemStack::new(&kind.icon, 1),
                    tab.x + 4.0,
                    tab.y + 4.0,
                );
            }
        }
        let catalysts = self.aei_catalysts();
        if !catalysts.is_empty() {
            let catalyst = Rect::new(
                layout.area.x - 22.0,
                layout.area.y,
                28.0,
                10.0 + catalysts.len() as f32 * 18.0,
            );
            nine_slice(
                batch,
                &self.gui.atlas,
                "catalyst_tab",
                catalyst,
                [8.0, 9.0, 8.0, 8.0],
            );
            for (i, item) in catalysts.iter().enumerate() {
                let y = catalyst.y + 5.0 + i as f32 * 18.0;
                nine_slice(
                    batch,
                    &self.gui.atlas,
                    "recipe_catalyst_slot_background",
                    Rect::new(catalyst.x + 5.0, y, 18.0, 18.0),
                    [4.0; 4],
                );
                self.draw_item(batch, &ItemStack::new(item, 1), catalyst.x + 6.0, y + 1.0);
                let slot = Rect::new(catalyst.x + 6.0, y + 1.0, 16.0, 16.0);
                if slot.contains(self.cursor) {
                    slot.fill(batch, &self.gui.atlas, rgba(255, 255, 255, 128));
                }
            }
        }
        for (entry, r) in self.aei_visible_recipes(batch) {
            self.draw_aei_viewed_recipe(batch, &entry, r);
            self.draw_aei_transfer(batch, &entry, r);
        }
    }
    fn draw_aei_entry(&mut self, batch: &mut HudBatch, entry: &Entry, r: Rect) {
        panel(batch, &self.gui.atlas, r, false);
        let slots = self.aei_recipe_slots(entry, r);
        for (i, (stack, slot)) in slots.iter().enumerate() {
            let border = if i + 1 == slots.len() { 5.0 } else { 1.0 };
            panel(
                batch,
                &self.gui.atlas,
                Rect::new(
                    slot.x - border,
                    slot.y - border,
                    16.0 + 2.0 * border,
                    16.0 + 2.0 * border,
                ),
                true,
            );
            if let Some(stack) = stack {
                self.draw_item(batch, stack, slot.x, slot.y);
            }
            if slot.contains(self.cursor) {
                slot.fill(batch, &self.gui.atlas, rgba(255, 255, 255, 128));
            }
        }
        let cooking = matches!(entry.display, Display::Furnace(_));
        batch.sprite(
            &self.gui.atlas,
            "horizon/aei/recipe_arrow",
            r.x + if cooking { 30.0 } else { 65.0 },
            r.y + if cooking { 21.0 } else { 23.0 },
        );
        if cooking {
            panel(
                batch,
                &self.gui.atlas,
                Rect::new(r.x + 4.0, r.y + 40.0, 18.0, 18.0),
                true,
            );
            let recipe = self.res.recipes().cooking.iter().find(|recipe| {
                recipe.id == entry.id || entry.id.strip_prefix("minecraft:") == Some(&recipe.id)
            });
            let time = recipe
                .map(|recipe| recipe.time)
                .filter(|time| *time > 0)
                .unwrap_or(if RecipeKind::of(entry) == RecipeKind::Smelting {
                    200
                } else {
                    100
                });
            let elapsed = self.aei.recipe_started.elapsed().as_millis();
            // JEI TickTimer includes both empty and full frames (maxValue + 1).
            let filled =
                ((elapsed % (u128::from(time) * 50)) * 23 / (u128::from(time) * 50)) as f32;
            if filled > 0.0 {
                part(
                    batch,
                    &self.gui.atlas,
                    "recipe_arrow_filled",
                    Rect::new(0.0, 0.0, filled, 16.0),
                    Rect::new(r.x + 30.0, r.y + 21.0, filled, 16.0),
                    u32::MAX,
                );
            }
            batch.sprite(
                &self.gui.atlas,
                "horizon/aei/icons/flame_empty",
                r.x + 5.0,
                r.y + 24.0,
            );
            let burned = ((elapsed % 15000) * 15 / 15000) as f32;
            if burned < 14.0 {
                part(
                    batch,
                    &self.gui.atlas,
                    "icons/flame",
                    Rect::new(0.0, burned, 14.0, 14.0 - burned),
                    Rect::new(r.x + 5.0, r.y + 24.0 + burned, 14.0, 14.0 - burned),
                    u32::MAX,
                );
            }
            let mut labels = vec![(format!("{}s", time / 20), r.y + 50.0)];
            if let Some(recipe) = recipe
                && recipe.experience > 0.0
            {
                labels.push((format!("{} XP", recipe.experience), r.y + 4.0));
            }
            for (text, y) in labels {
                let x = r.x + 86.0 - HudBatch::text_width(&self.gui.atlas, &text);
                batch.text(
                    &self.gui.atlas,
                    &text,
                    x,
                    y,
                    rgba(128, 128, 128, 255),
                    false,
                );
            }
        }
    }
    pub(super) fn aei_click(&mut self, batch: &HudBatch, right: bool) -> bool {
        if !self.aei.open {
            return false;
        }
        if self.aei_transfer_click(batch, right) {
            return true;
        }
        let c = self.aei_catalogue_layout(batch);
        let pages = c.pages(self.aei.catalogue.len());
        self.aei.catalogue_page = self.aei.catalogue_page.min(pages - 1);
        if c.search.contains(self.cursor) {
            self.aei.search.focused = true;
            self.aei.search.reset_blink();
            if right {
                self.aei.search.set_value("");
                self.aei_search_changed();
            } else {
                let shift = self.has_shift_down();
                self.aei.search.x = c.search.x + 4.0;
                self.aei.search.width = (c.search.w - 14.0).max(1.0);
                self.aei
                    .search
                    .click(&self.gui.atlas, self.cursor.0, shift, false);
                self.aei.search_dragging = true;
            }
            return true;
        }
        self.aei_search_blur();
        if !right && c.categories.contains(self.cursor) {
            let target = if self.has_control_down() {
                if self.aei_cheat_available() {
                    Navigation::CheatMode
                } else {
                    Navigation::CheatUnavailable
                }
            } else {
                Navigation::AllCategories
            };
            self.aei.navigation_press = Some(NavigationPress {
                area: c.categories,
                target,
                backwards: false,
            });
            return true;
        }
        if !right && self.aei_modal() && self.aei_category_title_area(batch).contains(self.cursor) {
            self.aei.navigation_press = Some(NavigationPress {
                area: self.aei_category_title_area(batch),
                target: Navigation::AllRecipes,
                backwards: false,
            });
            return true;
        }
        for (i, b) in nav_buttons(c.navigation).into_iter().enumerate() {
            if b.contains(self.cursor) {
                if !right {
                    self.aei.navigation_press = Some(NavigationPress {
                        area: b,
                        target: Navigation::Catalogue,
                        backwards: i == 0,
                    });
                }
                return true;
            }
        }
        if !right && let Some(categories) = self.aei_click_categories(batch) {
            self.aei_show_categories(&categories);
            return true;
        }
        if let Some(stack) = self.aei_hovered(batch) {
            if self.aei_cheat_mode() && !right {
                self.aei_give_stack(&stack);
                return true;
            }
            self.aei_show_stack(&stack, right);
            return true;
        }
        if self.aei_modal() {
            let layout = self.aei_recipe_layout(batch);
            let kinds = self.aei_categories();
            let (first_tab, last_tab, tab_pages, tab_navigation) = self.aei_tabs(batch);
            if tab_pages > 1 && !right {
                for (i, area) in nav_buttons(tab_navigation).into_iter().enumerate() {
                    if area.contains(self.cursor) {
                        self.aei.navigation_press = Some(NavigationPress {
                            area,
                            target: Navigation::Tabs,
                            backwards: i == 0,
                        });
                        return true;
                    }
                }
            }
            for i in first_tab..last_tab {
                if layout.tab(i - first_tab).contains(self.cursor) {
                    if !right {
                        self.aei.navigation_press = Some(NavigationPress {
                            area: layout.tab(i - first_tab),
                            target: Navigation::CategoryTab(i),
                            backwards: false,
                        });
                    }
                    return true;
                }
            }
            for categories in [true, false] {
                let pages = if categories {
                    kinds.len()
                } else {
                    self.aei_category_entries().len().div_ceil(layout.per_page)
                };
                for (i, b) in nav_buttons(layout.nav(categories)).into_iter().enumerate() {
                    if b.contains(self.cursor) && !right && pages > 1 {
                        self.aei.navigation_press = Some(NavigationPress {
                            area: b,
                            target: if categories {
                                Navigation::Category
                            } else {
                                Navigation::Recipes
                            },
                            backwards: i == 0,
                        });
                        return true;
                    }
                }
            }
            return true; // A recipe window consumes clicks outside it too.
        }
        c.contains(self.cursor)
    }
    pub(super) fn aei_scroll(&mut self, batch: &HudBatch, steps: f32) -> bool {
        if self.aei_cycle_scroll(batch, steps) {
            return true;
        }
        if !self.aei.open {
            return false;
        }
        let c = self.aei_catalogue_layout(batch);
        if c.contains(self.cursor) {
            self.aei.catalogue_page = change_page(
                self.aei.catalogue_page,
                c.pages(self.aei.catalogue.len()),
                steps > 0.0,
            );
            return true;
        }
        if self.aei_modal() {
            let layout = self.aei_recipe_layout(batch);
            if !layout.area.contains(self.cursor) {
                return true;
            }
            let pages = self
                .aei_category_entries()
                .len()
                .div_ceil(layout.per_page)
                .max(1);
            self.aei.page = change_page(self.aei.page, pages, steps > 0.0);
            return true;
        }
        false
    }
}
