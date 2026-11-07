/**
 * Contract between the Pelli Instant interface and its engine.
 *
 * The engine (Rust: `crates/cull-engine`, exposed by `src-tauri` as Tauri
 * commands, and by `pelli-cull serve` over HTTP for development) implements
 * `CullApi`. JSON field names are camelCase, mirroring
 * `crates/cull-types/src/lib.rs`. Change both sides together.
 *
 * Transports (see `src/lib/api/`):
 * - Tauri:  `invoke(command, args)` + `listen(event)`;
 * - HTTP dev bridge: `POST {base}/invoke/{command}` with JSON body `args`,
 *   response JSON (`{ "error": { "code", "message" } }` with status >= 400 on
 *   failure); events as Server-Sent Events on `GET {base}/events`
 *   (`event:` = event name, `data:` = JSON payload);
 * - mock: in-memory fake engine for UI development and screenshots.
 *
 * Command names are snake_case (Rust side) — listed in `COMMANDS`.
 */

// ---------------------------------------------------------------------------
// Shared engine types (mirror cull-types)
// ---------------------------------------------------------------------------

export type FileKind = 'jpeg' | 'png' | 'tiff' | 'heif' | 'raw'
export type Verdict = 'keep' | 'maybe' | 'reject'
export type Reason =
  | 'blurry'
  | 'motionBlur'
  | 'subjectOutOfFocus'
  | 'eyesClosed'
  | 'underexposed'
  | 'overexposed'
  | 'duplicate'
  | 'exactDuplicate'
  | 'lowAesthetic'
export type GroupKind = 'burst' | 'similar' | 'exact'
export type Sensitivity = 'strict' | 'balanced' | 'lenient'
export type Flag = 'none' | 'pick' | 'reject'
export type ColorLabel = 'none' | 'red' | 'yellow' | 'green' | 'blue' | 'purple'
export type FinalDecision = 'selected' | 'toReview' | 'rejected'
/** `loading`: the Maximum pack's models load, before the first photo. */
export type Stage = 'scanning' | 'loading' | 'analyzing' | 'grouping' | 'done'
export interface NormRect {
  x: number
  y: number
  w: number
  h: number
}

export interface ScoreBreakdown {
  focus: number
  exposure: number
  eyes: number | null
  aesthetic: number | null
  uniqueness: number
}

export interface Assessment {
  /** 0..100 */
  score: number
  verdict: Verdict
  reasons: Reason[]
  groupId: number | null
  /** 0 = best of its group */
  groupRank: number
  breakdown: ScoreBreakdown
}

/**
 * What the culling looks for: one switch per reason the engine gives (a
 * detection turned off never produces its reason). Per project, and the
 * app's defaults for new projects (`AppSettings.cull`). Older engines stored
 * `detectBlur` for the three kinds of blur and `detectExposure` for both
 * directions: the engine reads them into the split switches.
 */
export interface CullSettings {
  sensitivity: Sensitivity
  /** whole frame out of focus (`blurry`) */
  detectBlur: boolean
  /** camera shake or subject motion (`motionBlur`) */
  detectMotionBlur: boolean
  /** main face soft while something else is sharp (`subjectOutOfFocus`) */
  detectSubjectFocus: boolean
  /**
   * respect artistic blur (bokeh, shallow depth of field, blur on purpose):
   * blur only rejects a photo when nothing in it is sharp, clearly soft
   * photos go to review, and `subjectOutOfFocus` is not raised
   */
  artisticBlur: boolean
  detectEyesClosed: boolean
  detectUnderexposed: boolean
  detectOverexposed: boolean
  /**
   * keep only the best `keepPerGroup` frames of each burst or group of
   * similar shots, the others get `duplicate`; groups exist (and are stacked
   * in the review) either way
   */
  groupSimilar: boolean
  /** 1..5 */
  keepPerGroup: number
  /** reject copies of the same image (`exactDuplicate`) */
  detectExactDuplicates: boolean
  groupWindowS: number
  similarityThreshold: number
  sceneGapS: number
}

