param([int]$Port = 15703)
$ErrorActionPreference = 'Stop'
function Rpc($Method, $Params) {
    $body = @{jsonrpc='2.0'; id=1; method=$Method; params=$Params} | ConvertTo-Json -Depth 30 -Compress
    $reply = Invoke-RestMethod -Uri "http://127.0.0.1:$Port" -Method Post -ContentType application/json -Body $body -TimeoutSec 10 -NoProxy
    if ($reply.error) { throw ($reply.error | ConvertTo-Json -Compress) }
    return $reply.result
}
function Commands($Items) {
    $null = Rpc 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'; value=@{commands=@($Items); log=@()}}
    Start-Sleep -Milliseconds 250
    $result = Rpc 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    $result.value.log | ForEach-Object { Write-Output $_ }
    if ($result.value.log -match '^ERR') { throw 'Editor command failed' }
}
function Component($Type) {
    return @(Rpc 'world.query' @{data=@{components=@("bevy_arc_studio::behavior::$Type",'bevy_camera::visibility::Visibility')}})
}
function AssertVisibility($Type, $Expected) {
    $rows = Component $Type
    if ($rows.Count -ne 1 -or $rows[0].components.'bevy_camera::visibility::Visibility' -ne $Expected) { throw "$Type expected $Expected" }
    Write-Output "$Type visibility=$Expected"
}
Commands @(@{LoadScene=@{path='scenes/behavior_pilot.scn.json'}})
Start-Sleep -Seconds 1
AssertVisibility 'TimedVisibility' 'Inherited'
AssertVisibility 'SignalVisibility' 'Inherited'
Commands @(@{SetPrefabOverride=@{target=@{Name='behavior_pilot'};member='timer';field='components';value='{"bevy_arc_studio::behavior::TimedVisibility":{"delay_seconds":7.0,"visible":false}}'}})
Commands @(@{SaveScene=@{path='target/behavior_roundtrip.scn.json'}})
Start-Sleep -Seconds 1
if (!(Test-Path 'target/behavior_roundtrip.scn.json')) { throw 'Scene save failed' }
Commands @(@{LoadScene=@{path='target/behavior_roundtrip.scn.json'}})
Start-Sleep -Seconds 1
AssertVisibility 'TimedVisibility' 'Inherited'
AssertVisibility 'SignalVisibility' 'Inherited'
$timer = Component 'TimedVisibility'
if ($timer[0].components.'bevy_arc_studio::behavior::TimedVisibility'.delay_seconds -ne 7.0) { throw 'Prefab behavior override did not survive scene roundtrip' }
Commands @(@{SetPrefabOverride=@{target=@{Name='behavior_pilot'};member='timer';field='components';value='null'}})
Commands @(@{SetComponent=@{target=@{Name='Timer: hides after 5 seconds'};component='TimedVisibility';value='{"delay_seconds":0.5}'}})
$timer = Component 'TimedVisibility'
if ($timer[0].components.'bevy_arc_studio::behavior::TimedVisibility'.delay_seconds -ne 0.5) { throw 'Component edit failed' }
Commands @('Undo')
$timer = Component 'TimedVisibility'
if ($timer[0].components.'bevy_arc_studio::behavior::TimedVisibility'.delay_seconds -ne 5.0) { throw 'Component Undo failed' }
Commands @(@{SetPlayMode=@{mode='Playing'}}, @{SetPlayMode=@{mode='Paused'}})
Start-Sleep -Seconds 6
AssertVisibility 'TimedVisibility' 'Inherited'
Commands @(@{SetPlayMode=@{mode='Playing'}})
Start-Sleep -Seconds 6
AssertVisibility 'TimedVisibility' 'Hidden'
AssertVisibility 'SignalVisibility' 'Inherited'
Commands @(@{Custom=@{name='behavior.emit';args='{"signal":"demo.hide"}'}})
AssertVisibility 'SignalVisibility' 'Hidden'
Commands @(@{SetPlayMode=@{mode='Edit'}})
AssertVisibility 'TimedVisibility' 'Inherited'
AssertVisibility 'SignalVisibility' 'Inherited'
Commands @(@{Select=@{name='behavior_pilot'}},'FocusSelected')
Write-Output 'PASS: prefab/scene load, component edit/Undo, Pause, timer, signal, Stop'
