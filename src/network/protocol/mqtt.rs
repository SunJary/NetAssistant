//! MQTT 客户端（MQTT 3.1.1，基于 rumqttc）。
//!
//! 采用**双任务模型**（规划 §6.2）：
//! - 任务 A：纯事件循环，`loop { eventloop.poll() }`，**绝不调用** `AsyncClient` 方法；
//! - 任务 B：唯一持有 `AsyncClient`，消费写入通道（发布）、订阅控制通道（订阅/取消）
//!   与任务 A 的重连信号，并调用 `publish/subscribe/unsubscribe`。
//!
//! 之所以必须拆成两个任务：rumqttc 的 `publish/subscribe/...` 是向一个 cap 受限的
//! 内部通道投递请求并 await 容量，而该通道只由 `eventloop.poll()` 排空；同一任务既
//! 投递又排空，容量满时会自我阻塞、poll 停摆、连接僵死。
//!
//! MQTT 无服务端角色（规划 D-5），`MqttServer::start` 直接返回错误。

use crate::config::connection::{ClientConfig, Qos, ServerConfig};
use crate::core::message_processor::{DefaultMessageProcessor, MessageProcessor};
use crate::message::MessageType;
use crate::network::events::{
    ConnectionEvent, MqttCommand, MqttPublishMeta, NetCounters, ReceivedBatch, WireMessage,
};
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::network::protocol::frame::{flush_batch, process_frame_shared};
use crate::reply::exec::RuleWireMode;
use crate::reply::ReplyRulesStore;
use bytes::BytesMut;
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, LastWill, MqttOptions, Outgoing, Packet, SubscribeReasonCode};
use rust_i18n::t;
use smol::channel::{Sender, unbounded as smol_unbounded};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// rumqttc 内部请求通道容量（发布速率由 broker 确认节奏天然背压；规划 §6.2）
const REQUEST_CHANNEL_CAP: usize = 10;

/// 单批聚合上限：达到即立即推送 `MessagesReceived`，避免洪泛订阅下批次无界增长（规划 §11）
const BATCH_MAX: usize = 64;

/// 取消后等待 DISCONNECT 报文写出的上限（规划 §6.2）。
///
/// 正常情况下任务 B 入队的 Disconnect 请求会在毫秒级被 poll 消费；该超时只用于
/// 兜底异常路径（broker 不回包、请求分支因 inflight 满/碰撞被禁用），避免退出挂死。
const DISCONNECT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// broker 端点解析失败时的哨兵地址（规划 §6.3①：规则引擎 `source` 用哨兵兜底）
fn broker_endpoint_sentinel() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 0))
}

/// 由配置构造 rumqttc `MqttOptions`（规划 §5.1 / §6.2）。
///
/// `client_id` 为空时强制 clean session：rumqttc 在「空 client_id + clean_session=false」
/// 时会 panic（`set_clean_session` 断言），此处提前规避。
fn build_options(config: &ClientConfig) -> MqttOptions {
    let settings = config.mqtt.clone().unwrap_or_default();
    let mut options = MqttOptions::new(
        settings.client_id.clone(),
        config.server_address.clone(),
        config.server_port,
    );
    // KeepAlive 由 rumqttc 内建（自动 PINGREQ），无需自建心跳
    options.set_keep_alive(Duration::from_secs(settings.keep_alive_secs as u64));
    options.set_clean_session(settings.clean_session || settings.client_id.is_empty());
    if let Some(username) = &settings.username {
        options.set_credentials(username.clone(), settings.password.clone().unwrap_or_default());
    }
    if let Some(will) = &settings.will {
        options.set_last_will(LastWill::new(
            will.topic.clone(),
            will.payload.clone().into_bytes(),
            will.qos.to_rumqttc(),
            will.retain,
        ));
    }
    options
}

/// 把 broker `host:port` 解析为 `SocketAddr`（供 `Connected` 事件与规则引擎 `source` 使用）。
///
/// `lookup_host` 同时支持 IP 字面量与主机名；失败时返回哨兵地址（规划 §6.3①）。
async fn resolve_broker_endpoint(host: &str, port: u16) -> SocketAddr {
    let target = if host.contains(':') && !host.contains('[') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
    };
    match tokio::net::lookup_host(&target).await {
        Ok(mut addrs) => addrs.next().unwrap_or_else(broker_endpoint_sentinel),
        Err(e) => {
            warn!(
                "MQTT broker 地址 '{}' 解析失败: {}（规则引擎 source 使用哨兵地址）",
                target, e
            );
            broker_endpoint_sentinel()
        }
    }
}

