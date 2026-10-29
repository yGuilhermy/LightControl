param([string]$SdkRoot=(Join-Path $PSScriptRoot 'tools\sdk'))
$ErrorActionPreference='Stop'
$toolsDir=Join-Path $PSScriptRoot 'tools'
New-Item -ItemType Directory -Force -Path $toolsDir,$SdkRoot | Out-Null
foreach($package in @(@('platform-36_r02.zip','platform.zip'),@('build-tools_r36_windows.zip','build-tools.zip'))){
 $archive=Join-Path $toolsDir $package[1]
 if(!(Test-Path -LiteralPath $archive)){Invoke-WebRequest -Uri ('https://dl.google.com/android/repository/'+$package[0]) -OutFile $archive}
 Expand-Archive -LiteralPath $archive -DestinationPath $SdkRoot -Force
}
Write-Host 'SDK oficial pronto. Execute .\build.ps1 com JDK 17 ou superior.'
