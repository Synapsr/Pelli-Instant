#!/usr/bin/env node
// Fake Pelli for developing and testing Pelli Instant without the Pelli web app,
// its database, Stripe or its upload service. Zero dependencies (Node 20+).
//
//   node scripts/fake-pelli.mjs              # http://localhost:3100
//   pnpm app:fake-pelli                      # the app against it (PELLI_API_URL + test licence key)
//
// Implements the contract of docs/pelli-api.md:
// - Pelli Instant Plus: price (`plus/offer`), checkout (a fake Stripe page;
//   paid by its button, or by itself after FAKE_PELLI_PAY_AFTER polls),
//   claim, activation of a key, release, daily check, and the download URLs
//   of the Maximum pack (`models`), served from FAKE_PELLI_MODELS_DIR with
//   range requests;
// - anonymous telemetry (`telemetry`): validated as strictly as Pelli does,
//   each payload printed on stdout (`[telemetry] {...}`);
// - the optional Pelli account (sign-in by browser consent, `me` with
//   `plus`), galleries, folders, upload token and the upload service
//   (`POST /api/photo`).
//
// Licence tokens are signed with a TEST key (Ed25519 seed of 32 bytes 0x07,
// the one of the engine's tests). Debug builds accept it with
//   PELLI_LICENSE_PUBLIC_KEY=6kpsY+KcUgq+9VB7Ey7F+ZVHdq6+vnuSQh7qaRRG0iw=
// (`pnpm app:fake-pelli` sets it); release builds only trust Pelli's key.
//
// http://localhost:3100/ shows the accounts (one per plan, switch plan), the
// licences (pay, refund, release), the last telemetry events and the photos
// received, written to `<data dir>/uploads/<account>/<gallery>/<folder>/`.
//
// Environment:
//   FAKE_PELLI_PORT        port (default 3100)
//   FAKE_PELLI_DIR         state + uploads (default ./.fake-pelli)
//   FAKE_PELLI_PLAN        account signed in by FAKE_PELLI_AUTO_APPROVE:
//                          free | lite | pro | ultra | trial (default pro)
//   FAKE_PELLI_AUTO_APPROVE=1   approve sign-ins without the consent page (scripts, Playwright)
//   FAKE_PELLI_COUNTRY     country of the visitor when no `cf-ipcountry` header (default FR: euros)
//   FAKE_PELLI_PAY_AFTER   a checkout is paid at its Nth claim (default 2; 0 = only by the fake Stripe page)
//   FAKE_PELLI_MODELS_DIR  where the Maximum pack files are read (default ./models/maximum,
//                          what `node scripts/fetch-models.mjs --maximum` fills)
//   FAKE_PELLI_URL_TTL_S   life of a download URL (default 43200 = 12 h, as Pelli; a few seconds to test the refresh)
//   FAKE_PELLI_UPLOAD_FAIL share of uploads answered 500 (0..1, retry testing)
//   FAKE_PELLI_STORAGE_GB  storage limit of each account (default 50)

