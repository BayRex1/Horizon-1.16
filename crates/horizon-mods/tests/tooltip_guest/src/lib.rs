//! Real guest for tooltip capability, validation, and lifecycle tests.
use horizon_mod_sdk::{
    Mod, Registration, storage,
    tooltip::{self, EnchantmentSelector, EnchantmentSource, Rule, Selector},
};
struct Guest;
fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "item".into(),
            selector: Selector::ItemId("minecraft:stick".into()),
            translation_key: "tooltip.fixture.item".into(),
            color: Some(0x123456),
        },
        Rule {
            id: "applied".into(),
            selector: Selector::Enchantment(EnchantmentSelector {
                source: EnchantmentSource::Applied,
                id: "minecraft:mending".into(),
            }),
            translation_key: "tooltip.fixture.applied".into(),
            color: None,
        },
        Rule {
            id: "stored".into(),
            selector: Selector::Enchantment(EnchantmentSelector {
                source: EnchantmentSource::Stored,
                id: "minecraft:mending".into(),
            }),
            translation_key: "tooltip.fixture.stored".into(),
            color: None,
        },
    ]
}
impl Mod for Guest {
    fn init() -> Result<(Self, Registration), String> {
        tooltip::replace(&rules())?;
        if storage::get("fail").is_some() {
            return Err("fixture init failed after registration".into());
        }
        Ok((Guest, Registration::new().command("tooltip")))
    }
    fn command(&mut self, _: &str, args: &str) -> Result<Option<String>, String> {
        let mut next = rules();
        match args {
            "clear" => {
                tooltip::clear();
                return Ok(None);
            }
            "replace" => next.truncate(1),
            "duplicate" => next.push(rules().remove(0)),
            "invalid" => next[1].translation_key = String::new(),
            "color" => next[1].color = Some(0x1000000),
            "quota" => {
                next = (0..257)
                    .map(|i| Rule {
                        id: format!("r{i}"),
                        ..rules().remove(0)
                    })
                    .collect()
            }
            "fail_reload" => {
                storage::set("fail", b"yes")?;
                return Ok(None);
            }
            "trap" => panic!("fixture trap"),
            _ => {}
        }
        tooltip::replace(&next)?;
        Ok(None)
    }
}
horizon_mod_sdk::export_mod!(Guest);
