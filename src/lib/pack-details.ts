import type { ModelPackState } from '@/lib/contract'

/** A path inside the user's home folder, with the folder (and so the user name) replaced by `~`. */
export function redactHome(text: string): string {
  // A user folder may hold spaces (Windows): everything up to the next separator goes.
  return text.replace(/(?:\/Users\/|\/home\/)[^/\\\n]+/g, '~').replace(/[A-Za-z]:[\\/]Users[\\/][^\\/\n]+/gi, '~')
}

/** What "Details" shows and copies for a failed pack: code, message, app and processor. Never a home path. */
export function packFailureDetails(state: Extract<ModelPackState, { status: 'failed' }>, app?: { version: string; platform: string; arch: string } | null): string {
  const lines = [`${state.code} — ${redactHome(state.message)}`]
  if (app) lines.push(`Pelli Instant ${app.version} · ${app.platform} ${app.arch}`)
  return lines.join('\n')
}
