param([Parameter(Mandatory=$true)][string]$UpstreamRoot)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot/..").Path
$upstream = (Resolve-Path $UpstreamRoot).Path.Replace('\', '/')
foreach ($name in @('project_core','shader_project','light_domain')) {
    if (!(Test-Path "$upstream/crates/$name/Cargo.toml")) { throw "Missing upstream crate: $name" }
}
if ($upstream.Contains('"')) { throw 'Unsupported quote in upstream path' }
$runner = "$root/target/quillshade-runner"
New-Item -ItemType Directory -Force "$runner/src" | Out-Null
@"
[package]
name = "arc-quillshade-regen"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
project_core = { path = "$upstream/crates/project_core" }
shader_project = { path = "$upstream/crates/shader_project" }
light_domain = { path = "$upstream/crates/light_domain", default-features = false }
"@ | Set-Content "$runner/Cargo.toml"
@'
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let project = project_core::Project::open(&args[1]).unwrap();
    let material = shader_project::backend::MaterialBackend::discovering();
    let light = light_domain::backend::LightBackend;
    let report = project_core::regen::run(&project, &[&material, &light], &[], false, args.iter().any(|a| a == "--dry-run")).unwrap();
    for diagnostic in &report.diagnostics { eprintln!("{diagnostic:?}"); }
    println!("{} changes", report.changes.len());
    if report.has_errors() { std::process::exit(1); }
}
'@ | Set-Content "$runner/src/main.rs"
$oldTarget = $env:CARGO_TARGET_DIR
$oldTemp = $env:TEMP
$oldTmp = $env:TMP
try {
    $env:CARGO_TARGET_DIR = "$root/target/quillshade-tools"
    $env:TEMP = "$root/target/tmp"
    $env:TMP = $env:TEMP
    cargo run --message-format=short -q --manifest-path "$runner/Cargo.toml" -- $root --dry-run
    if ($LASTEXITCODE -ne 0) { throw 'Upstream backend dry-run failed' }
} finally {
    $env:CARGO_TARGET_DIR = $oldTarget
    $env:TEMP = $oldTemp
    $env:TMP = $oldTmp
}
