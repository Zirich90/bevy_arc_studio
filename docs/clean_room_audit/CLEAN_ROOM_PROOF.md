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

## Process and boundary (facts supported by the recorded data)

- **Stage 0 (analysis)** ran 2026-10-22 08:34-08:38 UTC in session `d9322c13-...`. It downloaded 10 reference-only donor files into `.tmp_donor_ref/`, extracted formulas and algorithm descriptions into `docs/architecture/01..05.md` (a sixth spec, `06_tactical_rpg_framework.md`, was added later), hashed the specs (table above) and deleted `.tmp_donor_ref/`.
- **Stages 1-5 (Rust implementation)** ran as separate Claude Code sessions, each with its own session id and a later start time (`e72921ec` 08:40, `d0307e82` 09:21, `0aa1fa46` 10:18, `0f958162` 10:35, `8c334f8a` 10:54). Their recorded tool calls contain no access to `.tmp_donor_ref`, the donor repository or any `.cs` file; spec reads are of `docs/architecture/*.md` only (checked on the published transcripts `raw_transcripts/stage_01..05`).
- **What this does not claim.** The specs were produced inside this repository by the same author and model family as the later stages; there is no organizational separation between "dirty" and "clean" roles, only a process separation (new sessions, donor files deleted before Stage 1, code written against the specs). Stages 6-11 and the model-import work have logs but no raw transcripts in this repository.
- **Stage 0 raw transcript withheld.** `stage_00_session.jsonl` contains excerpts of the donor C# source (command output) and is therefore not published. It is kept in a private archive outside this repository. Size: 955650 bytes; SHA-256: `9e3ca876d783ea2b38f7cd779b417768a8080651c5997759b105b586bf6def69`. The Stage 0 prompt and log remain in `prompts/stage_00_prompt.md` and `logs/stage_00_log.md`.
- Git history was rewritten (2026-10-24) to drop that transcript; commit order and dates of the remaining history are preserved.
