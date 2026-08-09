// 压测 CSV 报告格式化
//
// 复用 export.rs 的 csv_escape (已提升为 pub(crate))。

use crate::export::csv_escape;
use crate::stress::events::StressReport;

/// 将最终报告格式化为 CSV 字符串。
///
/// 包含两部分:
/// 1. 摘要区(以 # 开头的注释行): 配置、汇总指标
/// 2. 时序表: 每秒一行采样数据
pub fn format_stress_csv(report: &StressReport) -> String {
    let mut out = String::new();

    // ---- 摘要区 ----
    out.push_str("# NetAssistant 压测报告\n");
    out.push_str(&format!("# 目标: {}:{} ({})\n",
        csv_escape(&report.config.target_address),
        report.config.target_port,
        report.config.protocol));
    out.push_str(&format!("# 模式: {} / {}\n", report.config.mode, report.config.connection_mode));
    out.push_str(&format!("# 并发客户端数: {}\n", report.config.concurrency));
    out.push_str(&format!("# 开始时间: {}\n", csv_escape(&report.start_time)));
    out.push_str(&format!("# 结束时间: {}\n", csv_escape(&report.end_time)));
    out.push_str(&format!("# 持续时长: {} ms\n", report.duration_ms));
    out.push_str(&format!("# 总发包: {}\n", report.total_sent));
    out.push_str(&format!("# 成功: {}\n", report.total_success));
    out.push_str(&format!("# 失败: {}\n", report.total_failure));
    out.push_str(&format!("# 平均 QPS: {:.2}\n", report.avg_qps));
    out.push_str(&format!("# 峰值 QPS: {:.2}\n", report.peak_qps));
    out.push_str(&format!("# 断开连接: {}\n", report.disconnects));
    out.push_str(&format!("# 自动重连: {}\n", report.reconnects));
    if let Some(p) = report.latency_p50_us {
        out.push_str(&format!("# 延迟 p50/p95/p99/avg/max (us): {}/{}/{}/{}/{}\n",
            p,
            report.latency_p95_us.unwrap_or(0),
            report.latency_p99_us.unwrap_or(0),
            report.latency_avg_us.unwrap_or(0),
            report.latency_max_us.unwrap_or(0)));
    }
    out.push_str(&format!("# 发送字节: {}\n", report.bytes_sent));
    out.push_str(&format!("# 接收字节: {}\n", report.bytes_received));
    out.push('\n');

    // ---- 时序表 ----
    out.push_str("秒,发送,成功,失败,QPS,活跃连接,断连,重连,p50_us,p95_us,p99_us,发送字节,接收字节\n");
    for (i, s) in report.per_second_samples.iter().enumerate() {
        out.push_str(&format!(
            "{},{},{},{},{:.2},{},{},{},{},{},{},{},{}\n",
            i + 1,
            s.total_sent,
            s.total_success,
            s.total_failure,
            s.current_qps,
            s.active_connections,
            s.disconnects,
            s.reconnects,
            s.latency_p50_us.unwrap_or(0),
            s.latency_p95_us.unwrap_or(0),
            s.latency_p99_us.unwrap_or(0),
            s.bytes_sent,
            s.bytes_received,
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stress::config::{ConnectionMode, RampUpConfig, StopCondition, StressMode, StressTestConfig};
    use crate::stress::stats::StressStats;
    use crate::config::connection::ConnectionType;

    fn sample_report() -> StressReport {
        let config = StressTestConfig {
            target_address: "127.0.0.1".to_string(),
            target_port: 8080,
            protocol: ConnectionType::Tcp,
            mode: StressMode::PingPong,
            connection_mode: ConnectionMode::Long,
            concurrency: 10,
            message_input_mode: "text".to_string(),
            payload_template: "PING".to_string(),
            send_interval_ms: 100,
            global_qps_limit: None,
            stop_condition: StopCondition::Duration(2),
            ramp_up: RampUpConfig::default(),
            auto_reconnect: true,
            response_validation: None,
            timeout_ms: 3000,
        };
        let s1 = StressStats {
            elapsed_ms: 1000,
            current_qps: 95.0,
            total_sent: 100,
            total_success: 100,
            total_failure: 0,
            active_connections: 10,
            latency_p50_us: Some(500),
            latency_p95_us: Some(800),
            latency_p99_us: Some(1200),
            latency_avg_us: Some(550),
            latency_max_us: Some(1200),
            bytes_sent: 500,
            bytes_received: 400,
            ..Default::default()
        };
        let s2 = StressStats {
            elapsed_ms: 2000,
            current_qps: 100.0,
            total_sent: 200,
            total_success: 200,
            total_failure: 0,
            active_connections: 10,
            latency_p50_us: Some(480),
            latency_p95_us: Some(750),
            latency_p99_us: Some(1100),
            latency_avg_us: Some(520),
            latency_max_us: Some(1100),
            bytes_sent: 1000,
            bytes_received: 800,
            ..Default::default()
        };
        StressReport {
            config,
            start_time: "2026-08-08 10:00:00".to_string(),
            end_time: "2026-08-08 10:00:02".to_string(),
            duration_ms: 2000,
            total_sent: 200,
            total_success: 200,
            total_failure: 0,
            avg_qps: 100.0,
            peak_qps: 100.0,
            disconnects: 0,
            reconnects: 0,
            latency_p50_us: Some(480),
            latency_p95_us: Some(750),
            latency_p99_us: Some(1100),
            latency_avg_us: Some(520),
            latency_max_us: Some(1100),
            bytes_sent: 1000,
            bytes_received: 800,
            per_second_samples: vec![s1, s2],
        }
    }

    #[test]
    fn test_csv_has_summary_and_timeseries() {
        let report = sample_report();
        let csv = format_stress_csv(&report);
        assert!(csv.starts_with("# NetAssistant 压测报告"));
        assert!(csv.contains("# 总发包: 200"));
        assert!(csv.contains("# 平均 QPS: 100.00"));
    }

    #[test]
    fn test_csv_timeseries_header() {
        let report = sample_report();
        let csv = format_stress_csv(&report);
        assert!(csv.contains("秒,发送,成功,失败,QPS,活跃连接,断连,重连,p50_us,p95_us,p99_us,发送字节,接收字节"));
    }

    #[test]
    fn test_csv_timeseries_rows() {
        let report = sample_report();
        let csv = format_stress_csv(&report);
        // 第一秒: 1,100,100,0,95.00,10,...
        assert!(csv.contains("1,100,100,0,95.00,10,"));
        // 第二秒
        assert!(csv.contains("2,200,200,0,100.00,10,"));
    }

    #[test]
    fn test_csv_escape_in_address() {
        let mut report = sample_report();
        report.config.target_address = "host,with,comma".to_string();
        let csv = format_stress_csv(&report);
        assert!(csv.contains("\"host,with,comma\""));
    }

    #[test]
    fn test_csv_no_latency_when_empty() {
        let mut report = sample_report();
        report.latency_p50_us = None;
        let csv = format_stress_csv(&report);
        // 无延迟行
        assert!(!csv.contains("# 延迟 p50"));
    }
}