import { createHash, createPrivateKey, createPublicKey, randomBytes, randomUUID, sign, verify } from 'node:crypto'
import { createReadStream, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import { basename, dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const PORT = Number(process.env.FAKE_PELLI_PORT || 3100)
const BASE = `http://localhost:${PORT}`
const DIR = resolve(process.env.FAKE_PELLI_DIR || '.fake-pelli')
const STATE_FILE = join(DIR, 'state.json')
const STATE_VERSION = 3
const AUTO_APPROVE = process.env.FAKE_PELLI_AUTO_APPROVE === '1'
const COUNTRY = (process.env.FAKE_PELLI_COUNTRY || 'FR').toUpperCase()
const PAY_AFTER = Math.max(0, Number.parseInt(process.env.FAKE_PELLI_PAY_AFTER ?? '2', 10) || 0)
const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const MODELS_DIR = resolve(process.env.FAKE_PELLI_MODELS_DIR || join(REPO, 'models', 'maximum'))
const URL_TTL_MS = Math.max(1, Number(process.env.FAKE_PELLI_URL_TTL_S || 43200)) * 1000
const UPLOAD_FAIL = Math.min(1, Math.max(0, Number(process.env.FAKE_PELLI_UPLOAD_FAIL || 0)))
const STORAGE_LIMIT = Math.round(Number(process.env.FAKE_PELLI_STORAGE_GB || 50) * 1024 ** 3)
const MAX_FILE_SIZE = 100 * 1024 * 1024
const SESSION_TTL_S = 600
const TRIAL_DAYS = 15
/** A checkout nobody paid expires (Stripe: 24 h; shorter here). */
const CHECKOUT_TTL_MS = 60 * 60 * 1000

/** Plans as Pelli resolves them; Pro and Ultra (trials and lifetime included) include Plus. */
const PLANS = {
  free: { plan: 'free', label: { fr: 'Gratuit', en: 'Free' }, plus: false, sendToGallery: false },
  lite: { plan: 'lite', label: { fr: 'Lite', en: 'Lite' }, plus: false, sendToGallery: true },
  pro: { plan: 'pro', label: { fr: 'Pro', en: 'Pro' }, plus: true, sendToGallery: true },
  ultra: { plan: 'ultra', label: { fr: 'Ultra', en: 'Ultra' }, plus: true, sendToGallery: true },
  trial: { plan: 'ultra', label: { fr: 'Essai Ultra', en: 'Ultra trial' }, plus: true, sendToGallery: true, trial: true },
}
const DEFAULT_PLAN = PLANS[process.env.FAKE_PELLI_PLAN] ? process.env.FAKE_PELLI_PLAN : 'pro'

mkdirSync(join(DIR, 'uploads'), { recursive: true })

/** Persistent state: survives restarts so the app's token and licence stay valid. */
const state = (() => {
  const initial = { version: STATE_VERSION, accounts: {}, tokens: {}, galleries: [], folders: [], requests: {}, licenses: {} }
  try {
    const saved = JSON.parse(readFileSync(STATE_FILE, 'utf8'))
    if (saved.version === STATE_VERSION) return { ...initial, ...saved }
    console.log('[fake-pelli] state of an older fake Pelli ignored: sign in again')
  } catch {}
  return initial
})()
const save = () => writeFileSync(STATE_FILE, JSON.stringify(state, null, 2))
save()

/** Sign-in sessions live in memory only (10 minutes, like Pelli). */
const sessions = new Map()
/** The last telemetry events (memory only), for the overview page. */
const telemetryLog = []

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const MESSAGES = {
  invalid_request: 'The request body is invalid.',
  missing_token: 'Authorization header missing.',
  invalid_token: 'Invalid or expired token.',
  subscription_required: 'This account has no active Pelli subscription.',
  gallery_not_found: 'Gallery not found.',
  folder_not_found: 'Folder not found.',
  folder_exists: 'A folder with this name already exists.',
  request_conflict: 'requestId reused with different values.',
  not_found: 'Not found.',
  file_too_large: 'File too large.',
  server_error: 'Simulated server error.',
  missing_authorization: 'Authorization header missing.',
  rate_limited: 'Too many requests.',
  invalidKey: 'This licence key does not exist.',
  otherDevice: 'This licence is active on another computer: release it there first.',
  revoked: 'This licence was refunded.',
  invalidLicense: 'Invalid licence token.',
  plusRequired: 'The Maximum pack is part of Pelli Instant Plus.',
  expired: 'This download link expired.',
}

function send(res, status, body, headers = {}) {
  const text = typeof body === 'string' ? body : JSON.stringify(body)
  res.writeHead(status, {
    'Content-Type': typeof body === 'string' ? 'text/html; charset=utf-8' : 'application/json',
    ...headers,
  })
  res.end(text)
}

/** Pelli's error envelope `{ code, message, error }`, plus `extra` fields. */
function fail(res, status, code, extra = {}) {
  const message = MESSAGES[code] ?? code
  send(res, status, { code, message, error: message, ...extra })
}

function readBody(req, limit = MAX_FILE_SIZE + 1024 * 1024) {
  return new Promise((done, reject) => {
    const chunks = []
    let size = 0
    req.on('data', (c) => {
      size += c.length
      if (size > limit) {
        reject(Object.assign(new Error('too large'), { code: 'file_too_large' }))
        req.destroy()
      } else chunks.push(c)
    })
    req.on('end', () => done(Buffer.concat(chunks)))
    req.on('error', reject)
  })
}

async function readJson(req) {
  const raw = (await readBody(req, 1024 * 1024)).toString('utf8')
  if (!raw.trim()) return {}
  return JSON.parse(raw)
}

async function readForm(req) {
  return new URLSearchParams((await readBody(req, 64 * 1024)).toString('utf8'))
}

const b64url = (buf) => Buffer.from(buf).toString('base64url')
const sha256 = (s) => createHash('sha256').update(s).digest('hex')
// As in Pelli: the browser's consent URL carries a public id derived one way
// from the app's secret `sessionId`, so a leaked URL cannot poll for the token.
const consentIdOf = (sessionId) => `cull_${b64url(createHash('sha256').update(`pelli-instant/consent:${sessionId}`).digest())}`
const fold = (s) => s.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase().trim()
const slugify = (s) => fold(s).replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'galerie'
const escapeHtml = (s) => String(s).replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`)
const isUuid = (s) => typeof s === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s)
const isDevice = (s) => typeof s === 'string' && /^[0-9a-f]{64}$/.test(s)
/** `fr` for French, else English (Pelli localises in five languages). */
const langOf = (value) => (String(value || '').toLowerCase().startsWith('fr') ? 'fr' : 'en')

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

/** The test account `key` (one per plan), created on first use. */
function accountFor(key) {
  if (!state.accounts[key]) {
    state.accounts[key] = { key, createdAt: Date.now(), hadTrial: false }
    setPlan(state.accounts[key], key)
  }
  return state.accounts[key]
}

function setPlan(acc, plan) {
  acc.plan = plan
  acc.trialEndsAt = PLANS[plan].trial ? Date.now() + TRIAL_DAYS * 86_400_000 : null
  acc.hadTrial ||= Boolean(PLANS[plan].trial)
}

function accountView(acc, lang) {
  const p = PLANS[acc.plan]
  return {
    name: 'Camille Test',
    email: `camille+${acc.key}@example.test`,
    slug: `camille-${acc.key}`,
    avatarUrl: null,
    plan: p.plan,
    planLabel: p.label[lang],
    trialEndsAt: acc.trialEndsAt,
    // A trial is offered once, to accounts without subscription.
    trialAvailable: !acc.hadTrial && acc.plan === 'free',
    lifetime: false,
    plus: p.plus,
    // Sorting is unlimited for everyone; the fields stay for 0.1 apps.
    entitlements: { analyze: true, monthlyQuota: null, usedThisMonth: 0, resetsAt: 0, sendToGallery: p.sendToGallery },
  }
}

/** The account behind the bearer token, or `null` once the error is sent. */
function bearer(req, res) {
  const auth = req.headers.authorization || ''
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : null
  if (!token) return fail(res, 401, 'missing_token'), null
  const grant = state.tokens[token]
  if (!grant?.account || !state.accounts[grant.account]) return fail(res, 401, 'invalid_token'), null
  return state.accounts[grant.account]
}

// ---------------------------------------------------------------------------
// Pelli Instant Plus: licences signed with the test key
// ---------------------------------------------------------------------------

const TEST_SEED = Buffer.alloc(32, 7)
const PRIVATE_KEY = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), TEST_SEED]),
  format: 'der',
  type: 'pkcs8',
})
const PUBLIC_KEY_B64 = createPublicKey(PRIVATE_KEY).export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64')
const PUBLIC_KEY = createPublicKey(PRIVATE_KEY)

/** `base64url(payload) "." base64url(Ed25519 signature of the first part)`. */
function licenseToken(lic) {
  const first = b64url(JSON.stringify({ v: 1, lid: lic.id, dev: lic.device, iat: Math.floor(Date.now() / 1000) }))
  return `${first}.${b64url(sign(null, Buffer.from(first), PRIVATE_KEY))}`
}

/** The claims of a token signed here, or null. */
function readToken(token) {
  if (typeof token !== 'string') return null
  const [first, signature, extra] = token.split('.')
  if (!first || !signature || extra !== undefined) return null
  try {
    if (!verify(null, Buffer.from(first), PUBLIC_KEY, Buffer.from(signature, 'base64url'))) return null
    const claims = JSON.parse(Buffer.from(first, 'base64url').toString('utf8'))
    return claims.v === 1 ? claims : null
  } catch {
    return null
  }
}

const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ'

/** `XXXX-XXXX-XXXX-XXXX`: 80 random bits in Crockford base32, unique. */
function newKey() {
  for (;;) {
    const bytes = randomBytes(10)
    let bits = 0n
    for (const b of bytes) bits = (bits << 8n) | BigInt(b)
    let key = ''
    for (let i = 0; i < 16; i++) key = CROCKFORD[Number((bits >> BigInt(5 * i)) & 31n)] + key
    key = key.match(/.{4}/g).join('-')
    if (!Object.values(state.licenses).some((l) => l.key === key)) return key
  }
}

/** As typed: case ignored, spaces and dashes dropped, I/L → 1, O → 0. */
function normalizeKey(input) {
  const raw = String(input ?? '')
    .toUpperCase()
    .replace(/[\s\-‐–—]/g, '')
    .replace(/O/g, '0')
    .replace(/[IL]/g, '1')
  return /^[0-9A-HJKMNP-TV-Z]{16}$/.test(raw) ? raw.match(/.{4}/g).join('-') : null
}

/** EU/EEA, Switzerland, the UK, the rest of Europe and the territories that pay in euros (Pelli's list). */
const EURO_COUNTRIES = new Set(
  'AT BE BG HR CY CZ DK EE FI FR DE GR HU IE IT LV LT LU MT NL PL PT RO SK SI ES SE IS LI NO CH GB AD AL AX BA BY FO GG GI IM JE MC MD ME MK RS RU SJ SM UA VA XK GF GP MQ RE YT PM BL MF'.split(' '),
)

function offerFor(req, lang) {
  const country = String(req.headers['cf-ipcountry'] || COUNTRY).toUpperCase()
  const currency = EURO_COUNTRIES.has(country) ? 'eur' : 'usd'
  let label
  try {
    label = new Intl.NumberFormat(lang, { style: 'currency', currency, maximumFractionDigits: 0 }).format(25)
  } catch {
    label = new Intl.NumberFormat('en', { style: 'currency', currency, maximumFractionDigits: 0 }).format(25)
  }
  return { amount: 2500, currency, label }
}

/** Marks a checkout paid: the licence gets its key (the Stripe webhook does this in Pelli). */
function pay(lic) {
  if (lic.status !== 'pending') return
  lic.status = 'active'
  lic.key = newKey()
  lic.paidAt = Date.now()
  save()
  const amount = lic.currency === 'eur' ? '25 €' : '$25'
  console.log(`[discord] 💎 Achat Pelli Instant Plus — ${amount} (${lic.currency.toUpperCase()})`)
}

/** The licence of a token signed here, with its claims; null when unknown. */
function licenseOfToken(token) {
  const claims = readToken(token)
  const lic = claims && state.licenses[claims.lid]
  return lic ? { lic, claims } : null
}

/** The Maximum pack files of the manifest (name, size, sha256). */
const MAXIMUM_FILES = (() => {
  try {
    const manifest = JSON.parse(readFileSync(join(REPO, 'models', 'manifest.json'), 'utf8'))
    return manifest.models.filter((m) => m.bundle === 'maximum').map(({ file, size, sha256 }) => ({ file, size, sha256 }))
  } catch {
    return []
  }
})()

/** Short-lived URLs of the pack, served by this fake from MODELS_DIR. */
function modelUrls() {
  const expiresAt = Date.now() + URL_TTL_MS
  return {
    files: MAXIMUM_FILES.map((f) => ({
      ...f,
      url: `${BASE}/fake/models/${encodeURIComponent(f.file)}?expires=${expiresAt}&sig=${sha256(`${f.file}:${expiresAt}`).slice(0, 16)}`,
    })),
    expiresAt,
  }
}

/** GET /fake/models/<file>: the file with range requests; 403 once the URL expired (like S3). */
function serveModel(req, res, url) {
  const file = decodeURIComponent(url.pathname.slice('/fake/models/'.length))
  const expires = Number(url.searchParams.get('expires'))
  if (!MAXIMUM_FILES.some((f) => f.file === file)) return fail(res, 404, 'not_found')
  if (url.searchParams.get('sig') !== sha256(`${file}:${expires}`).slice(0, 16) || !(expires > Date.now())) {
    return fail(res, 403, 'expired')
  }
  const path = join(MODELS_DIR, file)
  if (!existsSync(path)) {
    console.log(`[fake-pelli] ${file} is missing from ${MODELS_DIR} (FAKE_PELLI_MODELS_DIR)`)
    return fail(res, 404, 'not_found')
  }
  const size = statSync(path).size
  const range = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range || '')
  if (range) {
    const start = Number(range[1])
    const end = range[2] ? Math.min(Number(range[2]), size - 1) : size - 1
    if (start >= size) return send(res, 416, '', { 'Content-Range': `bytes */${size}` })
    res.writeHead(206, {
      'Content-Type': 'application/octet-stream',
      'Content-Length': end - start + 1,
      'Content-Range': `bytes ${start}-${end}/${size}`,
      'Accept-Ranges': 'bytes',
    })
    return createReadStream(path, { start, end }).pipe(res)
  }
  res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': size, 'Accept-Ranges': 'bytes' })
  createReadStream(path).pipe(res)
}

// ---------------------------------------------------------------------------
// Telemetry: validated as strictly as Pelli does
// ---------------------------------------------------------------------------

const int = (v) => Number.isInteger(v) && v >= 0
const bool = (v) => typeof v === 'boolean'
const TELEMETRY_PROPS = {
  onboarding_completed: {},
  app_opened: {},
  analysis_completed: {
    photos: int,
    raw: int,
    jpeg: int,
    seconds: (v) => typeof v === 'number' && v >= 0 && Number.isFinite(v),
    precision: (v) => v === 'standard' || v === 'maximum',
    selected: int,
    toReview: int,
    rejected: int,
    groups: int,
  },
  export_completed: { photos: int, xmp: bool, copied: bool, moved: bool },
  plus_activated: { via: (v) => ['purchase', 'key', 'account'].includes(v) },
  telemetry_disabled: {},
}
const TELEMETRY_KEYS = ['installId', 'event', 'appVersion', 'platform', 'arch', 'locale', 'props']

/** Why a payload is refused, or null when it is valid. */
function telemetryError(p) {
  if (!p || typeof p !== 'object' || Array.isArray(p)) return 'not an object'
  const extra = Object.keys(p).filter((k) => !TELEMETRY_KEYS.includes(k))
  if (extra.length) return `unknown fields: ${extra.join(', ')}`
  if (!isUuid(p.installId) || p.installId[14] !== '4') return 'installId: not a UUID v4'
  const spec = TELEMETRY_PROPS[p.event]
  if (!spec) return `unknown event: ${p.event}`
  if (typeof p.appVersion !== 'string' || !/^[0-9A-Za-z][0-9A-Za-z.+-]{0,31}$/.test(p.appVersion)) return 'appVersion'
  if (!['mac', 'windows', 'linux'].includes(p.platform)) return 'platform'
  if (!['arm64', 'x64'].includes(p.arch)) return 'arch'
  if (typeof p.locale !== 'string' || !/^[a-z]{2,3}$/.test(p.locale)) return 'locale (language only)'
  const props = p.props ?? {}
  if (!props || typeof props !== 'object' || Array.isArray(props)) return 'props'
  const keys = Object.keys(props)
  const unknown = keys.filter((k) => !(k in spec))
  if (unknown.length) return `unknown props: ${unknown.join(', ')}`
  const bad = Object.entries(spec).filter(([k, ok]) => !ok(props[k]))
  if (bad.length) return `bad props: ${bad.map(([k]) => k).join(', ')}`
  return null
}

/** installId → timestamps of the last hour. */
const telemetryRate = new Map()
const PLATFORM_NAMES = { mac: 'Mac', windows: 'Windows', linux: 'Linux' }

function discordLine(p) {
  const where = `${PLATFORM_NAMES[p.platform]} ${p.arch} · ${p.locale} · ${p.appVersion}`
  const n = (v) => new Intl.NumberFormat('fr-FR').format(v)
  switch (p.event) {
    case 'onboarding_completed':
      return `📥 Nouvelle installation Pelli Instant — ${where}`
    case 'analysis_completed': {
      const q = p.props
      const precision = q.precision === 'maximum' ? 'Maximale' : 'Standard'
      const seconds = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 1 }).format(q.seconds)
      return `⚡ Tri Pelli Instant : ${n(q.photos)} photos en ${seconds} s (${precision}, ${PLATFORM_NAMES[p.platform]}) — ${n(q.selected)} sélectionnées · ${n(q.toReview)} à vérifier · ${n(q.rejected)} rejetées`
    }
    case 'plus_activated':
      return `💎 Pelli Instant Plus activé (${p.props.via}) — ${where}`
    case 'telemetry_disabled':
      return `🔕 Télémétrie désactivée — ${PLATFORM_NAMES[p.platform]} · ${p.appVersion}`
    default:
      return null
  }
}

// ---------------------------------------------------------------------------
// /api/cull/*
// ---------------------------------------------------------------------------

function galleryView(g) {
  return {
    id: g.id,
    title: g.title,
    slug: g.slug,
    status: g.status,
    imageCount: g.imageCount,
    coverUrl: null,
    createdAt: g.createdAt,
    url: `${BASE}/#${encodeURIComponent(g.slug)}`,
  }
}

