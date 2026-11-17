import type { Language } from '@/lib/contract'

/** Markdown of a bullet reduced to plain text: links, bold, code and tags. */
function plain(text: string): string {
  return text
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/(\*\*|__)(.+?)\1/g, '$2')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/<[^>]+>/g, '')
    .trim()
}

/**
 * The top-level bullet points of a release's notes (its CHANGELOG.md section)
 * in the app language: English is the list before the `<details>` block, the
 * French list sits in `<details><summary>Français</summary>`. Other languages
 * get none: the announcement keeps its own words and links to the release.
 */
export function releaseHighlights(notes: string | null | undefined, language: Language, max = 5): string[] {
  if (!notes) return []
  let block: string | undefined
  if (language === 'fr') block = /<summary>\s*Français\s*<\/summary>([\s\S]*?)(?:<\/details>|$)/i.exec(notes)?.[1]
  else if (language === 'en') block = notes.split(/<details>/i)[0]
  if (!block) return []
  return block
    .split('\n')
    .map((line) => /^[-*]\s+(.+)$/.exec(line)?.[1])
    .filter((line): line is string => Boolean(line))
    .map(plain)
    .filter(Boolean)
    .slice(0, max)
}
