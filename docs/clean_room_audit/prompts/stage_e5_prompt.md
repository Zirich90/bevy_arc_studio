# Этап E5: Asset Browser, Prefabs, группировка тайлов в Scenetree

(Промпт сохранён в сокращённом виде; полный текст — в запросе пользователя.)

1. Scenetree: группы LevelTile по этажу и типу (`▸ Floor L0 ×36`), ToggleDoor в Undo/Redo.
2. `src/prefabs.rs`: `assets/prefabs/*.prefab.json` (иерархия, локальные координаты относительно пивота, SceneObject/LevelTile/Door/света/glb), `PrefabInstance { prefab_path }`, команды `SaveSelectionAsPrefab { path }`, `SpawnPrefab { path, position, rotation_y_deg }` (один шаг Undo, Grid/Surface Snap, BRP).
3. `src/ui_assets.rs`: нижняя панель Asset Browser (тулбар-кнопка, поиск): Models (.glb/.gltf в assets/models, assets/showcase), Prefabs (+Save Selected as Prefab), Scenes (.scn.json, Load/Save Current), Procedural & Level Kit.
4. Приёмка: юнит-тесты (prefab save/spawn/undo, скан каталога), cargo check/test чистые, скриншот docs/screenshots/e5_asset_browser.png (tavern_corner.prefab.json + второй экземпляр), лог docs/clean_room_audit/logs/stage_e5_log.md со статусом PASS.
