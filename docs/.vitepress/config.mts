import fs from 'node:fs'
import path from 'node:path'
import { defineConfig } from 'vitepress'

// 默认走 GitHub Pages 子路径；Cloudflare Pages 构建时设置 VITEPRESS_BASE=/ 覆盖
const base = process.env.VITEPRESS_BASE || '/NetAssistant/'

const siteUrl = 'https://netassistant.trydo.top'
const ogImage = `${siteUrl}/og-cover.png`

const defaultDescription = {
  zh: 'NetAssistant 是基于 Rust 的开源跨平台网络调试工具：TCP/UDP 客户端与服务端、MQTT 3.1.1 / 5.0、内置工具箱、四种解码器与 TCP/UDP 高并发压测，支持 Windows、Linux、macOS。',
  en: 'NetAssistant is an open-source Rust network debugging tool: TCP/UDP client & server, MQTT 3.1.1 / 5.0, built-in toolbox, four decoders and TCP/UDP stress testing for Windows, Linux and macOS.'
}

// 面包屑与结构化数据所需的页面名称；key 为去掉 .md 与 en/ 前缀后的路径
const pageNames: Record<string, { zh: string; en: string }> = {
  features: { zh: '功能特性', en: 'Features' },
  download: { zh: '下载', en: 'Download' },
  faq: { zh: '常见问题', en: 'FAQ' },
  comparison: { zh: '网络调试工具对比', en: 'Network Debugging Tool Comparison' },
  'comparison-mqttx': { zh: 'vs MQTTX', en: 'vs MQTTX' },
  changelog: { zh: '更新日志', en: 'Changelog' },
  'guide/index': { zh: '使用指南', en: 'Guide' },
  'guide/tcp-udp': { zh: 'TCP/UDP 调试', en: 'TCP/UDP Debugging' },
  'guide/mqtt': { zh: 'MQTT 调试', en: 'MQTT Debugging' },
  'mqtt-client': { zh: 'MQTT 客户端', en: 'MQTT Client' },
  'mqtt-troubleshooting': { zh: 'MQTT 排错', en: 'MQTT Troubleshooting' },
  'mqtt-tls': { zh: 'MQTT TLS', en: 'MQTT TLS' },
  'mqtt-5': { zh: 'MQTT 5.0', en: 'MQTT 5.0' },
  'mqtt-testing': { zh: 'MQTT 测试场景', en: 'MQTT Testing' },
  'compare/mqtt-explorer': { zh: 'MQTT Explorer 对比', en: 'MQTT Explorer Comparison' },
  'mqtt-client-windows': { zh: 'Windows MQTT 客户端', en: 'MQTT Client for Windows' },
  'mqtt-client-mac': { zh: 'macOS MQTT 客户端', en: 'MQTT Client for macOS' },
  'mqtt-client-linux': { zh: 'Linux MQTT 客户端', en: 'MQTT Client for Linux' },
  'compare/mqtt-fx': { zh: 'MQTT.fx 对比', en: 'MQTT.fx Comparison' },
  'mqtt-faq': { zh: 'MQTT 常见问题', en: 'MQTT FAQ' },
  'mqtt': { zh: 'MQTT 调试指南', en: 'MQTT Debugging Guide' },
  'compare/index': { zh: 'MQTT 客户端对比', en: 'MQTT Client Comparison' },
  'compare/netassist': { zh: 'NetAssist 对比', en: 'NetAssist Comparison' },
  'guide/stress': { zh: '压力测试', en: 'Stress Testing' },
  'tcp-debug-tool': { zh: 'TCP 调试工具', en: 'TCP Debug Tool' },
  'udp-debug-tool': { zh: 'UDP 调试工具', en: 'UDP Debug Tool' },
  'socket-debug-tool': { zh: 'Socket 调试工具', en: 'Socket Debug Tool' },
  'tcp-sticky-packet': { zh: 'TCP 粘包处理', en: 'TCP Sticky Packets' },
  'tcp-stress-test': { zh: 'TCP/UDP 压力测试', en: 'TCP/UDP Stress Testing' },
  'modbus-tcp': { zh: 'Modbus TCP 调试', en: 'Modbus TCP Debugging' },
  'hex-send': { zh: '十六进制发送', en: 'Hex Send' },
  'udp-broadcast-device-discovery': { zh: 'UDP 广播设备发现', en: 'UDP Broadcast Discovery' },
  'windows-network-debug-tool': { zh: 'Windows 网络调试助手', en: 'Windows Network Debugging Tool' },
  'mac-network-debug-tool': { zh: 'macOS 网络调试助手', en: 'macOS Network Debugging Tool' },
  'linux-network-debug-tool': { zh: 'Linux 网络调试助手', en: 'Linux Network Debugging Tool' }
}

