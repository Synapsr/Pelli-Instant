import { Button, Progress, Spinner } from '@heroui/react'
import { CircleCheck, ExternalLink, FolderSearch, Info, TriangleAlert, UploadCloud } from 'lucide-react'
import type { ReactNode } from 'react'
import { IconBox } from '@/components/ui/IconBox'
import { api } from '@/lib/api'
import { formatBytes, formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { isMac } from '@/lib/platform'
import { useAppStore } from '@/store/app-store'
import { useExportStore } from '@/store/export-store'

/** Upload failures explained in the photographer's words (others show the engine message). */
const UPLOAD_ERRORS = ['unauthorized', 'offline', 'storage_full', 'gallery_limit_reached', 'subscription_required', 'gallery_not_found'] as const
type KnownUploadError = (typeof UPLOAD_ERRORS)[number]
const isKnownUploadError = (code: string | null): code is KnownUploadError => UPLOAD_ERRORS.includes(code as KnownUploadError)

export function ExportRunning() {
  const t = useT()
  const phase = useExportStore((s) => s.phase)
  const upload = useExportStore((s) => s.upload)
  const cancelUpload = useExportStore((s) => s.cancelUpload)
  const value = upload && upload.total ? (upload.done / upload.total) * 100 : 0
  return (
    <div className="flex flex-col items-center px-10 py-14 text-center" role="status" aria-live="polite">
      {phase === 'exporting' ? (
        <>
          <Spinner color="current" className="text-[#5840f0] dark:text-[#a599f5]" />
          <p className="mt-5 text-base font-semibold text-foreground">{t('export.running')}</p>
          <p className="mt-1 text-sm text-default-500">{t('export.runningBody')}</p>
        </>
      ) : (
        <>
          <IconBox icon={UploadCloud} size="lg" />
          <p className="mt-5 text-base font-semibold text-foreground">{t('export.uploading')}</p>
          <div className="mt-5 w-full max-w-md">
            <Progress aria-label={t('export.uploading')} value={value} size="sm" classNames={{ indicator: 'bg-[#7c6af0]', track: 'bg-default-100 dark:bg-white/[0.06]' }} />
            <p className="tabular mt-2 text-xs text-default-500">
              {upload ? t('export.uploadProgress', { done: formatNumber(upload.done), total: formatNumber(upload.total), bytes: formatBytes(upload.bytesSent) }) : t('common.loading')}
            </p>
            {upload?.current && <p className="mt-0.5 truncate font-mono text-[11px] text-default-400">{upload.current}</p>}
          </div>
          <Button size="sm" variant="light" className="mt-6" onPress={() => void cancelUpload()}>
            {t('export.cancelUpload')}
          </Button>
        </>
      )}
    </div>
  )
}

function Line({ children, tone = 'success' }: { children: ReactNode; tone?: 'success' | 'danger' }) {
  return (
    <li className="flex items-center gap-2.5 text-[13px] text-foreground">
      {tone === 'success' ? <CircleCheck className="size-4 shrink-0 text-success" aria-hidden /> : <TriangleAlert className="size-4 shrink-0 text-danger" aria-hidden />}
      {children}
    </li>
  )
}

export function ExportReportView({ folder, revealPath, xmp }: { folder: string; revealPath: string; xmp: boolean }) {
  const t = useT()
  const report = useExportStore((s) => s.report)
  const uploadReport = useExportStore((s) => s.uploadReport)
  const error = useExportStore((s) => s.error)
  const errorCode = useExportStore((s) => s.errorCode)
  const signOut = useAppStore((s) => s.signOut)
  const closeDialog = useAppStore((s) => s.closeDialog)

  return (
    <div className="px-8 py-8">
      <div className="flex flex-col items-center text-center">
        <IconBox icon={error ? TriangleAlert : CircleCheck} size="lg" tone={error ? 'warning' : 'success'} />
        <h2 className="mt-4 text-lg font-semibold text-foreground">{t('export.reportTitle')}</h2>
      </div>
      <ul className="mx-auto mt-6 max-w-md space-y-2.5 rounded-2xl border border-default-200/70 p-5 dark:border-white/[0.07]">
        {report && report.xmpWritten > 0 && <Line>{t('export.reportXmp', { count: report.xmpWritten })}</Line>}
        {report && report.copied > 0 && <Line>{t('export.reportCopied', { count: report.copied })}</Line>}
        {report && report.moved > 0 && (
          <Line>
            {t('export.reportMoved', { count: report.moved })} <span className="font-mono text-xs text-default-400">{folder}</span>
          </Line>
        )}
        {report?.fileListPath && <Line>{t('export.reportList')}</Line>}
        {uploadReport && uploadReport.uploaded > 0 && <Line>{t('export.reportUploaded', { count: uploadReport.uploaded })}</Line>}
        {uploadReport && (uploadReport.skipped ?? 0) > 0 && <Line>{t('export.reportUploadSkipped', { count: uploadReport.skipped ?? 0 })}</Line>}
        {uploadReport && uploadReport.failed > 0 && (
          <li>
            <Line tone="danger">{t('export.reportUploadFailed', { count: uploadReport.failed })}</Line>
            <p className="mt-1 pl-6 text-[11px] leading-snug text-default-500">{t('export.reportUploadRetry')}</p>
          </li>
        )}
        {report && report.errors.length > 0 && (
          <li>
            <Line tone="danger">{t('export.reportErrors', { count: report.errors.length })}</Line>
            <ul className="mt-1.5 max-h-24 space-y-0.5 overflow-y-auto pl-6 font-mono text-[11px] text-default-500">
              {report.errors.slice(0, 20).map((e) => (
                <li key={e.photoId} className="selectable truncate">
                  {e.message}
                </li>
              ))}
            </ul>
          </li>
        )}
        {error && (
          <Line tone="danger">{isKnownUploadError(errorCode) ? t(`export.uploadErrors.${errorCode}`) : t('export.uploadError', { message: error })}</Line>
        )}
      </ul>
      {xmp && report && report.xmpWritten > 0 && (
        <p className="mx-auto mt-4 flex max-w-md items-start gap-2 text-xs leading-snug text-default-500">
          <Info className="mt-0.5 size-3.5 shrink-0" aria-hidden />
          {t('export.lightroomHint')}
        </p>
      )}
      <div className="mt-6 flex justify-center gap-2">
        <Button variant="flat" startContent={<FolderSearch className="size-4" />} onPress={() => void api().revealInFolder(revealPath)}>
          {isMac() ? t('common.revealInFinder') : t('common.revealInExplorer')}
        </Button>
        {uploadReport && (
          <Button color="primary" startContent={<ExternalLink className="size-4" />} onPress={() => void api().openExternal(uploadReport.galleryUrl)}>
            {t('export.openGallery')}
          </Button>
        )}
        {errorCode === 'unauthorized' && (
          <Button
            color="primary"
            onPress={() => {
              closeDialog()
              useExportStore.getState().reset()
              void signOut()
            }}
          >
            {t('export.relogin')}
          </Button>
        )}
      </div>
    </div>
  )
}
