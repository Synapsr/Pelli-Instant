#!/usr/bin/env node
// Fake Pelli for developing and testing Pelli Instant without the Pelli web app,
// its database or its upload service. Zero dependencies (Node 20+).
//
//   node scripts/fake-pelli.mjs              # http://localhost:3100
//   PELLI_API_URL=http://localhost:3100 pnpm app      (or: pnpm app:fake-pelli)
//
// Implements the contract of docs/pelli-api.md: `/api/cull/*` (sign-in by
// browser consent, account and plan, monthly usage, offers, galleries,
// folders, upload token) and the upload service (`POST /api/photo`).
//
// Each plan has its own test account (camille+<plan>@example.test) with a
// real monthly counter: the consent page picks the account that signs in and
// can set how many photos it already sorted this month. http://localhost:3100/
// shows every account (plan, usage, switch plan, reset usage) and the photos
// received, written to `<data dir>/uploads/<account>/<gallery>/<folder>/`.
//
// Environment:
//   FAKE_PELLI_PORT        port (default 3100)
//   FAKE_PELLI_DIR         state + uploads (default ./.fake-pelli)
//   FAKE_PELLI_PLAN        account signed in by FAKE_PELLI_AUTO_APPROVE:
//                          free | lite | pro | ultra | trial (default pro)
//   FAKE_PELLI_AUTO_APPROVE=1   approve sign-ins without the consent page (scripts, Playwright)
//   FAKE_PELLI_UPLOAD_FAIL      share of uploads answered 500 (0..1, retry testing)
//   FAKE_PELLI_STORAGE_GB       storage limit of each account (default 50)

import { createHash, randomBytes, randomUUID } from 'node:crypto'
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import { basename, join, resolve } from 'node:path'

const PORT = Number(process.env.FAKE_PELLI_PORT || 3100)
const BASE = `http://localhost:${PORT}`
const DIR = resolve(process.env.FAKE_PELLI_DIR || '.fake-pelli')
const STATE_FILE = join(DIR, 'state.json')
const STATE_VERSION = 2
const AUTO_APPROVE = process.env.FAKE_PELLI_AUTO_APPROVE === '1'
const UPLOAD_FAIL = Math.min(1, Math.max(0, Number(process.env.FAKE_PELLI_UPLOAD_FAIL || 0)))
const STORAGE_LIMIT = Math.round(Number(process.env.FAKE_PELLI_STORAGE_GB || 50) * 1024 ** 3)
const MAX_FILE_SIZE = 100 * 1024 * 1024
const SESSION_TTL_S = 600
const TRIAL_DAYS = 15
/** Bounds of `photos` in `usage/reserve` and `usage/release`. */
const MAX_PHOTOS_PER_REQUEST = 50_000

/**
 * Plans as Pelli resolves them (src/lib/cull/entitlements.ts): photos sorted
 * per calendar month (UTC), `null` = unlimited; the free plan is behind the
 * paywall for galleries.
 */
const PLANS = {
  free: { plan: 'free', label: { fr: 'Gratuit', en: 'Free' }, quota: 200, sendToGallery: false },
  lite: { plan: 'lite', label: { fr: 'Lite', en: 'Lite' }, quota: 1000, sendToGallery: true },
  pro: { plan: 'pro', label: { fr: 'Pro', en: 'Pro' }, quota: null, sendToGallery: true },
  ultra: { plan: 'ultra', label: { fr: 'Ultra', en: 'Ultra' }, quota: null, sendToGallery: true },
  trial: { plan: 'ultra', label: { fr: 'Essai Ultra', en: 'Ultra trial' }, quota: null, sendToGallery: true, trial: true },
}
const DEFAULT_PLAN = PLANS[process.env.FAKE_PELLI_PLAN] ? process.env.FAKE_PELLI_PLAN : 'pro'

mkdirSync(join(DIR, 'uploads'), { recursive: true })

