param(
    [ValidateSet('CLI', 'Desktop', 'Both')][string]$Mode = 'CLI',
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\LightControl'),
    [string]$SourceDirectory,
    [switch]$NoPath
)
$ErrorActionPreference = 'Stop'
$release = 'https://github.com/yGuilhermy/LightControl/releases/latest/download'
$assets = @()
if ($Mode -ne 'Desktop') { $assets += 'LightControl-CLI.exe' }
if ($Mode -ne 'CLI') { $assets += 'LightControl.exe' }

$stage = Join-Path ([IO.Path]::GetTempPath()) ('lightcontrol-install-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    foreach ($asset in ($assets + 'SHA256SUMS.txt')) {
        $target = Join-Path $stage $asset
        if ($SourceDirectory) {
            Copy-Item -LiteralPath (Join-Path $SourceDirectory $asset) -Destination $target
        } else {
            Invoke-WebRequest -UseBasicParsing -Uri "$release/$asset" -OutFile $target
        }
    }
    $checksums = Get-Content -LiteralPath (Join-Path $stage 'SHA256SUMS.txt')
    foreach ($asset in $assets) {
        $expected = $checksums | Where-Object { $_ -match "^[0-9A-Fa-f]{64}  $([regex]::Escape($asset))$" }
        $actual = (Get-FileHash -LiteralPath (Join-Path $stage $asset) -Algorithm SHA256).Hash
        if (@($expected).Count -ne 1 -or $expected.Substring(0, 64) -ne $actual) {
            throw "SHA-256 inválido para $asset."
        }
    }

    $InstallDir = [IO.Path]::GetFullPath($InstallDir)
    if ($Mode -ne 'CLI') {
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        Copy-Item -LiteralPath (Join-Path $stage 'LightControl.exe') -Destination $InstallDir -Force
        Write-Host "Desktop: $(Join-Path $InstallDir 'LightControl.exe')"
    }
    if ($Mode -ne 'Desktop') {
        $bin = Join-Path $InstallDir 'bin'
        New-Item -ItemType Directory -Force -Path $bin | Out-Null
        $cli = Join-Path $bin 'lightcontrol.exe'
        Copy-Item -LiteralPath (Join-Path $stage 'LightControl-CLI.exe') -Destination $cli -Force
        if (!$NoPath) {
            $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
            if ($bin -notin ($userPath -split ';')) {
                $updated = (@($bin) + @($userPath -split ';' | Where-Object { $_ })) -join ';'
                [Environment]::SetEnvironmentVariable('Path', $updated, 'User')
            }
            if ($bin -notin ($env:Path -split ';')) { $env:Path = "$bin;$env:Path" }
        }
        Write-Host "CLI: $cli"
    }
} finally {
    foreach ($asset in ($assets + 'SHA256SUMS.txt')) {
        $file = Join-Path $stage $asset
        if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file -Force }
    }
    Remove-Item -LiteralPath $stage
}
