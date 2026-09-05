// 临时端口范围检测
//
// 提供按需检测本机临时端口范围的能力，用于压测配置弹窗的端口上限警告。
// 设计要点:
//   - 第一层: 编译期静态默认值作为阈值，未超过则不调用任何检测 API
//   - 第二层: 超过静态阈值时调用 detect() 获取真实端口范围 (用户可能已自行调优)
//   - 全程只读，无需管理员权限
//   - 检测失败时返回 None，调用方应提示用户手动获取而非回退默认值

use log::warn;

use crate::stress::config::ConnectionMode;

/// TIME_WAIT 时长上限 (毫秒)，用于短连接模式下的端口消耗折算
const TIME_WAIT_MS: u64 = 120_000;

/// 各平台临时端口范围的静态默认值 (编译期判定，用于第一层阈值判断)
#[cfg(target_os = "windows")]
pub const PLATFORM_DEFAULT_COUNT: u32 = 16384;

#[cfg(target_os = "linux")]
pub const PLATFORM_DEFAULT_COUNT: u32 = 28232; // 60999 - 32768 + 1

#[cfg(target_os = "macos")]
pub const PLATFORM_DEFAULT_COUNT: u32 = 16384; // 65535 - 49152 + 1

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub const PLATFORM_DEFAULT_COUNT: u32 = 16384;

/// 静态阈值 (系统默认端口数)，超过此值才触发实际检测
pub const STATIC_THRESHOLD: u32 = PLATFORM_DEFAULT_COUNT;

/// 临时端口范围
#[derive(Debug, Clone)]
pub struct EphemeralPortRange {
    pub start: u16,
    pub count: u32,
}

impl EphemeralPortRange {
    /// 结束端口 (含)
    pub fn end(&self) -> u16 {
        let end_u32 = (self.start as u32)
            .saturating_add(self.count)
            .saturating_sub(1);
        end_u32.min(65535) as u16
    }

    /// 运行时检测本机临时端口范围 (只读，无需管理员权限)
    /// 失败时返回 None，调用方应提示用户手动获取而非回退编造的默认值
    pub fn detect() -> Option<Self> {
        #[cfg(target_os = "windows")]
        {
            Self::detect_windows()
        }
        #[cfg(target_os = "linux")]
        {
            Self::detect_linux()
        }
        #[cfg(target_os = "macos")]
        {
            Self::detect_macos()
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            None
        }
    }

    /// 建议并发上限
    ///
    /// - 长连接 / UDP: count (按系统实际临时端口数, 超出由 OS 报错连接失败)
    /// - 短连接 TCP: count × (interval / TIME_WAIT_MS)
    ///   稳态下 TIME_WAIT 占用端口数 ≈ 并发 × (TIME_WAIT / 间隔)
    ///   公式源自 TCP 四元组 (源IP, 源端口, 目标IP, 目标端口) 在压测同一目标时
    ///   只有源端口可变，TIME_WAIT 期间端口不可复用
    pub fn suggested_max_concurrency(
        &self,
        mode: ConnectionMode,
        send_interval_ms: u64,
        protocol: crate::config::connection::ConnectionType,
    ) -> usize {
        // UDP 不走 TCP 连接，无 TIME_WAIT 问题
        if matches!(protocol, crate::config::connection::ConnectionType::Udp) {
            return self.count as usize;
        }

        let base = self.count as u64; // 按实际端口数, 超出走 OS 报错
        match mode {
            ConnectionMode::Long => base as usize,
            ConnectionMode::Short => {
                if send_interval_ms == 0 {
                    // 间隔为 0: 趋近于 0，调用方应警告用户改用长连接
                    return 1;
                }
                let factor = (send_interval_ms as f64 / TIME_WAIT_MS as f64).min(1.0);
                ((base as f64) * factor) as usize
            }
        }
    }
}

// ===== 平台实现 =====

