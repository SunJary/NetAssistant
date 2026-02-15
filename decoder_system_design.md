# NetAssistant 解码器系统设计需求文档

## 1. 项目背景

NetAssistant 是一个网络调试工具，需要实现多种解码器来处理 TCP 消息的接收和解析。本文档描述了使用 `tokio_util::codec` 从头实现解码器系统的设计方案。

## 2. 需求目标

### 2.1 核心目标
- 使用 `tokio_util::codec` 库实现解码器系统
- 支持多种解码器类型，满足不同场景的需求
- 提供简洁、高效的消息处理流程
- 集成 UI 界面，支持解码器选择

### 2.2 功能需求
1. **支持的解码器类型**：
   - 原始数据解码器（Bytes）：直接传递原始字节，使用 tokio_util 提供的 BytesCodec
   - 换行符解码器（LineBased）：按换行符分隔消息，使用 tokio_util 提供的 LinesCodec
   - 长度前缀解码器（LengthDelimited）：基于长度前缀解析消息，使用 tokio_util 提供的 LengthDelimitedCodec
   - JSON解码器（Json）：解析JSON格式消息，自定义实现

2. **TCP 消息处理**：
   - 正确处理 TCP 消息的拆包和粘包问题
   - 支持基于不同解码器的消息解析
   - 集成超时处理机制

3. **UDP 消息处理**：
   - UDP 不存在拆包问题，直接处理完整数据包
   - UI 上选择 UDP 时，不需要显示解码器选择界面

4. **配置系统**：
   - 实现解码器配置的序列化和反序列化
   - 通过 `Default` trait 提供合理的默认配置
   - 支持配置文件的保存和加载

5. **UI 集成**：
   - 在新建连接对话框中提供解码器选择界面（仅 TCP）
   - 在连接标签页中显示当前使用的解码器类型
   - 根据协议类型动态显示/隐藏解码器选择界面

## 3. 技术方案

### 3.1 技术选型
- **核心库**：`tokio-util = { version = "0.7", features = ["codec"] }`
- **辅助库**：`bytes = "1.4"`（用于字节处理）
- **序列化库**：`serde = { version = "1.0", features = ["derive"] }`
- **JSON库**：`serde_json = "1.0"`（用于JSON处理）
- **JSON编解码库**：`tokio-serde-json = "0.8"`（用于JSON编解码）

### 3.2 架构设计

#### 3.2.1 解码器架构
- **基于 `tokio_util::codec`**：使用官方提供的 `Decoder` 和 `Encoder` traits
- ** codec 组合**：为每种解码器实现独立的 codec
- **工厂模式**：创建 codec 工厂，根据配置生成相应的 codec 实例

#### 3.2.2 消息处理流程
1. **TCP 客户端流程**：
   - 建立 TCP 连接
   - 根据配置创建相应的 codec
   - 使用 `Framed` 包装 TcpStream
   - 通过 `Framed` 读取和写入消息
   - 处理超时和错误情况

2. **TCP 服务器流程**：
   - 启动 TCP 监听
   - 接受客户端连接
   - 为每个连接创建相应的 codec
   - 使用 `Framed` 包装 TcpStream
   - 处理客户端消息和连接事件

3. **UDP 处理流程**：
   - 建立 UDP 连接
   - 直接发送和接收完整数据包
   - 不需要 codec 处理

### 3.3 核心数据结构

#### 3.3.1 解码器配置结构
```rust
// 解码器类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecoderType {
    Bytes,              // 原始数据，使用 BytesCodec
    LineBased,          // 换行符，使用 LinesCodec
    LengthDelimited,    // 长度前缀，使用 LengthDelimitedCodec
    Json,               // JSON，自定义实现
}

// 长度前缀解码器配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LengthDelimitedConfig {
    pub max_frame_length: usize, // 最大帧长度
    pub length_field_offset: u8, // 长度字段偏移量
    pub length_field_length: u8, // 长度字段长度
    pub length_adjustment: i32,  // 长度调整值
    pub length_field_is_including_length_field: bool, // 长度字段是否包含自身长度
}

// 解码器配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecoderConfig {
    Bytes,
    LineBased,
    LengthDelimited(LengthDelimitedConfig),
    Json,
}

// 默认配置实现
impl Default for DecoderConfig {
    fn default() -> Self {
        DecoderConfig::Bytes
    }
}
```

#### 3.3.2 连接配置结构
```rust
// 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub id: String,
    pub server_address: String,
    pub server_port: u16,
    pub connection_type: ConnectionType,
    pub decoder_config: DecoderConfig, // 仅用于 TCP
}

// 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: String,
    pub listen_address: String,
    pub listen_port: u16,
    pub connection_type: ConnectionType,
    pub decoder_config: DecoderConfig, // 仅用于 TCP
}
```

### 3.4 解码器实现细节

#### 3.4.1 原始数据解码器
```rust
// 直接使用 tokio_util 提供的 BytesCodec
pub type BytesCodec = tokio_util::codec::BytesCodec;
```

#### 3.4.2 换行符解码器
```rust
// 直接使用 tokio_util 提供的 LinesCodec
pub type LineBasedCodec = tokio_util::codec::LinesCodec;
```

