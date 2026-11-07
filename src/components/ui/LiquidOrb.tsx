import { type ReactNode, useEffect, useRef, useState } from 'react'
import { cn } from '@/lib/cn'

/*
 * A liquid sphere rendered by a WebGL2 fragment shader (ray marching, no
 * library): it breathes slowly, and reaches for the pointer like a drop
 * drawn by a magnet. Shape: a sphere displaced by slow interfering waves,
 * smoothly merged with a small "satellite" sphere that springs towards the
 * pointer. Colours: Pelli violet only (#5840f0 and its tints).
 *
 * Coordinates: the canvas spans [-1, 1] world units and the sphere radius
 * is ORB_RADIUS of that: the canvas is larger than the orb (size / 0.42) so
 * the reach and the halo never touch its edges, and the halo fades out
 * before them anyway. Falls back to a plain glass disc without WebGL2.
 */

const ORB_RADIUS = 0.42

const VERT = `#version 300 es
in vec2 aPos;
void main() { gl_Position = vec4(aPos, 0.0, 1.0); }`

const FRAG = `#version 300 es
precision highp float;
uniform vec2 uRes;
uniform float uTime;
uniform float uRadius;
uniform vec3 uOffset;
uniform vec3 uSat;
uniform float uSatR;
uniform float uEnergy;
uniform vec2 uLook;
out vec4 outColor;

const vec3 BRAND = vec3(0.345, 0.251, 0.941);
const vec3 MUTED = vec3(0.647, 0.600, 0.961);
const vec3 DEEP = vec3(0.040, 0.031, 0.105);

float smin(float a, float b, float k) {
  float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

// Slow organic motion: two octaves of interfering waves.
float waves(vec3 p, float t) {
  float a = sin(p.x * 3.1 + t * 0.9) * sin(p.y * 2.7 - t * 0.7) * sin(p.z * 2.9 + t * 0.5);
  float b = sin(p.x * 5.3 - t * 1.1 + p.z * 1.7) * sin(p.y * 6.1 + t * 0.8) * 0.5;
  return a + b;
}

float map(vec3 p) {
  vec3 q = p - uOffset;
  float d = length(q) - uRadius + waves(q * 1.4, uTime) * (0.017 + 0.012 * uEnergy);
  float s = length(p - uSat) - uSatR;
  return smin(d, s, 0.2);
}

vec3 normalAt(vec3 p) {
  const vec2 k = vec2(1.0, -1.0);
  const float e = 0.0015;
  return normalize(
    k.xyy * map(p + k.xyy * e) + k.yyx * map(p + k.yyx * e) +
    k.yxy * map(p + k.yxy * e) + k.xxx * map(p + k.xxx * e));
}

void main() {
  vec2 uv = (gl_FragCoord.xy / uRes) * 2.0 - 1.0;
  vec3 ro = vec3(0.0, 0.0, 3.2);
  vec3 rd = normalize(vec3(uv * 0.3125, -1.0));
  float t = 1.9;
  float dmin = 1e3;
  bool hit = false;
  for (int i = 0; i < 80; i++) {
    float d = map(ro + rd * t);
    dmin = min(dmin, d);
    if (d < 0.0007) { hit = true; break; }
    t += d * 0.8;
    if (t > 4.5) break;
  }

  if (!hit) {
    // Halo hugging the shape, and a one-pixel soft edge (anti-aliasing).
    // The halo fades out well before the canvas border: no visible cut.
    float px = 2.0 / uRes.y;
    float edge = clamp(1.0 - dmin / (px * 1.5), 0.0, 1.0);
    float fade = 1.0 - smoothstep(0.62, 0.96, length(uv));
    float glow = exp(-max(dmin, 0.0) * 9.0) * (0.3 + 0.22 * uEnergy) * fade;
    float a = clamp(glow + edge * 0.85, 0.0, 1.0);
    vec3 col = BRAND * glow + MUTED * edge * 0.75;
    outColor = vec4(min(col, vec3(a)), a);
    return;
  }

  vec3 p = ro + rd * t;
  vec3 n = normalAt(p);
  vec3 v = -rd;
  float ndv = clamp(dot(n, v), 0.0, 1.0);
  // Key light drifting slowly: the highlight glides over the surface.
  vec3 L = normalize(vec3(-0.55 + 0.25 * sin(uTime * 0.23), 0.7 + 0.12 * cos(uTime * 0.31), 0.6));
  float dif = clamp(dot(n, L), 0.0, 1.0);
  float spec = pow(clamp(dot(n, normalize(L + v)), 0.0, 1.0), 80.0);
  // A second, softer light on the side of the pointer.
  float lookAmt = length(uLook);
  float look = lookAmt > 0.001 ? pow(clamp(dot(n, normalize(vec3(uLook, 0.45))), 0.0, 1.0), 5.0) * lookAmt : 0.0;
  // Liquid inside: slow veins seen through the surface.
  vec3 inner = refract(rd, n, 0.78);
  float veins = smoothstep(0.15, 0.95, waves(p * 1.6 + inner * 0.9, uTime * 0.6));
  float flow = 0.5 + 0.5 * sin(dot(n, vec3(1.7, 2.3, 0.9)) * 2.2 + uTime * 0.7);

  float rim = 1.0 - ndv;
  vec3 col = mix(DEEP, BRAND * 0.5, 0.18 + 0.5 * dif);
  col += BRAND * veins * 0.32 * ndv;
  col += mix(BRAND, MUTED, flow) * pow(rim, 2.2) * (0.85 + 0.35 * uEnergy);
  col += MUTED * pow(rim, 7.0) * 0.55;
  col += MUTED * look * 0.4;
  col += vec3(1.0) * spec * 0.7;
  outColor = vec4(col, 1.0);
}`

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader | null {
  const shader = gl.createShader(type)
  if (!shader) return null
  gl.shaderSource(shader, source)
  gl.compileShader(shader)
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    console.error('LiquidOrb shader:', gl.getShaderInfoLog(shader))
    gl.deleteShader(shader)
    return null
  }
  return shader
}

