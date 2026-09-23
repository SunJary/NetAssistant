import { defineConfig } from 'vitepress'

// 默认走 GitHub Pages 子路径；Cloudflare Pages 构建时设置 VITEPRESS_BASE=/ 覆盖
const base = process.env.VITEPRESS_BASE || '/NetAssistant/'

const siteUrl = 'https://netassistant.trydo.top'

// https://vitepress.dev/zh/reference/site-config
export default defineConfig({
  title: 'NetAssistant',
  base,
  // 基于 git 提交时间生成页面更新时间，供主题展示与 sitemap <lastmod> 使用
  lastUpdated: true,
  // 正式域名（Cloudflare Pages）会把 *.html 308 重定向到无后缀地址，GitHub Pages 两种都可访问；
  // 统一使用无后缀地址，避免站内链接、sitemap 与 canonical 出现「canonical 指向重定向」的冲突信号
  cleanUrls: true,
  sitemap: {
    hostname: siteUrl
  },
  head: [
    ['link', { rel: 'icon', type: 'image/png', href: `${base}logo.png` }],
    ['meta', { property: 'og:site_name', content: 'NetAssistant' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'NetAssistant - 开源跨平台网络调试助手' }],
    ['meta', { property: 'og:description', content: '基于 Rust 构建的高性能跨平台网络调试工具，支持 Windows、Linux、macOS（x64 / ARM64），涵盖 TCP/UDP 客户端与服务端、多种解码器、消息管理与高并发压力测试。' }],
    ['meta', { property: 'og:image', content: `${siteUrl}/logo.png` }],
    ['meta', { name: 'twitter:card', content: 'summary' }],
    ['meta', { name: 'twitter:title', content: 'NetAssistant - 开源跨平台网络调试助手' }],
    ['meta', { name: 'twitter:description', content: '基于 Rust 构建的高性能跨平台网络调试工具，支持 Windows、Linux、macOS（x64 / ARM64），涵盖 TCP/UDP 客户端与服务端、多种解码器、消息管理与高并发压力测试。' }],
    ['meta', { name: 'twitter:image', content: `${siteUrl}/logo.png` }]
  ],
  // 为每个页面生成指向唯一规范域名的 canonical，避免 GitHub Pages 与正式域名重复内容
  transformPageData(pageData) {
    const canonicalUrl = `${siteUrl}/${pageData.relativePath}`
      .replace(/index\.md$/, '')
      .replace(/\.md$/, '')
    pageData.frontmatter.head ??= []
    pageData.frontmatter.head.push(['link', { rel: 'canonical', href: canonicalUrl }])
  },
  locales: {
    root: {
      label: '简体中文',
      lang: 'zh-CN',
      description:
        '基于 Rust 构建的高性能跨平台网络调试工具，支持 Windows、Linux、macOS（x64 / ARM64），涵盖 TCP/UDP 客户端与服务端、多种解码器、消息管理与高并发压力测试。',
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: '首页', link: '/' },
          { text: '功能特性', link: '/features' },
          { text: '同类对比', link: '/comparison' },
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
        lastUpdated: { text: '最后更新于' },
        footer: {
          message: '基于 Apache-2.0 许可证发布',
          copyright: 'Copyright © 2026 SunJary'
        }
      }
    },
    en: {
      label: 'English',
      lang: 'en-US',
      link: '/en/',
      description:
        'A high-performance cross-platform network debugging tool built with Rust, available for Windows, Linux and macOS on x64 and ARM64, featuring TCP/UDP client & server, multiple decoders, message management and high-concurrency stress testing.',
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: 'Home', link: '/en/' },
          { text: 'Features', link: '/en/features' },
          { text: 'Comparison', link: '/en/comparison' },
          { text: 'Guide', link: '/en/guide/' },
          { text: 'Download', link: '/en/download' },
          { text: 'Changelog', link: '/en/changelog' }
        ],
        sidebar: {
          '/en/guide/': [
            {
              text: 'Guide',
              items: [
                { text: 'Getting Started', link: '/en/guide/' },
                { text: 'TCP/UDP Debugging', link: '/en/guide/tcp-udp' },
                { text: 'Stress Testing', link: '/en/guide/stress' }
              ]
            }
          ]
        },
        socialLinks: [
          { icon: 'github', link: 'https://github.com/SunJary/NetAssistant' }
        ],
        search: {
          provider: 'local'
        },
        outline: { level: [2, 3] },
        lastUpdated: { text: 'Last updated' },
        footer: {
          message: 'Released under the Apache-2.0 License',
          copyright: 'Copyright © 2026 SunJary'
        }
      }
    }
  }
})
