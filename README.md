# Bevy ArcStudio

**3D environment studio, procedural terrain generator & level editor for Bevy 0.20**  
`MIT OR Apache-2.0` · [Русское описание ниже](#-русский)

![Hero — misty dusk lake and procedural pines](docs/images/01_hero_dusk.png)

**Bevy ArcStudio** is a lightweight 3D world-building studio and scene editor built on **Bevy 0.20** and `bevy_egui`. Originally created to build levels for an upcoming RPG, it combines procedural terrain generation (with droplet hydraulic erosion) and manual scene editing with a remote command protocol (`bevy_remote`) so scenes can be built both by hand and via AI agents (such as Claude).

---

## Highlights

- **Procedural Terrain & Erosion:** Domain-warped noise, droplet hydraulic erosion, thermal relaxation, 5-layer automatic splatmap (shoreline sand, grass, dirt, rock, snow), and Poisson-disk prop scattering.
- **Atmosphere & Environment:** Dynamic sun cycle, sky & volumetric fog, lake water plane with shore foam, bloom, plus instant world presets (`dusk`, `noon`, `night`, `dawn`) and quality tiers (`low`, `med`, `high`, `ultra`).
- **Interactive 3D Editor:**
  - 3D **Move / Rotate / Scale** gizmo with ray-plane & ray-circle intersection math.
  - **Surface Snapping:** Raycast against the heightfield with analytical normal alignment.
  - **6 Terrain Brushes:** Raise, Lower, Smooth, Flatten, Noise, and local Erosion.
  - **Scene Management:** `.glb` asset import, entity inspector, and reflection-based save/load (`.scn.json` + binary `.heights`), easily extensible via `register_scene_component::<T>()`.
  - **Transactional Undo/Redo:** Unified history stack covering world settings, object transforms, and terrain brush strokes.
- **AI & Remote Control Ready (`bevy_remote`):** Every editor action is a `StudioCommand` exposed over Bevy Remote Protocol (BRP) alongside `brp_extras/screenshot`. You can connect an LLM agent (like Claude Code) to inspect, sculpt, populate, and screenshot levels live. See [`SKILL.md`](SKILL.md) for the protocol specification.

| Editor UI (`bevy_egui` + 3D gizmo) | Sunny Noon Preset | Midnight Camp Preset |
| :---: | :---: | :---: |
| ![Editor UI](docs/images/02_editor_ui.png) | ![Noon](docs/images/03_noon_lake.png) | ![Night](docs/images/04_night_camp.png) |

---

## Quick Start

Requires **Rust 1.85+** (2024 edition) and a GPU supporting Vulkan, Metal, or DX12.

```bash
# Run in release mode (recommended for smooth hydraulic erosion & rendering)
cargo run --release

# Launch with specific world & quality presets
cargo run --release -- --preset noon --quality high --seed 42

# Run unit tests for the math & terrain core
cargo test
```

### CLI Options

| Flag | Values | Default |
| :--- | :--- | :--- |
| `--preset <NAME>` | `dusk`, `noon`, `night`, `dawn` | `dusk` |
| `--quality <TIER>` | `low`, `med`, `high`, `ultra` | `high` |
| `--seed <U32>` | Any `u32` integer | `1337` |
| `--load <PATH>` | Path to `.scn.json` | — |
| `--headless-smoke` | Run 5 ticks and exit (CI check) | off |

### Controls & Hotkeys

| Input | Action |
| :--- | :--- |
| `Tab` | Toggle **Editor** / **Play** mode (hides UI) |
| `Q` / `W` / `E` / `R` | Select / Move / Rotate / Scale gizmo modes |
| `F` / `Delete` | Focus camera on selected / Delete selected entity |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo (settings, objects, terrain strokes) |
| `F5` / `F9` | Quick-save / Quick-load (`scenes/arc_scene.json`) |
| `F12` | Save clean screenshot to `screenshots/` |
| `RMB + WASD/QE` | Fly camera (`Shift` for speed boost) |
| `Alt + LMB` / `MMB` / `Wheel` | Orbit / Pan / Zoom camera |

---

## Architecture & Layout

The core mathematical routines are decoupled from ECS rendering into a pure, unit-tested module so you can easily reuse or adapt them in your own Bevy projects:

- `src/arc_core.rs` — Pure math & algorithms: value/FBM noise, domain warping, droplet hydraulic erosion, splatmap weights, Poisson disk sampling, gizmo ray geometry, surface snap, `UnitScale` (feet ↔ meters), and the `UndoStack` state machine.
- `src/main.rs` — Bevy 0.20 application, PBR materials, water & sky setup, 3D gizmos, `bevy_egui` panels, and the `StudioCommand` remote dispatcher.
- `SKILL.md` — Documentation for controlling the editor externally over JSON-RPC (`bevy_remote` on `127.0.0.1:15702`).
- `docs/clean_room_audit/` — Mathematical specifications (`01`–`05`) and design notes used to build the core systems.

---

## Project Status & Contributing

**Bevy ArcStudio** is being developed alongside my own upcoming RPG. My primary focus is building the game and evolving this engine/editor to fit its needs, so I cannot promise a strict release schedule or full-time support.

However, **forks, issues, and Pull Requests are very welcome!** I will gladly review and merge useful community contributions and improvements whenever time permits. Feel free to use this repository as a foundation for your own Bevy tools or games.

---

## 🇷🇺 Русский

**Bevy ArcStudio** — 3D-студия окружения, генератор процедурного ландшафта и редактор уровней на **Bevy 0.20** (`MIT OR Apache-2.0`).

Проект создавался как рабочий инструмент для разработки уровней моей будущей RPG. Он сочетает ручное редактирование сцены с процедурной генерацией и поддержкой протокола `bevy_remote` (BRP) — это позволяет собирать локации как вручную мышкой, так и с помощью ИИ-агентов (например, Claude), которые могут отправлять команды в запущенный редактор и видеть результат через скриншоты.

### Основные возможности
- **Террейн и эрозия:** FBM-шум с доменным искажением, капельная гидравлическая эрозия, 5-слойный сплатмап (песок, трава, грунт, скалы, снег) и рассадка объектов по алгоритму диска Пуассона.
- **Атмосфера и вода:** динамический цикл солнца, туман, вода с береговой пеной, bloom, 4 пресета времени суток (`dusk`, `noon`, `night`, `dawn`) и пресеты графики (`low`..`ultra`).
- **Редактор:** 3D-гизмо (перемещение, вращение, масштаб), прилипание объектов к рельефу по нормали (Surface Snap), 6 скульптурных кистей для террейна, импорт `.glb`, сохранение и загрузка сцен через `Reflect` (`.scn.json` + `.heights`), а также единый стек `Undo/Redo`.
- **Управление через ИИ (`bevy_remote`):** каждое действие в редакторе представлено как `StudioCommand`. Подробности подключения агентов — в [`SKILL.md`](SKILL.md).

### Статус проекта и участие
В первую очередь я развиваю этот инструмент под задачи своей игры, поэтому уделяю репозиторию время по мере возможности. Тем не менее, я открыт к идеям сообщества: **форки и пулл-реквесты приветствуются** — буду рад смотреть и вливать полезные доработки по мере свободного времени!

## License

Dual-licensed under **MIT** or **Apache-2.0**, at your option.
