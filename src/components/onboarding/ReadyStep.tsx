import { motion } from 'framer-motion'
import { Infinity as InfinityIcon } from 'lucide-react'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
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
 * Last step of the welcome: ready to drop a first shoot. The anonymous
 * statistics are not asked here: they are explained and switched off in
 * Settings › Privacy.
 */
export function ReadyStep() {
  const t = useT()
  return <Scene visualClassName="h-[210px] short:h-[160px]" visual={<UnlimitedOrb />} title={t('onboarding.ready.title')} body={t('onboarding.ready.body')} />
}
