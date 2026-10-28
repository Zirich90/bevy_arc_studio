# Phase 2.5 log (asset thumbnails, import metadata)
- Date: 2026-10-28
- Files: src/thumbnails.rs (new), src/asset_meta.rs (new), ui_assets.rs (cards with thumbnails, import menu), props.rs (meta on spawn, strip_model_cameras exempts ThumbnailCamera), showcase/mod.rs + lighting.rs (camera queries skip ThumbnailCamera), ui.rs, lib.rs.
- cargo check --message-format=short -q --tests: clean. cargo test: lib 76 + 45 passed, 0 failed (3 new: sidecar round trip, framing, request queue).
- Design: one job at a time, only cards scrolled into view; model loaded on RenderLayers 7 via Propagate, camera framed by Aabb bounds, 128px Rgba8UnormSrgb target shown through EguiUserTextures; first 3 jobs wait longer (pipeline warm-up).
- Bug found by Oracle: strip_model_cameras removed the preview camera (thumbnails were blank); fixed with a ThumbnailCamera marker.
- Runtime (BRP): Duck.glb.arcmeta.json {scale 3, yaw 90} -> SpawnObject model:Duck.glb gives scale [3,3,3], yaw 90 deg.
- Screenshot: docs/screenshots/p2_5_thumbnails.png (cards show bridge, pillar, door previews; PFB/scene badges unchanged; no layout bugs).
- Not verified visually: the right-click Import settings menu (BRP click opens nothing visible); logic covered by tests and the spawn check.
- Verdict: PASS.