export const DEFAULT_SETTINGS: CullSettings = {
  sensitivity: 'balanced',
  detectBlur: true,
  detectMotionBlur: true,
  detectSubjectFocus: true,
  artisticBlur: false,
  detectEyesClosed: true,
  detectUnderexposed: true,
  detectOverexposed: true,
  groupSimilar: true,
  keepPerGroup: 1,
  detectExactDuplicates: true,
  groupWindowS: 45,
  similarityThreshold: 0.9,
  sceneGapS: 900,
}

export interface UserMark {
  flag: Flag
  /** 0..5 */
  rating: number
  label: ColorLabel
}

export interface SimilarGroup {
  id: number
  kind: GroupKind
  /** best first */
  photoIds: string[]
}

export interface Scene {
  id: number
  startMs: number | null
  endMs: number | null
  photoIds: string[]
  /**
   * Optional name of the moment ("Cérémonie", "Cocktail"…), localised in the
   * app language. UI addition: the UI falls back to "Moment {n}" when absent.
   */
  label?: string | null
}

export interface Progress {
  stage: Stage
  done: number
  total: number
  current: string | null
  photosPerSecond: number
  etaMs: number | null
}

// ---------------------------------------------------------------------------
// UI-facing views (built by cull-engine)
// ---------------------------------------------------------------------------

export interface FaceView {
  bbox: NormRect
  confidence: number
  /** null when unknown */
  rightEyeOpen: number | null
  leftEyeOpen: number | null
  /** true = both eyes closed, null = unknown */
  eyesClosed: boolean | null
  /** 0..1 */
  sharpness: number | null
}

export interface ExifSummary {
  camera: string | null
  lens: string | null
  iso: number | null
  /** formatted: "1/250", "2\"" */
  shutter: string | null
  aperture: number | null
  focalLength: number | null
}

export interface PhotoView {
  id: string
  relPath: string
  fileName: string
  kind: FileKind
  hasRaw: boolean
  hasJpeg: boolean
  sizeBytes: number
  capturedAt: number | null
  /** upright source size */
  width: number | null
  height: number | null
  exif: ExifSummary
  /** URL usable in <img src> (asset protocol / http bridge), null until generated */
  thumbUrl: string | null
  status: 'pending' | 'analyzed' | 'error'
  error: string | null
  assessment: Assessment | null
  faces: FaceView[]
  mark: UserMark
  decision: FinalDecision
  sceneId: number | null
}

export interface DecisionCounts {
  selected: number
  toReview: number
  rejected: number
}

export interface ProjectSummary {
  id: string
  name: string
  rootPath: string
  photoCount: number
  rawCount: number
  createdAt: number
  lastOpenedAt: number
  analyzedAt: number | null
  coverThumbUrl: string | null
  counts: DecisionCounts
  settings: CullSettings
  /**
   * Precision of the last analysis, null before the first one. A project
   * analysed in 'standard' can be re-analysed in 'maximum' once the pack is
   * ready: `startAnalysis` re-measures its photos (not counted on the quota,
   * like any re-analysis).
   */
  precision: Precision | null
}

export interface ProjectData {
  project: ProjectSummary
  photos: PhotoView[]
  groups: SimilarGroup[]
  scenes: Scene[]
}

export interface AnalysisSummary {
  projectId: string
  analyzed: number
  failed: number
  durationMs: number
  counts: DecisionCounts
  reasons: Partial<Record<Reason, number>>
  groups: number
  /** precision actually used (falls back to 'standard' when the pack is not ready) */
  precision: Precision
}

export interface SearchHit {
  photoId: string
  /** cosine similarity, higher = better */
  score: number
}

// ---------------------------------------------------------------------------
// Account (Pelli)
// ---------------------------------------------------------------------------

export type Plan = 'free' | 'lite' | 'pro' | 'ultra'

