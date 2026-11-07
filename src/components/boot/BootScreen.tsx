import { Button, Spinner } from '@heroui/react'
import { TriangleAlert } from 'lucide-react'
import { EmptyState } from '@/components/ui/EmptyState'
import { InstantLogo } from '@/components/ui/Logo'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'

/** Splash while the engine starts, or the fatal error when it cannot. */
export function BootScreen() {
  const t = useT()
  const phase = useAppStore((s) => s.phase)
  const error = useAppStore((s) => s.bootError)
  return (
    <div className="flex h-full flex-col items-center justify-center gap-8 bg-background text-foreground">
      <InstantLogo size="lg" className="animate-fade-in" />
      {phase === 'error' ? (
        <EmptyState
          icon={TriangleAlert}
          tone="danger"
          title={t('errors.generic')}
          body={<span className="selectable font-mono text-xs">{error}</span>}
          action={
            <Button variant="flat" onPress={() => location.reload()}>
              {t('common.retry')}
            </Button>
          }
        />
      ) : (
        <Spinner size="sm" color="default" aria-label={t('common.loading')} />
      )}
    </div>
  )
}
