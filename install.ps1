# PowerShell: irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
# Advanced path: installs the same universal app as the downloadable setup.exe.
$ErrorActionPreference = 'Stop'
$base = 'https://github.com/CloudDown/student-mcp/releases/latest/download'
$name = 'student-mcp-setup.exe'
$temp = Join-Path ([System.IO.Path]::GetTempPath()) ("student-mcp-" + [guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $temp -Force)

try {
    $setup = Join-Path $temp $name
    $checksum = "$setup.sha256"
    Write-Host 'Downloading student-mcp for Windows...'
    Invoke-WebRequest -Uri "$base/$name" -OutFile $setup -UseBasicParsing
    Invoke-WebRequest -Uri "$base/$name.sha256" -OutFile $checksum -UseBasicParsing
    $expected = ((Get-Content -Path $checksum -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash -Path $setup -Algorithm SHA256).Hash
    if ($expected.Length -ne 64 -or $expected -ine $actual) {
        throw 'The download does not match its SHA-256 checksum.'
    }

    if ($env:MOODLE_INSTALL_CHECK_ONLY -eq '1') {
        # Installing a temporary copy with the same AppId would overwrite the
        # real app's uninstall registration/shortcuts. Check only the download.
        Write-Host 'Installer download verified. Check-only mode does not install or launch.'
    } else {
        if (Get-Process -Name 'student-mcp' -ErrorAction SilentlyContinue) {
            throw 'Quit student-mcp from its tray icon, then run the installer again.'
        }
        $installDir = Join-Path $env:LOCALAPPDATA 'student-mcp'
        $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$installDir`"")
        $process = Start-Process -FilePath $setup -ArgumentList $arguments -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "Installation failed (code $($process.ExitCode))." }
        $launcher = Join-Path $env:LOCALAPPDATA 'student-mcp\student-mcp-desktop.exe'
        if (-not (Test-Path -Path $launcher -PathType Leaf)) {
            throw 'The installation did not create the student-mcp app.'
        }
        Start-Process -FilePath $launcher
        Write-Host "The student-mcp window is opening. Installed app: $launcher"
    }
} finally {
    Remove-Item -Path $temp -Recurse -Force -ErrorAction SilentlyContinue
}
