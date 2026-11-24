<script setup>
import { computed } from 'vue'
import { useData, withBase } from 'vitepress'

const { page, lang } = useData()

const isZh = computed(() => lang.value.startsWith('zh'))
const downloadHref = computed(() => withBase(isZh.value ? '/zh/download' : '/download'))
const hidden = computed(() => {
  const path = page.value.relativePath || ''
  return page.value.isHome || path === 'download.md' || path === 'zh/download.md'
})
</script>

<template>
  <aside v-if="!hidden" class="download-cta">
    <div>
      <p class="download-cta__title">
        {{ isZh ? '在下一次调试中试试 NetAssistant' : 'Try it in your next debugging session' }}
      </p>
      <p class="download-cta__desc">
        {{
          isZh
            ? '免费开源，约 20MB。支持 Windows、macOS 与 Linux，无需注册。'
            : 'Free and open source, about 20 MB. Windows, macOS and Linux, no signup required.'
        }}
      </p>
    </div>
    <a class="download-cta__button" :href="downloadHref">
      {{ isZh ? '立即下载' : 'Download' }}
    </a>
  </aside>
</template>
