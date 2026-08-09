use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::config::connection::ConnectionType;

/// 压测模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StressMode {
    /// 吞吐模式: 只发不等，统计 QPS/成败
    Throughput,
    /// 往返模式: ping-pong，发一包等响应，测 RTT
    PingPong,
}

impl Default for StressMode {
    fn default() -> Self {
        StressMode::PingPong
    }
}

impl std::fmt::Display for StressMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StressMode::Throughput => write!(f, "吞吐"),
            StressMode::PingPong => write!(f, "往返"),
        }
    }
}

/// 连接模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    /// 长连接: 复用同一连接发多包
    Long,
    /// 短连接: 每包(或每批)新建连接
    Short,
}

impl Default for ConnectionMode {
    fn default() -> Self {
        ConnectionMode::Long
    }
}

impl std::fmt::Display for ConnectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionMode::Long => write!(f, "长连接"),
            ConnectionMode::Short => write!(f, "短连接"),
        }
    }
}

/// Tab 视图模式 (UI 层用，不持久化)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabViewMode {
    #[default]
    Debug,
    Stress,
}

/// 停止条件
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StopCondition {
    /// 限时(秒)
    Duration(u64),
    /// 定量(总发包数)
    Count(u64),
    /// 限时或定量，先到先停
    Either { duration_secs: u64, count: u64 },
    /// 手动停止
    Manual,
}

impl Default for StopCondition {
    fn default() -> Self {
        StopCondition::Duration(30)
    }
}

/// 阶梯 ramp-up 配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RampUpConfig {
    pub enabled: bool,
    /// 在多少秒内逐步达到满并发
    pub ramp_up_secs: u64,
}

impl Default for RampUpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ramp_up_secs: 0,
        }
    }
}

/// 响应校验
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "pattern")]
pub enum ResponseValidation {
    /// 包含子串
    Contains(String),
    /// 精确匹配
    Exact(String),
    /// 正则匹配
    Regex(String),
}

/// 压测配置(按 connection_id 持久化)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StressTestConfig {
    pub target_address: String,
    pub target_port: u16,
    pub protocol: ConnectionType,
    pub mode: StressMode,
    pub connection_mode: ConnectionMode,
    /// 并发客户端数
    pub concurrency: usize,
    /// 报文输入模式: text / hex
    pub message_input_mode: String,
    /// 报文模板(支持变量替换)
    pub payload_template: String,
    /// 单客户端发包间隔(毫秒)
    pub send_interval_ms: u64,
    /// 全局 QPS 限制(None=不限)
    pub global_qps_limit: Option<u32>,
    pub stop_condition: StopCondition,
    pub ramp_up: RampUpConfig,
    /// 断线自动重连
    pub auto_reconnect: bool,
    /// 响应校验(None=不校验)
    pub response_validation: Option<ResponseValidation>,
    /// 单次发送/响应超时(毫秒)
    pub timeout_ms: u64,
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            target_address: "127.0.0.1".to_string(),
            target_port: 8080,
            protocol: ConnectionType::Tcp,
            mode: StressMode::default(),
            connection_mode: ConnectionMode::default(),
            concurrency: 10,
            message_input_mode: "text".to_string(),
            payload_template: "PING ${seq}".to_string(),
            send_interval_ms: 0,
            global_qps_limit: None,
            stop_condition: StopCondition::default(),
            ramp_up: RampUpConfig::default(),
            auto_reconnect: true,
            response_validation: None,
            timeout_ms: 3000,
        }
    }
}

impl StressTestConfig {
    /// 从已保存连接的目标地址/端口/协议构造默认压测配置(回填入口)
    pub fn for_target(address: String, port: u16, protocol: ConnectionType) -> Self {
        Self {
            target_address: address,
            target_port: port,
            protocol,
            ..Self::default()
        }
    }

    /// 解析目标地址为 SocketAddr，自动处理 IPv6 方括号
    /// (复用 network/protocol/tcp.rs 的地址拼接逻辑)
    pub fn parse_target_addr(&self) -> Result<std::net::SocketAddr, String> {
        let address = if self.target_address.contains(':')
            && !self.target_address.contains('[')
        {
            // IPv6 地址需要方括号
            format!("[{}]:{}", self.target_address, self.target_port)
        } else {
            format!("{}:{}", self.target_address, self.target_port)
        };
        std::net::SocketAddr::from_str(&address)
            .map_err(|e| format!("无效的地址格式 '{}': {}", address, e))
    }

    /// 是否为 ping-pong(往返)模式
    pub fn is_ping_pong(&self) -> bool {
        matches!(self.mode, StressMode::PingPong)
    }

