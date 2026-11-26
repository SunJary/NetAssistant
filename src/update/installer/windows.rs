//! Windows：交给一个隐藏的 PowerShell "守卫进程"完成安装。
//!
//! 不在应用退出前直接运行安装包，而是启动 PowerShell 脚本：
//! 1. 轮询等待当前进程退出（避免覆盖正在运行的 exe）；
//! 2. 静默运行 Inno Setup 安装包并等待完成；
//! 3. 重新拉起应用。
//!
//! 路径全部通过环境变量传入，脚本里不拼接路径，也就不存在引号转义问题。

use super::{InstallError, Staged};
use std::process::Command;

/// 启动时不闪控制台窗口。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 只使用单引号字符串，避免 Windows 命令行层面的引号转义问题。
const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$waiterLog = Join-Path $env:TEMP 'netassistant-update-waiter.log'
function Note($message) {
    ((Get-Date).ToString('s') + ' ' + $message) | Out-File -FilePath $waiterLog -Append -Encoding utf8
}
Note ('等待进程退出 pid=' + $env:NETASSISTANT_UPDATE_PID)
while (Get-Process -Id $env:NETASSISTANT_UPDATE_PID -ErrorAction SilentlyContinue) {
    Start-Sleep -Milliseconds 100
}
Note '进程已退出，开始运行安装包'
try {
    $arguments = @(
        '/VERYSILENT',
        '/SUPPRESSMSGBOXES',
        '/NORESTART',
        '/SP-',
        ('/LOG=' + $env:NETASSISTANT_UPDATE_LOG)
    )
    $setup = Start-Process -FilePath $env:NETASSISTANT_UPDATE_SETUP -ArgumentList $arguments -Wait -PassThru
    Note ('安装结束，退出码=' + $setup.ExitCode)
} catch {
    Note ('安装失败: ' + $_.Exception.Message)
}
if ($env:NETASSISTANT_UPDATE_RELAUNCH -eq '1') {
    Note '重新启动应用'
    $app = $env:NETASSISTANT_UPDATE_APP
    # 必须走 ShellExecute：PowerShell 7 的 Start-Process 默认 UseShellExecute=false，
    # 子进程继承不到有效的标准输出句柄，console 程序写日志时会 panic（退出码 101）。
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $app
    $psi.WorkingDirectory = Split-Path $app
    $psi.UseShellExecute = $true
    [System.Diagnostics.Process]::Start($psi) | Out-Null
    Start-Sleep -Seconds 2
    $running = Get-Process -Name netassistant -ErrorAction SilentlyContinue
    if ($running) {
        Note ('新进程运行中 pid=' + (($running | ForEach-Object { $_.Id }) -join ','))
    } else {
        Note '新进程未运行'
    }
}
Note '结束'
"#;

/// 便携版（zip 解压、开发目录直接运行）没有卸载器，直接跑安装包会额外装一份，
/// 应降级为手动下载。
pub fn is_portable_build(exe: &std::path::Path) -> bool {
    match exe.parent() {
        Some(dir) => !dir.join("unins000.exe").is_file(),
        None => true,
    }
}

pub fn apply(staged: &Staged) -> Result<(), InstallError> {
    if !staged.path.is_file() {
        return Err(InstallError::Failed(format!(
            "安装包不存在: {}",
            staged.path.display()
        )));
    }

    let app = std::env::current_exe()
        .map_err(|e| InstallError::Failed(format!("获取当前程序路径失败: {e}")))?;
    if is_portable_build(&app) {
        return Err(InstallError::Unsupported(
            "当前是便携版运行，请手动下载安装包".to_string(),
        ));
    }
    // 安装日志便于排查"退出后到底装没装、装到哪"
    let log_path = std::env::temp_dir().join("netassistant-update.log");

    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            SCRIPT,
        ])
        .env("NETASSISTANT_UPDATE_PID", std::process::id().to_string())
        .env("NETASSISTANT_UPDATE_SETUP", &staged.path)
        .env("NETASSISTANT_UPDATE_LOG", &log_path)
        .env("NETASSISTANT_UPDATE_RELAUNCH", if cfg!(debug_assertions) { "0" } else { "1" })
        .env("NETASSISTANT_UPDATE_APP", app);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| InstallError::Failed(format!("启动更新脚本失败: {e}")))
}
