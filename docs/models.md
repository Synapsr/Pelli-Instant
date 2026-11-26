# Models

Pelli Instant judges focus, motion blur, exposure and duplicates with classic
image analysis, no neural network involved
([`crates/cull-analysis`](../crates/cull-analysis/README.md)). Neural
networks add what classic analysis can't see: faces, eyes and expressions,
which photos look alike, what you type in the search field, and an aesthetic
score. They come in two sets: **Standard**, inside the app, and the optional
**Maximum** pack, downloaded when the photographer asks for it. They all run
on the computer, with [ONNX Runtime](https://onnxruntime.ai).

## Standard: inside the app

| Model | id | File | Size | Licence | What it does here |
| --- | --- | --- | --- | --- | --- |
| **YuNet 2023mar** (OpenCV Zoo) | `yunet` | `face_detection_yunet_2023mar.onnx` | 0.23 MB | MIT | Finds faces: box, confidence, eyes, nose and mouth corners. Required. |
| **MediaPipe Face Mesh V2** (Google; ONNX by PINTO_model_zoo) | `face-mesh` | `face_landmarks_detector_Nx3x256x256.onnx` | 5.0 MB | Apache-2.0 | 478 points on each face, from which the eye opening, the head pose and the mouth opening are measured. |
| **MediaPipe Face Blendshapes V2** (Google; ONNX by Pelli) | `face-blendshapes` | `face_blendshapes_v2.onnx` | 1.9 MB | Apache-2.0 | 52 expression scores (blink, squint, smile, jaw open…) from 146 of those points. With the eye opening, they tell a blink from a smile that narrows the eyes. Without this file, the eye opening alone decides. |
| **CLIP ViT-B/32 image model**, int8 (OpenAI; ONNX and int8 by Xenova) | `clip-vision` | `clip-vit-b32-vision-int8.onnx` | 88.6 MB | MIT | A fingerprint of each photo, to group similar shots, to search, and for the aesthetic score. Runs on the CPU. |
| CLIP ViT-B/32 image model, fp16 | `clip-vision-fp16` | `clip-vit-b32-vision-fp16.onnx` | 176 MB | MIT | The same, run by DirectML on Windows. Windows installer only. |
| **CLIP ViT-B/32 text model**, fp16 (OpenAI; ONNX by Xenova) | `clip-text` | `clip-vit-b32-text-fp16.onnx` | 127 MB | MIT | Turns your search into a fingerprint to compare with the photos'. |
| **LAION-Aesthetics Predictor V1** (linear head for ViT-B/32) | `aesthetic` | compiled into the app | 2 KB | MIT | An aesthetic score from the CLIP fingerprint, used relative to your own shoot. |

The Mac installer carries about 223 MB of models; the Windows installer adds
the fp16 model (about 399 MB). For a build from source, `pnpm models`
downloads the same files as the Mac installer; `pnpm models --gpu` adds the
fp16 one.

Face detection is not face recognition: Pelli Instant finds where faces are
and whether their eyes are open. It doesn't tell who anyone is.

## Maximum: the optional pack

The Maximum pack is part of Pelli Instant Plus. With Plus, the photographer
picks it in *Settings → Analysis → Precision*, or in the precision step of
the welcome tour (without Plus, that choice opens the Plus offer). Choosing
Maximum starts the download in the background (about 1 GB); until the pack
is ready, and whenever Plus is not active, analyses run in Standard. A pack
already installed stays on disk when Plus goes away, unused. Going back to Standard pauses a download in progress, and
*Delete* removes the pack from the computer.

| Model | id | File | Size | Licence | What it does here |
| --- | --- | --- | --- | --- | --- |
| **CLIP ViT-L/14 image model**, int8 (OpenAI; ONNX and int8 by Xenova) | `clip-l14-vision-int8` | `clip-vit-l14-vision-int8.onnx` | 306 MB | MIT | A bigger fingerprint, for search and the aesthetic score. |
| **CLIP ViT-L/14 text model**, fp16 (OpenAI; ONNX by Xenova) | `clip-l14-text` | `clip-vit-l14-text-fp16.onnx` | 248 MB | MIT | Search in a shoot analysed in Maximum. Loaded only when such a shoot is searched. |
| **DINOv2 ViT-B/14**, fp32 (Meta; ONNX by onnx-community) | `dinov2-b14-fp32` | `dinov2-base-fp32.onnx` | 347 MB | Apache-2.0 | The fingerprint that groups bursts and near-duplicates. On the calibration shoots it separates duplicates from different scenes by +0.25 of cosine, against +0.11 for CLIP B/32. |
| **RF-DETR-Seg Small** (Roboflow; exported to ONNX by Pelli) | `rfdetr-seg-s` | `rfdetr-seg-small-384-w16.onnx` | 77 MB | Apache-2.0 | Outlines people and animals. On a photo without a usable face, the largest one (at least 1 % of the frame) is the subject whose focus and exposure judge the photo: a back-lit figure seen from behind. |
| **RetinaFace ResNet-50** (biubug6; exported to ONNX by Pelli) | `retinaface-r50` | `retinaface-r50-w16.onnx` | 55 MB | MIT | A second face detector, run at 1280 px on group photos only (YuNet found 3 faces or more, or a small one). It finds faces YuNet misses in crowds and profiles; they get the mesh and the blendshapes too. |
| **LAION-Aesthetics Predictor V2** (head for ViT-L/14; Christoph Schuhmann) | `aesthetic-l14` | compiled into the app | 3 KB | Apache-2.0 | The aesthetic score from the ViT-L/14 fingerprint, brought to V1's scale. |

The pack keeps YuNet, Face Mesh and the blendshapes, and needs every one of
its files. It is slower: per photo, on one worker of a busy M4, DINOv2 takes
about 240 ms, CLIP ViT-L/14 about 360 ms, RF-DETR about 500 ms (photos
without a usable face) and RetinaFace about 620 ms (group photos). It also
needs more memory, so the engine runs at most 8 analysis workers with it, 4
below 12 GB of memory or 8 cores. Details and measures:
[`crates/cull-ml/README.md`](../crates/cull-ml/README.md#maximum-precision-optional-pack).

**DINOv2 in fp32, not int8.** Up to 1.1.0 the pack had DINOv2's int8
export (91 MB). Its results depend on the processor's integer kernels
(ONNX Runtime picks them from the CPU's features: AVX2 or AVX-VNNI on x86,
dot product or i8mm on arm64). It was right on the Mac that computed the
check's references (correlation 0.997 to fp32) but computes garbage on
every x86 processor measured and on emulated arm64 cores: fingerprints with
a cosine of 0.12–0.14 on average to the fp32 model's on photos, a check
correlation of 0.46–0.49. That is why the pack never installed on Windows,
and most likely why it failed on some Macs. Measured on an i7-14700KF:

| DINOv2 | Cosine to fp32 (39 photos) | Time a photo, 4 sessions × 1 thread | Memory, 4 sessions | Download |
| --- | --- | --- | --- | --- |
| fp32 (chosen) | 1.0000 | 106–137 ms | 1.58 GB | 347 MB |
| fp16 | 1.0000 (min) | 130–172 ms (+20–30 %) | 1.74 GB | 176 MB |
| int8 (1.1.0) | 0.13 mean, −0.03 min | 39 ms | 0.54 GB | 91 MB |

fp16 saves 171 MB of download once, but ONNX Runtime casts it at run time
on an x86 CPU (slower and heavier at every analysis), and on arm64 it
computes in half precision: cosine to fp32 0.991 on average, 0.988 at worst
on photos (emulated arm64 cores), enough to blur near-duplicates. fp32 is
the same file for every processor, exact everywhere. The
engine keeps 2 DINOv2 sessions of 2 threads (as fast as 4 of one, half the
memory). CLIP ViT-L/14 stays int8: its fp32 export is 1.2 GB and as many
sessions would need several GB of memory, while its int8 results, though
they vary with the processor, stay close to fp32 everywhere measured
(cosine ≥ 0.96 on photos, below).

The manifest can mark a model made for one processor architecture with
`arch` (`arm64`, `x64`); the app sends its architecture to Pelli, which
hands out the files of that architecture. Today both get the same files.

The models used are part of every analysis (`models_id`): switching
precision analyses the photos again. A project analysed in Maximum is
searched with the ViT-L/14 text model.

### How the app downloads and checks it

[`crates/cull-engine/src/modelpack.rs`](../crates/cull-engine/src/modelpack.rs)
does it, with the sizes and checksums of the manifest compiled into the app.
The manifest has no URL for these files: the app asks Pelli
(`POST /api/cull/models`, [docs/pelli-api.md](pelli-api.md#models-licence-or-bearer))
with the Plus licence token, or the Bearer of a Pelli account whose plan
includes Plus, and its processor architecture (`arch`), and gets links valid
12 hours. Without Plus the answer is `plusRequired` and nothing is
downloaded. When the answer lacks a file of this architecture (a Pelli older
than the app), the pack fails at once with `unsupported`.

1. **Before starting**, it checks that the disk holds what is left to
   download plus 256 MB, and asks Pelli for the links, so that a missing
   network or licence fails at once.
2. **Each file** downloads into `<file>.part` with HTTP range requests: a
   paused, interrupted or quit download resumes where it stopped. Once
   complete, its SHA-256 is checked against the manifest and the file is
   renamed into place. A file that doesn't match is deleted and fetched
   again: 3 attempts, with growing pauses. When the storage answers 403 (the
   link expired, the download took long), the app asks Pelli for new links
   and resumes from the part (3 times at most per file).
3. **Preparing**: each model is loaded once (its SHA-256 checked again) and
   run on synthetic inputs, and its outputs are compared with reference
   values computed from each model's full-precision (fp32) export
   ([`crates/cull-ml/assets/maximum-check.json`](../crates/cull-ml/assets/maximum-check.json)),
   the same for every processor, within a tolerance per model: a
   correlation of at least 0.99 for the models computed in floating point
   (DINOv2, RF-DETR, RetinaFace, the text model: 1.0000 measured), 0.85
   for CLIP ViT-L/14 int8, whose integer kernels differ from one processor
   to the next (0.919–0.966 measured; a model computing garbage gives
   0.46–0.49). A runtime or a file that loads but computes wrong fails
   here, before any photo, with `checkFailed`; the int8 DINOv2 of 1.1.0
   would fail on x86 (0.49). The references are recomputed with
   `cargo run --release -p cull-ml --example maximum_check -- --references <folder>`
   (the fp32 exports, listed in that file), and
   `--file dinov2 <file.onnx>` tries another variant of a model.
4. **Ready.** The sizes and dates of the prepared files are remembered in
   the library. At the next launch the pack is ready at once if they still
   match, prepared again in the background if the files are complete but
   changed, and shown as paused if only parts are on disk. A failed check
   is remembered too: the next launches show it without preparing the same
   files again, until *Try again* (which downloads the failing model's file
   again before preparing) or an update of the app. Files the pack no
   longer uses (1.1.0's int8 DINOv2) are deleted at launch; if the pack was
   in use, the new file downloads at once when Maximum is chosen and Plus
   active.

The pack lives in the app's data folder, `models/maximum/` next to the
library ([where](architecture.md#where-data-lives)), never in the cache:
emptying the cache keeps it. Its models are loaded when an analysis in
Maximum starts and released after it. If the pack fails to load then, the
analysis runs in Standard.

Every change of state is a `model-pack` event. A download that stops ends in
the `failed` state with a code, and the downloaded bytes stay on disk:

| Code | When |
| --- | --- |
| `cancelled` | Paused by the photographer, or parts found on disk at launch. *Resume* goes on from there. |
| `offline` | No network, or the connection was lost. |
| `diskFull` | Not enough free space for what is left (plus 256 MB). |
| `checksum` | A file never matched its SHA-256, or failed the check when loaded. |
| `checkFailed` | A model loads but computes wrong results on this computer (its sanity check), or does not load. Remembered: not prepared again at each launch. |
| `unsupported` | Pelli handed out no link for a file of this processor's architecture (a Pelli older than the app). |
| `plusRequired` | Pelli refused the links: Plus is no longer active (licence released or refunded, plan changed). |
| `notFound` | The storage answered 401, 404 or 410, or still 403 after new links, or a file is missing when the pack is prepared. |
| `internal` | Any other error: another HTTP status, a file that can't be written. |

Every failure but a pause is also reported as the `model_pack_failed`
telemetry event (code and model id only, unless telemetry is off), and
written to the app's log file (macOS `~/Library/Logs/io.pelli.instant/`,
Windows `%LOCALAPPDATA%\io.pelli.instant\logs\`, `pelli-instant.log`).
A CI job (`.github/workflows/maximum-pack.yml`) downloads the pack on a
Windows x64 runner and a macOS arm64 one (Apple M1) and runs the check
there.

**Debug builds only:** `PELLI_MODELS_BASE_URL=http://localhost:8000 pnpm tauri dev`
downloads every file of the pack from `<base>/<file name>` instead of asking
Pelli, and then needs no Plus: a local copy served by any static server with
HTTP range support. Release builds ignore the variable. The fake Pelli
(`pnpm fake-pelli`, see [docs/contributing.md](contributing.md#a-fake-pelli-plus-telemetry-sign-in))
also serves a local copy, behind its fake Plus.

## Where they come from

[`models/manifest.json`](../models/manifest.json) is the single source of
truth: for each file, its size, SHA-256, licence, source, purpose and bundle
(`default`, `gpu` or `maximum`), and the download URL of the Standard and
GPU files. The manifest is compiled into the app.

- **Hugging Face, at pinned revisions**: YuNet (`opencv/face_detection_yunet`,
  with a GitHub mirror) and CLIP ViT-B/32 (`Xenova/clip-vit-base-patch32`).
  Each of these URLs names a commit, so the file behind it never changes;
  YuNet's mirror follows a branch, which the SHA-256 check covers.
- **Face Mesh** comes from PINTO_model_zoo's storage, inside a `.tar.gz` whose
  size and SHA-256 are checked too.
- **The Face Blendshapes**, converted by Pelli (no upstream ONNX), are
  hosted at
  `https://pelli.s3.fr-par.scw.cloud/pelli-instant/models/face_blendshapes_v2.onnx`.
  A hosted file never changes: a new conversion gets a new file name.
- **The Maximum pack** comes through Pelli, for Plus: CLIP ViT-L/14
  (`Xenova/clip-vit-large-patch14`) and DINOv2
  (`onnx-community/dinov2-base-ONNX`) exported upstream, RF-DETR-Seg and
  RetinaFace converted by Pelli. The manifest records each file's source,
  licence, size and SHA-256; where Pelli stores them is its own
  configuration.
- **The LAION heads** are compiled into the app
  ([`crates/cull-ml/assets/`](../crates/cull-ml/assets/)); the manifest
  records the original files they were converted from and their SHA-256.

How each set reaches the computer:

- **In the installers**, the Standard models are inside the app, put there by
  the release workflow after `scripts/fetch-models.mjs --strict` downloaded
  and verified them. The app downloads nothing else on its own: the Maximum
  pack only comes when a photographer with Plus asks for it.
- **Building from source**, `pnpm models` runs that same script.
- **At every launch**, `cull-ml` checks each Standard file's SHA-256 against
  the manifest before loading it. A file that doesn't match is not used. The
  result is remembered in `.verified.json` (by file size and date) so the
  check doesn't slow every launch. YuNet is required; without the others,
  the app still sorts, with fewer measures.

### `scripts/fetch-models.mjs`

Node 20 or later, no dependencies. It downloads each file once, checks its
size and SHA-256, and skips files already there with the right checksum. It
honours `HTTPS_PROXY` / `https_proxy` and `NO_PROXY` (extra certificates
through `NODE_EXTRA_CA_CERTS`), tries the mirrors, and extracts archive
members.

| Command | What it does |
| --- | --- |
| `pnpm models` | The Standard set (`bundle: "default"`) into `models/`. |
| `pnpm models --gpu` | Adds the fp16 CLIP image model (the Windows installer). |
| `pnpm models --maximum` | Adds the Maximum pack of this processor (`--arch arm64` or `x64` for another's, should their files differ) into `models/maximum/`, for development: `pelli-cull analyze --precision maximum` reads it there (`--maximum-dir` elsewhere). Part of Plus: needs `PELLI_INSTANT_LICENSE=<licence token>` (links from Pelli) or `PELLI_MODELS_BASE_URL=<your copy>`. The app downloads its own copy. |
| `pnpm models --all` | Everything in the manifest. |
| `pnpm models --only yunet,face-mesh` | Only these ids. |
| `pnpm models --check` | Verifies only; exits with 1 when a file is missing or corrupt. |
| `pnpm models --strict` | A model that can't be fetched is an error, not a warning (installers). |
| `pnpm models --dir <folder>` | Another folder than `models/`. |

## Where they run

YuNet, Face Mesh, the blendshapes and the text models always run on the CPU:
they are small, or only run when you search. The CLIP ViT-B/32 image model is
the heavy one of the Standard set. On Windows it runs in fp16 through
DirectML, and in int8 on the CPU when DirectML isn't available. On Apple
Silicon it runs in int8 on the CPU: there it is 3 to 4 times faster than
CoreML in fp16 (60–80 photos a second against about 20 on an M4), so the
Mac installer doesn't carry the fp16 model. *Settings → About* shows which
accelerator is in use. There is no Intel Mac build: ONNX Runtime ships no
prebuilt library for Intel Macs
([packaging notes](../crates/cull-ml/README.md#packaging-onnx-runtime-tauri)).

The Maximum pack runs on the CPU everywhere: in fp16 on the Neural Engine,
DINOv2, RF-DETR and CLIP ViT-L/14 silently return wrong values. Int8
models depend on the processor's integer kernels: DINOv2 int8 is wrong on
many processors (hence its fp32 file), CLIP ViT-L/14 int8 stays close to full
precision everywhere measured (cosine to fp32 on the calibration photos:
0.977 on average, 0.965 at worst on an AVX-VNNI x86; 0.978 and 0.963 on
emulated arm64).

The int8 and fp16 variants give close but not identical fingerprints, and
Standard and Maximum fingerprints can't be compared at all: the models used
are stored with each analysis and never mixed within a shoot.

## What they don't do

- They don't learn. The files are fixed; your shoots change nothing in them.
- They don't send anything. ONNX Runtime's telemetry is switched off, and
  the photos are read from your disk only.
- They don't decide alone. Closed eyes on Balanced or Lenient send a photo
  to review, never to the rejects. A low aesthetic score, measured against
  the rest of your shoot, is never a reason to reject: at most it sends a
  photo to review (it does help pick the best shot of a burst).

## Known limits

Measured on 70+ freely licensed photos and three real shoots; details in
[`crates/cull-ml/README.md`](../crates/cull-ml/README.md#accuracy-observed).

- A laugh with the eyes squeezed shut reads open: it is an expression, not a
  blink. Eyes behind sunglasses are guessed. Very small or low-quality faces
  can still read as closed.
- Faces smaller than about 1 % of the photo's long side (crowds) are not
  reliably found; in Maximum, RetinaFace finds more of them on group photos.
- Animal faces are sometimes found as faces (they get no eye reading).
- The text models understand English. A glossary translates common wedding,
  event and portrait words from French, Spanish, German and Italian.

## Licences and training data

Every model's weights are under a permissive licence (MIT or Apache-2.0); see
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md), which also lists the
changes Pelli made to the files it converted. The training data have their
own terms, as for nearly every public model: YuNet and RetinaFace were
trained on WIDER FACE (research-use terms), CLIP on OpenAI's own image-text
set, the LAION heads on public aesthetic ratings, Face Mesh and the
blendshapes on Google's data, DINOv2 on Meta's LVD-142M, RF-DETR-Seg on
COCO. No dataset is distributed with Pelli Instant.

Contributions must keep it that way: never add weights under a
non-commercial, GPL or AGPL licence (InsightFace's models, for instance). A
new model goes in `models/manifest.json` with a pinned URL, its size and
SHA-256, and in `THIRD_PARTY_NOTICES.md`. A file Pelli converts goes to
Pelli's storage under a new name, with its source in the manifest and in
the `README.txt` there. Why each model was chosen, and the ones rejected
after evaluation, is in
[`crates/cull-ml/README.md`](../crates/cull-ml/README.md#models).
