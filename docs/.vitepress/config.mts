import { defineConfig } from 'vitepress'

// 默认走 GitHub Pages 子路径；Cloudflare Pages 构建时设置 VITEPRESS_BASE=/ 覆盖
const base = process.env.VITEPRESS_BASE || '/NetAssistant/'

const siteUrl = 'https://netassistant.trydo.top'
const ogImage = `${siteUrl}/og-cover.png`

const defaultDescription = {
  zh: 'NetAssistant 是基于 Rust 的开源跨平台网络调试工具：TCP/UDP 客户端与服务端、MQTT 3.1.1、内置工具箱、四种解码器与高并发压测，支持 Windows、Linux、macOS。',
  en: 'NetAssistant is an open-source Rust network debugging tool: TCP/UDP client & server, MQTT 3.1.1, built-in toolbox, four decoders and stress testing for Windows, Linux and macOS.'
}

// 面包屑与结构化数据所需的页面名称；key 为去掉 .md 与 en/ 前缀后的路径
const pageNames: Record<string, { zh: string; en: string }> = {
  features: { zh: '功能特性', en: 'Features' },
  download: { zh: '下载', en: 'Download' },
  faq: { zh: '常见问题', en: 'FAQ' },
  comparison: { zh: '同类工具对比', en: 'Comparison' },
  'comparison-mqttx': { zh: 'vs MQTTX', en: 'vs MQTTX' },
  changelog: { zh: '更新日志', en: 'Changelog' },
  guide: { zh: '使用指南', en: 'Guide' },
  'guide/index': { zh: '使用指南', en: 'Guide' },
  'guide/tcp-udp': { zh: 'TCP/UDP 调试', en: 'TCP/UDP Debugging' },
  'guide/stress': { zh: '压力测试', en: 'Stress Testing' }
}

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
    ['meta', { name: 'baidu-site-verification', content: 'codeva-HEUClUqDkM' }]
  ],
  // 为每个页面生成 canonical、hreflang、OG/Twitter 元数据与缺失的结构化数据
  transformPageData(pageData) {
    const { relativePath } = pageData
    const toUrl = (path) =>
      `${siteUrl}/${path}`.replace(/index\.md$/, '').replace(/\.md$/, '')
    const canonicalUrl = toUrl(relativePath)
    const isEn = relativePath.startsWith('en/')
    // 英文页去掉 en/ 前缀得到对应中文页路径
    const zhPath = relativePath.replace(/^en\//, '')
    const zhUrl = toUrl(zhPath)
    const enUrl = toUrl(`en/${zhPath}`)
    const pathKey = zhPath.replace(/\.md$/, '')
    const isHome = pathKey === 'index'
    const rawTitle = pageData.frontmatter.title || pageData.title || 'NetAssistant'
    const ogTitle = rawTitle.includes('NetAssistant') ? rawTitle : `${rawTitle} | NetAssistant`
    const description =
      pageData.frontmatter.description || (isEn ? defaultDescription.en : defaultDescription.zh)
    const inLanguage = isEn ? 'en-US' : 'zh-CN'

    pageData.frontmatter.head ??= []
    pageData.frontmatter.head.push(
      ['link', { rel: 'canonical', href: canonicalUrl }],
      ['link', { rel: 'alternate', hreflang: 'zh-CN', href: zhUrl }],
      ['link', { rel: 'alternate', hreflang: 'en-US', href: enUrl }],
      ['link', { rel: 'alternate', hreflang: 'x-default', href: zhUrl }],
      ['meta', { property: 'og:title', content: ogTitle }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: canonicalUrl }],
      ['meta', { property: 'og:locale', content: isEn ? 'en_US' : 'zh_CN' }],
      ['meta', { property: 'og:locale:alternate', content: isEn ? 'zh_CN' : 'en_US' }],
      ['meta', { property: 'og:image', content: ogImage }],
      ['meta', { property: 'og:image:width', content: '1200' }],
      ['meta', { property: 'og:image:height', content: '630' }],
      ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
      ['meta', { name: 'twitter:title', content: ogTitle }],
      ['meta', { name: 'twitter:description', content: description }],
      ['meta', { name: 'twitter:image', content: ogImage }]
    )

    // 已有手写 JSON-LD 的页面跳过自动生成，避免同一页面重复声明同一类型
    const hasJsonLd = JSON.stringify(pageData.frontmatter.head).includes('application/ld+json')
    if (isHome) {
      pageData.frontmatter.head.push([
        'script',
        { type: 'application/ld+json' },
        JSON.stringify({
          '@context': 'https://schema.org',
          '@graph': [
            {
              '@type': 'WebSite',
              name: 'NetAssistant',
              url: `${siteUrl}/`,
              description,
              inLanguage
            },
            {
              '@type': 'SoftwareApplication',
              name: 'NetAssistant',
              url: `${siteUrl}/download`,
              description,
              applicationCategory: 'DeveloperApplication',
              operatingSystem: 'Windows, macOS, Linux',
              inLanguage,
              offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }
            }
          ]
        })
      ])
    } else if (!hasJsonLd) {
      const graph: Record<string, unknown>[] = []
      if (pathKey === 'features' || pathKey === 'download') {
        graph.push({
          '@type': 'SoftwareApplication',
          name: 'NetAssistant',
          url: canonicalUrl,
          description,
          applicationCategory: 'DeveloperApplication',
          operatingSystem: 'Windows, macOS, Linux',
          inLanguage,
          offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }
        })
      } else {
        graph.push({
          '@type': 'TechArticle',
          headline: rawTitle,
          description,
          url: canonicalUrl,
          inLanguage,
          isPartOf: { '@type': 'WebSite', name: 'NetAssistant', url: `${siteUrl}/` }
        })
      }

      const items: Record<string, unknown>[] = [
        {
          '@type': 'ListItem',
          position: 1,
          name: isEn ? 'Home' : '首页',
          item: isEn ? `${siteUrl}/en/` : `${siteUrl}/`
        }
      ]
      const nameOf = (key: string) =>
        (pageNames[key] ?? { zh: rawTitle, en: rawTitle })[isEn ? 'en' : 'zh']
      if (pathKey.startsWith('guide/')) {
        items.push({
          '@type': 'ListItem',
          position: 2,
          name: isEn ? 'Guide' : '使用指南',
          item: `${siteUrl}${isEn ? '/en' : ''}/guide/`
        })
        items.push({ '@type': 'ListItem', position: 3, name: nameOf(pathKey), item: canonicalUrl })
      } else if (pathKey !== 'guide') {
        items.push({ '@type': 'ListItem', position: 2, name: nameOf(pathKey), item: canonicalUrl })
      }
      if (items.length > 1) {
        graph.push({ '@type': 'BreadcrumbList', itemListElement: items })
      }

      pageData.frontmatter.head.push([
        'script',
        { type: 'application/ld+json' },
        JSON.stringify({ '@context': 'https://schema.org', '@graph': graph })
      ])
    }
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
          {
            text: '平台',
            items: [
              { text: 'Windows 网络调试助手', link: '/windows-network-debug-tool' },
              { text: 'macOS 网络调试助手', link: '/mac-network-debug-tool' },
              { text: 'Linux 网络调试助手', link: '/linux-network-debug-tool' }
            ]
          },
          {
            text: '功能',
            items: [
              { text: '全部功能', link: '/features' },
              { text: 'TCP 调试工具', link: '/tcp-debug-tool' },
              { text: 'UDP 调试工具', link: '/udp-debug-tool' },
              { text: 'Socket 调试工具', link: '/socket-debug-tool' },
              { text: '十六进制发送', link: '/hex-send' },
              { text: 'Modbus TCP 调试', link: '/modbus-tcp' },
              { text: 'MQTT 客户端', link: '/mqtt-client' },
              { text: 'TCP/UDP 压测', link: '/tcp-stress-test' },
              { text: 'UDP 广播设备发现', link: '/udp-broadcast-device-discovery' }
            ]
          },
          {
            text: '对比',
            items: [
              { text: 'vs NetAssist（网络调试助手）', link: '/compare/netassist' },
              { text: '同类工具对比', link: '/comparison' },
              { text: 'vs MQTTX', link: '/comparison-mqttx' }
            ]
          },
          { text: '常见问题', link: '/faq' },
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
          {
            text: 'Platforms',
            items: [
              { text: 'Windows Network Debugging Tool', link: '/en/windows-network-debug-tool' },
              { text: 'Mac Network Debugging Tool', link: '/en/mac-network-debug-tool' },
              { text: 'Linux Network Debugging Tool', link: '/en/linux-network-debug-tool' }
            ]
          },
          {
            text: 'Features',
            items: [
              { text: 'All features', link: '/en/features' },
              { text: 'TCP debug tool', link: '/en/tcp-debug-tool' },
              { text: 'UDP debug tool', link: '/en/udp-debug-tool' },
              { text: 'Socket debug tool', link: '/en/socket-debug-tool' },
              { text: 'Hex send', link: '/en/hex-send' },
              { text: 'Modbus TCP debugging', link: '/en/modbus-tcp' },
              { text: 'MQTT client', link: '/en/mqtt-client' },
              { text: 'TCP/UDP stress testing', link: '/en/tcp-stress-test' },
              { text: 'UDP broadcast discovery', link: '/en/udp-broadcast-device-discovery' }
            ]
          },
          {
            text: 'Compare',
            items: [
              { text: 'vs NetAssist', link: '/en/compare/netassist' },
              { text: 'Network Debugging Tools', link: '/en/comparison' },
              { text: 'vs MQTTX', link: '/en/comparison-mqttx' }
            ]
          },
          { text: 'FAQ', link: '/en/faq' },
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
