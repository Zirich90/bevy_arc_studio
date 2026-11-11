# Stage 12 — Concept Visual & Icons (визуал концепта, функционал текущий)

Дата: 2026-11-11. Промпт владельца (сокращённо): «Не похоже на изначальный концепт. И поменяй иконки».
Уточнения владельца: «визуал концепта лучше, как мне кажется. Функционал лучше в текущей версии»;
язык — «По умолчанию английский, с переключением на русский».

## Задача
Принять визуальный стиль шести концепт-изображений (brand kit: акцент #14C7B3, chrome #0F1416,
панели #1E2326, границы #344146, текст #E6ECEB / #A8B3B1; A-логотип; векторные иконки
Terrain=гора, Level Builder=куб и т.д.) БЕЗ потери текущего функционала: все панели, вкладки,
меню, режимы Classic/Focus/Smart и Workspace сохраняются.

## Объём работ (выполнено в коде)
1. src/ui/i18n.rs — EN по умолчанию, RU переключатель, персист в APPDATA/ArcStudio/interface.json;
   tr/shown/label + таблица переводов; тест tr_follows_language_and_defaults_to_english.
2. src/ui/icons.rs — 25 векторных иконок painter'ом, icon_chip/button, панельная карта panel(id).
3. src/ui.rs — brand-константы TEAL/CHROME_BG/PANEL_BG/BORDER/TEXT/MUTED/AXIS_*, apply_style, логотип.
4. Хром: топ-бары Classic/Focus/Smart, статус-бар, viewport chips (src/ui_toolbar.rs, ui_hud.rs, ui/layout.rs).
5. Панели: inspector (карточка объекта, цветные XYZ, секции), outliner, assets (вкладки с иконками),
   terrain/brushes, level builder, materials, modules, node graph, prefab variants, quillshade adapter,
   separate game, command palette, reflect inspector — все строки через tr/label.
6. RU-проход завершён: ui_reflect.rs (Components/Add Component/Remove/not editable),
   layout.rs (Command Palette / Search commands…), фильтры поиска принимают EN и RU.

## Критерии приёмки
- cargo check --message-format=short -q exit 0, без предупреждений.
- cargo test --message-format=short -q exit 0 (все тесты, включая i18n).
- BRP-скриншоты через brp_extras/screenshot в docs/screenshots/concept_visual/, КАЖДЫЙ просмотрен:
  Classic 1600×900 EN, Focus 1600×900 EN, Smart Terrain 1600×900 EN,
  Classic 1600×900 RU, Focus 1600×900 RU, Smart Terrain 1600×900 RU,
  плюс 1280×720 проверка компактности (любая раскладка).
- Визуально: бренд-палитра, векторные иконки вместо текстовых глифов, логотип, чипы вьюпорта;
  функционал не потеряан (вкладки/меню/панели на месте).
- Журнал docs/clean_room_audit/logs/stage_12_concept_visual_log.md (15–20 строк, вердикт PASS),
  матрица multi_layout_matrix.md дополнена, .memory/00_STATE.md и 01_BLACKBOARD.md обновлены.
- После приёмки редактор остановлен через BRP; prefs возвращены к EN default. Коммит/push НЕ выполнять.