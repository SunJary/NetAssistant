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
; 与 Zed 一致: 系统版本过低时由安装器直接拦截, 避免"装完打不开"。
; 16299 = Windows 10 1709; gpui 的 DirectWrite 文本系统要求 1703+, 这里取与 Zed 相同的保守值。
MinVersion=10.0.16299
PrivilegesRequired=lowest
; 自动更新时安装器需要关闭仍在运行的本应用；是否重新拉起由 /RELAUNCH=1 决定
CloseApplications=yes
RestartApplications=no
UninstallDisplayName=NetAssistant
UninstallDisplayIcon={app}\netassistant.exe

[Messages]
; 系统版本低于 MinVersion 时的提示(覆盖 Inno 默认的英文 WindowsVersionNotSupported, 文案与 Zed 相同)
WindowsVersionNotSupported=此程序不支持当前计算机运行的 Windows 版本。

[Files]
Source: "..\target\x86_64-pc-windows-msvc\release\netassistant.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\NetAssistant"; Filename: "{app}\netassistant.exe"
Name: "{autodesktop}\NetAssistant"; Filename: "{app}\netassistant.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "创建桌面图标"; GroupDescription: "附加图标："

[Run]
Filename: "{app}\netassistant.exe"; Description: "运行 NetAssistant"; Flags: nowait postinstall skipifsilent
; 自动更新：安装完成后重新启动新版本（仅 /RELAUNCH=1 时生效）
Filename: "{app}\netassistant.exe"; Flags: nowait; Check: ShouldRelaunch

[Code]
function ShouldRelaunch: Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;