function createProgram(gl: WebGL2RenderingContext): WebGLProgram | null {
  const vs = compile(gl, gl.VERTEX_SHADER, VERT)
  const fs = compile(gl, gl.FRAGMENT_SHADER, FRAG)
  if (!vs || !fs) return null
  const program = gl.createProgram()
  if (!program) return null
  gl.attachShader(program, vs)
  gl.attachShader(program, fs)
  gl.linkProgram(program)
  gl.deleteShader(vs)
  gl.deleteShader(fs)
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    console.error('LiquidOrb program:', gl.getProgramInfoLog(program))
    return null
  }
  return program
}

/** Damped spring on one value (slightly under-damped: liquid, not rubber). */
class Spring {
  value: number
  velocity = 0
  constructor(value: number, private stiffness = 70, private damping = 11) {
    this.value = value
  }
  step(target: number, dt: number): number {
    const accel = this.stiffness * (target - this.value) - this.damping * this.velocity
    this.velocity += accel * dt
    this.value += this.velocity * dt
    return this.value
  }
}

const smoothstep = (a: number, b: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - a) / (b - a)))
  return t * t * (3 - 2 * t)
}

interface LiquidOrbProps {
  /** Diameter of the orb in CSS pixels (the canvas overflows it for the halo). */
  size: number
  /** A folder is dragged over the window: the orb swells and glows. */
  swell?: boolean
  /** Work in progress: the surface moves a little faster. */
  busy?: boolean
  /** The pointer is over the control the orb belongs to: it livens up. */
  hovered?: boolean
  /** Follow the pointer (off for a purely decorative orb). */
  magnetic?: boolean
  className?: string
  /** Content drawn over the orb (an icon); it follows the orb's lean. */
  children?: ReactNode
}

