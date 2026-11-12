# Stage 13 — panel opacity + neat sliders (prompt)

Owner request (2026-11-12, with two reference screenshots: new = opaque panels + bulky
rounded-square slider handles / thick tracks; old = semi-transparent panels showing the 3D
scene through + small round handles / thin tracks):

> в старой версии интерфейса боковые панели и нижняя панель были прозрачными. давай добавим
> ползунок прозрачностей панелей. и еще в новой версии ползунки громоздкие, в старой они были
> меньше и аккуратнее. давай найдем оптимум

Scope:
1. Add a persisted panel-opacity slider affecting the side (Scene Tree, Inspector) and bottom
   (Asset Browser) panels, exposed in Settings.
2. Tune global slider sizing to a neat optimum between the bulky new look and the tiny old look.

Constraints: fixed Cargo.toml stack; pure math stays in arc_core.rs; no commit/push without
request; acceptance by eye via BRP screenshots.