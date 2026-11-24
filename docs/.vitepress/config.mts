import fs from 'node:fs'
import path from 'node:path'
import { defineConfig } from 'vitepress'

// 默认走 GitHub Pages 子路径；Cloudflare Pages 构建时设置 VITEPRESS_BASE=/ 覆盖
const base = process.env.VITEPRESS_BASE || '/NetAssistant/'

const siteUrl = 'https://netassistant.trydo.top'
const ogImage = `${siteUrl}/og-cover.png`

type LocaleCode = 'zh' | 'en' | 'ja' | 'de' | 'ru' | 'pt-BR' | 'es'

// 语言清单：dir 为内容目录（英文为根目录），hreflang 用于 alternates，ogLocale 用于 Open Graph
const localeList: {
  code: LocaleCode
  dir: string
  hreflang: string
  ogLocale: string
  home: string
}[] = [
  { code: 'en', dir: '', hreflang: 'en-US', ogLocale: 'en_US', home: 'Home' },
  { code: 'zh', dir: 'zh', hreflang: 'zh-CN', ogLocale: 'zh_CN', home: '首页' },
  { code: 'ja', dir: 'ja', hreflang: 'ja-JP', ogLocale: 'ja_JP', home: 'ホーム' },
  { code: 'de', dir: 'de', hreflang: 'de-DE', ogLocale: 'de_DE', home: 'Startseite' },
  { code: 'ru', dir: 'ru', hreflang: 'ru-RU', ogLocale: 'ru_RU', home: 'Главная' },
  { code: 'pt-BR', dir: 'pt-BR', hreflang: 'pt-BR', ogLocale: 'pt_BR', home: 'Início' },
  { code: 'es', dir: 'es', hreflang: 'es-ES', ogLocale: 'es_ES', home: 'Inicio' }
]

const defaultDescription: Record<LocaleCode, string> = {
  zh: 'NetAssistant 是基于 Rust 的开源跨平台网络调试工具：TCP/UDP 客户端与服务端、MQTT 3.1.1 / 5.0、内置工具箱、四种解码器与 TCP/UDP 高并发压测，支持 Windows、Linux、macOS。',
  en: 'NetAssistant is an open-source Rust network debugging tool: TCP/UDP client & server, MQTT 3.1.1 / 5.0, built-in toolbox, four decoders and TCP/UDP stress testing for Windows, Linux and macOS.',
  ja: 'Rust 製の高性能クロスプラットフォーム・ネットワーク診断ツール。Windows・Linux・macOS（x64 / ARM64）対応で、TCP/UDP クライアント＆サーバー、複数のデコーダー、メッセージ管理、高負荷テストを搭載。',
  de: 'Ein hochperformantes, plattformübergreifendes Netzwerk-Debugging-Tool auf Rust-Basis für Windows, Linux und macOS (x64 / ARM64) mit TCP/UDP-Client und -Server, mehreren Decodern, Nachrichtenverwaltung und Lasttests.',
  ru: 'Высокопроизводительный кроссплатформенный инструмент отладки сети на Rust для Windows, Linux и macOS (x64 / ARM64): TCP/UDP-клиент и сервер, декодеры, управление сообщениями и нагрузочное тестирование.',
  'pt-BR':
    'Ferramenta de depuração de rede multiplataforma de alta performance escrita em Rust, para Windows, Linux e macOS (x64 / ARM64), com cliente e servidor TCP/UDP, decodificadores, gerenciamento de mensagens e testes de carga.',
  es: 'Herramienta de depuración de red multiplataforma de alto rendimiento escrita en Rust para Windows, Linux y macOS (x64 / ARM64), con cliente y servidor TCP/UDP, decodificadores, gestión de mensajes y pruebas de carga.'
}

