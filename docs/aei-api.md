# Native AEI API

AEI is Horizon's native item/recipe viewer. It works in survival, creative,
adventure and spectator. In spectator, E opens a read-only inventory browser;
AEI never moves items, crafts, drops stacks or bypasses the server's permissions.
Ctrl+R controls visibility in every mode. Escape closes a recipe back to its
original inventory/container screen, and Backspace follows recipe history.

## Settings and shortcuts

Open **Options → Horizon Settings → AEI Settings**. **AEI** is the master switch;
when off, the viewer and its shortcuts stay disabled. The visibility shortcut
only hides/shows an enabled viewer. **Vanilla recipe book** independently controls
Minecraft's book button, panel, ghost ingredients and recipe-unlock notifications.
Both features start enabled, and disabling the book preserves recipe data for AEI.

The page and **Controls → Key Binds → AEI** edit the same shortcuts. Click a binding,
hold any desired Ctrl/Shift/Alt/Super modifiers and press a key or mouse button.
To bind a modifier alone, press and release it without pressing another key.
Escape unbinds it. Individual Reset buttons restore a shortcut; Reset Keys on the
AEI page restores only AEI shortcuts. The normal controls list's Reset Keys restores
both vanilla and AEI bindings. Conflicting assignments are highlighted.

| Action | Default |
| --- | --- |
| Show / hide AEI | Ctrl+R |
| Show recipes / uses for the hovered item | R / U |
| Previous recipe in history | Backspace |
| Previous / next category | Shift+Page Up / Shift+Page Down |
| Previous / next recipe page | Page Up / Page Down |
| Focus search | Ctrl+F |
| Previous / next ingredient alternative in the hovered recipe | Left / Right arrow |
| Pause ingredient cycling while held | Shift (either side) |

Hold the pause control and scroll over a recipe to step through its alternatives
(up for previous, down for next). Releasing it resumes the remaining automatic
one-second interval. A manual step restarts that recipe's interval. Linked inputs,
outputs and displayed costs share one cycle index, including SDK recipes.

Escape always closes the recipe view; the inventory key also closes it when search
is not focused. Clipboard shortcuts remain standard text-editing commands.
AEI preferences and bindings are saved in `horizon-extras.txt`, separately from
vanilla's `options.txt`. The one-time visibility toast names the current binding.

When the world allows commands, Ctrl-click the crafting-table button beside the
search field to toggle cheat mode. The button turns red while it is active. Click
an AEI item to add one full stack to your inventory; click again for another
stack. Press **R** over an item to open its recipe. The server's command
permissions still decide whether items can be granted, including in multiplayer.

## Container integration

Native container arrows open their associated recipes, following the click areas
in JEI 19.27.0.340's `VanillaPlugin.registerGuiHandlers`:

| Screen id | Area (x, y, width, height), relative to its panel | Category |
| --- | --- | --- |
| `minecraft:inventory` | 137, 29, 10, 13 | `minecraft:crafting` |
| `minecraft:crafting` | 88, 32, 28, 23 | `minecraft:crafting` |
| `minecraft:crafter` | 88, 32, 28, 23 | `minecraft:crafting` |
| `minecraft:furnace` | 78, 32, 28, 23 | `minecraft:smelting`, `minecraft:fuel` |
| `minecraft:blast_furnace` | 78, 32, 28, 23 | `minecraft:blasting`, `minecraft:fuel` |
| `minecraft:smoker` | 78, 32, 28, 23 | `minecraft:smoking`, `minecraft:fuel` |
| `minecraft:brewing_stand` | 97, 16, 14, 30 | `minecraft:brewing` |
| `minecraft:anvil` | 102, 48, 22, 15 | `minecraft:anvil` |
| `minecraft:grindstone` | 92, 31, 28, 21 | `minecraft:grindstone` |
| `minecraft:smithing` | 68, 49, 22, 15 | `minecraft:smithing` |

The crafting table and crafter are crafting catalysts; the furnace, blast furnace
and smoker are their categories' catalysts. Looking up a catalyst's uses includes
all recipes in those categories. The recipe screen lists every registered catalyst
for its current category. Opening a recipe preserves the parent container.

