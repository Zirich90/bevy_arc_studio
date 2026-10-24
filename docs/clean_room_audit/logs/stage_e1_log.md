# Лог этапа E1: единая история отката

- Дата: 2026-10-24.
- Файлы: `src/operators.rs`, `src/studio.rs`, `src/arc_core.rs`, `docs/architecture/08_studio_commands.md`.
- Уже было: `UndoOrder` (глобальный хронологический порядок над четырьмя историями World/Scene/Terrain/Comp). Сохранён; сведение в один тип `EditorCommand` отложено (риск без выигрыша для приёмки).
- `Reparent` откатывается: `CompSnap.also` — группа компонентов (`ChildOf` + `Transform`) в одной записи истории, `apply_snap` восстанавливает группу целиком.
- Исправлено: новая правка в любой истории раньше не сбрасывала redo остальных историй (redo мог воспроизвести правку не по порядку). Теперь `UndoOrder::sync` возвращает «свежая правка» и `track_order` вызывает `clear_redo()` у всех историй.
- Логика порядка вынесена в `UndoOrder::sync` (чистая функция) и покрыта тестом.
- `cargo check --message-format=short -q`: OK. `cargo test --message-format=short -q`: OK, 30 тестов (+2: `mixed_chain_undoes_in_reverse_and_redoes_forward`, `reparent_undo_restores_parent_and_local_transform`).
- BRP-сценарий (живой редактор, dx12): SetTransform → Reparent(в корень) → Undo (вернулся родитель CampfireSite) → Undo (вернулась позиция) → Redo ×2 (снова в корне и перемещён). Порядок верный.
- Известное ограничение: `SetSpec`, пересоздающий пропсы (напр. `lake_radius`), убивает сущности; записи истории на них (`CompSnap`) становятся пустыми. Цель E1.1/E3: стабильные id вместо `Entity` в снимках.
- Снимок: `docs/screenshots/e1_unified_history.png`, просмотрен: панели и HUD без артефактов, счётчик Undo/Redo (5/0) единый.
- Вердикт: PASS.
