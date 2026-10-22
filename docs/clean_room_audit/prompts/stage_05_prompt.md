Выполни **Этап 5: Упаковка BevyArcStudioPlugin для публикации на GitHub и в каталог Bevy Assets**.

Сохрани текст этого промпта в `docs/clean_room_audit/prompts/stage_05_prompt.md`.

### Задачи этапа:
1. **Выделение переиспользуемого Плагина (`src/lib.rs`) + Легкого Бинарника (`src/main.rs`):**
   - Оформи в `src/lib.rs` публичный плагин `BevyArcStudioPlugin` и модуль `prelude::*` (`BevyArcStudioPlugin`, `WorldSpec`, `HeightfieldGrid`, `SceneObject`, `StudioCommandQueue`, `UndoHistory`), чтобы любой разработчик на Bevy 0.20 мог подключить всю студию к своей игре одной строкой:
     `app.add_plugins(BevyArcStudioPlugin::default());`
   - Сделай `src/main.rs` компактной точкой входа, запускающей студию.
   - Создай скрипты быстрого запуска `run.bat` и `run.sh`.

2. **Лицензии, CI и `SKILL.md` для ИИ-агентов:**
   - Создай стандартные файлы `LICENSE-MIT`, `LICENSE-APACHE`, `.gitignore` (игнорирующий `/target`, но сохраняющий `docs/`) и `.github/workflows/ci.yml` (`cargo check` + `cargo test`).
   - Создай в корне файл `SKILL.md` с готовыми инструкциями и примерами JSON-RPC вызовов для ИИ-агентов (Claude Code / Cursor), объясняющий, как через `bevy_brp_mcp` управлять студией и снимать скриншоты.

3. **Витринный `README.md` (EN + RU) с Доказательной Базой Чистой Комнаты:**
   - Оформи красивый `README.md` со встроенными скриншотами из `docs/screenshots/` (`stage_04_editor_global.png`, `stage_02_dusk_lake.png`, `stage_02_alpine_noon.png`, `stage_03_gizmo.png`).
   - Добавь описание возможностей, таблицу горячих клавиш (`Q/W/E/R`, `F`, `Shift` Surface Snap, `Tab` Play/Editor, `Ctrl+Z/Y`, ПКМ+WASD Fly Mode), инструкцию быстрого старта и подключения плагина.
   - Добавь раздел **"Clean-Room AI Engineering & Provenance Audit"** с прямыми ссылками на:
     - Манифест `docs/clean_room_audit/CLEAN_ROOM_PROOF.md` (с `SHA-256` слепками спецификаций);
     - Чистые математические спецификации `docs/architecture/01..05.md`;
     - Все промпты `docs/clean_room_audit/prompts/stage_00..05_prompt.md`;
     - Бортовые журналы `docs/clean_room_audit/logs/`.

### Финальная проверка:
Выполни `cargo test --message-format=short -q`, проверь наличие всех ссылок и скриншотов в `README.md` и запиши финальный лог в `docs/clean_room_audit/logs/stage_05_log.md`.