const usedBytes = (acc) => state.galleries.filter((g) => g.account === acc.key).reduce((n, g) => n + (g.bytes || 0), 0)
const galleryOf = (acc, id) => state.galleries.find((g) => g.id === id && g.account === acc.key && g.status !== 'deleted')

/** `{ name: "file", filename, data }` of a multipart body. */
function multipartFile(body, contentType) {
  const m = /boundary=(?:"([^"]+)"|([^;]+))/i.exec(contentType || '')
  if (!m) return null
  const boundary = Buffer.from(`--${m[1] || m[2]}`)
  let pos = body.indexOf(boundary)
  while (pos !== -1) {
    const start = pos + boundary.length + 2 // CRLF
    const headerEnd = body.indexOf('\r\n\r\n', start)
    if (headerEnd === -1) return null
    const headers = body.subarray(start, headerEnd).toString('utf8')
    const next = body.indexOf(boundary, headerEnd)
    if (next === -1) return null
    if (/name="file"/i.test(headers)) {
      const filename = /filename="([^"]*)"/i.exec(headers)?.[1] ?? 'photo.jpg'
      return { filename, data: body.subarray(headerEnd + 4, next - 2) }
    }
    pos = next
  }
  return null
}

/** POST routes; `lang` comes from `Accept-Language`. */
const routes = {
  'auth/create-session': async (req, res) => {
    const body = await readJson(req)
    if (typeof body.deviceName !== 'string' || !body.deviceName.trim() || !['macos', 'windows', 'linux'].includes(body.platform)) {
      return fail(res, 400, 'invalid_request')
    }
    const sessionId = `cull_${b64url(randomBytes(32))}`
    const consentId = consentIdOf(sessionId)
    sessions.set(consentId, {
      status: AUTO_APPROVE ? 'approved' : 'pending',
      account: DEFAULT_PLAN,
      device: body.deviceName.trim().slice(0, 80),
      platform: body.platform,
      appVersion: String(body.appVersion || ''),
      createdAt: Date.now(),
    })
    send(res, 200, {
      sessionId,
      authUrl: `${BASE}/cull/authorize?session=${consentId}`,
      expiresIn: SESSION_TTL_S,
      interval: 3,
    })
  },

  'auth/check-session': async (req, res, lang) => {
    const { sessionId } = await readJson(req)
    const consentId = typeof sessionId === 'string' ? consentIdOf(sessionId) : ''
    const s = sessions.get(consentId)
    if (!s || Date.now() - s.createdAt > SESSION_TTL_S * 1000) return send(res, 200, { status: 'expired' })
    if (s.status === 'pending') return send(res, 200, { status: 'pending' })
    sessions.delete(consentId) // final statuses are delivered once
    if (s.status === 'denied') return send(res, 200, { status: 'denied' })
    const acc = accountFor(s.account)
    const accessToken = `fake_${b64url(randomBytes(24))}`
    state.tokens[accessToken] = { account: acc.key, device: s.device, createdAt: Date.now() }
    save()
    send(res, 200, { status: 'approved', accessToken, expiresIn: 2592000, account: accountView(acc, lang) })
  },

  me: async (req, res, lang) => {
    const acc = bearer(req, res)
    if (acc) send(res, 200, { account: accountView(acc, lang) })
  },

  // 0.1 apps reserve photos before each analysis: always granted now.
  'usage/reserve': async (req, res) => {
    if (!bearer(req, res)) return
    send(res, 200, { granted: true, unlimited: true, monthlyQuota: null, usedThisMonth: 0, remaining: null, resetsAt: 0 })
  },
  'usage/release': async (req, res) => {
    if (bearer(req, res)) send(res, 200, { usedThisMonth: 0 })
  },

  'plus/checkout': async (req, res, lang) => {
    const body = await readJson(req)
    if (!isDevice(body.device) || !['mac', 'windows', 'linux'].includes(body.platform)) return fail(res, 400, 'invalid_request')
    const claimToken = b64url(randomBytes(24))
    const lic = {
      id: `lic_${b64url(randomBytes(9))}`,
      key: null,
      device: body.device,
      status: 'pending',
      claimTokenHash: sha256(claimToken),
      sessionId: `cs_test_${b64url(randomBytes(18))}`,
      currency: offerFor(req, body.lang || lang).currency,
      polls: 0,
      createdAt: Date.now(),
    }
    state.licenses[lic.id] = lic
    save()
    console.log(`[fake-pelli] checkout ${lic.id} (${lic.currency}, ${body.platform} ${body.appVersion}), paid ${PAY_AFTER ? `at claim #${PAY_AFTER}` : 'on the fake Stripe page'}`)
    send(res, 200, { id: lic.id, claimToken, url: `${BASE}/fake/checkout?license=${lic.id}` })
  },

  'plus/claim': async (req, res) => {
    const { id, claimToken } = await readJson(req)
    const lic = state.licenses[id]
    if (!lic || typeof claimToken !== 'string' || lic.claimTokenHash !== sha256(claimToken)) return send(res, 200, { status: 'expired' })
    if (lic.status === 'pending') {
      if (Date.now() - lic.createdAt > CHECKOUT_TTL_MS) {
        lic.status = 'expired'
        save()
      } else {
        lic.polls += 1
        if (PAY_AFTER && lic.polls >= PAY_AFTER) pay(lic)
        else save()
      }
    }
    if (lic.status === 'pending') return send(res, 200, { status: 'pending' })
    if (lic.status === 'expired' || !lic.key) return send(res, 200, { status: 'expired' })
    if (lic.status === 'revoked') return send(res, 200, { status: 'expired' })
    send(res, 200, { status: 'active', key: lic.key, license: licenseToken(lic) })
  },

  'plus/activate': async (req, res) => {
    const body = await readJson(req)
    if (!isDevice(body.device)) return fail(res, 400, 'invalid_request')
    const key = normalizeKey(body.key)
    const lic = key && Object.values(state.licenses).find((l) => l.key === key)
    if (!lic) return fail(res, 404, 'invalidKey')
    if (lic.status === 'revoked') return fail(res, 410, 'revoked')
    if (lic.device && lic.device !== body.device && lic.status === 'active') return fail(res, 409, 'otherDevice')
    lic.device = body.device
    lic.status = 'active'
    save()
    console.log(`[fake-pelli] licence ${lic.key} activated on ${lic.device.slice(0, 12)}…`)
    send(res, 200, { key: lic.key, license: licenseToken(lic) })
  },

  'plus/release': async (req, res) => {
    const { license } = await readJson(req)
    const found = licenseOfToken(license)
    if (!found) return fail(res, 401, 'invalidLicense')
    const { lic, claims } = found
    if (lic.status === 'revoked') return fail(res, 410, 'revoked')
    if (lic.status === 'active' && lic.device === claims.dev) {
      lic.status = 'released'
      lic.device = null
      save()
      console.log(`[fake-pelli] licence ${lic.key} released`)
    }
    send(res, 200, { ok: true })
  },

  'plus/verify': async (req, res) => {
    const { license } = await readJson(req)
    const found = licenseOfToken(license)
    if (!found) return send(res, 200, { status: 'invalid' })
    const { lic, claims } = found
    if (lic.status === 'revoked') return send(res, 200, { status: 'revoked' })
    if (lic.status === 'active' && lic.device === claims.dev) return send(res, 200, { status: 'active' })
    send(res, 200, { status: lic.status === 'released' || lic.status === 'active' ? 'released' : 'invalid' })
  },

  models: async (req, res) => {
    const { license } = await readJson(req)
    if (license !== undefined) {
      const found = licenseOfToken(license)
      if (found?.lic.status === 'active' && found.lic.device === found.claims.dev) return send(res, 200, modelUrls())
      return fail(res, 403, 'plusRequired')
    }
    const token = (req.headers.authorization || '').replace(/^Bearer /, '')
    const acc = state.accounts[state.tokens[token]?.account]
    if (acc && PLANS[acc.plan].plus) return send(res, 200, modelUrls())
    fail(res, 403, 'plusRequired')
  },

  telemetry: async (req, res) => {
    const p = await readJson(req)
    const error = telemetryError(p)
    if (error) {
      console.log(`[telemetry] REFUSED (${error}): ${JSON.stringify(p)}`)
      return fail(res, 400, 'invalid_request', { reason: error })
    }
    const now = Date.now()
    const recent = (telemetryRate.get(p.installId) ?? []).filter((t) => now - t < 3_600_000)
    if (recent.length >= 120) return fail(res, 429, 'rate_limited')
    telemetryRate.set(p.installId, [...recent, now])
    console.log(`[telemetry] ${JSON.stringify(p)}`)
    const line = discordLine(p)
    if (line) console.log(`[discord] ${line}`)
    telemetryLog.unshift({ at: now, payload: p })
    telemetryLog.length = Math.min(telemetryLog.length, 100)
    res.writeHead(204)
    res.end()
  },

  'galleries/list': async (req, res) => {
    const acc = bearer(req, res)
    if (!acc) return
    const galleries = state.galleries
      .filter((g) => g.account === acc.key && g.status !== 'deleted')
      .sort((a, b) => b.createdAt - a.createdAt)
      .map(galleryView)
    send(res, 200, { galleries })
  },

  'galleries/create': async (req, res) => {
    const acc = bearer(req, res)
    if (!acc) return
    const { requestId, title, type } = await readJson(req)
    if (!isUuid(requestId) || typeof title !== 'string' || !title.trim() || !['selection', 'delivery'].includes(type)) {
      return fail(res, 400, 'invalid_request')
    }
    if (!PLANS[acc.plan].sendToGallery) return fail(res, 403, 'subscription_required')
    const key = `gallery:${acc.key}:${requestId}`
    const previous = state.requests[key]
    if (previous) {
      const g = galleryOf(acc, previous.id)
      if (!g || previous.title !== title.trim() || previous.type !== type) return fail(res, 409, 'request_conflict')
      return send(res, 200, { gallery: galleryView(g), password: null })
    }
    let slug = slugify(title)
    for (let n = 2; state.galleries.some((g) => g.slug === slug); n++) slug = `${slugify(title)}-${n}`
    const g = {
      id: createHash('sha1').update(key).digest('hex').slice(0, 24),
      account: acc.key,
      title: title.trim(),
      slug,
      type,
      status: 'draft',
      imageCount: 0,
      bytes: 0,
      createdAt: Date.now(),
    }
    state.galleries.push(g)
    state.requests[key] = { id: g.id, title: g.title, type }
    save()
    console.log(`[fake-pelli] ${acc.key}: gallery created: ${g.title} (${type})`)
    send(res, 200, { gallery: galleryView(g), password: null })
  },

  'folders/create': async (req, res) => {
    const acc = bearer(req, res)
    if (!acc) return
    const { requestId, galleryId, name } = await readJson(req)
    if (!isUuid(requestId) || typeof name !== 'string' || !name.trim()) return fail(res, 400, 'invalid_request')
    if (!PLANS[acc.plan].sendToGallery) return fail(res, 403, 'subscription_required')
    if (!galleryOf(acc, galleryId)) return fail(res, 404, 'gallery_not_found')
    const key = `folder:${acc.key}:${requestId}`
    const previous = state.requests[key]
    if (previous) {
      const f = state.folders.find((x) => x.id === previous.id)
      if (!f || previous.galleryId !== galleryId || previous.name !== name.trim()) return fail(res, 409, 'request_conflict')
      return send(res, 200, { folder: { id: f.id, name: f.name } })
    }
    if (state.folders.some((f) => f.galleryId === galleryId && fold(f.name) === fold(name))) {
      return fail(res, 409, 'folder_exists')
    }
    const f = { id: randomUUID(), galleryId, name: name.trim() }
    state.folders.push(f)
    state.requests[key] = { id: f.id, galleryId, name: f.name }
    save()
    send(res, 200, { folder: { id: f.id, name: f.name } })
  },

  'upload-token': async (req, res) => {
    const acc = bearer(req, res)
    if (!acc) return
    const { galleryId } = await readJson(req)
    if (!PLANS[acc.plan].sendToGallery) return fail(res, 403, 'subscription_required')
    const g = galleryOf(acc, galleryId)
    if (!g) return fail(res, 404, 'gallery_not_found')
    const uploadToken = `up.${g.id}.${b64url(randomBytes(12))}`
    state.tokens[uploadToken] = { gallery: g.id, upload: true, createdAt: Date.now() }
    save()
    send(res, 200, {
      uploadToken,
      uploadUrl: `${BASE}/api/photo`,
      expiresIn: 172800,
      expiresAt: Date.now() + 172800 * 1000,
      maxFileSize: MAX_FILE_SIZE,
      maxConcurrency: 3,
      fileField: 'file',
      nameParam: 'name',
      folderParam: 'folder',
      acceptedExtensions: ['.jpg', '.jpeg', '.png', '.tif', '.tiff', '.heic'],
      galleryId: g.id,
      galleryTitle: g.title,
      storage: { usedBytes: usedBytes(acc), limitBytes: STORAGE_LIMIT },
    })
  },
}

// ---------------------------------------------------------------------------
// Upload service: POST /api/photo?name=&folder=
// ---------------------------------------------------------------------------

async function upload(req, res, url) {
  const token = req.headers.authorization || ''
  if (!token) return fail(res, 401, 'missing_authorization')
  if (token.startsWith('Bearer ')) return fail(res, 401, 'invalid_token') // the raw token is expected
  const grant = state.tokens[token]
  if (!grant?.upload) return fail(res, 401, 'invalid_token')
  const g = state.galleries.find((x) => x.id === grant.gallery && x.status !== 'deleted')
  if (!g) return fail(res, 404, 'gallery_not_found')
  const folderId = url.searchParams.get('folder')
  const folder = folderId ? state.folders.find((f) => f.id === folderId && f.galleryId === g.id) : null
  if (folderId && !folder) return fail(res, 400, 'folder_not_found')
  let body
  try {
    body = await readBody(req)
  } catch {
    return fail(res, 413, 'file_too_large')
  }
  if (Math.random() < UPLOAD_FAIL) return fail(res, 500, 'server_error')
  const file = multipartFile(body, req.headers['content-type'])
  if (!file || !file.data.length) return fail(res, 400, 'invalid_request')
  if (file.data.length > MAX_FILE_SIZE) return fail(res, 413, 'file_too_large')
  const name = basename(url.searchParams.get('name') || file.filename).replace(/[^\p{L}\p{N}._ -]/gu, '_')
  const dir = join(DIR, 'uploads', g.account, g.slug, folder ? folder.name.replace(/[/\\:]/g, '-') : '')
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, name), file.data)
  g.imageCount += 1
  g.bytes = (g.bytes || 0) + file.data.length
  save()
  send(res, 200, { success: true, imageId: randomUUID() })
}

