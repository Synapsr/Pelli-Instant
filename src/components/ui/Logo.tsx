import { useId } from 'react'
import { cn } from '@/lib/cn'

/** Pelli wordmark, in currentColor. */
export function PelliLogo({ className, title = 'Pelli' }: { className?: string; title?: string }) {
  return (
    <svg viewBox="0 0 89 31" fill="currentColor" role="img" aria-label={title} className={cn('h-6 w-auto', className)}>
      <path d="M28.171 10.2662C28.171 16.2247 22.6109 19.687 12.7264 19.7675V29.9532H16.4331V30.5974H0.370671V29.9532H3.66553V3.78442L0 1.61039H13.3853C23.517 1.61039 28.171 5.27403 28.171 10.2662ZM13.1794 2.25455H12.7264V19.1234C16.3095 19.0831 18.6983 15.7818 18.6983 10.3065C18.6983 5.03247 16.4743 2.25455 13.1794 2.25455Z" />
      <path d="M47.2319 26.9338L47.2731 26.974C45.5433 29.5507 42.7839 31 38.9536 31C33.1464 31 28.3689 27.0545 28.3689 20.8143C28.3689 14.9364 32.4051 10.0649 38.9948 10.0649C42.8663 10.0649 45.5433 12.2792 45.5433 15.2182C45.5433 18.5597 42.9074 21.4987 37.9652 23.3104C40.3127 25.5649 43.2781 27.0545 46.0376 27.0545C46.4494 27.0545 46.8613 27.0143 47.2319 26.9338ZM35.1645 11.474C34.0937 11.474 33.4759 12.3597 33.4759 13.8494C33.4759 17.1909 35.2469 20.5727 37.718 23.0688C38.8712 21.6195 39.3243 20.0091 39.3243 17.9558C39.3243 14.4532 36.9355 11.474 35.1645 11.474Z" />
      <path d="M48.4636 30.5974V29.9532H50.5229V5.9987L48.1753 4.02597L58.7601 0.402597V29.9532H60.8194V30.5974H48.4636Z" />
      <path d="M62.3397 30.5974V29.9532H64.399V5.9987L62.0514 4.02597L72.6361 0.402597V29.9532H74.6954V30.5974H62.3397Z" />
      <path d="M82.3936 8.97792C79.4694 8.97792 77.822 7.16623 77.822 4.46883C77.822 1.81169 79.4694 0 82.3936 0C85.3178 0 86.9652 1.81169 86.9652 4.46883C86.9652 7.16623 85.3178 8.97792 82.3936 8.97792ZM76.2157 30.5974V29.9532H78.275V15.2584L75.9274 13.2857L86.5121 9.66234V29.9532H88.5714V30.5974H76.2157Z" />
    </svg>
  )
}

/**
 * The Pelli Instant symbol: an aperture whose opening is a four-point
 * spark (the app icon without its tile). Ids are unique per instance.
 */
