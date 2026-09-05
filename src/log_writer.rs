use crate::message::{Message, MessageDirection};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Mutex;

/// 批量落盘的时间阈值
///
/// 消息洪泛下逐条 flush 是 I/O 瓶颈;改为距上次落盘超过该阈值才 flush,
/// 高频写入时 flush 频率上限 = 1/FLUSH_INTERVAL,尾部数据最多延迟一个阈值周期。
const FLUSH_INTERVAL: Duration = Duration::from_millis(100);

/// writer 内部状态: 缓冲写入器 + 上次落盘时间
struct WriterInner {
    writer: BufWriter<File>,
    last_flush: Instant,
}

/// 异步日志写入器
///
/// 将通信记录实时写入本地文件，使用缓冲写入提高性能。
/// 支持追加模式，断开连接时 flush 确保数据完整。
pub struct LogWriter {
    writer: Option<Arc<Mutex<WriterInner>>>,
}

impl LogWriter {
    /// 创建新的日志写入器并打开文件
    ///
    /// 以追加模式打开文件，如果文件不存在则创建。
    pub async fn open(path: PathBuf) -> std::io::Result<Self> {
        let file = File::create(&path).await?;
        let writer = BufWriter::new(file);

        Ok(Self {
            writer: Some(Arc::new(Mutex::new(WriterInner {
                writer,
                last_flush: Instant::now(),
            }))),
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

            let mut inner = writer.lock().await;
            // 写入失败只记录错误，不中断程序
            if let Err(e) = inner.writer.write_all(line.as_bytes()).await {
                log::error!("[日志写入] 写入失败: {:?}", e);
            }
            // 按时间阈值批量落盘: 高频写入时最多每 FLUSH_INTERVAL 一次 syscall,
            // 缓冲写满时 BufWriter 也会自动落盘,不会无限积压
            if inner.last_flush.elapsed() >= FLUSH_INTERVAL {
                if let Err(e) = inner.writer.flush().await {
                    log::error!("[日志写入] flush 失败: {:?}", e);
                }
                inner.last_flush = Instant::now();
            }
        }
    }

    /// 刷新缓冲区并关闭日志文件
    pub async fn close(&mut self) {
        if let Some(writer) = self.writer.take() {
            let mut inner = writer.lock().await;
            if let Err(e) = inner.writer.flush().await {
                log::error!("[日志写入] 关闭时 flush 失败: {:?}", e);
            }
            // BufWriter drop 时会自动 flush，但显式关闭更安全
            let _ = inner.writer.get_mut().shutdown().await;
        }
    }
}