// ---------------------------------------------------------------------------
// Pages: consent, fake Stripe, thank-you page, overview
// ---------------------------------------------------------------------------

const PAGE_STYLE = `
  :root { color-scheme: light dark; --bg: #f6f5f2; --card: #fff; --text: #1d1d1b; --muted: #6b6a66; --accent: #e8572a; --line: #e5e3dd; }
  @media (prefers-color-scheme: dark) { :root { --bg: #141414; --card: #1e1e1d; --text: #f1f0ec; --muted: #a09e98; --line: #2e2d2b; } }
  * { box-sizing: border-box; } body { margin: 0; font: 15px/1.5 -apple-system, system-ui, sans-serif; background: var(--bg); color: var(--text); }
  main { max-width: 760px; margin: 40px auto; padding: 0 16px; } .card { background: var(--card); border: 1px solid var(--line); border-radius: 14px; padding: 24px; margin-bottom: 16px; }
  h1 { font-size: 22px; margin: 0 0 8px; } h2 { font-size: 17px; margin: 0 0 12px; } .muted { color: var(--muted); }
  button, .btn { font: inherit; border-radius: 10px; border: 1px solid var(--line); padding: 9px 16px; background: var(--card); color: var(--text); cursor: pointer; text-decoration: none; }
  button.primary { background: var(--accent); border-color: var(--accent); color: #fff; } .row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  label.choice { border: 1px solid var(--line); border-radius: 10px; padding: 8px 12px; cursor: pointer; } label.choice:has(input:checked) { border-color: var(--accent); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); gap: 8px; margin-top: 8px; } .grid img { width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; }
  code, pre { font-size: 13px; } pre { white-space: pre-wrap; margin: 4px 0 0; } .badge { display: inline-block; padding: 2px 8px; border-radius: 99px; background: var(--line); font-size: 12px; }
  table { width: 100%; border-collapse: collapse; } td { padding: 6px 4px; border-top: 1px solid var(--line); vertical-align: top; }`

