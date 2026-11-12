# Stage 13 — panel opacity + neat sliders (log)

Timestamp: 2026-11-12 ~10:45Z. Verdict: PASS (this substage only).

Changes:
- src/ui.rs: `panel_fill(opacity_pct)` helper (PANEL_BG at alpha); `apply_style(ctx, opacity)`
  now sets faint_bg_color/panel_fill/window_fill to that fill; slider optimum =
  `handle_shape=Circle`, `interact_size.y 30→20`, `slider_rail_height=4`; `styled` Local became
  `Option<u8>` so style re-applies live when opacity changes.
- src/ui/layout.rs: persisted `Preferences.panel_opacity` (u8 percent, serde default 100) +
  LayoutState field/accessors `panel_opacity/set_panel_opacity/panel_fill`; save_if_dirty covers it.
- Frames switched from opaque PANEL_BG to `d.layout.panel_fill()`: ui_inspector (draw + draw_docks),
  ui_outliner, ui_assets (browser + collapsed_strip).
- src/ui_toolbar.rs: Settings menu gains "Panel opacity / Непрозрачность панелей" slider 0..=100 %.
- Tests: +`panel_opacity_defaults_opaque_and_maps_to_alpha`; roundtrip test sets panel_opacity=60.

Checks: `cargo check --message-format=short -q` PASS (no output);
`cargo test --message-format=short -q` 176 passed, 1 doctest ignored.

BRP screenshots captured AND viewed (1600x900, temp APPDATA prefs so real prefs untouched):
- docs/screenshots/ui_sliders_opaque.png — opacity 100: round handles + thin teal rails, neat.
- docs/screenshots/ui_panels_transparent.png — opacity 45: 3D shows through left/right/bottom panels.
- docs/screenshots/ui_settings_opacity_slider.png — Settings menu shows the slider at 45%.
- docs/screenshots/ui_opacity_live_drag.png — drag 45→100% updates panels live, menu stays open.

LIVE: editor shut down via BRP clean_shutdown (PID 17724); no live handles. No commit/push.
Deferred (unchanged): drag-coalescing helper for time/Transform sliders; PaintPines/EraseProps.