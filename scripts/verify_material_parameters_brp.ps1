param([int]$Port = 15702, [switch]$HotReload)
$ErrorActionPreference = 'Stop'
function Rpc($Method, $Params) {
    $body = @{jsonrpc='2.0';id=1;method=$Method;params=$Params} | ConvertTo-Json -Depth 50 -Compress
    $reply = Invoke-RestMethod -Uri "http://127.0.0.1:$Port" -Method Post -ContentType application/json -Body $body -TimeoutSec 20 -NoProxy
    if ($reply.error) { throw ($reply.error | ConvertTo-Json -Compress) }
    return $reply.result
}
function Commands($Items, [bool]$ExpectError = $false) {
    $null = Rpc 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue';value=@{commands=@($Items);log=@()}}
    Start-Sleep -Milliseconds 500
    $result = Rpc 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    $failed = [bool]($result.value.log -match '^ERR')
    if ($failed -ne $ExpectError) { throw "Unexpected command result: $($result.value.log -join '; ')" }
}
function Integration {
    Commands @('DescribeIntegration')
    $results = Rpc 'world.get_resources' @{resource='bevy_arc_studio::operators::CommandResults'}
    return (($results.value.items | Where-Object op -eq 'DescribeIntegration' | Select-Object -Last 1).data | ConvertFrom-Json)
}
function Binding($Name) {
    $rows = @(Rpc 'world.query' @{data=@{components=@('bevy_ecs::name::Name','bevy_arc_studio::quillshade_bridge::QuillshadeBinding')}})
    return ($rows | Where-Object { $_.components.'bevy_ecs::name::Name' -eq $Name }).components.'bevy_arc_studio::quillshade_bridge::QuillshadeBinding'
}
function Speed($Name) {
    return ((Binding $Name).parameters | Where-Object name -eq 'inp_scan_speed').value.Float
}
Commands @(@{LoadScene=@{path='scenes/behavior_pilot.scn.json'}})
Start-Sleep -Seconds 2
$layout = ((Integration).material_parameters | Where-Object id -eq 'fx/hologram').layout
if (!$layout) { throw 'Missing typed material schema' }
$member = 'Timer: hides after 5 seconds'
$target = @{Name=$member}
Commands @(@{AssignQuillshadeMaterial=@{target=$target;material='fx/hologram'}})
Commands @(@{SetQuillshadeParameters=@{target=$target;layout=$layout;parameters=@(@{name='inp_scan_speed';value=@{Float=0.9}})}})
if ([Math]::Abs((Speed $member) - 0.9) -gt 0.0001) { throw 'Member parameter not applied' }
Commands @('Undo')
if ((Binding $member).parameters.Count -ne 0) { throw 'Parameter Undo failed' }
Commands @('Redo')
if ([Math]::Abs((Speed $member) - 0.9) -gt 0.0001) { throw 'Parameter Redo failed' }
foreach ($bad in @(
    @{layout='SG_LAYOUT_invalid';parameters=@()},
    @{layout=$layout;parameters=@(@{name='_pad0';value=@{Float=1}})},
    @{layout=$layout;parameters=@(@{name='inp_color';value=@{Float=1}})}
)) {
    Commands @(@{SetQuillshadeParameters=@{target=$target;layout=$bad.layout;parameters=$bad.parameters}}) $true
    if ([Math]::Abs((Speed $member) - 0.9) -gt 0.0001) { throw 'Invalid edit changed binding' }
}
Commands @(@{AssignQuillshadeMaterial=@{target=@{Name='Signal: demo.hide'};material='fx/hologram'}})
$instances = @((Integration).material_instances | Where-Object material -eq 'fx/hologram')
if ($instances.Count -lt 2 -or @($instances.instance.handle | Select-Object -Unique).Count -ne $instances.Count) { throw 'Material handles are shared' }
if ($HotReload) {
    $source = Join-Path $PSScriptRoot '../quillshade/material/fx/hologram.material.ron'
    $runner = Join-Path $PSScriptRoot '../target/quillshade-tools/debug/arc-quillshade-regen.exe'
    if (!(Test-Path $runner)) { throw 'Build the upstream backend runner before Hot Reload verification' }
    $original = [IO.File]::ReadAllBytes($source)
    try {
        $text = [IO.File]::ReadAllText($source).Replace('Float(0.5)', 'Float(0.7)').Replace('Color((0.2, 0.8, 1.0, 1.0))', 'Color((1.0, 0.1, 0.1, 1.0))')
        [IO.File]::WriteAllText($source, $text)
        & $runner (Resolve-Path "$PSScriptRoot/..").Path 2> "$PSScriptRoot/../target/i2_hot_reload.log" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Upstream regeneration failed' }
        Start-Sleep -Seconds 3
        $live = @((Integration).material_instances | Where-Object material -eq 'fx/hologram')
        $speeds = @($live.instance.parameters | Where-Object name -eq 'inp_scan_speed' | ForEach-Object { $_.value.Float } | Sort-Object)
        if ($speeds.Count -ne 2 -or [Math]::Abs($speeds[0] - 0.7) -gt 0.0001 -or [Math]::Abs($speeds[1] - 0.9) -gt 0.0001) { throw 'Hot Reload failed to rebase private instances while retaining overrides' }
        foreach ($instance in $live) {
            $color = ($instance.instance.parameters | Where-Object name -eq 'inp_color').value.Vector
            if ([Math]::Abs($color[0] - 1) -gt 0.0001 -or [Math]::Abs($color[1] - 0.1) -gt 0.0001) { throw 'Hot Reload color not propagated' }
        }
    } finally {
        [IO.File]::WriteAllBytes($source, $original)
        & $runner (Resolve-Path "$PSScriptRoot/..").Path 2> "$PSScriptRoot/../target/i2_hot_reload_restore.log" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Restoring generated output failed' }
        Start-Sleep -Seconds 2
    }
}
Commands @(@{SaveScene=@{path='target/i2_material_parameters.scn.json'}})
Commands @(@{RemoveComponent=@{target=$target;component='QuillshadeBinding'}})
if (Binding $member) { throw 'Linked-member remove failed' }
Commands @('Undo')
if ([Math]::Abs((Speed $member) - 0.9) -gt 0.0001) { throw 'Linked-member remove Undo failed' }
Commands @(@{LoadScene=@{path='target/i2_material_parameters.scn.json'}})
Start-Sleep -Seconds 2
if ([Math]::Abs((Speed $member) - 0.9) -gt 0.0001) { throw 'Linked-member save/load failed' }
Commands @(@{SetPlayMode=@{mode='Playing'}},@{SetPlayMode=@{mode='Paused'}})
Commands @(@{SetQuillshadeParameters=@{target=$target;layout=$layout;parameters=@(@{name='inp_scan_speed';value=@{Float=0.2}})}})
Commands @(@{SetPlayMode=@{mode='Edit'}})
if ([Math]::Abs((Speed $member) - 0.9) -gt 0.0001) { throw 'Linked-member PIE rollback failed' }
Commands @(@{Select=@{name='behavior_pilot'}},'FocusSelected')
Write-Output 'PASS: typed parameters, layout/type guards, private handles, linked-member Undo/Redo/remove, scene and PIE rollback'

