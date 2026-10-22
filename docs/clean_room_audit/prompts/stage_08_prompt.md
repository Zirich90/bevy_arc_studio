# Этап 08: arc_rules — Изолированный опциональный модуль правил Pathfinder 2e (за фиче-флагом `pf2e`)

Прочитать `CLAUDE.md` и `docs/architecture/06_tactical_rpg_framework.md`.

## Главное архитектурное требование
Базовый редактор `bevy_arc_studio` остаётся чистой 3D-студией без RPG-зависимостей. Модуль правил ПОЛНОСТЬЮ ИЗОЛИРОВАН:
1. `Cargo.toml`: `[features] default = []`, `pf2e = []`.
2. Код правил в `src/rules/` под `#[cfg(feature = "pf2e")]`.
3. `cargo check --message-format=short -q` собирается с 0 строк RPG-кода.
4. Модуль не зависит от Bevy — чистая детерминированная математика на Rust с `rand::SeedableRng`.

## Задачи (`#[cfg(feature = "pf2e")]`)
1. Проверки d20 и 4 степени успеха: d20 + mod против DC; CriticalSuccess (>= +10), Success, Failure, CriticalFailure (<= -10). Nat 20 повышает на 1 ступень, Nat 1 понижает на 1. Детерминированный seed-генератор.
2. Владение и 6 атрибутов: Str, Dex, Con, Int, Wis, Cha (mod = (score-10)/2). Ранги: Untrained +0, Trained +2+lvl, Expert +4+lvl, Master +6+lvl, Legendary +8+lvl.
3. Защита, здоровье, состояния: AC, спасброски (Fort, Ref, Will), HP, Dying 1..4, Wounded. Frightened(u8) (штраф ко всем чекам, -1 в конце хода), OffGuard (-2 AC), Prone (-2 атаки, Off-Guard).
4. Экономика 3 действий и MAP: TurnActionEconomy (3 действия + 1 реакция). MAP: 0, -5 (-4 agile), -10 (-8 agile). Stride тратит 1 действие в пределах Speed (25 ft).
5. Шаблоны: Fighter 1 ур., Goblin warrior, Longsword 1d8, Dagger 1d4 agile.

## Oracle
1. `cargo check --message-format=short -q` чисто.
2. `cargo test --features pf2e --message-format=short -q` зелёные.
3. Лог в `docs/clean_room_audit/logs/stage_08_log.md`.