/** Persistent state: survives restarts so the app's stored token stays valid. */
const state = (() => {
  const initial = { version: STATE_VERSION, accounts: {}, tokens: {}, galleries: [], folders: [], requests: {} }
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const MESSAGES = {
  invalid_request: 'The request body is invalid.',
  missing_token: 'Authorization header missing.',
  invalid_token: 'Invalid or expired token.',
  subscription_required: 'This account has no active Pelli subscription.',
  quota_exceeded: 'The monthly photo quota of this plan is reached.',
  gallery_not_found: 'Gallery not found.',
  folder_not_found: 'Folder not found.',
  folder_exists: 'A folder with this name already exists.',
  request_conflict: 'requestId reused with different values.',
  not_found: 'Not found.',
  file_too_large: 'File too large.',
  server_error: 'Simulated server error.',
  missing_authorization: 'Authorization header missing.',
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
// As in Pelli: the browser's consent URL carries a public id derived one way
// from the app's secret `sessionId`, so a leaked URL cannot poll for the token.
const consentIdOf = (sessionId) => `cull_${b64url(createHash('sha256').update(`pelli-instant/consent:${sessionId}`).digest())}`
const fold = (s) => s.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase().trim()
const slugify = (s) => fold(s).replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'galerie'
const escapeHtml = (s) => String(s).replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`)
const isUuid = (s) => typeof s === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s)
const isPhotoCount = (n) => Number.isInteger(n) && n >= 1 && n <= MAX_PHOTOS_PER_REQUEST
/** `fr` for French, else English (Pelli localises in five languages). */
const langOf = (value) => (String(value || '').toLowerCase().startsWith('fr') ? 'fr' : 'en')

// ---------------------------------------------------------------------------
// Accounts and monthly usage
// ---------------------------------------------------------------------------

/** Calendar month (UTC) of a date: the period of the counter. */
const monthOf = (ms) => new Date(ms).toISOString().slice(0, 7)

/** First instant of next month (UTC), epoch ms. */
function resetsAt(ms = Date.now()) {
  const d = new Date(ms)
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 1)
}

/** The test account `key` (one per plan), created on first use. */
function accountFor(key) {
  if (!state.accounts[key]) {
    state.accounts[key] = { key, createdAt: Date.now(), hadTrial: false, usage: { month: monthOf(Date.now()), used: 0 } }
    setPlan(state.accounts[key], key)
  }
  return state.accounts[key]
}

function setPlan(acc, plan) {
  acc.plan = plan
  acc.trialEndsAt = PLANS[plan].trial ? Date.now() + TRIAL_DAYS * 86_400_000 : null
  acc.hadTrial ||= Boolean(PLANS[plan].trial)
}

/** Photos counted this month (the counter starts again every month). */
function usage(acc) {
  const month = monthOf(Date.now())
  if (acc.usage.month !== month) acc.usage = { month, used: 0 }
  return acc.usage
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
    entitlements: {
      analyze: true,
      monthlyQuota: p.quota,
      usedThisMonth: usage(acc).used,
      resetsAt: resetsAt(),
      sendToGallery: p.sendToGallery,
    },
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
// Offers
// ---------------------------------------------------------------------------

const HIGHLIGHTS = {
  fr: {
    free: [`Essai de ${TRIAL_DAYS} jours de Pelli`, 'Sans carte bancaire'],
    lite: ['10 galeries', '20 Go de stockage', '1 h de vidéo'],
    pro: ['Galeries illimitées', '50 Go de stockage', '5 h de vidéo'],
    ultra: ['Galeries illimitées', '100 To de stockage', '15 h de vidéo', 'Boutique en ligne'],
  },
  en: {
    free: [`${TRIAL_DAYS}-day Pelli trial`, 'No credit card'],
    lite: ['10 galleries', '20 GB of storage', '1 h of video'],
    pro: ['Unlimited galleries', '50 GB of storage', '5 h of video'],
    ultra: ['Unlimited galleries', '100 TB of storage', '15 h of video', 'Online store'],
  },
}

/** Pelli's prices in EUR: per month billed monthly, and per year. */
const PRICES = { free: [0, 0], lite: [5, 60], pro: [25, 180], ultra: [50, 420] }

function offers(lang) {
  const link = (path, content, hash = '') => `${BASE}${path}?utm_source=pelli-instant&utm_content=${content}${hash}`
  return {
    currency: 'EUR',
    trialDays: TRIAL_DAYS,
    plans: Object.entries(PRICES).map(([slug, [monthlyPrice, yearlyPrice]]) => ({
      slug,
      name: PLANS[slug].label[lang],
      monthlyPrice,
      yearlyPrice,
      yearlyPricePerMonth: Math.round((yearlyPrice / 12) * 100) / 100,
      photosPerMonth: PLANS[slug].quota,
      highlights: HIGHLIGHTS[lang][slug],
    })),
    links: {
      register: link('/register', 'register'),
      pricing: link('/dashboard/billing', 'pricing', '#plans-section'),
      trial: link('/register', 'trial'),
      billing: link('/dashboard/billing', 'billing'),
    },
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

  'usage/reserve': async (req, res) => {
    const acc = bearer(req, res)
    if (!acc) return
    const { photos } = await readJson(req)
    if (!isPhotoCount(photos)) return fail(res, 400, 'invalid_request')
    const quota = PLANS[acc.plan].quota
    const counter = usage(acc)
    if (quota === null) {
      // Unlimited: granted without counting.
      return send(res, 200, { granted: true, unlimited: true, monthlyQuota: null, usedThisMonth: counter.used, remaining: null, resetsAt: resetsAt() })
    }
    const remaining = Math.max(0, quota - counter.used)
    if (photos > remaining) {
      return fail(res, 403, 'quota_exceeded', { monthlyQuota: quota, usedThisMonth: counter.used, remaining, resetsAt: resetsAt() })
    }
    counter.used += photos
    save()
    console.log(`[fake-pelli] ${acc.key}: ${photos} photo(s) reserved, ${counter.used}/${quota} this month`)
    send(res, 200, { granted: true, unlimited: false, monthlyQuota: quota, usedThisMonth: counter.used, remaining: quota - counter.used, resetsAt: resetsAt() })
  },

  'usage/release': async (req, res) => {
    const acc = bearer(req, res)
    if (!acc) return
    const { photos } = await readJson(req)
    if (!isPhotoCount(photos)) return fail(res, 400, 'invalid_request')
    const counter = usage(acc)
    if (PLANS[acc.plan].quota !== null) {
      counter.used = Math.max(0, counter.used - photos)
      save()
      console.log(`[fake-pelli] ${acc.key}: ${photos} photo(s) given back, ${counter.used} this month`)
    }
    send(res, 200, { usedThisMonth: counter.used })
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
// Pages: consent, overview, offer links
// ---------------------------------------------------------------------------

const PAGE_STYLE = `
  :root { color-scheme: light dark; --bg: #f6f5f2; --card: #fff; --text: #1d1d1b; --muted: #6b6a66; --accent: #e8572a; --line: #e5e3dd; }
  @media (prefers-color-scheme: dark) { :root { --bg: #141414; --card: #1e1e1d; --text: #f1f0ec; --muted: #a09e98; --line: #2e2d2b; } }
  * { box-sizing: border-box; } body { margin: 0; font: 15px/1.5 -apple-system, system-ui, sans-serif; background: var(--bg); color: var(--text); }
  main { max-width: 760px; margin: 40px auto; padding: 0 16px; } .card { background: var(--card); border: 1px solid var(--line); border-radius: 14px; padding: 24px; margin-bottom: 16px; }
  h1 { font-size: 22px; margin: 0 0 8px; } h2 { font-size: 17px; margin: 0 0 12px; } .muted { color: var(--muted); }
  button, .btn { font: inherit; border-radius: 10px; border: 1px solid var(--line); padding: 9px 16px; background: var(--card); color: var(--text); cursor: pointer; text-decoration: none; }
  button.primary { background: var(--accent); border-color: var(--accent); color: #fff; } .row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  input[type=number] { font: inherit; width: 110px; padding: 8px 10px; border-radius: 10px; border: 1px solid var(--line); background: var(--card); color: var(--text); }
  label.choice { border: 1px solid var(--line); border-radius: 10px; padding: 8px 12px; cursor: pointer; } label.choice:has(input:checked) { border-color: var(--accent); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); gap: 8px; margin-top: 8px; } .grid img { width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; }
  code { font-size: 13px; } .badge { display: inline-block; padding: 2px 8px; border-radius: 99px; background: var(--line); font-size: 12px; }`

function page(title, content) {
  return `<!doctype html><html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>${escapeHtml(title)}</title><style>${PAGE_STYLE}</style></head><body><main>${content}</main></body></html>`
}

const redirect = (res, location) => send(res, 303, '', { Location: location })

/** "12 / 200 photos ce mois-ci", or the unlimited equivalent. */
function usageText(acc) {
  const quota = PLANS[acc.plan].quota
  const used = usage(acc).used
  const reset = new Date(resetsAt()).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', timeZone: 'UTC' })
  return quota === null
    ? `illimité (${used} photo(s) comptée(s) ce mois-ci)`
    : `${used} / ${quota} photos ce mois-ci · remise à zéro le ${reset}`
}

function consentPage(id) {
  const s = sessions.get(id)
  if (!s || s.status !== 'pending') {
    return page('Pelli Instant', `<div class="card"><h1>Demande expirée</h1><p class="muted">Relancez la connexion depuis Pelli Instant.</p></div>`)
  }
  const choices = Object.entries(PLANS)
    .map(([key, p]) => {
      const acc = state.accounts[key]
      const now = acc && acc.plan !== key ? ` → ${escapeHtml(PLANS[acc.plan].label.fr)}` : ''
      const used = acc ? ` <span class="muted">· ${usage(acc).used} triée(s)</span>` : ''
      return `<label class="choice"><input type="radio" name="account" value="${key}"${key === s.account ? ' checked' : ''}> ${escapeHtml(p.label.fr)}${now}${used}</label>`
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
        <p class="row" style="margin:16px 0">
          <span class="muted">Photos déjà triées ce mois-ci :</span>
          <input type="number" name="used" min="0" placeholder="inchangé">
        </p>
        <div class="row">
          <button class="primary" name="decision" value="approve">Autoriser</button>
          <button name="decision" value="deny">Refuser</button>
        </div>
      </form></div>`,
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
    <p class="muted">${escapeHtml(PLANS[acc.plan].label.fr)} · ${usageText(acc)} · ${devices} appareil(s) connecté(s)</p>
    <form method="post" action="/fake/plan?account=${acc.key}" class="row"><span class="muted">Forfait :</span>${planButtons}</form>
    <form method="post" action="/fake/usage?account=${acc.key}" class="row" style="margin-top:12px">
      <span class="muted">Photos triées ce mois-ci :</span>
      <input type="number" name="used" min="0" value="${usage(acc).used}"><button>Enregistrer</button>
      <button name="used" value="0">Remettre à zéro</button>
    </form></div>`
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
      <p class="muted">API : <code>${BASE}/api/cull</code> · offres : <a href="/api/cull/offers?lang=fr">fr</a>, <a href="/api/cull/offers?lang=en">en</a></p>
      <form method="post" action="/fake/reset" class="row"><button>Tout effacer (comptes, galeries, jetons)</button></form></div>
      ${accounts || '<div class="card muted">Aucun compte : connectez Pelli Instant pour en créer un.</div>'}
      ${galleries || '<div class="card muted">Aucune galerie pour le moment.</div>'}`,
  )
}

/** Where the offer links lead in Pelli (sign-up, billing). */
function offerPage(url) {
  const what = url.searchParams.get('utm_content') || url.pathname
  return page('Pelli', `<div class="card"><span class="badge">Faux Pelli · développement</span><h1>Lien « ${escapeHtml(what)} »</h1>
    <p class="muted">Ici, Pelli ouvrirait <code>${escapeHtml(url.pathname + url.hash)}</code> (inscription, formules, essai, facturation).</p>
    <p><a class="btn" href="/">Voir le banc</a></p></div>`)
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

const server = createServer(async (req, res) => {
  const url = new URL(req.url, BASE)
  const lang = langOf(req.headers['accept-language'])
  try {
    if (req.method === 'GET' && url.pathname === '/api/cull/offers') {
      return send(res, 200, offers(langOf(url.searchParams.get('lang') || req.headers['accept-language'])))
    }
    if (req.method === 'POST' && url.pathname.startsWith('/api/cull/')) {
      const handler = routes[url.pathname.slice('/api/cull/'.length)]
      if (!handler) return fail(res, 404, 'not_found')
      return await handler(req, res, lang)
    }
    if (req.method === 'POST' && url.pathname === '/api/photo') return await upload(req, res, url)
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
          const used = Number.parseInt(form.get('used') ?? '', 10)
          if (Number.isInteger(used) && used >= 0) usage(accountFor(s.account)).used = used
          accountFor(s.account)
          save()
        }
      }
      const message = approve ? 'Vous pouvez revenir dans Pelli Instant.' : 'Accès refusé.'
      return send(res, 200, page('Pelli Instant', `<div class="card"><h1>${message}</h1><p><a class="btn" href="/">Voir le banc</a></p></div>`))
    }
    const acc = state.accounts[url.searchParams.get('account')]
    if (req.method === 'POST' && url.pathname === '/fake/plan' && acc) {
      const plan = (await readForm(req)).get('plan')
      if (PLANS[plan]) setPlan(acc, plan)
      save()
      console.log(`[fake-pelli] ${acc.key}: plan ${acc.plan}`)
      return redirect(res, '/')
    }
    if (req.method === 'POST' && url.pathname === '/fake/usage' && acc) {
      const used = Number.parseInt((await readForm(req)).get('used') ?? '', 10)
      if (Number.isInteger(used) && used >= 0) usage(acc).used = used
      save()
      return redirect(res, '/')
    }
    if (req.method === 'POST' && url.pathname === '/fake/delete-gallery') {
      const g = state.galleries.find((x) => x.id === url.searchParams.get('id'))
      if (g) g.status = 'deleted'
      save()
      return redirect(res, '/')
    }
    if (req.method === 'POST' && url.pathname === '/fake/reset') {
      Object.assign(state, { accounts: {}, tokens: {}, galleries: [], folders: [], requests: {} })
      save()
      return redirect(res, '/')
    }
    if (req.method === 'GET' && (url.pathname === '/register' || url.pathname === '/dashboard/billing')) {
      return send(res, 200, offerPage(url))
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
  console.log(`[fake-pelli] launch the app with PELLI_API_URL=${BASE}`)
})
