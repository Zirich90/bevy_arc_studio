# Stage E9 prompt — Modules tab (hot-connectable modules)

User request (2026-10-24): add a "Modules" tab so modules (e.g. pf2e) can be connected from the editor UI at runtime, plus an on/off switch for already connected modules.

Correction (same day): connecting = finding the module file on disk. A `modules/<id>/module.json` the editor sees is already connected, the tab only does on/off; plus Rescan and Add-from-path. Chosen with the user: manifest `module.json` (not dll), scan at start + Rescan button.

Decision: modules stay compiled in (Rust has no stable ABI for dynamic loading); "connect" adds a module to the project (persisted in `assets/modules.json`), "on/off" toggles it. Contributions to `EditorRegistry`/`SpawnKinds`/`CustomCommands` are owner-tagged and filtered; module systems gate on `module_active`; activate/deactivate hooks reset module state. PF2e becomes the first module; the stock app lives in `crates/arc_studio_app` so it can link both the editor and PF2e.