export function InstantMark({ className, size = 24 }: { className?: string; size?: number }) {
  const u = useId().replace(/:/g, '')
  return (
    <svg viewBox="212 212 600 600" width={size} height={size} aria-hidden className={cn('shrink-0', className)}>
      <defs>
          <clipPath id={`${u}-disc`}>
          <path d="M212 512 A300 300 0 1 0 812 512 A300 300 0 1 0 212 512Z M284 512 C439.04 512 512 439.04 512 284 C512 439.04 584.96 512 740 512 C584.96 512 512 584.96 512 740 C512 584.96 439.04 512 284 512Z" clipRule="evenodd" />
          </clipPath>
          <linearGradient id={`${u}-bg0`} x1="212.73" y1="532.89" x2="491.11" y2="212.73" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#5b4eaf" />
          <stop offset="0.55" stopColor="#8271f5" />
          <stop offset="1" stopColor="#a89cf8" />
          </linearGradient>
          <linearGradient id={`${u}-bg1`} x1="491.11" y1="212.73" x2="811.27" y2="491.11" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#5d51af" />
          <stop offset="0.55" stopColor="#8675f5" />
          <stop offset="1" stopColor="#aa9ef8" />
          </linearGradient>
          <linearGradient id={`${u}-bg2`} x1="811.27" y1="491.11" x2="532.89" y2="811.27" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#402fa8" />
          <stop offset="0.55" stopColor="#5942ea" />
          <stop offset="1" stopColor="#8b7bf0" />
          </linearGradient>
          <linearGradient id={`${u}-bg3`} x1="532.89" y1="811.27" x2="212.73" y2="532.89" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#3e2ea8" />
          <stop offset="0.55" stopColor="#563fe9" />
          <stop offset="1" stopColor="#8979f0" />
          </linearGradient>
          <linearGradient id={`${u}-dl`} x1="0" y1="212" x2="0" y2="812" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#fff" stopOpacity="0.18" />
          <stop offset="0.5" stopColor="#fff" stopOpacity="0" />
          <stop offset="0.5" stopColor="#120d2e" stopOpacity="0" />
          <stop offset="1" stopColor="#120d2e" stopOpacity="0.12" />
          </linearGradient>
          <linearGradient id={`${u}-dr`} x1="0" y1="212" x2="0" y2="812" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#fff" stopOpacity="0.6" />
          <stop offset="0.3" stopColor="#fff" stopOpacity="0" />
          <stop offset="0.7" stopColor="#fff" stopOpacity="0" />
          <stop offset="1" stopColor="#fff" stopOpacity="0.15" />
          </linearGradient>
      </defs>
      <g clipPath={`url(#${u}-disc)`}>
        <circle cx="512" cy="512" r="302" fill="#6a55f3" />
        <path d="M284 512 C439.04 512 512 439.04 512 284 A132 132 0 0 0 487.22 207.01 L312.23 -36.67 L204.09 -333.69 A900 900 0 0 0 -333.69 819.91 L-36.67 711.77 L207.01 536.78 A132 132 0 0 1 284 512Z" fill={`url(#${u}-bg0)`} />
        <path d="M512 284 C512 439.04 584.96 512 740 512 A132 132 0 0 0 816.99 487.22 L1060.67 312.23 L1357.69 204.09 A900 900 0 0 0 204.09 -333.69 L312.23 -36.67 L487.22 207.01 A132 132 0 0 1 512 284Z" fill={`url(#${u}-bg1)`} />
        <path d="M740 512 C584.96 512 512 584.96 512 740 A132 132 0 0 0 536.78 816.99 L711.77 1060.67 L819.91 1357.69 A900 900 0 0 0 1357.69 204.09 L1060.67 312.23 L816.99 487.22 A132 132 0 0 1 740 512Z" fill={`url(#${u}-bg2)`} />
        <path d="M512 740 C512 584.96 439.04 512 284 512 A132 132 0 0 0 207.01 536.78 L-36.67 711.77 L-333.69 819.91 A900 900 0 0 0 819.91 1357.69 L711.77 1060.67 L536.78 816.99 A132 132 0 0 1 512 740Z" fill={`url(#${u}-bg3)`} />
        <rect x="212" y="212" width="600" height="600" fill={`url(#${u}-dl)`} />
        <circle cx="512" cy="512" r="300" fill="none" stroke={`url(#${u}-dr)`} strokeWidth="6" />
        </g>
    </svg>
  )
}

/** "Pelli Instant" lock-up: the symbol, the Pelli wordmark + "Instant" in Cabinet Grotesk. */
export function InstantLogo({ className, size = 'md' }: { className?: string; size?: 'sm' | 'md' | 'lg' }) {
  const h = size === 'sm' ? 'h-[18px]' : size === 'lg' ? 'h-9' : 'h-6'
  const text = size === 'sm' ? 'text-[15px]' : size === 'lg' ? 'text-[30px]' : 'text-xl'
  const mark = size === 'sm' ? 20 : size === 'lg' ? 40 : 26
  return (
    <span className={cn('inline-flex items-end gap-1.5 text-foreground', className)} aria-label="Pelli Instant">
      {/* Optically centred on the wordmark's x-height, a little below its cap. */}
      <InstantMark size={mark} className={cn('self-center', size === 'sm' ? 'mr-1' : 'mr-1.5')} />
      <PelliLogo className={h} title="Pelli" />
      <span className={cn('font-semibold leading-none tracking-tight text-[#5840f0] dark:text-[#a599f5]', text)} style={{ marginBottom: size === 'lg' ? 1 : 0 }}>
        Instant
      </span>
    </span>
  )
}
