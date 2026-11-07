import { Spinner } from '@heroui/react'
import { Sparkles, X } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { modLabel } from '@/lib/platform'
import { useAppStore } from '@/store/app-store'
import { useReviewStore } from '@/store/review-store'
import { useWorkspaceStore } from '@/store/workspace-store'

/** Natural-language search ("mariée bouquet"), run by the engine's CLIP model. */
export function SearchField({ autoFocus = false }: { autoFocus?: boolean }) {
  const t = useT()
  const search = useReviewStore((s) => s.search)
  const runSearch = useReviewStore((s) => s.runSearch)
  const clearSearch = useReviewStore((s) => s.clearSearch)
  const focusTick = useWorkspaceStore((s) => s.searchFocusTick)
  const available = useAppStore((s) => s.appInfo?.capabilities.textSearch !== false)
  const [value, setValue] = useState(search.query)
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (focusTick > 0) {
      input.current?.focus()
      input.current?.select()
    }
  }, [focusTick])
  useEffect(() => {
    if (!search.query) setValue('')
  }, [search.query])

  const active = search.hits !== null || search.status === 'loading'

  return (
    <div
      className={cn(
        'group flex h-8 w-full items-center gap-2 rounded-full border px-3 transition-all duration-200 ease-out',
        'border-default-200/80 bg-default-100/50 focus-within:border-[#5840f0]/40 focus-within:bg-[#5840f0]/[0.04] dark:border-white/[0.07] dark:bg-white/[0.03] dark:focus-within:border-[#a599f5]/40 dark:focus-within:bg-[#5840f0]/10',
        active && 'border-[#5840f0]/30 bg-[#5840f0]/[0.05] dark:border-[#a599f5]/30 dark:bg-[#5840f0]/10',
        !available && 'opacity-50',
      )}
      title={available ? undefined : t('review.search.unavailable')}
    >
      {search.status === 'loading' ? (
        <Spinner size="sm" color="current" className="size-3.5 text-[#5840f0] dark:text-[#a599f5]" classNames={{ wrapper: 'size-3.5' }} />
      ) : (
        <Sparkles className="size-3.5 shrink-0 text-[#5840f0] dark:text-[#a599f5]" aria-hidden />
      )}
      <input
        ref={input}
        autoFocus={autoFocus}
        type="search"
        value={value}
        disabled={!available}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            void runSearch(value)
          } else if (e.key === 'Escape') {
            e.preventDefault()
            setValue('')
            clearSearch()
            input.current?.blur()
          }
        }}
        placeholder={t('review.search.placeholder')}
        aria-label={t('review.search.badge')}
        className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-foreground outline-none placeholder:text-default-400 [&::-webkit-search-cancel-button]:hidden"
      />
      {value || active ? (
        <button
          type="button"
          onClick={() => {
            setValue('')
            clearSearch()
          }}
          aria-label={t('review.search.clear')}
          className="focus-ring flex size-5 items-center justify-center rounded-md text-default-400 hover:bg-default-200/60 hover:text-foreground"
        >
          <X className="size-3.5" />
        </button>
      ) : (
        <kbd className="hidden rounded-md border border-default-200 px-1.5 font-sans text-[10px] text-default-400 group-hover:inline dark:border-white/10">{modLabel()} F</kbd>
      )}
    </div>
  )
}
