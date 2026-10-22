# Stage 02 prompt (verbatim summary)

Этап 2: Визуализация Террейна с Гидравлической Эрозией, Озера с Отражениями и Детализированных 3D-Пропсов.

1. `src/terrain_render.rs` + `src/water.rs`: меш террейна из `HeightfieldGrid` (позиции, гладкие нормали, вершинные цвета Splatmap: влажный песок, галька/пена у `water_level`, трава, мох в оврагах, базальт на уклонах, снег). Водный меш на `water_level` с анимацией волн, PBR (`perceptual_roughness = 0.06`, `reflectance = 0.9`, бирюзово-синий), совместим с `ScreenSpaceReflections`.
2. `src/props.rs`: авто-подхват `.glb` из `assets/models/` через `AssetServer`; встроенные префабы PineTree (ствол + 5 ярусов), Boulder (с мхом), Reeds/GrassClump, WoodenPier (сваи, настил, лодка, фонарь PointLight), CampfireSite (кольцо, дрова, скамьи, палатка, мерцающий PointLight с тенями и VolumetricLight, языки пламени, искры). Каждый корень — `SceneObject { id, name, category, aabb_half_extents }` (Component, Reflect, Serialize, Deserialize, Clone). Реактивность: свет/туман/время суток мгновенно; изменение геометрии пересобирает меш и пересаживает пропсы.

Oracle: `cargo check --message-format=short -q` = 0 ошибок; скриншоты через brp_extras/screenshot:
- docs/screenshots/stage_02_dusk_lake.png (MistyDuskLake, time_of_day = 20.8)
- docs/screenshots/stage_02_alpine_noon.png (SunnyAlpineNoon, time_of_day = 13.5)
Проверить глазами: сосны не под водой, пирс у берега, костер светится, горы естественны. Лог: docs/clean_room_audit/logs/stage_02_log.md.
