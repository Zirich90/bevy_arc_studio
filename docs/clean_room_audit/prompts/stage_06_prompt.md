# Stage 06 prompt

Утверждён план docs/architecture/06_tactical_rpg_framework.md целиком.

1. Сохранить документ в docs/architecture/06_tactical_rpg_framework.md.
2. Этап 06 (Шаг 0): переход на reflect-сцену DynamicScene / BSN с реестром компонентов.
   - Промпт сохранить в docs/clean_room_audit/prompts/stage_06_prompt.md.
   - Проверки компиляции: `cargo check --message-format=short -q`.
   - Тесты: `cargo test --message-format=short -q`.
   - Скриншот студии с сохранённой reflect-сценой через BRP; лог в docs/clean_room_audit/logs/stage_06_log.md.

Дополнение пользователя: перевод футов в метры через коэффициент: либо точный пересчёт 1:1 (0.3048 м/фт),
либо игровой режим, где 5 футов = 5 метров (1.0 м/фт). Реализовать как `arc_core::UnitScale`.
