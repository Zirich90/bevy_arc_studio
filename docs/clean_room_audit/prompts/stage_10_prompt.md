# Stage 10 — universal Reflect inspector + MCP coverage

User request (RU): "Давай делаем" — implement, in this order of priority from the agreed plan:
1. Universal Reflect inspector: show and edit any registered Reflect component of the selected entity (no per-type UI code).
2. Audit that every editor function is reachable through MCP (StudioCommand / BRP); close the gaps with typed commands
   (gizmo mode/snap/align, transform edit, select by id/multi, duplicate, WorldSpec field set, scene listing).

Constraints: stack and versions fixed by CLAUDE.md; compile checks only with `cargo check --message-format=short -q`;
screenshot via brp_extras/screenshot into docs/screenshots/ before PASS.
