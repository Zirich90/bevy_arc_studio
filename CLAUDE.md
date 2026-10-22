# Project Rules: Bevy ArcStudio (Clean-Room AI Visual Generator & Super GUI on Bevy 0.20.0)

Ты — главный системный архитектор проекта **Bevy ArcStudio** (`MIT OR Apache-2.0`).
Мы создаем нативный генератор 3D-окружения и визуальный редактор сцен для **Bevy 0.20.0** с полной поддержкой управления через ИИ-агентов по протоколу `bevy_remote` (BRP MCP).

## 1. РЕЖИМ ЖЕСТКОЙ ЭКОНОМИИ ТОКЕНОВ (TOKEN DIET)
1. **Запуск компилятора:** КАТЕГОРИЧЕСКИ ЗАПРЕЩЕНО запускать обычный `cargo build` или `cargo check` без флагов. Всегда проверяй код только сжатой командой:
   `cargo check --message-format=short -q`
   А тесты запускай через:
   `cargo test --message-format=short -q`
2. **Никакой воды в ответах и логах:** Пиши только рабочий код и сухие технические сводки (10–15 строк в лог этапа). Не пересказывай код в чате.
3. **Точечное чтение файлов:** Не читай файлы целиком, если нужно поправить одну функцию.

## 2. ЖЕСТКО ЗАФИКСИРОВАННЫЙ СТЕК И API BEVY 0.20.0 + BEVY_EGUI 0.43.1

### Точный `Cargo.toml` (НЕ МЕНЯТЬ ВЕРСИИ):
```toml
[package]
name = "bevy_arc_studio"
version = "0.1.0"
edition = "2024"
resolver = "3"
license = "MIT OR Apache-2.0"
description = "AI-First Procedural 3D Environment Studio & Super GUI for Bevy 0.20"

[dependencies]
bevy = { version = "0.20.0", features = [
    "dynamic_linking",
    "bevy_remote",
    "bevy_feathers",
    "free_camera",
    "pan_orbit_camera",
] }
bevy_egui = "0.43.1"
bevy_brp_extras = "0.20.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
noise = "0.9"
rand = "0.8"

[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3

Обязательные правила API Bevy 0.20.0 (Защита от устаревшего синтаксиса 0.14–0.19):

    Свет, Атмосфера и Объемный Туман лежат СТРОГО в bevy::light:
    use bevy::light::{Atmosphere, AtmosphereEnvironmentMapLight, SunDisk, VolumetricFog, VolumetricLight, FogVolume, DirectionalLight, PointLight, SpotLight, GlobalAmbientLight, CascadeShadowConfigBuilder, ShadowFilteringMethod};

    PBR и Отражения:
    use bevy::pbr::{StandardMaterial, ExtendedMaterial, ScreenSpaceReflections, ScreenSpaceTransmission, DistanceFog, FogFalloff};

    Постпроцессинг:
    use bevy::post_process::bloom::Bloom;
    use bevy::core_pipeline::tonemapping::Tonemapping;

    3D Picking (клик мышью по объектам):
    Используй встроенный в Bevy 0.20 MeshPickingPlugin (use bevy::picking::prelude::*;).

    3D-Гизмо и линии:
    Используй встроенный Gizmos (use bevy::gizmos::prelude::*;).

    Правило bevy_egui 0.43.1 (КРИТИЧЕСКИ ВАЖНО!):
    Подключай EguiPlugin::default(). Все UI-системы egui регистрируй СТРОГО в расписании EguiPrimaryContextPass (НЕ в Update!):
    code Rust

    use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
    app.add_plugins(EguiPlugin::default())
       .add_systems(EguiPrimaryContextPass, studio_ui_system);

    Внутри системы получай контекст через: let Ok(ctx) = contexts.ctx_mut() else { return; };.

    Удаленное управление для ИИ и скриншоты:
    Подключай bevy_brp_extras::BrpExtrasPlugin::default() (поднимает BRP на порту 15702 и дает метод brp_extras/screenshot).

3. АРХИТЕКТУРНОЕ ЯДРО src/arc_core.rs

Вся чистая математика (паспорт мира WorldSpec, генерация сетки высот с Domain Warping и капельной гидравлической эрозией, расчет весов 5-слойного Splatmap, Poisson-Disk рассадка объектов, аналитический рейкаст луча в карту высот, тригонометрия камеры и 3D-гизмо, транзакционный стек UndoHistory) должна быть сосредоточена в чистом модуле src/arc_core.rs, чтобы она не зависела от ECS-систем рендера и легко тестировалась юнит-тестами.
4. ПРОТОКОЛ АУДИТА ЧИСТОЙ КОМНАТЫ И МЕХАНИЧЕСКОГО СУДЬИ (ORACLE)

    В начале каждого этапа сохраняй текст промпта в docs/clean_room_audit/prompts/stage_0X_prompt.md.

    В конце каждого этапа записывай краткую техническую сводку (15–20 строк: таймстамп, файлы, статус cargo check, пути к проверенным скриншотам и вердикт PASS) в docs/clean_room_audit/logs/stage_0X_log.md.

    Этап считается завершенным ТОЛЬКО после того, как ты снял скриншот через brp_extras/screenshot в папку docs/screenshots/, просмотрел его глазами и убедился в отсутствии визуальных и версточных багов.