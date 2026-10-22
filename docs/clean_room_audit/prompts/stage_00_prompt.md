# Stage 0 prompt (saved verbatim in summary form)

Выполни **Этап 0: Clean-Room экстракция эталонной математики в `docs/architecture/`**.

Роль: **Агент-Аналитик («Грязная комната»)**. НЕ ПИСАТЬ код проекта на Rust.

## Шаг 1. Скачать ровно 10 файлов донора в `.tmp_donor_ref/` (curl, без клонирования)
SceneViewMotion.cs, SceneView.cs, RectSelection.cs, SceneViewPicking.cs, HandleUtility.cs,
Slider1D.cs, Slider2D.cs, Disc.cs, PaintTreesTool.cs, TerrainPaintUtility.cs
(база: raw.githubusercontent.com/Unity-Technologies/UnityCsReference/master)

## Шаг 2. Создать 5 математических спецификаций в `docs/architecture/`
СТРОГОЕ ПРАВИЛО ЧИСТОЙ КОМНАТЫ: никакого C#-кода, брендов и имён классов донора; только формулы, тождества, пороги, таблицы переходов, сигнатуры линейной алгебры.

1. `01_viewport_camera_math.md` — орбита (yaw/pitch, clamp ±89°), Depth Pan, экспоненциальный зум и зум к точке на рельефе, свободный полёт (Shift-разгон, скорость колесом, затухание, pivot = eye + forward*distance), фокус F: target_distance = (r / sin(fov_y/2)).max(2).
2. `02_3d_gizmo_projection.md` — постоянный экранный размер, 1D axis drag (plane_normal, мёртвая зона 0.96, grab_offset), 2D plane drag (порог 0.08), диск вращения (atan2, шаг 15° с Ctrl).
3. `03_surface_snapping_and_picking.md` — ray-heightfield (марш + 10-шаговая дихотомия, 0.0005 м), snapping + выравнивание по нормали, marquee selection (Shift/Ctrl).
4. `04_procedural_terrain_and_splatmap.md` — рельеф долины (domain warping, горное кольцо, чаша озера, отмель, поляна, тропа), капельная + термальная эрозия, 5-слойный PBR splatmap, кисти и Poisson-Disk.
5. `05_transactional_undo_and_mcp_api.md` — транзакционный Undo/Redo (Begin -> UpdatePreview -> Commit), JSON-команды StudioCommandQueue через BRP.

## Шаг 3. Фиксация
SHA-256 всех 5 файлов + UTC-таймстамп -> `docs/clean_room_audit/CLEAN_ROOM_PROOF.md` и `logs/stage_00_log.md`; затем `rm -rf .tmp_donor_ref`.