// 面包屑父级：值为父页面的 pathKey（去掉 .md），未列出的页面直接挂在首页下
const breadcrumbParents: Record<string, string> = {
  // TCP/UDP 工具页统一挂到调试教程
  'tcp-debug-tool': 'guide/tcp-udp',
  'udp-debug-tool': 'guide/tcp-udp',
  'socket-debug-tool': 'guide/tcp-udp',
  'hex-send': 'guide/tcp-udp',
  'modbus-tcp': 'guide/tcp-udp',
  'tcp-sticky-packet': 'guide/tcp-udp',
  'tcp-stress-test': 'guide/tcp-udp',
  'udp-broadcast-device-discovery': 'guide/tcp-udp',
  // MQTT 内容页统一挂到 MQTT 枢纽页
  'mqtt-client': 'mqtt',
  'mqtt-5': 'mqtt',
  'mqtt-tls': 'mqtt',
  'mqtt-testing': 'mqtt',
  'mqtt-faq': 'mqtt',
  'mqtt-troubleshooting': 'mqtt',
  'mqtt-client-windows': 'mqtt',
  'mqtt-client-mac': 'mqtt',
  'mqtt-client-linux': 'mqtt',
  // 平台页挂到下载页
  'windows-network-debug-tool': 'download',
  'mac-network-debug-tool': 'download',
  'linux-network-debug-tool': 'download',
  // 对比页各归其类
  'compare/mqtt-explorer': 'compare/index',
  'compare/mqtt-fx': 'compare/index',
  'comparison-mqttx': 'compare/index',
  'compare/netassist': 'comparison'
}

// MQTT 相关内容共用同一套侧栏，避免教程页与客户端页菜单不一致
const mqttSidebarZh = [
  {
    text: 'MQTT 调试',
    items: [
      { text: 'MQTT 总览', link: '/mqtt' },
      { text: 'MQTT 客户端', link: '/mqtt-client' },
      { text: '调试教程', link: '/guide/mqtt' },
      { text: '连接与订阅排错', link: '/mqtt-troubleshooting' },
      { text: 'TLS 与双向认证', link: '/mqtt-tls' },
      { text: 'MQTT 5.0', link: '/mqtt-5' },
      { text: '测试场景', link: '/mqtt-testing' }
    ]
  },
  {
    text: '对比与选型',
    items: [
      { text: 'MQTTX 对比', link: '/comparison-mqttx' },
      { text: 'MQTT Explorer 对比', link: '/compare/mqtt-explorer' },
      { text: 'MQTT.fx 对比', link: '/compare/mqtt-fx' }
    ]
  },
  {
    text: '平台与常见问题',
    items: [
      { text: 'Windows 版', link: '/mqtt-client-windows' },
      { text: 'macOS 版', link: '/mqtt-client-mac' },
      { text: 'Linux 版', link: '/mqtt-client-linux' },
      { text: '常见问题', link: '/mqtt-faq' }
    ]
  }
]

const mqttSidebarEn = [
  {
    text: 'MQTT Debugging',
    items: [
      { text: 'MQTT overview', link: '/en/mqtt' },
      { text: 'MQTT client', link: '/en/mqtt-client' },
      { text: 'Tutorial', link: '/en/guide/mqtt' },
      { text: 'Troubleshooting', link: '/en/mqtt-troubleshooting' },
      { text: 'TLS & mutual auth', link: '/en/mqtt-tls' },
      { text: 'MQTT 5.0', link: '/en/mqtt-5' },
      { text: 'Testing scenarios', link: '/en/mqtt-testing' }
    ]
  },
  {
    text: 'Comparison',
    items: [
      { text: 'vs MQTTX', link: '/en/comparison-mqttx' },
      { text: 'vs MQTT Explorer', link: '/en/compare/mqtt-explorer' },
      { text: 'vs MQTT.fx', link: '/en/compare/mqtt-fx' }
    ]
  },
  {
    text: 'Platforms & FAQ',
    items: [
      { text: 'Windows client', link: '/en/mqtt-client-windows' },
      { text: 'macOS client', link: '/en/mqtt-client-mac' },
      { text: 'Linux client', link: '/en/mqtt-client-linux' },
      { text: 'FAQ', link: '/en/mqtt-faq' }
    ]
  }
]

