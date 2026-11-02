param([Parameter(Mandatory=$true)][string]$ShaderEditor)
$ErrorActionPreference = 'Stop'
$studioRoot = Split-Path -Parent $PSScriptRoot
& $ShaderEditor check $studioRoot
if ($LASTEXITCODE -ne 0) { throw 'Quillshade source validation failed' }
& $ShaderEditor regen $studioRoot
if ($LASTEXITCODE -ne 0) { throw 'Quillshade regeneration failed; inspect diagnostics' }
python (Join-Path $PSScriptRoot 'quillshade_catalog.py')
if ($LASTEXITCODE -ne 0) { throw 'Studio catalog generation failed' }
Push-Location $studioRoot
try {
    cargo check --message-format=short -q -j 1
    if ($LASTEXITCODE -ne 0) { throw 'Generated ABI changed; fix adapter before restarting Studio' }
} finally { Pop-Location }