#### 3.4.3 长度前缀解码器
```rust
// 直接使用 tokio_util 提供的 LengthDelimitedCodec
pub type LengthDelimitedCodec = tokio_util::codec::LengthDelimitedCodec;
```

#### 3.4.4 JSON解码器
```rust
// 直接使用 tokio-serde-json 提供的 JsonCodec
pub type JsonCodec = tokio_serde_json::JsonCodec<serde_json::Value, serde_json::Value>;
```

### 3.5 Codec 工厂实现
```rust
pub struct CodecFactory;

impl CodecFactory {
    pub fn create_decoder(config: &DecoderConfig) -> Box<dyn Decoder<Item = BytesMut, Error = std::io::Error> + Send + Sync> {
        match config {
            DecoderConfig::Bytes => {
                Box::new(BytesCodec::new())
            }
            DecoderConfig::LineBased => {
                // 将LinesCodec包装成输出BytesMut的Decoder
                Box::new(LineToBytesMutDecoder::new())
            }
            DecoderConfig::LengthDelimited(config) => {
                let length_delimited = LengthDelimitedCodec::builder()
                    .max_frame_length(config.max_frame_length)
                    .length_field_offset(config.length_field_offset.into())
                    .length_field_length(config.length_field_length.into())
                    .length_adjustment(config.length_adjustment.try_into().unwrap_or(0))
                    .new_codec();
                Box::new(length_delimited)
            }
            DecoderConfig::Json => {
                // 对于JSON，我们直接使用BytesCodec
                Box::new(BytesCodec::new())
            }
        }
    }
    
    pub fn create_encoder(config: &DecoderConfig) -> Box<dyn Encoder<BytesMut, Error = std::io::Error> + Send + Sync> {
        match config {
            DecoderConfig::Bytes => {
                Box::new(BytesCodec::new())
            }
            DecoderConfig::LineBased => {
                // 将LinesCodec包装成输入BytesMut的Encoder
                Box::new(LineToBytesMutEncoder::new())
            }
            DecoderConfig::LengthDelimited(config) => {
                let length_delimited = LengthDelimitedCodec::builder()
                    .max_frame_length(config.max_frame_length)
                    .length_field_offset(config.length_field_offset.into())
                    .length_field_length(config.length_field_length.into())
                    .length_adjustment(config.length_adjustment.try_into().unwrap_or(0))
                    .new_codec();
                Box::new(length_delimited)
            }
            DecoderConfig::Json => {
                // 对于JSON，我们直接使用BytesCodec
                Box::new(BytesCodec::new())
            }
        }
    }
}
```

## 4. 消息处理流程

### 4.1 TCP 客户端消息处理
```rust
async fn handle_tcp_client(socket: TcpStream, config: ClientConfig) {
    let codec = CodecFactory::create_codec(&config.decoder_config);
    let mut framed = Framed::new(socket, codec);

    loop {
        // 接收消息
        match framed.next().await {
            Some(Ok(data)) => {
                // 处理接收到的消息
                println!("Received: {:?}", data);
            }
            Some(Err(e)) => {
                // 处理解码错误
                println!("Decode error: {:?}", e);
                break;
            }
            None => {
                // 连接关闭
                println!("Connection closed");
                break;
            }
        }
    }
}
```

### 4.2 TCP 服务器消息处理
```rust
async fn handle_tcp_server_connection(socket: TcpStream, addr: SocketAddr, config: ServerConfig) {
    let codec = CodecFactory::create_codec(&config.decoder_config);
    let mut framed = Framed::new(socket, codec);

    loop {
        // 接收消息
        match framed.next().await {
            Some(Ok(data)) => {
                // 处理接收到的消息
                println!("Received from {}: {:?}", addr, data);
            }
            Some(Err(e)) => {
                // 处理解码错误
                println!("Decode error from {}: {:?}", addr, e);
                break;
            }
            None => {
                // 连接关闭
                println!("Connection closed from {}", addr);
                break;
            }
        }
    }
}
```

### 4.3 UDP 消息处理
```rust
async fn handle_udp(socket: UdpSocket) {
    let mut buffer = [0; 65536];
    
    loop {
        match socket.recv_from(&mut buffer).await {
            Ok((n, addr)) => {
                // 直接处理完整的 UDP 数据包
                let data = &buffer[..n];
                println!("Received from {}: {:?}", addr, data);
            }
            Err(e) => {
                println!("UDP error: {:?}", e);
                break;
            }
        }
    }
}
```

## 5. UI 集成

### 5.1 新建连接对话框
1. **协议选择**：
   - 提供 TCP 和 UDP 两个选项
   - 默认选择 TCP

2. **解码器选择**（仅当选择 TCP 时显示）：
   - 提供四个解码器类型的选择按钮：原始数据、换行符、长度前缀、JSON
   - 默认选择原始数据解码器
   - 当选择不同解码器时，显示相应的配置选项

3. **配置选项**：
   - 原始数据解码器：无配置选项
   - 换行符解码器：无配置选项
   - 长度前缀解码器：输入最大帧长度、长度字段偏移量、长度字段长度、长度调整值、是否包含自身长度
   - JSON解码器：无配置选项

