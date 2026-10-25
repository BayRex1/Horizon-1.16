# AEI's JEI reference

AEI uses JEI's implementation as the specification for shared UI and behavior. Read the relevant
classes from the actual jar before changing a widget or interaction; do not approximate them from memory.
Horizon-specific choices explicitly requested for AEI are documented in [extras.md](extras.md).

## Pinned artifact and local workflow

- Artifact: `jei-1.21.1-neoforge-19.27.0.340.jar`.
- SHA-256: `8aaf547432f1b4958239b036356b910692fe40f858c3073d996f56bbf7c99826`.
- Current local copy: `/home/murillo-deolino/Downloads/backup/mods/jei-1.21.1-neoforge-19.27.0.340.jar`.
- Decompiler: Vineflower 1.12.0, pinned by `research-lock.json`.

Keep the jar, decompiler and resulting sources in a temporary directory. For example, with a verified
Vineflower jar and the local JEI path supplied by the developer:

```sh
sha256sum "$JEI_JAR"
jei_reference=$(mktemp -d /tmp/horizon-jei-reference.XXXXXX)
java -Xmx2G -jar "$VINEFLOWER_JAR" --folder --skip-extra-files=1 --log-level=warn --thread-count=4 "$JEI_JAR" "$jei_reference"
```

The runtime remains native Rust and does not load JEI or Java. Horizon uses its own prepared Minecraft
26.3 recipes; this reference jar targets Minecraft 1.21.1. Do not silently replace the reference with
another JEI version or assume a Minecraft version difference is a JEI UI behavior.

## Classes used for recipe screen parity

All class names below are relative to `mezz.jei` in the decompiled jar.

| Behavior | Reference |
| --- | --- |
| Empty recipe/usage lookup leaves the screen and history unchanged | `gui.recipes.RecipesGui.show`, `gui.recipes.RecipeGuiLogic.setState` |
| Native container click areas and catalysts | `library.plugins.vanilla.VanillaPlugin.registerGuiHandlers`, `registerRecipeCatalysts` |
| Native station layouts and generated recipe families | `library.plugins.vanilla.{stonecutting,anvil,brewing,grindstone,compostable,cooking}` category classes and recipe makers |
| SDK extension concepts | `api.registration.IRecipeCategoryRegistration`, `IRecipeRegistration`, `IRecipeCatalystRegistration`, `IGuiHandlerRegistration` |
| Full category browser/title lookup | `gui.recipes.RecipeGuiLogic.showAllRecipes`, `RecipesGui.RecipeGuiInputHandler` |
| Station category and missing-input ordering | `gui.recipes.RecipeSortUtil`, `gui.recipes.layouts.LazyRecipeLayoutList`, `library.transfer.BasicRecipeTransferHandler` |
| Transfer button, Shift maximum, error visibility and red missing-input highlights | `gui.recipes.RecipeTransferButtonController`, `library.transfer.BasicRecipeTransferHandler`, `RecipeTransferErrorMissingSlots`, `library.gui.recipes.layout.builder.RecipeLayoutBuilder` |
| Window, header bars, four navigation buttons and title | `gui.recipes.RecipesGui.init`, `render`, `updateLayout` |
| Wrapping and enabled states | `gui.recipes.RecipeGuiLogic`, `gui.PageNavigation` |
| Left-button press/release, hover, disabled tint and icon alignment | `gui.elements.IconButton`, `gui.elements.InternalIconButton` |
| Sprite sizes, trimming and tiling | `common.gui.textures.Textures`, `common.gui.elements.DrawableSprite`, `DrawableNineSliceTexture` |
| Tabs and catalyst position | `gui.recipes.RecipeGuiTabs`, `RecipeGuiTab`, `RecipeCatalysts` |
| Recipe dimensions, spacing and centered slots | `gui.recipes.RecipeGuiLayouts`, `library.gui.recipes.RecipeLayout`, `library.gui.ingredient.CraftingGridHelper` |
| Crafting and cooking arrows, flame and labels | `library.plugins.vanilla.crafting.CraftingRecipeCategory`, `library.plugins.vanilla.cooking.AbstractCookingCategory`, `library.gui.helpers.GuiHelper`, `common.util.TickTimer` |
| Escape/inventory closes; Backspace returns; page/category shortcuts | `gui.recipes.RecipesGui.RecipeGuiInputHandler`, `gui.config.InternalKeyMappings` |
| Shared ingredient/output cycle and held Shift pause | `library.gui.ingredients.CycleTicker`, `library.gui.ingredients.RecipeSlot`, `library.gui.recipes.RecipeLayout` |

