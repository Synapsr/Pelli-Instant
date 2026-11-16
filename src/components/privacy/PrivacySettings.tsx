import { ShieldCheck } from 'lucide-react'
import { SettingRow as Row } from '@/components/ui/SettingRow'
import { Toggle } from '@/components/ui/Toggle'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'
import { TelemetryContent, useTelemetryPreview } from './TelemetryDetails'

/** Settings → Privacy: the telemetry switch, what is never sent, what is. */
export function PrivacySettings() {
  const t = useT()
  const telemetry = useAppStore((s) => s.settings?.telemetry ?? true)
  const update = useAppStore((s) => s.updateSettings)
  const preview = useTelemetryPreview()
  return (
    <div>
      <h3 className="mb-2 text-lg font-semibold tracking-tight text-foreground">{t('settings.nav.privacy')}</h3>
      <div className="mt-3 flex items-center gap-3 rounded-2xl border border-success/20 bg-success/[0.06] px-4 py-3">
        <ShieldCheck className="size-5 shrink-0 text-success" aria-hidden />
        <p className="text-pretty text-[13px] font-medium leading-snug text-foreground">{t('settings.privacy.never')}</p>
      </div>
      <div className="mb-5">
        <Row
          label={t('settings.privacy.telemetry')}
          hint={
            <>
              {t('settings.privacy.telemetryHint')}
              <span className="mt-1 block text-[11px] text-default-400">{t('settings.privacy.offNote')}</span>
            </>
          }
        >
          <Toggle isSelected={telemetry} onValueChange={(on) => void update({ telemetry: on })} aria-label={t('settings.privacy.telemetry')} />
        </Row>
      </div>

      <h4 className="mb-2.5 text-[11px] font-semibold uppercase tracking-[0.06em] text-default-400">{t('settings.privacy.what')}</h4>
      <div className="rounded-2xl border border-white/[0.07] p-4">
        <TelemetryContent preview={preview} />
        <div className="mt-3 flex items-baseline justify-between gap-4 border-t border-white/[0.06] pt-3 text-[12px]">
          <span className="text-default-400">{t('settings.privacy.installId')}</span>
          <span className="selectable tabular truncate font-mono text-[11px] text-default-600">{telemetry && preview?.installId ? preview.installId : t('settings.privacy.noInstallId')}</span>
        </div>
      </div>
    </div>
  )
}
