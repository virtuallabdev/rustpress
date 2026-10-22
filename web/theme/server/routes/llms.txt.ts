export default defineEventHandler((event) => {
  return proxyRequest(event, `${useRuntimeConfig(event).apiInternal}/llms.txt`, {
    headers: { cookie: '', authorization: '' },
  })
})
