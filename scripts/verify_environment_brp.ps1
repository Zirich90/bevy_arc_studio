param([int]$Port = 15702)
$ErrorActionPreference = 'Stop'
function Rpc($Method, $Params) {
    $reply = Invoke-RestMethod -Uri "http://127.0.0.1:$Port" -Method Post -ContentType application/json -Body (@{jsonrpc='2.0';id=1;method=$Method;params=$Params} | ConvertTo-Json -Depth 50 -Compress) -TimeoutSec 30 -NoProxy
    if ($reply.error) { throw ($reply.error | ConvertTo-Json -Compress) }; $reply.result
}
function Commands($Items) {
    $null = Rpc 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue';value=@{commands=@($Items);log=@()}}
    Start-Sleep -Milliseconds 700
    $r = Rpc 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    if ($r.value.log -match '^ERR') { throw ($r.value.log -join '; ') }
}
function Integration {
    Commands @('DescribeIntegration')
    $r = Rpc 'world.get_resources' @{resource='bevy_arc_studio::operators::CommandResults'}
    (($r.value.items | Where-Object op -eq 'DescribeIntegration' | Select-Object -Last 1).data | ConvertFrom-Json)
}
function Environment { (Rpc 'world.get_resources' @{resource='bevy_arc_studio::environment::Environment'}).value }
function AssertPreset($Preset) { if ((Environment).preset -ne $Preset) { throw "Expected preset $Preset" } }
Commands @(@{TransitionEnvironment=@{preset=$null;duration=0.0}})
$baseline = (Integration).environment_runtime
# Thumbnail cameras are temporary; compare persistent atmosphere cameras.
$baseline.cameras = @($baseline.cameras | Where-Object atmosphere_settings)
if (!$baseline.cameras.Count) { throw "No persistent atmosphere camera found" }
Commands @(@{SetQuillshadeEnvironment=@{preset='day'}}); AssertPreset 'day'
Commands @(@{SetQuillshadeEnvironment=@{preset='night'}}); AssertPreset 'night'
Commands @('Undo'); AssertPreset 'day'
Commands @('Redo'); AssertPreset 'night'
Commands @(@{SetPlayMode=@{mode='Playing'}})
Commands @(@{SetQuillshadeEnvironment=@{preset='day'}})
Commands @(@{SetPlayMode=@{mode='Edit'}}); AssertPreset 'night'
Commands @(@{TransitionEnvironment=@{preset='day';duration=60.0}})
$before = Environment
if ($before.elapsed -le 0 -or !$before.from_json) { throw 'Transition did not advance' }
Commands @(@{SaveScene=@{path='target/i2_environment_transition.scn.json'}})
Commands @(@{SetQuillshadeEnvironment=@{preset='night'}})
Commands @(@{LoadScene=@{path='target/i2_environment_transition.scn.json'}})
$loaded = Environment
if ($loaded.preset -ne 'day' -or $loaded.elapsed -lt $before.elapsed -or $loaded.elapsed -gt ($before.elapsed+8)) { throw 'Transition progress did not resume' }
Commands @(@{SetPlayMode=@{mode='Paused'}})
$frozen = (Environment).elapsed; Start-Sleep -Seconds 2
if ((Environment).elapsed -ne $frozen) { throw 'Paused transition advanced' }
Commands @(@{SetPlayMode=@{mode='Edit'}})
Commands @(@{TransitionEnvironment=@{preset=$null;duration=0.0}})
if ((Environment).preset) { throw 'Environment release failed' }
$managed = (Integration).environment_runtime.managed_lights
if ($managed -gt 0) { throw 'Managed lights remain after release' }
$restored = (Integration).environment_runtime
foreach ($camera in $baseline.cameras) {
    $after = @($restored.cameras | Where-Object entity -eq $camera.entity)
    if ($after.Count -ne 1) { throw 'Original camera missing after release' }
    foreach ($field in @('hdr','skybox','blended_skybox','environment_map','generated_environment_map','atmosphere_settings','atmosphere_ibl','volumetric_fog')) {
        if ($after[0].$field -ne $camera.$field) { throw "Camera ownership mismatch: $field" }
    }
}
if ($restored.values) { throw 'Upstream runtime remains active after release' }
'PASS environment Undo / Redo / PIE / transition save-load / Pause / managed-light release'
