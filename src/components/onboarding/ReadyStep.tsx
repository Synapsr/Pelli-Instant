import { motion } from 'framer-motion'
import { Infinity as InfinityIcon } from 'lucide-react'
import { TelemetryDisclosure } from '@/components/privacy/TelemetryDetails'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
import { Toggle } from '@/components/ui/Toggle'
import { useT } from '@/lib/i18n'
import { Scene } from './Scene'

/** The orb holding the infinity sign: nothing is counted. */
function UnlimitedOrb() {
  return (
    <div className="flex flex-col items-center short:scale-[0.8]">
      <LiquidOrb size={168}>
        <motion.span
          initial={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }}
          animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
          transition={{ type: 'spring', duration: 0.5, bounce: 0, delay: 0.45 }}
        >
          <InfinityIcon className="size-14 text-white" strokeWidth={1.5} />
        </motion.span>
      </LiquidOrb>
    </div>
  )
}

/**
 * Last step of the welcome: ready to drop a first shoot, and the anonymous
 * statistics switch (pre-checked, one sentence, the details on demand).
 * Finishing the tour saves the choice.
 */
export function ReadyStep({ telemetry, onTelemetry }: { telemetry: boolean; onTelemetry: (on: boolean) => void }) {
  const t = useT()
  return (
    <Scene visualClassName="h-[210px] short:h-[160px]" visual={<UnlimitedOrb />} title={t('onboarding.ready.title')} body={t('onboarding.ready.body')}>
      <div className="glass mt-7 w-[540px] max-w-full rounded-2xl px-5 pb-2 pt-4 text-left short:mt-5">
        <div className="flex items-start gap-5">
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-medium text-white">{t('onboarding.ready.telemetry')}</p>
            <p className="mt-1 text-pretty text-[12.5px] leading-snug text-white/55">{t('onboarding.ready.telemetryBody')}</p>
          </div>
          <Toggle isSelected={telemetry} onValueChange={onTelemetry} aria-label={t('onboarding.ready.telemetry')} className="mt-0.5" />
        </div>
        <TelemetryDisclosure label={t('onboarding.ready.what')} className="mt-1" />
      </div>
    </Scene>
  )
}