export function LiquidOrb({ size, swell = false, busy = false, hovered = false, magnetic = true, className, children }: LiquidOrbProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)
  const stateRef = useRef({ swell, busy, hovered })
  const [fallback, setFallback] = useState(false)
  stateRef.current = { swell, busy, hovered }

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const gl = canvas.getContext('webgl2', { alpha: true, premultipliedAlpha: true, antialias: false, powerPreference: 'low-power' })
    const program = gl ? createProgram(gl) : null
    if (!gl || !program) {
      setFallback(true)
      return
    }

    const dpr = Math.min(window.devicePixelRatio || 1, 2)
    const side = Math.round(size / ORB_RADIUS)
    canvas.width = Math.round(side * dpr)
    canvas.height = Math.round(side * dpr)
    gl.viewport(0, 0, canvas.width, canvas.height)

    const buffer = gl.createBuffer()
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer)
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
    const vao = gl.createVertexArray()
    gl.bindVertexArray(vao)
    const loc = gl.getAttribLocation(program, 'aPos')
    gl.enableVertexAttribArray(loc)
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0)
    gl.useProgram(program)

    const u = (name: string) => gl.getUniformLocation(program, name)
    const uRes = u('uRes')
    const uTime = u('uTime')
    const uRadius = u('uRadius')
    const uOffset = u('uOffset')
    const uSat = u('uSat')
    const uSatR = u('uSatR')
    const uEnergy = u('uEnergy')
    const uLook = u('uLook')
    gl.uniform2f(uRes, canvas.width, canvas.height)

    // Pointer in world units relative to the orb centre (null: away).
    let pointer: { x: number; y: number } | null = null
    const onMove = (e: PointerEvent) => {
      const rect = canvas.getBoundingClientRect()
      const half = rect.width / 2
      pointer = { x: (e.clientX - rect.left - half) / half, y: -(e.clientY - rect.top - half) / half }
    }
    const onLeave = () => {
      pointer = null
    }
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    if (magnetic && !reduceMotion) {
      window.addEventListener('pointermove', onMove)
      document.documentElement.addEventListener('pointerleave', onLeave)
      window.addEventListener('blur', onLeave)
    }

    const sat = { x: new Spring(0), y: new Spring(0), z: new Spring(0), r: new Spring(0.13) }
    const off = { x: new Spring(0, 60, 12), y: new Spring(0, 60, 12) }
    const look = { x: new Spring(0, 40, 12), y: new Spring(0, 40, 12) }
    let energy = 0
    let radius = ORB_RADIUS
    let time = 0
    let last = performance.now()
    let frame = 0

    const render = (now: number) => {
      const dt = Math.min(0.033, (now - last) / 1000)
      last = now
      const { swell: swelling, busy: working, hovered: zoneHovered } = stateRef.current

      let prox = 0
      let dir = { x: 0, y: 0 }
      let dist = 0
      if (pointer) {
        dist = Math.hypot(pointer.x, pointer.y)
        if (dist > 1e-4) dir = { x: pointer.x / dist, y: pointer.y / dist }
        prox = smoothstep(0, 1, 1 - (dist - 0.55) / 2.2)
      }
      // At rest the satellite wanders slowly just under the surface (a soft
      // travelling swell); near the pointer it leans towards it instead, a
      // little only: the orb stays round.
      const wander = { x: Math.cos(time * 0.37) * 0.27, y: Math.sin(time * 0.53) * 0.22, z: Math.sin(time * 0.29) * 0.12 }
      const reach = Math.min(dist, ORB_RADIUS * 0.82) * prox
      sat.x.step(wander.x * (1 - prox) + dir.x * reach, dt)
      sat.y.step(wander.y * (1 - prox) + dir.y * reach, dt)
      sat.z.step(wander.z * (1 - prox) + 0.06 * prox, dt)
      sat.r.step(0.13 + 0.03 * prox, dt)
      const lean = 0.03 * prox * smoothstep(0, 0.5, dist)
      off.x.step(dir.x * lean, dt)
      off.y.step(dir.y * lean, dt)
      look.x.step(dir.x * prox, dt)
      look.y.step(dir.y * prox, dt)

      const onOrb = pointer !== null && dist < ORB_RADIUS
      const energyTarget = swelling ? 1.6 : working ? 1.2 : onOrb ? 1 : zoneHovered ? 0.7 : 0.15 * prox
      energy += (energyTarget - energy) * Math.min(1, dt * 4)
      radius += ((swelling ? ORB_RADIUS * 1.08 : ORB_RADIUS) - radius) * Math.min(1, dt * 6)
      time += dt * (1 + energy * 0.5)
      const breath = 1 + 0.018 * Math.sin(time * 0.9)

      gl.uniform1f(uTime, time)
      gl.uniform1f(uRadius, radius * breath)
      gl.uniform3f(uOffset, off.x.value, off.y.value, 0)
      gl.uniform3f(uSat, off.x.value + sat.x.value, off.y.value + sat.y.value, sat.z.value)
      gl.uniform1f(uSatR, sat.r.value)
      gl.uniform1f(uEnergy, energy)
      gl.uniform2f(uLook, look.x.value, look.y.value)
      gl.drawArrays(gl.TRIANGLES, 0, 3)

      // The overlaid icon leans with the orb, a little more (depth).
      const content = contentRef.current
      if (content) {
        const px = (side / 2) * 1.4
        content.style.transform = `translate3d(${(off.x.value * px).toFixed(2)}px, ${(-off.y.value * px).toFixed(2)}px, 0)`
      }
      if (!reduceMotion) frame = requestAnimationFrame(render)
    }
    frame = requestAnimationFrame(render)

    return () => {
      cancelAnimationFrame(frame)
      window.removeEventListener('pointermove', onMove)
      document.documentElement.removeEventListener('pointerleave', onLeave)
      window.removeEventListener('blur', onLeave)
      gl.deleteBuffer(buffer)
      gl.deleteVertexArray(vao)
      gl.deleteProgram(program)
      // Free the GPU context now rather than at garbage collection (WebKit
      // caps live contexts; the home screen remounts often). Only once the
      // canvas has left the page: a re-run of this effect on the same canvas
      // (StrictMode, prop change) gets the same context back and needs it.
      setTimeout(() => {
        if (!canvas.isConnected) gl.getExtension('WEBGL_lose_context')?.loseContext()
      }, 0)
    }
  }, [size, magnetic])

  return (
    // Positioned by the caller (absolute) or relative by default.
    <div className={cn(className ?? 'relative')} style={{ width: size, height: size }} aria-hidden>
      {fallback ? (
        <span className="glass absolute inset-0 rounded-full" />
      ) : (
        <canvas ref={canvasRef} className="pointer-events-none absolute left-1/2 top-1/2 max-w-none -translate-x-1/2 -translate-y-1/2" style={{ width: Math.round(size / ORB_RADIUS), height: Math.round(size / ORB_RADIUS) }} />
      )}
      {children && (
        <div ref={contentRef} className="absolute inset-0 flex items-center justify-center will-change-transform">
          {children}
        </div>
      )}
    </div>
  )
}
