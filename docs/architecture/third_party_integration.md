# Third-party integration provenance

Imported 2026-11-02 from existing local clones, without source modifications.

- `vendor/bevy_noodle`: upstream `https://github.com/gold-silver-copper/bevy_noodle`, commit `211f5df175755d9c73b1da9457f1802008f69951`, version 0.5.0, MIT; LICENSE retained. Core, examples and tests retained so Cargo manifest targets remain valid. This is not Quillshade's fork.
- `crates/quillshade`, `quillshade`, `assets/quillshade`, `assets/textures`: `examples/sample_game` of `https://github.com/fe3dback/bevy-quillshade`, commit `35be8595ddf71d3fb4d354f8508e6460f1d340db`, MIT per upstream Cargo.toml/README. The local repository has no standalone LICENSE file; none was fabricated. Generated source headers retained.
- Studio adapters are outside generated directories. Future regeneration may replace all imported generated output. Source RON is kept for reproduction; no reverse editing from generated Rust is supported.

This is an explicitly requested dependency integration, not an independent clean-room reimplementation of either library. The ArcStudio audit records this distinction.

Upstream runtime search (2026-11-02): both remote HEADs matched the snapshots above. No Biology/Granular/FluidFlow, hydraulic erosion, Fireball or phase-change simulation backend was found in their source trees. Noodle's `examples/geometry_nodes.rs` implements point processing and instancing as application example code; the library delegates graph execution to the host. Quillshade's temperature controls are color grading/black-body color operations. Missing simulation and remaining integration contracts are scheduled for [stage I2](08_quillshade_noodle.md#следующий-этап-i2-исполнение-графов-и-мультифизика).
