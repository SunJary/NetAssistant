use std::net::SocketAddr;
use crate::network::connection::manager::NetworkConnectionManager;
use crate::core::message_processor::MessageProcessor;

/// 消息发送目标枚举
pub enum MessageTarget {
    /// 广播给所有客户端（仅服务器模式）
    AllClients,
    /// 发送给指定客户端（仅服务器模式）
    SpecificClient(SocketAddr),
    /// 发送给服务器（仅客户端模式）
    Server,
}

/// 消息发送器接口
pub trait MessageSender: Send + Sync + 'static {
    /// 发送文本消息
    async fn send_text_message(
        &self, 
        tab_id: &str, 
        content: &str,
        target: MessageTarget
    ) -> Result<(), super::error::MessageError>;
    
    /// 发送字节消息
    async fn send_bytes_message(
        &self, 
        tab_id: &str, 
        bytes: Vec<u8>,
        target: MessageTarget
    ) -> Result<(), super::error::MessageError>;
}

/// 默认消息发送器实现
pub struct DefaultMessageSender {
    network_manager: std::sync::Arc<tokio::sync::Mutex<NetworkConnectionManager>>,
    message_processor: Arc<dyn MessageProcessor>,
}

impl MessageSender for DefaultMessageSender {
    async fn send_text_message(
        &self, 
        tab_id: &str, 
        content: &str,
        target: MessageTarget
    ) -> Result<(), super::error::MessageError> {
        // 处理文本消息，这里假设默认使用Text类型，实际应用中可能需要根据tab配置确定
        let bytes = content.as_bytes().to_vec();
        self.send_bytes_message(tab_id, bytes, target).await
    }
    
    async fn send_bytes_message(
        &self, 
        tab_id: &str, 
        bytes: Vec<u8>,
        target: MessageTarget
    ) -> Result<(), super::error::MessageError> {
        let mut network_manager = self.network_manager.lock().await;
        
        match target {
            MessageTarget::AllClients => {
                // 广播给所有客户端（仅服务器模式）
                let client_addrs = network_manager.get_server_client_addresses(tab_id);
                if client_addrs.is_empty() {
                    return Err(super::error::MessageError::NoClientsConnected);
                }
                
                // 并行发送给所有客户端
                let mut success_count = 0;
                let mut error_count = 0;
                
                for addr in client_addrs {
                    if let Err(e) = network_manager.send_message_to_server_client(tab_id, addr, bytes.clone()).await {
                        error!("发送给客户端 {} 失败: {:?}", addr, e);
                        error_count += 1;
                    } else {
                        success_count += 1;
                    }
                }
                
                info!("广播完成，成功发送给 {} 个客户端，失败 {} 个", success_count, error_count);
                Ok(())
            },
            MessageTarget::SpecificClient(addr) => {
                // 发送给指定客户端（仅服务器模式）
                if let Err(e) = network_manager.send_message_to_server_client(tab_id, addr, bytes).await {
                    return Err(super::error::MessageError::NetworkError(format!("{}", e)));
                }
                Ok(())
            },
            MessageTarget::Server => {
                // 发送给服务器（仅客户端模式）
                if let Err(e) = network_manager.send_message_to_client(tab_id, bytes).await {
                    return Err(super::error::MessageError::NetworkError(format!("{}", e)));
                }
                Ok(())
            },
        }
    }
}

impl DefaultMessageSender {
    pub fn new(
        network_manager: std::sync::Arc<tokio::sync::Mutex<NetworkConnectionManager>>,
        message_processor: Arc<dyn MessageProcessor>
    ) -> Self {
        Self {
            network_manager,
            message_processor,
        }
    }
}

use std::sync::Arc;
use log::{error, info};