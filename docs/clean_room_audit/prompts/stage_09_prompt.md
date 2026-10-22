Выполни **Этап 09: arc_actors — Шаблоны актеров PF2e (Fighter, Goblin), фракции, зоны-триггеры и интеграция с редактором (под фичей `pf2e`)**.

Сохрани текст этого промпта в `docs/clean_room_audit/prompts/stage_09_prompt.md`.
Прочитай `CLAUDE.md`, `docs/architecture/06_tactical_rpg_framework.md`, `src/arc_core.rs` и `src/movement.rs`.
Все RPG-компоненты держи за фиче-флагом `#[cfg(feature = "pf2e")]`. Проверки запускай через `cargo test --features pf2e --message-format=short -q`.

### Задачи этапа:
1. **Reflect-компоненты сущностей (`src/actors/mod.rs`):**
   - `Actor`: имя, уникальный ID, ссылка на шаблон.
   - `CharacterSheet`: привязка статов, спасбросков, AC, HP из `arc_rules` с поддержкой `#[derive(Reflect)]`.
   - `Faction`: `Player`, `Hostile`, `Neutral`.
   - `Interactable`: тип взаимодействия (`Loot`, `Talk`, `Trigger`).
   - Зарегистрируй их в TypeRegistry Bevy, чтобы они автоматически сохранялись в `.scn.json` и инспектировались в правом Инспекторе студии!

2. **Файлы-Шаблоны Актеров в `assets/actors/` (`.ron`):**
   - `assets/actors/hero_fighter.ron`: Боец 1 уровня, HP 20, AC 18, Speed 25 ft, Longsword (1d8 slashing).
   - `assets/actors/goblin_warrior.ron`: Гоблин 1 уровня, HP 15, AC 16, Speed 25 ft, Dagger (1d4 piercing, agile).

3. **Интерактивные Зоны-Триггеры (`src/actors/triggers.rs`):**
   - Компонент `TriggerVolume { shape: TriggerShape::Cylinder { radius: f32, height: f32 }, event_id: String }`.
   - В режиме редактора триггер подсвечивается габаритным контуром через `Gizmos` и передвигается стрелочками Гизмо.
   - В режиме Play: система отслеживает вход (`TriggerEnterEvent`) и выход (`TriggerExitEvent`) актеров фракции `Player` в зону триггера.

4. **Интеграция со Студией и MCP-командами:**
   - Во вкладку `Objects & Spawner` добавь кнопки быстрого спавна: `[+ Spawn Fighter]`, `[+ Spawn Goblin]`, `[+ Spawn Trigger Volume]`.
   - При спавне актер автоматически получает компонент `Mover` (из Этапа 07) и ставится на рельеф через `sample_height`.
   - Спавн, перемещение гизмо и удаление актеров поддерживают `UndoHistory` (`Ctrl+Z` / `Ctrl+Y`).
   - Команды MCP: `SpawnActorFromTemplate { template: String, pos: Vec3, faction: Faction }`.

### Проверка этапа (Oracle PASS / FAIL):
1. Проверь сборку с фичей: `cargo check --features pf2e --message-format=short -q` (0 ошибок).
2. Запусти тесты: спавн бойца из RON-шаблона, проверка получения урона, вход бойца в триггер-зону гоблина. `cargo test --features pf2e --message-format=short -q`.
3. Запусти студию (через `cargo run --features pf2e`), заспавни на поляне у костра Бойца и двух Гоблинов, сними скриншот через BRP (`stage_09_actors_spawn.png`).
4. Проверь глазами, что актеры стоят на рельефе, выделяются кликом и показывают свойства в инспекторе. Запиши лог в `docs/clean_room_audit/logs/stage_09_log.md`.
