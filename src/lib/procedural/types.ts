/**
 * Procedural "photos" used by the mock engine (and the sign-in illustration):
 * a shot is a small scene description rendered on a canvas. People's head
 * positions are exact, so detected faces (bbox) line up with the picture.
 */

export type SceneKind = 'prep' | 'ceremony' | 'groups' | 'couple' | 'cocktail' | 'dinner' | 'party'
export type DetailKind = 'rings' | 'bouquet' | 'glasses' | 'cake' | 'table'
export type Role = 'bride' | 'groom' | 'guest' | 'child' | 'officiant'
export type HairStyle = 'short' | 'long' | 'bun' | 'bald'

export interface Person {
  /** head centre, normalised to the image width */
  x: number
  /** head centre, normalised to the image height */
  y: number
  /** head height, normalised to the image height */
  size: number
  role: Role
  skin: string
  hair: string
  hairStyle: HairStyle
  outfit: string
  veil: boolean
  /** eye openness (right, left), 0 closed … 1 open */
  eyes: [number, number]
  /** head turn, -1 (left) … 1 (right) */
  turn: number
  smile: number
}

export interface Shot {
  seed: number
  kind: SceneKind
  /** background layout seed (shared by the frames of a moment) */
  backdrop: number
  /** width / height */
  aspect: number
  people: Person[]
  detail: DetailKind | null
  /** global focus miss, 0 … 1 */
  defocus: number
  /** subject soft while the frame is sharp, 0 … 1 */
  subjectDefocus: number
  /** camera shake / subject motion, 0 … 1 */
  motion: number
  motionAngle: number
  /** -1 (black) … 0 … 1 (blown) */
  exposure: number
}

export interface NormBox {
  x: number
  y: number
  w: number
  h: number
}

/** Face box of a person, normalised to the image. */
export function faceBox(person: Person, aspect: number): NormBox {
  const h = person.size
  const w = (person.size * 0.76) / aspect
  return { x: person.x - w / 2, y: person.y - h / 2, w, h }
}
