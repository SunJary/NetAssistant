---
name: "seo-docs"
description: "为 NetAssistant 官网（docs/ + VitePress）撰写与优化 SEO 页面：TCP/UDP 调试、MQTT、平台落地页、教程、排错与竞品对比，负责 title/description/FAQ/内链/中英镜像/结构化数据与关键词分工。Invoke when 新增或改写 docs 任意页面、规划内容集群、检查站内关键词冲突或技术 SEO 时使用。"
---

# NetAssistant 官网 SEO 写作

把 `docs/` 当成获客入口而不是产品说明书：一个页面只服务一个搜索意图，先回答、再演示、最后给下载入口。

## 适用范围

**适用**：

- 新增或改写 SEO 页面：TCP/UDP 工具页、MQTT 页、教程页、排错页、平台落地页、竞品对比页、FAQ
- 调整页面 title / description / H1 / FAQ / 内链 / JSON-LD
- 补英文镜像、规划内容集群、判断扩写还是新建、排查关键词冲突

**不适用**：

- `changelog` 与版本发布文档 → 走 [release-changelog](file:///d:/code/NetAssistant/skills/release-changelog/SKILL.md)
- GitHub Release 页说明 → 由 [release.yml](file:///d:/code/NetAssistant/.github/workflows/release.yml) 生成（「更新内容」来自提交记录），不要在这里改 Release Notes
- README、内部规划文档、纯代码改动、产品 UI 文案

## 写前先核对

1. **确认分支与站点范围**：完整站点（MQTT 集群、`/compare/*`）在 `dev` 分支，`main` 只合入了一部分。先 `git ls docs` 看当前分支的实际文件，**不要按本文假设页面已存在**。
2. 读 [config.mts](file:///d:/code/NetAssistant/docs/.vitepress/config.mts)：`pageNames`、`nav`、`sidebar`、`llms.txt` 以当前分支实际内容为准。
3. **现取能力清单**：读 `docs/features.md` 与 `docs/en/features.md`，再回 `src/` 与 `locales/panels.yml` 核对默认值、上限与实际文案。本文**不保存**功能明细，不要凭记忆或旧页面写能力。
4. **对齐提交与文档**：文档通常滞后于提交。用 `git log --oneline -30`（或 `git log <上次文档更新/tag>..HEAD`）扫一遍最近提交，找出**代码里已有、但 `docs/features.md` 与最新 changelog 还没写**的用户可见能力，写页面时补进去。提交消息只当线索（可能只写 `fix`/`refactor`），结论以 diff 与 `src/` 为准。
5. 读同集群现有页面：功能、默认值、限制不编造；拿不准就标注「未验证」。

## 产品定位与硬边界

**一句话定位**：Rust 原生、约 20MB 内存、TCP/UDP + MQTT 一体的桌面调试工具，是 NetAssist、MQTTX、MQTT Explorer、MQTT.fx 的**轻量场景替代**，不是功能全集替代。

**功能清单不在本文维护副本**：协议、解码器、压测、工具箱分组等随版本变化，写页面时按右列重新读取当前分支，本文只保留定位与授权这类不易变的事实。

| 事实 | 每次写页面前重新读取 |
| ---- | ---- |
| 平台 | `.github/workflows/release.yml` 的实际构建矩阵 + `docs/features.md` |
| 协议 / 解码器 / 压测 / 工具箱 | `docs/features.md`（中英对照）；细节回 `src/`，如 `src/core/toolbox.rs`、`src/core/random.rs` |
| 默认值与上限 | `src/app.rs`、`src/core/*.rs` 中的常量与实际逻辑，不要抄旧页面 |
| 授权 | Apache-2.0，免费开源，无账号、无云上传（`Cargo.toml` / `LICENSE`） |

新增能力的入口是 `docs/changelog.md` 最新条目**和最近提交**（`git log`，changelog 未覆盖时以提交为准），再到 `src/` 与 `locales/panels.yml` 核实。

相关页面至少出现一次以下边界，不虚构；**这些边界同样会随版本变化，写前先用 `docs/features.md` 与最新 changelog 复核，已不成立的条目直接删掉，不要保留旧结论**：

- 不做串口、Unix Domain Socket、HTTP/WebSocket 客户端
- MQTT over WebSocket 不支持；无脚本引擎；主题树只统计当前会话出现过的 topic（不是 Broker 全量 retained 扫描），无数值曲线与 retained 批量清理
- MQTT 5.0 仅覆盖 Clean Start / 会话过期 / 接收最大值 / 原因码等基础特性，无用户属性、Will Delay、共享订阅、订阅标识符
- MQTT 不支持压测、逐行发送与定时任务（这些仅 TCP/UDP 提供）
- UDP 支持广播发送与多设备回复；**组播接收未实现**，不要写成「支持组播调试」
- 不以「完全替代 XX」作结论，只按场景说明可替代性

## 搜索意图与人群

| 人群 | 典型搜索 | 关心什么 | 转化点 |
| ---- | ---- | ---- | ---- |
| 嵌入式 / 物联网开发者 | tcp 调试工具、udp 调试工具、mqtt 调试工具 | 连得上、分得清、Hex 收发 | 工具页的下载链接 |
| 硬件 / 上位机工程师 | 网络调试助手、串口调试助手替代、modbus tcp 调试 | 免安装、低内存、Windows 现场 | 平台落地页 |
| 后端 / 平台开发者 | tcp 粘包、tcp 压测、qps p99、mqtt 5.0 客户端 | 解码器、压测指标、QoS | 教程页 / 压测页 |
| 现场 / 运维 / 技术支持 | tcp 连接失败、udp 收不到回复、mqtt 连接失败 | 排错步骤、便携、中文界面 | 排错页 |
| 替代品寻找者 | netassist 替代、mqttx 替代、packet sender 替代 | 平台、体积、是否收费、是否开源 | 对比页 |

## 页面地图（一页一意图）

同一主关键词只能有一个页面；新增前先查下表，能扩写就不要新建。下表是**目标结构**，`dev` 分支已基本实现；其它分支作业前先确认文件是否已合并。

### TCP/UDP 集群

| 页面 | 主查询词 | 次查询词 / 长尾 | 意图 | 备注 |
| ---- | ---- | ---- | ---- | ---- |
| `/tcp-debug-tool` | tcp 调试工具 | tcp 客户端、tcp 服务端、tcp 收发测试 | 交易型 | 解码器只做概览，细节指向粘包页与教程 |
| `/udp-debug-tool` | udp 调试工具 | udp 客户端、udp 服务端、udp 收发 | 交易型 | 广播只做入口，长尾归广播页 |
| `/socket-debug-tool` | socket 调试工具 | 端口监听、socket 客户端、端口通不通 | 交易型 | 与 TCP 页按「端口/字节」角度区分 |
| `/tcp-sticky-packet` | tcp 粘包 | tcp 拆包、长度前缀、分隔符、粘包解决方案 | 信息型 | 原理与四种方案的主页 |
| `/tcp-stress-test` | tcp 压测 | udp 压测、qps 测试、p99 延迟、并发测试工具 | 商业调查 | 指标与模式说明，教程细节归 `/guide/stress` |
| `/modbus-tcp` | modbus tcp 调试 | modbus 报文、crc16 modbus、mbap | 信息型 | 明确「不是 Modbus 协议客户端」 |
| `/hex-send` | 十六进制发送 | hex 收发、文本 hex 转换、二进制报文 | 信息型 | 十六进制编辑器的唯一入口 |
| `/udp-broadcast-device-discovery` | udp 广播设备发现 | 局域网设备发现、255.255.255.255、discovery | 信息型 | 广播地址与排查的主页 |
| `/guide/tcp-udp` | tcp/udp 调试教程 | 周期发送、自动回复、消息管理、ipv6 | 信息型 | 功能教程，不抢「粘包处理」主词 |
| `/comparison` | 网络调试工具对比 | tcp 调试工具对比、packet sender 对比 | 商业调查 | NetAssistant 细节归 `/compare/netassist` |
| `/compare/netassist` | netassist 替代品 | 网络调试助手、netassist mac | 商业调查 | 迁移与逐项对比 |

### MQTT 集群

| 页面 | 主查询词 | 备注 |
| ---- | ---- | ---- |
| `/mqtt` | mqtt 调试 | 任务型总览页，只做分流 |
| `/mqtt-client` | mqtt 客户端 | 能力与界面参考 |
| `/guide/mqtt` | mqtt 客户端 使用教程 | 连接、订阅、发布 |
| `/mqtt-troubleshooting` | mqtt 连接失败 | 现象 → 原因 → 步骤 |
| `/mqtt-tls` | mqtt tls | 证书校验、自签、双向认证 |
| `/mqtt-5` | mqtt 5.0 客户端 | 支持范围与 3.1.1 差异 |
| `/mqtt-testing` | mqtt 测试工具 | QoS、retain、遗嘱、通配符 |
| `/mqtt-faq` | mqtt 工具 常见问题 | 端口、Client ID、版本选择 |
| `/comparison-mqttx` | mqttx 替代 | 轻量替代边界 |
| `/compare/mqtt-explorer` | mqtt explorer 替代 | 会话主题树 vs Broker 全量主题树 |
| `/compare/mqtt-fx` | mqtt.fx 替代 | 商业授权 vs 免费开源 |
| `/mqtt-client-windows`、`/mqtt-client-mac`、`/mqtt-client-linux` | mqtt 客户端 + 平台 | 三个独立页面，安装为主，能力复用 `/mqtt-client` |

### 平台与核心页

| 页面 | 主查询词 | 备注 |
| ---- | ---- | ---- |
| `/` | 网络调试助手、网络调试工具 | 品牌词 + 头部词 |
| `/windows-network-debug-tool` | windows 网络调试助手 | 不抢「mqtt 客户端 windows」 |
| `/mac-network-debug-tool` | mac 网络调试助手 | 强调 Apple Silicon 原生 |
| `/linux-network-debug-tool` | linux 网络调试助手 | deb / AppImage / tar.gz |
| `/features` | 网络调试工具 功能 | 全能力清单 |
| `/download` | netassistant 下载 | 平台与包格式 |
| `/faq` | 网络调试助手 常见问题 | 平台、授权、与 NetAssist 区别 |
| `/compare/` | mqtt 客户端对比 | 四工具选型总览 |

**分工红线**：

- 「tcp 粘包」归 `/tcp-sticky-packet`；其它页只用 H2 带出并内链，title/description 不重复该主词
- 「udp 广播」归 `/udp-broadcast-device-discovery`；UDP 工具页 title 不写「广播」
- 「网络调试助手」归平台页与 `/compare/netassist`；`/comparison` 用「对比」类词
- 「mqtt 客户端 + 平台」归三个 MQTT 平台页；平台落地页的 MQTT 只写能力概述
- 一个关键词若已被覆盖，优先扩写该页，不新建 doorway page

英文镜像使用本地化关键词，不逐字翻译：`tcp debug tool`、`udp debug tool`、`tcp sticky packets`、`network debugging tool`、`mqtt client`、`mqttx alternative`、`netassist alternative`。

## 配置与自动机制

- `transformPageData` 自动生成 canonical、hreflang、OG/Twitter 与缺失的 JSON-LD；**不要手写** canonical / hreflang
- 页面一旦手写 `application/ld+json`，自动结构化数据会跳过该页；手写时 URL、面包屑、语言必须与当前路径一致
- `pageNames` 提供面包屑与 JSON-LD 名称；新增页面按集群登记。未登记时回退为页面 title（长 title 会让面包屑很难看）
- 新页面需同时处理：`pageNames`、对应 `sidebar`、必要的 `nav`、英文镜像、`docs/public/llms.txt`
- `llms.txt` 位于 `docs/public/llms.txt`（`dev` 分支已提供）；当前分支没有时先确认是否已合并，不要凭空新建
- 站点使用 `cleanUrls`，站内链接写无后缀路径；`sitemap` 由配置自动生成
- 面包屑层级必须与 nav/sidebar 实际归属一致，不能为了 SEO 虚构父级
- 截图放在仓库根目录 `assets/screenshots/zh|en`，页面内按相对深度写 `../assets/...` 或 `../../assets/...`，alt 含关键词

## Frontmatter 规范

```yaml
---
title: 主关键词 + 限定词（中文 ≤ 30 字，英文 ≤ 60 字符，不重复站名）
description: 主关键词 + 差异点 + 行动词（中文 70–110 字，英文 120–160 字符，每页唯一）
---
```

- `| NetAssistant` 由模板补全，不手写
- 现有部分页面标题偏长（30–48 字）属历史遗留；新增与改写页按上面的范围执行，不要照抄旧页
- `head` 中的 JSON-LD 只在页面有**可见** FAQ / 对比内容时添加，且与正文逐条、逐字一致
- 手写 JSON-LD 的 `url` / 面包屑必须带 `/en/` 前缀（英文页）或对应中文路径，平台与系统名与全站一致
- 更新日期由 `lastUpdated` 生成，不手写

## 内容结构（固定骨架）

1. **H1**：主关键词，全页唯一，可带一句价值主张
2. **首段 80–120 字 answer-first**：是什么、适合谁、和竞品差在哪；删掉行业空话
3. **适用 / 不适用清单**：3–5 条，直接写边界
4. **H2 用真实查询句式**：「怎么连不上」「收不到回复怎么办」「支持 IPv6 吗」
5. **操作步骤**：编号 + 界面截图，alt 含关键词
6. **表格**：参数、模式、方案对比；移动端列数 ≤ 5
7. **FAQ 5–8 条**：与 JSON-LD `FAQPage` 一一对应，答案 2–4 句
8. **CTA**：`/download` 为主，相关教程 / 对比页为辅；文末必须有下载链接
9. **相关页面**：3–6 条描述性锚文本，不用「点击这里」

## 分页型规则

- **工具落地页**（如 `/tcp-debug-tool`）：先给能力的三种使用方式，再给解码器 / 收发 / 自动化概览，每个能力链到专题页；不展开原理长文
- **教程页**（如 `/guide/tcp-udp`）：以操作顺序组织，截图与快捷键齐全；可重复功能细节，但不与工具页共用大段文案
- **排错页**（如 `/mqtt-troubleshooting`）：按现象分节，每节给「原因 → 判断方法 → 操作步骤」，明确哪些不是本工具能解决的
- **对比页**（如 `/compare/netassist`）：固定维度表（技术栈 / 平台 / 协议 / 体积内存 / 粘包处理 / 压测 / 自动化 / 开源 / 价格）；先写对方优势，再写适用场景，结论落到具体场景而非「全面替代」
- **平台页**（如 `/windows-network-debug-tool`）：安装方式优先，接着核心能力摘要、与同平台替代品差异、系统要求与 FAQ；协议细节链回专题页

## 写作规则

- 主关键词自然出现在 title、description、H1、首段、至少 2 个 H2、FAQ；不堆砌
- 英文页重写而非直译，使用英文搜索习惯的术语与句式
- 不复制其他页面段落；跨页重复内容会形成 doorway page，宁可内链
- 示例报文、命令、截图路径必须真实可执行；代码块标语言；命令区分 Windows / macOS / Linux
- 不改写用户配置文本、报文与解码器语义
- 每页至少 3 条内链，锚文本描述具体内容

## 事实一致性清单

写 TCP/UDP 相关内容时对照（**具体数值同样以 `src/` 为准，写前复核，过期就改这一条而不是照抄旧页**）：

- 解码器名称统一为「原始数据 / 基于行 / 长度前缀 / JSON」
- 文本 ⇄ Hex 按 UTF-8 双向转换，不可打印字节用 `\xNN` 转义；文件导入上限 1 MiB，编码 UTF-8 / GBK / ANSI
- 消息保留默认 10000 条，`0` 为不限制；关闭自动滚动时冻结列表
- 本地地址仅支持 IP，不支持域名；本地地址族需与远端一致
- 压力测试仅 TCP/UDP；MQTT 页面不得暗示可压测
- UDP 广播默认不跨路由器；组播接收未实现
- 不支持的协议（串口、Unix Socket、HTTP、WebSocket）用 FAQ 明确否定，避免错误流量

## 交付检查清单

- [ ] title / description 唯一，含主关键词，长度符合上文
- [ ] H1 唯一，与 title 不重复堆词
- [ ] 新页已登记 `pageNames`，并加入 nav 或 sidebar
- [ ] 面包屑 / JSON-LD URL 与实际路径、语言一致
- [ ] JSON-LD FAQ 条数与可见 FAQ 完全一致
- [ ] 内链 ≥ 3，含 `/download` CTA，且从集群主页面两步可达
- [ ] 英文镜像存在，路径为 `en/` + 中文路径，关键词本地化
- [ ] `llms.txt` 已补新页（仅当当前分支存在该文件）
- [ ] 表格在移动端不横向溢出
- [ ] `npm run docs:build` 通过
- [ ] 无关键词堆砌、无跨页重复段落、无编造功能

## 交付前自检

1. 用主关键词在站内搜索（VitePress local search），确认没有第二个页面抢同一意图
2. 从集群主页面能否两步内到达新页？不能就补内链
3. 首屏是否能在 5 秒内回答「这是什么、支不支持、怎么下载」
4. 是否明确写出「不支持的场景」，避免用户下载后落差