/**
 * What the account may do. Pelli Instant is free with a Pelli account; the
 * number of photos sorted per calendar month depends on the Pelli plan
 * (free 200, Lite 1,000, Pro and Ultra unlimited, unlimited during a trial).
 */
export interface Entitlements {
  /** the account may sort (not suspended) */
  analyze: boolean
  /** photos per calendar month (UTC); null = unlimited */
  monthlyQuota: number | null
  usedThisMonth: number
  /** epoch ms: first instant of next month (UTC), when the count resets */
  resetsAt: number
  sendToGallery: boolean
}

export interface Account {
  name: string
  email: string
  slug: string
  avatarUrl: string | null
  plan: Plan
  /** "Pro", "Ultra", "Ultra à vie", "Essai Pro"… localised by the server */
  planLabel: string
  /** epoch ms when the running Pelli trial ends, null without trial */
  trialEndsAt: number | null
  /** the account can start a Pelli trial now (absent from older Pelli versions) */
  trialAvailable?: boolean
  lifetime: boolean
  entitlements: Entitlements
}

/** A Pelli plan as offered in the onboarding and when the quota runs out. */
export interface PlanOffer {
  slug: Plan
  name: string
  /** price per month, billed monthly */
  monthlyPrice: number
  /** price per year, billed yearly */
  yearlyPrice: number
  /** price per month, billed yearly */
  yearlyPricePerMonth: number
  /** photos sorted per month; null = unlimited */
  photosPerMonth: number | null
  /** what the plan brings in Pelli itself (galleries, storage…), localised */
  highlights: string[]
}

/** Pelli's current offers (prices and links stay right when Pelli changes them). */
export interface Offers {
  currency: string
  trialDays: number
  plans: PlanOffer[]
  links: { register: string; pricing: string; trial: string; billing: string }
}

/** Details of the `quotaExceeded` / `quotaOffline` analysis errors. */
export interface QuotaDetails {
  monthlyQuota: number | null
  usedThisMonth?: number
  remaining?: number
  resetsAt?: number
  /** photos this analysis needed */
  needed: number
}

export type AuthState =
  | { status: 'signedOut' }
  | { status: 'pending'; authUrl: string; expiresAt: number }
  | { status: 'signedIn'; account: Account }

export interface GallerySummary {
  id: string
  title: string
  slug: string
  status: string
  imageCount: number
  coverUrl: string | null
  createdAt: number
}

// ---------------------------------------------------------------------------
// Export & upload
// ---------------------------------------------------------------------------

export interface XmpExportOptions {
  enabled: boolean
  /** stars written per final decision (0..5, -1 = rejected) */
  rating: Record<FinalDecision, number>
  /** colour label written per final decision */
  label: Record<FinalDecision, ColorLabel>
  /** user stars/labels override the mapping when set */
  keepUserMarks: boolean
}

export interface ExportOptions {
  xmp: XmpExportOptions
  copySelected: {
    enabled: boolean
    destination: string | null
    /** include RAW and/or JPEG twin */
    includeRaw: boolean
    includeJpeg: boolean
    /** keep sub-folder structure */
    keepStructure: boolean
  }
  moveRejected: {
    enabled: boolean
    /** sub-folder created next to each rejected photo */
    folderName: string
  }
  /** write selected.txt (file names, comma separated, for Lightroom filter) */
  fileList: boolean
}

export interface ExportReport {
  xmpWritten: number
  copied: number
  moved: number
  fileListPath: string | null
  errors: { photoId: string; message: string }[]
}

export interface UploadRequest {
  /** existing gallery, or create one */
  galleryId: string | null
  newGallery: { title: string; type: 'selection' | 'delivery' } | null
  which: 'selected' | 'selectedAndToReview'
  /** one Pelli folder per scene */
  folderPerScene: boolean
}

export interface UploadProgress {
  projectId: string
  galleryId: string
  done: number
  total: number
  bytesSent: number
  current: string | null
}

