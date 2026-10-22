# Model import log
- Timestamp: 2026-10-22
- Asset: assets/models/Duck.glb (120 KB), KhronosGroup/glTF-Sample-Assets (Duck, CC-BY 4.0, SIE). Attribution required if shipped.
- Test: tests/model_spawn.rs (file valid + headless AssetServer load of Scene(0) + ground-snap + scene instantiated under WorldAssetRoot).
- Fix: props::assets_dir() now picks first existing of BEVY_ASSET_ROOT / CARGO_MANIFEST_DIR / cwd / exe-dir `assets`; main.rs passes it to AssetPlugin.file_path (model was not discovered when launched without CARGO_MANIFEST_DIR).
- cargo check --message-format=short -q: clean. cargo test: 9 + 2 passed.
- Runtime: Duck SceneObject (category Model) at (41.35, 13.31, -14.39) via sync_props ring around camp, on grass next to the camp.
- Screenshot: docs/screenshots/model_duck_spawn.png (checked, no visual defects).
- Verdict: PASS
