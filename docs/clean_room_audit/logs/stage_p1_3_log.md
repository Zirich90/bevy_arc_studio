# Phase 1.3 log (copy / paste / duplicate with descendants)
- Date: 2026-10-27
- Files: gizmo_3d.rs (ObjectClipboard, FreshIds, top_level, clone_objects, refresh_cloned_ids, Ctrl+C/V/D), mcp_commands.rs (Duplicate rewritten; new Copy, Paste), operators.rs (UNDOABLE += Paste), SKILL.md, docs/architecture/08_studio_commands.md.
- Behaviour: selection reduced to top-level roots; clone via linked cloning (descendants included); cloned descendants get fresh SceneObject ids; paste staggers per repeat; children are shifted in local space (no ground snap); undo/redo through SceneHistory.
- cargo check --message-format=short -q --tests: clean. cargo test: lib 70 passed (new: clone_carries_descendants_with_unique_ids), others unchanged.
- Runtime (BRP): campfire + child pine -> Duplicate/Paste x2 gave 4 campfires + 4 pines, ids 1000001..1000008 unique, each clone's pine is ChildOf its own campfire.
- Screenshot: docs/screenshots/p1_3_clone.png (no layout bugs; status "pasted 1 (with descendants)").
- Known limits: several clone commands in one frame can reuse the same root id (ids come from the live query); Ctrl+C keeps entity refs, so deleting the source before paste clones nothing.
- Verdict: PASS.