// 面包屑与结构化数据所需的页面名称；key 为去掉 .md 与语言目录前缀后的路径
const pageNames: Record<string, Partial<Record<LocaleCode, string>>> = {
  features: { zh: '功能特性', en: 'Features' },
  download: {
    zh: '下载',
    en: 'Download',
    ja: 'ダウンロード',
    de: 'Download',
    ru: 'Скачать',
    'pt-BR': 'Download',
    es: 'Descargar'
  },
  faq: {
    zh: '常见问题',
    en: 'FAQ',
    ja: 'よくある質問',
    de: 'FAQ',
    ru: 'FAQ',
    'pt-BR': 'Perguntas frequentes',
    es: 'Preguntas frecuentes'
  },
  comparison: { zh: '网络调试工具对比', en: 'Network Debugging Tool Comparison' },
  'comparison-mqttx': { zh: 'vs MQTTX', en: 'vs MQTTX' },
  changelog: { zh: '更新日志', en: 'Changelog' },
  'guide/index': { zh: '使用指南', en: 'Guide' },
  'guide/tcp-udp': { zh: 'TCP/UDP 调试', en: 'TCP/UDP Debugging' },
  'guide/mqtt': { zh: 'MQTT 调试', en: 'MQTT Debugging' },
  'mqtt-client': { zh: 'MQTT 客户端', en: 'MQTT Client', ja: 'MQTT クライアント' },
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
  'tcp-debug-tool': { zh: 'TCP 调试工具', en: 'TCP Debug Tool', ja: 'TCP デバッグツール' },
  'udp-debug-tool': { zh: 'UDP 调试工具', en: 'UDP Debug Tool', ja: 'UDP デバッグツール' },
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
      { text: 'MQTT 总览', link: '/zh/mqtt' },
      { text: 'MQTT 客户端', link: '/zh/mqtt-client' },
      { text: '调试教程', link: '/zh/guide/mqtt' },
      { text: '连接与订阅排错', link: '/zh/mqtt-troubleshooting' },
      { text: 'TLS 与双向认证', link: '/zh/mqtt-tls' },
      { text: 'MQTT 5.0', link: '/zh/mqtt-5' },
      { text: '测试场景', link: '/zh/mqtt-testing' }
    ]
  },
  {
    text: '对比与选型',
    items: [
      { text: 'MQTTX 对比', link: '/zh/comparison-mqttx' },
      { text: 'MQTT Explorer 对比', link: '/zh/compare/mqtt-explorer' },
      { text: 'MQTT.fx 对比', link: '/zh/compare/mqtt-fx' }
    ]
  },
  {
    text: '平台与常见问题',
    items: [
      { text: 'Windows 版', link: '/zh/mqtt-client-windows' },
      { text: 'macOS 版', link: '/zh/mqtt-client-mac' },
      { text: 'Linux 版', link: '/zh/mqtt-client-linux' },
      { text: '常见问题', link: '/zh/mqtt-faq' }
    ]
  }
]

const mqttSidebarEn = [
  {
    text: 'MQTT Debugging',
    items: [
      { text: 'MQTT overview', link: '/zh/en/mqtt' },
      { text: 'MQTT client', link: '/zh/en/mqtt-client' },
      { text: 'Tutorial', link: '/zh/en/guide/mqtt' },
      { text: 'Troubleshooting', link: '/zh/en/mqtt-troubleshooting' },
      { text: 'TLS & mutual auth', link: '/zh/en/mqtt-tls' },
      { text: 'MQTT 5.0', link: '/zh/en/mqtt-5' },
      { text: 'Testing scenarios', link: '/zh/en/mqtt-testing' }
    ]
  },
  {
    text: 'Comparison',
    items: [
      { text: 'vs MQTTX', link: '/zh/en/comparison-mqttx' },
      { text: 'vs MQTT Explorer', link: '/zh/en/compare/mqtt-explorer' },
      { text: 'vs MQTT.fx', link: '/zh/en/compare/mqtt-fx' }
    ]
  },
  {
    text: 'Platforms & FAQ',
    items: [
      { text: 'Windows client', link: '/zh/en/mqtt-client-windows' },
      { text: 'macOS client', link: '/zh/en/mqtt-client-mac' },
      { text: 'Linux client', link: '/zh/en/mqtt-client-linux' },
      { text: 'FAQ', link: '/zh/en/mqtt-faq' }
    ]
  }
]

const compareSidebarZh = [
  {
    text: 'MQTT 客户端',
    items: [
      { text: 'MQTT 客户端对比', link: '/zh/compare/' },
      { text: 'vs MQTTX', link: '/zh/comparison-mqttx' },
      { text: 'vs MQTT Explorer', link: '/zh/compare/mqtt-explorer' },
      { text: 'vs MQTT.fx', link: '/zh/compare/mqtt-fx' }
    ]
  },
  {
    text: 'TCP/UDP 工具',
    items: [
      { text: '网络调试工具对比', link: '/zh/comparison' },
      { text: 'vs NetAssist', link: '/zh/compare/netassist' }
    ]
  }
]

