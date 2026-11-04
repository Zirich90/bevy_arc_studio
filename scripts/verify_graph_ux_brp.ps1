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
function GraphPositions {
    $r=Rpc 'world.get_resources' @{resource='bevy_arc_studio::arc_core::StudioGraph'}
    ConvertTo-Json -InputObject @($r.value.nodes | ForEach-Object { $_.position }) -Depth 8 -Compress
}
Commands @(@{LoadScene=@{path='target/i2_graph_roundtrip.scn.json'}})
$before=(Integration).graph
$id=$before.nodes[0].id
Commands @(@{SetStudioGraphParameters=@{id=$id;params=@{Noise=@{seed=97;frequency=2.5;warp=0.1}}}})
$edited=(Integration).graph
if ($edited.nodes[0].params.Noise.seed -ne 97) {throw 'Typed edit failed'}
Commands @('Undo')
if ((Integration).graph.nodes[0].params.Noise.seed -ne $before.nodes[0].params.Noise.seed) {throw 'Undo failed'}
Commands @('Redo')
Commands @(@{SaveStudioGraph=@{path='target/i2_graph_ux.ron'}})
Commands @(@{AddStudioGraphNode=@{kind='Scatter'}})
$added=(Integration).graph
if ($added.nodes.Count -ne ($edited.nodes.Count + 1)) {throw 'Add node failed'}
$graphResource='bevy_arc_studio::arc_core::StudioGraph'
$displaced=(Rpc 'world.get_resources' @{resource=$graphResource}).value
$displaced.nodes[0].position=@(77.0,63.0)
$null=Rpc 'world.insert_resources' @{resource=$graphResource;value=$displaced}
$added=(Integration).graph
$beforeLayout=GraphPositions
Commands @('AutoLayoutStudioGraph')
if ((GraphPositions) -eq $beforeLayout) {throw 'Auto layout made no change'}
Commands @('Undo')
if ((GraphPositions) -ne $beforeLayout) {throw 'Layout Undo did not restore positions'}
if (((Integration).graph | ConvertTo-Json -Depth 25 -Compress) -ne ($added | ConvertTo-Json -Depth 25 -Compress)) {throw 'Layout Undo did not restore the graph'}
Commands @(@{LoadStudioGraph=@{path='target/i2_graph_ux.ron'}})
$loaded=(Integration).graph
if (($loaded|ConvertTo-Json -Depth 25 -Compress) -ne ($edited|ConvertTo-Json -Depth 25 -Compress)) {throw 'RON roundtrip failed'}
$legacy='(version:1,nodes:[(id:7,kind:Noise,position:(12.0,0.0),value:2.0)],edges:[])'
[IO.File]::WriteAllText("$PWD/target/i2_graph_legacy.ron", $legacy)
Commands @(@{LoadStudioGraph=@{path='target/i2_graph_legacy.ron'}})
$migrated=(Integration).graph
if ($migrated.version -ne 2 -or $migrated.nodes[0].params.Noise.frequency -ne 2) {throw 'Legacy RON migration failed'}
Commands @('Undo')
if ((((Integration).graph)|ConvertTo-Json -Depth 25 -Compress) -ne ($loaded|ConvertTo-Json -Depth 25 -Compress)) {throw 'Load Undo failed'}
'PASS typed edit / Undo / Redo / add node / layout / RON / v1 migration'
