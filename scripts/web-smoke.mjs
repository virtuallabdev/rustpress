import assert from 'node:assert/strict'
import { randomBytes, createHash } from 'node:crypto'
import { spawn } from 'node:child_process'
import { createServer } from 'node:net'
import { join } from 'node:path'
import { rmdir } from 'node:fs/promises'
import { setTimeout as delay } from 'node:timers/promises'

async function freePort() {
  const server = createServer()
  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolve)
  })
  const address = server.address()
  if (!address || typeof address === 'string') throw new Error('No TCP port allocated')
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()))
  return address.port
}

export async function webSmoke({ databaseUrl, client, mediaDir, browser }) {
  const [apiPort, adminPort, themePort] = await Promise.all([freePort(), freePort(), freePort()])
  const api = `http://127.0.0.1:${apiPort}`
  const admin = `http://localhost:${adminPort}`
  const theme = `http://localhost:${themePort}`
  const children = []
  const logs = []
  function start(command, args, env) {
    const child = spawn(command, args, { env: { ...process.env, ...env }, stdio: ['ignore', 'pipe', 'pipe'] })
    child.stdout.on('data', data => logs.push(data.toString()))
    child.stderr.on('data', data => logs.push(data.toString()))
    child.on('error', error => logs.push(error.message))
    children.push(child)
  }
  async function ready(url) {
    for (let attempt = 0; attempt < 100; attempt++) {
      if (children.some(child => child.exitCode !== null)) throw new Error(`Child process exited: ${logs.join('\n')}`)
      try {
        if ((await fetch(url)).ok) return
      } catch (error) {
        if (!(error instanceof TypeError)) throw error
      }
      await delay(100)
    }
    throw new Error(`Service did not become ready: ${url}\n${logs.join('\n')}`)
  }
  let connected = false
  try {
    start(join(process.cwd(), 'target', 'debug', process.platform === 'win32' ? 'rustpress.exe' : 'rustpress'), [], {
      DATABASE_URL: databaseUrl, RUSTPRESS_ENV: 'development', ADMIN_ORIGIN: admin, SITE_ORIGIN: theme,
      BIND_ADDR: `127.0.0.1:${apiPort}`, MEDIA_DIR: mediaDir,
      GOOGLE_CLIENT_ID: '', GOOGLE_CLIENT_SECRET: '', MICROSOFT_CLIENT_ID: '', MICROSOFT_CLIENT_SECRET: '',
      META_CLIENT_ID: '', META_CLIENT_SECRET: '',
    })
    for (const [app, port] of [['admin', adminPort], ['theme', themePort]]) {
      start(process.execPath, [join('web', app, '.output', 'server', 'index.mjs')], {
        HOST: '127.0.0.1', PORT: String(port), NUXT_API_INTERNAL: api, NUXT_PUBLIC_SITE_ORIGIN: theme,
      })
    }
    await ready(`${api}/health/ready`)
    await Promise.all([ready(admin), ready(theme)])
    await client.connect()
    connected = true
    const token = randomBytes(32).toString('hex')
    await client.query("INSERT INTO credentials(token_hash,role,label,expires_at) VALUES ($1,'admin','web-smoke',now()+interval '5 minutes')",
      [createHash('sha256').update(token).digest('hex')])
    const body = {
      content_type: 'page', slug: 'web-smoke', title: 'Smoke <script>alert("xss")</script>',
      blocks: [{ kind: 'paragraph', text: '<img src=x onerror=alert(1)> Safe text' }],
      fields: { subtitle: 'Smoke subtitle' }, status: 'published',
      publish_at: '2026-01-01T00:00:00Z', seo: { description: 'Smoke description', noindex: false },
    }
    const saved = await fetch(`${admin}/api/v1/contents`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', Cookie: `rp_session=${token}`, Origin: admin },
      body: JSON.stringify(body),
    })
    assert.equal(saved.status, 201, await saved.text())
    const pageResponse = await fetch(`${theme}/web-smoke`)
    assert.equal(pageResponse.status, 200)
    const html = await pageResponse.text()
    assert.ok(html.includes('Smoke &lt;script&gt;'), 'Title must be escaped in rendered HTML')
    assert.ok(!html.includes('<img src=x onerror='), 'Content must never become raw HTML')
    assert.ok(html.includes('application/ld+json'), 'Structured metadata is required')
    assert.ok(html.includes(`href="${theme}/web-smoke"`), 'Canonical must use configured public origin')
    assert.equal((await fetch(`${theme}/api/v1/contents`)).status, 404, 'Public theme must not proxy editorial API')
    const csrf = await fetch(`${admin}/api/v1/auth/logout`, { method: 'POST', headers: { Cookie: `rp_session=${token}`, Origin: 'https://evil.test' } })
    assert.equal(csrf.status, 403, 'Admin proxy must preserve Origin for CSRF checks')
    const session = await fetch(`${admin}/api/v1/auth/me`, { headers: { Cookie: `rp_session=${token}` } })
    assert.equal(session.status, 200, 'Admin proxy must preserve session cookies')
    const sitemap = await fetch(`${theme}/sitemap.xml`)
    assert.equal(sitemap.status, 200)
    assert.ok((await sitemap.text()).includes(`${theme}/web-smoke`))
    const loginPage = await fetch(admin)
    assert.equal(loginPage.status, 200)
    assert.equal(loginPage.headers.get('cache-control'), 'no-store')
    console.log('Web smoke passed: real Axum + PostgreSQL + production Nuxt SSR, session proxy, CSRF, XSS escaping, sitemap and public API isolation.')
    if (browser) {
      console.log(`Browser preview (120 seconds): admin=${admin} theme=${theme}/web-smoke`)
      await delay(120_000)
    }
  } finally {
    if (connected) await client.end()
    await Promise.all(children.map(child => new Promise(resolve => {
      if (child.exitCode !== null) return resolve()
      child.once('exit', resolve)
      child.kill()
    })))
    await rmdir(join(mediaDir, 'public'))
    await rmdir(join(mediaDir, 'originals'))
    await rmdir(mediaDir)
  }
}
