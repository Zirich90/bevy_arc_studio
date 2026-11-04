# I2.08 Graph UX

Время проверки 2026-11-03T20:12:35.5236837+10:00.
Код редактора в src/node_graph.rs и src/ui.rs.
Команды автоматизации в src/mcp_commands.rs.
BRP-сценарий scripts/verify_graph_ux_brp.ps1.
Cargo check --workspace --tests прошёл, после него запущены тесты workspace.
Приложение собрано и запущено с Vulkan.
BRP-сессия 85738 завершилась с кодом 0.
Проверены типизированное редактирование Noise, Undo и Redo.
Добавление Scatter создаёт ноду с новым ID.
Auto Layout меняет координаты предварительно смещённой ноды.
Один Undo восстанавливает раскладку до Auto Layout.
Проверены сохранение и загрузка RON, миграция v1.
Скриншот docs/screenshots/i2_graph_ux.png снят через brp_extras/screenshot и просмотрен.
Canvas прозрачен, компактные ноды и связи видны вместе со сценой и HUD.
Раскрытый инспектор Noise показывает seed, frequency и warp.
Вердикт I2.08 PASS.
