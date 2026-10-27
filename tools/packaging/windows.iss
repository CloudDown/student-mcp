; Built by tools/package-desktop.py with Inno Setup 6.3 or newer.
; One installer carries both x64 and ARM64 payloads and picks at install time.
[Setup]
AppId={{89E21526-82CC-4EC9-AEB9-C789AA5A90AB}
AppName=student-mcp
AppVersion={#AppVersion}
AppPublisher=CloudDown
AppPublisherURL=https://github.com/CloudDown/student-mcp
DefaultDirName={localappdata}\student-mcp
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible or arm64
MinVersion=10.0.17763
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename=student-mcp-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\student-mcp-desktop.exe

[Files]
Source: "{#X64Binary}"; DestDir: "{app}"; DestName: "student-mcp.exe"; Flags: ignoreversion; Check: not IsArm64
Source: "{#X64Launcher}"; DestDir: "{app}"; DestName: "student-mcp-desktop.exe"; Flags: ignoreversion; Check: not IsArm64
Source: "{#Arm64Binary}"; DestDir: "{app}"; DestName: "student-mcp.exe"; Flags: ignoreversion; Check: IsArm64
Source: "{#Arm64Launcher}"; DestDir: "{app}"; DestName: "student-mcp-desktop.exe"; Flags: ignoreversion; Check: IsArm64

[Icons]
Name: "{autoprograms}\student-mcp"; Filename: "{app}\student-mcp-desktop.exe"

[Run]
Filename: "{app}\student-mcp-desktop.exe"; Description: "Open student-mcp"; Flags: nowait postinstall skipifsilent

; Credentials and tunnel keys in the user's home are deliberately not removed.
