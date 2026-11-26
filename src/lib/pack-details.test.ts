import { describe, expect, it } from 'vitest'
import { packFailureDetails, redactHome } from './pack-details'

describe('pack failure details', () => {
  it('replace the home folder, never show the user name', () => {
    expect(redactHome('/Users/camille/Library/Application Support/io.pelli.instant/models/maximum/a.onnx: denied')).toBe(
      '~/Library/Application Support/io.pelli.instant/models/maximum/a.onnx: denied',
    )
    expect(redactHome('C:\\Users\\Camille Dupont\\AppData\\x')).toBe('~\\AppData\\x')
    expect(redactHome('C:\\Users\\camille\\AppData\\Roaming\\io.pelli.instant')).toBe('~\\AppData\\Roaming\\io.pelli.instant')
    expect(redactHome('/home/camille/.local/share/x')).toBe('~/.local/share/x')
    expect(redactHome('dinov2-b14-fp32: wrong results')).toBe('dinov2-b14-fp32: wrong results')
  })

  it('give the code, the message and the app', () => {
    const text = packFailureDetails(
      { status: 'failed', code: 'checkFailed', message: 'clip-l14-vision-int8: wrong results (correlation 0.412 with the reference, at least 0.85 expected)', receivedBytes: 1, totalBytes: 1 },
      { version: '1.1.1', platform: 'macos', arch: 'aarch64' },
    )
    expect(text).toBe('checkFailed — clip-l14-vision-int8: wrong results (correlation 0.412 with the reference, at least 0.85 expected)\nPelli Instant 1.1.1 · macos aarch64')
  })
})
