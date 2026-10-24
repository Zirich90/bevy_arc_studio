# Лог этапа E0: базовая линия

- Дата: 2026-10-24.
- Файлы: `docs/clean_room_audit/prompts/stage_e0_prompt.md`, `docs/architecture/08_studio_commands.md`, `.github/workflows/ci.yml`.
- Решения по умолчанию: цель — редактор под игру с пилотом; формат сцены `DynamicWorld`; `dynamic_linking` не менялся; Jackdaw не пробовали.
- `cargo check --message-format=short -q`: OK (exit 0).
- `cargo test --message-format=short -q`: OK, 28 тестов (26 + 2), 0 провалов.
- CI: матрица ubuntu/windows/macos, `fail-fast: false`, apt-зависимости только для Linux. Локально не запускалась; зелёный CI на трёх ОС проверяем только после пуша.
- Падение при старте (`Fallback system failed to choose present mode ... Options: []`): Vulkan на AMD RX 9060 XT возвращает пустой список present_modes. Обход: `WGPU_BACKEND=dx12`, без `-WindowStyle Minimized`.
- Снимок: `docs/screenshots/e0_baseline.png` снят через `brp_extras/screenshot` (первый кадр сразу после старта был чёрным, второй через ~20 с — сцена загружена).
- Просмотр глазами: тулбар, HUD (FPS/ENT/PROPS/LIGHTS), панель параметров справа, сцена (лес, озеро, костёр) — без визуальных и вёрсточных багов.
- Вердикт: PASS (локальная часть). Приёмка «зелёный CI на трёх ОС» ожидает пуша.
