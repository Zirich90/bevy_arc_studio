# I2.07 — промежуточная проверка
Дата: 2026-11-04, Asia/Vladivostok.
Статус: WIP. Итоговый вердикт после повторного BRP и просмотра снимка.
Вход: PF2e CombatInput::Fireball, Hero Wizard и Goblin Warrior.
Базовый BRP: успешный каст, тепло в CPU grid, Stop вернул поля.
Сценарий: scripts/verify_fireball_brp.ps1 -Reactions.
Подготовка: scripts/prepare_fireball_reactions.ps1 и sidecar .heights.
Начальные данные: 1 кг масла и 1 кг льда в разных ячейках.
Параметры сценария: радиус 1,5 м, воспламенение при 1 °C.
Theta* до каста: 3,8243 м, стоимость 14,8993 фута.
Theta* после каста: 4,2757 м, стоимость 17,9318 фута.
Откат сценария прервался паникой отложенной вставки Pickable.
Исправление: src/selection.rs, try_insert для Pickable и SpawnScale.
Сборка после исправления: сессия 72236, target/i2_reaction_retry_stderr.log.
В работе: повтор реакций и Stop, скриншот через BRP, просмотр, итоговые тесты.
## Повторный прогон
Сессия 11971, exit 0. Fireball принят через CombatInput.
Масло сгорело, лёд растаял, масса и энергия в заданных допусках.
Granular изменил высоты, CPU terrain и mesh совпали после обновления.
Theta*: 3,8243 → 4,0000 м, стоимость 14,8993 → 14,7638 фута.
Pause сохранил тик и поля. Stop вернул физические итоги и маршрут.
BRP снимок docs/screenshots/i2_fireball_reactions.png просмотрен.
Журнал боя читается, рельеф частично закрыт деревьями.
Итоговый Cargo и сверка критериев ещё в работе.

Workspace check with --workspace --tests passed.
Workspace tests passed with target/debug/deps and the rustc target-libdir prepended to PATH.
Includes 237 executable tests and four passing doctests, one ignored.
Without the Rust DLL directory, two Noodle doctests exited 0xc0000135 on Windows.
Evidence: target/i2_accept_with_rust_dll.log.
I2 remains open pending lava, fluid velocity, and MeltSnow parameter behavior.
<!-- Latest multiphase verification, 2026-11-04 -->
### Повторная проверка после изменений CPU backend

- Проверены локальный MeltSnow, воспламенение масла и Granular после успешного PF2e Fireball.
- `scripts/verify_fireball_brp.ps1 -Reactions` завершился с кодом 0.
- Длина маршрута Theta* изменилась с 3.8242645 до 4 м. Маршрут остался проходимым.
- Проверены баланс массы и энергии, ревизия рельефа, соответствие сетки, Pause и восстановление состояния и маршрута после Stop.
- BRP-снимок `docs/screenshots/i2_fireball_multiphase_final.png` просмотрен. Панель Separate game и деревья закрывают область реакции. Снимок подтверждает состояние Pause и журнал боя, визуальная приёмка области реакции открыта.
- Вердикт PASS для автоматических проверок сценария. Полная приёмка I2 открыта.

Повторный BRP-прогон с камерой над областью Fireball и без Scatter в тестовой сцене прошёл. Снимок docs/screenshots/i2_fireball_multiphase_overview.png просмотрен. Участок рельефа и актёр видны. Состав фаз и баланс подтверждены числовыми проверками BRP.
