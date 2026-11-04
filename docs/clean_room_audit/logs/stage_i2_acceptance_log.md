# I2 — итоговая сверка, 2026-11-04

Критерии взяты из §10 плана редактора и раздела I2 контракта интеграции.

| Критерий | Свидетельство и остаток проверки |
|---|---|
| Native canvas и Hot Reload | I2.01–02 и I2.08. Drag/connect/delete, история, оба режима и baked Hot Reload. |
| Typed parameters и независимые экземпляры | I2.02. Числовые f32/Vec4, layout guards, независимые handles, rebasing и prefab member overrides. |
| Surface slots | I2.03 water/terrain. Проверены master, шесть слоёв и global/per-basin. Дополнительный прогон `verify_material_parameters_brp.ps1` подтвердил назначение, удаление, сохранение и восстановление override тайла на render mesh. |
| Окружение | I2.04. Сохранение перехода, Undo/Redo, PIE, возврат исходного освещения. |
| Noise, erosion, Scatter | I2.05. Проверки evaluator и BRP scene round-trip. |
| CPU физика | I2.06–07. Снег и лёд представлены общей твёрдой фазой воды, описанной в контракте. Тесты энтальпии, переноса лавы и локальной теплоты плавления. Свежие BRP-прогоны балансов и Fireball прошли. |
| WorldEffect, collision, Theta* | I2.07. Свежий BRP подтверждает изменение маршрута, Pause, Stop и обновление сетки. Ветка `rebuild_nav` применяет локальный raster update при совпадении ревизии. |
| Graph UX | I2.08. Инспектор, меню, standalone RON и история layout. |

Последний кадр Fireball `docs/screenshots/i2_fireball_multiphase_overview.png` просмотрен.
Первый BRP-прогон тайла выявил panic при пропуске полей `facing` и `style` в `PlaceLevelTile`. Для обоих полей добавлен Reflect default. После пересборки сокращённый запрос, сохранение, загрузка и проверка render mesh прошли. Cargo check workspace/tests завершился с кодом 0.
Полный `cargo test --message-format=short -q --workspace -j 1` после исправления завершился с кодом 0: 239 тестов и 4 doctest прошли, 1 doctest пропущен. Журнал `target/i2_tile_defaults_tests.log`.
Итоговый вердикт PASS. Следующий продуктовый этап по плану — E7.
