# Multi-Layout UI — промежуточная приёмка

- Дата проверки: 2026-11-04.
- Исполняемый файл запущен через Cargo в пользовательском сеансе Windows.
- BRP на 15702 сохранил кадры редактора размером 1600×900.
- Classic: docs/screenshots/layout_classic.png, кадр просмотрен.
- Focus: docs/screenshots/layout_focus.png, кадр просмотрен.
- Smart Scene: docs/screenshots/layout_smart.png, кадр просмотрен.
- Smart Terrain: docs/screenshots/layout_smart_terrain.png, кадр просмотрен.
- Переключение меню Classic → Focus → Smart отработало, Move виден на кадрах.
- В Smart Terrain Move сохранился, отображаются генерация, кисти и слои.
- Контраст и читаемость четырёх кадров сверены с обзором ArcStudio_UI_3_Layouts.
- cargo check до исправления горячих клавиш завершился с кодом 0.
- Ctrl+Shift+1/2/3 повторно проверены через BRP с физическими клавишами. Скриншоты показывают Classic, Focus и Smart.
- Просмотрены шесть кадров `docs/screenshots/layout_{classic,focus,smart}_{1280x720,1920x1080}.png`. Размеры PNG подтверждены.
- На обоих размерах Focus показывает нижний док и вертикальные инструменты, Smart показывает Terrain и Move одновременно.
- В запущенном редакторе проверена запись активной раскладки в `interface.json`: Smart → Focus → Smart через BRP. Исходная раскладка восстановлена.
- Изолированный запуск с сохранённым профилем восстановил Focus и открытый Inspector с контекстом Global Settings. Кадр `docs/screenshots/layout_focus_profile_launch.png` просмотрен.
- Reset Current Layout через меню восстановил дефолты Focus (open, pinned, context и высота 176). Сравнение JSON подтвердило сохранение Classic и Smart. BRP-кадр `docs/screenshots/layout_focus_profile_reset.png` просмотрен.
- Reset All Layouts в изолированном экземпляре восстановил Classic, Workspace Scene, видимость и закрепления панелей, высоты 176 во всех трёх профилях. BRP-кадр `docs/screenshots/layout_reset_all.png` просмотрен.
- В Focus закрепление закрытого Inspector оставило его закрытым. Цикл дока открыть → закрыть → открыть сохранил `pinned=true` в JSON. BRP-кадр `docs/screenshots/layout_focus_pinned_reopen.png` просмотрен.
- В Focus `B` активировал кисть в обоих случаях: закреплённый Inspector сохранил Objects & Spawner, откреплённый переключился на Terrain Brushes. Оба BRP-кадра `docs/screenshots/layout_focus_{pinned,unpinned}_brush.png` просмотрены.
- В Focus кнопка Assets подсвечивается при открытом браузере и теряет акцент после закрытия. JSON `open` совпадает с состоянием на обоих просмотренных BRP-кадрах `docs/screenshots/layout_focus_assets_{True,False}.png`.
- Smart: переход Scene → Terrain сохранил Move. Нажатие `B` активировало Terrain Brush при той же вкладке Terrain. Просмотрены BRP-кадры `docs/screenshots/layout_smart_accept_terrain_{move,brush}.png`.
- Цикл Classic → Focus → Smart → Classic с выделенным CampfireSite #1684 и Move прошёл. Выделение и гизмо видны на четырёх просмотренных BRP-кадрах `docs/screenshots/layout_selected_cycle_{0,1,2,3}.png`. Повторный цикл через BRP подтвердил тот же идентификатор камеры и точное равенство Transform на каждом шаге.
- `cargo check --offline --message-format=short -q` после правки повторного сохранения завершился с кодом 0.
- BRP подтвердил одинаковые имена и ID 1736 именованных сущностей с Transform во всех трёх раскладках. Сравнение всех Transform включает анимированные сущности, поэтому эту проверку учитываем только как проверку идентичности сущностей.
- Classic: `+ Duck` добавил вторую утку. После переключения в Focus BRP подтвердил для новой сущности `Inherited → Hidden → Inherited` при Ctrl+Z/Ctrl+Y, исходная утка сохранила видимость.
- Smart Terrain: F5 показал игровой HUD, Shift+F5 восстановил панели и статус `pre-play scene restored`. Просмотрены `docs/screenshots/layout_pie_{playing,stopped}.png`. Отдельный BRP-прогон подтвердил откат правки Transform тестовой Duck: X 19.9221725463867 → 80 → 19.9221725463867 при том же ID.
- Separate Game из Smart запустил дочерний процесс для `scenes/behavior_pilot.scn.json`. BRP на 15704 вернул 22 именованные сущности. Панель подтверждена просмотренным кадром `docs/screenshots/layout_separate_smart.png`.
- Separate Game остановлен кнопкой Stop, дочерний процесс завершился. Команды SaveScene/LoadScene через BRP восстановили исходные координаты обеих Duck после изменения одной из них. Использован отдельный JSON в `target/layout-acceptance/`.
- Первый прогон тестов выявил устаревшее ожидание списка панелей World. После добавления `terrain_settings` в ожидаемый список повторный `cargo test --offline --message-format=short -q --lib` прошёл: 167 тестов.
- Stop-хук для актуального кода: tmpmjcmupl_.
- Baseline взят из архивных BRP-кадров непосредственного предшественника `9c15c1c`: `docs/screenshots/i2_environment_restored.png` и `docs/screenshots/i2_ultra_safe.png`. Оба повторно просмотрены.
- После запуска с записанным UI-конфигом восстановлены активная Smart Scene, Node Graph и различающаяся видимость доков Classic и Focus. Отдельный запуск с профилем Focus восстановил закреплённый Terrain Brushes при активном Move. Просмотрены `docs/screenshots/layout_profile_restarted.png` и `docs/screenshots/layout_profile_pin_restored.png`.
- Smart Scene → Node Graph → Add node добавил Fireball #7. Через BRP подтверждены неизменность полного графа при Classic → Focus → Smart → Classic и точное восстановление исходного JSON после Undo. Просмотрены `docs/screenshots/layout_graph_ui_added.png` и заголовки кадров `layout_graph_preserved_*.png`.
- Stop-хук `tmp_yihyhxo` завершился по таймауту отсутствия вывода. Отдельный прогон тестов раскладок запущен в сессии `61654`.
- Открытые сценарии перечислены в [матрице приёмки](../multi_layout_matrix.md).
- Повторный Stop-хук `tmplgpoi042` остановился после 600 секунд без вывода. Прямой `cargo check --offline --message-format=short -q` завершился с кодом 0. Полный `cargo test --offline --message-format=short -q` в сессии `39137` завершился ошибкой линковки bevy_dylib. Тестовый редактор удерживал DLL. После его закрытия повторный прогон `14913` прошёл линковку этой DLL и начал компилировать проект.
- Smart и Classic сохранили отдельные тестовые JSON. После установки X существующей утки в 80 через BRP команда File → Load Scene восстановила X=41.350193 в обеих раскладках. Путь тестовых файлов задан через BRP. Статусы и названия раскладок сверены на кадрах `docs/screenshots/layout_scene_roundtrip_smart.png` и `docs/screenshots/layout_scene_roundtrip_classic_verified.png`.

