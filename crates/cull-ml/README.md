# cull-ml

Neural networks of Pelli Instant: faces, eye state, CLIP embeddings (similar
shots, text search) and aesthetic score, run locally with ONNX Runtime
through the [`ort`](https://crates.io/crates/ort) crate, on the CPU or a
GPU (DirectML on Windows, CUDA on Linux; CoreML on request only: on Apple
Silicon the int8 CPU path is faster).

```rust
let models = ModelSet::load(&models_dir, &RuntimeOptions::default())?; // verifies SHA-256
models.prepare(workers);                 // sessions created now, kept
let obs = models.analyze(&rgb_image)?;   // faces + mesh + blendshapes, embedding, aesthetic
let obs = models.analyze_with_detail(&preview, &|long_edge| full_size(long_edge))?; // small faces
let q = models.embed_text("a bride holding flowers")?;  // cosine with obs.embedding
models.accelerator();                    // "CPU" | "CoreML" | "DirectML" | "CUDA"

// Optional Maximum pack (see "Maximum precision" below)
let max = MaximumModels::load(&pack_dir, &opts)?;       // verifies, one session per model
max.check(&|progress| ())?;                             // reference outputs, before first use
let obs = models.analyze_maximum(&max, &preview, &detail)?;
let q = MaximumText::load(&pack_dir, &opts)?.embed_text("a bride")?; // vs obs.search_embedding
```

`ModelSet` is `Send + Sync`; `analyze` may be called from many threads at
once. `ort` 2.0's `Session::run` takes `&mut self`, so each model has a
small pool of sessions, up to the number of CPU cores (2 for a GPU
session), kept for the life of the `ModelSet`. `prepare(workers)` creates
them in advance, in parallel (0.1 s per worker and model on an M4); without
it they are created on first use.

## Models

All weights and code are licensed for commercial closed-source
distribution. Licenses were checked on the primary source (links below); the
compiled-in `models/manifest.json` records URL, size, SHA-256, license and
purpose of every file. Model files are **not** in git: run
`node scripts/fetch-models.mjs` (Node ≥ 20, no dependencies; `--gpu` adds
the fp16 CLIP, `--check` verifies only).

| Model | File (size) | License | Used for |
|---|---|---|---|
| **YuNet 2023mar** (OpenCV Zoo, Shiqi Yu et al.) | `face_detection_yunet_2023mar.onnx` (0.23 MB) | MIT — [LICENSE](https://github.com/opencv/opencv_zoo/blob/main/models/face_detection_yunet/LICENSE) | faces: box, score, 5 landmarks. **Required.** |
| **MediaPipe Face Mesh V2** (Google), ONNX by [PINTO_model_zoo #410](https://github.com/PINTO0309/PINTO_model_zoo/tree/main/410_FaceMeshV2) | `face_landmarks_detector_Nx3x256x256.onnx` (5.0 MB, from an 18.6 MB `.tar.gz`) | Apache-2.0 — [model card](https://storage.googleapis.com/mediapipe-assets/Model%20Card%20MediaPipe%20Face%20Mesh%20V2.pdf) ("Licensed under Apache License, Version 2.0"); PINTO folder LICENSE: Apache-2.0 | eye open/closed, head pose |
| **MediaPipe Face Blendshapes V2** (Google), from the official `face_landmarker.task` (float16/1), ONNX by Pelli (tf2onnx, I/O renamed, explicit `Conv` pads; weights unchanged) | `face_blendshapes_v2.onnx` (1.9 MB) | Apache-2.0 — [model card](https://storage.googleapis.com/mediapipe-assets/Model%20Card%20Blendshape%20V2.pdf) | blink / squint / smile → eye state (optional: the EAR alone without it) |
| **CLIP ViT-B/32 image tower**, int8 (OpenAI; ONNX + quantization by [Xenova](https://huggingface.co/Xenova/clip-vit-base-patch32)) | `clip-vit-b32-vision-int8.onnx` (88.6 MB) | MIT — [openai/CLIP LICENSE](https://github.com/openai/CLIP/blob/main/LICENSE); HF card `license: mit` | embeddings, aesthetic (CPU) |
| CLIP ViT-B/32 image tower, fp16 (optional, `--gpu`) | `clip-vit-b32-vision-fp16.onnx` (176 MB) | MIT | same, on CoreML/DirectML/CUDA |
| **CLIP ViT-B/32 text tower**, fp16 | `clip-vit-b32-text-fp16.onnx` (127 MB) | MIT | text search |
| **LAION-Aesthetics Predictor V1**, linear head ViT-B/32 | `assets/laion_aesthetic_linear_vit_b_32.f32` (2 KB, compiled in) | MIT — [LICENSE](https://github.com/LAION-AI/aesthetic-predictor/blob/main/LICENSE), copy in `assets/` | aesthetic score |
| CLIP BPE tokenizer: [`instant-clip-tokenizer`](https://crates.io/crates/instant-clip-tokenizer) + OpenAI vocabulary | compiled in | MIT | text search |
| ONNX Runtime 1.31 (Microsoft) via `ort` 2.0.0-rc.15 | statically linked | MIT / `ort`: MIT OR Apache-2.0 | runtime |

Desktop bundle (default set): ~223 MB of models; Windows adds the fp16 CLIP
(`--gpu`, run by DirectML): ~399 MB. The Mac installer does not ship it (the
int8 CPU path is faster on Apple Silicon). The optional Maximum pack
(~777 MB) is downloaded by the app (see below).

**Provenance caveat for legal review** (licenses of the *weights* are
permissive; the training data have their own terms, as for virtually every
public model): YuNet was trained on WIDER FACE (research-use dataset terms),
CLIP on OpenAI's private image-text set (its model card lists "deployed use
cases" as out of scope, a recommendation, not a license term), the LAION
head on SAC/LAION-Logos/AVA ratings, Face Mesh on Google's own data. No
model here is distributed under a non-commercial license.

Rejected after evaluation:
- **OpenVINO `open-closed-eye-0001`** (Apache-2.0, 46 KB, 32x32 eye-crop
  classifier; ONNX at `storage.openvinotoolkit.org/.../open_closed_eye.onnx`):
  trained on MRL Eye (near-infrared driver-monitoring crops); on our colour
  photos it caught only 17/26 closed eyes vs 24/24 for the mesh EAR.
- Xenova `vision_model_quantized.onnx` (uint8 dynamic): cosine to fp32
  0.94 mean / 0.83 min, aesthetic error up to 1.1; `vision_model_int8.onnx`
  is 2x faster and closer (0.975 / 0.93, error 0.22 mean). q4/bnb4: no better
  and slower on CPU. fp16 on CPU: 4x slower (casts).
- Text tower int8: cosine to fp32 0.90–0.97 (bad for search); fp16 is 1.000.
- InsightFace (buffalo, non-commercial) and any NC/GPL weights: excluded.

## Pipeline and preprocessing

`analyze(img)` on an upright RGB photo of any size:

1. **Working copy**: long side ≤ 1280 (box filter, `fast_image_resize`).
2. **Faces (YuNet)**. The published ONNX declares a fixed 1x3x640x640 input
   but the graph is fully convolutional: at load we rewrite the declared
   input H/W (and output anchor dims) as symbolic in the protobuf (weights
   untouched, checksum verified on the original), so it runs at the photo's
   aspect ratio. Input BGR float 0–255 NCHW, zero-padded to multiples of 32.
   Decoding per stride 8/16/32: `score = sqrt(clamp(cls)·clamp(obj))`,
   `cx = (col+dx)·s`, `w = exp(dw)·s`, landmarks `(col+kx)·s`; score ≥ 0.6;
   NMS IoU 0.3. Passes: 640 px long side; + 1280 px when a face is < 40 px
   (at 640) or ≥ 3 faces (groups); 320 px when nothing was found (face filling
   a close-up; YuNet is trained for ~10–300 px faces). Coordinates are mapped
   back to normalised upright coordinates; faces sorted largest first.
3. **Detail image** (`analyze_with_detail`): when a face confident enough
   to be a subject (≥ 0.8) has 12-40 px between the eyes at 1440 px, or
   3 faces have under 64 px (a group), the caller is asked once for the
   same photo at a larger size (enough for 64 px between the eyes, at most
   2.5x: the RAW's full-size JPEG or the camera JPEG at 1/2 scale); the
   mesh then reads every face on it.
4. **Face mesh** (24 largest faces whose box is ≥ 48 px in the sampled
   image, i.e. ≥ 20 px between the eyes for a frontal face,
   `EYE_STATE_MIN_IOD_PX`; profiles included): square crop of 1.3 x
   max(face w, h) centred on the box, rotated so the eye line is
   horizontal, sampled from the most detailed image (pre-reduced with the
   SIMD resizer, then bilinear) to 256x256 RGB [0,1], batched by 8. Face
   Mesh returns 478 landmarks + face presence (`mesh_presence`: < 0.5 = not
   a face, e.g. a bicycle part YuNet boxed). **Tilted heads**: when the
   mesh's own eye line (corners 33 → 263) differs from the crop's by more
   than 8°, the face is read again on MediaPipe's tracking crop (rotated
   along that line, centred on the landmarks, 1.5 x their extent), at most
   3 passes: YuNet reads a head tilted by 52° as −2°, and the mesh then
   put the eyes on the eyebrows (EAR "open" for closed eyes). On the
   calibration shoots a re-crop happened for 129 of 522 face readings
   (portrait session 46/160, spa shoot 70/268, Mariage-Test 13/94), with no
   change to the labelled eye states. Eye aspect
   ratio `EAR = (|p2-p6|+|p3-p5|) / 2|p1-p4|` (indices 33/160/158/133/153/144
   and 362/385/387/263/373/380), `p_open = sigmoid((EAR - 0.145) / 0.012)`.
   `None` when presence < 0.5 or the eye is hidden (narrower than 0.45x the
   other eye). Head pose (`pose`): yaw from the depth across the cheeks
   (234 → 454), pitch from forehead → chin (10 → 152), roll from the eye
   corners plus the crop rotation, mouth opening = inner lips (13-14) /
   inner mouth width (78-308).
5. **Blendshapes** (every face the mesh read, CPU, ~1 ms): x, y of the 146
   landmarks of MediaPipe's `kLandmarksSubsetIdxs`, in crop pixels → 52
   ARKit scores; kept in `expression` (blink, squint per eye, smile, jaw).
   The eye state then combines both readings, per eye:
   `p_open = sigmoid(0.5·clamp((EAR − 0.145)/0.012, ±5) + (0.70 − b)/0.05)`
   with `b = blink − 0.35·squint·smile + 0.2·squint·(1 − smile)`. The EAR is
   decisive for eyes shut tight, relaxed or squeezed (0.00-0.05; the
   blendshapes still give blink 0.4-0.55 with a high squint there); the
   blendshapes tell a smile (blink 0.3-0.5, squint × smile high, EAR
   0.15-0.19) from a blink (0.65-0.85). Calibrated on 26 hand-labelled faces
   of three real shoots and checked on 220 faces both readings call open
   (all ≥ 0.99): open smiles and downcast looks ≥ 0.75, blinks and closed
   poses ≤ 0.33 except two squeezed laughs (read open, which the culling
   treats as intended). `models_id` tag `blendshapes-v2-eye1`.
6. **CLIP image**: the **whole** image letterboxed: long side Catmull-Rom
   (PIL bicubic) to 224, centred on a 224x224 canvas whose bars are the
   CLIP mean colour, `/255`, mean (0.4815, 0.4578, 0.4082), std
   (0.2686, 0.2613, 0.2758), NCHW → `image_embeds` (512), L2-normalised.
   (`CLIPImageProcessor` crops the central square: the outer thirds of a
   3:2 frame were ignored, framings differing on the sides looked alike.)
7. **Aesthetic**: `w · e + b` on the L2-normalised embedding (LAION scale).
8. **Text**: lower-case, whitespace collapsed, CLIP BPE,
   `[49406] + tokens[..75] + [49407]`, zero-padded to 77, int64 →
   `text_embeds`, L2-normalised.

`models_id` (stored with each analysis):
`yunet-2023mar+facemesh-v2-ear3+blendshapes-v2-eye1+clip-b32-int8-lb+laion-linear-b32`
(`clip-b32-fp16-lb` when the accelerated model ran: int8 and fp16
embeddings agree to ~0.975 cosine, don't mix them in one project's
grouping; `ear3`, `eye1` and `lb` are the mesh, blendshapes and CLIP preprocessing
revisions: a change re-analyses the photos).

## Maximum precision (optional pack)

`MaximumModels` (module `maximum`) loads the five files of the `maximum`
bundle (~777 MB, downloaded by the app into its data folder, see
`cull-engine/src/modelpack.rs`); `analyze_maximum` then replaces, for the
same faces, mesh and blendshapes:

| Step | Standard | Maximum |
|---|---|---|
| faces | YuNet | YuNet, plus **RetinaFace R50** at 1280 px on group photos (YuNet found 3+ faces or a face under 1/16 of the long side; score ≥ 0.9, a new face only where YuNet has none): its faces get the mesh and blendshapes too |
| grouping embedding (`embedding`) | CLIP B/32, letterboxed | **DINOv2 B/14** int8, CLS of the photo squashed to 224x224, ImageNet normalisation (duplicates vs different scenes: +0.25 of cosine against +0.11 for CLIP B/32) |
| search embedding | `embedding` | `search_embedding`: **CLIP L/14** int8, letterboxed; queries with its fp16 text tower (`MaximumText`, loaded on first search) |
| aesthetic | LAION V1 head on B/32 | **LAION V2** head on L/14 (an MLP without activations, collapsed exactly into one linear map), mapped onto V1's scale (`1.552 × v2 − 2.878`, median and MAD matched on the 570 photos of the calibration shoots) |
| subject without a usable face | — | **RF-DETR-Seg Small** (384x384 squashed, ImageNet normalisation): largest person (COCO 1) or animal (16-25) mask scoring ≥ 0.5 and covering ≥ 1% of the frame → `subject` |

`models_id` = the face tags + `retinaface-r50`, `dinov2-b14-int8-sq`,
`clip-l14-int8-lb`, `laion-v2-l14`, `rfdetr-seg-s-384`
(`is_maximum_models_id`): switching precision re-analyses the photos.

Everything runs on the CPU (int8, or fp16-stored weights computed in fp32):
the accelerator variants were rejected (10 min of CoreML compilation for
CLIP L/14, silent errors on the Neural Engine). CLIP L/14 keeps 2 sessions
(3 with ≥ 8 cores and ≥ 12 GB of memory) of a few intra-op threads each,
RF-DETR and RetinaFace 2, DINOv2 up to 4, whatever the number of workers;
the engine runs at most `MaximumModels::max_workers()` analysis workers
with the pack (8, or 4 below 12 GB or 8 cores): more only wait, holding
decoded photos in memory. Peak memory of the CLI on a 211-photo shoot
(10-core M4, 32 GB, pack preparation included): Standard 2.8-3.5 GB,
Maximum 4.4 GB with 8 workers, 3.7 GB with 4. `MaximumModels::check` runs each model on a synthetic input
and compares its outputs with reference values computed when the pack was
built (`assets/maximum-check.json`, Pearson correlation ≥ 0.99): the app
does it once after the download ("preparing").

Cost per photo on one worker (M4, machine loaded, load average ~20):
DINOv2 ~240 ms, CLIP L/14 ~360 ms (3 threads), RF-DETR ~500 ms (photos
without a usable face only), RetinaFace ~620 ms (group photos only).

## Accuracy observed

On 70+ freely licensed photos (Wikimedia Commons, Openverse/Flickr CC; kept
locally in `samples/`, never committed): portraits, couples, families,
group/crowd shots, closed eyes (singers, sleeping, laughing), profiles,
animals, landscapes. Most are ≤ 2 MP Flickr renditions, i.e. *harder* than
real 24 MP files for small faces.

- **Faces**: every frontal/three-quarter face of the portraits, couples and
  families found (conf 0.83–0.96); crowd shots at 1024 px: 50–276 faces of
  6–15 px found (not exhaustively counted); close-ups filling the frame and
  two human profiles found (eyes `None` for profiles). Misses: ~25 px faces
  of three low-contrast vintage full-length bride scans (also missed at
  full resolution). False positives: 2 tiny boxes on a patterned fabric
  (fine pass), animal faces (lion, lynx) at 0.63–0.81. 0 faces on 7
  landscapes / seascapes, flower close-ups and back-view or silhouette
  couples.
- **Eyes**, on three real shoots judged by the photographer (a 165-photo
  D810 portrait session, a 144-photo delivered spa shoot, the Mariage-Test
  set), EAR + blendshapes against the EAR alone: on Strict, the portrait
  session flags 8 photos instead of 18, all with closed eyes (both real
  blinks kept; dropped: smiles, downcast looks, a laugh read as closed); on
  Balanced, the spa shoot's group photos lose their 2 false "eyes closed"
  (downcast looks during a laugh) and keep one boy with his eyes really
  shut; no new flag anywhere. Eye probabilities of faces both readings call
  open stay ≥ 0.99 (220 faces). Unknown (`None`) for a face lying at 90°
  and faces < 20 px IOD. Known failures: eyes behind sunglasses (guessed
  by the mesh), very small or low-quality faces.
- **Similarity** (cosine of embeddings): simulated burst frames (2–4 %
  reframing, 0.7° rotation, +8 % exposure) 0.967–0.989; same singer, two
  moments of a concert 0.86; same concert, other framing 0.75; two different
  mountain landscapes 0.73; unrelated photos 0.29–0.62. int8 noise alone is
  ~0.99, so thresholds above 0.97 are not meaningful.
- **Text search**: "a bride holding flowers" → a bride with her bouquet
  first; "a large group photo of many people", "a mountain landscape", "a
  sunset over the sea", "a man playing guitar on stage", "a wedding couple
  kissing" → matching photo first. Scores are low in absolute terms (0.26–
  0.32): rank, don't threshold.
- **Aesthetic**: 3.5–7.8 on our photos (crowd snapshots 3.5–4.6, portraits
  5–7, landscapes/sunsets 6.2–7.8); int8 vs fp32 error 0.22 mean (max 0.95):
  use it within a shoot, not as an absolute grade.

## Performance

Measured on the dev container: Intel Xeon 2.1 GHz (AVX-512 VNNI, SHA-NI),
4 vCPUs **shared with other builds** (load average 4–9 during the runs), so
wall times are pessimistic; CPU time per photo (`/proc/thread-self/schedstat`)
is the more stable number. Release build, `intra_threads = 1`.

| Stage (24 MP photo) | CPU ms |
|---|---|
| resize to 1280 (box) | ~16 |
| YuNet 640 pass / 1280 pass / 320 pass | ~6 / ~25 / ~2 |
| Face Mesh, per face (crop + inference) | ~15 |
| CLIP ViT-B/32 int8 | ~37–47 |

- Whole `analyze` (CPU time per photo): 24 MP portrait with one face
  ~79 ms; 24 MP landscape (no face) ~67 ms; mixed 24 MP set (1–3 faces)
  ~115 ms; mixed 1–2 MP set ~85 ms. CLIP is ~55 % of a no-face photo.
- Throughput on this shared box (load average ~5 from other jobs):
  1–2 MP photos: 1 thread 11.6 photos/s, 2 threads 20.5, 4 threads 28.9;
  24 MP photos: 1 thread 8.7 photos/s, 4 threads 14.9. On an idle 4-core
  machine expect close to linear scaling (sessions are independent).
  `examples/validate bench THREADS ROUNDS IMG…` prints wall and CPU time.
- A modern laptop core (Apple M-series, Zen 4) is roughly 1.5–2x this vCPU:
  ~50–60 ms per single-face photo on one core.
- Memory: ~260 MB with one worker, +~140 MB per extra concurrent worker
  (one CLIP int8 session each; prepacked weights shared).
- Load: ~0.3–0.5 s (SHA-256 of ~220 MB ≈ 0.15 s with SHA-NI / ARMv8; cached in
  `<models_dir>/.verified.json` by size + mtime, best effort).

## Cargo features and execution providers

| Feature | Target | Effect |
|---|---|---|
| `coreml` | macOS | CoreML EP (MLProgram, all compute units, compiled models cached in `RuntimeOptions::accelerator_cache_dir`) for CLIP fp16, only with `Acceleration::Accelerator`: on an M4 the fp16 graph (dynamic batch, only partly on the Neural Engine) ran 3-4x slower than int8 on the CPU |
| `directml` | Windows | DirectML EP for CLIP fp16 |
| `cuda` | Linux/Windows | CUDA EP (CUDA 13 + cuDNN 9 on the host) for CLIP fp16 |

Each feature compiles everywhere and is a no-op on other targets
(`ort/lax-feature-matching` selects the plain prebuilt runtime there).
`Acceleration::Auto` tries the compiled providers except CoreML with the
fp16 model (if present) and falls back to int8 on the CPU silently, with a
log line;
`accelerator()` tells what is in use. YuNet, Face Mesh, the blendshapes and
the text towers always run on the CPU (small, or latency-insensitive; CoreML
splits the blendshapes into 9 partitions). Graph optimisation level 3,
telemetry disabled, intra-op spinning off when `intra_threads > 1`.

CoreML compute units are set **per model** (`runtime::CoreMlUnits`): in
fp16 the Neural Engine silently returns wrong values for DINOv2, RF-DETR
and CLIP ViT-L/14 (cosine −0.03 to the CPU, no error, no NaN), the GPU is
exact. CLIP B/32 fp16 keeps CoreML's own choice (`All`); RF-DETR, the only
Maximum model allowed on CoreML (and only with `Acceleration::Accelerator`),
uses `CPUAndGPU`. Models with symbolic input sizes get them fixed at session
creation (`with_dimension_override`: DINOv2 and CLIP L/14 `1x3x224x224`,
the L/14 text tower `1x77`), so ONNX Runtime folds the shape arithmetic and
CoreML would take each graph in one partition.

## Packaging ONNX Runtime (Tauri)

`ort` (`download-binaries`) fetches pyke's prebuilt ONNX Runtime 1.31 from
`cdn.pyke.io` at build time (cached in `~/.cache/ort.pyke.io`, checksum
verified, `tls-native`, honours `HTTPS_PROXY`). These are **static**
libraries linked into the executable:

- **macOS arm64**: fully static (+ system `CoreML`/`Foundation`
  frameworks). Nothing to ship; sign/notarise the single binary as usual.
- **macOS x86_64 (Intel)**: pyke ships **no** prebuilt for this target in
  rc.15. Either build ONNX Runtime (static) and point `ORT_LIB_PATH` at it,
  or ship arm64-only.
- **Windows x64**: ONNX Runtime is static, but pyke's Windows build always
  links **DirectML** (with or without our `directml` feature):
  `DirectML.dll` must sit next to the `.exe` (it is copied into
  `target/<profile>/` by `copy-dylibs`). In `tauri.conf.json`:
  `"bundle": { "resources": { "../target/release/DirectML.dll": "./" } }`
  (or the equivalent `windows` installer hook), and keep it in the app's
  code-signing step. Use the DirectML.dll from that folder (redistributable),
  not the older one in System32.
- **Linux CUDA**: static core + `libonnxruntime_providers_shared.so`
  and `libonnxruntime_providers_cuda.so` (copied/symlinked into
  `target/<profile>/`) next to the binary; CUDA 13 and cuDNN 9 runtime
  libraries on the host. Without a GPU, the CPU fallback is automatic.
- Alternative for all platforms: `ORT_LIB_PATH` + `ORT_PREFER_DYNAMIC_LINK=1`
  to link a Microsoft release dylib instead (then ship `libonnxruntime.dylib`
  / `onnxruntime.dll` in the bundle).
- **Models**: either Tauri `resources` (~223 MB, read-only: checksums are
  then re-hashed at each launch, ~0.15–0.6 s) or first-launch download with
  the same manifest into the app data dir (writable: verification cached).
  `ModelSet::load` takes the directory; the manifest is compiled in.
- Rust ≥ 1.92 (required by `ort` rc.15).

## Tests and tools

- `cargo test -p cull-ml`: unit tests (protobuf patch, NMS, sampling,
  tokenizer against OpenAI reference ids, aesthetic weights, manifest,
  checksum errors) and integration tests (`tests/models.rs`): degenerate
  images (0x0, 1x1, 3x2000, noise), concurrency = sequential results,
  missing/corrupt models, faces-only degradation, and on real photos
  (portraits open, closed eyes, landscapes without faces, burst vs unrelated
  similarity, text search ranking). Tests needing models or samples print
  `SKIP` and pass when those are absent (`PELLI_CULL_MODELS`,
  `PELLI_CULL_SAMPLES` override the folders).
- `cargo run --release -p cull-ml --example validate -- MODE …` with modes
  `faces`, `json`, `emb`, `similar`, `search "query"`, `bench THREADS
  ROUNDS`, `scale IOD,…` (eye state vs face size), `resize`.

## Known limitations

- Eye state: a laugh with the eyes squeezed shut reads open (by design: it
  is an expression, not a blink); eyes behind sunglasses are guessed by the
  mesh (one false "closed" on Strict in the calibration shoots); very small
  or low-quality faces may still read closed. Without the blendshapes file
  the EAR alone decides, and smiles narrowing the eyes read closed more
  often. The culling only flags closed eyes on group photos (and portraits
  on Strict).
- Faces below ~1 % of the photo's long side (crowds) are not reliably found
  (and never get an eye state); small low-contrast faces (vintage scans,
  veils) can be missed.
- Animal faces are sometimes detected as faces (no eye state for them).
- English-only text search (CLIP); the caller translates French queries.
- Embeddings from the int8 (CPU) and fp16 (GPU) towers are close but not
  identical; `models_id` tells them apart.
