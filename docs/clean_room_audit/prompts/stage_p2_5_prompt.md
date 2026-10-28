# Stage P2.5 prompt
Phase 2 item 5: Asset Browser with thumbnails. Preview rendered by a separate camera into a texture (own render layer),
asset metadata sidecar, import settings for models. User: "Закоммить 2.4 (без gold.mat.json), потом начни 2.5".
Scope done: src/thumbnails.rs (lazy preview renderer), src/asset_meta.rs (<model>.arcmeta.json: scale/yaw/y_offset applied on
SpawnObject), ui_assets.rs (thumbnail cards, right-click Import settings menu). Bevy's own `.meta` is not used: the AssetServer parses it.
