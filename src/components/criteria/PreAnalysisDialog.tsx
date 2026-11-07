import { Button, Modal, ModalBody, ModalContent, ModalFooter } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { ArrowRight, ChevronDown, ScanEye } from 'lucide-react'
import { useEffect, useState } from 'react'
import { SensitivityPicker } from '@/components/analysis/SensitivityPicker'
import { Toggle } from '@/components/ui/Toggle'
import { cn } from '@/lib/cn'
import type { CullSettings, ProjectSummary } from '@/lib/contract'
import { normalizeCriteria, sameCriteria } from '@/lib/criteria'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'
import { CriteriaChips } from './CriteriaChips'
import { CriteriaList } from './CriteriaList'

const EASE = [0.22, 1, 0.36, 1] as const

/**
 * Before the first analysis of a shoot: what the AI will check, at a glance.
 * One click (or Enter) sorts with these criteria; each chip turns a check on
 * or off, and "Customise" unfolds every switch with its explanation.
 */
export function PreAnalysisDialog() {
  const project = useAppStore((s) => (s.dialog === 'preAnalysis' ? s.pendingAnalysis : null))
  const dismiss = useAppStore((s) => s.dismissAnalysis)
  // The content stays while the dialog animates out.
  const [shown, setShown] = useState(project)
  if (project && project !== shown) setShown(project)
  return (
    <Modal
      isOpen={project !== null}
      onClose={dismiss}
      size="3xl"
      radius="lg"
      backdrop="blur"
      scrollBehavior="inside"
      classNames={{ base: 'glass-strong max-h-[calc(100vh-48px)] rounded-[28px] text-foreground', backdrop: 'bg-black/50' }}
    >
      <ModalContent>{shown && <Content key={shown.id} project={shown} onClose={dismiss} />}</ModalContent>
    </Modal>
  )
}

function Content({ project, onClose }: { project: ProjectSummary; onClose: () => void }) {
  const t = useT()
  const defaults = useAppStore((s) => s.settings?.cull)
  const confirm = useAppStore((s) => s.confirmAnalysis)
  const [criteria, setCriteria] = useState(() => normalizeCriteria(project.settings))
  const [details, setDetails] = useState(false)
  const [remember, setRemember] = useState(false)
  const differs = defaults !== undefined && !sameCriteria(criteria, normalizeCriteria(defaults))
  const change = (patch: Partial<CullSettings>) => setCriteria((c) => ({ ...c, ...patch }))
  const start = () => void confirm(criteria, remember && differs)

  // Enter sorts right away, unless a control has the focus (it gets the key).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Enter' || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
      if (e.target instanceof Element && e.target.closest('button, input, [role="switch"], [role="radio"], a')) return
      e.preventDefault()
      start()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  return (
    <>
      <div className="flex items-center gap-4 px-7 pb-1 pt-7 short:pt-5">
        <span className="brand-fill flex size-12 shrink-0 items-center justify-center rounded-2xl short:size-10 short:rounded-xl">
          <ScanEye className="size-6 short:size-5" strokeWidth={1.8} />
        </span>
        <div className="min-w-0">
          <p className="text-[13px] font-medium text-default-500">{t('preAnalysis.title')}</p>
          <h2 className="truncate text-xl font-semibold tracking-tight text-foreground">{project.name}</h2>
        </div>
      </div>

      <ModalBody className="gap-0 px-7 py-4 short:py-3">
        <p className="mb-2 text-[13px] font-medium text-foreground short:mb-1.5">{t('preAnalysis.sensitivity')}</p>
        <SensitivityPicker value={criteria.sensitivity} onChange={(sensitivity) => change({ sensitivity })} />

        <div className="glass mt-3 rounded-2xl p-4 short:mt-2 short:py-3">
          <div className="flex items-center justify-between gap-4">
            <p className="text-[13px] font-medium text-foreground">{t('preAnalysis.criteria')}</p>
            <button
              type="button"
              aria-expanded={details}
              aria-controls="pre-analysis-details"
              onClick={() => setDetails((d) => !d)}
              className="focus-ring -mr-1.5 inline-flex h-7 items-center gap-1 rounded-full px-2.5 text-xs font-medium text-[#a599f5] transition-colors hover:bg-white/[0.05] hover:text-[#c3baf8]"
            >
              {t('preAnalysis.customize')}
              <ChevronDown className={cn('size-3.5 transition-transform duration-300 ease-out', details && 'rotate-180')} aria-hidden />
            </button>
          </div>
          <div id="pre-analysis-details">
            <AnimatePresence initial={false}>
              {details ? (
                <motion.div key="list" className="overflow-hidden" initial={{ height: 0, opacity: 0 }} animate={{ height: 'auto', opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={{ duration: 0.3, ease: EASE }}>
                  <div className="pt-3">
                    <CriteriaList value={criteria} onChange={change} density="compact" columns />
                  </div>
                </motion.div>
              ) : (
                <motion.div key="chips" className="overflow-hidden" initial={{ height: 0, opacity: 0 }} animate={{ height: 'auto', opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={{ duration: 0.3, ease: EASE }}>
                  <div className="pt-3">
                    <CriteriaChips value={criteria} onChange={change} />
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </ModalBody>

      <ModalFooter className="items-center justify-between gap-4 px-7 pb-6 pt-2 short:pb-5">
        {/* Kept in the layout when hidden, so the buttons never move. */}
        <div className={cn('min-w-0 transition-opacity duration-200', differs ? 'opacity-100' : 'invisible opacity-0')}>
          <Toggle isSelected={remember && differs} onValueChange={setRemember} isDisabled={!differs} classNames={{ label: 'text-[13px] text-default-600' }}>
            {t('preAnalysis.remember')}
          </Toggle>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button variant="light" radius="full" onPress={onClose}>
            {t('common.cancel')}
          </Button>
          <Button radius="full" className="btn-brand h-11 px-6 font-semibold" endContent={<ArrowRight className="size-4" />} onPress={start}>
            {t('preAnalysis.start', { count: project.photoCount })}
          </Button>
        </div>
      </ModalFooter>
    </>
  )
}
