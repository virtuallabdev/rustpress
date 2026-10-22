export default defineEventHandler(async (event) => {
  const path = getRequestURL(event).pathname
  if (!/^\/media\/[0-9a-f-]{36}\/\d{1,4}\.webp$/.test(path)) throw createError({ statusCode: 404 })
  return proxyRequest(event, `${useRuntimeConfig(event).apiInternal}${path}`)
})
