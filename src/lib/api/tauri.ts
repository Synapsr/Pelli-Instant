import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { CullApi, CullEvents } from '@/lib/contract'
import { createInvokeApi, type Subscribe } from './invoke-api'

/** Desktop transport: Tauri commands + Tauri events. */
export function createTauriApi(): CullApi {
  const on: Subscribe = <E extends keyof CullEvents>(event: E, handler: (payload: CullEvents[E]) => void) => {
    let disposed = false
    let unlisten: (() => void) | null = null
    void listen<CullEvents[E]>(event, (e) => handler(e.payload)).then((fn) => {
      if (disposed) fn()
      else unlisten = fn
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }
  return createInvokeApi((command, args) => invoke(command, args), on)
}
