import DefaultTheme from 'vitepress/theme'
import Layout from './Layout.vue'
import DownloadHero from './DownloadHero.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  Layout,
  enhanceApp({ app }: { app: any }) {
    app.component('DownloadHero', DownloadHero)
  }
}