function page(title, content) {
  return `<!doctype html><html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>${escapeHtml(title)}</title><style>${PAGE_STYLE}</style></head><body><main>${content}</main></body></html>`
}

const redirect = (res, location) => send(res, 303, '', { Location: location })

function consentPage(id) {
  const s = sessions.get(id)
  if (!s || s.status !== 'pending') {
    return page('Pelli Instant', `<div class="card"><h1>Demande expirée</h1><p class="muted">Relancez la connexion depuis Pelli Instant.</p></div>`)
  }
  const choices = Object.entries(PLANS)
    .map(([key, p]) => {
      const acc = state.accounts[key]
      const now = acc && acc.plan !== key ? ` → ${escapeHtml(PLANS[acc.plan].label.fr)}` : ''
      const plus = p.plus ? ' <span class="muted">· Plus inclus</span>' : ''
      return `<label class="choice"><input type="radio" name="account" value="${key}"${key === s.account ? ' checked' : ''}> ${escapeHtml(p.label.fr)}${now}${plus}</label>`
    })
    .join('')
  return page(
    'Autoriser Pelli Instant',
    `<div class="card"><span class="badge">Faux Pelli · développement</span>
      <h1>Pelli Instant sur « ${escapeHtml(s.device)} » veut accéder à votre compte</h1>
      <p class="muted">${escapeHtml(s.platform)} · version ${escapeHtml(s.appVersion)}</p>
      <form method="post" action="/cull/authorize/decide?session=${encodeURIComponent(id)}">
        <p style="margin:16px 0 8px">Compte (un par forfait) :</p>
        <div class="row">${choices}</div>
        <div class="row" style="margin-top:16px">
          <button class="primary" name="decision" value="approve">Autoriser</button>
          <button name="decision" value="deny">Refuser</button>
        </div>
      </form></div>`,
  )
}

