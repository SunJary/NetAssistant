//! MQTT 5.0 客户端（基于 rumqttc::v5）。
//!
//! 与 `mqtt.rs`（3.1.1）并存而非合并：v5 的报文类型（`Bytes` topic、
//! `Option<Properties>`、`Vec<SubscribeReasonCode>`）与 v4 不共享任何父类型，
//! 且 broker 断开 / 首连被拒走 `Err(StateError)` 而非 `Ok(Packet::Disconnect/ConnAck)`。
//! 塞进同一函数会让每个事件分支都带版本判断，且 v5 出问题时无法单独回滚。
//!
//! 复用（唯一真源，不复制）：`process_frame_mqtt` / `resolve_broker_endpoint` /
//! `build_transport`（TLS）/ `should_emit_warning` / 容量与超时常量。
//! 重复的只有「双任务骨架」——任务 A 只 poll 事件循环，任务 B 独占 `AsyncClient`。

use crate::config::connection::{ClientConfig, Qos, ServerConfig};
use crate::core::message_processor::{DefaultMessageProcessor, MessageProcessor};
use crate::network::events::{
    ConnectionEvent, MqttCommand, MqttPublishFailure, MqttPublishMeta, MqttSubAckItem, NetCounters,
    ReceivedBatch, WireMessage,
};
use crate::network::interfaces::{NetworkConnection, NetworkServer};
use crate::network::protocol::frame::flush_batch;
use crate::network::protocol::mqtt::{
    BATCH_MAX, DISCONNECT_DRAIN_TIMEOUT, DISCONNECT_WAIT_TASK_TIMEOUT, REQUEST_CHANNEL_CAP,
    WRITE_CHANNEL_CAP, emit_expired_subscriptions, process_frame_mqtt, resolve_broker_endpoint,
    should_emit_warning, wait_for_subscribe_slot,
};
use crate::network::protocol::mqtt_tls::build_transport;
use crate::network::protocol::publish_tracker::PublishTracker;
use crate::network::protocol::subscription_tracker::{CHUNK_SIZE, SubscriptionTracker};
use crate::reply::ReplyRulesStore;
use log::{debug, error, info, warn};
use rumqttc::Outgoing;
use rumqttc::v5::mqttbytes::QoS as V5Qos;
use rumqttc::v5::mqttbytes::v5::{
    ConnectReturnCode, DisconnectReasonCode, Filter, LastWill, Packet, PubAckReason, PubCompReason,
    PubRecReason, SubscribeReasonCode,
};
use rumqttc::v5::{AsyncClient, Event, MqttOptions, StateError};
use rust_i18n::t;
use smol::channel::{Sender, bounded as smol_bounded, unbounded as smol_unbounded};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// 项目 Qos → rumqttc v5 QoS。
///
/// v5 模块有自己的一份 `QoS` 枚举（与 v4 的同名枚举是两个类型），
/// 不能直接复用 `Qos::to_rumqttc`。
fn to_v5_qos(qos: Qos) -> V5Qos {
    match qos {
        Qos::AtMostOnce => V5Qos::AtMostOnce,
        Qos::AtLeastOnce => V5Qos::AtLeastOnce,
        Qos::ExactlyOnce => V5Qos::ExactlyOnce,
    }
}

/// 由配置构造 rumqttc v5 `MqttOptions`。
///
/// `client_id` 为空时强制 clean start：v5 虽然允许空 client_id，但规范要求
/// `clean_start=false` 时 broker 必须拒绝或强制清理会话，这里提前对齐 v4 的行为，
/// 避免「空 id + 持久会话」被 broker 拒绝后用户不知道原因。
fn build_options_v5(config: &ClientConfig) -> Result<MqttOptions, String> {
    let settings = config.mqtt.clone().unwrap_or_default();
    let mut options = MqttOptions::new(
        settings.client_id.clone(),
        config.server_address.clone(),
        config.server_port,
    );
    // KeepAlive 由 rumqttc 内建（自动 PINGREQ），无需自建心跳。
    // 持久化配置是系统边界：v5 断言 >= 5s（v4 是 >= 1s），手工编辑过的
    // connections.json 可能残留 1-4，直接传入会在 assert 处 panic，故在此净化。
    options.set_keep_alive(Duration::from_secs(u64::from(
        settings.keep_alive_secs.max(5),
    )));
    options.set_clean_start(settings.clean_session || settings.client_id.is_empty());
    if let Some(username) = &settings.username {
        options.set_credentials(
            username.clone(),
            settings.password.clone().unwrap_or_default(),
        );
    }
    if let Some(will) = &settings.will {
        // v5 遗嘱多一个 properties 参数；首版不下发遗嘱属性
        options.set_last_will(LastWill::new(
            will.topic.clone(),
            will.payload.clone().into_bytes(),
            to_v5_qos(will.qos),
            will.retain,
            None,
        ));
    }
    options.set_request_channel_capacity(REQUEST_CHANNEL_CAP);
    // 仅有值时下发连接属性：None 表示让 broker 使用默认值（不下发属性字节）。
    if let Some(secs) = settings.session_expiry_secs {
        options.set_session_expiry_interval(Some(secs));
    }
    // Receive Maximum 合法范围 1-65535，0 是协议禁止值（broker 会以 0x82 断开），
    // 同样对持久化配置做一次净化。
    if let Some(max) = settings.receive_maximum.filter(|v| *v > 0) {
        options.set_receive_maximum(Some(max));
    }
    // TLS：仅启用时覆盖 transport（默认 Transport::Tcp，行为不变）
    if let Some(tls) = settings.tls.as_ref().filter(|t| t.enabled) {
        options.set_transport(build_transport(tls)?);
    }
    Ok(options)
}

/// 连接失败映射为可读文案：TLS 沿用与 v4 相同的处置建议，协议层拒绝附原因码。
fn describe_connect_error(err: &rumqttc::v5::ConnectionError) -> String {
    match err {
        rumqttc::v5::ConnectionError::Tls(rumqttc::TlsError::TLS(
            rustls::Error::InvalidCertificate(cert_err),
        )) => match cert_err {
            // rumqttc 用 broker 地址作为证书校验主机名，IP 连接 + 域名证书必失败
            rustls::CertificateError::NotValidForName
            | rustls::CertificateError::NotValidForNameContext { .. } => {
                t!("mqtt_net.tls_error_hostname").to_string()
            }
            rustls::CertificateError::UnknownIssuer => {
                t!("mqtt_net.tls_error_unknown_issuer").to_string()
            }
            _ => t!("mqtt_net.tls_error_generic", reason => err.to_string()).to_string(),
        },
        // 首连被拒（v5 的 CONNACK 失败不会产生 Ok 事件，见 B5 差异表）
        rumqttc::v5::ConnectionError::ConnectionRefused(code) => describe_conn_fail(*code),
        rumqttc::v5::ConnectionError::MqttState(StateError::ConnFail { reason }) => {
            describe_conn_fail(*reason)
        }
        _ => err.to_string(),
    }
}

