#!/usr/bin/env sh
# Usage: ./run.sh [dusk|noon|night|dawn]
exec cargo run --release -- "$@"