const compareSidebarEn = [
  {
    text: 'MQTT clients',
    items: [
      { text: 'MQTT client comparison', link: '/zh/en/compare/' },
      { text: 'vs MQTTX', link: '/zh/en/comparison-mqttx' },
      { text: 'vs MQTT Explorer', link: '/zh/en/compare/mqtt-explorer' },
      { text: 'vs MQTT.fx', link: '/zh/en/compare/mqtt-fx' }
    ]
  },
  {
    text: 'TCP/UDP tools',
    items: [
      { text: 'Network debugging tool comparison', link: '/zh/en/comparison' },
      { text: 'vs NetAssist', link: '/zh/en/compare/netassist' }
    ]
  }
]

const platformSidebarZh = [
  {
    text: '平台支持',
    items: [
      { text: 'Windows 版', link: '/zh/windows-network-debug-tool' },
      { text: 'macOS 版', link: '/zh/mac-network-debug-tool' },
      { text: 'Linux 版', link: '/zh/linux-network-debug-tool' },
      { text: '下载', link: '/zh/download' }
    ]
  }
]

const platformSidebarEn = [
  {
    text: 'Platforms',
    items: [
      { text: 'Windows', link: '/zh/en/windows-network-debug-tool' },
      { text: 'macOS', link: '/zh/en/mac-network-debug-tool' },
      { text: 'Linux', link: '/zh/en/linux-network-debug-tool' },
      { text: 'Download', link: '/zh/en/download' }
    ]
  }
]

const tcpSidebarZh = [
  {
    text: 'TCP/UDP 调试',
    items: [
      { text: 'TCP 调试工具', link: '/zh/tcp-debug-tool' },
      { text: 'UDP 调试工具', link: '/zh/udp-debug-tool' },
      { text: 'Socket 调试工具', link: '/zh/socket-debug-tool' },
      { text: 'TCP 粘包处理', link: '/zh/tcp-sticky-packet' },
      { text: 'TCP/UDP 压测', link: '/zh/tcp-stress-test' },
      { text: 'Modbus TCP 调试', link: '/zh/modbus-tcp' },
      { text: '十六进制发送', link: '/zh/hex-send' },
      { text: 'UDP 广播设备发现', link: '/zh/udp-broadcast-device-discovery' }
    ]
  },
  {
    text: '学习与总览',
    items: [
      { text: '调试教程', link: '/zh/guide/tcp-udp' },
      { text: '全部功能', link: '/zh/features' }
    ]
  }
]

const tcpSidebarEn = [
  {
    text: 'TCP/UDP Debugging',
    items: [
      { text: 'TCP debug tool', link: '/zh/en/tcp-debug-tool' },
      { text: 'UDP debug tool', link: '/zh/en/udp-debug-tool' },
      { text: 'Socket debug tool', link: '/zh/en/socket-debug-tool' },
      { text: 'TCP sticky packets', link: '/zh/en/tcp-sticky-packet' },
      { text: 'TCP/UDP stress testing', link: '/zh/en/tcp-stress-test' },
      { text: 'Modbus TCP debugging', link: '/zh/en/modbus-tcp' },
      { text: 'Hex send', link: '/zh/en/hex-send' },
      { text: 'UDP broadcast discovery', link: '/zh/en/udp-broadcast-device-discovery' }
    ]
  },
  {
    text: 'Learn',
    items: [
      { text: 'Tutorial', link: '/zh/en/guide/tcp-udp' },
      { text: 'All features', link: '/zh/en/features' }
    ]
  }
]

