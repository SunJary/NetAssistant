use crate::message::{Message, MessageDirection};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 异步日志写入器
///
/// 将通信记录实时写入本地文件，使用缓冲写入提高性能。
/// 支持追加模式，断开连接时 flush 确保数据完整。
pub struct LogWriter {
    writer: Option<Arc<Mutex<BufWriter<File>>>>,
}

impl LogWriter {
    /// 创建新的日志写入器并打开文件
    ///
    /// 以追加模式打开文件，如果文件不存在则创建。
    pub async fn open(path: PathBuf) -> std::io::Result<Self> {
        let file = File::create(&path).await?;
        let writer = BufWriter::new(file);

        Ok(Self {
            writer: Some(Arc::new(Mutex::new(writer))),
        })
    }

    /// 生成默认日志文件路径（数字递增）
    ///
    /// 格式：{documents_dir}/NetAssistant/logs/{connection_label}_{n}.log
    /// 自动检测目录下已有文件，递增序号避免覆盖
    pub fn default_log_path(connection_label: &str) -> PathBuf {
        let mut dir = dirs::document_dir().unwrap_or_else(|| PathBuf::from("."));
        dir.push("NetAssistant");
        dir.push("logs");

        // 确保目录存在
        let _ = std::fs::create_dir_all(&dir);

        // 扫描目录，找到该连接前缀的最大序号
        let prefix = format!("{}_", connection_label);
        let mut max_num = 0;

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                if name.starts_with(&prefix) && name.ends_with(".log") {
                    // 提取序号: prefix{n}.log
                    let num_part = &name[prefix.len()..name.len() - 4];
                    if let Ok(n) = num_part.parse::<u32>() {
                        max_num = max_num.max(n);
                    }
                }
            }
        }

        // 下一个序号
        let next_num = max_num + 1;
        let filename = format!("{}_{}.log", connection_label, next_num);

        dir.push(filename);
        dir
    }

    /// 写入一条消息到日志文件
    pub async fn write_message(&self, message: &Message) {
        if let Some(writer) = &self.writer {
            let direction = match message.direction {
                MessageDirection::Sent => "发送",
                MessageDirection::Received => "接收",
            };

            let source_part = match &message.source {
                Some(src) => format!(" ({})", src),
                None => String::new(),
            };

            let line = format!(
                "[{}] {}{} {}\n",
                direction,
                message.timestamp,
                source_part,
                message.get_content_by_type()
            );

            let mut writer = writer.lock().await;
            // 写入失败只记录错误，不中断程序
            if let Err(e) = writer.write_all(line.as_bytes()).await {
                log::error!("[日志写入] 写入失败: {:?}", e);
            }
            // 每条消息都 flush，确保数据实时落盘
            if let Err(e) = writer.flush().await {
                log::error!("[日志写入] flush 失败: {:?}", e);
            }
        }
    }

    /// 刷新缓冲区并关闭日志文件
    pub async fn close(&mut self) {
        if let Some(writer) = self.writer.take() {
            let mut writer = writer.lock().await;
            if let Err(e) = writer.flush().await {
                log::error!("[日志写入] 关闭时 flush 失败: {:?}", e);
            }
            // BufWriter drop 时会自动 flush，但显式关闭更安全
            let _ = writer.get_mut().shutdown().await;
        }
    }
}
