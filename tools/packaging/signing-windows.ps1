$ErrorActionPreference = 'Stop'
if (-not $env:WINDOWS_CERTIFICATE_BASE64) { exit 0 }
if (-not $env:WINDOWS_CERTIFICATE_PASSWORD) { throw 'Missing WINDOWS_CERTIFICATE_PASSWORD' }
$path = Join-Path $env:RUNNER_TEMP 'student-signing.pfx'
try {
    [IO.File]::WriteAllBytes($path, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE_BASE64))
    $password = ConvertTo-SecureString $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force
    $certificate = Import-PfxCertificate -FilePath $path -CertStoreLocation Cert:\CurrentUser\My -Password $password
    $signer = @($certificate | Where-Object { $_.HasPrivateKey })
    if ($signer.Count -ne 1) { throw 'Expected one signing certificate with a private key' }
    "WINDOWS_SIGNING_THUMBPRINT=$($signer[0].Thumbprint)" >> $env:GITHUB_ENV
    $tool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' | Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $tool) { throw 'Windows SDK signtool not found' }
    "SIGNTOOL=$($tool.FullName)" >> $env:GITHUB_ENV
} finally {
    Remove-Item $path -Force -ErrorAction SilentlyContinue
}
