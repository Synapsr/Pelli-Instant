import { COMMANDS, type CullApi, type CullEvents } from '@/lib/contract'
import { CullError } from './errors'
import { createInvokeApi, type Subscribe } from './invoke-api'

/**
 * Development transport against the engine's HTTP bridge (`pelli-cull serve`):
 * `POST {base}/invoke/{command}` with the JSON arguments, events as SSE on
 * `GET {base}/events`.
 */
export function createHttpApi(baseUrl: string): CullApi {
  const base = baseUrl.replace(/\/+$/, '')

  async function invoke(command: string, args: Record<string, unknown>): Promise<unknown> {
    // The bridge serves the engine alone: there is no app to update.
    if (command === COMMANDS.updateStatus || command === COMMANDS.checkForUpdate) return { status: 'disabled' }
    if (command === COMMANDS.installUpdate) return null
    const response = await fetch(`${base}/invoke/${command}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(args),
    })
    const text = await response.text()
    const body: unknown = text ? JSON.parse(text) : null
    if (!response.ok) {
      const error = (body as { error?: { code?: string; message?: string } } | null)?.error
      throw new CullError(error?.code ?? `http${response.status}`, error?.message ?? response.statusText)
    }
    return body
  }

  let source: EventSource | null = null
  let listeners = 0

  const on: Subscribe = <E extends keyof CullEvents>(event: E, handler: (payload: CullEvents[E]) => void) => {
    source ??= new EventSource(`${base}/events`)
    listeners += 1
    const listener = (e: MessageEvent<string>) => {
      try {
        handler(JSON.parse(e.data) as CullEvents[E])
      } catch (error) {
        console.error(`[bridge] bad payload for ${event}`, error)
      }
    }
    const current = source
    current.addEventListener(event, listener as EventListener)
    return () => {
      current.removeEventListener(event, listener as EventListener)
      listeners -= 1
      if (listeners === 0 && source === current) {
        current.close()
        source = null
      }
    }
  }

  return createInvokeApi(invoke, on)
}
