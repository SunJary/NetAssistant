<script setup>
import { computed, onMounted, ref } from 'vue'
import { useData } from 'vitepress'

const { lang } = useData()
const isEn = computed(() => lang.value.startsWith('en'))

const os = ref('other')
const copied = ref(false)
const loading = ref(false)
const release = ref(null)

const repo = 'SunJary/NetAssistant'
const releaseBase = `https://github.com/${repo}/releases/latest/download/`
const latestPage = `https://github.com/${repo}/releases/latest`

const texts = computed(() =>
  isEn.value
    ? {
        eyebrow: 'Free and open source · No signup',
        title: 'Download NetAssistant',
        lead: 'Pick your platform. Links always point to the latest release.',
        recommended: 'Recommended for your system',
        download: 'Download',
        winget: 'Or install with winget',
        copy: 'Copy',
        copied: 'Copied',
        other: 'Other platforms and packages',
        version: 'Latest version',
        published: 'Published',
        size: 'Size',
        loading: 'Fetching version information…',
        slow: 'Download slow from GitHub? Try winget on Windows, or use a trusted GitHub acceleration service.',
        windows: 'Windows',
        mac: 'macOS',
        linux: 'Linux',
        winNote: 'Windows 10 1709+ · x64 · installer',
        macNote: 'macOS 10.15+ · Universal (Apple Silicon & Intel)',
        linuxNote: 'Ubuntu 22.04+ · x64 · deb package',
        winZip: 'Portable zip',
        winArm: 'Windows ARM64 zip',
        macZip: 'macOS zip',
        linuxAppImage: 'AppImage',
        linuxTar: 'tar.gz',
        linuxArm: 'Linux ARM64',
        all: 'All releases on GitHub'
      }
    : {
        eyebrow: '免费开源 · 无需注册',
        title: '下载 NetAssistant',
        lead: '选择你的平台，链接始终指向最新版本。',
        recommended: '为你的系统推荐',
        download: '下载',
        winget: '或使用 winget 安装',
        copy: '复制',
        copied: '已复制',
        other: '其他平台与安装包',
        version: '最新版本',
        published: '发布时间',
        size: '体积',
        loading: '正在获取版本信息…',
        slow: 'GitHub 下载缓慢？Windows 可优先用 winget，或使用你信任的 GitHub 加速服务。',
        windows: 'Windows',
        mac: 'macOS',
        linux: 'Linux',
        winNote: 'Windows 10 1709+ · x64 · 安装程序',
        macNote: 'macOS 10.15+ · Universal（Apple Silicon 与 Intel）',
        linuxNote: 'Ubuntu 22.04+ · x64 · deb 包',
        winZip: '免安装 zip',
        winArm: 'Windows ARM64 zip',
        macZip: 'macOS zip',
        linuxAppImage: 'AppImage',
        linuxTar: 'tar.gz',
        linuxArm: 'Linux ARM64',
        all: 'GitHub 上的全部版本'
      }
)

const primary = computed(() => {
  if (os.value === 'win') {
    return {
      label: texts.value.windows,
      note: texts.value.winNote,
      file: 'netassistant-windows-x86_64-setup.exe'
    }
  }
  if (os.value === 'mac') {
    return {
      label: texts.value.mac,
      note: texts.value.macNote,
      file: 'netassistant-macos-universal.dmg'
    }
  }
  if (os.value === 'linux') {
    return {
      label: texts.value.linux,
      note: texts.value.linuxNote,
      file: 'netassistant-linux-x86_64.deb'
    }
  }
  return null
})

const platforms = computed(() => [
  {
    name: texts.value.windows,
    note: texts.value.winNote,
    file: 'netassistant-windows-x86_64-setup.exe',
    extra: [
      { label: texts.value.winZip, file: 'netassistant-windows-x86_64.zip' },
      { label: texts.value.winArm, file: 'netassistant-windows-aarch64.zip' }
    ]
  },
  {
    name: texts.value.mac,
    note: texts.value.macNote,
    file: 'netassistant-macos-universal.dmg',
    extra: [{ label: texts.value.macZip, file: 'netassistant-macos-universal.zip' }]
  },
  {
    name: texts.value.linux,
    note: texts.value.linuxNote,
    file: 'netassistant-linux-x86_64.deb',
    extra: [
      { label: texts.value.linuxAppImage, file: 'netassistant-linux-x86_64.AppImage' },
      { label: texts.value.linuxTar, file: 'netassistant-linux-x86_64.tar.gz' },
      { label: texts.value.linuxArm, file: 'netassistant-linux-aarch64.deb' }
    ]
  }
])

