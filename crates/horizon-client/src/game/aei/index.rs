//! Search documents are cached until the resources or tooltip language change.
use super::{Game, search::Document};
use std::{collections::HashMap, sync::Arc};

impl Game<'_> {
    pub(super) fn aei_search_index_stale(&self) -> bool {
        self.aei
            .search_names
            .as_ref()
            .is_none_or(|names| !Arc::ptr_eq(names, &self.pack.names))
            || self.aei.search_language != self.settings.language
            || self.aei.search_advanced != self.settings.file.bool("advancedItemTooltips")
    }
    pub(super) fn aei_ensure_search_index(&mut self) {
        if !self.aei_search_index_stale() {
            return;
        }
        let mut mods = HashMap::from([("minecraft".to_owned(), "Minecraft".to_owned())]);
        let dir = Self::options_path().with_file_name(horizon_client::mod_list::DIR);
        let mut paths = horizon_mods::find(&dir);
        paths.extend(self.mods.dev.iter().cloned());
        for path in paths {
            let Ok(package) = horizon_mods::Package::read(&path) else {
                continue;
            };
            let manifest = &package.manifest;
            if !self.settings.mods.enabled.contains(&manifest.id) && !self.mods.dev.contains(&path)
            {
                continue;
            }
            for namespace in manifest.owned_namespaces() {
                mods.entry(namespace.to_owned())
                    .or_insert_with(|| format!("{} {}", manifest.id, manifest.name));
            }
        }
        let mut documents = Vec::new();
        let mut by_id = HashMap::new();
        for id in &self.res.items().registry_order {
            if id.as_ref() == "minecraft:air" {
                continue;
            }
            let namespace = id.split_once(':').map_or("minecraft", |p| p.0);
            let mut document = Document {
                id: id.to_string(),
                name: self.pack.names.item(id),
                mods: vec![
                    namespace.to_owned(),
                    mods.get(namespace).cloned().unwrap_or_default(),
                ],
                tooltip: self
                    .stack_tooltip(&horizon_client::items::ItemStack::new(id, 1))
                    .into_iter()
                    .map(|p| p.0.plain())
                    .collect(),
                ..Default::default()
            };
            document.normalize();
            by_id.insert(id.to_string(), documents.len());
            documents.push(document);
        }
        // The creative export includes variants such as enchanted books and potions.
        for (id, lines) in self.creative_screen.search_tooltips() {
            if let Some(&i) = by_id.get(id) {
                for line in lines {
                    let line = horizon_client::clipboard::strip_formatting(line).to_lowercase();
                    if !documents[i].tooltip.contains(&line) {
                        documents[i].tooltip.push(line);
                    }
                }
            }
        }
        for (tag, items) in &self.res.items().item_tag_order {
            for id in items {
                if let Some(&i) = by_id.get(id) {
                    documents[i].tags.push(tag.to_lowercase());
                }
            }
        }
        for tab in &self.creative_screen.tabs {
            if tab.search || tab.inventory {
                continue;
            }
            for stack in &tab.items {
                if let Some(&i) = by_id.get(stack.item.as_ref()) {
                    let label = format!("{} {}", tab.id, tab.title).to_lowercase();
                    if !documents[i].tabs.contains(&label) {
                        documents[i].tabs.push(label);
                    }
                }
            }
        }
        self.aei.search_index = documents;
        self.aei.search_names = Some(self.pack.names.clone());
        self.aei.search_language = self.settings.language.clone();
        self.aei.search_advanced = self.settings.file.bool("advancedItemTooltips");
    }
}
