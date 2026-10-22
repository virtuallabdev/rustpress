export default defineEventHandler(async (event) => {
  const url = getRequestURL(event)
  if (!url.pathname.startsWith('/api/v1/public/') && url.pathname !== '/api/v1/openapi.json') {
    throw createError({ statusCode: 404 })
  }
  const config = useRuntimeConfig(event)
  return proxyRequest(event, `${config.apiInternal}${url.pathname}${url.search}`, {
    headers: { cookie: '', authorization: '' },
  })
})
