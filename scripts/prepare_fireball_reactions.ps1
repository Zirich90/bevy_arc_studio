param(
    [string]$Source = 'target/i2_graph_roundtrip.scn.json',
    [string]$Output = 'target/i2_fireball_reactions.scn.json'
)
$ErrorActionPreference = 'Stop'
$scene = Get-Content $Source -Raw | ConvertFrom-Json -AsHashtable
$state = $scene.resources['bevy_arc_studio::simulation::SimulationState']
$fields = $state.fields
# Scatter is verified by verify_world_graph_brp.ps1. Keep the reaction cells visible.
$fields.scatter = @()
$n = $fields.grid.n
$spacing = $fields.grid.size / ($n - 1)
$area = $spacing * $spacing
$cells = for ($i = 0; $i -lt $n * $n; $i++) {
    $x = ($i % $n) * $spacing - $fields.grid.size / 2
    $z = [math]::Floor($i / $n) * $spacing - $fields.grid.size / 2
    # Fireball is aimed at (3, 0). Separate oil from ice so ash cannot mask melting.
    $oil = if ($x -eq 3 -and $z -eq 0) {
        $fields.grid.heights[$i] = [math]::Min(
            $fields.grid.heights[$i - 1], [math]::Min($fields.grid.heights[$i + 1],
            [math]::Min($fields.grid.heights[$i - $n], $fields.grid.heights[$i + $n]))
        ) - 0.05
        1.0
    } else { 0.0 }
    $ice = if ($x -eq 4 -and $z -eq 0) { 1.0 } else { 0.0 }
    $fields.water_m[$i] = 0.0
    $fields.temperature_c[$i] = 0.0
    @{
        water_kg = $ice
        oil_kg = $oil
        biomass_kg = $fields.biomass[$i] * $area
        ash_kg = 0.0
        sediment_kg = 1600.0 * $area
        energy_j = 0.0
    }
}
$state.physics = @{ cells = @($cells); sunlight_j = 0.0; external_heat_j = 0.0 }
$scene.resources['bevy_arc_studio::arc_core::StudioGraph'] = @{
    version = 2
    nodes = @(
        @{id=1;kind='Fireball';position=@(12,95);value=1;params=@{Fireball=@{energy_j=4000000;radius_m=1.5}}}
        @{id=2;kind='IgniteOil';position=@(225,0);value=1;params=@{IgniteOil=@{ignition_c=1}}}
        @{id=3;kind='Granular';position=@(225,190);value=1;params=@{Granular=@{talus_degrees=0;iterations=8}}}
        @{id=4;kind='MeltSnow';position=@(225,380);value=1;params=@{MeltSnow=@{latent_heat_j_per_kg=250000}}}
    )
    edges = @(@{from=1;to=2}, @{from=1;to=3}, @{from=1;to=4})
}
$scene | ConvertTo-Json -Depth 100 -Compress | Set-Content -LiteralPath $Output

$heightPath = [IO.Path]::GetFullPath("$Output.heights")
$stream = [IO.File]::Create($heightPath)
$writer = [IO.BinaryWriter]::new($stream)
try {
    foreach ($height in $fields.grid.heights) { $writer.Write([single]$height) }
} finally {
    $writer.Dispose()
}
