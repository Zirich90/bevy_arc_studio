param([int]$Port = 15702)
$ErrorActionPreference = 'Stop'
function Rpc($Method, $Params) {
    $body = @{jsonrpc='2.0';id=1;method=$Method;params=$Params} | ConvertTo-Json -Depth 40 -Compress
    $reply = Invoke-RestMethod -Uri "http://127.0.0.1:$Port" -Method Post -ContentType application/json -Body $body -TimeoutSec 20 -NoProxy
    if ($reply.error) { throw ($reply.error | ConvertTo-Json -Compress) }
    return $reply.result
}
function Commands($Items) {
    $null = Rpc 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue';value=@{commands=@($Items);log=@()}}
    Start-Sleep -Milliseconds 500
    $result = Rpc 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    if ($result.value.log -match '^ERR') { throw ($result.value.log -join '; ') }
}
function Integration {
    Commands @('DescribeIntegration')
    $results = Rpc 'world.get_resources' @{resource='bevy_arc_studio::operators::CommandResults'}
    return (($results.value.items | Where-Object op -eq 'DescribeIntegration' | Select-Object -Last 1).data | ConvertFrom-Json)
}
function Bindings {
    return @(Rpc 'world.query' @{data=@{components=@('bevy_arc_studio::quillshade_bridge::QuillshadeBinding')}})
}
Commands @(@{LoadScene=@{path='scenes/behavior_pilot.scn.json'}})
Start-Sleep -Seconds 2
$initial = Integration
if ($initial.materials.Count -lt 1) { throw 'Empty generated material registry' }
# Linked prefab members are rebuilt from their source; test the persistent instance root.
$target = @{Name='behavior_pilot'}
Commands @(@{AssignQuillshadeMaterial=@{target=$target;material='fx/hologram'}})
if ((Bindings).Count -ne 1) { throw 'Binding assignment failed' }
Commands @('Undo')
if ((Bindings).Count -ne 0) { throw 'Binding Undo failed' }
Commands @('Redo')
if ((Bindings).Count -ne 1) { throw 'Binding Redo failed' }
$empty = '(version:1,nodes:[],edges:[])'
Commands @(@{ReplaceStudioGraph=@{ron=$empty}})
if ((Integration).graph.nodes.Count -ne 0) { throw 'Graph replacement failed' }
Commands @('Undo')
if ((Integration).graph.nodes.Count -ne $initial.graph.nodes.Count) { throw 'Graph Undo failed' }
Commands @('Redo')
if ((Integration).graph.nodes.Count -ne 0) { throw 'Graph Redo failed' }
Commands @(@{ReplaceStudioGraph=@{ron=$initial.graph_ron}})
Commands @(@{SetPlayMode=@{mode='Playing'}},@{SetPlayMode=@{mode='Paused'}})
Commands @(@{ReplaceStudioGraph=@{ron=$empty}})
Commands @(@{SetPlayMode=@{mode='Edit'}})
if ((Integration).graph.nodes.Count -ne $initial.graph.nodes.Count) { throw 'PIE graph rollback failed' }
Commands @(@{PlaceLevelTile=@{kind='Floor';pos=@(5,0,5);facing=$null;style=$null}})
Commands @(@{AssignQuillshadeMaterial=@{target=@{Name='Floor L0 (5, 5)'};material='fx/coords'}})
if ((Bindings).Count -ne 2) { throw 'Tile material assignment failed' }
Commands @(@{SaveScene=@{path='target/integration_roundtrip.scn.json'}})
Start-Sleep -Seconds 1
Commands @(@{ReplaceStudioGraph=@{ron=$empty}},@{RemoveComponent=@{target=$target;component='QuillshadeBinding'}})
Commands @(@{LoadScene=@{path='target/integration_roundtrip.scn.json'}})
Start-Sleep -Seconds 2
if ((Integration).graph.nodes.Count -ne $initial.graph.nodes.Count -or (Bindings).Count -ne 2) { throw 'Graph/object/tile binding scene roundtrip failed' }
Commands @(@{Select=@{name='behavior_pilot'}},'FocusSelected')
Write-Output "PASS: $($initial.materials.Count) materials; assign/Undo/Redo; graph Undo/Redo; PIE; scene roundtrip"