export interface UploadReport {
  galleryId: string
  galleryUrl: string
  uploaded: number
  failed: number
  /**
   * Photos the gallery already had from an earlier send (same file): sending
   * again only uploads what is new or changed. Absent from older engines.
   */
  skipped?: number
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

export type Language = 'en' | 'fr' | 'es' | 'de' | 'it' | 'pt' | 'nl' | 'pl' | 'sv'
export type Theme = 'system' | 'dark' | 'light'

export interface AppInfo {
  version: string
  platform: 'macos' | 'windows' | 'linux' | 'web'
  arch: string
  /**
   * "CoreML", "DirectML", "CUDA", "CPU": where the heavy model runs. "CPU" on
   * Apple Silicon (measured faster than CoreML there).
   */
  accelerator: string
  modelsReady: boolean
  capabilities: {
    faces: boolean
    eyes: boolean
    embeddings: boolean
    aesthetic: boolean
    textSearch: boolean
    /** the Maximum pack is installed and its models load */
    maximum: boolean
  }
  cpuThreads: number
  /** total memory of the computer in bytes, null when unknown */
  memoryBytes: number | null
  cpuCores: number
  /**
   * Built with PELLI_INSTANT_NO_ACCOUNT=1: no Pelli account, no quota (for
   * developers who build the app themselves).
   */
  noAccount: boolean
}

/**
 * 'standard': the models shipped with the app. 'maximum': the optional
 * Maximum pack (bigger models, slower, finer results); only used when
 * `ModelPack.state.status === 'ready'`, otherwise analyses run in 'standard'.
 */
export type Precision = 'standard' | 'maximum'

/** Download and installation of the optional Maximum pack. */
export type ModelPackState =
  | { status: 'absent' }
  /** resumable: a cancelled or interrupted download restarts where it stopped */
  | { status: 'downloading'; receivedBytes: number; totalBytes: number; bytesPerSecond: number | null }
  /** checking the files and loading the models once (CoreML compile cache…) */
  | { status: 'preparing'; progress: number }
  | { status: 'ready' }
  /** keeps the downloaded part; `downloadModelPack` resumes it */
  | { status: 'failed'; code: ErrorCode | string; message: string; receivedBytes: number; totalBytes: number }

export interface ModelPack {
  id: 'maximum'
  /** total download size in bytes */
  sizeBytes: number
  /** space used on disk once installed (models + compiled cache), bytes */
  diskBytes: number
  /** free space on the disk that receives the pack, null when unknown */
  freeBytes: number | null
  state: ModelPackState
}

/**
 * Where the app's own updates stand. Answered by the desktop shell
 * (`src-tauri/src/updates.rs`), not the engine: release builds check a few
 * seconds after launch and every six hours. A background check only reports
 * news (a failure stays silent, offline included); `checkForUpdate` reports
 * its outcome.
 */
export type UpdateStatus =
  /** not checked yet */
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'upToDate' }
  | { status: 'available'; version: string }
  /** `progress` from 0 to 1, null while the size is unknown */
  | { status: 'downloading'; version: string; progress: number | null }
  /**
   * downloaded and verified: installs and restarts the app as soon as no job
   * is under way (analysis, export, upload, Maximum pack), `waiting` while one is
   */
  | { status: 'restarting'; version: string; waiting: boolean }
  /** `version`: the update that failed to install (it can be tried again); null when checking failed */
  | { status: 'failed'; code: UpdateErrorCode; message: string; version: string | null }
  /** a development build, or `PELLI_INSTANT_DISABLE_UPDATER=1` (packagers who update the app themselves) */
  | { status: 'disabled' }

/** `unsupported`: no update published for this kind of install; `signature`: not signed by Pelli, not installed. */
export type UpdateErrorCode = 'offline' | 'unsupported' | 'signature' | 'internal'

