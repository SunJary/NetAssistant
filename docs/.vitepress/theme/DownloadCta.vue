<script setup>
import { computed } from 'vue'
import { useData, withBase } from 'vitepress'

const { page, lang } = useData()

const isEn = computed(() => lang.value.startsWith('en'))
const downloadHref = computed(() => withBase(isEn.value ? '/en/download' : '/download'))
const hidden = computed(() => {
  const path = page.value.relativePath || ''
  return page.value.isHome || path === 'download.md' || path === 'en/download.md'
})
</script>

<template>
  <aside v-if="!hidden" class="download-cta">
    <div>
      <p class="download-cta__title">
        {{ isEn ? 'Try it in your next debugging session' : '在下一次调试中试试 NetAssistant' }}
      </p>
      <p class="download-cta__desc">
        {{
          isEn
            ? 'Free and open source, about 20 MB. Windows, macOS and Linux, no signup required.'
            : '免费开源，约 20MB。支持 Windows、macOS 与 Linux，无需注册。'
        }}
      </p>
    </div>
    <a class="download-cta__button" :href="downloadHref">
      {{ isEn ? 'Download' : '立即下载' }}
    </a>
  </aside>
</template>