4. **确认逻辑**：
   - 验证配置的有效性
   - 根据选择创建相应的连接配置
   - 保存配置到存储系统

### 5.2 连接标签页
1. **连接信息显示**：
   - 显示协议类型、地址、端口等基本信息
   - 当协议为 TCP 时，显示当前使用的解码器类型
   - 当协议为 UDP 时，不显示解码器信息

2. **解码器信息**：
   - 显示解码器类型
   - 显示解码器的具体配置参数

### 5.3 UI 状态管理
```rust
// 新建连接对话框状态
struct NewConnectionState {
    protocol: ConnectionType, // TCP 或 UDP
    decoder_type: Option<DecoderType>, // 仅 TCP 有效
    decoder_config: Option<DecoderConfig>, // 仅 TCP 有效
    // 其他配置...
}

// 当协议改变时的处理
fn on_protocol_change(&mut self, protocol: ConnectionType) {
    self.protocol = protocol;
    if protocol == ConnectionType::UDP {
        // UDP 不需要解码器
        self.decoder_type = None;
        self.decoder_config = None;
    } else {
        // TCP 需要解码器，设置默认值
        self.decoder_type = Some(DecoderType::Bytes);
        self.decoder_config = Some(DecoderConfig::default());
    }
    // 更新 UI 显示
}
```

## 6. 配置系统

### 6.1 配置序列化和反序列化
```rust
// 使用 serde 实现配置的序列化和反序列化
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub connections: Vec<ConnectionConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionConfig {
    Client(ClientConfig),
    Server(ServerConfig),
}

// 保存配置
fn save_config(config: &AppConfig) -> Result<(), Box<dyn Error>> {
    let config_str = serde_json::to_string_pretty(config)?;
    std::fs::write("config.json", config_str)?;
    Ok(())
}

// 加载配置
fn load_config() -> Result<AppConfig, Box<dyn Error>> {
    let config_str = std::fs::read_to_string("config.json")?;
    let config = serde_json::from_str(&config_str)?;
    Ok(config)
}
```

### 6.2 默认配置
```rust
impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            server_address: "127.0.0.1".to_string(),
            server_port: 8080,
            connection_type: ConnectionType::Tcp,
            decoder_config: DecoderConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            listen_address: "0.0.0.0".to_string(),
            listen_port: 8080,
            connection_type: ConnectionType::Tcp,
            decoder_config: DecoderConfig::default(),
        }
    }
}
```

## 7. 技术风险评估

### 7.1 风险点
1. **JSON 解码器性能**：复杂的 JSON 解析可能影响性能
2. **UI 状态管理**：需要正确处理协议切换时的状态变化
3. **配置兼容性**：需要确保配置文件的格式正确

### 7.2 缓解策略
1. **性能优化**：
   - 使用更高效的 JSON 解析库
   - 优化解码器的缓冲区管理

2. **UI 测试**：
   - 充分测试协议切换时的 UI 状态变化
   - 确保 UI 操作的流畅性

3. **配置验证**：
   - 实现配置的验证逻辑
   - 提供合理的默认值和错误处理

## 8. 开发计划

### 8.1 阶段一：核心库实现
1. **添加依赖**：在 `Cargo.toml` 中添加必要的依赖
2. **实现解码器**：实现所有类型的解码器
3. **实现 codec 工厂**：创建 codec 工厂来管理解码器

### 8.2 阶段二：网络处理实现
1. **TCP 客户端**：实现基于 codec 的 TCP 客户端
2. **TCP 服务器**：实现基于 codec 的 TCP 服务器
3. **UDP 处理**：实现 UDP 消息处理

### 8.3 阶段三：配置系统实现
1. **配置结构**：实现配置的序列化和反序列化
2. **配置存储**：实现配置的保存和加载
3. **默认配置**：实现合理的默认配置

### 8.4 阶段四：UI 集成实现
1. **新建连接对话框**：实现解码器选择界面
2. **连接标签页**：实现解码器信息显示
3. **UI 状态管理**：实现协议切换时的状态管理

### 8.5 阶段五：测试和验证
1. **单元测试**：为每个解码器编写单元测试
2. **集成测试**：测试完整的消息处理流程
3. **UI 测试**：测试 UI 界面的交互逻辑
4. **性能测试**：测试系统的性能表现

## 9. 总结

本设计文档详细描述了使用 `tokio_util::codec` 从头实现 NetAssistant 解码器系统的方案。通过使用官方提供的 codec 库，可以获得以下好处：

1. **代码简化**：减少自定义缓冲管理和编解码逻辑
2. **性能提升**：利用 Tokio 官方优化的编解码实现
3. **可维护性**：使用标准库，减少维护成本
4. **扩展性**：更容易添加新的解码器类型
5. **正确性**：官方库经过充分测试，可靠性更高

同时，本设计考虑了 UDP 协议的特点，针对 TCP 和 UDP 分别实现了适合的消息处理机制。在 UI 方面，根据协议类型动态显示/隐藏解码器选择界面，提供了更好的用户体验。