Recipe navigation uses 13×13 buttons, with trimmed 9×9 arrow icons and JEI's 20×20 button sprites tiled
with six-pixel borders. Both arrows stay enabled on the first/last page and wrap, but are disabled when
only one recipe page/category exists. The catalogue's arrows remain active even on one page. Buttons
execute on left release inside their original bounds; right clicks and releases outside do not navigate.

The default recipe window is 198 pixels wide, starts at y=45, and uses screen height minus 58, with a
minimum height of 175 and maximum of 350. Excess vertical space centers the capped window. Tabs are
24×24 at window x+2/y−21. Recipe panels include four pixels of border: crafting 124×62, cooking 90×62.
The layout uses JEI's integer division and minimum inter-recipe padding of four to calculate page size.

JEI-owned UI sprites are bundled unchanged under
[`crates/horizon-client/resources/aei/jei`](../crates/horizon-client/resources/aei/jei), with their full
MIT license and attribution. No Minecraft-namespace assets or decompiled Java sources are bundled.

## AEI-specific browsing choices

The pinned `CycleTicker` advances once every 20 UI ticks, stops advancing while
`Screen.hasShiftDown()` is true, and shares its index across a recipe's linked
slots. AEI preserves the one-second interval and held Shift pause, including the
remaining interval on release. The requested previous/next controls are an AEI
extension: Left/Right over the hovered recipe, or wheel up/down while pause is
held. All three controls are remappable. Manual steps share the recipe's input,
output and cost index and restart its interval; they do not advance other recipes
or consume world RNG. Closing the recipe view discards its manual selections.

The user requested a six-pixel right margin for the whole catalogue, an all-category
button beside search, and station-local prioritization of catalogue results. The
existing JEI category-title lookup and compatible-category/missing-input hierarchy
are the reference for the browser. The result catalogue uses the same hierarchy
inside a workstation, retains all results, and returns to normal order in ordinary
inventory/container screens. This ordering is always active in those stations;
it is not a claim that JEI's default item-list sort is inventory-dependent.

## Remaining coverage

AEI exposes all twelve native categories registered in `VanillaPlugin`: crafting,
stonecutting, smelting, smoking, blasting, campfire cooking, smithing, composting,
fuel, brewing, anvil and grindstone. Layouts follow their corresponding category
classes; dynamic repair/enchantment displays use Horizon's native workstation rules. Mods can add native
categories, recipes, catalysts and container click areas through
[the SDK API](aei-api.md). Java JEI plugins, bookmarks and
every configurable JEI option are not yet implemented. This is a parity target, not a claim that the
entire JEI feature set has shipped.
New coverage should follow the jar and include a regression check for the interaction being added.

The native transfer button uses JEI's unchanged 7×7 `icons/recipe_transfer` sprite
in a 13×13 button, positioned two pixels right of the recipe panel and aligned
with its bottom. Missing inputs use JEI's `0x66ff0000` highlight while the button
is hovered. Left release transfers and returns to the parent menu; Shift requests
the maximum amount. Native Horizon transfers use normal validated container
clicks rather than JEI's Java server extension. See [transfer coverage](aei-api.md#recipe-transfer).
