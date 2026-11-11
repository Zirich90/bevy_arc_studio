# Stage 12 — Concept Visual + i18n (EN default / RU toggle)

Timestamp: 2026-11-11T13:41:24Z. Промпт: [stage_12_concept_visual_prompt.md](../prompts/stage_12_concept_visual_prompt.md).
Scope: визуал концепта (бренд-палитра, векторные иконки, chrome, композиция панелей) при сохранении всего текущего функционала; EN по умолчанию, RU переключается и персистится.

Файлы: src/ui.rs (TEAL/CHROME_BG/PANEL_BG/BORDER/TEXT/MUTED/AXIS), src/ui/icons.rs (25 векторных иконок + panel map), src/ui/i18n.rs (tr/shown/label, LANG AtomicU8, prefs lang), src/ui/layout.rs (palette i18n, prefs), src/ui_hud.rs (status bar/FPS tooltip), src/ui_toolbar.rs (camera chips), src/ui_inspector.rs (compact tab row: 3 primary + More), src/ui_reflect.rs (RU sweep).

Проверки: cargo check --message-format=short -q exit 0; cargo test --message-format=short -q exit 0 — 173 passed, 1 ignored (45+1+125+2). Сборка b55mihw13 CHECK/TEST/BUILD=0.

Скриншоты (BRP brp_extras/screenshot, все просмотрены глазами): docs/screenshots/concept_visual/
01_classic_1600_en.png, 02_focus_1600_en.png, 03_smart_terrain_1600_en.png,
04_classic_1600_ru.png, 05_focus_1600_ru.png, 06_smart_terrain_1600_ru.png, 07_classic_1280_ru.png.
Размеры подтверждены по PNG IHDR: 1600×900 (01–06), 1280×720 (07).

Визуальный вердикт: бренд-палитра и векторные иконки на месте; строка вкладок Inspector в одну линию (Global/Terrain/Objects + More; RU Общие/Ландшафт/Объекты + Ещё); статус-бары и camera chips локализованы («Готово», «Орбитальная камера»); без тултипов/артефактов; 1280 compact корректен (toolbar переносится допустимо).

Функционал сохранён: панели/вкладки/меню/режимы, Undo/Redo, Save/Load, PIE, Separate Game, BRP, модули, Terrain Tools — без регрессий (173 теста).
LIVE: редактор остановлен через BRP clean_shutdown; prefs lang=en (APPDATA/ArcStudio/interface.json); живых handles нет. Коммит/push НЕ выполнялись.

Доп. правка (контраст, 2026-11-11T13:52Z): активный чип имел почти-белый TEXT на ярком TEAL (~2:1, «плохо видно»). Foreground активного состояния заменён на тёмный CHROME_BG #0F1416 (~8.7:1) в icon_chip (src/ui.rs) и icons::button (src/ui/icons.rs); иконка активного чипа тоже тёмная. Неактивные чипы не тронуты. cargo check/test/build exit 0 (173 passed, 1 ignored). Все 7 кадров пересняты и просмотрены повторно: тёмный текст/иконка на TEAL читаемы (Move/Перемещение, вкладки Inspector, транспорт, Stats/Статистика, workspace-табы).

Доп. правка (тёмный ТЕКСТ активных чипов, 2026-11-11T15:17Z): Painter::galley не переопределяет цвет, запечённый в galley через layout_no_wrap(..., TEXT), поэтому icon_chip (src/ui.rs) пересобирает galley с финальным цветом (CHROME_BG для активного состояния). Текст активных чипов/вкладок теперь тёмный, не только иконка. RU-кадры 04–07 пересняты и просмотрены: «Перемещение (W)», «Все», «Общие», «Ландшафт», «Статистика» — тёмный текст+иконка на TEAL; раскладки Classic/Focus/Smart и 1280 compact корректны. Размеры по IHDR: 1600×900 (04–06), 1280×720 (07).

Доп. правка (туман, 2026-11-11T15:17Z): FogVolume.density_factor = fog_density*0.1 (было *1.6 → переусиление/инверт на максимуме), стартовый density_factor 0.0004 (было 0.02) — src/lighting.rs:141, src/scene.rs:44; DistanceFog Exponential без изменений. Sweep fog_density 0.0 / 0.004 / 0.03 снят и просмотрен: docs/screenshots/fog_fix/min.png (прозрачно), default.png (лёгкая дымка), max.png (плотно, цвета нормальные, БЕЗ инверта). Монотонно min→max. cargo check/test/build exit 0 (173 passed, 1 ignored).

LIVE: редактор остановлен через BRP clean_shutdown; prefs lang=en (APPDATA/ArcStudio/interface.json); живых handles нет.

VERDICT: PASS