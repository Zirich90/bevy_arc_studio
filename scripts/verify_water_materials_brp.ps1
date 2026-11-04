param([int]$Port = 15702)
$ErrorActionPreference = 'Stop'
function Rpc($Method, $Params) {
    $body = @{jsonrpc='2.0';id=1;method=$Method;params=$Params} | ConvertTo-Json -Depth 50 -Compress
    $reply = Invoke-RestMethod -Uri "http://127.0.0.1:$Port" -Method Post -ContentType application/json -Body $body -TimeoutSec 20 -NoProxy
    if ($reply.error) { throw ($reply.error | ConvertTo-Json -Compress) }
    $reply.result
}
function Commands($Items) {
    $null = Rpc 'world.insert_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue';value=@{commands=@($Items);log=@()}}
    Start-Sleep -Milliseconds 600
    $result = Rpc 'world.get_resources' @{resource='bevy_arc_studio::mcp_commands::StudioCommandQueue'}
    if ($result.value.log -match '^ERR') { throw ($result.value.log -join '; ') }
}
function Integration {
    Commands @('DescribeIntegration')
    $result = Rpc 'world.get_resources' @{resource='bevy_arc_studio::operators::CommandResults'}
    (($result.value.items | Where-Object op -eq 'DescribeIntegration' | Select-Object -Last 1).data | ConvertFrom-Json)
}
function AssertBindings {
    $integration = Integration
    $sections = @($integration.water_sections)
    if ($sections.Count -ne 2 -or @($sections | Where-Object { $_.triangles -gt 0 }).Count -ne 2) { throw 'Expected two nonempty water draw sections' }
    $global = $sections | Where-Object { $null -eq $_.basin }
    $basin = $sections | Where-Object { $null -ne $_.basin }
    if ($global.binding.material -ne 'env/water' -or $basin.binding.material -ne 'fx/hologram') { throw 'Surface bindings changed or crossed' }
    foreach ($section in $sections) {
        $instance = @($integration.material_instances | Where-Object entity -eq $section.entity)
        if ($instance.Count -ne 1 -or !$instance[0].instance.handle) { throw 'Water binding has no actual generated material instance' }
    }
}
$scene = Get-Content "$PSScriptRoot/../scenes/behavior_pilot.scn.json" -Raw | ConvertFrom-Json
$spec = $scene.resources.'bevy_arc_studio::arc_core::WorldSpec'
$spec.terrain_enabled = $true; $spec.resolution = 65; $spec.size = 64; $spec.erosion_droplets = 0; $spec.thermal_iterations = 0
$spec.forest_density = 0; $spec.boulder_density = 0; $spec.grass_density = 0
$scene.entities = [pscustomobject]@{}
$path = 'target/i2_water_fixture.scn.json'
$scene | ConvertTo-Json -Depth 60 | Set-Content $path -Encoding utf8
$bytes = [Collections.Generic.List[byte]]::new()
for ($z=0; $z -lt 65; $z++) { for ($x=0; $x -lt 65; $x++) {
    [single]$h = 10
    if ($x -ge 17 -and $x -lt 26 -and $z -ge 17 -and $z -lt 26) { $h=7 }
    if ($x -ge 40 -and $x -lt 52 -and $z -ge 40 -and $z -lt 52) { $h=2 }
    $bytes.AddRange([BitConverter]::GetBytes($h))
} }
[IO.File]::WriteAllBytes("$PWD/$path.heights", $bytes.ToArray())
Commands @(@{LoadScene=@{path=$path}})
Start-Sleep -Seconds 2
Commands @(@{SetWaterBody=@{x=-11;z=-11;level=9}})
Commands @(@{SetWaterMaterial=@{seed=$null;binding=@{material='env/water';layout='';parameters=@()}}})
Commands @(@{SetWaterMaterial=@{seed=@(-11,-11);binding=@{material='fx/hologram';layout='';parameters=@()}}})
AssertBindings
Commands @('Undo')
if (((Integration).water_sections | Where-Object { $null -ne $_.basin }).binding) { throw 'Basin binding Undo failed' }
Commands @('Redo'); AssertBindings
Commands @(@{SetPlayMode=@{mode='Playing'}})
Commands @(@{SetWaterMaterial=@{seed=@(-11,-11);binding=$null}})
Commands @(@{SetPlayMode=@{mode='Edit'}}); AssertBindings
Commands @(@{SaveScene=@{path='target/i2_water_roundtrip.scn.json'}})
Commands @(@{SetWaterMaterial=@{seed=$null;binding=$null}})
Commands @(@{LoadScene=@{path='target/i2_water_roundtrip.scn.json'}})
Start-Sleep -Seconds 2
AssertBindings
Commands @(@{SetCamera=@{pivot=@(1,7,1);yaw=-0.78;pitch=1.15;distance=65}})
'PASS water draw sections / independent bindings / Undo / Redo / PIE / scene roundtrip'
