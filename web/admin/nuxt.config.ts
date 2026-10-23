import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath } from 'node:url'

export default defineNuxtConfig({
  compatibilityDate: '2026-10-22',
  devtools: { enabled: false },
  telemetry: false,
  modules: ['@nuxtjs/i18n'],
  i18n: {
    defaultLocale: 'it',
    strategy: 'no_prefix',
    locales: [{ code: 'it', language: 'it-IT', name: 'Italiano' }, { code: 'en', language: 'en-GB', name: 'English' }],
    vueI18n: fileURLToPath(new URL('../shared/i18n.config.ts', import.meta.url)),
    detectBrowserLanguage: false,
  },
  css: ['~/assets/main.css', fileURLToPath(new URL('../shared/markdown.css', import.meta.url))],
  vite: { plugins: [tailwindcss()] },
  typescript: { strict: true },
  runtimeConfig: { apiInternal: 'http://127.0.0.1:8080' },
  app: { head: { htmlAttrs: { lang: 'it' }, title: 'RustPress | Amministrazione', meta: [{ name: 'robots', content: 'noindex,nofollow' }] } },
  routeRules: {
    '/**': { headers: { 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'no-referrer', 'X-Frame-Options': 'DENY' } },
  },
})
