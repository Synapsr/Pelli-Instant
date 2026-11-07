/**
 * Plural message, chosen with `Intl.PluralRules` from the `count` variable.
 * `few` and `many` are the extra forms some languages need (Polish: 2–4 and
 * 5+ photos); a missing form falls back to `other`.
 */
export interface Plural {
  zero?: string
  one: string
  few?: string
  many?: string
  other: string
}

export type MessageValue = string | Plural
export interface MessageTree {
  [key: string]: MessageValue | MessageTree
}

/** Same shape as the reference catalogue, every leaf widened to string/Plural. */
export type Catalogue<T> = {
  [K in keyof T]: T[K] extends string ? string : T[K] extends Plural ? Plural : Catalogue<T[K]>
}

/** Dot paths of every leaf ("review.tabs.selected"). */
export type MessageKey<T, Prefix extends string = ''> = {
  [K in keyof T & string]: T[K] extends string | Plural ? `${Prefix}${K}` : MessageKey<T[K], `${Prefix}${K}.`>
}[keyof T & string]

export type Vars = Record<string, string | number>
