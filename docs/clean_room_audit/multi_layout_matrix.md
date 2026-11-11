# Multi-Layout UI — матрица входов

| Действие | Существующий обработчик | Classic | Focus | Smart |
|---|---|---|---|---|
| Создать объект и править компоненты | objects_tab, ReflectEdits, StudioCommandQueue | Inspector | Inspector | Scene |
| Поиск, rename, reparent | ui_outliner::draw, операторы сцены | Scene Tree | Док Scene Tree | Scene |
| Выделение и трансформация | MeshPicking, GizmoState, EditorRegistry::activate_tool | Toolbar | Вертикальные инструменты | Toolbar |
| Камера и Focus Selected | SetCameraStyle, FocusSelected | View | Верхнее меню | View |
| Генерация и слои | world_environment::global_tab | Inspector | Window | Terrain |
| Кисть и растительность | world_environment::brushes_tab, terrain_brush | Inspector | Контекст кисти | Terrain |
| Свет, качество, атмосфера и вода | world_environment::global_tab, команды воды | Inspector | Window и контекст воды | World |
| Тайлы, этажи и постройки | level_builder::level_panel | Inspector | Контекст Build | Build |
| Ассеты и сцены | ui_assets::draw, AssetBrowserState | Нижняя панель | Док Assets | Scene |
| Материалы, префабы, Quillshade, граф | PanelDef, обработчики модулей | Inspector | Window | Scene |
| Расширения и управление модулями | EditorRegistry::panels, ModuleRegistry | Window | Window | Modules |
| Undo, Redo, Save, Load | StudioCommandQueue | File / Edit | Верхнее меню | File / Edit |
| PIE и Separate Game | StudioMode, play_mode, ui_hud | Общий блок запуска | Общий блок запуска | Общий блок запуска |

Входы Classic, Focus и Smart описывают целевое размещение. Действующие команды и панели общие.

По исходникам Window и Ctrl+K перечисляют активные панели `RightInspector` из одного `EditorRegistry`. Оба входа вызывают `show_registered_panel`: Classic выбирает вкладку Inspector, Focus открывает Inspector с выбранной панелью, Smart переводит её в соответствующий Workspace. Это подтверждает маршрутизацию по коду. Живые сценарии отмечены отдельно ниже.

Baseline непосредственного предшественника Multi-Layout взят из проверенных BRP-кадров коммита `9c15c1c`: [сцена после восстановления окружения](../screenshots/i2_environment_restored.png) и [открытая панель Separate Game](../screenshots/i2_ultra_safe.png). Оба кадра просмотрены повторно. Видны прежняя двухстрочная панель инструментов, плавающий HUD и прежнее размещение Separate Game. [BRP-кадр E0](../screenshots/e0_baseline.png) из `38c1577` оставлен как более ранний исторический пример.

## Статус сквозной проверки

Автоматические проверки текущего кода прошли: `cargo check --offline --message-format=short -q` (exit 0) и `cargo test --offline --message-format=short -q`. 171 тест прошёл, один doctest пропущен. Журналы: `target/layout_accept_check.log`, `target/layout_accept_tests.log`.

Проверка исходников для ассетов. `LayoutState::apply_pending` меняет только выбранную раскладку и Workspace. `AssetBrowserState` общий, а его индекс обновляется по собственному таймеру только при отрисовке видимого браузера. Поведение загрузчика подтверждено в живом прогоне счётчиком `AssetLoadStats` (src/ui_assets.rs): при цикле Classic→Focus→Smart→Classic `added`=821 и `loaded`=627 не изменились; рост `modified` вызван покадровой анимацией воды/искр, а не перезагрузкой.

