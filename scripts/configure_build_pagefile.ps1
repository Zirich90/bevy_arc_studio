#requires -PSEdition Desktop
param([ValidateSet('Inspect','Apply','Restore')][string]$Action = 'Inspect')
$ErrorActionPreference = 'Stop'
$studioRoot = Split-Path -Parent $PSScriptRoot
$backupPath = Join-Path $studioRoot 'target/pagefile-backup.json'
$resultPath = Join-Path $studioRoot 'target/pagefile-result.json'
$memoryKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management'
$pagePath = 'E:\pagefile.sys'
New-Item -ItemType Directory -Force -Path (Join-Path $studioRoot 'target/tmp') | Out-Null
$env:TEMP = Join-Path $studioRoot 'target/tmp'
$env:TMP = $env:TEMP

function Snapshot {
    $os = Get-WmiObject Win32_OperatingSystem
    [ordered]@{
        AutomaticManagedPagefile = (Get-WmiObject Win32_ComputerSystem).AutomaticManagedPagefile
        PagingFiles = @((Get-ItemProperty -LiteralPath $memoryKey).PagingFiles)
        Active = @(Get-WmiObject Win32_PageFileUsage | Select-Object Name,AllocatedBaseSize,CurrentUsage)
        Settings = @(Get-WmiObject Win32_PageFileSetting | Select-Object Name,InitialSize,MaximumSize)
        FreeVirtualMiB = [math]::Round($os.FreeVirtualMemory / 1024)
        CommitLimitMiB = [math]::Round($os.TotalVirtualMemorySize / 1024)
    }
}
function RestoreSettings($saved) {
    $computer = Get-WmiObject Win32_ComputerSystem -EnableAllPrivileges
    $computer.AutomaticManagedPagefile = [bool]$saved.AutomaticManagedPagefile
    $null = $computer.Put()
    Set-ItemProperty -LiteralPath $memoryKey -Name PagingFiles -Value ([string[]]$saved.PagingFiles)
}

try {
    if ($Action -eq 'Inspect') {
        Snapshot | ConvertTo-Json -Depth 5
        exit 0
    }
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Administrator elevation is required. Run this script using RunAs.'
    }
    if ($Action -eq 'Restore') {
        $saved = Get-Content -LiteralPath $backupPath -Raw | ConvertFrom-Json
        RestoreSettings $saved
    } else {
        $drive = Get-WmiObject Win32_LogicalDisk -Filter "DeviceID='E:'"
        if (!$drive -or $drive.FileSystem -ne 'NTFS' -or [uint64]$drive.FreeSpace -lt 48GB) {
            throw 'E: must be NTFS with at least 48 GiB free before allocating a 32 GiB pagefile.'
        }
        $before = Snapshot
        if (!(Test-Path -LiteralPath $backupPath)) {
            $before | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $backupPath -Encoding UTF8
        }
        try {
            $computer = Get-WmiObject Win32_ComputerSystem -EnableAllPrivileges
            $computer.AutomaticManagedPagefile = $false
            $null = $computer.Put()
            # Retain all active pagefiles when switching from global automatic placement.
            foreach ($active in $before.Active) {
                $existing = Get-WmiObject Win32_PageFileSetting | Where-Object Name -eq $active.Name
                if (!$existing) {
                    $null = Set-WmiInstance -Class Win32_PageFileSetting -EnableAllPrivileges -Arguments @{Name=$active.Name;InitialSize=0;MaximumSize=0}
                }
            }
            $page = Get-WmiObject Win32_PageFileSetting | Where-Object Name -eq $pagePath
            if ($page) {
                $page.InitialSize = [uint32][math]::Max(32768, $page.InitialSize)
                $page.MaximumSize = [uint32][math]::Max(65536, $page.MaximumSize)
                $null = $page.Put()
            } else {
                $null = Set-WmiInstance -Class Win32_PageFileSetting -EnableAllPrivileges -Arguments @{Name=$pagePath;InitialSize=32768;MaximumSize=65536}
            }
            $page = Get-WmiObject Win32_PageFileSetting | Where-Object Name -eq $pagePath
            if (!$page -or $page.InitialSize -lt 32768 -or $page.MaximumSize -lt 65536) {
                throw 'Pagefile setting readback failed.'
            }
        } catch {
            RestoreSettings $before
            throw
        }
    }
    $state = Snapshot
    $activePage = @($state.Active | Where-Object { $_.Name -eq $pagePath -and $_.AllocatedBaseSize -ge 32768 })
    $restartRequired = $Action -eq 'Restore' -or $activePage.Count -eq 0
    @{ok=$true;action=$Action;restartRequired=$restartRequired;backup=$backupPath;state=$state} |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $resultPath -Encoding UTF8
} catch {
    @{ok=$false;action=$Action;error=$_.Exception.Message} |
        ConvertTo-Json | Set-Content -LiteralPath $resultPath -Encoding UTF8
    exit 1
}