The button beside AEI's search opens all loaded categories with recipes. Category
titles also open the complete browser, preserving the selected category, as in JEI.
Inside a workstation, matching native and mod categories are listed first. Catalyst
and click-area registrations supply the workstation/category association for SDK
recipes too. Available input recipes and their catalogue results are prioritized,
then partial matches, then other recipes. Input alternatives and counts consume
the same stock rather than counting an item twice. The order refreshes when the
inventory or station inputs change; ordinary inventory/chest browsing retains the
normal order. This changes browsing priority only, and does not craft items or
assume intermediate recipes have already been made.

## Native categories

AEI includes all twelve categories registered by the reference JEI vanilla plugin:

| Category id | Source in Horizon |
| --- | --- |
| `minecraft:crafting` | Loaded crafting recipe displays |
| `minecraft:stonecutting` | Loaded stonecutter recipes |
| `minecraft:smelting`, `minecraft:blasting`, `minecraft:smoking` | Loaded cooking recipes |
| `minecraft:campfire_cooking` | Loaded campfire cooking recipes |
| `minecraft:smithing` | Loaded smithing transforms and trims, with actual result components |
| `minecraft:compostable` | Item compost providers, with normal-layer probabilities |
| `minecraft:fuel` | Item fuel providers and their actual burn times |
| `minecraft:brewing` | Loaded brewing predicates and potion outputs |
| `minecraft:anvil` | Repair materials, self-repair and enchanted books, evaluated by the native anvil rules |
| `minecraft:grindstone` | Self-repair and non-curse disenchantment, evaluated by the native grindstone rules |

The dynamic workstation operations use representative, valid input variants;
they are not invented entries in the gameplay recipe registry. Their viewer ids
live under `minecraft:aei/`. Potion contents, enchantments, damage and armor trims
are preserved for rendering and tooltips. Availability distinguishes potion types,
required enchantments and damaged repair inputs. Fuel and compost displays are
input-only, matching JEI; they do not pretend to be crafting outputs. Native menus
have their corresponding JEI click areas, including brewing, anvil, smithing and
grindstone. Explicit SDK click areas take precedence over an overlapping native area.

## Recipe transfer

A 13×13 **+** button beside a recipe fills the compatible open menu from the
player's inventory and existing input slots. A normal click places one set of
ingredients; Shift-click places the largest set that fits the input slots. The
viewer closes back to its parent menu after a successful transfer. Take the
result manually: AEI does not click the output, grant items or spend creative-mode
materials that the player does not own.

Missing ingredients disable the button. Hover it for item names and quantities;
the missing recipe input slots are highlighted red, as in JEI. Alternatives share
the available stock, and each physical item is reserved once. Transfers retain
stack components, including potion contents, damage and enchantments.

The native handlers cover 2×2 inventory crafting, crafting tables, crafters,
furnace/smoker/blast-furnace ingredients and fuel, stonecutting, smithing, anvil,
grindstone and brewing inputs. Stonecutting also selects the requested result.
A recipe that needs a larger crafting grid explains that requirement. Categories
without a handler for the current screen have no transfer button. The current SDK
registers viewer definitions only; arbitrary mod-machine transfer mappings are
not inferred from a catalyst or a display's coordinates.

A carried stack, spectator mode or insufficient space for returning old inputs
blocks the transfer with a tooltip. Planning and menu validation finish before any
click is applied. The actual move uses ordinary container clicks and the server's
normal inventory checks in both single-player paths and multiplayer; it does not
require JEI networking. Recipe-book visibility and recipe-unlock status do not
control the viewer's placement operation. Server crafting restrictions still apply
when the player takes a result. A release outside the original button, or a recipe
page change before release, cancels the press.

## Register a mod integration

Add `api = "0.3"` and `capabilities = ["aei"]` to `horizon-mod.toml`. Import
`horizon_mod_sdk::aei` and call `aei::register(&plugin)?` from `Mod::init`.
The [complete example](../crates/horizon-mod-sdk/examples/aei-integration/src/lib.rs)
registers a Pressing category, a recipe, an anvil catalyst and a clickable anvil arrow.
It has no per-tick or per-frame callback: the game retains and draws the definitions.