/// v5 CONNACK 拒绝码 → 用户可读文案（附原始原因码便于对照 broker 日志）。
fn describe_conn_fail(code: ConnectReturnCode) -> String {
    t!("mqtt_net.connack_refused", code => format!("{code:?}")).to_string()
}

/// broker 主动 DISCONNECT → 用户可读文案（附 reason code 与可选的 reason string）。
fn describe_server_disconnect(
    reason_code: DisconnectReasonCode,
    reason_string: Option<&str>,
) -> String {
    let code = format!("{reason_code:?}");
    match reason_string.filter(|s| !s.is_empty()) {
        Some(detail) => {
            t!("mqtt_net.server_disconnect_reason", code => code, reason => detail).to_string()
        }
        None => t!("mqtt_net.server_disconnect", code => code).to_string(),
    }
}

/// SUBACK 单个失败码 → 用户可读文案。
fn subscribe_reason_text(code: SubscribeReasonCode) -> String {
    match code {
        SubscribeReasonCode::Success(_) => t!("mqtt_net.suback_reason_success").to_string(),
        SubscribeReasonCode::Failure => t!("mqtt_net.suback_reason_failure").to_string(),
        SubscribeReasonCode::Unspecified => t!("mqtt_net.suback_reason_unspecified").to_string(),
        SubscribeReasonCode::ImplementationSpecific => {
            t!("mqtt_net.suback_reason_implementation_specific").to_string()
        }
        SubscribeReasonCode::NotAuthorized => {
            t!("mqtt_net.suback_reason_not_authorized").to_string()
        }
        SubscribeReasonCode::TopicFilterInvalid => {
            t!("mqtt_net.suback_reason_topic_filter_invalid").to_string()
        }
        SubscribeReasonCode::PkidInUse => t!("mqtt_net.suback_reason_pkid_in_use").to_string(),
        SubscribeReasonCode::QuotaExceeded => {
            t!("mqtt_net.suback_reason_quota_exceeded").to_string()
        }
        SubscribeReasonCode::SharedSubscriptionsNotSupported => {
            t!("mqtt_net.suback_reason_shared_not_supported").to_string()
        }
        SubscribeReasonCode::SubscriptionIdNotSupported => {
            t!("mqtt_net.suback_reason_sub_id_not_supported").to_string()
        }
        SubscribeReasonCode::WildcardSubscriptionsNotSupported => {
            t!("mqtt_net.suback_reason_wildcard_not_supported").to_string()
        }
    }
}

/// 单个 reason code → 可读原因（附 broker reason string）。
fn suback_reason(code: SubscribeReasonCode, server: Option<&str>) -> String {
    let reason = subscribe_reason_text(code);
    match server.filter(|s| !s.is_empty()) {
        Some(detail) => t!(
            "mqtt_net.suback_reason_with_server",
            reason => reason,
            detail => detail
        )
        .to_string(),
        None => reason,
    }
}

/// 按 `CHUNK_SIZE` 切块下发订阅，逐块登记 tracker；同步下发失败则回滚并回报失败。
async fn dispatch_subscribe_v5(
    client: &AsyncClient,
    tracker: &Arc<StdMutex<SubscriptionTracker>>,
    event_sender: &Option<Sender<ConnectionEvent>>,
    config_id: &str,
    topics: Vec<(String, Qos)>,
) {
    for chunk in topics.chunks(CHUNK_SIZE) {
        wait_for_subscribe_slot(tracker).await;
        let names: Vec<String> = chunk.iter().map(|(topic, _)| topic.clone()).collect();
        if let Ok(mut guard) = tracker.lock() {
            guard.queue(names);
        }
        let filters: Vec<Filter> = chunk
            .iter()
            .map(|(topic, qos)| Filter::new(topic.clone(), to_v5_qos(*qos)))
            .collect();
        if let Err(e) = client.subscribe_many(filters).await {
            warn!("MQTT5 批量订阅失败（{} 个 topic）: {:?}", chunk.len(), e);
            if let Ok(mut guard) = tracker.lock() {
                guard.rollback_last();
            }
            if let Some(sender) = event_sender {
                let items: Vec<MqttSubAckItem> = chunk
                    .iter()
                    .map(|(topic, _)| MqttSubAckItem {
                        topic: topic.clone(),
                        ok: false,
                        reason: Some(
                            t!("mqtt_net.subscribe_failed", topic => topic.clone()).to_string(),
                        ),
                    })
                    .collect();
                let _ = sender
                    .send(ConnectionEvent::MqttSubAck(config_id.to_string(), items))
                    .await;
            }
        }
    }
}

/// 发布确认失败码 → 用户可读文案。
///
/// v4 的 PUBACK 只有报文标识符，v5 起 broker 可以拒绝单条发布
/// （未授权 / 配额超限 / 载荷格式非法）而不影响连接，这是 v5 独有的排障信息。
///
/// `topic` 为 `None` 表示无法还原主题（旧会话残留 / 未登记的确认），
/// 此时退化为不带主题的通用文案；`NotAuthorized` 使用带主题的专用文案，
/// 与订阅失败的「主题「xx」没有权限」保持一致。
fn describe_publish_rejected(
    topic: Option<&str>,
    code: &impl std::fmt::Debug,
    reason_string: Option<&str>,
) -> String {
    let code_text = format!("{code:?}");
    let detail = reason_string.filter(|s| !s.is_empty());
    match topic {
        Some(topic) if code_text == "NotAuthorized" => {
            t!("mqtt_net.publish_not_authorized_topic", topic => topic).to_string()
        }
        Some(topic) => match detail {
            Some(detail) => t!(
                "mqtt_net.publish_rejected_topic_reason",
                topic => topic,
                code => code_text,
                reason => detail
            )
            .to_string(),
            None => {
                t!("mqtt_net.publish_rejected_topic", topic => topic, code => code_text).to_string()
            }
        },
        None => match detail {
            Some(detail) => t!(
                "mqtt_net.publish_rejected_reason",
                code => code_text,
                reason => detail
            )
            .to_string(),
            None => t!("mqtt_net.publish_rejected", code => code_text).to_string(),
        },
    }
}

/// 把 broker 的拒收确认转成 `MqttPublishFailed` 事件。
///
/// 主题从 [`PublishTracker`] 按 pkid 还原；无法还原（旧会话残留 / 未登记）时
/// 用空主题退化为通用文案，事件仍会发出，避免漏掉用户可见的失败。
async fn emit_publish_rejected(
    event_sender: &Option<Sender<ConnectionEvent>>,
    config_id: &str,
    tracker: &Arc<StdMutex<PublishTracker>>,
    pkid: u16,
    code: &impl std::fmt::Debug,
    reason_string: Option<&str>,
) {
    let resolved = tracker
        .lock()
        .ok()
        .and_then(|mut guard| guard.resolve(pkid));
    let (topic, qos) = match resolved {
        Some((topic, qos)) => (topic, qos),
        None => (String::new(), Qos::AtMostOnce),
    };
    let message = describe_publish_rejected(
        if topic.is_empty() {
            None
        } else {
            Some(topic.as_str())
        },
        code,
        reason_string,
    );
    warn!(
        "MQTT5 发布被拒绝: pkid={}, topic='{}', {}",
        pkid, topic, message
    );
    if let Some(s) = event_sender {
        let _ = s
            .send(ConnectionEvent::MqttPublishFailed(
                config_id.to_string(),
                MqttPublishFailure {
                    topic,
                    qos,
                    message,
                },
            ))
            .await;
    }
}

