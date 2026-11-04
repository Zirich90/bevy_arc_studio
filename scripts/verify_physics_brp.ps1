param([int]$Port=15702)
$ErrorActionPreference='Stop'
function Rpc($Method,$Params) {
    $r=Invoke-RestMethod "http://127.0.0.1:$Port" -NoProxy -Method Post -ContentType application/json -Body (@{jsonrpc='2.0';id=1;method=$Method;params=$Params}|ConvertTo-Json -Depth 60 -Compress) -TimeoutSec 45
    if ($r.error) {throw ($r.error|ConvertTo-Json -Compress)}; $r.result
}
function Commands($Items) {
    $null=Rpc 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue';value=@{commands=@($Items);log=@()}}
    Start-Sleep -Milliseconds 800
    $r=Rpc 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    if ($r.value.log -match '^ERR') {throw ($r.value.log -join '; ')}
}
function Integration {
    Commands @('DescribeIntegration')
    $r=Rpc 'world.get_resources' @{resource='bevy_arc_studio::operators::CommandResults'}
    (($r.value.items|Where-Object op -eq 'DescribeIntegration'|Select-Object -Last 1).data|ConvertFrom-Json)
}
Commands @(@{LoadScene=@{path='target/i2_graph_roundtrip.scn.json'}})
$before=(Integration).simulation
if (!$before.active -or !$before.mesh_matches -or !$before.terrain_matches -or $before.cells -ne 4225) {
    throw 'Loaded CPU fields do not match the terrain mesh'
}
Commands @(@{SetPlayMode=@{mode='Playing'}})
$a=(Integration).simulation
$b=(Integration).simulation
if (!$a.physical_totals -or $b.tick -le $a.tick) {throw 'Physics did not advance'}
function ClosedEnergy($s) {
    $p=$s.physical_totals
    $p.energy_j - $p.sunlight_j - $p.external_heat_j
}
if ([math]::Abs($a.physical_totals.mass_kg - $b.physical_totals.mass_kg) -gt 0.001) {throw 'Mass drift'}
if ([math]::Abs((ClosedEnergy $a) - (ClosedEnergy $b)) -gt 0.1) {throw 'Energy drift'}
Commands @(@{SetPlayMode=@{mode='Paused'}})
$a=(Integration).simulation
$b=(Integration).simulation
if ($a.tick -ne $b.tick -or $a.fingerprint -ne $b.fingerprint) {throw 'Pause changed simulation'}
Commands @(@{SetPlayMode=@{mode='Edit'}})
$after=(Integration).simulation
if ($after.fingerprint -ne $before.fingerprint -or $after.tick -ne $before.tick -or $after.physical_totals) {throw 'Stop did not restore the pre-Play fields'}
'PASS physics Play / balances / Pause / Stop'
