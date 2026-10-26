# Experimental tooltip contributions

The proposed API level **4** adds the client-only `tooltip` capability. This is
an additive development API, not a promise that a released Horizon supports it.
API levels are derived from component imports; the manifest remains `api = "0.1"`
for code or `api = "0.2"` for content plus code. Declare both
`capabilities = ["tooltip"]` and `experimental = ["tooltip"]` (plus any other
capabilities the mod actually uses).

```rust,ignore
use horizon_mod_sdk::tooltip::{self, Rule, Selector};
tooltip::replace(&[Rule {
    id: "wand-help".into(),
    selector: Selector::ItemId("example:wand".into()),
    translation_key: "example.wand.help".into(),
    color: None,
}])?;
```

`Selector::Enchantment(EnchantmentSelector { source: EnchantmentSource::Applied,
id: "minecraft:sharpness".into() })` selects an applied enchantment.
`EnchantmentSource::Stored` selects the corresponding stored book enchantment.
IDs are explicit resource IDs; the host derives no translation keys and knows no
mod IDs. There is no wildcard, component predicate, literal-text or script API.
The two selector forms are an ABI contract; new cases require a new interface or
function rather than extending this variant in place.

Each running module owns its rule set. `replace` validates the entire replacement
before committing it; an error preserves the previous set. `clear` empties only
the calling module's set. Stop, disable, trap, unload and failed initialization
remove the owning runtime instance; reload begins empty. Existing hooks may call
these functions, but ordinary static rules need only registration during `init`.
Hovering does not execute WASM or disclose the hovered stack to a module.

Enchantment rules follow the matching vanilla line in canonical enchantment
order, then module load order and registration order. Item rules follow vanilla
details. Whole-tooltip hiding suppresses every contribution, including the
creative name-only exception. Applied and stored sections respect their own
component visibility. These rules are used by actual item hover assembly, not
recipe-search indexing.

Translations resolve on every hover using the active language and existing
English fallback; missing keys produce no contribution. Text uses native font
wrapping at 200 GUI pixels and opaque host rendering, with optional `0xRRGGBB`
color (gray when absent). Translations are plain text, not commands or markup.

Input rejection limits: 256 rules per module; unique nonempty rule IDs;
nonempty translation keys; IDs and keys at most 128 Unicode scalar values;
well-formed namespaced selector IDs; colors at most `0xFFFFFF`. Host registration
runs under the ordinary WASM memory and fuel limits. Component canonical ABI
lifting happens before validation, so the host does not claim to reject a guest
list before Wasmtime allocates it.

Output clipping limits: resolve at most 1,024 Unicode scalar values per rule
before wrapping; retain at most 16 wrapped contribution lines per module and 32
across the tooltip, in display order. Vanilla lines never consume these budgets.
This clipping is deterministic and distinct from rejecting an invalid rule set.

Existing exported worlds, registration records and variants are unchanged. The
SDK's additive `horizon-tooltip-mod` world includes the prior SDK world and imports
the new interface; old compiled modules still bind to the old contract. API-level
inspection identifies `tooltip.replace` and `tooltip.clear` as level 4 so older
hosts can report their update requirement.