/** Where the app sends the photographer to pay: Stripe Checkout in Pelli. */
function checkoutPage(lic) {
  if (!lic) return page('Paiement', `<div class="card"><h1>Paiement introuvable</h1></div>`)
  const amount = lic.currency === 'eur' ? '25 €' : '$25'
  const done = lic.status !== 'pending'
  return page(
    'Pelli Instant Plus — paiement',
    `<div class="card"><span class="badge">Faux Stripe · développement</span>
      <h1>Pelli Instant Plus — ${amount}</h1>
      <p class="muted">Paiement unique, pour cet ordinateur. ${PAY_AFTER ? `Payé tout seul à la ${PAY_AFTER}e vérification de l'app.` : ''}</p>
      ${
        done
          ? `<p>Statut : <strong>${escapeHtml(lic.status)}</strong></p>`
          : `<form method="post" action="/fake/pay?license=${lic.id}" class="row">
               <button class="primary">Payer ${amount}</button>
               <a class="btn" href="/cull/plus?license=${lic.id}&cancelled=1">Annuler</a>
             </form>`
      }</div>`,
  )
}

/** The thank-you page Stripe returns to (`/cull/plus?license=&session_id=`). */
function thankYouPage(url) {
  if (url.searchParams.get('cancelled') === '1') {
    return page('Paiement annulé', `<div class="card"><h1>Paiement annulé, rien n'a été débité</h1><p class="muted">Retournez dans l'app.</p></div>`)
  }
  const lic = state.licenses[url.searchParams.get('license')]
  if (!lic || lic.sessionId !== url.searchParams.get('session_id') || !lic.key) {
    return page('Pelli Instant Plus', `<div class="card"><h1>Paiement en cours…</h1><p class="muted">Rechargez cette page dans un instant.</p></div>`)
  }
  return page(
    'Merci !',
    `<div class="card"><h1>Merci ! Pelli Instant Plus est activé</h1>
      <p>Retournez dans l'app, elle s'est activée toute seule.</p>
      <p>Votre clé : <code>${escapeHtml(lic.key)}</code></p>
      <p class="muted">Gardez-la pour activer Plus sur un autre ordinateur après l'avoir libérée.</p></div>`,
  )
}

