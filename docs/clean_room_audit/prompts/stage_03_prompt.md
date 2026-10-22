# Stage 03 prompt (verbatim summary)

Этап 3: Интерактивный 3D-Манипулятор (Gizmos), Surface Snapping и Выделение Объектов.
Прочитать CLAUDE.md, docs/architecture/02_3d_gizmo_projection.md, 03_surface_snapping_and_picking.md, src/arc_core.rs.

1. `src/selection.rs`: клик ЛКМ по мешу (MeshPickingPlugin) -> подъём по ChildOf до корневого SceneObject -> ресурс
   `EditorSelection { primary: Option<Entity>, selected: HashSet<Entity> }`. 2D Marquee (Shift = union, Ctrl = toggle).
   Вокруг выбранных: светящийся Wireframe AABB с угловыми скобками через Gizmos.
2. `src/gizmo_3d.rs`: Q/W/E/R = Select/Move/Rotate/Scale, World/Local. Размер через calc_handle_size.
   Move: 3 оси, 3 плоскости (XZ, XY, YZ), центр; axis_drag_delta / plane_drag_point + grab_offset.
   Surface Snapping (Shift или тумблер Snap to Ground): HeightfieldGrid::raycast, Align to Normal.
   Rotate: 3 кольца, atan2, шаг 15° на Ctrl. Scale: ручки масштаба.
   Перетаскивание, Ctrl+D (дубликат), Delete -> транзакционный UndoHistory (Ctrl+Z / Ctrl+Y).

Oracle: `cargo check --message-format=short -q` = 0 ошибок; программно выделить CampfireSite в режиме Move,
скриншот docs/screenshots/stage_03_gizmo.png через brp_extras/screenshot, осмотреть; лог docs/clean_room_audit/logs/stage_03_log.md.
