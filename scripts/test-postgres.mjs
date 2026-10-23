import EmbeddedPostgres from 'embedded-postgres'
import { randomBytes } from 'node:crypto'
import { mkdtemp, rmdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createServer } from 'node:net'
import { spawn } from 'node:child_process'
import { webSmoke } from './web-smoke.mjs'

const reservation = createServer()
await new Promise((resolve, reject) => {
  reservation.once('error', reject)
  reservation.listen(0, '127.0.0.1', resolve)
})
const address = reservation.address()
if (!address || typeof address === 'string') throw new Error('No local test port allocated')
const port = address.port
await new Promise((resolve, reject) => reservation.close(error => error ? reject(error) : resolve()))
const root = await mkdtemp(join(tmpdir(), 'rustpress-pg-'))
const password = randomBytes(32).toString('hex')
const postgres = new EmbeddedPostgres({
  databaseDir: join(root, 'cluster'),
  user: 'postgres',
  password,
  port,
  authMethod: 'scram-sha-256',
  persistent: false,
  initdbFlags: ['--encoding=UTF8', '--locale=C'],
  postgresFlags: ['-h', '127.0.0.1'],
})
let started = false
try {
  await postgres.initialise()
  await postgres.start()
  started = true
  await postgres.createDatabase('rustpress_test')
  const code = await new Promise((resolve, reject) => {
    const child = spawn('cargo', ['test', '--test', 'postgres', '--', '--ignored', '--test-threads=1'], {
      stdio: 'inherit',
      env: { ...process.env, TEST_DATABASE_URL: `postgres://postgres:${password}@127.0.0.1:${port}/rustpress_test` },
    })
    child.once('error', reject)
    child.once('exit', code => resolve(code ?? 1))
  })
  if (code !== 0) throw new Error(`PostgreSQL integration tests failed (exit ${code})`)
  if (process.argv.includes('--web')) {
    await webSmoke({
      databaseUrl: `postgres://postgres:${password}@127.0.0.1:${port}/rustpress_test`,
      client: postgres.getPgClient('rustpress_test'),
      mediaDir: join(root, 'media'),
      browser: process.argv.includes('--browser'),
      browserTest: process.argv.includes('--playwright'),
    })
  }
} finally {
  if (started) {
    await postgres.stop()
    await rmdir(root)
  } else {
    console.error(`PostgreSQL did not start. Inspect temporary files at ${root}`)
  }
}