const compareSidebarZh = [
  {
    text: 'MQTT 客户端',
    items: [
      { text: 'MQTT 客户端对比', link: '/compare/' },
      { text: 'vs MQTTX', link: '/comparison-mqttx' },
      { text: 'vs MQTT Explorer', link: '/compare/mqtt-explorer' },
      { text: 'vs MQTT.fx', link: '/compare/mqtt-fx' }
    ]
  },
  {
    text: 'TCP/UDP 工具',
    items: [
      { text: '网络调试工具对比', link: '/comparison' },
      { text: 'vs NetAssist', link: '/compare/netassist' }
    ]
  }
]

const compareSidebarEn = [
  {
    text: 'MQTT clients',
    items: [
      { text: 'MQTT client comparison', link: '/en/compare/' },
      { text: 'vs MQTTX', link: '/en/comparison-mqttx' },
      { text: 'vs MQTT Explorer', link: '/en/compare/mqtt-explorer' },
      { text: 'vs MQTT.fx', link: '/en/compare/mqtt-fx' }
    ]
  },
  {
    text: 'TCP/UDP tools',
    items: [
      { text: 'Network debugging tool comparison', link: '/en/comparison' },
      { text: 'vs NetAssist', link: '/en/compare/netassist' }
    ]
  }
]

const platformSidebarZh = [
  {
    text: '平台支持',
    items: [
      { text: 'Windows 版', link: '/windows-network-debug-tool' },
      { text: 'macOS 版', link: '/mac-network-debug-tool' },
      { text: 'Linux 版', link: '/linux-network-debug-tool' },
      { text: '下载', link: '/download' }
    ]
  }
]

const platformSidebarEn = [
  {
    text: 'Platforms',
    items: [
      { text: 'Windows', link: '/en/windows-network-debug-tool' },
      { text: 'macOS', link: '/en/mac-network-debug-tool' },
      { text: 'Linux', link: '/en/linux-network-debug-tool' },
      { text: 'Download', link: '/en/download' }
    ]
  }
]

const tcpSidebarZh = [
  {
    text: 'TCP/UDP 调试',
    items: [
      { text: 'TCP 调试工具', link: '/tcp-debug-tool' },
      { text: 'UDP 调试工具', link: '/udp-debug-tool' },
      { text: 'Socket 调试工具', link: '/socket-debug-tool' },
      { text: 'TCP 粘包处理', link: '/tcp-sticky-packet' },
      { text: 'TCP/UDP 压测', link: '/tcp-stress-test' },
      { text: 'Modbus TCP 调试', link: '/modbus-tcp' },
      { text: '十六进制发送', link: '/hex-send' },
      { text: 'UDP 广播设备发现', link: '/udp-broadcast-device-discovery' }
    ]
  },
  {
    text: '学习与总览',
    items: [
      { text: '调试教程', link: '/guide/tcp-udp' },
      { text: '全部功能', link: '/features' }
    ]
  }
]

