param([int]$Port = 15702)
$ErrorActionPreference = 'Stop'
function Rpc($Method, $Params) {
    $body = @{jsonrpc='2.0';id=1;method=$Method;params=$Params} | ConvertTo-Json -Depth 20 -Compress
    $reply = Invoke-RestMethod -Uri "http://127.0.0.1:$Port" -Method Post -ContentType application/json -Body $body -TimeoutSec 20 -NoProxy
    if ($reply.error) { throw ($reply.error | ConvertTo-Json -Compress) }
    return $reply.result
}
function Quality($Name) {
    $null = Rpc 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue';value=@{commands=@(@{SetQuality=$Name});log=@()}}
    Start-Sleep -Seconds 2
    $queue = Rpc 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    if ($queue.value.log -match '^ERR') { throw ($queue.value.log -join '; ') }
    $spec = Rpc 'world.get_resources' @{resource='bevy_arc_studio::arc_core::WorldSpec'}
    if ($spec.value.quality_preset -ne $Name) { throw "Quality switch failed: $Name" }
    $map = Rpc 'world.get_resources' @{resource='bevy_light::directional_light::DirectionalLightShadowMap'}
    $expected = switch ($Name) { Low {1024} Medium {2048} default {4096} }
    if ($map.value.size -ne $expected) { throw "Shadow size mismatch for ${Name}: $($map.value.size)" }
}
$initial = Rpc 'world.get_resources' @{resource='bevy_arc_studio::arc_core::WorldSpec'}
try {
    foreach ($name in @('Low','Ultra','Medium','Ultra','High','Ultra')) { Quality $name }
    $path = Join-Path (Split-Path $PSScriptRoot -Parent) 'docs/screenshots/i2_ultra_safe.png'
    $null = Rpc 'brp_extras/screenshot' @{path=$path}
    Write-Output "PASS: repeated quality switches; Ultra shadows 4096; screenshot $path"
} finally {
    Quality $initial.value.quality_preset
}
