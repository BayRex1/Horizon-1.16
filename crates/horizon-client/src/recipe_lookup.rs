//! Recipe and usage lookups for Horizon's native item browser (AEI).
//!
//! The recipe-book UI filters displays by what the player has learned, but
//! `Book::build` holds every crafting and cooking display the client loaded.
//! This index can answer both directions without changing that vanilla UI.
use crate::recipe_book::{Book, Display};
use std::collections::{HashMap, HashSet};

/// Item ids to recipe-book entry indices, for both result and ingredient
/// queries. The vectors preserve the order of `Book::entries`.
#[derive(Clone, Debug, Default)]
pub struct RecipeLookup {
    results: HashMap<String, Vec<usize>>,
    uses: HashMap<String, Vec<usize>>,
}

impl RecipeLookup {
    /// Builds a lookup over every display currently modeled by `Book`.
    /// Multiple result variants or repeated ingredient slots add an entry
    /// only once for a given item.
    pub fn build(book: &Book) -> Self {
        let mut lookup = Self::default();
        for (index, entry) in book.entries.iter().enumerate() {
            let mut result_ids = HashSet::new();
            for result in entry.results() {
                let id = result.item.to_string();
                if result_ids.insert(id.clone()) {
                    lookup.results.entry(id).or_default().push(index);
                }
            }

            let mut ingredient_ids = HashSet::new();
            let requirements = match &entry.display {
                Display::Shaped { cells, .. } => cells.iter().flatten().collect::<Vec<_>>(),
                Display::Shapeless(ingredients) => ingredients.iter().collect(),
                Display::Furnace(ingredient) => vec![ingredient],
            };
            for ingredient in requirements {
                for id in &ingredient.0 {
                    if ingredient_ids.insert(id.clone()) {
                        lookup.uses.entry(id.clone()).or_default().push(index);
                    }
                }
            }
        }
        lookup
    }

    /// Entries that produce this item, including displayed result variants.
    pub fn recipes_for(&self, item: &str) -> &[usize] {
        self.results.get(item).map_or(&[], Vec::as_slice)
    }

    /// Entries that accept this item as an ingredient.
    pub fn uses_of(&self, item: &str) -> &[usize] {
        self.uses.get(item).map_or(&[], Vec::as_slice)
    }
}