function listUploads(acc, slug) {
  const root = join(DIR, 'uploads', acc, slug)
  if (!existsSync(root)) return []
  const out = []
  const walk = (dir, rel) => {
    for (const entry of readdirSync(dir)) {
      const full = join(dir, entry)
      if (statSync(full).isDirectory()) walk(full, rel ? `${rel}/${entry}` : entry)
      else out.push({ folder: rel, name: entry, path: `${acc}/${slug}/${rel ? `${rel}/` : ''}${entry}` })
    }
  }
  walk(root, '')
  return out
}

function accountCard(acc) {
  const planButtons = Object.entries(PLANS)
    .map(([key, p]) => `<button name="plan" value="${key}" class="${key === acc.plan ? 'primary' : ''}">${escapeHtml(p.label.fr)}</button>`)
    .join('')
  const devices = Object.values(state.tokens).filter((t) => t.account === acc.key).length
  return `<div class="card"><h2>camille+${escapeHtml(acc.key)}@example.test</h2>
    <p class="muted">${escapeHtml(PLANS[acc.plan].label.fr)} · Plus ${PLANS[acc.plan].plus ? 'inclus' : 'non inclus'} · ${devices} appareil(s) connecté(s)</p>
    <form method="post" action="/fake/plan?account=${acc.key}" class="row"><span class="muted">Forfait :</span>${planButtons}</form></div>`
}

function licensesCard() {
  const rows = Object.values(state.licenses)
    .sort((a, b) => b.createdAt - a.createdAt)
    .map(
      (l) => `<tr><td><code>${escapeHtml(l.key ?? '—')}</code><br><span class="muted">${escapeHtml(l.id)}</span></td>
        <td>${escapeHtml(l.status)}</td><td class="muted">${l.device ? `${l.device.slice(0, 12)}…` : '—'}</td>
        <td><form method="post" action="/fake/license?id=${l.id}" class="row">
          ${l.status === 'pending' ? '<button name="action" value="pay" class="primary">Payer</button>' : ''}
          ${l.status === 'active' ? '<button name="action" value="release">Libérer</button>' : ''}
          ${l.status === 'active' || l.status === 'released' ? '<button name="action" value="refund">Rembourser</button>' : ''}
        </form></td></tr>`,
    )
    .join('')
  return `<div class="card"><h2>Licences Plus</h2>${rows ? `<table>${rows}</table>` : '<p class="muted">Aucune licence.</p>'}
    <p class="muted">Clé publique de test : <code>${PUBLIC_KEY_B64}</code></p></div>`
}

function telemetryCard() {
  const rows = telemetryLog
    .slice(0, 30)
    .map((t) => `<tr><td class="muted">${new Date(t.at).toLocaleTimeString('fr-FR')}</td><td><strong>${escapeHtml(t.payload.event)}</strong><pre>${escapeHtml(JSON.stringify(t.payload.props))}</pre></td></tr>`)
    .join('')
  return `<div class="card"><h2>Télémétrie (derniers événements)</h2>${rows ? `<table>${rows}</table>` : '<p class="muted">Rien reçu.</p>'}</div>`
}

function galleryCard(g) {
  const files = listUploads(g.account, g.slug)
  const byFolder = new Map()
  for (const f of files) byFolder.set(f.folder, [...(byFolder.get(f.folder) ?? []), f])
  const sections = [...byFolder.entries()]
    .map(
      ([folder, list]) =>
        `<p style="margin:12px 0 0"><strong>${escapeHtml(folder || 'Racine')}</strong> <span class="muted">· ${list.length} photo(s)</span></p>
         <div class="grid">${list.map((f) => `<img loading="lazy" title="${escapeHtml(f.name)}" src="/uploads/${f.path.split('/').map(encodeURIComponent).join('/')}">`).join('')}</div>`,
    )
    .join('')
  return `<div class="card" id="${escapeHtml(g.slug)}"><div class="row" style="justify-content:space-between"><h2>${escapeHtml(g.title)}</h2>
    <form method="post" action="/fake/delete-gallery?id=${g.id}"><button>Supprimer</button></form></div>
    <p class="muted">camille+${escapeHtml(g.account)} · ${g.type} · ${g.imageCount} photo(s) · ${(g.bytes / 1024 / 1024).toFixed(1)} Mo</p>${sections}</div>`
}