const jaSidebar = [
  {
    text: 'ドキュメント',
    items: [
      { text: 'TCP デバッグツール', link: '/zh/ja/tcp-debug-tool' },
      { text: 'UDP デバッグツール', link: '/zh/ja/udp-debug-tool' },
      { text: 'MQTT クライアント', link: '/zh/ja/mqtt-client' },
      { text: 'よくある質問', link: '/zh/ja/faq' },
      { text: 'ダウンロード', link: '/zh/ja/download' }
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
    // 旧 /en/ 地址现为重定向存根页，不生成 SEO 元数据（实际跳转由 Cloudflare _redirects 308 完成）
    if (relativePath.startsWith('en/')) return
    const toUrl = (p: string) =>
      `${siteUrl}/${p}`.replace(/index\.md$/, '').replace(/\.md$/, '')
    // 对应语言页面是否存在，避免 hreflang 指向不存在的地址
    const pageExists = (p: string) => fs.existsSync(path.join(ctx.siteConfig.srcDir, p))
    // 当前页面所属语言：带目录前缀的走对应语言，根目录视为英文
    const locale =
      localeList.find((l) => l.dir && relativePath.startsWith(`${l.dir}/`)) ?? localeList[0]
    const localePath = (key: string) => (locale.dir ? `${locale.dir}/${key}` : key)
    const pathKey = (locale.dir ? relativePath.slice(locale.dir.length + 1) : relativePath).replace(
      /\.md$/,
      ''
    )
    const canonicalUrl = toUrl(relativePath)
    const homeUrl = `${siteUrl}/${locale.dir ? `${locale.dir}/` : ''}`
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
    const description = pageData.frontmatter.description || defaultDescription[locale.code]
    const inLanguage = locale.hreflang

    // 只为真实存在的翻译页面生成 hreflang 与 og:locale:alternate
    const existingLocales = localeList.filter((l) =>
      pageExists(`${l.dir ? `${l.dir}/` : ''}${pathKey}.md`)
    )
    const alternates: any[] = []
    for (const l of existingLocales) {
      alternates.push([
        'link',
        {
          rel: 'alternate',
          hreflang: l.hreflang,
          href: toUrl(`${l.dir ? `${l.dir}/` : ''}${pathKey}.md`)
        }
      ])
    }
    // x-default 优先英文（根目录），其次中文，再次任意已有语言
    const xDefault =
      existingLocales.find((l) => l.code === 'en') ??
      existingLocales.find((l) => l.code === 'zh') ??
      existingLocales[0]
    if (xDefault) {
      alternates.push([
        'link',
        {
          rel: 'alternate',
          hreflang: 'x-default',
          href: toUrl(`${xDefault.dir ? `${xDefault.dir}/` : ''}${pathKey}.md`)
        }
      ])
    }

    pageData.frontmatter.head ??= []
    pageData.frontmatter.head.push(
      ['link', { rel: 'canonical', href: canonicalUrl }],
      ...alternates,
      ['meta', { property: 'og:title', content: pageTitle }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: canonicalUrl }],
      ['meta', { property: 'og:locale', content: locale.ogLocale }],
      ...existingLocales
        .filter((l) => l.code !== locale.code)
        .map((l) => ['meta', { property: 'og:locale:alternate', content: l.ogLocale }]),
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
                '@id': `${homeUrl}#website`,
                name: 'NetAssistant',
                url: homeUrl,
                description,
                inLanguage
              },
              {
                '@type': 'SoftwareApplication',
                '@id': `${homeUrl}#software`,
                name: 'NetAssistant',
                url: homeUrl,
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
            name: locale.home,
            item: homeUrl
          }
        ]
        const nameOf = (key: string) =>
          pageNames[key]?.[locale.code] ?? pageNames[key]?.en ?? rawTitle
        const parentKey = breadcrumbParents[pathKey]
        if (parentKey && pageExists(`${localePath(parentKey)}.md`)) {
          items.push({
            '@type': 'ListItem',
            position: 2,
            name: nameOf(parentKey),
            item: toUrl(`${localePath(parentKey)}.md`)
          })
          items.push({ '@type': 'ListItem', position: 3, name: nameOf(pathKey), item: canonicalUrl })
        } else if (
          pathKey.startsWith('guide/') &&
          pathKey !== 'guide/index' &&
          pageExists(`${localePath('guide/index')}.md`)
        ) {
          items.push({
            '@type': 'ListItem',
            position: 2,
            name: nameOf('guide/index'),
            item: toUrl(`${localePath('guide/index')}.md`)
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
      label: 'English',
      lang: 'en-US',
      description:
        'A high-performance cross-platform network debugging tool built with Rust, available for Windows, Linux and macOS on x64 and ARM64, featuring TCP/UDP client & server, multiple decoders, message management and TCP/UDP stress testing.',
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: 'Home', link: '/' },
          {
            text: 'Platforms',
            items: [
              { text: 'Windows Network Debugging Tool', link: '/windows-network-debug-tool' },
              { text: 'Mac Network Debugging Tool', link: '/mac-network-debug-tool' },
              { text: 'Linux Network Debugging Tool', link: '/linux-network-debug-tool' }
            ]
          },
          {
            text: 'TCP/UDP',
            items: [
              { text: 'TCP debug tool', link: '/tcp-debug-tool' },
              { text: 'UDP debug tool', link: '/udp-debug-tool' },
              { text: 'Socket debug tool', link: '/socket-debug-tool' },
              { text: 'TCP sticky packets', link: '/tcp-sticky-packet' },
              { text: 'TCP/UDP stress testing', link: '/tcp-stress-test' },
              { text: 'Modbus TCP debugging', link: '/modbus-tcp' },
              { text: 'Hex send', link: '/hex-send' },
              { text: 'UDP broadcast discovery', link: '/udp-broadcast-device-discovery' },
              { text: 'Tutorial', link: '/guide/tcp-udp' },
              { text: 'All features', link: '/features' }
            ]
          },
          {
            text: 'MQTT',
            items: [
              { text: 'MQTT overview', link: '/mqtt' },
              { text: 'MQTT client', link: '/mqtt-client' },
              { text: 'Tutorial', link: '/guide/mqtt' },
              { text: 'Troubleshooting', link: '/mqtt-troubleshooting' },
              { text: 'TLS & mutual auth', link: '/mqtt-tls' },
              { text: 'MQTT 5.0', link: '/mqtt-5' },
              { text: 'Testing scenarios', link: '/mqtt-testing' },
              { text: 'vs MQTTX', link: '/comparison-mqttx' },
              { text: 'vs MQTT Explorer', link: '/compare/mqtt-explorer' },
              { text: 'vs MQTT.fx', link: '/compare/mqtt-fx' },
              { text: 'Windows client', link: '/mqtt-client-windows' },
              { text: 'macOS client', link: '/mqtt-client-mac' },
              { text: 'Linux client', link: '/mqtt-client-linux' },
              { text: 'FAQ', link: '/mqtt-faq' }
            ]
          },
          {
            text: 'Compare',
            items: [
              { text: 'MQTT client comparison', link: '/compare/' },
              { text: 'vs NetAssist', link: '/compare/netassist' },
              { text: 'Network debugging tool comparison', link: '/comparison' },
              { text: 'vs MQTTX', link: '/comparison-mqttx' }
            ]
          },
          { text: 'FAQ', link: '/faq' },
          { text: 'Guide', link: '/guide/' },
          { text: 'Download', link: '/download' },
          { text: 'Changelog', link: '/changelog' }
        ],
        sidebar: {
          '/mqtt': mqttSidebarEn,
          '/guide/mqtt': mqttSidebarEn,
          '/compare/': compareSidebarEn,
          '/comparison-mqttx': compareSidebarEn,
          '/comparison': compareSidebarEn,
          '/windows-network-debug-tool': platformSidebarEn,
          '/mac-network-debug-tool': platformSidebarEn,
          '/linux-network-debug-tool': platformSidebarEn,
          '/tcp-debug-tool': tcpSidebarEn,
          '/udp-debug-tool': tcpSidebarEn,
          '/socket-debug-tool': tcpSidebarEn,
          '/tcp-sticky-packet': tcpSidebarEn,
          '/tcp-stress-test': tcpSidebarEn,
          '/modbus-tcp': tcpSidebarEn,
          '/hex-send': tcpSidebarEn,
          '/udp-broadcast-device-discovery': tcpSidebarEn,
          '/guide/': [
            {
              text: 'Guide',
              items: [
                { text: 'Getting Started', link: '/guide/' },
                { text: 'MQTT Debugging', link: '/guide/mqtt' },
                { text: 'TCP/UDP Debugging', link: '/guide/tcp-udp' },
                { text: 'Stress Testing', link: '/guide/stress' }
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
    },
    zh: {
      label: '简体中文',
      lang: 'zh-CN',
      link: '/zh/',
      description:
        '基于 Rust 构建的高性能跨平台网络调试工具，支持 Windows、Linux、macOS（x64 / ARM64），涵盖 TCP/UDP 客户端与服务端、多种解码器、消息管理与高并发压力测试。',
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: '首页', link: '/zh/' },
          {
            text: '平台',
            items: [
              { text: 'Windows 网络调试助手', link: '/zh/windows-network-debug-tool' },
              { text: 'macOS 网络调试助手', link: '/zh/mac-network-debug-tool' },
              { text: 'Linux 网络调试助手', link: '/zh/linux-network-debug-tool' }
            ]
          },
          {
            text: 'TCP/UDP',
            items: [
              { text: 'TCP 调试工具', link: '/zh/tcp-debug-tool' },
              { text: 'UDP 调试工具', link: '/zh/udp-debug-tool' },
              { text: 'Socket 调试工具', link: '/zh/socket-debug-tool' },
              { text: 'TCP 粘包处理', link: '/zh/tcp-sticky-packet' },
              { text: 'TCP/UDP 压测', link: '/zh/tcp-stress-test' },
              { text: 'Modbus TCP 调试', link: '/zh/modbus-tcp' },
              { text: '十六进制发送', link: '/zh/hex-send' },
              { text: 'UDP 广播设备发现', link: '/zh/udp-broadcast-device-discovery' },
              { text: '调试教程', link: '/zh/guide/tcp-udp' },
              { text: '全部功能', link: '/zh/features' }
            ]
          },
          {
            text: 'MQTT',
            items: [
              { text: 'MQTT 总览', link: '/zh/mqtt' },
              { text: 'MQTT 客户端', link: '/zh/mqtt-client' },
              { text: '调试教程', link: '/zh/guide/mqtt' },
              { text: '连接与订阅排错', link: '/zh/mqtt-troubleshooting' },
              { text: 'TLS 与双向认证', link: '/zh/mqtt-tls' },
              { text: 'MQTT 5.0', link: '/zh/mqtt-5' },
              { text: '测试场景', link: '/zh/mqtt-testing' },
              { text: 'vs MQTTX', link: '/zh/comparison-mqttx' },
              { text: 'vs MQTT Explorer', link: '/zh/compare/mqtt-explorer' },
              { text: 'vs MQTT.fx', link: '/zh/compare/mqtt-fx' },
              { text: 'Windows 版', link: '/zh/mqtt-client-windows' },
              { text: 'macOS 版', link: '/zh/mqtt-client-mac' },
              { text: 'Linux 版', link: '/zh/mqtt-client-linux' },
              { text: '常见问题', link: '/zh/mqtt-faq' }
            ]
          },
          {
            text: '对比',
            items: [
              { text: 'MQTT 客户端对比', link: '/zh/compare/' },
              { text: 'vs NetAssist（网络调试助手）', link: '/zh/compare/netassist' },
              { text: '网络调试工具对比', link: '/zh/comparison' },
              { text: 'vs MQTTX', link: '/zh/comparison-mqttx' }
            ]
          },
          { text: '常见问题', link: '/zh/faq' },
          { text: '使用指南', link: '/zh/guide/' },
          { text: '下载', link: '/zh/download' },
          { text: '更新日志', link: '/zh/changelog' }
        ],
        sidebar: {
          '/zh/mqtt': mqttSidebarZh,
          '/zh/guide/mqtt': mqttSidebarZh,
          '/zh/compare/': compareSidebarZh,
          '/zh/comparison-mqttx': compareSidebarZh,
          '/zh/comparison': compareSidebarZh,
          '/zh/windows-network-debug-tool': platformSidebarZh,
          '/zh/mac-network-debug-tool': platformSidebarZh,
          '/zh/linux-network-debug-tool': platformSidebarZh,
          '/zh/tcp-debug-tool': tcpSidebarZh,
          '/zh/udp-debug-tool': tcpSidebarZh,
          '/zh/socket-debug-tool': tcpSidebarZh,
          '/zh/tcp-sticky-packet': tcpSidebarZh,
          '/zh/tcp-stress-test': tcpSidebarZh,
          '/zh/modbus-tcp': tcpSidebarZh,
          '/zh/hex-send': tcpSidebarZh,
          '/zh/udp-broadcast-device-discovery': tcpSidebarZh,
          '/zh/guide/': [
            {
              text: '使用指南',
              items: [
                { text: '快速上手', link: '/zh/guide/' },
                { text: 'MQTT 调试', link: '/zh/guide/mqtt' },
                { text: 'TCP/UDP 调试', link: '/zh/guide/tcp-udp' },
                { text: '压力测试', link: '/zh/guide/stress' }
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
    ja: {
      label: '日本語',
      lang: 'ja-JP',
      link: '/ja/',
      description: defaultDescription.ja,
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: 'ホーム', link: '/ja/' },
          {
            text: 'TCP/UDP',
            items: [
              { text: 'TCP デバッグツール', link: '/ja/tcp-debug-tool' },
              { text: 'UDP デバッグツール', link: '/ja/udp-debug-tool' }
            ]
          },
          { text: 'MQTT クライアント', link: '/ja/mqtt-client' },
          { text: 'よくある質問', link: '/ja/faq' },
          { text: 'ダウンロード', link: '/ja/download' }
        ],
        sidebar: {
          '/ja/tcp-debug-tool': jaSidebar,
          '/ja/udp-debug-tool': jaSidebar,
          '/ja/mqtt-client': jaSidebar,
          '/ja/faq': jaSidebar
        },
        socialLinks: [{ icon: 'github', link: 'https://github.com/SunJary/NetAssistant' }],
        search: {
          provider: 'local',
          options: {
            translations: {
              button: { buttonText: '検索', buttonAriaLabel: '検索' },
              modal: {
                noResultsText: '該当する結果が見つかりません',
                resetButtonTitle: '検索条件をクリア',
                footer: { selectText: '選択', navigateText: '移動', closeText: '閉じる' }
              }
            }
          }
        },
        outline: { level: [2, 3] },
        lastUpdated: { text: '最終更新:' },
        footer: {
          message: 'Apache-2.0 ライセンスで公開',
          copyright: 'Copyright © 2026 SunJary'
        }
      }
    },
    de: {
      label: 'Deutsch',
      lang: 'de-DE',
      link: '/de/',
      description: defaultDescription.de,
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: 'Startseite', link: '/de/' },
          { text: 'FAQ', link: '/de/faq' },
          { text: 'Download', link: '/de/download' }
        ],
        socialLinks: [{ icon: 'github', link: 'https://github.com/SunJary/NetAssistant' }],
        search: { provider: 'local' },
        outline: { level: [2, 3] },
        lastUpdated: { text: 'Zuletzt aktualisiert:' },
        footer: {
          message: 'Veröffentlicht unter der Apache-2.0-Lizenz',
          copyright: 'Copyright © 2026 SunJary'
        }
      }
    },
    ru: {
      label: 'Русский',
      lang: 'ru-RU',
      link: '/ru/',
      description: defaultDescription.ru,
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: 'Главная', link: '/ru/' },
          { text: 'FAQ', link: '/ru/faq' },
          { text: 'Скачать', link: '/ru/download' }
        ],
        socialLinks: [{ icon: 'github', link: 'https://github.com/SunJary/NetAssistant' }],
        search: { provider: 'local' },
        outline: { level: [2, 3] },
        lastUpdated: { text: 'Обновлено:' },
        footer: {
          message: 'Лицензия Apache-2.0',
          copyright: 'Copyright © 2026 SunJary'
        }
      }
    },
    'pt-BR': {
      label: 'Português (Brasil)',
      lang: 'pt-BR',
      link: '/pt-BR/',
      description: defaultDescription['pt-BR'],
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: 'Início', link: '/pt-BR/' },
          { text: 'Perguntas frequentes', link: '/pt-BR/faq' },
          { text: 'Download', link: '/pt-BR/download' }
        ],
        socialLinks: [{ icon: 'github', link: 'https://github.com/SunJary/NetAssistant' }],
        search: { provider: 'local' },
        outline: { level: [2, 3] },
        lastUpdated: { text: 'Última atualização:' },
        footer: {
          message: 'Licenciado sob a Apache-2.0',
          copyright: 'Copyright © 2026 SunJary'
        }
      }
    },
    es: {
      label: 'Español',
      lang: 'es-ES',
      link: '/es/',
      description: defaultDescription.es,
      themeConfig: {
        logo: '/logo.png',
        nav: [
          { text: 'Inicio', link: '/es/' },
          { text: 'Preguntas frecuentes', link: '/es/faq' },
          { text: 'Descargar', link: '/es/download' }
        ],
        socialLinks: [{ icon: 'github', link: 'https://github.com/SunJary/NetAssistant' }],
        search: { provider: 'local' },
        outline: { level: [2, 3] },
        lastUpdated: { text: 'Última actualización:' },
        footer: {
          message: 'Publicado bajo la licencia Apache-2.0',
          copyright: 'Copyright © 2026 SunJary'
        }
      }
    }
  }
})
