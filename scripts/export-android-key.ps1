#Requires -Version 7.2
param([string]$CredentialPath,[string]$OutputPath=(Join-Path $PSScriptRoot '..\dist\device-transfer.lck'),[Security.SecureString]$Password)
$ErrorActionPreference='Stop'
if(!$CredentialPath){$CredentialPath=Join-Path $env:LOCALAPPDATA 'LightControl\device.dpapi';if(!(Test-Path -LiteralPath $CredentialPath)){$CredentialPath=Join-Path $PSScriptRoot '..\data\device.dpapi'}}
if(!(Test-Path -LiteralPath $CredentialPath)){throw 'Credencial DPAPI não encontrada. Informe -CredentialPath.'}
if(!$Password){$Password=Read-Host 'Crie uma senha para importar a chave no Android (mínimo 12 caracteres)' -AsSecureString;$confirm=Read-Host 'Confirme a senha' -AsSecureString}
$plainPassword=[Net.NetworkCredential]::new('', $password).Password
if($plainPassword.Length -lt 12){throw 'Use uma senha de pelo menos 12 caracteres.'}
if($confirm -and $plainPassword -cne [Net.NetworkCredential]::new('', $confirm).Password){throw 'As senhas são diferentes.'}
$raw=$null;$derived=$null;$passwordBytes=$null
try{
 Add-Type -AssemblyName System.Security.Cryptography.ProtectedData
 $raw=[Security.Cryptography.ProtectedData]::Unprotect([IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $CredentialPath)), $null,[Security.Cryptography.DataProtectionScope]::CurrentUser)
 $credential=[Text.Encoding]::UTF8.GetString($raw) | ConvertFrom-Json
 if(!$credential.id -or !$credential.ip -or [Text.Encoding]::UTF8.GetByteCount($credential.key) -ne 16){throw 'Credencial inválida.'}
 $salt=[Security.Cryptography.RandomNumberGenerator]::GetBytes(16)
 $iv=[Security.Cryptography.RandomNumberGenerator]::GetBytes(12)
 $passwordBytes=[Text.Encoding]::UTF8.GetBytes($plainPassword)
 $derived=[Security.Cryptography.Rfc2898DeriveBytes]::Pbkdf2($passwordBytes,$salt,210000,[Security.Cryptography.HashAlgorithmName]::SHA256,32)
 $cipher=[byte[]]::new($raw.Length);$tag=[byte[]]::new(16)
 $aes=[Security.Cryptography.AesGcm]::new($derived)
 try{$aes.Encrypt($iv,$raw,$cipher,$tag,[Text.Encoding]::UTF8.GetBytes('LightControl transfer v1'))}finally{$aes.Dispose()}
 $envelope=@{version=1;iterations=210000;salt=[Convert]::ToBase64String($salt);iv=[Convert]::ToBase64String($iv);ciphertext=[Convert]::ToBase64String([byte[]]($cipher+$tag))} | ConvertTo-Json
 $resolvedOutput=[IO.Path]::GetFullPath($OutputPath)
 if(Test-Path -LiteralPath $resolvedOutput){throw 'O arquivo de saída já existe. Escolha outro -OutputPath.'}
 [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($resolvedOutput)) | Out-Null
 [IO.File]::WriteAllText($resolvedOutput,$envelope,[Text.UTF8Encoding]::new($false))
 Write-Host "Arquivo cifrado: $resolvedOutput"
 Write-Host 'No Android: Ajustes → Importar chave do PC. A senha será pedida no celular.'
}finally{
 if($raw){[Array]::Clear($raw,0,$raw.Length)}
 if($derived){[Array]::Clear($derived,0,$derived.Length)}
 if($passwordBytes){[Array]::Clear($passwordBytes,0,$passwordBytes.Length)}
 $plainPassword=$null;$credential=$null;$password.Dispose();if($confirm){$confirm.Dispose()}
}
