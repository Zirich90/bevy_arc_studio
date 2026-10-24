@echo off
rem Usage: run_pf2e.bat [dusk|noon|night|dawn]   (editor + Pathfinder 2e plugin)
cargo run --release --example pf2e_studio -- %*
