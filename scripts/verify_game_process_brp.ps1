param([int]$EditorPort=15702,[int]$GamePort=15704)
$ErrorActionPreference='Stop'
function Rpc($Port,$Method,$Params) {
    $body=@{jsonrpc='2.0';id=1;method=$Method;params=$Params}|ConvertTo-Json -Depth 40 -Compress
    $r=Invoke-RestMethod "http://127.0.0.1:$Port" -Method Post -ContentType application/json -Body $body -TimeoutSec 15 -NoProxy
    if($r.error){throw ($r.error|ConvertTo-Json -Compress)}
    $r.result
}
function Cmd($Port,$Commands) {
    $null=Rpc $Port 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue';value=@{commands=@($Commands);log=@()}}
    Start-Sleep -Milliseconds 500
    $r=Rpc $Port 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    if($r.value.log -match '^ERR'){throw ($r.value.log -join ';')}
}
function Mode($Port){(Rpc $Port 'world.get_resources' @{resource='bevy_arc_studio::studio::StudioState'}).value.mode}
function Rows($Port){@(Rpc $Port 'world.query' @{data=@{components=@('bevy_arc_studio::props::SceneObject');option=@('bevy_ecs::hierarchy::ChildOf','bevy_arc_studio::behavior::TimedVisibility')}})}
Cmd $EditorPort @(@{LoadScene=@{path='scenes/behavior_pilot.scn.json'}})
$before=Rows $EditorPort
Cmd $EditorPort @(@{Custom=@{name='game.start';args=(@{path='scenes/behavior_pilot.scn.json';port=$GamePort}|ConvertTo-Json -Compress)}})
try {
    $ready=$false
    for($i=0;$i -lt 45;$i++) { try { if((Mode $GamePort) -eq 'Playing'){$ready=$true;break} } catch {} ; Start-Sleep -Seconds 1 }
    if(!$ready){throw 'Child game did not enter Playing'}
    if((Mode $EditorPort) -ne 'Edit'){throw 'Editor entered game mode'}
    Cmd $GamePort @(@{SetPlayMode=@{mode='Paused'}})
    if((Mode $GamePort) -ne 'Paused'){throw 'Pause failed'}
    $timer=@(Rows $GamePort|Where-Object {$_.components.'bevy_arc_studio::behavior::TimedVisibility'})[0]
    if(!$timer){throw 'Missing timer entity'}
    $null=Rpc $GamePort 'world.insert_components' @{entity=$timer.entity;components=@{'bevy_arc_studio::behavior::TimedVisibility'=@{delay_seconds=123.0;visible=$false}}}
    $changed=@(Rows $GamePort|Where-Object {$_.entity -eq $timer.entity})[0]
    if($changed.components.'bevy_arc_studio::behavior::TimedVisibility'.delay_seconds -ne 123.0){throw 'Live component edit failed'}
    Cmd $GamePort @(@{SaveScene=@{path='target/e5_transition.scn.json'}})
    Cmd $GamePort @(@{Custom=@{name='scene.switch';args='{"path":"target/e5_transition.scn.json"}'}})
    if((Mode $GamePort) -ne 'Playing'){throw 'Scene transition did not resume Play'}
    Cmd $GamePort @(@{SetPlayMode=@{mode='Paused'}})
    $after=Rows $GamePort
    if($after.Count -ne $before.Count){throw 'Scene transition object count changed'}
    $editor=Rows $EditorPort
    if(($editor|ConvertTo-Json -Depth 20 -Compress) -ne ($before|ConvertTo-Json -Depth 20 -Compress)){throw 'Game changed editor scene'}
    Write-Output 'PASS: child process, Pause, live edit, scene transition, editor isolation'
} finally { Cmd $EditorPort @(@{Custom=@{name='game.stop';args='{}'}}) }
