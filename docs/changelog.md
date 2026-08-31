# 更新日志

所有正式版本的更新记录发布在 [GitHub Releases](https://github.com/sunjary/netassistant/releases) 页面。

## v1.0.0

首个正式版本。核心能力：

- TCP/UDP 客户端与服务端，IPv4/IPv6 双栈
- 四种 TCP 解码器（原始 / 行分隔 / 长度前缀 / JSON），解决粘包问题
- 聊天式报文记录，消息收藏与备注、关键字搜索
- 消息导出（TXT/JSON/CSV）与实时日志记录
- 自动回复、周期发送
- UDP 广播回复智能展示，助力物联网设备发现调试
- 内置 TCP/UDP 高并发压力测试引擎（QPS、延迟分位数、失败原因分类、CSV 报告）
- 暗黑模式、多标签页、多语言界面
- 十六进制编辑器

完整变更明细请见 [GitHub Releases](https://github.com/sunjary/netassistant/releases)。

## 路线图

- [ ] 文件数据源
- [ ] 多语言支持
- [ ] 支持 SSE 调试
- [ ] 支持更多数据格式的编解码
- [ ] 支持 WebSocket 协议
