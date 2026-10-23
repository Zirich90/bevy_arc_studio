# CLEAN_ROOM_PROOF

UTC: 2026-10-22T08:36:32Z
Algorithm: SHA-256

| file | sha256 |
|---|---|
| 01_viewport_camera_math.md | `318d1b7e1c762006336d7fd84d1f0dd0db7edf76ec4a351a63147f4f05d9ef56` |
| 02_3d_gizmo_projection.md | `3a1779ca2589dd3814b510a5f0e07a36f2d6d0c22ad4c3fff7e5ab706f34f46d` |
| 03_surface_snapping_and_picking.md | `6108cbf482a41b7c0ac462c86fd22a9bac23e7a9231536cbeb1ed0b2fc58de91` |
| 04_procedural_terrain_and_splatmap.md | `57fb2fddc55401982a0c8239d8f346b65665a556085a907b026fd6a3fddfdb7a` |
| 05_transactional_undo_and_mcp_api.md | `b8e698dc1008344f84d369a496d0b4de95f5ddea74b742d2b9dab5da64be41cc` |

Donor temp folder `.tmp_donor_ref` removed after hashing.

## Redaction of the Stage 0 transcript (2026-10-24)

`raw_transcripts/stage_00_session.jsonl` originally contained raw donor C# reference text inside tool output (7 strings, 27021 bytes). These were replaced by one-line stubs carrying the byte length and SHA-256 of each omitted string, by `redact_stage00.py` in this folder. Prompts, timestamps, model reasoning, tool calls and the writes of `docs/architecture/*.md` are untouched. The original is kept privately outside the repository (original SHA-256 `9e3ca876d783ea2b38f7cd779b417768a8080651c5997759b105b586bf6def69`). Git history was rewritten so the unredacted blob is no longer present.

Redacted transcript SHA-256: `2ae1ae7f805c799a472d604b04b8164b681a201da7fa743d9a8c1dbb2d6c2913`
