import { Button, Tooltip } from '@heroui/react'
import { ArrowRight, Settings } from 'lucide-react'
import { AnimatePresence, LayoutGroup, motion } from 'framer-motion'
import { useEffect, useRef, useState } from 'react'
import { useFolderDrop } from '@/hooks/use-folder-drop'
import { PlusPill } from '@/components/plus/PlusPill'
import { PelliHomePromo } from '@/components/promo/PelliPromo'
import { InstantLogo } from '@/components/ui/Logo'
import { Backdrop } from '@/components/ui/Backdrop'
import { WindowBar } from '@/components/ui/WindowBar'
import { UpdateNotice } from '@/components/update/UpdateNotice'
import { cn } from '@/lib/cn'
import { useT } from '@/lib/i18n'
import { useMediaQuery } from '@/hooks/use-media-query'
import { hasMoreProjects, RECENT_COUNT, RECENT_COUNT_NARROW } from '@/lib/projects'
import { ARRIVAL_S, playSound } from '@/lib/sound'
import { useAppStore } from '@/store/app-store'
import { AllProjects } from './AllProjects'
import { DropZone } from './DropZone'
import { ProjectCard } from './ProjectCard'

type HomeView = 'home' | 'all'

/**
 * Home: one big drop target and the recent shoots underneath; "See all"
 * turns it into the full list of projects (the orb shrinks to the top, the
 * recent cards glide into the grid), and back.
 */
export function HomeScreen() {
  const t = useT()
  const openFolder = useAppStore((s) => s.openFolder)
  const openDialog = useAppStore((s) => s.openDialog)
  const dialogOpen = useAppStore((s) => s.dialog !== null)
  const { over } = useFolderDrop((path) => void openFolder(path), !dialogOpen)
  const [view, setView] = useState<HomeView>('home')
  const returning = useRef(false)

  // On launch and after the welcome tour, the home arrives: the orb comes
  // from afar with a soft sound, the rest follows once it has landed.
  const [intro] = useState(() => useAppStore.getState().homeIntro)
  const played = useRef(false)
  useEffect(() => {
    if (!intro || played.current) return
    played.current = true
    useAppStore.getState().consumeHomeIntro()
    playSound('startup')
  }, [intro])

  const show = (next: HomeView) => {
    returning.current = next === 'home'
    setView(next)
  }

  return (
    <div className="relative flex h-full flex-col overflow-hidden text-white animate-fade-in">
      <Backdrop />
      {/* Whole-window glow while a folder is dragged over. */}
      <div aria-hidden className={`pointer-events-none absolute inset-3 z-30 rounded-[28px] ring-2 ring-[#7c6af0]/60 transition-opacity duration-300 ${over ? 'opacity-100' : 'opacity-0'}`} />

      <WindowBar>
        <InstantLogo size="sm" className="text-white" />
        <div className="ml-auto flex items-center gap-1.5">
          <UpdateNotice className="mr-1" />
          <PlusPill className="mr-1" />
          <Tooltip content={t('settings.title')} delay={400} closeDelay={0}>
            <Button isIconOnly variant="light" size="sm" radius="full" aria-label={t('settings.title')} onPress={() => openDialog('settings', 'general')} className="h-10 w-10 min-w-10 text-white/60 hover:text-white">
              <Settings className="size-[18px]" strokeWidth={1.8} />
            </Button>
          </Tooltip>
        </div>
      </WindowBar>

      <div className="relative z-0 flex min-h-0 flex-1 flex-col">
        <LayoutGroup id="home">
          <AnimatePresence mode="popLayout" initial={false}>
            {view === 'home' ? (
              <HomeContent key="home" over={over} intro={intro} returning={returning.current} onSeeAll={() => show('all')} />
            ) : (
              <AllProjects key="all" over={over} onBack={() => show('home')} />
            )}
          </AnimatePresence>
        </LayoutGroup>
      </div>
    </div>
  )
}

const QUICK = { duration: 0.14, ease: 'easeIn' } as const

function HomeContent({ over, intro, returning, onSeeAll }: { over: boolean; intro: boolean; returning: boolean; onSeeAll: () => void }) {
  const t = useT()
  const projects = useAppStore((s) => s.projects)
  // Five cards need 1,152 px; a narrow window shows four, never a scrolling row.
  const shown = useMediaQuery('(min-width: 1160px)') ? RECENT_COUNT : RECENT_COUNT_NARROW
  const recent = projects.slice(0, shown)
  const more = hasMoreProjects(projects, shown)
  const after = intro ? ARRIVAL_S : 0

  return (
    // Stays fully drawn while it leaves: its parts fade on their own, the shared orb and cards glide.
    <motion.div className="absolute inset-0 flex flex-col" exit={{ opacity: 1, transition: { duration: 0.32 } }}>
      <main className="relative flex min-h-0 flex-1 flex-col items-center justify-center px-8">
        <DropZone over={over} intro={intro} returning={returning} />
      </main>

      {recent.length > 0 && (
        <section className="relative pb-6 short:pb-5" aria-labelledby="recent-title">
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0, transition: QUICK }}
            transition={{ duration: 0.4, delay: returning ? 0.1 : 0.25 + after }}
            className={more ? cn('mx-auto mb-1.5 flex w-full items-center justify-between px-8', shown === RECENT_COUNT ? 'max-w-[1152px]' : 'max-w-[932px]') : 'mb-3 text-center'}
          >
            <h2 id="recent-title" className="text-[11px] font-semibold uppercase tracking-[0.12em] text-white/35">
              {t('home.recentTitle')}
            </h2>
            {more && (
              <button
                type="button"
                onClick={onSeeAll}
                className="focus-ring group -mr-3 inline-flex h-10 items-center gap-1.5 rounded-full px-3 text-[12px] font-medium text-white/55 transition-[color,background-color,scale] duration-150 hover:bg-white/[0.05] hover:text-white active:scale-[0.96]"
              >
                <span className="tabular">{t('home.seeAll', { count: projects.length })}</span>
                <ArrowRight className="size-3.5 transition-transform duration-200 ease-out group-hover:translate-x-0.5" aria-hidden />
              </button>
            )}
          </motion.div>
          {/* No overflow clipping: the cards glide in from the full list, above this row. */}
          <div className="mx-auto flex max-w-full justify-center gap-3 px-8 pb-2 pt-1">
            {recent.map((p, i) => (
              <ProjectCard key={p.id} project={p} index={i} delay={after} appear={returning ? 'none' : 'rise'} exitFade />
            ))}
          </div>
          <motion.div
            className="mt-4 flex justify-center px-8 short:hidden"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0, transition: QUICK }}
            transition={{ duration: 0.3, delay: returning ? 0.12 : 0 }}
          >
            <PelliHomePromo delay={returning ? 0 : 0.7 + after} />
          </motion.div>
        </section>
      )}
      {recent.length === 0 && (
        <div className="flex justify-center px-8 pb-8">
          <PelliHomePromo delay={0.7 + after} />
        </div>
      )}
    </motion.div>
  )
}