## Сводка на паузе — 2026-11-11

### Сделано

- Реализованы Classic, Focus и Smart с общим реестром панелей, меню, палитрой команд и переключением через Ctrl+Shift+1/2/3.
- Проверены независимые настройки доков, сброс профилей и восстановление закреплённого Terrain Brushes в Focus после запуска.
- Подтверждены сохранение и загрузка сцены, запуск и остановка PIE и Separate Game во всех трёх раскладках. В Smart подтверждён откат Transform после PIE.
- Проверены rename, reparent, кисть и Undo/Redo. В Smart проверены Build, Water Bodies, префабы, материалы, Quillshade и Node Graph.
- Последний прогон подтвердил выбор CampfireSite кликом в Smart. Гизмо изменило Y с 13.668392 до 14.921064, Undo точно восстановил Transform. Оба BRP-кадра просмотрены: [выбор](../../screenshots/layout_smart_pick_verified.png), [перемещение](../../screenshots/layout_smart_gizmo_verified.png).
- Компиляция прошла. Повторный полный прогон Cargo завершился с кодом 0 после закрытия тестового редактора. 171 тест прошёл, один doctest пропущен. Журнал в `target/layout_final_tests_retry.log`.
- Собран [ZIP с тремя BRP-кадрами 1600×900](../../screenshots/ArcStudio_3_Layouts.zip). Также сняты кадры раскладок в 1280×720 и 1920×1080.

### Осталось

- Начать с живого редактирования компонента через Inspector. Сверить новое значение через BRP и подтвердить Undo.
- Проверить захват клика, drag, колеса и клавиатуры над остальными областями UI, включая палитру, Scene Tree и Assets.
- Подтвердить отсутствие повторной загрузки ассетов при смене раскладки по событиям загрузчика. BRP подтвердил стабильные ID и имена 1682 объектов с мешами.
- Сверить результат теста восстановления закреплённой панели отключаемого модуля и закрыть оставшиеся строки [матрицы приёмки](../multi_layout_matrix.md).
- После закрытия матрицы оформить итоговые журналы этапов и обновить общую память. Статус цели — пауза. Oracle PASS ожидает полной приёмки.
