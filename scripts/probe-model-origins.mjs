#!/usr/bin/env node
// Checks that every model file is still downloadable from its origin: one
// HEAD request per file (redirects followed), expecting HTTP 200 and the
// size of models/manifest.json. Downloads nothing. Exits 1 when a file is
// missing or has another size. Node >= 20, no dependencies.
//
//   node scripts/probe-model-origins.mjs          # every origin
//   node scripts/probe-model-origins.mjs --urls   # "<file> <url>" of the Maximum pack's public
//                                                 #   origins, one per line (CI downloads them)
//
// The Standard models' origins are their `url` in the manifest. The Maximum
// pack has none there (the app gets short-lived links from Pelli, POST
// /api/cull/models): its public origins are listed below, the upstream
// exports at the manifest's revisions and Pelli's own conversions on its
// storage, plus the int8 DINOv2 of 1.1.0, which Pelli still hands out to
// that version and the CI keeps rejecting.

import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const manifest = JSON.parse(readFileSync(path.join(root, 'models', 'manifest.json'), 'utf8'))

const HF_CLIP = 'https://huggingface.co/Xenova/clip-vit-large-patch14/resolve/c307790166907339eed5a9a53a249af534102536/onnx'
const HF_DINO = 'https://huggingface.co/onnx-community/dinov2-base-ONNX/resolve/9a1732c724ef0847dcedf00d3d5a93f61dea0370/onnx'
const S3 = 'https://pelli.s3.fr-par.scw.cloud/pelli-instant/models'

/** File of the Maximum pack → its public origin. */
export const MAXIMUM_ORIGINS = {
  'clip-vit-l14-vision-int8.onnx': `${HF_CLIP}/vision_model_int8.onnx`,
  'clip-vit-l14-text-fp16.onnx': `${HF_CLIP}/text_model_fp16.onnx`,
  'dinov2-base-fp32.onnx': `${HF_DINO}/model.onnx`,
  'rfdetr-seg-small-384-w16.onnx': `${S3}/rfdetr-seg-small-384-w16.onnx`,
  'retinaface-r50-w16.onnx': `${S3}/retinaface-r50-w16.onnx`,
}
/** Retired: 1.1.0's int8 DINOv2 (not in the manifest any more). */
const RETIRED = { file: 'dinov2-base-int8.onnx', url: `${HF_DINO}/model_int8.onnx`, size: 90948899 }

function origins() {
  const list = []
  for (const m of manifest.models) {
    if (m.bundle === 'maximum') {
      const url = MAXIMUM_ORIGINS[m.file]
      if (!url) throw new Error(`no public origin listed for ${m.file} (scripts/probe-model-origins.mjs)`)
      list.push({ file: m.file, url, size: m.size })
    } else {
      list.push({ file: m.file, url: m.url, size: m.archive?.size ?? m.size })
    }
  }
  list.push(RETIRED)
  return list
}

async function probe({ file, url, size }) {
  for (let attempt = 1; ; attempt++) {
    try {
      const res = await fetch(url, { method: 'HEAD', redirect: 'follow', headers: { 'User-Agent': 'PelliInstantCI/1.0' } })
      const length = Number(res.headers.get('content-length') ?? res.headers.get('x-linked-size') ?? NaN)
      if (res.status === 200 && length === size) return { ok: true, line: `ok      ${file} (${size} bytes)` }
      const why = res.status !== 200 ? `HTTP ${res.status}` : `${length} bytes instead of ${size}`
      if (attempt === 3 || res.status === 404) return { ok: false, line: `FAILED  ${file}: ${why} at ${new URL(url).origin}` }
    } catch (e) {
      if (attempt === 3) return { ok: false, line: `FAILED  ${file}: ${e.message}` }
    }
    await new Promise((r) => setTimeout(r, 2000 * attempt))
  }
}

async function main() {
  const list = origins()
  if (process.argv.includes('--urls')) {
    for (const o of list.filter((o) => o.file in MAXIMUM_ORIGINS || o.file === RETIRED.file)) console.log(`${o.file} ${o.url}`)
    return
  }
  let failed = 0
  for (const o of list) {
    const r = await probe(o)
    console.log(r.line)
    if (!r.ok) failed++
  }
  if (failed) {
    console.error(`${failed} origin(s) unavailable`)
    process.exit(1)
  }
}

main().catch((e) => {
  console.error(e.message)
  process.exit(1)
})
