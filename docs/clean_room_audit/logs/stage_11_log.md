# Stage 11 — операторы (лог)

- Дата: 2026-10-23, ветка stage-10-reflect-inspector.
- Файлы: новый src/operators.rs; изменены src/mcp_commands.rs, src/studio.rs, src/ui_reflect.rs, src/lib.rs; промпт docs/clean_room_audit/prompts/stage_11_prompt.md.
- Реестр: `Describe` строит JSON-схему всех операторов из Reflect-описания StudioCommand (имя, kind, params/required, undoable) — без ручной схемы.
- Ответы: ресурс `CommandResults` (seq, op, ok, message, data-JSON), читается через BRP; старый `log` сохранён. Отложенные world-команды (SpawnActor/QueryPath/MoveActor/Describe*/…Component/Reparent) пишут свой результат сами.
- Undo/redo: `ComponentHistory` — 4-й источник (HistSrc::Comp) в общем UndoOrder; правки Reflect-инспектора склеиваются в одну запись (0.4 с); MCP-правки — по записи на команду.
- Новые команды: DescribeScene{filter,limit}, SetComponent (частичный JSON merge), AddComponent, RemoveComponent (undoable), Reparent (сохраняет мировую позицию, НЕ undoable).
- Адресация: `Target::{Id, Name, Path, Entity}`; путь `Camp/Tent#1`, `#k` выбирает среди одноимённых соседей.
- cargo check --message-format=short -q: OK (с --features pf2e и без).
- cargo test: 50 passed (pf2e), 28 passed (без).
- Живая проверка BRP (первая сборка): Describe (6.4 КБ, все операторы), DescribeScene, AddComponent, SetComponent(Name)+Undo — имя откатилось. Ответы читались из CommandResults.
- Живая проверка (вторая сборка): DescribeScene{filter:Campfire,limit:2} (счётчики по именам, путь, bounds, дети), Reparent по Id и по Path (`PineTree#0/PineTree` -> корень, путь стал `PineTree#1`), RemoveComponent; отдельный Undo прошёл.
- Ограничение: в одном пакете `Undo` исполняется до отложенных world-команд (они применяются позже в кадре), поэтому `Undo` отправлять отдельным пакетом после результата правки.
- Не проверено вживую: откат Reflect-правки мышью в UI (только юнит-тест и MCP-путь); результат Undo после RemoveComponent не перечитан из сцены (только "undo" ok в CommandResults); pf2e-команды не трогались.
- Известные ограничения: Reparent без undo; Duplicate для процедурных пропов, list/map/Option/Handle в инспекторе, смена id после пересборки террейна (Path/Name смягчают) — как и раньше.
- Скриншот: docs/screenshots/stage_11.png — просмотрен, вёрстка и сцена без дефектов; статус «removed Visibility» виден в тулбаре.
- Вердикт: PASS (с оговорками выше).
