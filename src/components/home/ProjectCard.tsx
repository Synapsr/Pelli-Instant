import { Dropdown, DropdownItem, DropdownMenu, DropdownTrigger } from '@heroui/react'
import { motion } from 'framer-motion'
import { Ellipsis, FolderSearch, ImageIcon, Trash2 } from 'lucide-react'
import { memo, useState } from 'react'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { DECISION_BG } from '@/components/ui/Decision'
import type { FinalDecision, ProjectSummary } from '@/lib/contract'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { formatNumber, formatRelative } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { isMac } from '@/lib/platform'
import { useAppStore } from '@/store/app-store'

function Count({ decision, value, label }: { decision: FinalDecision; value: number; label: string }) {
  return (
    <span className="inline-flex items-center gap-1" title={label}>
      <span className={cn('size-1.5 rounded-full', DECISION_BG[decision])} />
      <span className="tabular text-[11px] text-white/60">{formatNumber(value)}</span>
    </span>
  )
}

const LAYOUT = { layout: { type: 'spring', duration: 0.3, bounce: 0 } } as const

/**
 * A shoot on the home screen and in the full list: cover, name, sorting at a
 * glance. The same card in both (shared `layoutId`), so the recent ones glide
 * into the full list. `appear`: how it shows up when it mounts.
 */
export const ProjectCard = memo(function ProjectCard({
  project,
  index = 0,
  delay = 0,
  appear = 'rise',
  exitFade = false,
  fluid = false,
}: {
  project: ProjectSummary
  index?: number
  delay?: number
  appear?: 'rise' | 'fade' | 'none'
  /** fades out when its view leaves (a card with no twin on the other view) */
  exitFade?: boolean
  /** fills its grid cell (the full list) instead of the home's fixed width */
  fluid?: boolean
}) {
  const t = useT()
  const openProject = useAppStore((s) => s.openProject)
  const removeProject = useAppStore((s) => s.removeProject)
  const [confirm, setConfirm] = useState(false)
  const analyzed = project.analyzedAt !== null

  return (
    <motion.article
      layoutId={`project-${project.id}`}
      initial={appear === 'none' ? false : appear === 'fade' ? { opacity: 0, y: 8 } : { opacity: 0, y: 12, filter: 'blur(4px)' }}
      animate={{ opacity: 1, y: 0, filter: 'blur(0px)' }}
      exit={exitFade ? { opacity: 0, transition: { duration: 0.14 } } : undefined}
      transition={{
        ...LAYOUT,
        default: appear === 'fade' ? { duration: 0.3, ease: [0.22, 1, 0.36, 1], delay: Math.min(0.24, index * 0.025) } : { duration: 0.5, ease: [0.22, 1, 0.36, 1], delay: delay + 0.3 + index * 0.08 },
      }}
      style={{ borderRadius: 16 }}
      className={cn('glass glass-hover group relative shrink-0 overflow-hidden rounded-2xl hover:-translate-y-0.5', fluid ? 'w-full' : 'w-[208px]')}
    >
      <button type="button" onClick={() => void openProject(project)} className="focus-ring absolute inset-0 z-10 rounded-2xl" aria-label={`${t('home.open')} ${project.name}`} />
      <div className="relative m-1.5 aspect-[16/10] overflow-hidden rounded-[10px] bg-white/[0.04] outline outline-1 -outline-offset-1 outline-white/10">
        {project.coverThumbUrl ? (
          <img src={project.coverThumbUrl} alt="" draggable={false} className="h-full w-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.04]" />
        ) : (
          <div className="flex h-full items-center justify-center text-white/20">
            <ImageIcon className="size-7" strokeWidth={1.4} />
          </div>
        )}
        <div className="absolute right-1.5 top-1.5 z-20 opacity-0 transition-opacity duration-200 group-focus-within:opacity-100 group-hover:opacity-100">
          <Dropdown placement="bottom-end">
            <DropdownTrigger>
              <button type="button" aria-label={t('home.projectMenu')} className="focus-ring flex size-7 items-center justify-center rounded-lg bg-black/55 text-white backdrop-blur-md hover:bg-black/75">
                <Ellipsis className="size-4" />
              </button>
            </DropdownTrigger>
            <DropdownMenu aria-label={t('home.projectMenu')} variant="flat">
              <DropdownItem key="reveal" startContent={<FolderSearch className="size-4" />} onPress={() => void api().revealInFolder(project.rootPath)}>
                {isMac() ? t('common.revealInFinder') : t('common.revealInExplorer')}
              </DropdownItem>
              <DropdownItem key="forget" startContent={<Trash2 className="size-4" />} className="text-danger" color="danger" onPress={() => setConfirm(true)}>
                {t('home.forget')}
              </DropdownItem>
            </DropdownMenu>
          </Dropdown>
        </div>
      </div>
      <div className="px-3 pb-3 pt-1.5 text-left">
        <h3 className="truncate text-[13px] font-semibold text-white">{project.name}</h3>
        <div className="mt-1 flex items-center justify-between gap-2">
          <span className="truncate text-[11px] text-white/40">
            {t('common.photos', { count: project.photoCount })} · {formatRelative(project.lastOpenedAt)}
          </span>
          {analyzed ? (
            <span className="flex shrink-0 items-center gap-2">
              <Count decision="selected" value={project.counts.selected} label={t('home.kept', { count: project.counts.selected })} />
              <Count decision="toReview" value={project.counts.toReview} label={t('review.tabs.toReview')} />
            </span>
          ) : (
            <span className="shrink-0 text-[11px] text-white/35">{t('home.notSorted')}</span>
          )}
        </div>
      </div>
      <ConfirmDialog
        open={confirm}
        title={t('home.forgetTitle', { name: project.name })}
        body={t('home.forgetBody')}
        confirmLabel={t('common.remove')}
        danger
        onConfirm={() => void removeProject(project.id)}
        onClose={() => setConfirm(false)}
      />
    </motion.article>
  )
})
