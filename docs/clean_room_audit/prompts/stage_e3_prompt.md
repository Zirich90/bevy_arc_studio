# Stage E3 prompt

Выполни **Этап E3: Реестр плагинов редактора (`EditorRegistry`) и перевод генератора мира (`WorldPlugin`) на модульную архитектуру** согласно плану `docs/architecture/07_editor_plan.md`.

## Шаг 1. Контекст и фиксация промпта
1. Сохрани текст этого промпта в `docs/clean_room_audit/prompts/stage_e3_prompt.md`.
2. Прочитай `CLAUDE.md`, раздел **E3** в `docs/architecture/07_editor_plan.md` и текущие файлы `src/lib.rs`, `src/ui.rs`, `src/ui_toolbar.rs`, `src/ui_inspector.rs`, `src/ui_reflect.rs`, `src/mcp_commands.rs`.
3. Режим экономии токенов: `cargo check --message-format=short -q`, `cargo test --message-format=short -q`.

## Шаг 2. Задачи разработки Этапа E3
1. **Единый Реестр Расширений Редактора (`src/editor_registry.rs`)** — ресурс `EditorRegistry`:
   - `register_panel(id, title, location, render_fn)` — вкладки правого инспектора или нижней/левой панели;
   - `register_tool(id, label, hotkey, icon/hint, on_activate, update_system)` — инструменты вьюпорта для верхнего тулбара (`Select/Gizmo`, `Terrain Brush`, в E4 `Wall/Room Builder`);
   - `register_scene_component::<T>()` — авто-регистрация в `TypeRegistry`, в сохранении/загрузке `DynamicWorld` (`world_io.rs`) и в рефлексивном инспекторе (`ui_reflect.rs`);
   - `register_inspector_override::<T>(custom_ui_fn)` — кастомный виджет компонента;
   - `register_spawner_item(category, label, spawn_fn)` — элементы меню `+ Spawn`.
2. **Перевод генератора окружения (`WorldPlugin`) на `EditorRegistry`:** террейн, вода, атмосфера, рассеивание пропсов и кисти как модульный `WorldEnvironmentPlugin`: вкладки (`Global Settings`, `Terrain Brushes`), инструмент кисти и спавнеры (`Pine`, `Boulder`, `Campfire`, `Pier`, `Duck`) через `EditorRegistry` (без хардкода в ядре инспектора). Флаг `terrain_enabled` в `WorldSpec` (режим `Empty Grid Scene`): отключает процедурный террейн и озеро, включает `Infinite Grid` на `Y = 0` (подготовка к E4).
3. **Универсальный инспектор выбранного объекта:** все зарегистрированные компоненты через `ui_reflect.rs` + кнопка **`[+ Add Component]`** (список из `EditorRegistry`) и кнопка удаления компонента `[×]`.

## Шаг 3. Приёмка (Oracle PASS / FAIL)
1. Юнит-тесты на регистрацию инструментов, панелей и компонентов; `cargo test` и `cargo check` без ошибок.
2. Скриншот `docs/screenshots/e3_registry_inspector.png` (Windows/AMD: `WGPU_BACKEND=dx12` или `Screenshot::primary_window()`): выбран объект (`Duck #1728` или `CampfireSite`), открыта карточка с компонентами, видна `[+ Add Component]`.
3. Осмотреть скриншот, записать отчёт в `docs/clean_room_audit/logs/stage_e3_log.md` со статусом **PASS**.
