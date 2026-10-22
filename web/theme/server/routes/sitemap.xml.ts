export default defineEventHandler((event) => {
  return proxyRequest(event, `${useRuntimeConfig(event).apiInternal}/api/v1/public/sitemap.xml`, {
    headers: { cookie: '', authorization: '' },
  })
})