#[cfg(target_os = "windows")]
impl EphemeralPortRange {
    fn detect_windows() -> Option<Self> {
        use std::os::windows::process::CommandExt;
        // netsh int ipv4 show dynamicport tcp
        // 输出示例:
        //   Start Port : 49152
        //   Number of Ports : 16384
        // CREATE_NO_WINDOW (0x08000000): 避免 GUI 程序启动控制台子进程时弹出黑色 cmd 窗口
        let output = match std::process::Command::new("netsh")
            .args(["int", "ipv4", "show", "dynamicport", "tcp"])
            .creation_flags(0x0800_0000)
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                warn!("[端口检测] 执行 netsh 失败: {}", e);
                return None;
            }
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            warn!(
                "[端口检测] netsh 退出码 {:?}, stdout: {:?}, stderr: {:?}",
                output.status.code(),
                stdout.trim(),
                stderr.trim()
            );
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        match parse_windows_netsh(&text) {
            Some(r) => Some(r),
            None => {
                // 打印 netsh 原始输出, 便于诊断为何正则未匹配 (如本地化/格式变更)
                warn!("[端口检测] 无法解析 netsh 输出: {:?}", text.trim());
                None
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn parse_windows_netsh(text: &str) -> Option<EphemeralPortRange> {
    use regex::Regex;
    // (?s) 让 . 匹配换行, 因为 Start Port 和 Number of Ports 在不同行
    // 正则兼容 netsh 本地化输出:
    //   英文: "Start Port : 49152" / "Number of Ports : 16384"
    //   中文: "启动端口        : 10000" / "端口数          : 55535"
    // 用 非贪婪 .*? 跨行匹配, 字段名用 alternation 同时覆盖中英文。
    let re = Regex::new(
        r"(?s)(?:Start Port|启动端口)\s*:\s*(\d+).*?(?:Number of Ports|端口数)\s*:\s*(\d+)",
    )
    .ok()?;
    let caps = re.captures(text)?;
    let start: u16 = caps.get(1)?.as_str().parse().ok()?;
    let count: u32 = caps.get(2)?.as_str().parse().ok()?;
    if count == 0 {
        return None;
    }
    Some(EphemeralPortRange { start, count })
}

#[cfg(target_os = "linux")]
impl EphemeralPortRange {
    fn detect_linux() -> Option<Self> {
        // /proc/sys/net/ipv4/ip_local_port_range
        // 输出示例: "32768   60999\n"
        let content = match std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range") {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "[端口检测] 读取 /proc/sys/net/ipv4/ip_local_port_range 失败: {}",
                    e
                );
                return None;
            }
        };
        match parse_linux_proc(&content) {
            Some(r) => Some(r),
            None => {
                warn!(
                    "[端口检测] 无法解析 ip_local_port_range 内容: {:?}",
                    content.trim()
                );
                None
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn parse_linux_proc(text: &str) -> Option<EphemeralPortRange> {
    let mut parts = text.split_whitespace();
    let start: u16 = parts.next()?.parse().ok()?;
    let end: u16 = parts.next()?.parse().ok()?;
    if end < start {
        return None;
    }
    let count = (end as u32) - (start as u32) + 1;
    Some(EphemeralPortRange { start, count })
}

#[cfg(target_os = "macos")]
impl EphemeralPortRange {
    fn detect_macos() -> Option<Self> {
        // sysctl -n net.inet.ip.portrange.first  -> 49152
        // sysctl -n net.inet.ip.portrange.last   -> 65535
        let first_out = match std::process::Command::new("sysctl")
            .args(["-n", "net.inet.ip.portrange.first"])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                warn!("[端口检测] 执行 sysctl(portrange.first) 失败: {}", e);
                return None;
            }
        };
        let last_out = match std::process::Command::new("sysctl")
            .args(["-n", "net.inet.ip.portrange.last"])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                warn!("[端口检测] 执行 sysctl(portrange.last) 失败: {}", e);
                return None;
            }
        };
        let first_str = String::from_utf8_lossy(&first_out.stdout);
        let last_str = String::from_utf8_lossy(&last_out.stdout);
        match parse_macos_sysctl(&first_str, &last_str) {
            Some(r) => Some(r),
            None => {
                warn!(
                    "[端口检测] 无法解析 sysctl 输出: first={:?}, last={:?}",
                    first_str.trim(),
                    last_str.trim()
                );
                None
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn parse_macos_sysctl(first: &str, last: &str) -> Option<EphemeralPortRange> {
    let start: u16 = first.trim().parse().ok()?;
    let end: u16 = last.trim().parse().ok()?;
    if end < start {
        return None;
    }
    let count = (end as u32) - (start as u32) + 1;
    Some(EphemeralPortRange { start, count })
}

// ===== 单元测试 =====

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection::ConnectionType;

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_windows_netsh() {
        let sample = "\r\nProtocol tcp Dynamic Port Range\r\n---------------------------------\r\nStart Port      : 49152\r\nNumber of Ports : 16384\r\n";
        let r = parse_windows_netsh(sample).expect("should parse");
        assert_eq!(r.start, 49152);
        assert_eq!(r.count, 16384);
        assert_eq!(r.end(), 65535);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_windows_netsh_chinese() {
        // netsh 在中文 Windows 上的本地化输出
        let sample = "\r\n协议 tcp 动态端口范围\r\n---------------------------------\r\n启动端口        : 10000\r\n端口数          : 55535\r\n";
        let r = parse_windows_netsh(sample).expect("should parse");
        assert_eq!(r.start, 10000);
        assert_eq!(r.count, 55535);
        assert_eq!(r.end(), 65534);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_windows_netsh_invalid() {
        assert!(parse_windows_netsh("garbage").is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_linux_proc() {
        let r = parse_linux_proc("32768   60999\n").expect("should parse");
        assert_eq!(r.start, 32768);
        assert_eq!(r.count, 28232);
        assert_eq!(r.end(), 60999);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_linux_proc_invalid() {
        assert!(parse_linux_proc("not a number").is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_parse_macos_sysctl() {
        let r = parse_macos_sysctl("49152\n", "65535\n").expect("should parse");
        assert_eq!(r.start, 49152);
        assert_eq!(r.count, 16384);
        assert_eq!(r.end(), 65535);
    }

    #[test]
    fn test_suggested_max_long() {
        let r = EphemeralPortRange {
            start: 49152,
            count: 16384,
        };
        // 长连接: 16384 (按实际端口数, 不再打 0.8 折扣)
        let m = r.suggested_max_concurrency(ConnectionMode::Long, 1000, ConnectionType::Tcp);
        assert_eq!(m, 16384);
    }

    #[test]
    fn test_suggested_max_short() {
        let r = EphemeralPortRange {
            start: 49152,
            count: 16384,
        };
        // 短连接: 16384 * (1000 / 120000) = 16384 * 0.00833.. = 136 (向下取整)
        let m = r.suggested_max_concurrency(ConnectionMode::Short, 1000, ConnectionType::Tcp);
        assert_eq!(m, 136);
    }

    #[test]
    fn test_suggested_max_short_zero_interval() {
        let r = EphemeralPortRange {
            start: 49152,
            count: 16384,
        };
        let m = r.suggested_max_concurrency(ConnectionMode::Short, 0, ConnectionType::Tcp);
        assert_eq!(m, 1);
    }

    #[test]
    fn test_suggested_max_udp_no_timewait() {
        let r = EphemeralPortRange {
            start: 49152,
            count: 16384,
        };
        // UDP 无 TIME_WAIT, 短连接模式也不折算
        let m = r.suggested_max_concurrency(ConnectionMode::Short, 1000, ConnectionType::Udp);
        assert_eq!(m, 16384);
    }

    #[test]
    fn test_end_port() {
        let r = EphemeralPortRange {
            start: 49152,
            count: 16384,
        };
        assert_eq!(r.end(), 65535);
    }

    #[test]
    fn test_end_port_overflow() {
        let r = EphemeralPortRange {
            start: 60000,
            count: 100000, // 远超 u16
        };
        assert_eq!(r.end(), 65535);
    }
}
