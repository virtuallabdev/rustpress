import tailwindcss from '@tailwindcss/vite'

export default defineNuxtConfig({
  compatibilityDate: '2026-10-22',
  devtools: { enabled: false },
  telemetry: false,
  css: ['~/assets/main.css'],
  vite: { plugins: [tailwindcss()] },
  typescript: { strict: true },
  runtimeConfig: { apiInternal: 'http://127.0.0.1:8080' },
  app: { head: { htmlAttrs: { lang: 'it' }, title: 'RustPress | Amministrazione', meta: [{ name: 'robots', content: 'noindex,nofollow' }] } },
  routeRules: {
    '/**': { headers: { 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'no-referrer', 'X-Frame-Options': 'DENY' } },
  },
})
