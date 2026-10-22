# Direct launch of the dev (dynamic_linking) binary: puts std-*.dll and bevy_dylib-*.dll on PATH.
# Usage: ./run_debug.ps1 [dusk|noon|night|dawn]   (cargo run --release does not need this)
$sr = rustc --print sysroot
$env:PATH = "$sr\lib\rustlib\x86_64-pc-windows-msvc\lib;$PSScriptRoot\target\debug\deps;$env:PATH"
cargo build -q
& "$PSScriptRoot\target\debug\bevy_arc_studio.exe" @args
