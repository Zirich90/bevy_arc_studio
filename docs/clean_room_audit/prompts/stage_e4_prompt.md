# Stage E4 prompt (summary of user request)
Этап E4: LevelBuilderPlugin — стены, полы, двери, лестницы, этажи, Grid Snap (src/level_builder.rs).
- GridSettings{enabled,cell_size=2,angle_snap_deg=15,floor_level,floor_height=3.5}; снап в gizmo_3d (Move X/Z, Rotate); floor_level поднимает рабочую плоскость.
- Reflect-компоненты LevelTile{kind,grid_pos,facing,style}, Door{is_open,is_locked}; меши/PBR для 3 стилей; факел с PointLight.
- Инструмент Level Builder (L) + вкладка: Room Box, Wall Line, Floor Brush/Eraser, Place Door/Stairs/Pillar/Torch.
- StudioCommand: BuildRoom, BuildWallLine, PlaceLevelTile, ToggleDoor; один Undo откатывает комнату.
- Навигация: стены/колонны/закрытые двери блокируют, открытые двери и полы проходимы.
- Приёмка: юнит-тесты (Room Box+Undo, Door+nav), cargo check/test, скриншот docs/screenshots/e4_level_builder.png, лог stage_e4_log.md PASS.
