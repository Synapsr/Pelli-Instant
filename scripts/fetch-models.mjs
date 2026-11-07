#!/usr/bin/env node
// Download the models of Pelli Instant listed in models/manifest.json into
// models/ (or --dir), verifying sizes and SHA-256 checksums. Idempotent: a
// file already present with the right checksum is left alone.
//
// Node >= 20, no dependencies. Honours HTTPS_PROXY / https_proxy (CONNECT
// tunnel) and NO_PROXY; extra CAs via NODE_EXTRA_CA_CERTS.
//
//   node scripts/fetch-models.mjs            # models of the desktop bundle
//   node scripts/fetch-models.mjs --gpu      # + fp16 CLIP for DirectML/CUDA
//   node scripts/fetch-models.mjs --maximum  # + the Maximum pack, into <dir>/maximum (development:
//                                            #   the app downloads it into its data folder itself)
//   node scripts/fetch-models.mjs --all      # everything in the manifest
//   node scripts/fetch-models.mjs --only yunet,face-mesh
//   node scripts/fetch-models.mjs --check    # verify only (exit 1 when missing/corrupt)
//   node scripts/fetch-models.mjs --dir /path/to/models
//   node scripts/fetch-models.mjs --strict   # an optional model that cannot be fetched is an error
//                                            #   (installers); otherwise a warning

import { createHash } from 'node:crypto'
import {
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import http from 'node:http'
import https from 'node:https'
import path from 'node:path'
import { pipeline } from 'node:stream/promises'
import tls from 'node:tls'
import { fileURLToPath } from 'node:url'
import zlib from 'node:zlib'

const here = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(here, '..')
const manifestPath = path.join(root, 'models', 'manifest.json')

function parseArgs(argv) {
  const opts = {
    dir: path.join(root, 'models'),
    gpu: false,
    maximum: false,
    all: false,
    only: null,
    check: false,
    strict: false,
  }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a === '--gpu') opts.gpu = true
    else if (a === '--maximum') opts.maximum = true
    else if (a === '--strict') opts.strict = true
    else if (a === '--all') opts.all = true
    else if (a === '--check') opts.check = true
    else if (a === '--dir') opts.dir = path.resolve(argv[++i] ?? '.')
    else if (a === '--only') opts.only = (argv[++i] ?? '').split(',').filter(Boolean)
    else if (a === '-h' || a === '--help') {
      console.log('usage: fetch-models.mjs [--gpu] [--maximum] [--all] [--only id,id] [--check] [--strict] [--dir DIR]')
      process.exit(0)
    } else {
      console.error(`unknown argument ${a}`)
      process.exit(2)
    }
  }
  return opts
}

// ---------------------------------------------------------------------------
// HTTP(S) with optional CONNECT proxy and redirects
// ---------------------------------------------------------------------------

function proxyFor(url) {
  const proxy = process.env.HTTPS_PROXY || process.env.https_proxy
  if (!proxy) return null
  const noProxy = (process.env.NO_PROXY || process.env.no_proxy || '')
    .split(',')
    .map((s) => s.trim().replace(/^\*?\./, ''))
    .filter(Boolean)
  const host = url.hostname
  if (noProxy.some((n) => host === n || host.endsWith('.' + n))) return null
  return new URL(proxy.includes('://') ? proxy : `http://${proxy}`)
}

function tunnel(proxy, url) {
  return new Promise((resolve, reject) => {
    const port = url.port || 443
    const headers = { Host: `${url.hostname}:${port}` }
    if (proxy.username) {
      const cred = `${decodeURIComponent(proxy.username)}:${decodeURIComponent(proxy.password)}`
      headers['Proxy-Authorization'] = `Basic ${Buffer.from(cred).toString('base64')}`
    }
    const req = http.request({
      host: proxy.hostname,
      port: proxy.port || 80,
      method: 'CONNECT',
      path: `${url.hostname}:${port}`,
      headers,
    })
    req.once('connect', (res, socket) => {
      if (res.statusCode !== 200) {
        socket.destroy()
        reject(new Error(`proxy CONNECT ${url.hostname} -> ${res.statusCode}`))
        return
      }
      resolve(tls.connect({ socket, servername: url.hostname }))
    })
    req.once('error', reject)
    req.end()
  })
}

