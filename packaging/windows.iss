; Built by scripts/package-desktop.py with Inno Setup 6.3 or newer.
[Setup]
AppId={{89E21526-82CC-4EC9-AEB9-C789AA5A90AB}
AppName=student-mcp
AppVersion={#AppVersion}
AppPublisher=CloudDown
AppPublisherURL=https://github.com/CloudDown/student-mcp
DefaultDirName={localappdata}\student-mcp
PrivilegesRequired=lowest
ArchitecturesAllowed={#Architecture}
ArchitecturesInstallIn64BitMode={#Architecture}
MinVersion=10.0.17763
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename=student-mcp-{#Label}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\student-mcp-desktop.exe

[Files]
Source: "{#Binary}"; DestDir: "{app}"; DestName: "student-mcp.exe"; Flags: ignoreversion
Source: "{#Launcher}"; DestDir: "{app}"; DestName: "student-mcp-desktop.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\student-mcp"; Filename: "{app}\student-mcp-desktop.exe"

[Run]
Filename: "{app}\student-mcp-desktop.exe"; Description: "Open student-mcp"; Flags: nowait postinstall skipifsilent

; Credentials and tunnel keys in the user's home are deliberately not removed.
