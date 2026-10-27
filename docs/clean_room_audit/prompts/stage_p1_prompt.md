# Phase 1 (editor foundation)
1.1 Close undo gaps: Reparent/ToggleDoor already undoable (docs/UNDOABLE list stale); add SetWaterBody (+ water UI/pick routed through the command). Grid/GroundSnap are tool settings, intentionally not undoable.
1.2 Scene format: v3 = v2 DynamicWorld + ChildOf hierarchy + SceneMeta version resource; load rejects newer versions, files without SceneMeta are v2.
