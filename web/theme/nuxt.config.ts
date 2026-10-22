import tailwindcss from '@tailwindcss/vite'

export default defineNuxtConfig({
  compatibilityDate: '2026-10-22',
  devtools: { enabled: false },
  telemetry: false,
  css: ['~/assets/main.css'],
  vite: { plugins: [tailwindcss()] },
  typescript: { strict: true },
  runtimeConfig: { apiInternal: 'http://127.0.0.1:8080', public: { siteOrigin: 'http://localhost:3000' } },
  app: { head: { htmlAttrs: { lang: 'it' }, title: 'RustPress' } },
  routeRules: {
    '/**': { headers: { 'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'strict-origin-when-cross-origin', 'X-Frame-Options': 'DENY' } },
  },
})
