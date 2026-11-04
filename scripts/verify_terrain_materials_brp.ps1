param([int]$Port = 15702, [switch]$HotReload)
$ErrorActionPreference = 'Stop'
function Rpc($Method, $Params) {
    $body = @{jsonrpc='2.0';id=1;method=$Method;params=$Params} | ConvertTo-Json -Depth 50 -Compress
    $reply = Invoke-RestMethod -Uri "http://127.0.0.1:$Port" -Method Post -ContentType application/json -Body $body -TimeoutSec 30 -NoProxy
    if ($reply.error) { throw ($reply.error | ConvertTo-Json -Compress) }
    $reply.result
}
function Commands($Items) {
    $null = Rpc 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue';value=@{commands=@($Items);log=@()}}
    Start-Sleep -Milliseconds 700
    $result = Rpc 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    if ($result.value.log -match '^ERR') { throw ($result.value.log -join '; ') }
}
function Integration {
    Commands @('DescribeIntegration')
    $result = Rpc 'world.get_resources' @{resource='bevy_arc_studio::operators::CommandResults'}
    (($result.value.items | Where-Object op -eq 'DescribeIntegration' | Select-Object -Last 1).data | ConvertFrom-Json)
}
function AssertLayers {
    $integration = Integration
    $sections = @($integration.terrain_sections)
    if ($sections.Count -ne 6) { throw 'Expected six terrain layer draw sections' }
    $handles = @()
    foreach ($section in $sections) {
        $instance = @($integration.material_instances | Where-Object entity -eq $section.entity)
        if ($instance.Count -ne 1) { throw 'Missing actual layer instance' }
        $handles += $instance[0].instance.handle
        $speed = ($instance[0].instance.parameters | Where-Object name -eq 'inp_scan_speed').value.Float
        if ([Math]::Abs($speed - (0.1 * ($section.layer + 1))) -gt 0.0001) { throw 'Layer override leaked or disappeared' }
    }
    if (@($handles | Select-Object -Unique).Count -ne 6) { throw 'Layer materials share instance handles' }
    if ($integration.terrain_surfaces.master.material -ne 'env/rock_wet') { throw 'Master binding disappeared' }
}
Commands @(@{LoadScene=@{path='target/i2_water_roundtrip.scn.json'}})
Start-Sleep -Seconds 2
$layout = ((Integration).material_parameters | Where-Object id -eq 'fx/hologram').layout
Commands @(@{SetTerrainMaterial=@{layer=$null;binding=@{material='env/rock_wet';layout='';parameters=@()}}})
for ($i=0; $i -lt 6; $i++) {
    Commands @(@{SetTerrainMaterial=@{layer=$i;binding=@{material='fx/hologram';layout=$layout;parameters=@(@{name='inp_scan_speed';value=@{Float=(0.1*($i+1))}})}}})
}
AssertLayers
if ($HotReload) {
    $source = Join-Path $PSScriptRoot '../quillshade/material/fx/hologram.material.ron'
    $runner = Join-Path $PSScriptRoot '../target/quillshade-tools/debug/arc-quillshade-regen.exe'
    if (!(Test-Path $runner)) { throw 'Build upstream backend runner first' }
    $original = [IO.File]::ReadAllBytes($source)
    try {
        [IO.File]::WriteAllText($source, [IO.File]::ReadAllText($source).Replace('Color((0.2, 0.8, 1.0, 1.0))', 'Color((1.0, 0.1, 0.1, 1.0))'))
        & $runner (Resolve-Path "$PSScriptRoot/..").Path 2> "$PSScriptRoot/../target/i2_terrain_hot_reload.log" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Upstream regeneration failed' }
        Start-Sleep -Seconds 4
        AssertLayers
        $integration = Integration
        foreach ($section in $integration.terrain_sections) {
            $instance = $integration.material_instances | Where-Object entity -eq $section.entity
            $color = ($instance.instance.parameters | Where-Object name -eq 'inp_color').value.Vector
            if ([Math]::Abs($color[0] - 1.0) -gt 0.0001 -or [Math]::Abs($color[1] - 0.1) -gt 0.0001) { throw 'Layer Hot Reload did not rebase actual uniforms' }
        }
    } finally {
        [IO.File]::WriteAllBytes($source, $original)
        & $runner (Resolve-Path "$PSScriptRoot/..").Path 2> "$PSScriptRoot/../target/i2_terrain_hot_reload_restore.log" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Upstream restoration failed' }
        Start-Sleep -Seconds 4
    }
    AssertLayers
}
Commands @('Undo')
if (@((Integration).terrain_sections).Count -ne 5) { throw 'Terrain layer Undo failed' }
Commands @('Redo'); AssertLayers
Commands @(@{SetPlayMode=@{mode='Playing'}})
Commands @(@{SetTerrainMaterial=@{layer=2;binding=$null}})
Commands @(@{SetPlayMode=@{mode='Edit'}}); AssertLayers
Commands @(@{SaveScene=@{path='target/i2_terrain_roundtrip.scn.json'}})
Commands @(@{SetTerrainMaterial=@{layer=$null;binding=$null}})
Commands @(@{LoadScene=@{path='target/i2_terrain_roundtrip.scn.json'}})
Start-Sleep -Seconds 2
AssertLayers
'PASS terrain master / six layer instances / isolated parameters / Undo / Redo / PIE / scene roundtrip'
