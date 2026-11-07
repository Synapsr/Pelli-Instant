import type { CullApi } from '@/lib/contract'

export type Transport = 'tauri' | 'http' | 'mock'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

export function detectTransport(): Transport {
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) return 'tauri'
  if (import.meta.env.VITE_BRIDGE_URL) return 'http'
  return 'mock'
}

let current: CullApi | null = null
let transport: Transport = 'mock'

/** Loads the transport (each lives in its own chunk) and keeps it as the app API. */
export async function initApi(): Promise<CullApi> {
  if (current) return current
  transport = detectTransport()
  if (transport === 'tauri') {
    const { createTauriApi } = await import('./tauri')
    current = createTauriApi()
  } else if (transport === 'http') {
    const { createHttpApi } = await import('./http')
    current = createHttpApi(String(import.meta.env.VITE_BRIDGE_URL))
  } else {
    const { createMockApi } = await import('./mock')
    current = createMockApi()
  }
  return current
}

/** The API once `initApi()` resolved (the app boots on it). */
export function api(): CullApi {
  if (!current) throw new Error('API not initialised: call initApi() first')
  return current
}

export function currentTransport(): Transport {
  return transport
}

/** Tests and stories inject their own implementation. */
export function setApi(next: CullApi, kind: Transport = 'mock'): void {
  current = next
  transport = kind
}

export { CullError, toCullError } from './errors'
