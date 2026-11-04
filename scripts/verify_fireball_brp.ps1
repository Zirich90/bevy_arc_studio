param([int]$Port=15702, [switch]$Reactions, [string]$Screenshot)
$ErrorActionPreference='Stop'
# Reuse the RPC helpers without executing the baseline physics scenario.
$helpers = Get-Content "$PSScriptRoot/verify_physics_brp.ps1" -Raw
$helpers = $helpers.Substring($helpers.IndexOf('function Rpc'), $helpers.IndexOf("Commands @(@{LoadScene") - $helpers.IndexOf('function Rpc'))
. ([scriptblock]::Create($helpers))
$path = 'target/i2_graph_roundtrip.scn.json'
if ($Reactions) {
    & "$PSScriptRoot/prepare_fireball_reactions.ps1"
    $path = 'target/i2_fireball_reactions.scn.json'
}
Commands @(@{LoadScene=@{path=$path}})
if ($Reactions -and $Screenshot) {
    $fixture = Get-Content $path -Raw | ConvertFrom-Json -AsHashtable
    $grid = $fixture.resources['bevy_arc_studio::simulation::SimulationState'].fields.grid
    $center = [int](($grid.n - 1) / 2)
    $height = $grid.heights[$center * $grid.n + $center + 3]
    Commands @(@{SetCamera=@{pivot=@(3,$height,0);yaw=-0.78;pitch=1.45;distance=26}})
}
Commands @(
    @{Custom=@{name='pf2e.spawn_actor';args='{"template":"hero_wizard","pos":[0,0,0],"faction":"Player"}'}}
    @{Custom=@{name='pf2e.spawn_actor';args='{"template":"goblin_warrior","pos":[6,0,0],"faction":"Hostile"}'}}
)
function QueryPath {
    Commands @(@{QueryPath=@{from=@(1,0,0);to=@(5,0,0)}})
    $result = Rpc 'world.get_resources' @{resource='bevy_arc_studio::operators::CommandResults'}
    ($result.value.items | Where-Object op -eq 'QueryPath' | Select-Object -Last 1).data
}
$before=(Integration).simulation
if ($Reactions) { $pathBefore = QueryPath }
Commands @(@{SetPlayMode=@{mode='Playing'}})
try {
    for ($attempt=0; $attempt -lt 12; $attempt++) {
        $null=Rpc 'world.insert_resources' @{
            resource='bevy_arc_pf2e::encounter::CombatInput'
            value=@{hover=$null;orders=@(@{Fireball=@{x=3.0;z=0.0}})}
        }
        $after=(Integration).simulation
        if ($after.last_effect) { break }
    }
    if (!$after.last_effect -or $after.physical_totals.external_heat_j -le 0) {
        throw 'CombatInput did not produce a consumed heat effect'
    }
    if ($Reactions) {
        $pathAfter = QueryPath
        $after = (Integration).simulation
        Write-Host "Theta* before: $pathBefore"
        Write-Host "Theta* after:  $pathAfter"
        if (!$pathBefore -or !$pathAfter -or $pathAfter -eq $pathBefore) {
            throw "Theta* report did not change after Granular"
        }
        $a = $before.physical_totals
        $b = $after.physical_totals
        if ($a.melt_snow_cells -ne 0 -or $b.melt_snow_cells -le 0) {
            throw "MeltSnow did not apply its fusion override to the affected cells"
        }
        if ($a.oil_kg -ne 1 -or $a.ice_kg -ne 1 -or $b.oil_kg -ge $a.oil_kg -or $b.ash_kg -le 0 -or $b.ice_kg -ge $a.ice_kg) {
            throw "Reaction totals before: $($a | ConvertTo-Json -Compress); after: $($b | ConvertTo-Json -Compress)"
        }
        $netEnergy = ($b.energy_j - $a.energy_j) - ($b.external_heat_j - $a.external_heat_j) - ($b.sunlight_j - $a.sunlight_j)
        if ([math]::Abs($b.mass_kg - $a.mass_kg) -gt 0.001 -or [math]::Abs($netEnergy) -gt 0.1) {
            throw "Reaction balance drift: $($b.mass_kg - $a.mass_kg) kg, $netEnergy J"
        }
        if ($after.height_fingerprint -eq $before.height_fingerprint -or $after.terrain_revision -le $before.terrain_revision -or !$after.mesh_matches -or !$after.terrain_matches) {
            throw "Granular: heights $($before.height_fingerprint) -> $($after.height_fingerprint), revision $($before.terrain_revision) -> $($after.terrain_revision), mesh=$($after.mesh_matches), terrain=$($after.terrain_matches), validation=$($after.validation_error)"
        }
    }
    if ($Reactions) {
        Commands @(@{SetPlayMode=@{mode="Paused"}})
        $paused = (Integration).simulation
        $stillPaused = (Integration).simulation
        if ($paused.tick -ne $stillPaused.tick -or $paused.fingerprint -ne $stillPaused.fingerprint) {
            throw "Pause did not freeze the reaction state"
        }
    }
    if ($Screenshot) {
        $null = Rpc 'brp_extras/screenshot' @{path=$Screenshot}
    }
    $after.last_effect | ConvertTo-Json -Depth 8
} finally {
    Commands @(@{SetPlayMode=@{mode='Edit'}})
}
$restored=(Integration).simulation
if ($restored.fingerprint -ne $before.fingerprint -or $restored.last_effect) {
    throw 'Stop did not restore the graph state'
}
if ($Reactions) {
    $a = $before.physical_totals
    $r = $restored.physical_totals
    foreach ($field in @('mass_kg','energy_j','oil_kg','ice_kg','ash_kg','sunlight_j','external_heat_j','melt_snow_cells')) {
        if ($r.$field -ne $a.$field) { throw "Stop did not restore $field" }
    }
    if ((QueryPath) -ne $pathBefore) { throw 'Stop did not restore the path report' }
    'PASS PF2e oil ignition / ice melt / Granular / physical rollback'
}
'PASS PF2e Fireball / consumed heat / Stop'
