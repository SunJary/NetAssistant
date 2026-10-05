use crate::network::events::ConnectionEvent;
use smol::channel::Sender;
use std::any::Any;
use std::future::Future;
use std::pin::Pin;

/// 网络连接接口
pub trait NetworkConnection: Send + Sync {
    /// 建立连接
    fn connect(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;

    /// 断开连接
    fn disconnect(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;
}

/// 网络服务器接口
pub trait NetworkServer: Send + Sync {
    /// 启动服务器
    fn start(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>>;

    /// 停止服务器
    fn stop(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>;

    /// 返回 self 的 Any 引用，用于 downcast 到具体类型
    fn as_any(&self) -> &dyn Any;
}

/// 网络工厂接口
pub trait NetworkFactory {
    /// 创建客户端连接
    ///
    /// `reply_rules` 为客户端侧回复规则共享状态：客户端是 1:1，构造时直接注入
    /// （决策 D-12），少一次 `*StateReady` 事件往返。
    fn create_client(
        config: &crate::config::connection::ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<crate::network::events::NetCounters>,
        trailer: crate::config::connection::TrailerSetting,
        reply_rules: std::sync::Arc<crate::reply::ReplyRulesStore>,
    ) -> Box<dyn NetworkConnection>
    where
        Self: Sized;

    /// 创建服务器
    ///
    /// 服务端多客户端共享同一个 store，但 server 先于 UI 就绪，
    /// 因此在这里注入一个**初始** store，规则集由 UI 通过
    /// `ConnectionEvent::ReplyRulesStoreReady` 运行时下发。
    fn create_server(
        config: &crate::config::connection::ServerConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<crate::network::events::NetCounters>,
        trailer: crate::config::connection::TrailerSetting,
    ) -> Box<dyn NetworkServer>
    where
        Self: Sized;
}
