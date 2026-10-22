# Stage 06 log: reflect-сцена + UnitScale

- Время: 2026-10-22 (сессия BRP, порт 15702)
- Файлы: src/world_io.rs (новый), src/arc_core.rs (`UnitScale`), src/mcp_commands.rs (маршрутизация `.scn.json`, `execute_commands` pub),
  src/lib.rs, src/studio.rs (путь по умолчанию), src/ui_toolbar.rs (подписи, ширина поля пути);
  docs/architecture/06_tactical_rpg_framework.md, docs/clean_room_audit/prompts/stage_06_prompt.md
- Формат: `DynamicWorld` (bevy::world_serialization, в 0.20 бывший DynamicScene) → `.scn.json` + сайдкар `.heights` (f32 LE).
  Сохраняются `Transform`, `UserProp`, компоненты из `SceneComponentRegistry` и ресурсы `WorldSpec`, `UnitScale`.
- Расширение: `app.register_scene_component::<T>()` для любого `#[derive(Reflect)]` компонента с `#[reflect(Component)]`.
- Старый v1 `.json` загружается прежним путём (обратная совместимость); `Cargo.toml` не менялся.
- `UnitScale`: REAL = 0.3048 м/фт, GAME = 1.0 (5 фт = 5 м), по умолчанию GAME; ресурс сохраняется в сцене.
- cargo check --message-format=short -q: OK. cargo test --message-format=short -q: 15 passed (4 новых: roundtrip reflect-сцены,
  фильтр незарегистрированных компонентов, определение пути, режимы UnitScale).
- BRP: SpawnObject x3 → SaveScene (3 объекта, v2) → лишний SpawnObject → LoadScene: осталось ровно 3 объекта (pine, boulder, lantern).
- Скриншот: docs/screenshots/stage_06_reflect_scene.png, просмотрен: статус `loaded ... (v2, 3 objects)`, вёрстка без дефектов;
  найден обрез поля пути (150px) → расширено до 190px (после правки проверен только cargo check, повторный скриншот не снимался).
- Не покрыто: загрузка/сохранение геймплейных компонентов проверена юнит-тестом, в живой сцене таких компонентов пока нет.
- Вердикт: PASS