function overviewPage() {
  const accounts = Object.values(state.accounts).map(accountCard).join('')
  const galleries = state.galleries
    .filter((g) => g.status !== 'deleted')
    .sort((a, b) => b.createdAt - a.createdAt)
    .map(galleryCard)
    .join('')
  return page(
    'Faux Pelli',
    `<div class="card"><span class="badge">Faux Pelli · développement</span><h1>Pelli Instant — banc local</h1>
      <p class="muted">API : <code>${BASE}/api/cull</code> · prix : <a href="/api/cull/plus/offer?lang=fr">fr</a>, <a href="/api/cull/plus/offer?lang=en">en</a> · pack Maximum lu dans <code>${escapeHtml(MODELS_DIR)}</code></p>
      <form method="post" action="/fake/reset" class="row"><button>Tout effacer (comptes, licences, galeries, jetons)</button></form></div>
      ${licensesCard()}${telemetryCard()}
      ${accounts || '<div class="card muted">Aucun compte : connectez Pelli Instant pour en créer un.</div>'}
      ${galleries || '<div class="card muted">Aucune galerie pour le moment.</div>'}`,
  )
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

const server = createServer(async (req, res) => {
  const url = new URL(req.url, BASE)
  const lang = langOf(req.headers['accept-language'])
  if (url.pathname.startsWith('/api/')) console.log(`[request] ${req.method} ${url.pathname}`)
  try {
    if (req.method === 'GET' && url.pathname === '/api/cull/plus/offer') {
      return send(res, 200, offerFor(req, url.searchParams.get('lang') || lang))
    }
    if (req.method === 'POST' && url.pathname.startsWith('/api/cull/')) {
      const handler = routes[url.pathname.slice('/api/cull/'.length)]
      if (!handler) return fail(res, 404, 'not_found')
      return await handler(req, res, lang)
    }
    if (req.method === 'POST' && url.pathname === '/api/photo') return await upload(req, res, url)
    if (req.method === 'GET' && url.pathname.startsWith('/fake/models/')) return serveModel(req, res, url)
    if (req.method === 'GET' && url.pathname === '/cull/authorize') {
      return send(res, 200, consentPage(url.searchParams.get('session')))
    }
    if (req.method === 'POST' && url.pathname === '/cull/authorize/decide') {
      const s = sessions.get(url.searchParams.get('session'))
      const form = await readForm(req)
      const approve = form.get('decision') === 'approve'
      if (s?.status === 'pending') {
        s.status = approve ? 'approved' : 'denied'
        if (approve) {
          if (PLANS[form.get('account')]) s.account = form.get('account')
          accountFor(s.account)
          save()
        }
      }
      const message = approve ? 'Vous pouvez revenir dans Pelli Instant.' : 'Accès refusé.'
      return send(res, 200, page('Pelli Instant', `<div class="card"><h1>${message}</h1><p><a class="btn" href="/">Voir le banc</a></p></div>`))
    }
    if (req.method === 'GET' && url.pathname === '/fake/checkout') {
      return send(res, 200, checkoutPage(state.licenses[url.searchParams.get('license')]))
    }
    if (req.method === 'POST' && url.pathname === '/fake/pay') {
      const lic = state.licenses[url.searchParams.get('license')]
      if (lic) pay(lic)
      return redirect(res, lic ? `/cull/plus?license=${lic.id}&session_id=${lic.sessionId}` : '/')
    }
    if (req.method === 'GET' && url.pathname === '/cull/plus') return send(res, 200, thankYouPage(url))
    if (req.method === 'POST' && url.pathname === '/fake/license') {
      const lic = state.licenses[url.searchParams.get('id')]
      const action = (await readForm(req)).get('action')
      if (lic && action === 'pay') pay(lic)
      if (lic && action === 'release') Object.assign(lic, { status: 'released', device: null })
      if (lic && action === 'refund') {
        lic.status = 'revoked'
        console.log(`[fake-pelli] licence ${lic.key} refunded: revoked`)
      }
      save()
      return redirect(res, '/')
    }
    const acc = state.accounts[url.searchParams.get('account')]
    if (req.method === 'POST' && url.pathname === '/fake/plan' && acc) {
      const plan = (await readForm(req)).get('plan')
      if (PLANS[plan]) setPlan(acc, plan)
      save()
      console.log(`[fake-pelli] ${acc.key}: plan ${acc.plan}`)
      return redirect(res, '/')
    }
    if (req.method === 'POST' && url.pathname === '/fake/delete-gallery') {
      const g = state.galleries.find((x) => x.id === url.searchParams.get('id'))
      if (g) g.status = 'deleted'
      save()
      return redirect(res, '/')
    }
    if (req.method === 'POST' && url.pathname === '/fake/reset') {
      Object.assign(state, { accounts: {}, tokens: {}, galleries: [], folders: [], requests: {}, licenses: {} })
      save()
      return redirect(res, '/')
    }
    if (req.method === 'GET' && url.pathname.startsWith('/uploads/')) {
      const rel = decodeURIComponent(url.pathname.slice('/uploads/'.length))
      const file = resolve(DIR, 'uploads', rel)
      if (!file.startsWith(join(DIR, 'uploads')) || !existsSync(file)) return fail(res, 404, 'not_found')
      res.writeHead(200, { 'Content-Type': 'image/jpeg' })
      return res.end(readFileSync(file))
    }
    if (req.method === 'GET' && url.pathname === '/') return send(res, 200, overviewPage())
    fail(res, 404, 'not_found')
  } catch (error) {
    if (error instanceof SyntaxError) return fail(res, 400, 'invalid_request')
    console.error('[fake-pelli]', error)
    if (!res.headersSent) fail(res, 500, 'server_error')
  }
})

server.listen(PORT, '127.0.0.1', () => {
  console.log(`[fake-pelli] ${BASE}  (${AUTO_APPROVE ? `auto-approve as ${DEFAULT_PLAN}, ` : ''}data ${DIR})`)
  console.log(`[fake-pelli] launch the app with PELLI_API_URL=${BASE} PELLI_LICENSE_PUBLIC_KEY=${PUBLIC_KEY_B64}`)
})
