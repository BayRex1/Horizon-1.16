//! Native tooltip assembly for retained mod contributions. No guest calls on hover.
use crate::{
    enchant::Enchantments, font::Font, hud::rgba, item_tooltip::TooltipDisplay, items::ItemStack,
    text::Text,
};
use horizon_mods::{EnchantmentSource, TooltipRule, TooltipRules};

/// Per-hover output budgets, shared by every section of the actual tooltip.
pub struct Contributions<'a> {
    modules: Vec<&'a TooltipRules>,
    used: Vec<usize>,
    total: usize,
}
impl<'a> Contributions<'a> {
    pub fn new(modules: impl Iterator<Item = &'a TooltipRules>) -> Self {
        let modules: Vec<_> = modules.collect();
        Self {
            used: vec![0; modules.len()],
            modules,
            total: 0,
        }
    }
    fn append(
        &mut self,
        output: &mut Vec<(Text, u32)>,
        font: &Font,
        translate: &impl Fn(&str) -> Option<String>,
        select: impl Fn(&TooltipRules) -> &[TooltipRule],
    ) {
        for (index, module) in self.modules.iter().enumerate() {
            for rule in select(module) {
                let remaining = (16 - self.used[index]).min(32 - self.total);
                if remaining == 0 {
                    break;
                }
                let Some(text) = translate(&rule.translation_key) else {
                    continue;
                };
                let text = Text::literal(text.chars().take(1024).collect::<String>());
                let rgb = rule.color.unwrap_or(0xAAAAAA);
                let color = rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255);
                for (line, _) in font.split_lines(&text, 200.0).into_iter().take(remaining) {
                    output.push((line, color));
                    self.used[index] += 1;
                    self.total += 1;
                }
            }
        }
    }
    /// Append visible vanilla enchantments and contributions in canonical order.
    /// Used directly by Game::stack_tooltip for applied equipment and stored books.
    pub fn enchantments(
        &mut self,
        output: &mut Vec<(Text, u32)>,
        stack: &ItemStack,
        enchantments: &Enchantments,
        source: EnchantmentSource,
        font: &Font,
        translate: &impl Fn(&str) -> Option<String>,
    ) {
        let (component, levels) = match source {
            EnchantmentSource::Applied => ("minecraft:enchantments", stack.enchantments()),
            EnchantmentSource::Stored => {
                ("minecraft:stored_enchantments", stack.stored_enchantments())
            }
        };
        if !TooltipDisplay::for_stack(stack).shows(component) {
            return;
        }
        for (id, text, curse) in enchantments.tooltip_entries(levels) {
            output.push((
                Text::legacy(&text),
                if curse {
                    rgba(255, 85, 85, 255)
                } else {
                    rgba(170, 170, 170, 255)
                },
            ));
            self.append(output, font, translate, |rules| {
                rules.enchantment(source, id)
            });
        }
    }
    /// Item hints follow all vanilla details; whole hiding also covers creative.
    pub fn item(
        &mut self,
        output: &mut Vec<(Text, u32)>,
        stack: &ItemStack,
        font: &Font,
        translate: &impl Fn(&str) -> Option<String>,
    ) {
        if !TooltipDisplay::for_stack(stack).hides_tooltip() {
            self.append(output, font, translate, |rules| rules.item(&stack.item));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mods::{EnchantmentSelector, TooltipSelector};
    use horizon_world::nbt::Tag;
    use std::{collections::HashMap, path::Path};
    fn font() -> Font {
        Font::load(
            Path::new("/nonexistent-horizon-tooltip-test"),
            &HashMap::new(),
            &[],
            (1, 1),
            (0, 0),
            crate::hud::Rect {
                u0: 0.0,
                v0: 0.0,
                u1: 1.0,
                v1: 1.0,
                w: 1,
                h: 1,
            },
        )
    }
    fn rule(id: &str, selector: TooltipSelector) -> TooltipRule {
        TooltipRule {
            id: id.into(),
            selector,
            translation_key: id.into(),
            color: Some(0x123456),
        }
    }
    fn rules() -> TooltipRules {
        let mut rules = TooltipRules::default();
        rules
            .replace(vec![
                rule("hint", TooltipSelector::ItemId("example:wand".into())),
                rule(
                    "applied-a",
                    TooltipSelector::Enchantment(EnchantmentSelector {
                        source: EnchantmentSource::Applied,
                        id: "example:a".into(),
                    }),
                ),
                rule(
                    "applied-b",
                    TooltipSelector::Enchantment(EnchantmentSelector {
                        source: EnchantmentSource::Applied,
                        id: "example:b".into(),
                    }),
                ),
                rule(
                    "stored-a",
                    TooltipSelector::Enchantment(EnchantmentSelector {
                        source: EnchantmentSource::Stored,
                        id: "example:a".into(),
                    }),
                ),
            ])
            .unwrap();
        rules
    }
    fn stack() -> ItemStack {
        let mut stack = ItemStack::new("example:wand", 1);
        stack.data_mut().enchantments = vec![("example:a".into(), 1), ("example:b".into(), 2)];
        stack.data_mut().stored_enchantments = vec![("example:a".into(), 3)];
        stack
    }
    fn assemble(
        stack: &ItemStack,
        rules: &[&TooltipRules],
        tr: impl Fn(&str) -> Option<String>,
    ) -> Vec<(Text, u32)> {
        let mut ench = Enchantments::default();
        ench.tags.insert(
            "minecraft:tooltip_order".into(),
            vec!["example:b".into(), "example:a".into()],
        );
        let mut c = Contributions::new(rules.iter().copied());
        let mut out = vec![];
        let font = font();
        c.enchantments(
            &mut out,
            stack,
            &ench,
            EnchantmentSource::Stored,
            &font,
            &tr,
        );
        c.enchantments(
            &mut out,
            stack,
            &ench,
            EnchantmentSource::Applied,
            &font,
            &tr,
        );
        c.item(&mut out, stack, &font, &tr);
        out
    }
    #[test]
    fn actual_enchantment_assembly_interleaves_canonical_ids_and_item_hint() {
        let rules = rules();
        let out = assemble(&stack(), &[&rules], |key| Some(key.into()));
        let texts: Vec<_> = out.iter().map(|l| l.0.plain()).collect();
        assert_eq!(
            texts,
            [
                "example:a III",
                "stored-a",
                "example:b II",
                "applied-b",
                "example:a I",
                "applied-a",
                "hint"
            ]
        );
        assert_eq!(out[1].1, rgba(0x12, 0x34, 0x56, 255));
    }
    #[test]
    fn whole_hiding_including_creative_and_independent_sections() {
        let rules = rules();
        let mut stack = stack();
        for component in ["minecraft:enchantments", "minecraft:stored_enchantments"] {
            stack.data_mut().set_other(
                "minecraft:tooltip_display",
                Some(Tag::compound([(
                    "hidden_components",
                    Tag::List(vec![Tag::String(component.into())]),
                )])),
            );
            let out = assemble(&stack, &[&rules], |key| Some(key.into()));
            let texts: Vec<_> = out.iter().map(|l| l.0.plain()).collect();
            assert!(texts.contains(&"hint".into()));
            assert_eq!(
                texts.contains(&"applied-a".into()),
                component != "minecraft:enchantments"
            );
            assert_eq!(
                texts.contains(&"stored-a".into()),
                component != "minecraft:stored_enchantments"
            );
        }
        stack.data_mut().set_other(
            "minecraft:tooltip_display",
            Some(Tag::compound([("hide_tooltip", Tag::Byte(1))])),
        );
        assert!(TooltipDisplay::for_stack(&stack).name_visible(true));
        assert!(assemble(&stack, &[&rules], |key| Some(key.into())).is_empty());
    }
    #[test]
    fn translations_resolve_each_hover_missing_keys_omit_and_unicode_wraps_natively() {
        let rules = rules();
        let stack = ItemStack::new("example:wand", 1);
        assert!(assemble(&stack, &[&rules], |_| None).is_empty());
        assert_eq!(
            assemble(&stack, &[&rules], |_| Some("日本語".into()))[0]
                .0
                .plain(),
            "日本語"
        );
        assert_eq!(
            assemble(&stack, &[&rules], |_| Some("changed language".into()))[0]
                .0
                .plain(),
            "changed language"
        );
        let text = "界".repeat(1200);
        let out = assemble(&stack, &[&rules], |_| Some(text.clone()));
        assert_eq!(out.len(), 16);
        assert!(out.iter().all(|(line, _)| font().width_text(line) <= 200.0));
        assert!(
            out.iter()
                .map(|(l, _)| l.plain().chars().count())
                .sum::<usize>()
                <= 1024
        );
    }
    #[test]
    fn native_name_resolver_uses_english_fallback_and_omits_unknown_keys() {
        let root =
            std::env::temp_dir().join(format!("horizon-tooltip-language-{}", std::process::id()));
        let dir = root.join("assets/minecraft/lang");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("en_us.json"),
            r#"{"example.tooltip.fallback":"English fallback"}"#,
        )
        .unwrap();
        let names = crate::creative::Names::load(&root);
        let mut registered = TooltipRules::default();
        let mut r = rule("english", TooltipSelector::ItemId("example:wand".into()));
        r.translation_key = "example.tooltip.fallback".into();
        registered.replace(vec![r]).unwrap();
        let out = assemble(&ItemStack::new("example:wand", 1), &[&registered], |key| {
            names.display_get(key)
        });
        assert_eq!(out[0].0.plain(), "English fallback");
        assert!(
            names
                .display_get("example.tooltip.definitely_missing")
                .is_none()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wrapped_budgets_are_per_module_and_aggregate_in_display_order() {
        let (rules, other, third) = (rules(), rules(), rules());
        let stack = ItemStack::new("example:wand", 1);
        let out = assemble(&stack, &[&rules, &other, &third], |_| {
            Some(
                (0..25)
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        });
        assert_eq!(out.len(), 32);
        assert_eq!(out[15].0.plain(), "15");
        assert_eq!(out[16].0.plain(), "0");
    }
}