Commands @(@{PlaceLevelTile=@{kind='Floor';pos=@(0,0,0)}})
$tiles = @(Rpc 'world.query' @{data=@{components=@('bevy_ecs::name::Name','bevy_arc_studio::level_builder::LevelTile')}})
if ($tiles.Count -ne 1) { throw "Expected one tile, found $($tiles.Count)" }
$tile = $tiles[0].components.'bevy_ecs::name::Name'
$tileTarget = @{Name=$tile}
Commands @(@{AssignQuillshadeMaterial=@{target=$tileTarget;material='fx/hologram'}})
Commands @(@{SetQuillshadeParameters=@{target=$tileTarget;layout=$layout;parameters=@(@{name='inp_scan_speed';value=@{Float=0.65}})}})
Commands @(@{SaveScene=@{path='target/i2_tile_material.scn.json'}})
Commands @(@{RemoveComponent=@{target=$tileTarget;component='QuillshadeBinding'}})
if (Binding $tile) { throw 'Tile binding removal failed' }
Commands @(@{LoadScene=@{path='target/i2_tile_material.scn.json'}})
if ([Math]::Abs((Speed $tile) - 0.65) -gt 0.0001) { throw 'Tile parameter did not survive scene reload' }
$tileMeshes = @((Integration).material_instances | Where-Object {
    $_.material -eq 'fx/hologram' -and
    @($_.instance.parameters | Where-Object { $_.name -eq 'inp_scan_speed' -and [Math]::Abs($_.value.Float - 0.65) -lt 0.0001 }).Count
})
if (!$tileMeshes.Count) { throw 'Restored tile override was not applied to a render mesh' }
Write-Output 'PASS: tile binding and typed override persisted and applied to a render mesh'
