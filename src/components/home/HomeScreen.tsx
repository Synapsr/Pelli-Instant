import { Button, Tooltip } from '@heroui/react'
import { Settings } from 'lucide-react'
import { motion } from 'framer-motion'
import { useEffect, useRef, useState } from 'react'
import { useFolderDrop } from '@/hooks/use-folder-drop'
import { InstantLogo } from '@/components/ui/Logo'
import { Backdrop } from '@/components/ui/Backdrop'
import { WindowBar } from '@/components/ui/WindowBar'
import { UpdateNotice } from '@/components/update/UpdateNotice'
import { api } from '@/lib/api'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { billingUrl, photosLeft } from '@/lib/plans'
import { ARRIVAL_S, playSound } from '@/lib/sound'
import { useAppStore } from '@/store/app-store'
import { AccountMenu } from './AccountMenu'
import { DropZone } from './DropZone'
import { ProjectCard } from './ProjectCard'

/** On limited plans, what's left this month and the way to unlimited. */
function QuotaHint({ delay }: { delay: number }) {
  const t = useT()
  const auth = useAppStore((s) => s.auth)
  const offers = useAppStore((s) => s.offers)
  if (auth.status !== 'signedIn') return null
  const left = photosLeft(auth.account)
  if (left === null) return null
  return (
    <motion.p
      initial={{ opacity: 0, y: 8, filter: 'blur(4px)' }}
      animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
      transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1], delay }}
      className="mt-3 text-[13px] text-white/45"
    >
      <span className="tabular">{t('home.photosLeft', { count: left, n: formatNumber(left) })}</span>
      <span aria-hidden> · </span>
      <button type="button" className="focus-ring rounded font-medium text-[#a599f5] transition-colors hover:text-[#c9c1fb]" onClick={() => void api().openExternal(billingUrl(offers))}>
        {t('home.goUnlimited')}
      </button>
    </motion.p>
  )
}

/** Home: one big drop target, and the recent shoots underneath. */
export function HomeScreen() {
  const t = useT()
  const projects = useAppStore((s) => s.projects)
  const openFolder = useAppStore((s) => s.openFolder)
  const openDialog = useAppStore((s) => s.openDialog)
  const dialogOpen = useAppStore((s) => s.dialog !== null)
  const { over } = useFolderDrop((path) => void openFolder(path), !dialogOpen)
  const recent = projects.slice(0, 5)

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
  const after = intro ? ARRIVAL_S : 0

  return (
    <div className="relative flex h-full flex-col overflow-hidden text-white animate-fade-in">
      <Backdrop />
      {/* Whole-window glow while a folder is dragged over. */}
      <div aria-hidden className={`pointer-events-none absolute inset-3 z-10 rounded-[28px] ring-2 ring-[#7c6af0]/60 transition-opacity duration-300 ${over ? 'opacity-100' : 'opacity-0'}`} />

      <WindowBar>
        <InstantLogo size="sm" className="text-white" />
        <div className="ml-auto flex items-center gap-1">
          <UpdateNotice className="mr-1.5" />
          <Tooltip content={t('settings.title')} delay={400} closeDelay={0}>
            <Button isIconOnly variant="light" size="sm" radius="full" aria-label={t('settings.title')} onPress={() => openDialog('settings', 'general')} className="text-white/60 hover:text-white">
              <Settings className="size-[18px]" strokeWidth={1.8} />
            </Button>
          </Tooltip>
          <AccountMenu />
        </div>
      </WindowBar>

      <main className="relative z-0 flex flex-1 flex-col items-center justify-center px-8">
        <DropZone over={over} intro={intro} />
        <QuotaHint delay={0.45 + after} />
      </main>

      {recent.length > 0 && (
        <section className="relative z-0 pb-8" aria-labelledby="recent-title">
          <motion.h2
            id="recent-title"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.5, delay: 0.25 + after }}
            className="mb-3 text-center text-[11px] font-semibold uppercase tracking-[0.12em] text-white/35"
          >
            {t('home.recentTitle')}
          </motion.h2>
          <div className="mx-auto flex max-w-full justify-center gap-3 overflow-x-auto px-8 pb-2 pt-1">
            {recent.map((p, i) => (
              <ProjectCard key={p.id} project={p} index={i} delay={after} />
            ))}
          </div>
        </section>
      )}
    </div>
  )
}
