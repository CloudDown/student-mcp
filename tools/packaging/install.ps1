# PowerShell: irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/tools/packaging/install.ps1 | iex
# Installs the same universal setup.exe linked in the README.
$ErrorActionPreference = 'Stop'
$base = 'https://github.com/CloudDown/student-mcp/releases/latest/download'
$name = 'student-mcp-setup.exe'
$temp = Join-Path ([System.IO.Path]::GetTempPath()) ("student-mcp-" + [guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $temp -Force)

try {
    $setup = Join-Path $temp $name
    $manifest = Join-Path $temp 'SHA256SUMS.txt'
    Write-Host 'Downloading student-mcp for Windows...'
    Invoke-WebRequest -Uri "$base/$name" -OutFile $setup -UseBasicParsing
    Invoke-WebRequest -Uri "$base/SHA256SUMS.txt" -OutFile $manifest -UseBasicParsing
    $entries = @(Get-Content -Path $manifest | Where-Object { ($_ -split '\s+', 3)[1] -eq $name })
    if ($entries.Count -ne 1) { throw "Missing or duplicate checksum for $name." }
    $expected = ($entries[0] -split '\s+')[0]
    $actual = (Get-FileHash -Path $setup -Algorithm SHA256).Hash
    if ($expected.Length -ne 64 -or $expected -ine $actual) {
        throw 'The download does not match its SHA-256 checksum.'
    }

    if ($env:MOODLE_INSTALL_CHECK_ONLY -eq '1') {
        # Do not register an installer in check-only mode.
        Write-Host 'Installer download verified. Check-only mode does not install or launch.'
    } else {
        if (Get-Process -Name 'student-mcp' -ErrorAction SilentlyContinue) {
            throw 'Quit student-mcp from its tray icon, then run the installer again.'
        }
        $installDir = Join-Path $env:LOCALAPPDATA 'student-mcp'
        $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$installDir`"")
        $process = Start-Process -FilePath $setup -ArgumentList $arguments -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "Installation failed (code $($process.ExitCode))." }
        $launcher = Join-Path $installDir 'student-mcp-desktop.exe'
        if (-not (Test-Path -Path $launcher -PathType Leaf)) {
            throw 'The installation did not create the student-mcp app.'
        }
        Start-Process -FilePath $launcher
        Write-Host "The student-mcp configuration page is opening. Installed app: $launcher"
    }
} finally {
    Remove-Item -Path $temp -Recurse -Force -ErrorAction SilentlyContinue
}
