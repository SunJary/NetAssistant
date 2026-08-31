import { defineConfig } from 'vitepress'

// https://vitepress.dev/zh/reference/site-config
export default defineConfig({
  lang: 'zh-CN',
  title: 'NetAssistant',
  description:
    '基于 Rust 构建的高性能跨平台网络调试工具，支持 TCP/UDP 客户端与服务端、多种解码器、消息管理与高并发压力测试。',
  base: '/NetAssistant/',
  head: [['link', { rel: 'icon', type: 'image/png', href: '/NetAssistant/logo.png' }]],
  themeConfig: {
    logo: '/logo.png',
    nav: [
      { text: '首页', link: '/' },
      { text: '功能特性', link: '/features' },
      { text: '使用指南', link: '/guide/' },
      { text: '下载', link: '/download' },
      { text: '更新日志', link: '/changelog' }
    ],
    sidebar: {
      '/guide/': [
        {
          text: '使用指南',
          items: [
            { text: '快速上手', link: '/guide/' },
            { text: 'TCP/UDP 调试', link: '/guide/tcp-udp' },
            { text: '压力测试', link: '/guide/stress' }
          ]
        }
      ]
    },
    socialLinks: [
      { icon: 'github', link: 'https://github.com/SunJary/NetAssistant' }
    ],
    search: {
      provider: 'local',
      options: {
        translations: {
          button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
          modal: {
            noResultsText: '未找到相关结果',
            resetButtonTitle: '清除查询条件',
            footer: { selectText: '选择', navigateText: '切换', closeText: '关闭' }
          }
        }
      }
    },
    outline: { level: [2, 3] },
    footer: {
      message: '基于 Apache-2.0 许可证发布',
      copyright: 'Copyright © 2026 SunJary'
    }
  }
})
