# Linked prefab variants (2.6)

Base `.prefab.json` files remain version 1, with stable string member `id` and optional reflected scene `components`.
Legacy files receive IDs from their original indices on load. Saving source objects persists `PrefabSourceKey` identities across source-scene reloads. Preserve these IDs when editing/reordering a base.
Variants are version 2: `base` is an assets-relative prefab path; `overrides` maps member IDs to changed fields.
Chains resolve recursively (maximum 32 files). Cycles, invalid transforms/paths and missing member IDs fail explicitly.

```json
{"version":2,"name":"Wide","base":"prefabs/boulder_pair.prefab.json","overrides":{"left":{"scale":[3,2,2]}}}
```

Select an instance root and open the **Prefabs** inspector panel. Edit a field as JSON and Apply.
Changed fields show `*`; Reset removes the instance override and restores its inherited value.
Save variant creates a new asset referencing the selected instance's prefab; it never overwrites an existing file.
Instantiate the saved variant through the asset browser or SpawnPrefab.
Sample: `boulder_pair_blue.prefab.json` changes one boulder's scale and material colour while inheriting all other fields.

Commands:
- `SetPrefabOverride { target, member, field, value }`: `value` is JSON text; `null` resets the field.
  An empty `field` with `null` resets the entire member. Scene changes participate in unified Undo/Redo.
- `SavePrefabVariant { target, path }`: creates an asset, not an undoable scene change.
- Existing `SpawnPrefab` accepts bases or variants.

Names, free-object transforms, doors, lights and registered scene components may be overridden.
Nested component fields merge into the base. A null component entry removes that component; Reset on components restores inheritance.
New components require a complete reflect-serde value. Hierarchy/kind and grid-cell layout are authored in the base.
Grid members follow the instance root; their transforms are not independently overridden.
The Prefabs panel is the supported override authoring path; generic child inspector edits do not create overrides.

Resolved files are polled every 0.5 seconds. Existing matching members retain their ECS identity.
New/removed base members are reconciled; overrides referencing a removed member report a conflict and preserve the last valid instance.
`PrefabOverrides` is stored on the scene root via DynamicWorld. Hydration restores members and linked grid tiles.
Only roots are serialized, so inherited children and grid tiles are not duplicated in the scene file.
Reordering legacy files before persisting stable IDs can change identity; save IDs first.
