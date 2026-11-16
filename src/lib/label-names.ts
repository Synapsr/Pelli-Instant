import type { ColorLabel, Language } from '@/lib/contract'

type Color = Exclude<ColorLabel, 'none'>

/**
 * The colour names of Lightroom Classic's default Color Label Set in each
 * language (the items of Photo › Set Color Label). Lightroom shows a label
 * in colour only when its text is one of the names of its set, and shows
 * any other text as a white "custom label": the sidecars get the names of
 * the photographer's Lightroom. Same table as `LabelLanguage::names` in
 * `crates/cull-types/src/lib.rs`.
 */
export const LABEL_NAMES: Record<Language, Record<Color, string>> = {
  en: { red: 'Red', yellow: 'Yellow', green: 'Green', blue: 'Blue', purple: 'Purple' },
  fr: { red: 'Rouge', yellow: 'Jaune', green: 'Vert', blue: 'Bleu', purple: 'Violet' },
  es: { red: 'Rojo', yellow: 'Amarillo', green: 'Verde', blue: 'Azul', purple: 'Morado' },
  de: { red: 'Rot', yellow: 'Gelb', green: 'Grün', blue: 'Blau', purple: 'Lila' },
  it: { red: 'Rosso', yellow: 'Giallo', green: 'Verde', blue: 'Blu', purple: 'Viola' },
  pt: { red: 'Vermelho', yellow: 'Amarelo', green: 'Verde', blue: 'Azul', purple: 'Roxo' },
  nl: { red: 'Rood', yellow: 'Geel', green: 'Groen', blue: 'Blauw', purple: 'Paars' },
  pl: { red: 'Czerwony', yellow: 'Żółty', green: 'Zielony', blue: 'Niebieski', purple: 'Fioletowy' },
  sv: { red: 'Röd', yellow: 'Gul', green: 'Grön', blue: 'Blå', purple: 'Lila' },
}

/** "Rouge, Jaune, Vert…": enough names to recognise the set. */
export function labelNamesSample(language: Language): string {
  const names = LABEL_NAMES[language]
  return `${names.red}, ${names.yellow}, ${names.green}…`
}

/** The text a label gets in the sidecars of a Lightroom in `language` (null: no label). */
export function labelName(label: ColorLabel, language: Language): string | null {
  return label === 'none' ? null : LABEL_NAMES[language][label]
}

/** The language of the label names: the one the photographer chose, else the app's. */
export function resolveLabelLanguage(chosen: Language | null | undefined, appLanguage: Language): Language {
  return chosen ?? appLanguage
}
