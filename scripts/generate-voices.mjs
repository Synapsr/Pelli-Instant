// Generates the voice-over of the welcome tour with ElevenLabs, from
// scripts/voice/narration.json, into src/assets/onboarding/voice/<lang>/<step>.mp3.
//
//   ELEVENLABS_API_KEY=… pnpm voices            # only the lines that changed
//   ELEVENLABS_API_KEY=… pnpm voices --force    # everything
//   ELEVENLABS_API_KEY=… pnpm voices fr de      # some languages
//
// The key is read from the environment and never written anywhere. The
// generated files are committed: the app ships them and never calls
// ElevenLabs. A manifest records what each file was made from, so a line
// is generated again only when its text, voice or model changes.
import { createHash } from 'node:crypto'
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const source = JSON.parse(await readFile(join(root, 'scripts/voice/narration.json'), 'utf8'))
const outDir = join(root, 'src/assets/onboarding/voice')
const manifestPath = join(outDir, 'manifest.json')

const args = process.argv.slice(2)
const force = args.includes('--force')
const only = args.filter((a) => !a.startsWith('--'))
const key = process.env.ELEVENLABS_API_KEY
if (!key) {
  console.error('ELEVENLABS_API_KEY is not set.')
  process.exit(1)
}

let manifest = {}
try {
  manifest = JSON.parse(await readFile(manifestPath, 'utf8'))
} catch {
  /* first run */
}

const jobs = []
for (const [lang, lines] of Object.entries(source.lines)) {
  if (only.length && !only.includes(lang)) continue
  const voice = source.voices[lang]
  if (!voice) throw new Error(`no voice for ${lang}`)
  const spellings = Object.entries(source.pronounce?.[lang] ?? {})
  for (const [step, line] of Object.entries(lines)) {
    const text = spellings.reduce((t, [word, spoken]) => t.replaceAll(word, spoken), line)
    const id = `${lang}/${step}`
    const hash = createHash('sha256').update(JSON.stringify([source.model, source.format, voice.id, text])).digest('hex').slice(0, 16)
    if (!force && manifest[id]?.hash === hash) continue
    jobs.push({ id, lang, step, text, voice, hash })
  }
}

console.log(`${jobs.length} line(s) to generate`)

async function generate(job) {
  const url = `https://api.elevenlabs.io/v1/text-to-speech/${job.voice.id}?output_format=${source.format}`
  for (let attempt = 1; ; attempt++) {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'xi-api-key': key, 'content-type': 'application/json', accept: 'audio/mpeg' },
      body: JSON.stringify({ text: job.text, model_id: source.model, language_code: job.lang }),
    })
    if (res.ok) {
      const audio = Buffer.from(await res.arrayBuffer())
      const file = join(outDir, job.lang, `${job.step}.mp3`)
      await mkdir(dirname(file), { recursive: true })
      await writeFile(file, audio)
      manifest[job.id] = { hash: job.hash, voice: job.voice.name, bytes: audio.length }
      console.log(`✓ ${job.id} (${Math.round(audio.length / 1024)} KB)`)
      return
    }
    const body = await res.text()
    if ((res.status === 429 || res.status >= 500) && attempt < 4) {
      await new Promise((r) => setTimeout(r, 1500 * attempt))
      continue
    }
    throw new Error(`${job.id}: ${res.status} ${body.slice(0, 300)}`)
  }
}

// A few at a time (the plan's concurrency limit).
const queue = [...jobs]
const failures = []
await Promise.all(
  Array.from({ length: 3 }, async () => {
    for (let job = queue.shift(); job; job = queue.shift()) {
      try {
        await generate(job)
      } catch (error) {
        failures.push(error.message)
        console.error(`✗ ${error.message}`)
      }
    }
  }),
)

// A step or language removed from the narration takes its clip with it.
const wanted = new Set(Object.entries(source.lines).flatMap(([lang, lines]) => Object.keys(lines).map((step) => `${lang}/${step}`)))
for (const id of Object.keys(manifest)) {
  if (wanted.has(id)) continue
  await rm(join(outDir, `${id}.mp3`), { force: true })
  delete manifest[id]
  console.log(`- ${id}`)
}

await mkdir(outDir, { recursive: true })
const sorted = Object.fromEntries(Object.entries(manifest).sort(([a], [b]) => a.localeCompare(b)))
await writeFile(manifestPath, `${JSON.stringify(sorted, null, 2)}\n`)
if (failures.length) process.exit(1)
