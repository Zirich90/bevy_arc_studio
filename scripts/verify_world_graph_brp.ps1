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
function State {
    $s=(Integration).simulation
    if (!$s.active -or !$s.mesh_matches -or !$s.terrain_matches -or $s.cells -ne 4225) {throw 'CPU fields do not match actual terrain mesh'}
    if ($s.scatter -le 0 -or $s.pine_entities -ne $s.scatter) {throw 'CPU scatter differs from actual PineTree entities'}
    $s
}
$scene=Get-Content "$PSScriptRoot/../scenes/behavior_pilot.scn.json" -Raw|ConvertFrom-Json
$spec=$scene.resources.'bevy_arc_studio::arc_core::WorldSpec'
$spec.terrain_enabled=$true;$spec.resolution=65;$spec.size=64;$spec.erosion_droplets=0;$spec.thermal_iterations=0
$spec.base_height=12;$spec.mountain_height=20;$spec.max_height=40;$spec.water_level=3
$spec.lake_center=@(-8,8);$spec.lake_radius=10;$spec.lake_depth=5;$spec.camp_center=@(20,-12);$spec.clearing_radius=4
$spec.time_of_day=12;$spec.auto_sun=$false;$spec.fog_enabled=$false;$spec.god_rays=$false
$scene.entities=[pscustomobject]@{}
$legacy=@{version=1;nodes=@(@{id=1;kind='Noise';position=@(12,0);value=2.0});edges=@()}
$scene.resources|Add-Member -Force -NotePropertyName 'bevy_arc_studio::arc_core::StudioGraph' -NotePropertyValue $legacy
$fixture='target/i2_graph_fixture.scn.json';$scene|ConvertTo-Json -Depth 60|Set-Content $fixture -Encoding utf8
$bytes=[Collections.Generic.List[byte]]::new();1..4225|ForEach-Object {$bytes.AddRange([BitConverter]::GetBytes([single]10))}
[IO.File]::WriteAllBytes("$PWD/$fixture.heights",$bytes.ToArray())
Commands @(@{LoadScene=@{path=$fixture}});Start-Sleep -Seconds 2
$g=(Integration).graph
if ($g.version -ne 2 -or $g.nodes[0].params.Noise.frequency -ne 2) {throw 'Legacy scene graph migration failed'}
$graph=@'
(version:2,nodes:[
(id:1,kind:Noise,position:(12.0,0.0),value:1.0,params:Noise(seed:41,frequency:2.0,warp:0.1)),
(id:2,kind:HydraulicErosion,position:(225.0,0.0),value:1.0,params:HydraulicErosion(seed:41,droplets:1000,thermal_iterations:2)),
(id:3,kind:Temperature,position:(438.0,0.0),value:1.0,params:Temperature(ambient_c:18.0,lapse_c_per_m:0.0065,diffusion:0.1)),
(id:4,kind:FluidFlow,position:(12.0,150.0),value:1.0,params:FluidFlow(rain_depth_m:0.05,conductivity:0.2,iterations:8)),
(id:5,kind:Biology,position:(225.0,150.0),value:1.0,params:Biology(growth:0.8,min_water_m:0.01,optimum_c:20.0)),
(id:6,kind:Scatter,position:(438.0,150.0),value:1.0,params:Scatter(seed:41,density:0.8,spacing_m:4.0))
],edges:[(from:1,to:2),(from:2,to:3),(from:3,to:4),(from:4,to:5),(from:5,to:6)])
'@
Commands @(@{ReplaceStudioGraph=@{ron=$graph}},@{EvaluateStudioGraph=@{target=$null}})
$a=State
Commands @(@{EvaluateStudioGraph=@{target=$null}})
if ((State).fingerprint -ne $a.fingerprint) {throw 'Repeated CPU graph evaluation is not deterministic'}
$other=$graph.Replace('seed:41,frequency','seed:42,frequency')
Commands @(@{ReplaceStudioGraph=@{ron=$other}},@{EvaluateStudioGraph=@{target=$null}})
$b=State
if ($a.fingerprint -eq $b.fingerprint) {throw 'Noise seed had no effect'}
Commands @('Undo');if ((State).fingerprint -ne $a.fingerprint) {throw 'CPU fields Undo failed'}
Commands @('Redo');if ((State).fingerprint -ne $b.fingerprint) {throw 'CPU fields Redo failed'}
Commands @(@{SaveScene=@{path='target/i2_graph_roundtrip.scn.json'}})
Commands @(@{SetPlayMode=@{mode='Playing'}},@{ReplaceStudioGraph=@{ron=$graph}},@{EvaluateStudioGraph=@{target=$null}})
Commands @(@{SetPlayMode=@{mode='Edit'}})
if ((State).fingerprint -ne $b.fingerprint) {throw 'CPU fields PIE rollback failed'}
Commands @(@{ReplaceStudioGraph=@{ron=$graph}},@{EvaluateStudioGraph=@{target=$null}})
Commands @(@{LoadScene=@{path='target/i2_graph_roundtrip.scn.json'}});Start-Sleep -Seconds 2
if ((State).fingerprint -ne $b.fingerprint) {throw 'CPU fields scene roundtrip changed values'}
Commands @(@{SetCamera=@{pivot=@(0,10,0);yaw=-0.78;pitch=0.9;distance=70}})
'PASS graph v1 scene migration / CPU determinism / actual mesh and Scatter / Undo / Redo / PIE / scene roundtrip'