`aei::Plugin::new()` starts an empty registration with four lists:

| List | Contract |
| --- | --- |
| `categories` | `Category { id, title, icon, width, height }`; content dimensions exclude the four-pixel recipe border. |
| `recipes` | `Recipe { id, category, slots }`; each `Slot { role, x, y, alternatives }` has 16×16 item content. Coordinates exclude the border. |
| `catalysts` | `Catalyst { item, categories }`; a workstation item may open several categories through its uses lookup. |
| `click_areas` | `ClickArea { screen, x, y, width, height, categories }`; coordinates are relative to the open container panel. |

Slot roles are `Input`, `Output`, and `RenderOnly`. Recipes match only output slots;
uses match only input slots. `alternatives` cycle like native JEI ingredients.
`aei::Stack::new(item_id, count)` describes each alternative. This initial contract
supports item identifiers and counts, not stack components, fluids or arbitrary draw callbacks.
Categories and recipes belong to the registering mod's namespace (`my_mod:crusher`).
A recipe uses a category in the same registration. Catalyst/click-area references
may also name any native category id listed above. References to other mods'
categories are not part of this version.

Screen matching accepts a native menu id or the open container/station's block id.
For example, an anvil integration can target `minecraft:anvil`; a container exposed
by another Horizon subsystem can use its own namespaced block id. This API registers
viewer integration, not new block entities, menus, machine logic or server recipes.
A display definition does not make an item craftable. Content recipes continue to
use the normal data pack/mod content pipeline.

The host validates a whole replacement before applying it. A failed registration
leaves the previous one intact. Limits per mod: 32 categories, 1024 recipes, 128
catalysts and 128 click areas; 64 slots per recipe, 64 alternatives per slot, 16384
alternatives overall; category dimensions 18..256, item counts 1..99 and click-area
bounds 0..1024. Item ids must be namespaced, titles at most 128 characters.
Register at most once per hook call. Passing an empty plugin removes the mod's
integration. Stopping, disabling, trapping or reloading a mod removes its previous
definitions and recipe history that might refer to them. Other mods remain registered.

## Request a lookup

```rust,ignore
aei::show(aei::Lookup::Recipes, "minecraft:iron_block")?;
aei::show(aei::Lookup::Uses, "minecraft:iron_ingot")?;
aei::show(aei::Lookup::Category, "my_mod:crusher")?;
```

Requests are queued for the native client (up to eight per hook call). `Ok(())`
means the request was accepted, not that a match exists. No match leaves the screen
and history unchanged, as in JEI. Requests are honored while a world is open in
play, chat, or an inventory/container screen, and only when AEI is visible. They do
not interrupt title screens, settings, signs or books. Search, R/U, recipe history,
category tabs, navigation and tooltips work on native and mod-provided recipes.

## Build and test the example

Build Horizon with `--features mods`. Build the SDK example with the normal authoring tool:

```sh
cargo build -p cargo-horizon-mod
cd crates/horizon-mod-sdk/examples/aei-integration
../../../../target/debug/cargo-horizon-mod build
```

The tool prints the generated `.hmod` path. Use that path with `--dev-mod` in a
fresh test game home, as required by the repository's agent instructions. Run
`/aei_example` in chat to open Pressing, or `/aei_example recipes`, `uses`, `clear`
and `restore` to exercise the API. The example changes only the viewer.

The sandbox tests build the SDK example into a real WebAssembly component and
check capability enforcement, retained registration, lookup requests, clearing and
trap cleanup. Validation tests cover namespace ownership, duplicate definitions,
unknown category references and bounds. Run:

```sh
cargo test -p horizon-mods --features runtime,pack
cargo test -p horizon-client --bin horizon-client game::aei
python3 tools/aei_e2e.py
```

The WIT package remains `horizon:mods@0.1.0`: `aei` is a new optional imported
interface. Manifest API 0.3 prevents older hosts from accepting a mod requiring
this capability; existing 0.1/0.2 manifests and the events ABI remain supported.
