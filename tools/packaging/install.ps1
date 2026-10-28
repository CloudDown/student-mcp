# PowerShell: irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/tools/packaging/install.ps1 | iex
# Downloads the native Windows binaries from the latest GitHub release (no setup.exe).
$ErrorActionPreference = 'Stop'

$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($arch) {
    'X64' { $label = 'windows-x64' }
    'Arm64' { $label = 'windows-arm64' }
    default { throw "Unsupported Windows architecture: $arch" }
}

$base = 'https://github.com/CloudDown/student-mcp/releases/latest/download'
$binaryName = "student-mcp-$label.exe"
$launcherName = "student-mcp-desktop-$label.exe"
$temp = Join-Path ([System.IO.Path]::GetTempPath()) ("student-mcp-" + [guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $temp -Force)

function Get-ReleaseFile([string]$name) {
    $path = Join-Path $temp $name
    $checksum = "$path.sha256"
    Write-Host "Downloading $name..."
    Invoke-WebRequest -Uri "$base/$name" -OutFile $path -UseBasicParsing
    Invoke-WebRequest -Uri "$base/$name.sha256" -OutFile $checksum -UseBasicParsing
    $expected = ((Get-Content -Path $checksum -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash -Path $path -Algorithm SHA256).Hash
    if ($expected.Length -ne 64 -or $expected -ine $actual) {
        throw "The download does not match its SHA-256 checksum: $name"
    }
    return $path
}

try {
    $binary = Get-ReleaseFile $binaryName
    $launcher = Get-ReleaseFile $launcherName

    if ($env:MOODLE_INSTALL_CHECK_ONLY -eq '1') {
        & $binary --help
        if ($LASTEXITCODE -ne 0) { throw "Verification failed (code $LASTEXITCODE)." }
        Write-Host 'Download verified. Check-only mode does not install or launch.'
    } else {
        if (Get-Process -Name 'student-mcp' -ErrorAction SilentlyContinue) {
            throw 'Quit student-mcp from its tray icon, then run the installer again.'
        }
        # Preflight before replacing an existing install.
        & $binary --version
        if ($LASTEXITCODE -ne 0) {
            throw 'This build cannot run on this system. The existing installation was not changed.'
        }

        $installDir = Join-Path $env:LOCALAPPDATA 'student-mcp'
        [void](New-Item -ItemType Directory -Path $installDir -Force)
        $cli = Join-Path $installDir 'student-mcp.exe'
        $desk = Join-Path $installDir 'student-mcp-desktop.exe'
        Copy-Item -Path $binary -Destination $cli -Force
        Copy-Item -Path $launcher -Destination $desk -Force

        $startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
        [void](New-Item -ItemType Directory -Path $startMenu -Force)
        $shortcutPath = Join-Path $startMenu 'student-mcp.lnk'
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $desk
        $shortcut.WorkingDirectory = $installDir
        $shortcut.Description = 'student-mcp'
        $shortcut.Save()

        Start-Process -FilePath $desk
        Write-Host "The student-mcp configuration page is opening. Installed app: $desk"
    }
} finally {
    Remove-Item -Path $temp -Recurse -Force -ErrorAction SilentlyContinue
}
