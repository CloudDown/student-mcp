# PowerShell: irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
$ErrorActionPreference = 'Stop'
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($arch) {
    'X64' { $platform = 'windows-x64' }
    'Arm64' { $platform = 'windows-arm64' }
    default { throw "Architecture Windows non prise en charge : $arch" }
}

$base = 'https://github.com/CloudDown/student-mcp/releases/latest/download'
$name = "student-mcp-$platform.zip"
$temp = Join-Path ([System.IO.Path]::GetTempPath()) ("moodle-mcp-" + [guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $temp -Force)

try {
    $zip = Join-Path $temp $name
    $checksum = "$zip.sha256"
    Write-Host "Téléchargement de Moodle MCP pour $platform..."
    Invoke-WebRequest -Uri "$base/$name" -OutFile $zip -UseBasicParsing
    Invoke-WebRequest -Uri "$base/$name.sha256" -OutFile $checksum -UseBasicParsing
    $expected = ((Get-Content -Path $checksum -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash -Path $zip -Algorithm SHA256).Hash
    if ($expected.Length -ne 64 -or $expected -ine $actual) {
        throw 'Le téléchargement ne correspond pas à son empreinte SHA-256.'
    }

    $folder = Join-Path $temp 'app'
    Expand-Archive -Path $zip -DestinationPath $folder -Force
    $binary = Join-Path $folder 'student-mcp.exe'
    if (-not (Test-Path -Path $binary -PathType Leaf)) {
        throw 'Le téléchargement ne contient pas le programme attendu.'
    }
    if ($env:MOODLE_INSTALL_CHECK_ONLY -eq '1') {
        & $binary --help
        if ($LASTEXITCODE -ne 0) { throw "Vérification impossible (code $LASTEXITCODE)." }
    } else {
        # A new process attached to this console keeps the Moodle prompts
        # interactive even when this script came from `irm ... | iex`.
        $result = Start-Process -FilePath $binary -ArgumentList 'install' -NoNewWindow -Wait -PassThru
        if ($result.ExitCode -ne 0) { throw "Installation interrompue (code $($result.ExitCode))." }
    }
} finally {
    Remove-Item -Path $temp -Recurse -Force -ErrorAction SilentlyContinue
}
