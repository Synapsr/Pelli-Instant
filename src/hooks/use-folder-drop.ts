import { useEffect, useRef, useState } from 'react'
import { currentTransport } from '@/lib/api'

const MOCK_ROOT = '/Users/camille/Photos/2026'

/**
 * Folder drag & drop: Tauri webview events on desktop (real paths), HTML5
 * drag events in the browser (mock/bridge: the folder name only).
 */
export function useFolderDrop(onDrop: (path: string) => void, enabled = true): { over: boolean } {
  const [over, setOver] = useState(false)
  const handler = useRef(onDrop)
  handler.current = onDrop

  useEffect(() => {
    if (!enabled) return
    if (currentTransport() === 'tauri') {
      let unlisten: (() => void) | null = null
      let disposed = false
      void import('@tauri-apps/api/webview').then(({ getCurrentWebview }) =>
        getCurrentWebview()
          .onDragDropEvent((event) => {
            const payload = event.payload
            if (payload.type === 'enter' || payload.type === 'over') setOver(true)
            else if (payload.type === 'leave') setOver(false)
            else if (payload.type === 'drop') {
              setOver(false)
              const path = payload.paths[0]
              if (path) handler.current(path)
            }
          })
          .then((fn) => {
            if (disposed) fn()
            else unlisten = fn
          }),
      )
      return () => {
        disposed = true
        unlisten?.()
      }
    }

    let depth = 0
    const enter = (e: DragEvent) => {
      if (!e.dataTransfer?.types.includes('Files')) return
      e.preventDefault()
      depth += 1
      setOver(true)
    }
    const overEvt = (e: DragEvent) => {
      if (e.dataTransfer?.types.includes('Files')) e.preventDefault()
    }
    const leave = () => {
      depth = Math.max(0, depth - 1)
      if (depth === 0) setOver(false)
    }
    const drop = (e: DragEvent) => {
      e.preventDefault()
      depth = 0
      setOver(false)
      const item = e.dataTransfer?.items[0]
      const entry = item?.webkitGetAsEntry?.()
      const name = entry?.name ?? e.dataTransfer?.files[0]?.name
      if (name) handler.current(`${MOCK_ROOT}/${name}`)
    }
    window.addEventListener('dragenter', enter)
    window.addEventListener('dragover', overEvt)
    window.addEventListener('dragleave', leave)
    window.addEventListener('drop', drop)
    return () => {
      window.removeEventListener('dragenter', enter)
      window.removeEventListener('dragover', overEvt)
      window.removeEventListener('dragleave', leave)
      window.removeEventListener('drop', drop)
    }
  }, [enabled])

  return { over }
}