| Сценарий | Проверено | Осталось |
|---|---|---|
| Переключение раскладок | Горячие клавиши, Move, выделение, Transform камеры, ID и имена 1736 сущностей. Classic оставался активным до отпускания гизмо, затем открылся Focus. Undo точно восстановил Transform | — |
| Профили и контекст | Автозапись выбора, загрузка подготовленного Focus, оба сброса, закрепление и повторное открытие Inspector, подсветка Assets. Свежий процесс с тем же изолированным конфигом восстановил Smart Scene и Node Graph (layout_profile_restarted.png). В том же процессе Focus открыл только Inspector, Classic показал Inspector, Scene Tree и Assets. Это соответствует записанным open-флагам (layout_profile_restarted_focus_confirmed.png и layout_profile_restarted_classic.png). После запуска Focus также восстановил закреплённый Terrain Brushes при активном Move (layout_profile_pin_restored.png) | — |
| Объекты и история | Создание Duck в Classic, Undo/Redo после перехода в Focus, rename и reparent с проверкой BRP, picking CampfireSite и перемещение гизмо по Y с точным Undo в Classic. В Focus клик по костру показал выделение, drag гизмо изменил Y с 13.668392 до 14.921064, Undo точно восстановил Transform (layout_focus_gizmo_moved.png). Smart подтвердил picking CampfireSite, тот же сдвиг Y и точный Undo (layout_smart_pick_verified.png, layout_smart_gizmo_verified.png). Правка другого компонента: Inspector → SceneObject.aabb_half_extents, drag x 1.0→3.5, BRP подтвердил [3.5,1,1], Ctrl+Z вернул точно [1,1,1] (layout_accept_aabb.png, layout_accept_undo_restored.png) | — |
| Terrain | Переход в Terrain с Move, явный выбор кисти, мазок изменил 662 отсчёта высоты, Undo точно восстановил карту. В Focus колесо и drag над Inspector оставили Transform камеры и карту высот прежними. Поиск принял `12345w`, кисть осталась активной. Захват ввода подтверждён и вне кисти: orbit-drag над Scene Tree и колесо над Asset Browser не изменили Transform камеры (колесо прокрутило список ассетов); ввод `qeb` в фокусированный фильтр Scene Tree оставил инструмент Move и камеру прежними (layout_accept_kbd_capture.png); Command Palette принял `move` и отфильтровал «Tool: Move» без смены инструмента и камеры (layout_accept_palette_typed.png) | — |
| Игровой режим | Play и Stop в Classic, Focus и Smart, откат Transform в Smart, запуск и остановка Separate Game во всех трёх раскладках. Запрос сцены через BRP проверен в Smart | — |
| Сохранение сцены | SaveScene/LoadScene через BRP восстановили координаты. В Focus путь введён через File → Scene Path, кнопка Save Scene записала файл и показала статус. File → Load Scene восстановил сцену после добавления Duck. Smart сохранил отдельный JSON и после контрольного изменения X утки на 80 восстановил X=41.350193 через File → Load Scene (layout_scene_roundtrip_smart.png). Classic также сохранил отдельный файл через File → Save Scene и восстановил X=41.350193 после контрольного изменения на 80 (layout_scene_roundtrip_classic_verified.png) | — |
| Расширения | Smart показал Level Builder, Lake & Water и Modules. Pathfinder 2e переключён On → Off → On. Room Box в Smart Build создал 17 LevelTile, подтверждено BRP. В Smart World выбран водоём, локальный уровень изменён с 14.1 до 25.5 и сброшен при глобальном 6.0. Карточка Boulder Pair в Smart Scene создала корень префаба и два дочерних объекта. Click to place подтвердил размещение второго экземпляра по клику на рельефе. Materials применил gold к Boulder pair #1000021, MaterialOverride подтверждён BRP. Quillshade добавил привязку env/rock_wet к тому же префабу, значение подтверждено BRP. Node Graph открыт в Smart Scene, элементы управления видны на BRP-кадре layout_smart_graph.png. BRP добавил Fireball (6 → 7 узлов), Undo вернул 6 узлов и 5 связей. После добавления Fireball весь JSON графа совпал при переходах Classic → Focus → Smart → Classic через меню. Раскладки подтверждены кадрами layout_graph_preserved_*.png, Undo вернул исходный JSON. В Smart Scene добавлен Fireball #7 через Add node, подтверждён BRP и кадром layout_graph_ui_added.png. Undo восстановил исходный JSON. Закрепление панели отключаемого модуля: юнит-тест `smart_remembers_pinned_panel_while_its_module_is_disabled` прошёл; в живом Smart закреплённый Inspector с контекстом Node Graph сохранился при переходе в Workspace Modules (layout_accept_window_menu.png, layout_accept_modules_tab.png), пин Classic независим. Ни один поставляемый module.json не владеет панелью RightInspector (pf2e даёт только spawner/commands), поэтому цикл disable→remember→restore покрыт юнит-тестом с синтетической панелью модуля | — |

Подробности и пути к BRP-кадрам в [журнале проверки](logs/stage_10_layout_review_log.md) и [журнале закрытия приёмки](logs/stage_10_layout_07_log.md).

## Revision 2.0 (stage 11), 2026-11-11

Вторая ревизия UI/UX по [промпту](prompts/stage_11_ui_revision_prompt.md): три режима стали разными рабочими местами без переписывания архитектуры. Итог и вердикт PASS — [stage_11_ui_revision_log.md](logs/stage_11_ui_revision_log.md).

