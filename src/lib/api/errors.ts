import type { ErrorCode } from '@/lib/contract'

/** Error raised by any transport; `code` is an `ErrorCode` or an engine code. */
export class CullError extends Error {
  readonly code: ErrorCode | (string & {})
  /** structured context of some codes (QuotaDetails for the quota errors) */
  readonly details?: unknown

  constructor(code: ErrorCode | (string & {}), message: string, details?: unknown) {
    super(message)
    this.name = 'CullError'
    this.code = code
    this.details = details
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

/**
 * Normalises whatever a transport throws (Tauri rejects with the serialised
 * Rust error, fetch with TypeError, the bridge with `{ error: { code, message } }`).
 */
export function toCullError(error: unknown): CullError {
  if (error instanceof CullError) return error
  // fetch() rejects with a TypeError when the network (or the bridge) is unreachable.
  if (error instanceof TypeError) return new CullError('offline', error.message)
  if (isRecord(error)) {
    const inner = isRecord(error.error) ? error.error : error
    if (typeof inner.code === 'string') {
      return new CullError(inner.code, typeof inner.message === 'string' ? inner.message : inner.code, inner.details)
    }
  }
  if (error instanceof Error) return new CullError('internal', error.message)
  if (typeof error === 'string') return new CullError('internal', error)
  if (isRecord(error) && typeof error.message === 'string') return new CullError('internal', error.message)
  return new CullError('internal', 'Unknown error')
}
