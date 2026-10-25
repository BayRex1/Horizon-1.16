//! Native AEI registration contract shared by the SDK, runtime and client.
pub use horizon_mod_sdk::aei::{
    Catalyst, Category, ClickArea, Lookup, Plugin, Recipe, Role, Slot, Stack,
};

pub const NATIVE_CATEGORIES: [&str; 12] = [
    "minecraft:crafting",
    "minecraft:smelting",
    "minecraft:blasting",
    "minecraft:smoking",
    "minecraft:stonecutting",
    "minecraft:campfire_cooking",
    "minecraft:smithing",
    "minecraft:compostable",
    "minecraft:fuel",
    "minecraft:brewing",
    "minecraft:anvil",
    "minecraft:grindstone",
];

pub fn identifier(value: &str) -> bool {
    let Some((namespace, path)) = value.split_once(':') else {
        return false;
    };
    !namespace.is_empty()
        && !path.is_empty()
        && value.len() <= 256
        && namespace
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c))
        && path
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_./-".contains(&c))
}

/// Validate the whole replacement before making it visible. Limits bound host
/// allocations and retained rendering work independently of WebAssembly fuel.
pub fn validate(owner: &str, plugin: &Plugin) -> Result<(), String> {
    let fail = |message: &str| Err(message.to_string());
    if plugin.categories.len() > 32
        || plugin.recipes.len() > 1024
        || plugin.catalysts.len() > 128
        || plugin.click_areas.len() > 128
    {
        return fail("AEI registration exceeds its entry limits");
    }
    let owned = |id: &str| {
        identifier(id)
            && id
                .split_once(':')
                .is_some_and(|(namespace, _)| namespace == owner)
    };
    let mut categories = std::collections::HashMap::new();
    for category in &plugin.categories {
        if !owned(&category.id)
            || !identifier(&category.icon)
            || category.title.is_empty()
            || category.title.chars().count() > 128
            || !(18..=256).contains(&category.width)
            || !(18..=256).contains(&category.height)
            || categories.insert(category.id.as_str(), category).is_some()
        {
            return fail(
                "Invalid or duplicate AEI category (use the mod namespace and dimensions 18..256)",
            );
        }
    }
    let known = |id: &str| NATIVE_CATEGORIES.contains(&id) || categories.contains_key(id);
    let mut recipes = std::collections::HashSet::new();
    let mut alternatives = 0;
    for recipe in &plugin.recipes {
        let Some(category) = categories.get(recipe.category.as_str()) else {
            return fail("A mod recipe must use a category in the same registration");
        };
        if !owned(&recipe.id)
            || !recipes.insert(&recipe.id)
            || recipe.slots.is_empty()
            || recipe.slots.len() > 64
        {
            return fail("Invalid or duplicate AEI recipe");
        }
        for slot in &recipe.slots {
            alternatives += slot.alternatives.len();
            if slot.x.saturating_add(16) > category.width
                || slot.y.saturating_add(16) > category.height
                || slot.alternatives.is_empty()
                || slot.alternatives.len() > 64
                || slot
                    .alternatives
                    .iter()
                    .any(|stack| !identifier(&stack.item) || !(1..=99).contains(&stack.count))
            {
                return fail("Invalid AEI slot, item alternative or count");
            }
        }
    }
    if alternatives > 16384 {
        return fail("AEI registration exceeds its ingredient budget");
    }
    for catalyst in &plugin.catalysts {
        if !identifier(&catalyst.item)
            || catalyst.categories.is_empty()
            || catalyst.categories.len() > 32
            || catalyst.categories.iter().any(|id| !known(id))
        {
            return fail("Invalid AEI catalyst or category reference");
        }
    }
    for area in &plugin.click_areas {
        if !identifier(&area.screen)
            || area.width == 0
            || area.height == 0
            || area.x.saturating_add(area.width) > 1024
            || area.y.saturating_add(area.height) > 1024
            || area.categories.is_empty()
            || area.categories.len() > 32
            || area.categories.iter().any(|id| !known(id))
        {
            return fail("Invalid AEI container click area or category reference");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plugin() -> Plugin {
        Plugin {
            categories: vec![Category {
                id: "test:crusher".into(),
                title: "Crushing".into(),
                icon: "minecraft:anvil".into(),
                width: 116,
                height: 54,
            }],
            recipes: vec![Recipe {
                id: "test:iron".into(),
                category: "test:crusher".into(),
                slots: vec![Slot {
                    role: Role::Output,
                    x: 95,
                    y: 19,
                    alternatives: vec![Stack::new("minecraft:iron_ingot", 1)],
                }],
            }],
            catalysts: vec![Catalyst {
                item: "minecraft:anvil".into(),
                categories: vec!["test:crusher".into(), "minecraft:crafting".into()],
            }],
            click_areas: vec![ClickArea {
                screen: "minecraft:anvil".into(),
                x: 102,
                y: 48,
                width: 22,
                height: 15,
                categories: vec!["test:crusher".into()],
            }],
        }
    }
    #[test]
    fn validates_owned_categories_recipes_and_native_references() {
        assert!(validate("test", &plugin()).is_ok());
        assert!(validate("other", &plugin()).is_err());
        let mut value = plugin();
        value.categories.push(value.categories[0].clone());
        assert!(validate("test", &value).is_err());
        let mut value = plugin();
        value.recipes[0].category = "unknown:category".into();
        assert!(validate("test", &value).is_err());
        let mut value = plugin();
        value.catalysts[0].categories = vec!["other:crusher".into()];
        assert!(validate("test", &value).is_err());
    }
    #[test]
    fn rejects_host_work_and_coordinate_overflows() {
        let mut value = plugin();
        value.recipes[0].slots[0].x = u32::MAX;
        assert!(validate("test", &value).is_err());
        let mut value = plugin();
        value.recipes[0].slots[0].alternatives[0].count = 0;
        assert!(validate("test", &value).is_err());
        let mut value = plugin();
        value.click_areas[0].width = u32::MAX;
        assert!(validate("test", &value).is_err());
        let mut value = plugin();
        value.recipes = vec![value.recipes[0].clone(); 1025];
        assert!(validate("test", &value).is_err());
        assert!(identifier("minecraft:iron_ingot"));
        for id in ["", "stone", "TEST:iron"] {
            assert!(!identifier(id), "{id}");
        }
    }
}
