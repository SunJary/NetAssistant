#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

; Build: iscc /DMyAppVersion=1.2.0 packaging/NetAssistant.iss
; Source paths below are resolved relative to this .iss file's directory.
[Setup]
AppId={{E0A6C7F4-9D6B-4A1E-B8C3-4F7D2A9B5E1C}
AppName=NetAssistant
AppVersion={#MyAppVersion}
AppPublisher=SunJary
AppPublisherURL=https://github.com/SunJary/NetAssistant
AppSupportURL=https://github.com/SunJary/NetAssistant/issues
; Per-user install: no UAC prompt, smooth under winget silent install,
; and shortcuts land in the user's Start Menu.
DefaultDirName={localappdata}\Programs\NetAssistant
DefaultGroupName=NetAssistant
OutputDir=dist
OutputBaseFilename=netassistant-windows-x86_64-setup
SetupIconFile=NetAssistant.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
UninstallDisplayName=NetAssistant
UninstallDisplayIcon={app}\netassistant.exe

[Files]
Source: "..\target\x86_64-pc-windows-msvc\release\netassistant.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\NetAssistant"; Filename: "{app}\netassistant.exe"
Name: "{autodesktop}\NetAssistant"; Filename: "{app}\netassistant.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "创建桌面图标"; GroupDescription: "附加图标："

[Run]
Filename: "{app}\netassistant.exe"; Description: "运行 NetAssistant"; Flags: nowait postinstall skipifsilent