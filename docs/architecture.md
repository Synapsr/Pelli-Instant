# Architecture

Pelli Instant is a [Tauri 2](https://tauri.app) app: a Rust engine split into
small crates, a thin desktop shell, and a React interface that talks to the
engine through one contract.

```
 React interface (src/)
   │  CullApi: commands + events (src/lib/contract.ts)
   ├─ tauri  invoke + Tauri events ─────────────► src-tauri (desktop shell, updater) ─┐
   ├─ http   POST /invoke/<command>, SSE ───────► cull-cli serve (dev) ───────────────┤
   └─ mock   simulated engine, in the browser                                         │
                                                                                      ▼
                                       cull-engine: library, pipeline, decisions, search, export,
                                                    Pelli account and quota, Maximum pack
                                         ├─ cull-decode    files, EXIF, RAW previews
                                         ├─ cull-ml        ONNX models (faces, eyes, expression, CLIP;
                                         │                 the Maximum pack's models)
                                         ├─ cull-analysis  focus, exposure, groups, verdicts, XMP
                                         └─ cull-types     shared types
```

## Crates

| Crate | Role |
| --- | --- |
| `cull-types` | The types every crate and the interface speak: photos, measures, faces, verdicts, settings, precision. Serialised in camelCase so the interface reads them as they are. Serde only, no heavy dependency. |
| `cull-decode` | Scans a folder (RAW + JPEG pairs, sidecars; hidden and system folders skipped, and the folders an export filled, which hold the `.pelli-instant-export` marker; symlinks kept inside the folder), reads EXIF, and finds the JPEG preview the camera embedded in each RAW, which it reads with one ranged read and decodes at 1/2, 1/4 or 1/8 size. A camera JPEG's own large preview (MPF) is decoded instead of the full image when it is big enough. RAWs are never demosaiced. See its [README](../crates/cull-decode/README.md) for the formats. |
| `cull-ml` | The neural networks, run by ONNX Runtime through [`ort`](https://ort.pyke.io): YuNet finds faces, MediaPipe Face Mesh measures the eyes and the head pose, the Face Blendshapes read the expression (`expression.rs`: a blink, a squint, a smile) and refine the eye state, CLIP gives each photo an embedding (similar shots, search), and a LAION linear head turns it into an aesthetic score. `maximum.rs` holds the Maximum pack's models (`MaximumModels`, `MaximumText`) and their check against reference outputs. Verifies every model's SHA-256 at load; DirectML when compiled in and working, the CPU otherwise. See [docs/models.md](models.md). |
| `cull-analysis` | Classic image analysis, no neural network: focus (edge width), motion blur, exposure in stops, perceptual hashes, groups of similar shots, scores and verdicts, XMP sidecars. Cheap, deterministic and explainable. Its [README](../crates/cull-analysis/README.md) explains every rule, for photographers and for developers. |
| `cull-engine` | The engine the shell and the CLI drive, and `commands::dispatch`, which maps every command of the contract to a method. Its modules are listed below. |
| `cull-cli` | The `pelli-cull` binary: `analyze <folder>` sorts a folder in the terminal (`--precision maximum` with a prepared pack); `serve` exposes the engine over HTTP for the interface in a browser. |
| `src-tauri` | The desktop shell. One invoke handler routes every command to `dispatch` (except the three update commands), engine events become Tauri events, previews are served by the asset protocol (scope: the cache and the folders you opened), the Pelli token goes to the system keychain in release builds, native dialogs pick folders, and `updates.rs` runs the in-app updater. |

### Inside `cull-engine`

| Module | Role |
| --- | --- |
| `lib.rs` | `Engine`: projects, their in-memory state, analyses started in the background, and `Engine::busy` (an analysis, an export, an upload or the Maximum pack under way). |
| `store.rs` | The library: one SQLite database for every project, plus a key/value table (settings, cached account, quota reservations, the prepared pack). |
| `pipeline.rs` | The analysis: decode, measure, store, decide, with the progress events. |
| `export.rs` | XMP sidecars, copy of the selection, move of the rejects. |
| `query.rs` | Search queries into English CLIP prompts (glossary). |
| `settings.rs` | App settings: language, theme, `background`, `precision`, default criteria. |
| `system.rs` | Cores, efficiency cores, memory; the background mode's thread count and priority; `KeepAwake`, which keeps a long job at full speed and the computer out of idle sleep. |
| `modelpack.rs` | The Maximum pack: download with resume, SHA-256, preparation, loading for an analysis. |
| `account.rs`, `pelli.rs` | The Pelli account (browser sign-in, cached account, offers) and the client of the Pelli API. |
| `usage.rs` | The monthly quota: reserve, give back, remember a reservation across a quit. |
| `upload.rs`, `uploader.rs` | Sending a selection to a Pelli gallery (switched off in the interface for now). |
| `view.rs`, `commands.rs` | The JSON views of the contract and the command dispatcher. |

## How a shoot flows

1. **Open.** `open_folder` scans the folder (`cull-decode::scan_folder`) and
   creates or reuses a project in the library. A RAW and its JPEG twin become
   one photo, with the RAW as the primary file. Sub-folders that hold the
   export marker (a rejects folder, a copy of the selection) are skipped, so
   a second pass doesn't see what an export put there. A new project starts
   from the default criteria in the settings; the interface shows them
   before its first analysis (`src/components/criteria`).
2. **Decode.** `start_analysis` runs the pipeline (`cull-engine/src/pipeline.rs`)
   in a job that keeps the computer awake (`system::KeepAwake`). Workers
   (rayon) use every core, or, with the `background` setting, fewer threads
   at a low priority: one per efficiency core at background QoS on Apple
   Silicon, half the cores at below-normal priority on Windows. They take
   each photo's embedded preview, scaled down while it is decoded to at
   least 1440 px on the long side when the preview allows, and write a
   480 px thumbnail to the cache.
3. **Measure.** On that preview, `cull-analysis` measures focus, motion blur,
   exposure and hashes on a 1024 px luma copy, and `cull-ml` finds faces,
   their eyes and expression, then the CLIP embedding and the aesthetic
   score, on a working copy of 1280 px at most. Small faces are read on a
   more detailed decode of the same photo, made once when needed. In
   Maximum precision (the setting is Maximum and the pack is ready),
   `analyze_maximum` adds RetinaFace on group photos, DINOv2 for grouping,
   CLIP ViT-L/14 for search and the LAION V2 score, and RF-DETR's subject
   region on photos without a usable face; the pack's models are loaded for
   the analysis and released after it, with at most 8 workers (4 on smaller
   computers).
4. **Store.** A single job thread saves results to SQLite in batches,
   recomputes provisional decisions every second, and emits
   `analysis-photo` and `analysis-progress` so the grid fills in live. A
   photo whose file and models haven't changed is never analysed again; the
   models used (`models_id`) include the precision, so switching it analyses
   the photos again.
5. **Decide.** `cull-analysis` groups similar shots (same camera, close in
   time, close embeddings and framing; exact copies whatever their date),
   splits the shoot into moments, scores each photo from 0 to 100 and gives
   it a verdict, Keep, Maybe or Reject, with its reasons, for the project's
   criteria and sensitivity. Groups are always built, so bursts stay
   stacked; the "keep the best of each burst" criterion only decides
   whether the others are marked as duplicates. Changing them (`recompute`) reruns only this
   step, on the stored measures: it is instant.
6. **Review.** What a photo ends up as is `final_decision` in `cull-types`:
   the photographer's flag if there is one (keep or reject), otherwise the
   AI's verdict (Keep → Selected, Maybe → To review, Reject → Rejected).
   Marks (flag, stars, colour label) are saved at once with `set_marks`, and
   never change when the AI's verdicts do. A photo starts from the rating
   its camera wrote in the file (stars, or a reject), unless it is already
   marked or its RAW has an `.xmp` sidecar.
7. **Search.** `search` turns the query into an English CLIP prompt (a
   built-in glossary translates common French, Spanish, German and Italian
   words, colours, clothing and places, `query.rs`), embeds it with the CLIP text model of the project's
   precision (ViT-B/32, or ViT-L/14 from the pack), and ranks the photos by
   cosine similarity with their stored embeddings.
8. **Export.** `export_project` (`export.rs`) writes XMP sidecars next to the
   RAW files only (stars, colour label, and Pelli's flag, score, verdict and
   reasons in its own namespace; everything else in an existing sidecar is
   kept byte for byte, ratings set in another tool are kept, and the file is
   replaced atomically; the stars and colour of each decision come from the
   export options), copies the selection (each file through a hidden
   temporary file, one name suffix per photo when a name is taken, earlier
   copies left alone), moves the rejects to `_Rejets Pelli` (a photo's
   files all together or not at all), and leaves the export marker in the
   folders it filled. Originals are never modified or deleted.

## The interface

Built with React 19, [HeroUI](https://heroui.com), Tailwind CSS 4, Zustand,
TanStack Virtual and framer-motion, bundled by Vite.

| Path | Role |
| --- | --- |
| `src/lib/contract.ts` | The contract with the engine: every command (`CullApi`, `COMMANDS`), every event (`CullEvents`, among them `model-pack` and `update-status`), every type, every error code. |
| `src/lib/api/` | The three transports. `tauri.ts`: Tauri `invoke` and events, in the desktop app. `http.ts`: the `pelli-cull serve` bridge, `POST /invoke/<command>` and events as server-sent events, when `VITE_BRIDGE_URL` is set. `mock/`: a simulated engine with procedurally drawn photos, for `pnpm dev`, screenshots and tests. `index.ts` picks one at start-up. |
| `src/store/` | Zustand stores: the app (account, settings, projects), the analysis, the review (filters, selection, marks, undo), the workspace, the export, the Maximum pack (`pack-store.ts`), the updates (`update-store.ts`), and the photographer's preferences, kept in the webview's local storage (`prefs-store.ts`: sounds, voice-over, sub-folders…). |
| `src/components/` | One folder per screen or flow: `boot`, `signin`, `home`, `criteria` (before the first analysis), `analysis`, `review` (grid), `loupe`, `compare`, `export`, `settings`, `precision` (Standard or Maximum, the pack's download), `update`, `onboarding` (the welcome tour and its voice-over), `plans`, plus `ui` for shared components. |
| `src/lib/review/` | Pure logic with tests: keyboard shortcuts (`keyboard.ts`), selection, what the grid shows. |
| `src/lib/i18n/` | 9 languages, `fr.ts` as the reference, kept in sync by a test. |
| `src/assets/onboarding/` | The welcome tour's photos and voice-over, credited in [`CREDITS.md`](../src/assets/onboarding/CREDITS.md). |

## The Pelli account and the quota

The official builds ask for a Pelli account; the monthly number of photos you
can sort depends on its plan. `cull-engine/src/account.rs`, `pelli.rs` and
`usage.rs` handle it, against the API at `https://app.pelli.io/api/cull/*`
([docs/pelli-api.md](pelli-api.md); `PELLI_API_URL` points a debug build
elsewhere, for instance at `pnpm fake-pelli`).

```
 Pelli Instant                                browser                  app.pelli.io
 auth_start ── POST auth/create-session {deviceName, platform, appVersion} ──► sessionId, authUrl
   opens authUrl ───────────────────────► sign in to Pelli,
                                          "Allow Pelli Instant" ─────────────► approved
 auth_poll, every few seconds ── POST auth/check-session ───────────────────► access token, account
   token → macOS Keychain / Windows Credential Manager (service io.pelli.instant)
```

- **The token** is valid 30 days. When Pelli no longer accepts it, the app signs
  out and asks to sign in again. Debug builds (a new signature at every build
  would make macOS ask for the keychain password at each launch) and Linux
  keep it in the local library instead.
- **The account** (`me`: name, plan, photos sorted this month) is refreshed
  at start-up and before an analysis. Offline, the last known one is used.
- **The quota.** Before an analysis, the engine counts the photos it has
  never analysed and asks Pelli to reserve that number; when the run ends,
  it gives back the photos it could not analyse (cancelled, failed or
  unreadable). The requests carry only these numbers. Photos sorted again
  are not counted. On a limited plan, the reserved photos are kept in the
  library until the run ends: after a quit or a crash, the next analysis of
  the project doesn't reserve them again and gives back those it no longer
  needs. On the Free and Lite plans, an analysis that needs counting doesn't
  start offline (`quotaOffline`).
- **The offers** (Pelli's plans, in your language) are read to show what each
  plan includes, and cached for offline use. The app never hard-codes prices.
- **Without an account.** Built with `PELLI_INSTANT_NO_ACCOUNT=1`, the app
  skips sign-in and the quota entirely, and never sends the commands that
  talk to Pelli (`noAccount` in `AppInfo`).
- **Sending to a Pelli gallery.** The engine can also upload the selection
  to a gallery of the photographer's Pelli account, one folder per moment
  (`upload.rs`, `uploader.rs`, `src/components/export/GallerySection.tsx`).
  It is switched off in the interface for now (`SHOW_GALLERY_EXPORT` in
  `src/components/export/ExportDialog.tsx`).

## The Maximum pack

`cull-engine/src/modelpack.rs` downloads the files of the `maximum` bundle of
`models/manifest.json` into the data folder (HTTP range requests, so a
download resumes; SHA-256 of each file; at most about four `model-pack`
events a second), then prepares the pack: `cull_ml::MaximumModels::check`
runs every model on synthetic inputs against reference outputs compiled into
the app. Its states are `absent`, `downloading`, `preparing`, `ready` and
`failed` with a code; choosing Maximum in the interface starts or resumes the
download, going back to Standard pauses it. The details, and the failure
codes, are in [docs/models.md](models.md#how-the-app-downloads-and-checks-it).

## Updates

`src-tauri/src/updates.rs` drives Tauri's updater plugin. Release builds
read `latest.json` from the latest GitHub release eight seconds after
launch, then every six hours, and report only news (an offline computer
hears nothing). The update commands (`update_status`, `check_for_update`,
`install_update`) and the `update-status` event are answered by the shell,
not by the engine. Installing downloads the bundle, checks its signature
against the public key in `src-tauri/tauri.conf.json`, waits while
`Engine::busy()` is true, then installs and restarts. Debug builds never
check; `PELLI_INSTANT_DISABLE_UPDATER=1` switches it off in a release build.
How releases are signed is in [docs/contributing.md](contributing.md#signing).

## Where data lives

Nothing is written in your photo folders until you export.

| What | macOS | Windows |
| --- | --- | --- |
| Library: projects, measures, decisions, marks, settings, quota reservations (`library.sqlite`) | `~/Library/Application Support/io.pelli.instant/` | `%APPDATA%\io.pelli.instant\` |
| Maximum pack, when downloaded | `models/maximum/` next to the library | `models\maximum\` next to the library |
| Preview cache, one folder per project | `~/Library/Caches/io.pelli.instant/previews/` | `%LOCALAPPDATA%\io.pelli.instant\previews\` |
| Pelli token (release builds) | Keychain, service `io.pelli.instant` | Credential Manager, `io.pelli.instant` |
| Standard AI models | Inside the app: `Pelli Instant.app/Contents/Resources/models/` | `models\` in the install folder |
| Models already verified (`.verified.json`, by size and date) | Next to the library | Next to the library |

`PELLI_CULL_MODELS` points the app at another folder for the Standard
models. Removing a project deletes its previews and its analysis.
*Settings → Cache* shows the cache size and empties it; the Maximum pack is
not in the cache and stays. The `pelli-cull` CLI keeps its own library and
cache in `./.pelli-cull` unless `--data-dir` says otherwise.
