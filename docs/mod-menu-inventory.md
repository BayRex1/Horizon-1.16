# Inventory feed, schema 1

A launcher supporting this feed scans the selected instance's `horizon-mods/`
(packaged `.hmod` files and folder mods) and `horizon-native-mods/*.hnmod`.
It writes only when a valid installed package has id `mod_menu`.

```json
{
  "schema": 1,
  "mods": [
    {"id":"mod_menu","name":"Mod Menu","version":"0.1.0","kind":"Sandboxed","enabled":true},
    {"id":"veinminer","name":"Veinminer","version":"0.3.0","kind":"Native","enabled":false}
  ]
}
```

Kinds are display labels (`Sandboxed`, `Content`, `Native`). Unreadable files
remain visible with `enabled:false` and a `problem` string. Enabled state comes
from `horizon-mods.txt`; it is configuration, not runtime health. The registry's
installed-project metadata can supply a native mod's display name. No credentials,
account identifiers, absolute file paths, or Java source is exported.

Publish atomically via a temporary sibling to
`horizon-mods/.horizon-mod-data/mod_menu/inventory.json`. Limit the feed to 1 MiB.
The mod reads `inventory.json` using `storage.get`, validates schema and size,
and reloads on opening or refreshing. Unknown/missing/corrupt feeds show a useful
error. Resource packs and shader packs are not mods and are not listed.

Inventory does not include external `--dev-mod` paths or live runtime failure
information. Direct edits made outside the launcher need another launcher start
before they appear. Native and sandboxed mods are never executed by the scan.
