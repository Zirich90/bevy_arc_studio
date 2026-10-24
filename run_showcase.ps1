# Cinematic showcase runner (dynamic_linking dev binary). Usage: ./run_showcase.ps1 live | shot <t> <out.png> [cam] | record <dir> [fps]
$sr = rustc --print sysroot
$env:PATH = "$sr\lib\rustlib\x86_64-pc-windows-msvc\lib;$PSScriptRoot\target\debug\deps;$env:PATH"
cargo build -q --bin showcase
& "$PSScriptRoot\target\debug\showcase.exe" @args
