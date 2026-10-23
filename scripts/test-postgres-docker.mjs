import { randomBytes } from 'node:crypto'
import { spawn, spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const cwd = fileURLToPath(new URL('..', import.meta.url))
const project = `rustpress-integration-${randomBytes(6).toString('hex')}`
const env = {
  ...process.env,
  TEST_POSTGRES_PASSWORD: randomBytes(32).toString('hex'),
  TEST_POSTGRES_PORT: '0',
}
const probe = spawnSync('docker', ['version', '--format', '{{.Server.Version}}'], { cwd, encoding: 'utf8' })
const useWsl = process.platform === 'win32' && probe.error?.code === 'ENOENT'
if (!useWsl && (probe.error || probe.status !== 0)) {
  throw new Error(`Docker is unavailable: ${probe.error?.message ?? probe.stderr.trim()}`)
}
if (useWsl) {
  env.WSLENV = [env.WSLENV, 'TEST_POSTGRES_PASSWORD', 'TEST_POSTGRES_PORT'].filter(Boolean).join(':')
}
const composeArgs = ['compose', '-f', 'compose.integration.yaml', '-p', project]

function run(command, args, { capture = false, ...options } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env, stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit', ...options })
    let output = ''
    child.stdout?.on('data', chunk => { output += chunk })
    child.once('error', reject)
    child.once('exit', code => {
      if (code !== 0) reject(new Error(`${command} failed (exit ${code})`))
      else resolve(output.trim())
    })
  })
}

function compose(args, options) {
  return useWsl
    ? run('wsl', ['--exec', 'docker', ...composeArgs, ...args], options)
    : run('docker', [...composeArgs, ...args], options)
}

try {
  await compose(['up', '-d', '--wait', '--wait-timeout', '120'])
  const binding = await compose(['port', 'db', '5432'], { capture: true })
  const match = /^127\.0\.0\.1:(\d+)$/.exec(binding)
  if (!match) throw new Error(`Unexpected test database binding: ${binding}`)
  await run('cargo', ['test', '--locked', '--test', 'postgres', '--', '--ignored', '--test-threads=1'], {
    env: {
      ...env,
      TEST_DATABASE_URL: `postgres://rustpress_test:${env.TEST_POSTGRES_PASSWORD}@127.0.0.1:${match[1]}/rustpress_test`,
    },
  })
  console.log('Docker PostgreSQL 18.6 integration tests passed.')
} finally {
  await compose(['down', '--remove-orphans'])
}
