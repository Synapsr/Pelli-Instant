import { canRender, renderShotUrl } from '@/lib/procedural/render'
import type { Shot } from '@/lib/procedural/types'

const THUMB_EDGE = 440
const LARGE_EDGE = 2048
const LARGE_CACHE = 28

interface Job {
  key: string
  shot: Shot
  resolve: (url: string | null) => void
}

/**
 * Procedural previews of the mock engine, encoded as JPEG object URLs.
 * Thumbnails are rendered through a time-sliced queue so the UI stays
 * responsive while a few thousand of them are generated.
 */
export class MockImages {
  readonly enabled: boolean
  private readonly thumbs = new Map<string, string>()
  private readonly pending = new Map<string, Promise<string | null>>()
  private readonly large = new Map<string, string>()
  private queue: Job[] = []
  private running = false
  /** large previews in progress: the thumbnail queue yields to them */
  private largeBusy = 0

  constructor(enabled = canRender()) {
    this.enabled = enabled
  }

  cached(key: string): string | null {
    return this.thumbs.get(key) ?? null
  }

  /** Queues a thumbnail (deduplicated); resolves to null without canvas. */
  thumb(key: string, shot: Shot, urgent = false): Promise<string | null> {
    const hit = this.thumbs.get(key)
    if (hit) return Promise.resolve(hit)
    if (!this.enabled) return Promise.resolve(null)
    const existing = this.pending.get(key)
    if (existing) return existing
    const promise = new Promise<string | null>((resolve) => {
      const job = { key, shot, resolve }
      if (urgent) this.queue.unshift(job)
      else this.queue.push(job)
    })
    this.pending.set(key, promise)
    this.pump()
    return promise
  }

  async largePreview(key: string, shot: Shot): Promise<string> {
    const hit = this.large.get(key)
    if (hit) {
      this.large.delete(key)
      this.large.set(key, hit)
      return hit
    }
    if (!this.enabled) return ''
    this.largeBusy += 1
    let url: string
    try {
      url = await renderShotUrl(shot, LARGE_EDGE, 0.9)
    } finally {
      this.largeBusy -= 1
    }
    this.large.set(key, url)
    while (this.large.size > LARGE_CACHE) {
      const [oldKey, oldUrl] = this.large.entries().next().value as [string, string]
      this.large.delete(oldKey)
      URL.revokeObjectURL(oldUrl)
    }
    return url
  }

  private pump(): void {
    if (this.running) return
    this.running = true
    const step = () => {
      if (this.largeBusy > 0) {
        setTimeout(step, 16)
        return
      }
      const start = performance.now()
      while (this.queue.length && performance.now() - start < 12) {
        const job = this.queue.shift() as Job
        renderShotUrl(job.shot, THUMB_EDGE, 0.82)
          .then((url) => {
            this.thumbs.set(job.key, url)
            this.pending.delete(job.key)
            job.resolve(url)
          })
          .catch(() => {
            this.pending.delete(job.key)
            job.resolve(null)
          })
      }
      if (this.queue.length) setTimeout(step, 4)
      else this.running = false
    }
    setTimeout(step, 0)
  }
}
