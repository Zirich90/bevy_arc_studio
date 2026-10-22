Выполни **Этап 4: Сборка Супер-GUI Редактора (HUD, Тулбар, Инспектор на 3 вкладки), Кистей Террейна и Командной Шины MCP**.

Сохрани текст этого промпта в `docs/clean_room_audit/prompts/stage_04_prompt.md`.
Прочитай `CLAUDE.md`. Помни: все UI-системы `bevy_egui 0.43.1` регистрируются СТРОГО в расписании `EguiPrimaryContextPass`, а сборку проверяй через `cargo check --message-format=short -q`.

### Задачи этапа (Современная стилистика студии в духе ArcEngine):

1. **Левый верхний виджет телеметрии и быстрых тумблеров (`src/ui_hud.rs` — стиль ArcEngine):**
   - Полупрозрачная темная плашка в левом верхнем углу (работает и в Play, и в Editor режиме):
     - Живые метрики: `FPS`, `Frame ms`, `Entities`, `Props`, `Lights`;
     - Быстрые кнопки пресетов качества: `[Low] [Med] [High] [Ultra]`;
     - Блок времени суток: `Time: HH:MM`, ползунок `00:00..24:00`, кнопки скорости солнца `[-][+]` и тумблер `[Auto Sun]`;
     - Ряд быстрых визуальных тумблеров (подсвечиваются бирюзовым при включении): `[Water] [Reflections] [Fog] [God Rays] [Grass] [Shadows]`.

2. **Верхний Тулбар Редактора (`src/ui_toolbar.rs`):**
   - Аккуратная верхняя полоса:
     - Переключатель режимов: `[Editor Mode (Tab)]` / `[Play Mode (Full Screen)]`;
     - Кнопки инструментов: `[Select (Q)] [Move (W)] [Rotate (E)] [Scale (R)]` + тумблеры `[Snap to Ground]` и `[Align to Normal]`;
     - Кнопки камеры: `[Orbit Cam] [Fly Cam]` + `[Focus (F)]`;
     - Кнопки истории: `[Undo (Ctrl+Z)]` / `[Redo (Ctrl+Y)]`;
     - Быстрые пресеты сцены в 1 клик: `[Misty Dusk]` `[Sunny Noon]` `[Midnight Camp]` `[Golden Dawn]`;
     - Кнопки `[Save Scene JSON]` / `[Load Scene JSON]`.

3. **Правая Панель Инспектора (`src/ui_inspector.rs`, ширина ~360px) с Поиском и 3 Вкладками:**
   - **Строка живого поиска сверху (`Search parameters or objects...`):** мгновенная фильтрация параметров.
   - **Вкладка 1 `Global Settings` (Полный пульт мира):**
     - Раскрывающиеся секции `WorldSpec` с ползунками: *Terrain & Erosion*, *Lake & Water*, *Sun & Atmosphere*, *Fog & God Rays*, *Biome Foliage*. Отпускание ползунка фиксирует транзакцию в `UndoHistory`.
   - **Вкладка 2 `Objects & Spawner`:**
     - Кнопки быстрого спавна в центр сцены (`[+ Pine]`, `[+ Boulder]`, `[+ Campfire]`, `[+ Pier]`, `[+ Lantern]`) + поле импорта внешней `.glb` модели из `assets/models/`;
     - Список всех `SceneObject` сцены с поиском и кликом для выделения;
     - Детальный инспектор выбранного объекта: `Position (X, Y, Z)`, `Rotation`, `Scale`, кнопка `[Drop to Ground]`, настройки `PointLight` (цвет, радиус, интенсивность).
   - **Вкладка 3 `Terrain Brushes` (Интерактивный скульптинг рельефа в 3D):**
     - Выбор кисти (`[Raise]`, `[Lower]`, `[Smooth]`, `[Flatten]`, `[Paint Pines]`, `[Erase Props]`), ползунки `Brush Radius (2..45m)` и `Strength`.
     - При активной кисти рисуй 3D-кольцо радиуса кисти прямо на рельефе под курсором мыши через `Gizmos` и применяй изменение высоты при зажатой ЛКМ в реальном времени!

4. **Двусторонняя Командная Шина MCP (`src/mcp_commands.rs`):**
   - Ресурс `StudioCommandQueue` (`#[derive(Resource, Reflect, Default, Serialize, Deserialize)]` с `#[reflect(Resource)]`), чтобы внешний ИИ-агент по `bevy_remote` мог одной JSON-командой применять пресеты, спавнить объекты на рельеф, скульптить ландшафт и снимать скриншоты.

### Проверка этапа (Oracle PASS / FAIL):
1. Выполни `cargo test --message-format=short -q` и `cargo check --message-format=short -q` (0 ошибок).
2. Запусти студию и сними через MCP `brp_extras_screenshot` серию скриншотов:
   - `docs/screenshots/stage_04_editor_global.png` (полный вид студии с левым HUD, верхним тулбаром, открытой вкладкой `Global Settings` и закатным озером с костром);
   - `docs/screenshots/stage_04_editor_objects.png` (вкладка `Objects & Spawner` с выделенным объектом и 3D-стрелками гизмо);
   - `docs/screenshots/stage_04_terrain_brush.png` (вкладка `Terrain Brushes` с кольцом кисти на рельефе).
3. Визуально убедись, что интерфейс гармоничный, функциональный и не перекрывает 3D-сцену. Запиши отчет в `docs/clean_room_audit/logs/stage_04_log.md`.