const tcpSidebarEn = [
  {
    text: 'TCP/UDP Debugging',
    items: [
      { text: 'TCP debug tool', link: '/en/tcp-debug-tool' },
      { text: 'UDP debug tool', link: '/en/udp-debug-tool' },
      { text: 'Socket debug tool', link: '/en/socket-debug-tool' },
      { text: 'TCP sticky packets', link: '/en/tcp-sticky-packet' },
      { text: 'TCP/UDP stress testing', link: '/en/tcp-stress-test' },
      { text: 'Modbus TCP debugging', link: '/en/modbus-tcp' },
      { text: 'Hex send', link: '/en/hex-send' },
      { text: 'UDP broadcast discovery', link: '/en/udp-broadcast-device-discovery' }
    ]
  },
  {
    text: 'Learn',
    items: [
      { text: 'Tutorial', link: '/en/guide/tcp-udp' },
      { text: 'All features', link: '/en/features' }
    ]
  }
]

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
  transformPageData(pageData, ctx) {
    const { relativePath } = pageData
    const toUrl = (p: string) =>
      `${siteUrl}/${p}`.replace(/index\.md$/, '').replace(/\.md$/, '')
    // 对应语言页面是否存在，避免 hreflang 指向不存在的地址
    const pageExists = (p: string) => fs.existsSync(path.join(ctx.siteConfig.srcDir, p))
    const canonicalUrl = toUrl(relativePath)
    const isEn = relativePath.startsWith('en/')
    // 英文页去掉 en/ 前缀得到对应中文页路径
    const zhPath = relativePath.replace(/^en\//, '')
    const zhUrl = toUrl(zhPath)
    const enUrl = toUrl(`en/${zhPath}`)
    const pathKey = zhPath.replace(/\.md$/, '')
    const isHome = pathKey === 'index'
    const rawTitle = pageData.frontmatter.title || pageData.title || 'NetAssistant'
    // 与 VitePress 内部 createTitle 使用同一套规则，避免 <title> 与 og:title 不一致
    const titleTemplate = pageData.frontmatter.titleTemplate
    const pageTitle =
      typeof titleTemplate === 'string' && titleTemplate.includes(':title')
        ? titleTemplate.replace(/:title/g, rawTitle)
        : titleTemplate === false
          ? rawTitle
          : `${rawTitle} | NetAssistant`
    const description =
      pageData.frontmatter.description || (isEn ? defaultDescription.en : defaultDescription.zh)
    const inLanguage = isEn ? 'en-US' : 'zh-CN'

    const zhPageExists = pageExists(zhPath)
    const enPageExists = pageExists(`en/${zhPath}`)
    const alternates: any[] = []
    if (zhPageExists) {
      alternates.push(['link', { rel: 'alternate', hreflang: 'zh-CN', href: zhUrl }])
    }
    if (enPageExists) {
      alternates.push(['link', { rel: 'alternate', hreflang: 'en-US', href: enUrl }])
    }
    alternates.push([
      'link',
      { rel: 'alternate', hreflang: 'x-default', href: zhPageExists ? zhUrl : enUrl }
    ])

    pageData.frontmatter.head ??= []
    pageData.frontmatter.head.push(
      ['link', { rel: 'canonical', href: canonicalUrl }],
      ...alternates,
      ['meta', { property: 'og:title', content: pageTitle }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: canonicalUrl }],
      ['meta', { property: 'og:locale', content: isEn ? 'en_US' : 'zh_CN' }],
      ['meta', { property: 'og:locale:alternate', content: isEn ? 'zh_CN' : 'en_US' }],
      ['meta', { property: 'og:image', content: ogImage }],
      ['meta', { property: 'og:image:width', content: '1200' }],
      ['meta', { property: 'og:image:height', content: '630' }],
      ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
      ['meta', { name: 'twitter:title', content: pageTitle }],
      ['meta', { name: 'twitter:description', content: description }],
      ['meta', { name: 'twitter:image', content: ogImage }]
    )

    // 按类型补齐：手写 FAQPage 不再导致面包屑与文章结构化数据被整体跳过
    const manualJson = JSON.stringify(pageData.frontmatter.head)
    const hasJsonLd = manualJson.includes('application/ld+json')
    const hasBreadcrumb = manualJson.includes('"BreadcrumbList"')
    const hasArticleOrApp = /"(TechArticle|Article|BlogPosting|SoftwareApplication)"/.test(manualJson)
    if (isHome) {
      // 首页手写优先，避免 SoftwareApplication 被声明两次
      if (!hasJsonLd) {
        pageData.frontmatter.head.push([
          'script',
          { type: 'application/ld+json' },
          JSON.stringify({
            '@context': 'https://schema.org',
            '@graph': [
              {
                '@type': 'WebSite',
                '@id': `${siteUrl}/#website`,
                name: 'NetAssistant',
                url: `${siteUrl}/`,
                description,
                inLanguage
              },
              {
                '@type': 'SoftwareApplication',
                '@id': `${siteUrl}/#software`,
                name: 'NetAssistant',
                url: `${siteUrl}/`,
                downloadUrl: 'https://github.com/SunJary/NetAssistant/releases',
                sameAs: 'https://github.com/SunJary/NetAssistant',
                license: 'https://www.apache.org/licenses/LICENSE-2.0',
                description,
                applicationCategory: 'DeveloperApplication',
                operatingSystem: 'Windows, macOS, Linux',
                inLanguage,
                offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }
              }
            ]
          })
        ])
      }
    } else {
      const graph: Record<string, unknown>[] = []
      if (!hasArticleOrApp) {
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
      }

      if (!hasBreadcrumb) {
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
        const parentKey = breadcrumbParents[pathKey]
        if (parentKey) {
          items.push({
            '@type': 'ListItem',
            position: 2,
            name: nameOf(parentKey),
            item: toUrl(isEn ? `en/${parentKey}.md` : `${parentKey}.md`)
          })
          items.push({ '@type': 'ListItem', position: 3, name: nameOf(pathKey), item: canonicalUrl })
        } else if (pathKey.startsWith('guide/') && pathKey !== 'guide/index') {
          items.push({
            '@type': 'ListItem',
            position: 2,
            name: isEn ? 'Guide' : '使用指南',
            item: `${siteUrl}${isEn ? '/en' : ''}/guide/`
          })
          items.push({ '@type': 'ListItem', position: 3, name: nameOf(pathKey), item: canonicalUrl })
        } else {
          items.push({ '@type': 'ListItem', position: 2, name: nameOf(pathKey), item: canonicalUrl })
        }
        if (items.length > 1) {
          graph.push({ '@type': 'BreadcrumbList', itemListElement: items })
        }
      }

      if (graph.length > 0) {
        pageData.frontmatter.head.push([
          'script',
          { type: 'application/ld+json' },
          JSON.stringify({ '@context': 'https://schema.org', '@graph': graph })
        ])
      }
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
            text: 'TCP/UDP',
            items: [
              { text: 'TCP 调试工具', link: '/tcp-debug-tool' },
              { text: 'UDP 调试工具', link: '/udp-debug-tool' },
              { text: 'Socket 调试工具', link: '/socket-debug-tool' },
              { text: 'TCP 粘包处理', link: '/tcp-sticky-packet' },
              { text: 'TCP/UDP 压测', link: '/tcp-stress-test' },
              { text: 'Modbus TCP 调试', link: '/modbus-tcp' },
              { text: '十六进制发送', link: '/hex-send' },
              { text: 'UDP 广播设备发现', link: '/udp-broadcast-device-discovery' },
              { text: '调试教程', link: '/guide/tcp-udp' },
              { text: '全部功能', link: '/features' }
            ]
          },
          {
            text: 'MQTT',
            items: [
              { text: 'MQTT 总览', link: '/mqtt' },
              { text: 'MQTT 客户端', link: '/mqtt-client' },
              { text: '调试教程', link: '/guide/mqtt' },
              { text: '连接与订阅排错', link: '/mqtt-troubleshooting' },
              { text: 'TLS 与双向认证', link: '/mqtt-tls' },
              { text: 'MQTT 5.0', link: '/mqtt-5' },
              { text: '测试场景', link: '/mqtt-testing' },
              { text: 'vs MQTTX', link: '/comparison-mqttx' },
              { text: 'vs MQTT Explorer', link: '/compare/mqtt-explorer' },
              { text: 'vs MQTT.fx', link: '/compare/mqtt-fx' },
              { text: 'Windows 版', link: '/mqtt-client-windows' },
              { text: 'macOS 版', link: '/mqtt-client-mac' },
              { text: 'Linux 版', link: '/mqtt-client-linux' },
              { text: '常见问题', link: '/mqtt-faq' }
            ]
          },
          {
            text: '对比',
            items: [
              { text: 'MQTT 客户端对比', link: '/compare/' },
              { text: 'vs NetAssist（网络调试助手）', link: '/compare/netassist' },
              { text: '网络调试工具对比', link: '/comparison' },
              { text: 'vs MQTTX', link: '/comparison-mqttx' }
            ]
          },
          { text: '常见问题', link: '/faq' },
          { text: '使用指南', link: '/guide/' },
          { text: '下载', link: '/download' },
          { text: '更新日志', link: '/changelog' }
        ],
        sidebar: {
          '/mqtt': mqttSidebarZh,
          '/guide/mqtt': mqttSidebarZh,
          '/compare/': compareSidebarZh,
          '/comparison-mqttx': compareSidebarZh,
          '/comparison': compareSidebarZh,
          '/windows-network-debug-tool': platformSidebarZh,
          '/mac-network-debug-tool': platformSidebarZh,
          '/linux-network-debug-tool': platformSidebarZh,
          '/tcp-debug-tool': tcpSidebarZh,
          '/udp-debug-tool': tcpSidebarZh,
          '/socket-debug-tool': tcpSidebarZh,
          '/tcp-sticky-packet': tcpSidebarZh,
          '/tcp-stress-test': tcpSidebarZh,
          '/modbus-tcp': tcpSidebarZh,
          '/hex-send': tcpSidebarZh,
          '/udp-broadcast-device-discovery': tcpSidebarZh,
          '/guide/': [
            {
              text: '使用指南',
              items: [
                { text: '快速上手', link: '/guide/' },
                { text: 'MQTT 调试', link: '/guide/mqtt' },
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
        'A high-performance cross-platform network debugging tool built with Rust, available for Windows, Linux and macOS on x64 and ARM64, featuring TCP/UDP client & server, multiple decoders, message management and TCP/UDP stress testing.',
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
            text: 'TCP/UDP',
            items: [
              { text: 'TCP debug tool', link: '/en/tcp-debug-tool' },
              { text: 'UDP debug tool', link: '/en/udp-debug-tool' },
              { text: 'Socket debug tool', link: '/en/socket-debug-tool' },
              { text: 'TCP sticky packets', link: '/en/tcp-sticky-packet' },
              { text: 'TCP/UDP stress testing', link: '/en/tcp-stress-test' },
              { text: 'Modbus TCP debugging', link: '/en/modbus-tcp' },
              { text: 'Hex send', link: '/en/hex-send' },
              { text: 'UDP broadcast discovery', link: '/en/udp-broadcast-device-discovery' },
              { text: 'Tutorial', link: '/en/guide/tcp-udp' },
              { text: 'All features', link: '/en/features' }
            ]
          },
          {
            text: 'MQTT',
            items: [
              { text: 'MQTT overview', link: '/en/mqtt' },
              { text: 'MQTT client', link: '/en/mqtt-client' },
              { text: 'Tutorial', link: '/en/guide/mqtt' },
              { text: 'Troubleshooting', link: '/en/mqtt-troubleshooting' },
              { text: 'TLS & mutual auth', link: '/en/mqtt-tls' },
              { text: 'MQTT 5.0', link: '/en/mqtt-5' },
              { text: 'Testing scenarios', link: '/en/mqtt-testing' },
              { text: 'vs MQTTX', link: '/en/comparison-mqttx' },
              { text: 'vs MQTT Explorer', link: '/en/compare/mqtt-explorer' },
              { text: 'vs MQTT.fx', link: '/en/compare/mqtt-fx' },
              { text: 'Windows client', link: '/en/mqtt-client-windows' },
              { text: 'macOS client', link: '/en/mqtt-client-mac' },
              { text: 'Linux client', link: '/en/mqtt-client-linux' },
              { text: 'FAQ', link: '/en/mqtt-faq' }
            ]
          },
          {
            text: 'Compare',
            items: [
              { text: 'MQTT client comparison', link: '/en/compare/' },
              { text: 'vs NetAssist', link: '/en/compare/netassist' },
              { text: 'Network debugging tool comparison', link: '/en/comparison' },
              { text: 'vs MQTTX', link: '/en/comparison-mqttx' }
            ]
          },
          { text: 'FAQ', link: '/en/faq' },
          { text: 'Guide', link: '/en/guide/' },
          { text: 'Download', link: '/en/download' },
          { text: 'Changelog', link: '/en/changelog' }
        ],
        sidebar: {
          '/en/mqtt': mqttSidebarEn,
          '/en/guide/mqtt': mqttSidebarEn,
          '/en/compare/': compareSidebarEn,
          '/en/comparison-mqttx': compareSidebarEn,
          '/en/comparison': compareSidebarEn,
          '/en/windows-network-debug-tool': platformSidebarEn,
          '/en/mac-network-debug-tool': platformSidebarEn,
          '/en/linux-network-debug-tool': platformSidebarEn,
          '/en/tcp-debug-tool': tcpSidebarEn,
          '/en/udp-debug-tool': tcpSidebarEn,
          '/en/socket-debug-tool': tcpSidebarEn,
          '/en/tcp-sticky-packet': tcpSidebarEn,
          '/en/tcp-stress-test': tcpSidebarEn,
          '/en/modbus-tcp': tcpSidebarEn,
          '/en/hex-send': tcpSidebarEn,
          '/en/udp-broadcast-device-discovery': tcpSidebarEn,
          '/en/guide/': [
            {
              text: 'Guide',
              items: [
                { text: 'Getting Started', link: '/en/guide/' },
                { text: 'MQTT Debugging', link: '/en/guide/mqtt' },
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
