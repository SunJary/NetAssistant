<script setup>
import DefaultTheme from 'vitepress/theme'
import DownloadCta from './DownloadCta.vue'

const { Layout } = DefaultTheme
</script>

<template>
  <Layout>
    <template #doc-footer-before>
      <DownloadCta />
    </template>
  </Layout>
</template>
