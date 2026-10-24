# Showcase (snow castle, duelling knights) — log
- Files: src/showcase/{mod,knight}.rs, src/bin/showcase.rs, run_showcase.ps1, src/lighting.rs (exposure curve, low-sun, NightBoost), assets/showcase/*.
- Assets (CC0): Kenney Castle Kit 2.0, Kenney Nature Kit (kenney.nl). Knights are procedural (own code), not third-party models.
- Pipeline: `showcase record <dir>` = deterministic 30 fps frame dump 1920x1080 incl. egui HUD; ffmpeg H.264 -> docs/video/arcstudio_snow_castle.mp4 (14 s, ~8.7 MB).
- cargo check --message-format=short -q: clean.
- Screenshots checked by eye: docs/screenshots/showcase/final_{duel,dusk,night}.png.
- Verdict: PASS (visual pass by contact sheets; no brp_extras/screenshot used, frames captured via Bevy Screenshot).