/// MQTT 帧处理：应答按 D-3 语义回发到**收到该消息的 topic**。
///
/// 复用共享骨架 `process_frame_shared`：规则引擎调用 / 计数 / `ReceivedBatch` 组装
/// 与其他协议逐字一致，仅应答投递闭包不同（携带 topic/QoS）。
#[allow(clippy::too_many_arguments)]
fn process_frame_mqtt(
    payload: &[u8],
    topic: &str,
    retain: bool,
    processor: &Arc<dyn MessageProcessor>,
    batch: &mut ReceivedBatch,
    counters: &Option<NetCounters>,
    rules: &Arc<ReplyRulesStore>,
    sender: &Sender<WireMessage>,
    connection_id: &str,
    source: SocketAddr,
) {
    let deliver = |bytes: Arc<[u8]>, _mode: RuleWireMode| -> bool {
        // D-3：应答回发到原 topic；QoS 首版固定 0；retain=false；
        // MQTT 载荷不追加 trailer（配置层已强制 TrailerKind::None，D-7），故忽略 mode。
        let meta = MqttPublishMeta {
            topic: topic.to_string(),
            qos: Qos::AtMostOnce,
            retain: false,
        };
        sender
            .try_send(WireMessage::inherit_mqtt(bytes.to_vec(), meta))
            .is_ok()
    };
    process_frame_shared(
        BytesMut::from(payload),
        processor,
        batch,
        counters,
        MessageType::Text,
        rules,
        connection_id,
        source,
        Some(topic.to_string()),
        retain,
        &deliver,
    );
}

/// MQTT 客户端（双任务模型）。
pub struct MqttClient {
    config: ClientConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    message_processor: Arc<dyn MessageProcessor>,
    net_counters: Option<NetCounters>,
    /// 回复规则共享状态（客户端 1:1 构造注入，决策 D-12）
    reply_rules: Arc<ReplyRulesStore>,
    cancel_token: CancellationToken,
}

impl MqttClient {
    /// 构造 MQTT 客户端（签名与 `TcpClient::new` 一致；trailer 对 MQTT 无意义，忽略）
    pub fn new(
        config: ClientConfig,
        event_sender: Option<Sender<ConnectionEvent>>,
        net_counters: Option<NetCounters>,
        _trailer: crate::config::connection::TrailerSetting,
        reply_rules: Arc<ReplyRulesStore>,
    ) -> Self {
        Self {
            config,
            event_sender,
            message_processor: Arc::new(DefaultMessageProcessor),
            net_counters,
            reply_rules,
            cancel_token: CancellationToken::new(),
        }
    }
}

