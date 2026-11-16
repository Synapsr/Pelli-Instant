import { AnimatePresence, motion } from 'framer-motion'
import { ArrowLeft, FolderOpen, FolderPlus, Search, X } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
import { useT } from '@/lib/i18n'
import { useMediaQuery } from '@/hooks/use-media-query'
import { filterProjects, RECENT_COUNT, RECENT_COUNT_NARROW, SEARCH_THRESHOLD, sortProjects } from '@/lib/projects'
import { useAppStore } from '@/store/app-store'
import { DROP_ORB_LAYOUT_ID } from './DropZone'
import { ProjectCard } from './ProjectCard'

const EASE = [0.22, 1, 0.36, 1] as const
/** Parts of the view that are not shared with the home: they fade, quickly out, softly in. */
const FADE = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0, transition: { duration: 0.3, ease: EASE, delay: 0.12 } },
  exit: { opacity: 0, transition: { duration: 0.12, ease: 'easeIn' } },
} as const

/**
 * Every project, last opened first: the drop zone shrunk to a bar at the top
 * (the home's orb glides into it), a search field once there are many, and
 * the grid of shoots the recent cards glide into. Escape or Back returns.
 */
export function AllProjects({ over, onBack }: { over: boolean; onBack: () => void }) {
  const t = useT()
  const projects = useAppStore((s) => s.projects)
  const [query, setQuery] = useState('')
  const sorted = useMemo(() => sortProjects(projects), [projects])
  const shown = useMemo(() => filterProjects(sorted, query), [sorted, query])
  // The cards the home showed: they glide, the others fade in.
  const shownOnHome = useMediaQuery('(min-width: 1160px)') ? RECENT_COUNT : RECENT_COUNT_NARROW
  const recent = useMemo(() => new Set(projects.slice(0, shownOnHome).map((p) => p.id)), [projects, shownOnHome])
  const searchable = projects.length > SEARCH_THRESHOLD
  const search = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || useAppStore.getState().dialog !== null || e.defaultPrevented) return
      if (document.activeElement === search.current && search.current?.value) return
      e.preventDefault()
      onBack()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onBack])

  return (
    <motion.div className="absolute inset-0 flex flex-col" exit={{ opacity: 1, transition: { duration: 0.32 } }}>
      <div className="px-8 pt-1">
        <div className="mx-auto w-full max-w-[1308px]">
          <CompactDropZone over={over} />
        </div>
      </div>

      <motion.div {...FADE} className="mx-auto mt-6 flex w-full max-w-[1372px] items-center gap-3 px-8 short:mt-4">
        <button
          type="button"
          onClick={onBack}
          className="focus-ring -ml-2 inline-flex h-10 items-center gap-1.5 rounded-full pl-2 pr-3.5 text-[13px] font-medium text-white/60 transition-[color,background-color,scale] duration-150 hover:bg-white/[0.06] hover:text-white active:scale-[0.96]"
        >
          <ArrowLeft className="size-4" aria-hidden />
          {t('home.back')}
        </button>
        <h2 className="text-[17px] font-semibold tracking-[-0.01em] text-white">{t('home.allTitle')}</h2>
        <span className="tabular text-[13px] text-white/40">{t('home.allCount', { count: projects.length })}</span>
        {searchable && (
          <label className="glass ml-auto flex h-10 w-[280px] items-center gap-2 rounded-full pl-3.5 pr-1.5 transition-[background-color,box-shadow] duration-150 focus-within:shadow-[inset_0_0_0_1px_rgba(165,153,245,0.45)]">
            <Search className="size-4 shrink-0 text-white/45" aria-hidden />
            <input
              ref={search}
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Escape' && query) {
                  e.preventDefault()
                  setQuery('')
                }
              }}
              placeholder={t('home.searchProjects')}
              aria-label={t('home.searchProjects')}
              className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-white outline-none placeholder:text-white/35 [&::-webkit-search-cancel-button]:hidden"
            />
            {query && (
              <button
                type="button"
                onClick={() => {
                  setQuery('')
                  search.current?.focus()
                }}
                aria-label={t('review.search.clear')}
                className="focus-ring flex size-7 items-center justify-center rounded-full text-white/50 hover:bg-white/[0.08] hover:text-white"
              >
                <X className="size-3.5" />
              </button>
            )}
          </label>
        )}
      </motion.div>

      <motion.div layoutScroll className="relative min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
        <div className="mx-auto grid w-full max-w-[1372px] grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3 px-8 pb-10 pt-4">
          {shown.map((p, i) => (
            <ProjectCard key={p.id} project={p} index={i} fluid appear={recent.has(p.id) ? 'none' : 'fade'} exitFade={!recent.has(p.id)} />
          ))}
        </div>
        <AnimatePresence initial={false}>
          {shown.length === 0 && (
            <motion.p key="none" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} className="mt-10 text-center text-[13px] text-white/45">
              {t('home.noMatch', { query: query.trim() })}
            </motion.p>
          )}
        </AnimatePresence>
      </motion.div>
    </motion.div>
  )
}

/** The drop zone as a bar at the top of the full list: same orb, same action. */
function CompactDropZone({ over }: { over: boolean }) {
  const t = useT()
  const opening = useAppStore((s) => s.opening)
  const pickAndOpen = useAppStore((s) => s.pickAndOpen)
  const busy = opening !== null

  return (
    <motion.button
      type="button"
      onClick={() => {
        if (!busy) void pickAndOpen()
      }}
      aria-disabled={busy}
      aria-label={t('home.chooseFolder')}
      whileTap={busy ? undefined : { scale: 0.99 }}
      className="focus-ring group relative flex h-[76px] w-full items-center gap-4 rounded-[22px] px-3 text-left"
    >
      {/* The bar's surface fades on its own: the orb above it glides from the home. */}
      <motion.span
        aria-hidden
        className="glass glass-hover absolute inset-0 rounded-[22px] transition-[background-color,box-shadow] duration-200 group-hover:bg-white/[0.06]"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1, transition: { duration: 0.3, delay: 0.08 } }}
        exit={{ opacity: 0, transition: { duration: 0.12 } }}
      />
      <motion.div layoutId={DROP_ORB_LAYOUT_ID} transition={{ layout: { type: 'spring', duration: 0.3, bounce: 0 } }} className="relative size-14 shrink-0">
        <LiquidOrb size={56} swell={over} busy={busy} className="pointer-events-none absolute inset-0">
          {busy ? (
            <span className="block size-5 rounded-full border-2 border-white/15 border-t-white/90 [animation:spin-slow_0.9s_linear_infinite]" />
          ) : over ? (
            <FolderPlus className="size-5 text-white" strokeWidth={1.6} />
          ) : (
            <FolderOpen className="size-5 text-white/90" strokeWidth={1.6} />
          )}
        </LiquidOrb>
      </motion.div>
      <motion.span {...FADE} className="relative min-w-0 flex-1">
        <span className="block truncate text-[15px] font-semibold text-white">{busy ? t('home.opening') : over ? t('home.dropActive') : t('home.dropCompact')}</span>
        <span className="block truncate text-[12px] text-white/45 transition-colors duration-200 group-hover:text-white/65">{busy ? opening : t('home.dropCompactHint')}</span>
      </motion.span>
      <motion.span {...FADE} className="relative mr-2 hidden h-9 items-center rounded-full bg-white/[0.07] px-4 text-[12px] font-medium text-white/80 transition-colors duration-200 group-hover:bg-white/[0.12] md:inline-flex">
        {t('home.chooseFolder')}
      </motion.span>
    </motion.button>
  )
}
