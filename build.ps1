param([switch]$Test, [switch]$DebugBuild)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
if (Test-Path -LiteralPath "$PSScriptRoot\.tools\cargo\bin\cargo.exe") {
    $env:RUSTUP_HOME = "$PSScriptRoot\.tools\rustup"
    $env:CARGO_HOME = "$PSScriptRoot\.tools\cargo"
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
if (!(Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
    throw 'Rust/Cargo nao foi encontrado no sistema. Por favor, instale o Rust atraves de https://rustup.rs e tente novamente.'
}
if ($Test) {
    cargo test -- --test-threads=1
} elseif ($DebugBuild) {
    cargo build
} else {
    cargo build --release
    if ($LASTEXITCODE -eq 0) {
        New-Item -ItemType Directory -Force -Path "$PSScriptRoot\dist" | Out-Null
        try {
            Copy-Item -LiteralPath "$PSScriptRoot\target\release\lightcontrol.exe" -Destination "$PSScriptRoot\dist\LightControl.exe" -Force
            Write-Output 'Pronto: dist\LightControl.exe'
        } catch {
            Write-Warning "Nao foi possivel atualizar dist\LightControl.exe (o arquivo pode estar em execucao). O binario foi gerado em target\release\lightcontrol.exe."
        }
    }
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