| Режим | Что изменилось | Подтверждение (просмотрено) |
|---|---|---|
| Classic | Resizable+collapsible нижний Asset Browser (per-layout высота), сгруппированный Inspector со всеми вкладками, разделённые меню и edit-toolbar, статус-бар FPS/MS/Entities/Ready, без дубля тулбара | ui_revision_2/01_classic_1280.png, 08_classic_1600.png, eval_classic_1920.png |
| Focus | Компактный верхний бар (имя сцены, транспорт, Commands), узкий вертикальный rail (38 px) с подсветкой активного, плавающая контекст-панель (open/pin/move), плавающий док с реальными состояниями кнопок, отключаемый HUD | ui_revision_2/02_focus_1280.png, 09_focus_1600.png, eval_focus_context_1600.png |
| Smart/Scene | Объекты, Transform, Components, Assets, Prefabs, Node Graph по требованию | ui_revision_2/03_smart_scene_1280.png, eval_smart_scene_1600.png |
| Smart/Terrain | Спецпанель Generation/Brushes/Layers/Erosion/Biome; Assets свёрнут по умолчанию, но доступен; кисть только по «Activate brush» | ui_revision_2/04_smart_terrain_1280.png, 10_smart_terrain_1600.png |
| Smart/World | Группы Environment (Sun/Atmosphere/Time/Water/Fog/Lighting/Reflections/Shadows/Quality); per-basin и global water сохранены | ui_revision_2/05_smart_world_1280.png |
| Smart/Build | LevelBuilder, Grid/Snap, Floors/Walls/Rooms/Doors/Stairs/Roofs, палитра снизу | ui_revision_2/06_smart_build_1280.png |
| Smart/Modules | Управление модулями как основной контент | ui_revision_2/07_smart_modules_1280.png |

Проверки: cargo check exit 0; 171 тест PASS, 1 doctest ignored. Сохранность: UiLayout/ActiveTool/ActiveWorkspace, open→pin, per-layout prefs, EditorRegistry, Undo/Redo, Save/Load, PIE, Separate Game, BRP, модули и Terrain Tools не сломаны; ActiveTool при смене Workspace не переключается. Новое поведение сворачивания Assets в Terrain покрыто юнит-тестом smart_docks_follow_workspace_and_open_precedes_pin и сессионным флагом smart_assets_touched (ручной toggle/open приоритетнее автоповедения).

## Concept Visual + i18n (stage 12), 2026-11-11

Визуал приведён к исходному концепту (бренд-палитра, векторные иконки, chrome, композиция панелей) без изменения функционала; EN по умолчанию, RU переключается и персистится. Итог и вердикт PASS — [stage_12_concept_visual_log.md](logs/stage_12_concept_visual_log.md).

| Кадр (просмотрен) | Раскладка / язык | Подтверждение |
|---|---|---|
| concept_visual/01_classic_1600_en.png | Classic / EN | строка вкладок Global/Terrain/Objects + More в одну линию; бренд-палитра, векторные иконки |
| concept_visual/02_focus_1600_en.png | Focus / EN | компактный бар, rail, статус-бар локализован |
| concept_visual/03_smart_terrain_1600_en.png | Smart Terrain / EN | контекстная панель, свёрнутый Assets |
| concept_visual/04_classic_1600_ru.png | Classic / RU | Общие/Ландшафт/Объекты + Ещё; «Готово», «Орбитальная камера» |
| concept_visual/05_focus_1600_ru.png | Focus / RU | полная RU-локализация |
| concept_visual/06_smart_terrain_1600_ru.png | Smart Terrain / RU | полная RU-локализация |
| concept_visual/07_classic_1280_ru.png | Classic 1280×720 / RU | compact: toolbar переносится допустимо, вкладки в одну линию |

Проверки: cargo check exit 0; cargo test exit 0 — 173 passed, 1 ignored. Функционал (панели/вкладки/меню/режимы, Undo/Redo, Save/Load, PIE, Separate Game, BRP, модули, Terrain Tools) сохранён. LIVE: редактор остановлен, prefs lang=en, коммит/push не выполнялись.

Доп. правка контраста: активный чип (TEAL фон) переведён с почти-белого TEXT (~2:1) на тёмный CHROME_BG #0F1416 (~8.7:1) в icon_chip (src/ui.rs) и icons::button (src/ui/icons.rs); иконка активного чипа тёмная. Все 7 кадров concept_visual/ пересняты и просмотрены повторно — текст/иконка на активных чипах читаемы. cargo check/test/build exit 0 (173 passed, 1 ignored).

Доп. правка (тёмный ТЕКСТ + туман, 2026-11-11T15:17Z): icon_chip пересобирает galley с финальным цветом (Painter::galley не переопределяет запечённый цвет) — текст активных чипов/вкладок тёмный в EN и RU; RU 04–07 пересняты и просмотрены («Перемещение (W)», «Все», «Общие», «Ландшафт», «Статистика»). Туман: FogVolume.density_factor = fog_density*0.1 (было *1.6), старт 0.0004 (было 0.02) — src/lighting.rs:141, src/scene.rs:44. Sweep 0.0/0.004/0.03 просмотрен: fog_fix/min.png (прозрачно), default.png (дымка), max.png (плотно, без инверта). cargo check/test/build exit 0 (173 passed, 1 ignored). LIVE: редактор остановлен, prefs lang=en.
