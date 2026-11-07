import { useEffect, useState } from 'react'
import { api } from '@/lib/api'

/**
 * FIFO cache of large-preview requests (the engine renders 2560 px previews on
 * demand). Kept small: the engine may recycle old preview files/URLs.
 */
const MAX = 20
const cache = new Map<string, Promise<string>>()

export function loadLargePreview(projectId: string, photoId: string): Promise<string> {
  const key = `${projectId}:${photoId}`
  let promise = cache.get(key)
  if (!promise) {
    promise = api().largePreview(projectId, photoId)
    promise.catch(() => cache.delete(key))
    cache.set(key, promise)
    while (cache.size > MAX) {
      const oldest = cache.keys().next().value
      if (oldest === undefined) break
      cache.delete(oldest)
    }
  }
  return promise
}

/** Large preview of `photoId`; neighbours are preloaded for instant navigation. */
export function useLargePreview(projectId: string | null, photoId: string | null, preload: (string | null | undefined)[] = []): string | null {
  const [state, setState] = useState<{ id: string | null; url: string | null }>({ id: null, url: null })
  const neighbours = preload.join('|')

  useEffect(() => {
    if (!projectId || !photoId) return
    let alive = true
    void loadLargePreview(projectId, photoId)
      .then((url) => {
        if (!alive) return
        setState({ id: photoId, url: url || null })
        neighbours
          .split('|')
          .filter(Boolean)
          .forEach((id) => void loadLargePreview(projectId, id).catch(() => undefined))
      })
      .catch(() => alive && setState({ id: photoId, url: null }))
    return () => {
      alive = false
    }
  }, [projectId, photoId, neighbours])

  return state.id === photoId ? state.url : null
}
