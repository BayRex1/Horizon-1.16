//! A retained AEI integration: recipe data, a workstation catalyst and an
//! anvil click area. This demonstrates display integration, not a new machine.
use horizon_mod_sdk::{Mod, Registration, aei};

struct Example;

fn plugin() -> aei::Plugin {
    let category = "aei_example:pressing".to_string();
    aei::Plugin {
        categories: vec![aei::Category {
            id: category.clone(),
            title: "Pressing".into(),
            icon: "minecraft:anvil".into(),
            width: 116,
            height: 54,
        }],
        recipes: vec![aei::Recipe {
            id: "aei_example:iron_block".into(),
            category: category.clone(),
            slots: vec![
                aei::Slot {
                    role: aei::Role::Input,
                    x: 1,
                    y: 19,
                    alternatives: vec![aei::Stack::new("minecraft:iron_ingot", 9)],
                },
                aei::Slot {
                    role: aei::Role::Output,
                    x: 95,
                    y: 19,
                    alternatives: vec![aei::Stack::new("minecraft:iron_block", 1)],
                },
            ],
        }],
        catalysts: vec![aei::Catalyst {
            item: "minecraft:anvil".into(),
            categories: vec![category.clone()],
        }],
        click_areas: vec![aei::ClickArea {
            screen: "minecraft:anvil".into(),
            x: 102,
            y: 48,
            width: 22,
            height: 15,
            categories: vec![category],
        }],
    }
}
impl Mod for Example {
    fn init() -> Result<(Self, Registration), String> {
        aei::register(&plugin())?;
        Ok((Self, Registration::new().command("aei_example")))
    }
    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        match args {
            "recipes" => aei::show(aei::Lookup::Recipes, "minecraft:iron_block")?,
            "uses" => aei::show(aei::Lookup::Uses, "minecraft:iron_ingot")?,
            "clear" => aei::register(&aei::Plugin::new())?,
            "restore" => aei::register(&plugin())?,
            _ => aei::show(aei::Lookup::Category, "aei_example:pressing")?,
        }
        Ok(None)
    }
}
horizon_mod_sdk::export_mod!(Example);
