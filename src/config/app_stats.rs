use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::storage::ConfigStorage;

const STATS_FILENAME: &str = "netassistant_stats.json";

/// 应用统计信息（独立于连接配置，存储于 netassistant_stats.json）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppStats {
    /// 累计不同打开天数
    #[serde(default)]
    pub open_day_count: u32,

    /// 上次打开日期 "2026-08-08"
    #[serde(default)]
    pub last_open_date: Option<String>,

    /// Star 提示已关闭次数（决定 snooze 时长）
    #[serde(default)]
    pub star_prompt_dismissal_count: u32,

    /// 上次关闭 Star 提示的日期
    #[serde(default)]
    pub star_prompt_last_dismissed: Option<String>,

    /// Star 数缓存（避免启动闪烁）
    #[serde(default)]
    pub cached_star_count: Option<u32>,
}

impl AppStats {
    /// 获取统计文件路径
    fn stats_file_path() -> PathBuf {
        ConfigStorage::get_config_dir().join(STATS_FILENAME)
    }

    /// 从文件加载，不存在则返回默认值
    pub fn load() -> Self {
        let path = Self::stats_file_path();
        if !path.exists() {
            return Self::default();
        }
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// 保存到文件
    pub fn save(&self) {
        let path = Self::stats_file_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = fs::write(&path, content) {
                    debug!("[app_stats] 保存统计文件失败: {:?}", e);
                }
            }
            Err(e) => debug!("[app_stats] 序列化失败: {:?}", e),
        }
    }

    /// 记录打开天数（若 last_open_date != today 则递增）
    pub fn record_open_day(&mut self, today: &str) {
        if self.last_open_date.as_deref() != Some(today) {
            self.open_day_count += 1;
            self.last_open_date = Some(today.to_string());
            self.save();
        }
    }

    /// 判断是否应该显示 Star 提示
    ///
    /// 条件：open_day_count >= 7 且满足以下之一：
    /// - 从未关闭（count == 0）
    /// - 关闭 1 次且距上次关闭 >= 30 天
    /// - 关闭 2+ 次且距上次关闭 >= 90 天
    pub fn should_show_star_prompt(&self, today: &str) -> bool {
        if self.open_day_count < 7 {
            return false;
        }
        match self.star_prompt_dismissal_count {
            0 => true,
            1 => days_since(self.star_prompt_last_dismissed.as_deref(), today) >= 30,
            _ => days_since(self.star_prompt_last_dismissed.as_deref(), today) >= 90,
        }
    }

    /// 关闭 Star 提示（递增关闭次数 + 记录日期）
    pub fn dismiss_star_prompt(&mut self, today: &str) {
        self.star_prompt_dismissal_count += 1;
        self.star_prompt_last_dismissed = Some(today.to_string());
        self.save();
    }
}

/// 计算从 `from_date` 到 `today` 的天数差（均为 "YYYY-MM-DD" 格式）
/// 若 from_date 为 None 或解析失败，返回 u32::MAX（视为可立即提示）
fn days_since(from_date: Option<&str>, today: &str) -> u32 {
    let from = match from_date {
        Some(d) => d,
        None => return u32::MAX,
    };
    let parse = |s: &str| -> Option<chrono::NaiveDate> {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
    };
    match (parse(from), parse(today)) {
        (Some(f), Some(t)) => (t - f).num_days().max(0) as u32,
        _ => u32::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_open_day_same_day() {
        let mut stats = AppStats {
            open_day_count: 5,
            last_open_date: Some("2026-08-08".to_string()),
            ..Default::default()
        };
        stats.record_open_day("2026-08-08");
        assert_eq!(stats.open_day_count, 5);
    }

    #[test]
    fn test_record_open_day_new_day() {
        let mut stats = AppStats {
            open_day_count: 5,
            last_open_date: Some("2026-08-07".to_string()),
            ..Default::default()
        };
        stats.record_open_day("2026-08-08");
        assert_eq!(stats.open_day_count, 6);
        assert_eq!(stats.last_open_date, Some("2026-08-08".to_string()));
    }

    #[test]
    fn test_should_show_star_prompt_never_dismissed() {
        let stats = AppStats {
            open_day_count: 7,
            star_prompt_dismissal_count: 0,
            ..Default::default()
        };
        assert!(stats.should_show_star_prompt("2026-08-08"));
    }

    #[test]
    fn test_should_show_star_prompt_not_enough_days() {
        let stats = AppStats {
            open_day_count: 3,
            star_prompt_dismissal_count: 0,
            ..Default::default()
        };
        assert!(!stats.should_show_star_prompt("2026-08-08"));
    }

    #[test]
    fn test_should_show_star_prompt_dismissed_once_within_30_days() {
        let stats = AppStats {
            open_day_count: 10,
            star_prompt_dismissal_count: 1,
            star_prompt_last_dismissed: Some("2026-08-01".to_string()),
            ..Default::default()
        };
        // 8月1日到8月8日 = 7天 < 30天
        assert!(!stats.should_show_star_prompt("2026-08-08"));
    }

    #[test]
    fn test_should_show_star_prompt_dismissed_once_after_30_days() {
        let stats = AppStats {
            open_day_count: 10,
            star_prompt_dismissal_count: 1,
            star_prompt_last_dismissed: Some("2026-07-01".to_string()),
            ..Default::default()
        };
        // 7月1日到8月8日 = 38天 >= 30天
        assert!(stats.should_show_star_prompt("2026-08-08"));
    }

    #[test]
    fn test_should_show_star_prompt_dismissed_twice_within_90_days() {
        let stats = AppStats {
            open_day_count: 10,
            star_prompt_dismissal_count: 2,
            star_prompt_last_dismissed: Some("2026-07-01".to_string()),
            ..Default::default()
        };
        // 7月1日到8月8日 = 38天 < 90天
        assert!(!stats.should_show_star_prompt("2026-08-08"));
    }

    #[test]
    fn test_should_show_star_prompt_dismissed_twice_after_90_days() {
        let stats = AppStats {
            open_day_count: 10,
            star_prompt_dismissal_count: 2,
            star_prompt_last_dismissed: Some("2026-05-01".to_string()),
            ..Default::default()
        };
        // 5月1日到8月8日 = 99天 >= 90天
        assert!(stats.should_show_star_prompt("2026-08-08"));
    }

    #[test]
    fn test_dismiss_star_prompt() {
        let mut stats = AppStats {
            open_day_count: 10,
            star_prompt_dismissal_count: 0,
            ..Default::default()
        };
        stats.dismiss_star_prompt("2026-08-08");
        assert_eq!(stats.star_prompt_dismissal_count, 1);
        assert_eq!(stats.star_prompt_last_dismissed, Some("2026-08-08".to_string()));
    }
}