export interface AppSettings {
  theme: Theme
  language: Language
  acceleration: 'auto' | 'cpu'
  /**
   * Analyse in the background: a few threads at a low priority (macOS
   * efficiency cores, below-normal priority on Windows) so the computer stays
   * fluid; false = every core at full priority (fastest).
   */
  background: boolean
  /** 'maximum' is kept even while the pack is missing (analyses then run in 'standard') */
  precision: Precision
  /** default criteria: every project opened for the first time starts from them */
  cull: CullSettings
  cacheDir: string
  cacheSizeBytes: number
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

export interface CullApi {
  appInfo(): Promise<AppInfo>
  getSettings(): Promise<AppSettings>
  setSettings(patch: Partial<Omit<AppSettings, 'cacheDir' | 'cacheSizeBytes'>>): Promise<AppSettings>
  clearCache(): Promise<AppSettings>

  authStatus(): Promise<AuthState>
  /** opens the browser on the Pelli consent page and returns the pending state */
  authStart(): Promise<AuthState>
  /** call every 2-5 s while pending */
  authPoll(): Promise<AuthState>
  authLogout(): Promise<AuthState>
  /** refresh plan/quota */
  refreshAccount(): Promise<AuthState>
  /** Pelli's offers in this language (cached for offline use) */
  getOffers(lang: Language): Promise<Offers>

  /** state of the optional Maximum pack */
  modelPack(): Promise<ModelPack>
  /**
   * Starts or resumes the download, returns immediately; progress via
   * `model-pack` events. Error `diskFull` when `freeBytes` is too small,
   * `offline` without network.
   */
  downloadModelPack(): Promise<void>
  /** stops the download and keeps the part already downloaded */
  cancelModelPack(): Promise<ModelPack>
  /** deletes the pack (and its compiled cache); `precision` stays as set */
  removeModelPack(): Promise<ModelPack>

  /** native folder picker; null when cancelled */
  pickFolder(): Promise<string | null>
  listProjects(): Promise<ProjectSummary[]>
  /** scan the folder (creating the project or reusing the existing one) */
  openFolder(path: string, recursive: boolean): Promise<ProjectSummary>
  /** forget a project (deletes cached previews, never the photos) */
  removeProject(projectId: string): Promise<void>
  getProject(projectId: string): Promise<ProjectData>

  /**
   * Returns immediately; progress via events. Photos never analysed before
   * are counted on the monthly quota (error `quotaExceeded` / `quotaOffline`).
   */
  startAnalysis(projectId: string, settings: CullSettings): Promise<void>
  cancelAnalysis(projectId: string): Promise<void>
  /** re-run decisions on cached measures (instant, no re-analysis) */
  recompute(projectId: string, settings: CullSettings): Promise<ProjectData>

  setMarks(projectId: string, photoIds: string[], patch: Partial<UserMark>): Promise<PhotoView[]>
  /** large (2560 px) preview URL, generated on demand */
  largePreview(projectId: string, photoId: string): Promise<string>
  search(projectId: string, query: string): Promise<SearchHit[]>

  exportProject(projectId: string, options: ExportOptions): Promise<ExportReport>
  listGalleries(): Promise<GallerySummary[]>
  /** returns immediately; progress via events */
  uploadToPelli(projectId: string, request: UploadRequest): Promise<void>
  cancelUpload(projectId: string): Promise<void>

  revealInFolder(path: string): Promise<void>
  openExternal(url: string): Promise<void>

  updateStatus(): Promise<UpdateStatus>
  /** checks now and resolves with the outcome, failures included */
  checkForUpdate(): Promise<UpdateStatus>
  /**
   * Installs the update found by the last check: returns immediately,
   * progress via `update-status` events (download, wait for the jobs under
   * way, restart). Does nothing without one.
   */
  installUpdate(): Promise<void>