/// MQTT 5.0 客户端（双任务模型，与 `MqttClient` 同构）。
pub struct MqttV5Client {
    config: ClientConfig,
    event_sender: Option<Sender<ConnectionEvent>>,
    message_processor: Arc<dyn MessageProcessor>,
    net_counters: Option<NetCounters>,
    reply_rules: Arc<ReplyRulesStore>,
    cancel_token: CancellationToken,
    /// 任务 A 的句柄：`disconnect()` 有界等待其退出，保证 `Disconnected` 的 FIFO 顺序（F1）
    read_task: Arc<StdMutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl MqttV5Client {
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
            read_task: Arc::new(StdMutex::new(None)),
        }
    }
}

impl NetworkConnection for MqttV5Client {
    fn connect(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>> {
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let message_processor = self.message_processor.clone();
        let net_counters = self.net_counters.clone();
        let reply_rules = self.reply_rules.clone();
        let cancel = self.cancel_token.clone();
        let read_task_slot = self.read_task.clone();

        Pin::from(Box::new(async move {
            let broker_endpoint =
                resolve_broker_endpoint(&config.server_address, config.server_port).await;
            let client_id = config
                .mqtt
                .as_ref()
                .map(|s| s.client_id.clone())
                .unwrap_or_default();
            info!(
                "MQTT5 客户端连接 broker: {}:{} (client_id={})",
                config.server_address, config.server_port, client_id
            );

            let options = match build_options_v5(&config) {
                Ok(options) => options,
                Err(reason) => {
                    // TLS 配置在建立网络连接前即失败：直接上报终态错误，不 spawn 任何任务
                    error!("MQTT5 TLS 配置无效: {}", reason);
                    if let Some(sender) = &event_sender {
                        let _ = sender
                            .send(ConnectionEvent::Error(
                                config.id.clone(),
                                t!("mqtt_net.tls_config_failed", reason => reason).to_string(),
                            ))
                            .await;
                    }
                    return Ok(());
                }
            };
            let (client, mut eventloop) = AsyncClient::new(options, REQUEST_CHANNEL_CAP);

            // 写通道（发布）/ 订阅控制通道
            let (write_tx, write_rx) = smol_bounded::<WireMessage>(WRITE_CHANNEL_CAP);
            let (control_tx, control_rx) = smol_unbounded::<MqttCommand>();
            // 订阅确认追踪（与 v4 共用同一份实现）
            let subscription_tracker = Arc::new(StdMutex::new(SubscriptionTracker::default()));
            // 发布确认追踪：PUBACK/PUBREC/PUBCOMP 只带 pkid，需还原主题才能提示「xx 主题没有权限」。
            // 仅 v5 需要（3.1.1 的 PUBACK 没有 reason code）。
            let publish_tracker = Arc::new(StdMutex::new(PublishTracker::default()));

            if let Some(sender) = &event_sender {
                if let Err(e) = sender
                    .send(ConnectionEvent::ClientWriteSenderReady(
                        config.id.clone(),
                        write_tx.clone(),
                    ))
                    .await
                {
                    error!(
                        "[MQTT5客户端] 发送 ClientWriteSenderReady 事件失败: {:?}",
                        e
                    );
                }
                if let Err(e) = sender
                    .send(ConnectionEvent::MqttControlSenderReady(
                        config.id.clone(),
                        control_tx,
                    ))
                    .await
                {
                    error!(
                        "[MQTT5客户端] 发送 MqttControlSenderReady 事件失败: {:?}",
                        e
                    );
                }
            } else {
                error!("[MQTT5客户端] event_sender 为空，无法下发写入/订阅控制通道");
            }

            // ---- 任务 A：事件循环（只 poll，绝不调用 client 方法）----
            let read_cancel = cancel.clone();
            let read_event_sender = event_sender.clone();
            let read_config_id = config.id.clone();
            let read_processor = message_processor.clone();
            let read_counters = net_counters.clone();
            let read_rules = reply_rules.clone();
            let read_tracker = subscription_tracker.clone();
            let read_publish_tracker = publish_tracker.clone();
            let reply_sender = write_tx.clone();
            let read_handle = tokio::spawn(async move {
                let mut batch = ReceivedBatch::with_capacity(BATCH_MAX);
                let mut connected_once = false;
                // 终态错误路径标记：已发 `Error` 的路径退出时不再补发 `Disconnected`
                let mut terminal_error = false;
                let mut last_warning: Option<String> = None;
                // 取消后进入排空态，等待 DISCONNECT 真正写出或超时（与 v4 相同语义）
                let mut drain_deadline: Option<tokio::time::Instant> = None;
                // SUBACK 超时检查必须是独立任务：与 `eventloop.poll()` 共用 `select!` 会
                // 在 tick 就绪时丢弃尚未完成的 poll future（取消半途读写，破坏连接）。
                let tick_tracker = read_tracker.clone();
                let tick_publish_tracker = read_publish_tracker.clone();
                let tick_sender = read_event_sender.clone();
                let tick_config_id = read_config_id.clone();
                let tick_cancel = read_cancel.clone();
                let tick_handle = tokio::spawn(async move {
                    let mut tick = tokio::time::interval(Duration::from_secs(1));
                    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    loop {
                        tokio::select! {
                            _ = tick_cancel.cancelled() => break,
                            _ = tick.tick() => {
                                emit_expired_subscriptions(
                                    &tick_tracker,
                                    &tick_sender,
                                    &tick_config_id,
                                )
                                .await;
                                // 发布记录只做 TTL 清理（不主动上报失败）：ACK 丢失时避免 map 泄漏
                                let cleared = tick_publish_tracker
                                    .lock()
                                    .map(|mut guard| guard.expire(Instant::now()))
                                    .unwrap_or(0);
                                if cleared > 0 {
                                    debug!("MQTT5 清理 {} 条超时发布记录", cleared);
                                }
                            }
                        }
                    }
                });
                loop {
                    if read_cancel.is_cancelled() && drain_deadline.is_none() {
                        info!("MQTT5 收到取消信号，排空等待 DISCONNECT 报文写出");
                        drain_deadline =
                            Some(tokio::time::Instant::now() + DISCONNECT_DRAIN_TIMEOUT);
                    }
                    let polled = match drain_deadline {
                        Some(deadline) if tokio::time::Instant::now() >= deadline => {
                            warn!("MQTT5 排空等待 DISCONNECT 超时，直接关闭连接");
                            break;
                        }
                        Some(deadline) => {
                            let remaining =
                                deadline.saturating_duration_since(tokio::time::Instant::now());
                            match tokio::time::timeout(remaining, eventloop.poll()).await {
                                Ok(result) => result,
                                Err(_) => {
                                    warn!("MQTT5 排空等待 DISCONNECT 超时，直接关闭连接");
                                    break;
                                }
                            }
                        }
                        None => eventloop.poll().await,
                    };
                    match polled {
                        Ok(Event::Incoming(Packet::ConnAck(_))) => {
                            // 连接已恢复：清空告警去重状态，使下次断线仍能提示（P4）
                            last_warning = None;
                            if connected_once {
                                // 重连：旧会话的 pkid 不会再收到 SUBACK，清空追踪状态。
                                // 首次连接不能 reset（会清掉 MqttControlSenderReady 提前登记的批次）。
                                if let Ok(mut guard) = read_tracker.lock() {
                                    guard.reset();
                                }
                                info!("MQTT5 已重连");
                                if let Some(s) = &read_event_sender {
                                    // 重连成功是正常状态变化：走 StatusHint（成功色 + 自动消失）
                                    let _ = s
                                        .send(ConnectionEvent::StatusHint(
                                            read_config_id.clone(),
                                            t!("mqtt_net.reconnected").to_string(),
                                        ))
                                        .await;
                                    // 订阅权威在 app：由 app 按当前 enabled 列表重发批次
                                    let _ = s
                                        .send(ConnectionEvent::Reconnected(read_config_id.clone()))
                                        .await;
                                }
                            } else {
                                connected_once = true;
                                info!("MQTT5 连接成功（CONNACK 已确认）: {}", broker_endpoint);
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
                            // v5 的 topic 是 Bytes；非法 UTF-8 用 lossy 兜底，不丢帧
                            let topic = String::from_utf8_lossy(&p.topic).into_owned();
                            process_frame_mqtt(
                                &p.payload,
                                &topic,
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
                            // v5 的 SUBACK 是 reason code 数组，且带可选的 reason string
                            let server = ack
                                .properties
                                .as_ref()
                                .and_then(|p| p.reason_string.as_deref());
                            let codes: Vec<Option<String>> = ack
                                .return_codes
                                .iter()
                                .map(|code| match code {
                                    SubscribeReasonCode::Success(_) => None,
                                    other => Some(suback_reason(*other, server)),
                                })
                                .collect();
                            // NotAuthorized 需要逐 topic 的精确文案（「主题「xx」没有权限」）
                            let unauthorized: Vec<bool> = ack
                                .return_codes
                                .iter()
                                .map(|code| matches!(code, SubscribeReasonCode::NotAuthorized))
                                .collect();
                            let resolved = read_tracker
                                .lock()
                                .ok()
                                .and_then(|mut guard| guard.resolve(ack.pkid, &codes));
                            match resolved {
                                Some(mut items) => {
                                    if items.len() == unauthorized.len() {
                                        for (item, denied) in
                                            items.iter_mut().zip(unauthorized.iter())
                                        {
                                            if !*denied {
                                                continue;
                                            }
                                            let reason = t!(
                                                "mqtt_net.suback_not_authorized_topic",
                                                topic => item.topic.clone()
                                            )
                                            .to_string();
                                            item.reason =
                                                Some(match server.filter(|s| !s.is_empty()) {
                                                    Some(detail) => t!(
                                                        "mqtt_net.suback_reason_with_server",
                                                        reason => reason,
                                                        detail => detail
                                                    )
                                                    .to_string(),
                                                    None => reason,
                                                });
                                        }
                                    }
                                    if items.iter().any(|item| !item.ok) {
                                        warn!(
                                            "MQTT5 SUBACK 含失败原因: pkid={}, {:?}",
                                            ack.pkid, ack.return_codes
                                        );
                                    } else {
                                        debug!(
                                            "MQTT5 SUBACK: pkid={}, codes={:?}",
                                            ack.pkid, ack.return_codes
                                        );
                                    }
                                    if let Some(s) = &read_event_sender {
                                        let _ = s
                                            .send(ConnectionEvent::MqttSubAck(
                                                read_config_id.clone(),
                                                items,
                                            ))
                                            .await;
                                    }
                                }
                                None => debug!("MQTT5 忽略未知/已超时的 SUBACK: pkid={}", ack.pkid),
                            }
                        }
                        Ok(Event::Incoming(Packet::PubAck(ack))) => {
                            // Success / NoMatchingSubscribers（消息已接受，仅当前无订阅者）都不是失败；
                            // 其余 reason code 表示 broker 拒收这条 QoS1 发布（连接仍可用）。
                            if matches!(
                                ack.reason,
                                PubAckReason::Success | PubAckReason::NoMatchingSubscribers
                            ) {
                                // 成功确认：立即消费在途记录，否则 QoS1 高频发布会在 TTL 内累积
                                let _ = read_publish_tracker
                                    .lock()
                                    .map(|mut guard| guard.resolve(ack.pkid));
                            } else {
                                let detail = ack
                                    .properties
                                    .as_ref()
                                    .and_then(|p| p.reason_string.as_deref());
                                emit_publish_rejected(
                                    &read_event_sender,
                                    &read_config_id,
                                    &read_publish_tracker,
                                    ack.pkid,
                                    &ack.reason,
                                    detail,
                                )
                                .await;
                            }
                        }
                        Ok(Event::Incoming(Packet::PubRec(rec))) => {
                            // QoS2 第一段确认：失败码只拒绝该条消息，连接不受影响。
                            // 成功时保留在途记录，等 PUBCOMP 再清理（其失败码也要能还原主题）。
                            if !matches!(
                                rec.reason,
                                PubRecReason::Success | PubRecReason::NoMatchingSubscribers
                            ) {
                                let detail = rec
                                    .properties
                                    .as_ref()
                                    .and_then(|p| p.reason_string.as_deref());
                                emit_publish_rejected(
                                    &read_event_sender,
                                    &read_config_id,
                                    &read_publish_tracker,
                                    rec.pkid,
                                    &rec.reason,
                                    detail,
                                )
                                .await;
                            }
                        }
                        Ok(Event::Incoming(Packet::PubComp(comp))) => {
                            // QoS2 完成：无论成功与否都在此消费在途记录
                            if matches!(comp.reason, PubCompReason::Success) {
                                let _ = read_publish_tracker
                                    .lock()
                                    .map(|mut guard| guard.resolve(comp.pkid));
                            } else {
                                let detail = comp
                                    .properties
                                    .as_ref()
                                    .and_then(|p| p.reason_string.as_deref());
                                emit_publish_rejected(
                                    &read_event_sender,
                                    &read_config_id,
                                    &read_publish_tracker,
                                    comp.pkid,
                                    &comp.reason,
                                    detail,
                                )
                                .await;
                            }
                        }
                        Ok(Event::Incoming(other)) => {
                            // PubComp/PubRel/PingResp/UnsubAck 等握手报文
                            debug!("MQTT5 收到报文: {:?}", other);
                        }
                        Ok(Event::Outgoing(Outgoing::Subscribe(pkid))) => {
                            // SUBSCRIBE 已写出：把 pkid 与任务 B 预登记的最早批次绑定
                            let bound = read_tracker
                                .lock()
                                .map(|mut guard| guard.bind(pkid))
                                .unwrap_or(false);
                            if bound {
                                debug!("MQTT5 SUBSCRIBE 已写出，绑定 pkid={}", pkid);
                            } else {
                                debug!(
                                    "MQTT5 Outgoing::Subscribe(pkid={}) 无待绑定批次（重连残留）",
                                    pkid
                                );
                            }
                        }
                        Ok(Event::Outgoing(Outgoing::Publish(pkid))) => {
                            // PUBLISH 已写出：把 pkid 与任务 B 预登记的最早发布绑定。
                            // pkid == 0 是 QoS 0（无确认）：不查表也不打日志，避免高频发送时刷屏。
                            if pkid != 0 {
                                let bound = read_publish_tracker
                                    .lock()
                                    .map(|mut guard| guard.bind(pkid))
                                    .unwrap_or(false);
                                if !bound {
                                    debug!(
                                        "MQTT5 Outgoing::Publish(pkid={}) 无待绑定发布（重连残留）",
                                        pkid
                                    );
                                }
                            }
                        }
                        Ok(Event::Outgoing(out)) => {
                            debug!("MQTT5 发出报文: {:?}", out);
                            // 排空完成的唯一可靠信号：该事件返回前报文已 write+flush 到网络
                            if matches!(out, Outgoing::Disconnect) {
                                info!("MQTT5 DISCONNECT 报文已写出，事件循环退出");
                                break;
                            }
                        }
                        Err(e) => {
                            // 连接异常：旧会话的 SUBACK / PUBACK 不会再到达，清空追踪状态
                            if let Ok(mut guard) = read_tracker.lock() {
                                guard.reset();
                            }
                            if let Ok(mut guard) = read_publish_tracker.lock() {
                                guard.reset();
                            }
                            if read_cancel.is_cancelled() {
                                break;
                            }
                            // v5 的 broker 主动断开走 Err(ServerDisconnect)，没有
                            // Ok(Packet::Disconnect) 事件（B5/B6）；必须先于普通断线处理，
                            // 否则会退化成「一直提示自动重连」的假象。
                            if let rumqttc::v5::ConnectionError::MqttState(
                                StateError::ServerDisconnect {
                                    reason_code,
                                    reason_string,
                                },
                            ) = &e
                            {
                                let message = describe_server_disconnect(
                                    *reason_code,
                                    reason_string.as_deref(),
                                );
                                error!("MQTT5 Broker 主动断开连接: {}", message);
                                if let Some(s) = &read_event_sender {
                                    let _ = s
                                        .send(ConnectionEvent::Error(
                                            read_config_id.clone(),
                                            message,
                                        ))
                                        .await;
                                }
                                // 终态：状态停在 Error，退出时不再补发 Disconnected
                                terminal_error = true;
                                read_cancel.cancel();
                                break;
                            }
                            if !connected_once {
                                // 首次连接失败（含 CONNACK 被拒）→ 终态
                                let reason = describe_connect_error(&e);
                                error!("MQTT5 首次连接失败: {}", reason);
                                if let Some(s) = &read_event_sender {
                                    let _ = s
                                        .send(ConnectionEvent::Error(
                                            read_config_id.clone(),
                                            t!("mqtt_net.connect_failed", reason => reason)
                                                .to_string(),
                                        ))
                                        .await;
                                }
                                terminal_error = true;
                                read_cancel.cancel();
                                break;
                            }
                            // 运行中被动断开：rumqttc 内建自动重连，仅提示（非终态）
                            warn!("MQTT5 连接断开，自动重连中: {}", e);
                            let message = t!("mqtt_net.reconnecting").to_string();
                            if should_emit_warning(&mut last_warning, &message)
                                && let Some(s) = &read_event_sender
                            {
                                let _ = s
                                    .send(ConnectionEvent::Warning(read_config_id.clone(), message))
                                    .await;
                            }
                        }
                    }
                    // 洪泛聚合：缓冲队列已排空或达到上限即推送本批
                    if batch.count >= BATCH_MAX as u64 || eventloop.state.events.is_empty() {
                        flush_batch(&read_event_sender, &read_config_id, &mut batch);
                    }
                }
                // 停止超时检查，避免在 Disconnected 之后仍推 MqttSubAck
                tick_handle.abort();
                flush_batch(&read_event_sender, &read_config_id, &mut batch);
                if !terminal_error && let Some(s) = &read_event_sender {
                    let _ = s
                        .send(ConnectionEvent::Disconnected(read_config_id.clone()))
                        .await;
                }
                info!("MQTT5 事件循环任务退出");
            });
            if let Ok(mut slot) = read_task_slot.lock() {
                *slot = Some(read_handle);
            }

            // ---- 任务 B：客户端操作（唯一持有 AsyncClient）----
            let write_cancel = cancel.clone();
            let write_event_sender = event_sender.clone();
            let write_config_id = config.id.clone();
            let write_counters = net_counters.clone();
            let write_tracker = subscription_tracker.clone();
            let write_publish_tracker = publish_tracker.clone();
            tokio::spawn(async move {
                // 接收端用 Option 包装：通道关闭后置 None 让分支退化为 pending，避免 select! 忙转
                let mut write_rx = Some(write_rx);
                let mut control_rx = Some(control_rx);
                loop {
                    tokio::select! {
                        biased;

                        _ = write_cancel.cancelled() => {
                            info!("MQTT5 客户端操作任务收到取消信号，发送 DISCONNECT 后退出");
                            let _ = client.disconnect().await;
                            break;
                        }

                        // 订阅 / 取消订阅（运行时下发，无需重连）
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
                                    dispatch_subscribe_v5(
                                        &client,
                                        &write_tracker,
                                        &write_event_sender,
                                        &write_config_id,
                                        vec![(topic, qos)],
                                    )
                                    .await;
                                }
                                Ok(MqttCommand::SubscribeBatch(topics)) => {
                                    dispatch_subscribe_v5(
                                        &client,
                                        &write_tracker,
                                        &write_event_sender,
                                        &write_config_id,
                                        topics,
                                    )
                                    .await;
                                }
                                Ok(MqttCommand::Unsubscribe { topic }) => {
                                    match client.unsubscribe(topic.clone()).await {
                                        Ok(()) => {}
                                        Err(e) => {
                                            warn!("MQTT5 取消订阅 '{}' 失败: {:?}", topic, e);
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
                                    debug!("MQTT5 订阅控制通道已关闭, 不再接收订阅下发");
                                    control_rx = None;
                                }
                            }
                        }

                        // 发布：payload 原样，topic/QoS 取自 mqtt_meta
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
                                    let (payload, meta) = wire.into_parts();
                                    match meta {
                                        Some(MqttPublishMeta {
                                            topic,
                                            qos,
                                            retain,
                                        }) => {
                                            // QoS1/2 先登记，等 Outgoing::Publish(pkid) 绑定；
                                            // QoS0 没有确认，不入表（queue 内部也会忽略）。
                                            if let Ok(mut guard) = write_publish_tracker.lock() {
                                                guard.queue(topic.clone(), qos);
                                            }
                                            match client
                                                .publish(
                                                    topic.as_str(),
                                                    to_v5_qos(qos),
                                                    retain,
                                                    payload,
                                                )
                                                .await
                                            {
                                                Ok(()) => {
                                                    // 计数口径与 v4 一致：请求进入 rumqttc 内部通道即计数
                                                    if let Some(c) = &write_counters {
                                                        c.add_sent(1);
                                                        c.add_sent_bytes(len);
                                                    }
                                                }
                                                Err(e) => {
                                                    // 请求没进 rumqttc：回滚登记，避免后续发布整体错位
                                                    if let Ok(mut guard) =
                                                        write_publish_tracker.lock()
                                                    {
                                                        guard.rollback_last();
                                                    }
                                                    warn!("MQTT5 发布到 '{}' 失败: {:?}", topic, e);
                                                    // 本地投递失败对所有 QoS 都提示：这是可确定的发送失败
                                                    // （QoS0 的 broker 拒收无法检测，但本地失败可以）。
                                                    if let Some(s) = &write_event_sender {
                                                        let message = t!(
                                                            "mqtt_net.publish_failed_local",
                                                            topic => topic.clone(),
                                                            reason => format!("{e:?}")
                                                        )
                                                        .to_string();
                                                        let _ = s
                                                            .send(ConnectionEvent::MqttPublishFailed(
                                                                write_config_id.clone(),
                                                                MqttPublishFailure {
                                                                    topic: topic.clone(),
                                                                    qos,
                                                                    message,
                                                                },
                                                            ))
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                        None => {
                                            warn!(
                                                "MQTT5 写通道收到缺少 topic/QoS 元数据的数据，已忽略"
                                            );
                                            if let Some(s) = &write_event_sender {
                                                let _ = s
                                                    .send(ConnectionEvent::Warning(
                                                        write_config_id.clone(),
                                                        t!("mqtt_net.missing_topic_meta").to_string(),
                                                    ))
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    debug!("MQTT5 写入通道已关闭");
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
        self.cancel_token.cancel();
        let handle = self.read_task.lock().ok().and_then(|mut slot| slot.take());
        Pin::from(Box::new(async move {
            if let Some(handle) = handle {
                match tokio::time::timeout(DISCONNECT_WAIT_TASK_TIMEOUT, handle).await {
                    Ok(_) => {}
                    Err(_) => warn!(
                        "MQTT5 等待事件循环任务退出超时（{}s），继续断开",
                        DISCONNECT_WAIT_TASK_TIMEOUT.as_secs()
                    ),
                }
            }
            Ok(())
        }))
    }
}

/// MQTT 无服务端角色：仅保证工厂 `match` 穷尽，启动即返回明确错误。
///
/// `ServerConfig` 不携带 `mqtt` 设置（版本是客户端属性），因此工厂实际复用
/// `MqttServer`；保留本占位是为了让 v5 客户端与占位成对出现，便于以后按版本分流。
#[allow(dead_code)]
pub struct MqttV5Server {
    #[allow(dead_code)]
    config: ServerConfig,
}

#[allow(dead_code)]
impl MqttV5Server {
    pub fn new(
        config: ServerConfig,
        _event_sender: Option<Sender<ConnectionEvent>>,
        _net_counters: Option<NetCounters>,
        _trailer: crate::config::connection::TrailerSetting,
    ) -> Self {
        Self { config }
    }
}

impl NetworkServer for MqttV5Server {
    fn start(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>> {
        Box::pin(async {
            Err("MQTT 不支持服务端角色（规划 D-5：仅客户端）".into())
        })
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
    use crate::config::connection::{ConnectionType, MqttSettings, MqttVersion, WillSettings};

    fn mqtt_config(settings: MqttSettings) -> ClientConfig {
        ClientConfig {
            protocol: ConnectionType::Mqtt,
            server_address: "broker.example".to_string(),
            server_port: 1883,
            mqtt: Some(settings),
            ..Default::default()
        }
    }

    /// 配置映射：clean start / keep alive / 遗嘱 / 会话过期 / 接收最大值逐项落到 v5 选项。
    #[test]
    fn test_build_options_v5_reflects_mqtt_settings() {
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
            protocol_version: MqttVersion::V5,
            session_expiry_secs: Some(300),
            receive_maximum: Some(20),
            tls: None,
        });

        let options = build_options_v5(&config).expect("明文配置应可构造");
        assert_eq!(
            options.broker_address(),
            ("broker.example".to_string(), 1883)
        );
        assert_eq!(options.client_id(), "cid");
        assert_eq!(options.keep_alive(), Duration::from_secs(30));
        assert!(!options.clean_start());
        assert_eq!(options.session_expiry_interval(), Some(300));
        assert_eq!(options.receive_maximum(), Some(20));

        let creds = options.credentials().expect("应携带用户名密码");
        assert_eq!(creds.username, "u");
        assert_eq!(creds.password, "p");

        let will = options.last_will().expect("应携带遗嘱消息");
        assert_eq!(&will.topic[..], b"will/t");
        assert_eq!(&will.message[..], b"bye");
        assert_eq!(will.qos, V5Qos::AtLeastOnce);
        assert!(will.retain);
    }

    /// 空 client_id + clean_session=false 必须强制 clean start（与 v4 行为一致）。
    #[test]
    fn test_build_options_v5_forces_clean_start_when_client_id_empty() {
        let config = mqtt_config(MqttSettings {
            client_id: String::new(),
            clean_session: false,
            ..MqttSettings::default()
        });
        let options = build_options_v5(&config).expect("空 client_id 配置应可构造");
        assert!(options.clean_start(), "空 client_id 时必须强制 clean start");
    }

    /// 未配置 v5 属性时不下发（保持 broker 默认语义）。
    #[test]
    fn test_build_options_v5_omits_unset_properties() {
        let config = mqtt_config(MqttSettings::default());
        let options = build_options_v5(&config).expect("默认配置应可构造");
        assert_eq!(options.session_expiry_interval(), None);
        assert_eq!(options.receive_maximum(), None);
        assert!(matches!(options.transport(), rumqttc::Transport::Tcp));
    }

    /// TLS 启用时 transport 为 Tls，关闭时为 Tcp。
    #[test]
    fn test_build_options_v5_applies_tls_transport_only_when_enabled() {
        let with_tls = mqtt_config(MqttSettings {
            client_id: "cid".to_string(),
            tls: Some(crate::config::connection::TlsSettings {
                enabled: true,
                verify: crate::config::connection::TlsVerifyMode::Insecure,
                ..Default::default()
            }),
            ..MqttSettings::default()
        });
        let options = build_options_v5(&with_tls).expect("跳过校验配置应可构造");
        assert!(matches!(options.transport(), rumqttc::Transport::Tls(_)));
    }

    /// 持久化配置可能残留非法值：1-4 秒 KeepAlive 会触发 v5 的 assert（v4 只要求 >=1），
    /// `receive_maximum=0` 是协议禁止值。构建选项时净化，而不是 panic / 下发非法属性。
    #[test]
    fn test_build_options_v5_sanitizes_invalid_persisted_values() {
        let config = mqtt_config(MqttSettings {
            keep_alive_secs: 1,
            receive_maximum: Some(0),
            ..MqttSettings::default()
        });
        let options = build_options_v5(&config).expect("非法持久化值不应导致 panic");
        assert_eq!(options.keep_alive(), Duration::from_secs(5));
        assert_eq!(options.receive_maximum(), None);
    }

    /// 发布确认失败码映射为可读原因，并附 broker 的 reason string。
    #[test]
    fn test_describe_publish_rejected_is_readable() {
        // 未授权使用带主题的专用文案，与订阅失败保持一致
        let unauthorized =
            describe_publish_rejected(Some("sensor/1"), &PubAckReason::NotAuthorized, None);
        assert_eq!(
            unauthorized,
            t!("mqtt_net.publish_not_authorized_topic", topic => "sensor/1").to_string()
        );

        // 其他失败码带主题与 broker reason string
        let with_detail = describe_publish_rejected(
            Some("sensor/1"),
            &PubRecReason::QuotaExceeded,
            Some("quota full"),
        );
        assert!(
            with_detail.contains("sensor/1"),
            "应带上主题: {with_detail}"
        );
        assert!(with_detail.contains("QuotaExceeded"));
        assert!(with_detail.contains("quota full"));

        // 主题无法还原时退化为不含主题的通用文案
        let unknown_topic = describe_publish_rejected(None, &PubAckReason::NotAuthorized, None);
        assert!(unknown_topic.contains("NotAuthorized"));
        assert!(
            !unknown_topic.contains("sensor/1"),
            "不应凭空生成主题: {unknown_topic}"
        );
    }

    /// SUBACK 失败码映射为可读原因，并附 broker 的 reason string。
    #[test]
    fn test_suback_reason_is_readable() {
        let msg = suback_reason(SubscribeReasonCode::NotAuthorized, Some("denied by ACL"));
        assert!(msg.contains(&t!("mqtt_net.suback_reason_not_authorized").to_string()));
        assert!(
            msg.contains("denied by ACL"),
            "应带上 broker 的 reason string: {msg}"
        );

        let invalid = suback_reason(SubscribeReasonCode::TopicFilterInvalid, None);
        assert_eq!(
            invalid,
            t!("mqtt_net.suback_reason_topic_filter_invalid").to_string()
        );
    }

    /// broker 主动断开：文案同时带 reason code 与 reason string。
    #[test]
    fn test_describe_server_disconnect_includes_reason() {
        let msg = describe_server_disconnect(
            DisconnectReasonCode::SessionTakenOver,
            Some("same client id"),
        );
        assert!(msg.contains("SessionTakenOver"));
        assert!(msg.contains("same client id"));
    }

    // ===================== 假 broker 端到端（v5 报文结构） =====================

    /// 读取一个 MQTT 报文：返回（首字节, 报文体）。
    async fn read_mqtt_packet(sock: &mut tokio::net::TcpStream) -> std::io::Result<(u8, Vec<u8>)> {
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

    /// 编码 v5 QoS0 PUBLISH：topic 后必须补属性长度字段 0x00（最容易漏的字节）。
    fn encode_mqtt_v5_publish(topic: &str, payload: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&(topic.len() as u16).to_be_bytes());
        body.extend_from_slice(topic.as_bytes());
        body.push(0x00); // 属性长度 = 0
        body.extend_from_slice(payload);
        let mut pkt = vec![0x30u8];
        assert!(body.len() < 128, "测试报文需为单字节剩余长度");
        pkt.push(body.len() as u8);
        pkt.extend_from_slice(&body);
        pkt
    }

    #[derive(Debug)]
    enum BrokerObs {
        Subscribed(String),
        Published { topic: String, payload: Vec<u8> },
    }

    /// 端到端：CONNECT → CONNACK → SUBSCRIBE → SUBACK → PUBLISH → 收帧 → 发布 → DISCONNECT。
    #[tokio::test]
    async fn test_mqtt_v5_connect_subscribe_publish_receive_roundtrip() {
        use crate::network::events::ConnectionEvent as Ev;
        use std::time::Duration;
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;
        use tokio::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broker_addr = listener.local_addr().unwrap();

        let (obs_tx, mut obs_rx) = mpsc::unbounded_channel::<BrokerObs>();

        let broker: tokio::task::JoinHandle<&'static str> = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            loop {
                let (header, body) = match read_mqtt_packet(&mut sock).await {
                    Ok(packet) => packet,
                    Err(_) => return "连接在收到 DISCONNECT 前被关闭（非优雅断开）",
                };
                match header {
                    0x10 => {
                        // CONNACK v5：ack flags / reason code / 属性长度 0
                        sock.write_all(&[0x20, 0x03, 0x00, 0x00, 0x00])
                            .await
                            .unwrap();
                    }
                    0x82 => {
                        // SUBSCRIBE v5：pkid(2) + 属性长度(1=0) + topic 过滤器
                        let pkid = [body[0], body[1]];
                        assert_eq!(body[2], 0x00, "SUBSCRIBE 属性长度应为 0");
                        let topic_len = u16::from_be_bytes([body[3], body[4]]) as usize;
                        let topic = String::from_utf8(body[5..5 + topic_len].to_vec()).unwrap();
                        // SUBACK v5：pkid + 属性长度 0 + reason code 0（成功 QoS0）
                        sock.write_all(&[0x90, 0x04, pkid[0], pkid[1], 0x00, 0x00])
                            .await
                            .unwrap();
                        obs_tx.send(BrokerObs::Subscribed(topic)).ok();
                        sock.write_all(&encode_mqtt_v5_publish("sensor/1", b"hello"))
                            .await
                            .unwrap();
                    }
                    0x30 => {
                        let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
                        let topic = String::from_utf8(body[2..2 + topic_len].to_vec()).unwrap();
                        // v5 的 PUBLISH 在 topic 之后有属性长度字段，漏读会把 0x00 当成 payload
                        assert_eq!(
                            body[2 + topic_len],
                            0x00,
                            "PUBLISH 属性长度应为 0（测试客户端不下发发布属性）"
                        );
                        obs_tx
                            .send(BrokerObs::Published {
                                topic,
                                payload: body[3 + topic_len..].to_vec(),
                            })
                            .ok();
                    }
                    0xE0 => return "ok",
                    _ => {}
                }
            }
        });

        let config = ClientConfig {
            protocol: ConnectionType::Mqtt,
            server_address: "127.0.0.1".to_string(),
            server_port: broker_addr.port(),
            mqtt: Some(MqttSettings {
                client_id: "netassist-v5-test".to_string(),
                protocol_version: MqttVersion::V5,
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
            .expect("MQTT5 客户端应启动成功");

        let mut write_sender = None;
        let mut control_sender = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while write_sender.is_none() || control_sender.is_none() {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let ev = tokio::time::timeout(remaining, rx_events.recv())
                .await
                .expect("等待 MQTT5 就绪事件超时")
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
                Ev::Error(id, msg) => panic!("MQTT5 连接不应失败: {id} {msg}"),
                _ => {}
            }
        }

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
            .expect("等待 broker 收到 SUBSCRIBE 超时")
            .expect("观测通道不应关闭");
        match obs {
            BrokerObs::Subscribed(topic) => assert_eq!(topic, "sensor/1"),
            other => panic!("首个观测应为 Subscribed, 实际: {other:?}"),
        }

        // 等 Connected（CONNACK 已到）
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

        // 收消息
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
                assert_eq!(batch.messages[0].topic.as_deref(), Some("sensor/1"));
                break;
            }
        }

        // 发布
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

        // 收尾：必须发出 v5 DISCONNECT（0xE0 0x00），否则 broker 按非优雅断开处理
        manager.disconnect_client(&config.id).await.unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(5), broker)
            .await
            .expect("等待 broker 观测到 DISCONNECT 超时: 断开未发出 DISCONNECT 报文")
            .expect("broker 任务不应 panic");
        assert_eq!(outcome, "ok");
    }

    /// CONNACK 被拒（0x87 NotAuthorized）→ 终态 Error；不产生 Connected，
    /// 也不补发 Disconnected（否则 UI 会把"被拒"误判成普通断线）。
    #[tokio::test]
    async fn test_mqtt_v5_connack_refused_is_terminal_error() {
        use crate::network::events::ConnectionEvent as Ev;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let broker: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let _ = read_mqtt_packet(&mut sock).await; // CONNECT
            // CONNACK v5：ack flags 0 / reason 0x87 NotAuthorized / 属性长度 0
            sock.write_all(&[0x20, 0x03, 0x00, 0x87, 0x00])
                .await
                .unwrap();
            let mut buf = [0u8; 1];
            let _ = sock.read(&mut buf).await; // 等客户端关闭连接
        });

        let config = ClientConfig {
            protocol: ConnectionType::Mqtt,
            server_address: "127.0.0.1".to_string(),
            server_port: port,
            mqtt: Some(MqttSettings {
                client_id: "netassist-v5-refused".to_string(),
                protocol_version: MqttVersion::V5,
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
            .expect("MQTT5 客户端应启动成功");

        let mut saw_connected = false;
        let mut error_msg = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while error_msg.is_none() {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let ev = tokio::time::timeout(remaining, rx_events.recv())
                .await
                .expect("等待 CONNACK 拒绝结果超时")
                .expect("事件通道不应关闭");
            match ev {
                Ev::Connected(..) => saw_connected = true,
                Ev::Error(_, msg) => error_msg = Some(msg),
                _ => {}
            }
        }
        assert!(!saw_connected, "CONNACK 被拒不应产生 Connected 事件");
        let msg = error_msg.unwrap();
        assert!(msg.contains("NotAuthorized"), "错误应带出拒绝码: {msg}");

        // 终态错误路径不应再补发 Disconnected
        let saw_disconnected = tokio::time::timeout(Duration::from_millis(300), async {
            while let Ok(ev) = rx_events.recv().await {
                if matches!(ev, Ev::Disconnected(_)) {
                    return true;
                }
            }
            false
        })
        .await;
        assert!(
            saw_disconnected.is_err() || !saw_disconnected.unwrap(),
            "终态错误后不应补发 Disconnected"
        );

        manager.disconnect_client(&config.id).await.ok();
        broker.await.ok();
    }

    /// broker 主动 DISCONNECT（0x8E SessionTakenOver + reason string）→ 带原因的
    /// 终态 Error，且**不能**退化成 v4 的"运行中被动断开 → 自动重连"提示。
    #[tokio::test]
    async fn test_mqtt_v5_server_disconnect_reports_reason_without_reconnect_hint() {
        use crate::network::events::ConnectionEvent as Ev;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let broker: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let _ = read_mqtt_packet(&mut sock).await; // CONNECT
            // CONNACK 成功
            sock.write_all(&[0x20, 0x03, 0x00, 0x00, 0x00])
                .await
                .unwrap();

            // DISCONNECT v5：reason=0x8E(SessionTakenOver) + Reason String(0x1F)
            let text = b"taken over";
            let mut props = vec![0x1Fu8];
            props.extend_from_slice(&(text.len() as u16).to_be_bytes());
            props.extend_from_slice(text);
            let mut pkt = vec![0xE0u8, 0x00, 0x8Eu8, props.len() as u8];
            pkt.extend_from_slice(&props);
            pkt[1] = (pkt.len() - 2) as u8; // 剩余长度
            sock.write_all(&pkt).await.unwrap();

            let mut buf = [0u8; 1];
            let _ = sock.read(&mut buf).await; // 等客户端关闭
        });

        let config = ClientConfig {
            protocol: ConnectionType::Mqtt,
            server_address: "127.0.0.1".to_string(),
            server_port: port,
            mqtt: Some(MqttSettings {
                client_id: "netassist-v5-disconnect".to_string(),
                protocol_version: MqttVersion::V5,
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
            .expect("MQTT5 客户端应启动成功");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let mut saw_connected = false;
        let mut error_msg = None;
        while error_msg.is_none() {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let ev = tokio::time::timeout(remaining, rx_events.recv())
                .await
                .expect("等待 broker DISCONNECT 结果超时")
                .expect("事件通道不应关闭");
            match ev {
                Ev::Connected(..) => saw_connected = true,
                Ev::Error(_, msg) => error_msg = Some(msg),
                _ => {}
            }
        }
        assert!(
            saw_connected,
            "应先收到 CONNACK 对应的 Connected，再收到 broker DISCONNECT"
        );
        let msg = error_msg.unwrap();
        assert!(msg.contains("SessionTakenOver"), "错误应带出原因码: {msg}");
        assert!(
            msg.contains("taken over"),
            "错误应带出 reason string: {msg}"
        );

        // 关键回归：broker 主动断开不得走"自动重连"分支
        let reconnecting = t!("mqtt_net.reconnecting").to_string();
        let saw_reconnecting = tokio::time::timeout(Duration::from_millis(400), async {
            while let Ok(ev) = rx_events.recv().await {
                if let Ev::Warning(_, msg) = ev
                    && msg == reconnecting
                {
                    return true;
                }
            }
            false
        })
        .await;
        assert!(
            saw_reconnecting.is_err() || !saw_reconnecting.unwrap(),
            "broker 主动断开不应提示自动重连"
        );

        manager.disconnect_client(&config.id).await.ok();
        broker.await.ok();
    }
}
