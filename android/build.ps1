param([string]$JavaHome,[string]$SdkRoot,[string]$KeyStore,[string]$KeyAlias='lightcontrol')
$ErrorActionPreference='Stop'
$projectRoot=$PSScriptRoot
if(!$JavaHome){$JavaHome=Split-Path (Split-Path (Get-Command javac.exe -ErrorAction Stop).Source)}
if(!$SdkRoot){$SdkRoot=Join-Path $projectRoot 'tools\sdk'}
$platform=Join-Path $SdkRoot 'android-36\android.jar'
$buildTools=Join-Path $SdkRoot 'android-16'
if(!(Test-Path -LiteralPath $platform)){$platform=Join-Path $SdkRoot 'platforms\android-36\android.jar'};if(!(Test-Path -LiteralPath $platform)){throw 'SDK ausente. Execute .\bootstrap.ps1 ou informe -SdkRoot.'}
if(!(Test-Path -LiteralPath (Join-Path $buildTools 'aapt2.exe'))){$buildTools=Join-Path $SdkRoot 'build-tools\36.0.0'}
if(!(Test-Path -LiteralPath $platform)){$platform=Join-Path $SdkRoot 'platforms\android-36\android.jar'}
function Invoke-Checked($exe,$arguments){ & $exe @arguments; if($LASTEXITCODE -ne 0){throw "Falha ao executar $exe (exit $LASTEXITCODE)"} }
$stage=Join-Path $projectRoot ('out\build-'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Force -Path $stage,(Join-Path $stage 'classes'),(Join-Path $stage 'generated'),(Join-Path $stage 'dex') | Out-Null
Invoke-Checked (Join-Path $buildTools 'aapt2.exe') @('compile','--dir',(Join-Path $projectRoot 'app\src\main\res'),'-o',(Join-Path $stage 'resources.zip'))
Invoke-Checked (Join-Path $buildTools 'aapt2.exe') @('link','-I',$platform,'--manifest',(Join-Path $projectRoot 'app\src\main\AndroidManifest.xml'),'-o',(Join-Path $stage 'base.apk'),'--java',(Join-Path $stage 'generated'),'--auto-add-overlay',(Join-Path $stage 'resources.zip'))
$sources=@(Get-ChildItem -LiteralPath (Join-Path $projectRoot 'app\src\main\java'),(Join-Path $stage 'generated') -Recurse -Filter '*.java' | ForEach-Object { $_.FullName })
Invoke-Checked (Join-Path $JavaHome 'bin\javac.exe') (@('--release','11','-encoding','UTF-8','-classpath',$platform,'-d',(Join-Path $stage 'classes'))+$sources)
Invoke-Checked (Join-Path $JavaHome 'bin\jar.exe') @('cf',(Join-Path $stage 'classes.jar'),'-C',(Join-Path $stage 'classes'),'.')
Invoke-Checked (Join-Path $JavaHome 'bin\java.exe') @('-cp',(Join-Path $buildTools 'lib\d8.jar'),'com.android.tools.r8.D8','--release','--min-api','30','--lib',$platform,'--output',(Join-Path $stage 'dex'),(Join-Path $stage 'classes.jar'))
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::Open((Join-Path $stage 'base.apk'),[IO.Compression.ZipArchiveMode]::Update)
try{Get-ChildItem -LiteralPath (Join-Path $stage 'dex') -Filter '*.dex' | ForEach-Object {[IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,$_.FullName,$_.Name,[IO.Compression.CompressionLevel]::Optimal) | Out-Null}}finally{$zip.Dispose()}
Invoke-Checked (Join-Path $buildTools 'zipalign.exe') @('-f','-p','4',(Join-Path $stage 'base.apk'),(Join-Path $stage 'aligned.apk'))
if(!$KeyStore){$KeyStore=Join-Path $projectRoot 'tools\development-signing.p12';$KeyAlias='lightcontrol';$env:LIGHTCONTROL_KEYSTORE_PASSWORD='android';if(!(Test-Path -LiteralPath $KeyStore)){New-Item -ItemType Directory -Force -Path (Split-Path $KeyStore) | Out-Null;Invoke-Checked (Join-Path $JavaHome 'bin\keytool.exe') @('-genkeypair','-keystore',$KeyStore,'-storetype','PKCS12','-storepass:env','LIGHTCONTROL_KEYSTORE_PASSWORD','-alias',$KeyAlias,'-keyalg','RSA','-keysize','3072','-validity','10000','-dname','CN=LightControl Local Development')}}
if(!$env:LIGHTCONTROL_KEYSTORE_PASSWORD){throw 'Defina LIGHTCONTROL_KEYSTORE_PASSWORD para a chave de assinatura.'}
$out=Join-Path (Split-Path $projectRoot) 'dist\LightControl-Android.apk'
New-Item -ItemType Directory -Force -Path (Split-Path $out) | Out-Null
Invoke-Checked (Join-Path $JavaHome 'bin\java.exe') @('-jar',(Join-Path $buildTools 'lib\apksigner.jar'),'sign','--ks',$KeyStore,'--ks-key-alias',$KeyAlias,'--ks-pass','env:LIGHTCONTROL_KEYSTORE_PASSWORD','--out',$out,(Join-Path $stage 'aligned.apk'))
Invoke-Checked (Join-Path $JavaHome 'bin\java.exe') @('-jar',(Join-Path $buildTools 'lib\apksigner.jar'),'verify','--verbose',$out)
Get-Item -LiteralPath $out | Select-Object FullName,Length
Get-FileHash -LiteralPath $out -Algorithm SHA256
