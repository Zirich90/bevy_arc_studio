#!/usr/bin/env sh
# Usage: ./run_pf2e.sh [dusk|noon|night|dawn]   (editor + Pathfinder 2e plugin)
exec cargo run --release --example pf2e_studio -- "$@"