impl NetworkConnection for MqttClient {
    fn connect(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let net_counters = self.net_counters.clone();
        let reply_rules = self.reply_rules.clone();
        let cancel = self.cancel_token.clone();

        Pin::from(Box::new(async move {
            let broker_endpoint =
                resolve_broker_endpoint(&config.server_address, config.server_port).await;
            let client_id = config
                .mqtt
                .as_ref()
                .map(|s| s.client_id.clone())
                .unwrap_or_default();
            info!(
                "MQTT 客户端连接 broker: {}:{} (client_id={})",
                config.server_address, config.server_port, client_id
            );

            let options = build_options(&config);
            let (client, mut eventloop) = AsyncClient::new(options, REQUEST_CHANNEL_CAP);

            // 写通道（发布，与既有发送点共用 WireMessage）/ 订阅控制通道 / 重连信号
            let (write_tx, write_rx) = smol_unbounded::<WireMessage>();
            let (control_tx, control_rx) = smol_unbounded::<MqttCommand>();
            let (restore_tx, restore_rx) = smol_unbounded::<()>();

            if let Some(sender) = &event_sender {
                if let Err(e) = sender
                    .send(ConnectionEvent::ClientWriteSenderReady(
                        config.id.clone(),
                        write_tx.clone(),
                    ))
                    .await
                {
                    error!("[MQTT客户端] 发送 ClientWriteSenderReady 事件失败: {:?}", e);
                }
                if let Err(e) = sender
                    .send(ConnectionEvent::MqttControlSenderReady(
                        config.id.clone(),
                        control_tx,
                    ))
                    .await
                {
                    error!("[MQTT客户端] 发送 MqttControlSenderReady 事件失败: {:?}", e);
                }
            } else {
                error!("[MQTT客户端] event_sender 为空，无法下发写入/订阅控制通道");
            }

            // ---- 任务 A：事件循环（只 poll，绝不调用 client 方法）----
            let read_cancel = cancel.clone();
            let read_event_sender = event_sender.clone();
            let read_config_id = config.id.clone();
            let read_processor = message_processor.clone();
            let read_counters = net_counters.clone();
            let read_rules = reply_rules.clone();
            // 读路径专用应答发送器：自动回复投递回写通道（D-3）
            let reply_sender = write_tx.clone();
            tokio::spawn(async move {
                let mut batch = ReceivedBatch::with_capacity(BATCH_MAX);
                let mut connected_once = false;
                // 取消后**不立即退出**，而是进入「排空」态继续 poll，直到观察到
                // `Outgoing::Disconnect`（该事件返回时报文已 write+flush 到网络）或超时。
                //
                // 若取消时直接 break，任务 B 入队的 Disconnect 请求会因 poll 不再被调用
                // 而永远写不出去 → 只剩 TCP 裸关闭，broker 会按非优雅断开发布遗嘱消息
                // （MQTT-3.1.2.8）。而任务 A 与任务 B 同时被同一 cancel 唤醒，
                // 谁先跑取决于调度顺序——不排空就意味着"遗嘱是否误发"不确定。
                let mut drain_deadline: Option<tokio::time::Instant> = None;
                loop {
                    if read_cancel.is_cancelled() && drain_deadline.is_none() {
                        info!("MQTT 收到取消信号，排空等待 DISCONNECT 报文写出");
                        drain_deadline =
                            Some(tokio::time::Instant::now() + DISCONNECT_DRAIN_TIMEOUT);
                    }
                    // 排空态用带超时的 poll，避免 broker 不回包时退出挂死
                    let polled = match drain_deadline {
                        Some(deadline) if tokio::time::Instant::now() >= deadline => {
                            warn!("MQTT 排空等待 DISCONNECT 超时，直接关闭连接");
                            break;
                        }
                        Some(deadline) => {
                            let remaining =
                                deadline.saturating_duration_since(tokio::time::Instant::now());
                            match tokio::time::timeout(remaining, eventloop.poll()).await {
                                Ok(result) => result,
                                Err(_) => {
                                    warn!("MQTT 排空等待 DISCONNECT 超时，直接关闭连接");
                                    break;
                                }
                            }
                        }
                        None => eventloop.poll().await,
                    };
                    match polled {
                        Ok(Event::Incoming(Packet::ConnAck(_))) => {
                            if connected_once {
                                // 重连：broker 不保留订阅（Clean Session=true），通知任务 B 重订阅
                                info!("MQTT 已重连");
                                if let Some(s) = &read_event_sender {
                                    let _ = s
                                        .send(ConnectionEvent::Warning(
                                            read_config_id.clone(),
                                            t!("mqtt_net.reconnected").to_string(),
                                        ))
                                        .await;
                                }
                                let _ = restore_tx.try_send(());
                            } else {
                                connected_once = true;
                                info!("MQTT 连接成功（CONNACK 已确认）: {}", broker_endpoint);
                                if let Some(s) = &read_event_sender {
                                    let _ = s
                                        .send(ConnectionEvent::Connected(
                                            read_config_id.clone(),
                                            broker_endpoint,
                                        ))
                                        .await;
                                }
                            }
                        }
                        Ok(Event::Incoming(Packet::Publish(p))) => {
                            process_frame_mqtt(
                                &p.payload,
                                &p.topic,
                                p.retain,
                                &read_processor,
                                &mut batch,
                                &read_counters,
                                &read_rules,
                                &reply_sender,
                                &read_config_id,
                                broker_endpoint,
                            );
                        }
                        Ok(Event::Incoming(Packet::SubAck(ack))) => {
                            // SUBACK 按协议仅含 pkid+返回码、不含 topic（D-10）：
                            // 失败无法定位条目，仅笼统提示
                            if ack.return_codes.contains(&SubscribeReasonCode::Failure) {
                                warn!("MQTT SUBACK 含失败返回码: pkid={}", ack.pkid);
                                if let Some(s) = &read_event_sender {
                                    let _ = s
                                        .send(ConnectionEvent::Warning(
                                            read_config_id.clone(),
                                            t!("mqtt_net.suback_rejected").to_string(),
                                        ))
                                        .await;
                                }
                            } else {
                                debug!("MQTT SUBACK: pkid={}, codes={:?}", ack.pkid, ack.return_codes);
                            }
                        }
                        Ok(Event::Incoming(Packet::Disconnect)) => {
                            error!("MQTT Broker 主动断开连接");
                            if let Some(s) = &read_event_sender {
                                let _ = s
                                    .send(ConnectionEvent::Error(
                                        read_config_id.clone(),
                                        t!("mqtt_net.broker_disconnected").to_string(),
                                    ))
                                    .await;
                            }
                            read_cancel.cancel();
                            break;
                        }
                        Ok(Event::Incoming(other)) => {
                            // PubAck/PubRec/PubComp/PubRel/PingResp/UnsubAck 等握手报文
                            debug!("MQTT 收到报文: {:?}", other);
                        }
                        Ok(Event::Outgoing(out)) => {
                            debug!("MQTT 发出报文: {:?}", out);
                            // 排空完成的唯一可靠信号：该事件返回前报文已 write+flush 到网络
                            if matches!(out, Outgoing::Disconnect) {
                                info!("MQTT DISCONNECT 报文已写出，事件循环退出");
                                break;
                            }
                        }
                        Err(e) => {
                            if read_cancel.is_cancelled() {
                                break;
                            }
                            if !connected_once {
                                // 首次连接失败 → 终态
                                error!("MQTT 首次连接失败: {}", e);
                                if let Some(s) = &read_event_sender {
                                    let _ = s
                                        .send(ConnectionEvent::Error(
                                            read_config_id.clone(),
                                            t!("mqtt_net.connect_failed", reason => e.to_string()).to_string(),
                                        ))
                                        .await;
                                }
                                read_cancel.cancel();
                                break;
                            }
                            // 运行中被动断开：rumqttc 内建自动重连，仅提示（非终态）
                            warn!("MQTT 连接断开，自动重连中: {}", e);
                            if let Some(s) = &read_event_sender {
                                let _ = s
                                    .send(ConnectionEvent::Warning(
                                        read_config_id.clone(),
                                        t!("mqtt_net.reconnecting").to_string(),
                                    ))
                                    .await;
                            }
                        }
                    }
                    // 洪泛聚合：缓冲队列已排空或达到上限即推送本批（事件数与消息条数解耦）
                    if batch.count >= BATCH_MAX as u64 || eventloop.state.events.is_empty() {
                        flush_batch(&read_event_sender, &read_config_id, &mut batch);
                    }
                }
                flush_batch(&read_event_sender, &read_config_id, &mut batch);
                if let Some(s) = &read_event_sender {
                    let _ = s
                        .send(ConnectionEvent::Disconnected(read_config_id.clone()))
                        .await;
                }
                info!("MQTT 事件循环任务退出");
            });

            // ---- 任务 B：客户端操作（唯一持有 AsyncClient；与 poll 任务分离避免自死锁）----
            let write_cancel = cancel.clone();
            let write_event_sender = event_sender.clone();
            let write_config_id = config.id.clone();
            let write_counters = net_counters.clone();
            tokio::spawn(async move {
                // 乐观维护的订阅列表（D-10：SUBACK 无法关联 topic，点击即入列）
                let mut active: Vec<(String, Qos)> = Vec::new();
                // 接收端用 Option 包装：通道关闭后置 None 让分支退化为 pending，避免 select! 忙转
                let mut write_rx = Some(write_rx);
                let mut control_rx = Some(control_rx);
                let mut restore_rx = Some(restore_rx);
                loop {
                    tokio::select! {
                        biased;

                        _ = write_cancel.cancelled() => {
                            info!("MQTT 客户端操作任务收到取消信号，发送 DISCONNECT 后退出");
                            let _ = client.disconnect().await;
                            break;
                        }

                        // 任务 A 的重连信号：重新订阅全部 active 列表（幂等，行为不分支）
                        restored = async {
                            match restore_rx.as_ref() {
                                Some(rx) => rx.recv().await,
                                None => std::future::pending::<
                                    Result<(), smol::channel::RecvError>,
                                >()
                                .await,
                            }
                        } => {
                            match restored {
                                Ok(()) => {
                                    let count = active.len();
                                    for (topic, qos) in &active {
                                        if let Err(e) = client
                                            .subscribe(topic.clone(), qos.to_rumqttc())
                                            .await
                                        {
                                            warn!("MQTT 重连后重新发起订阅 '{}' 失败: {:?}", topic, e);
                                        }
                                    }
                                    debug!("MQTT 重连后已重新发起订阅 {} 个 topic", count);
                                    if count > 0
                                        && let Some(s) = &write_event_sender
                                    {
                                        let _ = s
                                            .send(ConnectionEvent::MqttSubscriptionsRestored(
                                                write_config_id.clone(),
                                                count,
                                            ))
                                            .await;
                                    }
                                }
                                // 通道关闭 → 置 None 让分支退化为 pending，与 control/write 分支同构。
                                // 不能依赖"任务 A 退出前必然先 cancel"这条隐式不变量：一旦将来新增
                                // 不 cancel 的退出路径，本分支会恒就绪 → select! 忙转（反复重订阅）。
                                Err(_) => {
                                    debug!("MQTT 重连恢复通道已关闭, 不再接收重订阅信号");
                                    restore_rx = None;
                                }
                            }
                        }

                        // 订阅 / 取消订阅（运行时下发，无需重连；乐观维护 active 列表）
                        // 控制分支置于发布之前（配合 select! biased）: 持续高频发布时不饿死订阅命令
                        cmd = async {
                            match control_rx.as_ref() {
                                Some(rx) => rx.recv().await,
                                None => std::future::pending::<
                                    Result<MqttCommand, smol::channel::RecvError>,
                                >()
                                .await,
                            }
                        } => {
                            match cmd {
                                Ok(MqttCommand::Subscribe { topic, qos }) => {
                                    match client.subscribe(topic.clone(), qos.to_rumqttc()).await {
                                        Ok(()) => {
                                            match active.iter_mut().find(|(t, _)| *t == topic) {
                                                Some(entry) => entry.1 = qos,
                                                None => active.push((topic, qos)),
                                            }
                                        }
                                        Err(e) => {
                                            warn!("MQTT 订阅 '{}' 失败: {:?}", topic, e);
                                            if let Some(s) = &write_event_sender {
                                                let _ = s
                                                    .send(ConnectionEvent::Warning(
                                                        write_config_id.clone(),
                                                        t!("mqtt_net.subscribe_failed", topic => topic.clone()).to_string(),
                                                    ))
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                Ok(MqttCommand::Unsubscribe { topic }) => {
                                    match client.unsubscribe(topic.clone()).await {
                                        Ok(()) => active.retain(|(t, _)| *t != topic),
                                        Err(e) => {
                                            warn!("MQTT 取消订阅 '{}' 失败: {:?}", topic, e);
                                            if let Some(s) = &write_event_sender {
                                                let _ = s
                                                    .send(ConnectionEvent::Warning(
                                                        write_config_id.clone(),
                                                        t!("mqtt_net.unsubscribe_failed", topic => topic.clone()).to_string(),
                                                    ))
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    debug!("MQTT 订阅控制通道已关闭, 不再接收订阅下发");
                                    control_rx = None;
                                }
                            }
                        }

                        // 发布：payload 原样，topic/QoS 取自 mqtt_meta（D-2）
                        msg = async {
                            match write_rx.as_ref() {
                                Some(rx) => rx.recv().await,
                                None => std::future::pending::<
                                    Result<WireMessage, smol::channel::RecvError>,
                                >()
                                .await,
                            }
                        } => {
                            match msg {
                                Ok(wire) => {
                                    let len = wire.len() as u64;
                                    match wire.mqtt_meta().cloned() {
                                        Some(meta) => {
                                            let payload = wire.into_data();
                                            match client
                                                .publish(
                                                    meta.topic.clone(),
                                                    meta.qos.to_rumqttc(),
                                                    meta.retain,
                                                    payload,
                                                )
                                                .await
                                            {
                                                Ok(()) => {
                                                    // 计数口径：`publish()` 成功仅代表请求已进入
                                                    // rumqttc 内部通道（QoS1/2 的 broker 确认是后续
                                                    // 事件），因此断线期间仍会累加。与 TCP/UDP 在
                                                    // write 成功后计数略有差异，此处按"已产出"口径。
                                                    if let Some(c) = &write_counters {
                                                        c.add_sent(1);
                                                        c.add_sent_bytes(len);
                                                    }
                                                }
                                                Err(e) => warn!(
                                                    "MQTT 发布到 '{}' 失败: {:?}",
                                                    meta.topic, e
                                                ),
                                            }
                                        }
                                        None => warn!(
                                            "MQTT 写通道收到缺少 topic/QoS 元数据的数据，已忽略"
                                        ),
                                    }
                                }
                                Err(_) => {
                                    debug!("MQTT 写入通道已关闭");
                                    write_rx = None;
                                }
                            }
                        }
                    }
                }
            });

            Ok(())
        }))
    }

    fn disconnect(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        // cancel 驱动退出闭环：任务 B 发 DISCONNECT 后退出 → 任务 A 的 poll 返回错误后退出
        self.cancel_token.cancel();
        Pin::from(Box::new(async { Ok(()) }))
    }
}

/// MQTT 无服务端角色（规划 D-5）。
///
/// 仅保证工厂 `match` 穷尽；手改配置为 server+mqtt 时启动即返回明确错误，不会 panic。
pub struct MqttServer {
    #[allow(dead_code)]
    config: ServerConfig,
}

impl MqttServer {
    pub fn new(
        config: ServerConfig,
        _event_sender: Option<Sender<ConnectionEvent>>,
        _net_counters: Option<NetCounters>,
        _trailer: crate::config::connection::TrailerSetting,
    ) -> Self {
        Self { config }
    }
}

impl NetworkServer for MqttServer {
    fn start(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>> {
        Box::pin(async { Err("MQTT 不支持服务端角色（规划 D-5：仅客户端）".into()) })
    }

    fn stop(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection::{ConnectionType, MqttSettings, WillSettings};
    use crate::core::message_processor::DefaultMessageProcessor;
    use crate::reply::model::{MatchNode, ReplyPayload, ReplyRule, ReplyRulesConfig, RuleCodec};

    fn source() -> SocketAddr {
        "127.0.0.1:5000".parse().unwrap()
    }

    fn processor() -> Arc<dyn MessageProcessor> {
        Arc::new(DefaultMessageProcessor)
    }

    fn rule_on_len(text: &str) -> ReplyRule {
        ReplyRule {
            matcher: MatchNode::Length { min: 1, max: 64 },
            payload: ReplyPayload {
                text: text.to_string(),
                hex_mode: false,
                codec: RuleCodec::Raw,
            },
            ..ReplyRule::new("测试规则", 10)
        }
    }

    /// 构造挂到 "tab" 连接下的规则集（并开启该连接总闸）
    fn store_with_rule(rule: ReplyRule) -> Arc<ReplyRulesStore> {
        let store = ReplyRulesStore::new();
        let mut cfg = ReplyRulesConfig::default();
        cfg.connections.insert("tab".to_string(), vec![rule]);
        store.replace(&cfg);
        store.set_connection_gates([("tab".to_string(), true)].into_iter().collect());
        store
    }

    fn mqtt_config(settings: MqttSettings) -> ClientConfig {
        ClientConfig {
            protocol: ConnectionType::Mqtt,
            server_address: "broker.example".to_string(),
            server_port: 1883,
            mqtt: Some(settings),
            ..Default::default()
        }
    }

    /// 配置映射：KeepAlive / 用户名密码 / 遗嘱 / 非 clean session 逐项落到 MqttOptions。
    #[test]
    fn test_build_options_reflects_mqtt_settings() {
        let config = mqtt_config(MqttSettings {
            client_id: "cid".to_string(),
            username: Some("u".to_string()),
            password: Some("p".to_string()),
            keep_alive_secs: 30,
            clean_session: false,
            will: Some(WillSettings {
                topic: "will/t".to_string(),
                payload: "bye".to_string(),
                qos: Qos::AtLeastOnce,
                retain: true,
            }),
        });

        let options = build_options(&config);
        assert_eq!(options.broker_address(), ("broker.example".to_string(), 1883));
        assert_eq!(options.client_id(), "cid");
        assert_eq!(options.keep_alive(), Duration::from_secs(30));
        assert!(!options.clean_session());

        let creds = options.credentials().expect("应携带用户名密码");
        assert_eq!(creds.username, "u");
        assert_eq!(creds.password, "p");

        let will = options.last_will().expect("应携带遗嘱消息");
        assert_eq!(will.topic, "will/t");
        assert_eq!(&will.message[..], b"bye");
        assert_eq!(will.qos, rumqttc::QoS::AtLeastOnce);
        assert!(will.retain);
    }

    /// 空 client_id + clean_session=false 会让 rumqttc panic，必须强制 clean session。
    #[test]
    fn test_build_options_forces_clean_session_when_client_id_empty() {
        let config = mqtt_config(MqttSettings {
            client_id: String::new(),
            clean_session: false,
            ..MqttSettings::default()
        });
        let options = build_options(&config);
        assert!(options.clean_session(), "空 client_id 时必须强制 clean session");
    }

    /// IP 字面量 broker 直接解析为 SocketAddr（不依赖 DNS）。
    #[tokio::test]
    async fn test_resolve_broker_endpoint_ip_literal() {
        assert_eq!(
            resolve_broker_endpoint("127.0.0.1", 1883).await,
            "127.0.0.1:1883".parse().unwrap()
        );
    }

    /// D-3：规则应答必须回发到收到消息的 topic，且载荷不追加 trailer。
    #[test]
    fn test_process_frame_mqtt_reply_carries_topic_and_no_trailer() {
        let rules = store_with_rule(rule_on_len("RULE"));
        let (tx, rx) = smol_unbounded::<WireMessage>();
        let mut batch = ReceivedBatch::default();

        process_frame_mqtt(
            b"req",
            "sensor/1/temp",
            true,
            &processor(),
            &mut batch,
            &Some(NetCounters::default()),
            &rules,
            &tx,
            "tab",
            source(),
        );

        assert_eq!(batch.count, 1, "帧照常进明细");
        assert_eq!(batch.sent_messages.len(), 1, "应答进入本批 Sent 明细");
        // 步骤 5：接收明细必须携带 PUBLISH 的 topic/retain
        assert_eq!(
            batch.messages[0].topic.as_deref(),
            Some("sensor/1/temp"),
            "接收消息携带 topic"
        );
        assert!(batch.messages[0].retain, "接收消息携带 retain 标记");

        let wire = rx.try_recv().expect("应答必须投递到写通道");
        assert_eq!(wire.data(), b"RULE", "应答载荷原样");
        let meta = wire.mqtt_meta().expect("应答必须携带 topic 元数据");
        assert_eq!(meta.topic, "sensor/1/temp", "应答回发到原 topic");
        assert_eq!(meta.qos, Qos::AtMostOnce, "应答 QoS 首版固定 0");
        assert!(!meta.retain);
        assert!(wire.bypasses_trailer(), "MQTT 载荷不得追加连接 trailer");
    }

    /// 未启用规则时不产生应答（快速路径），帧仍进明细。
    #[test]
    fn test_process_frame_mqtt_no_reply_when_rules_disabled() {
        let mut disabled = rule_on_len("RULE");
        disabled.enabled = false;
        let rules = store_with_rule(disabled);
        assert!(!rules.is_enabled());

        let (tx, rx) = smol_unbounded::<WireMessage>();
        let mut batch = ReceivedBatch::default();
        process_frame_mqtt(
            b"req",
            "a/b",
            false,
            &processor(),
            &mut batch,
            &None,
            &rules,
            &tx,
            "tab",
            source(),
        );
        assert_eq!(batch.count, 1);
        assert!(batch.sent_messages.is_empty());
        assert_eq!(batch.messages[0].topic.as_deref(), Some("a/b"));
        assert!(!batch.messages[0].retain);
        assert!(rx.try_recv().is_err(), "无启用规则时不得投递应答");
    }

    // ===================== 假 broker 端到端（双任务模型验证） =====================

    /// 读取一个 MQTT 报文：返回（首字节, 报文体）。仅覆盖测试所需的短报文。
    ///
    /// 返回 `Err` 表示对端在读完一个完整报文前关闭了连接——对本测试而言，
    /// 这正是"未发出 DISCONNECT 就裸关连接"的证据。
    async fn read_mqtt_packet(
        sock: &mut tokio::net::TcpStream,
    ) -> std::io::Result<(u8, Vec<u8>)> {
        use tokio::io::AsyncReadExt;
        let mut b1 = [0u8; 1];
        sock.read_exact(&mut b1).await?;
        let mut len = 0usize;
        let mut mult = 1usize;
        loop {
            let mut b = [0u8; 1];
            sock.read_exact(&mut b).await?;
            len += ((b[0] & 0x7F) as usize) * mult;
            if b[0] & 0x80 == 0 {
                break;
            }
            mult *= 128;
        }
        let mut body = vec![0u8; len];
        sock.read_exact(&mut body).await?;
        Ok((b1[0], body))
    }

    /// 编码一个 QoS0 PUBLISH 报文（话题与载荷均为短字符串）。
    fn encode_mqtt_publish(topic: &str, payload: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&(topic.len() as u16).to_be_bytes());
        body.extend_from_slice(topic.as_bytes());
        body.extend_from_slice(payload);
        let mut pkt = vec![0x30u8];
        // 短报文：剩余长度单字节
        assert!(body.len() < 128, "测试报文需为单字节剩余长度");
        pkt.push(body.len() as u8);
        pkt.extend_from_slice(&body);
        pkt
    }

    /// broker 侧观测事件，用于把假 broker 处理结果回报给测试主体。
    #[derive(Debug)]
    enum BrokerObs {
        Subscribed(String),
        Published { topic: String, payload: Vec<u8> },
    }

    /// 端到端：连接 → 订阅 → 收消息 → 发布，全程验证双任务模型不自我死锁。
    ///
    /// 若 poll 任务与客户端操作任务合并，`subscribe()`（cap=10 的内部通道）会在容量
    /// 满时自我阻塞 → 本测试会在等待 SUBACK/SUBSCRIBE 观测时超时。
    #[tokio::test]
    async fn test_mqtt_connect_subscribe_publish_receive_roundtrip() {
        use crate::network::events::ConnectionEvent as Ev;
        use std::time::Duration;
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;
        use tokio::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broker_addr = listener.local_addr().unwrap();

        let (obs_tx, mut obs_rx) = mpsc::unbounded_channel::<BrokerObs>();

        // 假 broker：按报文类型应答；SUBSCRIBE 后立即推送一条 PUBLISH 给客户端。
        //
        // 返回值为结束原因：只有收到 DISCONNECT（0xE0）才是 "ok"；若对端在报文
        // 中途关闭连接（read 失败），说明客户端未发 DISCONNECT 就裸关 —— 这是
        // MQTT 语义下的非优雅断开，broker 会发布遗嘱消息。
        let broker: tokio::task::JoinHandle<&'static str> = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            loop {
                let (header, body) = match read_mqtt_packet(&mut sock).await {
                    Ok(packet) => packet,
                    Err(_) => return "连接在收到 DISCONNECT 前被关闭（非优雅断开）",
                };
                match header {
                    0x10 => {
                        // CONNECT → CONNACK(success, session_present=false)
                        sock.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
                    }
                    0x82 => {
                        // SUBSCRIBE → 解析 pkid 与首个过滤器 → SUBACK(success)
                        let pkid = [body[0], body[1]];
                        let topic_len = u16::from_be_bytes([body[2], body[3]]) as usize;
                        let topic = String::from_utf8(body[4..4 + topic_len].to_vec()).unwrap();
                        sock.write_all(&[0x90, 0x03, pkid[0], pkid[1], 0x00]).await.unwrap();
                        obs_tx.send(BrokerObs::Subscribed(topic)).ok();
                        // 推送一条消息，验证读路径 → MessagesReceived
                        sock.write_all(&encode_mqtt_publish("sensor/1", b"hello"))
                            .await
                            .unwrap();
                    }
                    0x30 => {
                        let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
                        let topic = String::from_utf8(body[2..2 + topic_len].to_vec()).unwrap();
                        obs_tx
                            .send(BrokerObs::Published {
                                topic,
                                payload: body[2 + topic_len..].to_vec(),
                            })
                            .ok();
                    }
                    0xE0 => return "ok", // DISCONNECT：优雅断开
                    _ => {}
                }
            }
        });

        let config = ClientConfig {
            protocol: ConnectionType::Mqtt,
            server_address: "127.0.0.1".to_string(),
            server_port: broker_addr.port(),
            mqtt: Some(MqttSettings {
                client_id: "netassist-test".to_string(),
                ..MqttSettings::default()
            }),
            ..Default::default()
        };

        let (tx_events, rx_events) = smol_unbounded::<ConnectionEvent>();
        let mut manager = crate::network::connection::manager::NetworkConnectionManager::new();
        manager
            .create_and_connect_client_with_counters(
                &config,
                Some(tx_events),
                Some(NetCounters::default()),
                crate::config::connection::TrailerSetting::default(),
            )
            .await
            .expect("MQTT 客户端应启动成功");

        // 收齐: 写发送器 + 订阅控制发送器 + Connected(等待 CONNACK)
        let mut write_sender = None;
        let mut control_sender = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while write_sender.is_none() || control_sender.is_none() {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let ev = tokio::time::timeout(remaining, rx_events.recv())
                .await
                .expect("等待 MQTT 就绪事件超时")
                .expect("事件通道不应关闭");
            match ev {
                Ev::ClientWriteSenderReady(id, s) => {
                    assert_eq!(id, config.id);
                    write_sender = Some(s);
                }
                Ev::MqttControlSenderReady(id, s) => {
                    assert_eq!(id, config.id);
                    control_sender = Some(s);
                }
                Ev::Error(id, msg) => panic!("MQTT 连接不应失败: {id} {msg}"),
                _ => {}
            }
        }

        // 订阅: 假 broker 应收到 SUBSCRIBE 并回来（证明双任务不自我死锁）
        control_sender
            .as_ref()
            .unwrap()
            .try_send(MqttCommand::Subscribe {
                topic: "sensor/1".to_string(),
                qos: Qos::AtLeastOnce,
            })
            .expect("投递订阅命令应成功");

        let obs = tokio::time::timeout(Duration::from_secs(10), obs_rx.recv())
            .await
            .expect("等待 broker 收到 SUBSCRIBE 超时（疑似 poll/操作任务自我死锁）")
            .expect("观测通道不应关闭");
        match obs {
            BrokerObs::Subscribed(topic) => assert_eq!(topic, "sensor/1"),
            other => panic!("首个观测应为 Subscribed, 实际: {other:?}"),
        }

        // 等 Connected 事件（CONNACK 已到）
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let ev = tokio::time::timeout(remaining, rx_events.recv())
                .await
                .expect("等待 Connected 超时")
                .expect("事件通道不应关闭");
            if let Ev::Connected(id, _) = ev {
                assert_eq!(id, config.id);
                break;
            }
        }

        // 收消息: broker 推送的 PUBLISH 应经读路径产生 MessagesReceived
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let ev = tokio::time::timeout(remaining, rx_events.recv())
                .await
                .expect("等待 MessagesReceived 超时")
                .expect("事件通道不应关闭");
            if let Ev::MessagesReceived(id, batch) = ev {
                if id != config.id {
                    continue;
                }
                assert_eq!(batch.count, 1, "应恰好聚合 1 条");
                assert_eq!(&*batch.messages[0].raw_data, b"hello");
                // 步骤 5：端到端验证接收消息携带 broker 推送的 topic
                assert_eq!(batch.messages[0].topic.as_deref(), Some("sensor/1"));
                break;
            }
        }

        // 发布: 经写通道投递 inherit_mqtt, 假 broker 应收到对应 PUBLISH
        write_sender
            .as_ref()
            .unwrap()
            .try_send(WireMessage::inherit_mqtt(
                b"payload".to_vec(),
                MqttPublishMeta {
                    topic: "out/1".to_string(),
                    qos: Qos::AtMostOnce,
                    retain: false,
                },
            ))
            .expect("投递发布消息应成功");

        let obs = tokio::time::timeout(Duration::from_secs(10), obs_rx.recv())
            .await
            .expect("等待 broker 收到 PUBLISH 超时")
            .expect("观测通道不应关闭");
        match obs {
            BrokerObs::Published { topic, payload } => {
                assert_eq!(topic, "out/1");
                assert_eq!(payload, b"payload");
            }
            other => panic!("观测应为 Published, 实际: {other:?}"),
        }

        // 收尾: 显式断开, 任务 B 应发 DISCONNECT 且必须真正写到网络(优雅断开)。
        // 这里必须断言 broker 的结束原因: 若客户端未发 DISCONNECT 就关闭 socket,
        // broker 读到 Err 并返回失败原因, 测试即失败(此前用 `let _ =` 丢弃超时结果,
        // 该断言形同虚设)。
        manager.disconnect_client(&config.id).await.unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(5), broker)
            .await
            .expect("等待 broker 观测到 DISCONNECT 超时: 断开未发出 DISCONNECT 报文")
            .expect("broker 任务不应 panic");
        assert_eq!(
            outcome, "ok",
            "客户端断开必须发出 DISCONNECT 报文, 否则 broker 按非优雅断开会发布遗嘱消息"
        );
    }
}