async function get(urlString, redirects = 0) {
  const url = new URL(urlString)
  if (url.protocol !== 'https:') throw new Error(`refusing non-https URL ${urlString}`)
  const proxy = proxyFor(url)
  const socket = proxy ? await tunnel(proxy, url) : null
  const res = await new Promise((resolve, reject) => {
    const req = https.request(url, {
      headers: { 'User-Agent': 'pelli-cull-fetch-models/1 (+https://pelli.io)' },
      ...(socket ? { createConnection: () => socket, agent: false } : {}),
    })
    req.once('response', resolve)
    req.once('error', reject)
    req.setTimeout(120_000, () => req.destroy(new Error('timeout')))
    req.end()
  })
  if ([301, 302, 303, 307, 308].includes(res.statusCode)) {
    res.resume()
    if (redirects > 8 || !res.headers.location) throw new Error(`too many redirects for ${urlString}`)
    return get(new URL(res.headers.location, url).toString(), redirects + 1)
  }
  if (res.statusCode !== 200) {
    res.resume()
    throw new Error(`HTTP ${res.statusCode} for ${url.origin}${url.pathname}`)
  }
  return res
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

async function sha256File(file) {
  const h = createHash('sha256')
  await pipeline(createReadStream(file), h)
  return h.digest('hex')
}

async function verify(file, size, sha256) {
  if (!existsSync(file)) return 'missing'
  if (statSync(file).size !== size) return 'wrong size'
  return (await sha256File(file)) === sha256 ? 'ok' : 'wrong checksum'
}

function fmtMB(bytes) {
  return `${(bytes / 1e6).toFixed(1)} MB`
}

async function download(urls, dest, size, sha256) {
  let lastErr
  for (const url of urls) {
    const part = `${dest}.part`
    try {
      const res = await get(url)
      const h = createHash('sha256')
      let got = 0
      let lastPct = -10
      res.on('data', (chunk) => {
        h.update(chunk)
        got += chunk.length
        const pct = Math.floor((got / size) * 100)
        if (process.stdout.isTTY && pct >= lastPct + 10) {
          lastPct = pct
          process.stdout.write(`\r    ${path.basename(dest)} ${pct}%`)
        }
      })
      await pipeline(res, createWriteStream(part))
      if (process.stdout.isTTY) process.stdout.write('\n')
      const digest = h.digest('hex')
      if (got !== size) throw new Error(`size ${got} instead of ${size}`)
      if (digest !== sha256) throw new Error(`sha256 ${digest} instead of ${sha256}`)
      renameSync(part, dest)
      return
    } catch (e) {
      rmSync(part, { force: true })
      lastErr = e
      console.warn(`    ${url}: ${e.message}`)
    }
  }
  throw lastErr ?? new Error('no URL')
}

// Extract one regular file of a .tar.gz (ustar, GNU long names, pax paths).
// Archives here are small (tens of MB): parsed in memory.
function extractTarMember(archive, member, dest) {
  const tar = zlib.gunzipSync(readFileSync(archive))
  let pos = 0
  let longName = null
  while (pos + 512 <= tar.length) {
    const header = tar.subarray(pos, pos + 512)
    pos += 512
    if (header.every((b) => b === 0)) continue
    const str = (a, b) => header.subarray(a, b).toString('utf8').replace(/\0.*$/s, '')
    const size = parseInt(str(124, 136).trim() || '0', 8)
    if (!Number.isFinite(size) || size < 0) throw new Error('corrupt tar header')
    const type = String.fromCharCode(header[156] || 48)
    const data = tar.subarray(pos, pos + size)
    pos += Math.ceil(size / 512) * 512
    if (type === 'L') {
      longName = data.toString('utf8').replace(/\0.*$/s, '')
      continue
    }
    if (type === 'x') {
      const m = data.toString('utf8').match(/\d+ path=([^\n]*)\n/)
      if (m) longName = m[1]
      continue
    }
    let name = str(0, 100)
    const prefix = str(345, 500)
    if (prefix) name = `${prefix}/${name}`
    if (longName) {
      name = longName
      longName = null
    }
    name = name.replace(/^\.\//, '')
    if ((type === '0' || type === '\0') && (name === member || name.endsWith(`/${member}`))) {
      if (data.length !== size) throw new Error('truncated tar')
      writeFileSync(dest, data)
      return
    }
  }
  throw new Error(`${member} not found in ${path.basename(archive)}`)
}

async function fetchModel(m, dir) {
  const dest = path.join(dir, m.file)
  const state = await verify(dest, m.size, m.sha256)
  if (state === 'ok') {
    console.log(`  ok        ${m.file} (${fmtMB(m.size)}, ${m.license})`)
    return
  }
  if (state !== 'missing') console.log(`  ${state}: re-downloading ${m.file}`)
  const urls = [m.url, ...(m.mirrors ?? [])]
  console.log(`  download  ${m.file} (${fmtMB(m.archive?.size ?? m.size)}, ${m.license})`)
  if (m.archive) {
    const archive = path.join(dir, `.${m.id}.${m.archive.format.replace(/\W/g, '')}`)
    if ((await verify(archive, m.archive.size, m.archive.sha256)) !== 'ok') {
      await download(urls, archive, m.archive.size, m.archive.sha256)
    }
    const part = `${dest}.part`
    try {
      extractTarMember(archive, m.archive.member, part)
      const got = await verify(part, m.size, m.sha256)
      if (got !== 'ok') throw new Error(`${m.archive.member} in archive: ${got}`)
      renameSync(part, dest)
    } finally {
      rmSync(part, { force: true })
    }
    rmSync(archive, { force: true })
  } else {
    await download(urls, dest, m.size, m.sha256)
  }
}

async function main() {
  const opts = parseArgs(process.argv.slice(2))
  const { default: manifest } = await import(manifestPath, { with: { type: 'json' } })
  const selected = manifest.models.filter((m) => {
    if (opts.only) return opts.only.includes(m.id)
    if (opts.all) return true
    return m.bundle === 'default' || (opts.gpu && m.bundle === 'gpu') || (opts.maximum && m.bundle === 'maximum')
  })
  if (opts.only) {
    const unknown = opts.only.filter((id) => !manifest.models.some((m) => m.id === id))
    if (unknown.length) {
      console.error(`unknown model id(s): ${unknown.join(', ')}`)
      process.exit(2)
    }
  }
  mkdirSync(opts.dir, { recursive: true })
  console.log(`${opts.check ? 'Checking' : 'Fetching'} ${selected.length} model(s) into ${opts.dir}`)
  let failed = 0
  for (const m of selected) {
    // The Maximum pack lives in its own folder, like in the app's data dir.
    const dir = m.bundle === 'maximum' ? path.join(opts.dir, 'maximum') : opts.dir
    mkdirSync(dir, { recursive: true })
    try {
      if (opts.check) {
        const state = await verify(path.join(dir, m.file), m.size, m.sha256)
        console.log(`  ${state.padEnd(14)} ${m.file}`)
        if (state !== 'ok' && (m.required || !opts.only)) failed++
      } else {
        await fetchModel(m, dir)
      }
    } catch (e) {
      if (m.required || opts.strict || opts.only) {
        failed++
        console.error(`  FAILED    ${m.file}: ${e.message}`)
      } else {
        // Optional: the app works without it (see models/README.md).
        console.warn(`  SKIPPED   ${m.file} (optional): ${e.message}`)
      }
    }
  }
  if (failed) {
    console.error(`${failed} model(s) missing or invalid`)
    process.exit(1)
  }
  console.log('done')
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