const version = computed(() => release.value?.tag_name || '')
const published = computed(() => {
  if (!release.value?.published_at) return ''
  return new Date(release.value.published_at).toLocaleDateString(isEn.value ? 'en-US' : 'zh-CN')
})
const primarySize = computed(() => {
  if (!primary.value || !release.value?.assets) return ''
  const asset = release.value.assets.find((item) => item.name === primary.value.file)
  return asset ? `${(asset.size / 1024 / 1024).toFixed(1)} MB` : ''
})

function assetUrl(file) {
  return `${releaseBase}${file}`
}

async function copyWinget() {
  try {
    await navigator.clipboard.writeText('winget install SunJary.NetAssistant')
    copied.value = true
    window.setTimeout(() => (copied.value = false), 2000)
  } catch {
    copied.value = false
  }
}

onMounted(async () => {
  const platform = (
    navigator.userAgentData?.platform ||
    navigator.platform ||
    navigator.userAgent ||
    ''
  ).toLowerCase()

  if (platform.includes('mac')) os.value = 'mac'
  else if (platform.includes('win')) os.value = 'win'
  else if (platform.includes('linux') || platform.includes('x11')) os.value = 'linux'

  const cacheKey = 'netassistant-latest-release'
  try {
    const cached = sessionStorage.getItem(cacheKey)
    if (cached) {
      release.value = JSON.parse(cached)
      return
    }
  } catch {
    // 忽略存储不可用
  }

  loading.value = true
  const controller = new AbortController()
  const timer = window.setTimeout(() => controller.abort(), 5000)
  try {
    const response = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, {
      signal: controller.signal
    })
    if (response.ok) {
      release.value = await response.json()
      try {
        sessionStorage.setItem(cacheKey, JSON.stringify(release.value))
      } catch {
        // 忽略存储不可用
      }
    }
  } catch {
    // 版本信息获取失败或超时都不影响下载，链接仍然可用
  } finally {
    window.clearTimeout(timer)
    loading.value = false
  }
})
</script>

<template>
  <section class="download-hero">
    <p class="download-hero__eyebrow">{{ texts.eyebrow }}</p>
    <h2 class="download-hero__title">{{ texts.title }}</h2>
    <p class="download-hero__lead">{{ texts.lead }}</p>

    <div class="download-hero__panel">
      <template v-if="primary">
        <p class="download-hero__hint">{{ texts.recommended }} · {{ primary.label }}</p>
        <a class="download-hero__primary" :href="assetUrl(primary.file)" rel="noopener">
          {{ texts.download }} {{ primary.label }}
        </a>
        <p class="download-hero__meta">
          <span>{{ primary.note }}</span>
          <span v-if="version">{{ texts.version }} {{ version }}</span>
          <span v-if="published">{{ texts.published }} {{ published }}</span>
          <span v-if="primarySize">{{ texts.size }} {{ primarySize }}</span>
          <span v-if="loading">{{ texts.loading }}</span>
        </p>
      </template>
      <template v-else>
        <p class="download-hero__hint">{{ texts.lead }}</p>
        <a class="download-hero__primary" :href="latestPage" rel="noopener">{{ texts.all }}</a>
      </template>

      <div v-if="os === 'win' || os === 'other'" class="download-hero__winget">
        <span>{{ texts.winget }}</span>
        <code>winget install SunJary.NetAssistant</code>
        <button type="button" @click="copyWinget">{{ copied ? texts.copied : texts.copy }}</button>
      </div>
    </div>

    <p v-if="!primary && loading" class="download-hero__loading">{{ texts.loading }}</p>

    <div class="download-hero__grid">
      <div v-for="platform in platforms" :key="platform.name" class="download-hero__card">
        <h3>{{ platform.name }}</h3>
        <p>{{ platform.note }}</p>
        <a :href="assetUrl(platform.file)" rel="noopener">{{ texts.download }} →</a>
        <div class="download-hero__extra">
          <a v-for="item in platform.extra" :key="item.file" :href="assetUrl(item.file)" rel="noopener">
            {{ item.label }}
          </a>
        </div>
      </div>
    </div>

    <p class="download-hero__slow">{{ texts.slow }}</p>
  </section>
</template>