  on<E extends keyof CullEvents>(event: E, handler: (payload: CullEvents[E]) => void): () => void
}

export interface CullEvents {
  'analysis-progress': Progress & { projectId: string }
  /** a photo finished (streamed so the grid fills in live) */
  'analysis-photo': { projectId: string; photo: PhotoView }
  'analysis-done': AnalysisSummary
  'analysis-error': { projectId: string; code: string; message: string; details?: QuotaDetails }
  'upload-progress': UploadProgress
  'upload-done': UploadReport & { projectId: string }
  'upload-error': { projectId: string; code: string; message: string }
  'auth-changed': AuthState
  /** every state change of the Maximum pack, at most ~4 per second while downloading */
  'model-pack': ModelPack
  /**
   * UI addition: photos changed outside an analysis (preview generated after
   * a scan, marks changed elsewhere…). The UI upserts them by id.
   */
  'photos-updated': { projectId: string; photos: PhotoView[] }
  /** every change of the app's updates, at most ~8 per second while downloading */
  'update-status': UpdateStatus
}

/**
 * UI addition: error codes the interface handles specifically (the `code` of
 * HTTP bridge errors, of Tauri command errors `{ code, message }` and of
 * `analysis-error` / `upload-error` events). Any other code is shown with its
 * message.
 */
export type ErrorCode =
  /** the OS refused access to the folder (macOS privacy, Windows ACL) */
  | 'permissionDenied'
  /** folder or file does not exist anymore */
  | 'notFound'
  /** the folder holds no supported photo */
  | 'noPhotos'
  /** AI models missing or corrupted (local analysis degraded) */
  | 'modelsMissing'
  /** network unreachable (Pelli account, offers, upload, Maximum pack) */
  | 'offline'
  /** not enough free disk space (Maximum pack download) */
  | 'diskFull'
  /** a downloaded model does not match its checksum (Maximum pack) */
  | 'checksum'
  /** not enough photos left this month for this analysis (details: QuotaDetails) */
  | 'quotaExceeded'
  /** limited plan and no network to count the photos (details: QuotaDetails) */
  | 'quotaOffline'
  /** Pelli session expired or revoked */
  | 'unauthorized'
  /** operation cancelled by the user */
  | 'cancelled'
  | 'internal'

/** CullApi method → engine command name */
export const COMMANDS = {
  appInfo: 'app_info',
  getSettings: 'get_settings',
  setSettings: 'set_settings',
  clearCache: 'clear_cache',
  authStatus: 'auth_status',
  authStart: 'auth_start',
  authPoll: 'auth_poll',
  authLogout: 'auth_logout',
  refreshAccount: 'refresh_account',
  getOffers: 'get_offers',
  modelPack: 'model_pack',
  downloadModelPack: 'download_model_pack',
  cancelModelPack: 'cancel_model_pack',
  removeModelPack: 'remove_model_pack',
  pickFolder: 'pick_folder',
  listProjects: 'list_projects',
  openFolder: 'open_folder',
  removeProject: 'remove_project',
  getProject: 'get_project',
  startAnalysis: 'start_analysis',
  cancelAnalysis: 'cancel_analysis',
  recompute: 'recompute',
  setMarks: 'set_marks',
  largePreview: 'large_preview',
  search: 'search',
  exportProject: 'export_project',
  listGalleries: 'list_galleries',
  uploadToPelli: 'upload_to_pelli',
  cancelUpload: 'cancel_upload',
  revealInFolder: 'reveal_in_folder',
  openExternal: 'open_external',
  updateStatus: 'update_status',
  checkForUpdate: 'check_for_update',
  installUpdate: 'install_update',
} as const satisfies Record<Exclude<keyof CullApi, 'on'>, string>

/**
 * Command arguments are sent as an object whose keys are the camelCase
 * parameter names above, e.g. `set_marks` ← `{ projectId, photoIds, patch }`,
 * `open_folder` ← `{ path, recursive }`, `start_analysis` ←
 * `{ projectId, settings }`, `get_offers` ← `{ lang }`. (Tauri converts camelCase argument keys
 * to the Rust snake_case parameters.)
 */
export type CommandName = (typeof COMMANDS)[keyof typeof COMMANDS]