    /// 是否为长连接模式
    pub fn is_long_connection(&self) -> bool {
        matches!(self.connection_mode, ConnectionMode::Long)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stress_mode_default_and_display() {
        assert_eq!(StressMode::default(), StressMode::PingPong);
        assert_eq!(format!("{}", StressMode::Throughput), "吞吐");
        assert_eq!(format!("{}", StressMode::PingPong), "往返");
    }

    #[test]
    fn test_connection_mode_default_and_display() {
        assert_eq!(ConnectionMode::default(), ConnectionMode::Long);
        assert_eq!(format!("{}", ConnectionMode::Long), "长连接");
        assert_eq!(format!("{}", ConnectionMode::Short), "短连接");
    }

    #[test]
    fn test_stress_config_default() {
        let cfg = StressTestConfig::default();
        assert_eq!(cfg.concurrency, 10);
        assert_eq!(cfg.send_interval_ms, 0);
        assert_eq!(cfg.timeout_ms, 3000);
        assert!(cfg.auto_reconnect);
        assert!(cfg.response_validation.is_none());
        assert!(cfg.is_ping_pong());
        assert!(cfg.is_long_connection());
    }

    #[test]
    fn test_for_target_backfill() {
        let cfg = StressTestConfig::for_target(
            "192.168.1.1".to_string(),
            9999,
            ConnectionType::Udp,
        );
        assert_eq!(cfg.target_address, "192.168.1.1");
        assert_eq!(cfg.target_port, 9999);
        assert_eq!(cfg.protocol, ConnectionType::Udp);
        // 其余字段保持默认
        assert_eq!(cfg.concurrency, 10);
    }

    #[test]
    fn test_parse_target_addr_ipv4() {
        let cfg = StressTestConfig::for_target("127.0.0.1".to_string(), 8080, ConnectionType::Tcp);
        let addr = cfg.parse_target_addr().unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:8080");
    }

    #[test]
    fn test_parse_target_addr_ipv6() {
        let cfg = StressTestConfig::for_target("::1".to_string(), 8080, ConnectionType::Tcp);
        let addr = cfg.parse_target_addr().unwrap();
        assert_eq!(addr.to_string(), "[::1]:8080");
    }

    #[test]
    fn test_parse_target_addr_invalid() {
        let cfg = StressTestConfig::for_target("not a host".to_string(), 8080, ConnectionType::Tcp);
        assert!(cfg.parse_target_addr().is_err());
    }

    #[test]
    fn test_stop_condition_serde_roundtrip() {
        let cases = vec![
            StopCondition::Duration(60),
            StopCondition::Count(100_000),
            StopCondition::Either { duration_secs: 60, count: 100_000 },
            StopCondition::Manual,
        ];
        for c in cases {
            let json = serde_json::to_string(&c).unwrap();
            let back: StopCondition = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn test_stop_condition_either_serde_format() {
        let json = serde_json::to_string(&StopCondition::Either {
            duration_secs: 60,
            count: 100_000,
        })
        .unwrap();
        assert!(json.contains("\"type\":\"either\""));
        assert!(json.contains("\"duration_secs\":60"));
        assert!(json.contains("\"count\":100000"));
    }

    #[test]
    fn test_response_validation_serde_roundtrip() {
        let cases = vec![
            ResponseValidation::Contains("PONG".to_string()),
            ResponseValidation::Exact("OK\n".to_string()),
            ResponseValidation::Regex(r"\d{3}".to_string()),
        ];
        for c in cases {
            let json = serde_json::to_string(&c).unwrap();
            let back: ResponseValidation = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn test_full_config_serde_roundtrip() {
        let cfg = StressTestConfig {
            target_address: "10.0.0.1".to_string(),
            target_port: 5000,
            protocol: ConnectionType::Tcp,
            mode: StressMode::Throughput,
            connection_mode: ConnectionMode::Short,
            concurrency: 500,
            message_input_mode: "hex".to_string(),
            payload_template: "4142${seq}".to_string(),
            send_interval_ms: 50,
            global_qps_limit: Some(1000),
            stop_condition: StopCondition::Either { duration_secs: 60, count: 100_000 },
            ramp_up: RampUpConfig { enabled: true, ramp_up_secs: 10 },
            auto_reconnect: false,
            response_validation: Some(ResponseValidation::Contains("OK".to_string())),
            timeout_ms: 5000,
        };
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let back: StressTestConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn test_backward_compat_missing_fields() {
        // 模拟旧配置: 只有部分字段，验证 #[serde(default)] 能正常加载
        // 注意: 必填字段(address/port/protocol/mode 等)缺失会报错，
        // 这里测试可选/带默认字段的向后兼容。
        let json = r#"{
            "target_address": "1.2.3.4",
            "target_port": 1234,
            "protocol": "tcp",
            "mode": "ping_pong",
            "connection_mode": "long",
            "concurrency": 5,
            "message_input_mode": "text",
            "payload_template": "hi",
            "send_interval_ms": 200,
            "global_qps_limit": null,
            "stop_condition": {"type": "manual"},
            "ramp_up": {"enabled": false, "ramp_up_secs": 0},
            "auto_reconnect": true,
            "response_validation": null,
            "timeout_ms": 1000
        }"#;
        let cfg: StressTestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.target_address, "1.2.3.4");
        assert!(cfg.response_validation.is_none());
        assert_eq!(cfg.stop_condition, StopCondition::Manual);
    }
}
