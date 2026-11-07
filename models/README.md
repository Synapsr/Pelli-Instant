# Pelli Instant models

This folder holds the ONNX models used by `crates/cull-ml`. Only
`manifest.json` and this README are committed; the model files are
downloaded (and verified) by:

```sh
node scripts/fetch-models.mjs            # desktop bundle set (~223 MB)
node scripts/fetch-models.mjs --gpu      # + fp16 CLIP for DirectML/CUDA (+176 MB; Windows installer)
node scripts/fetch-models.mjs --maximum  # + the Maximum pack into models/maximum/ (~777 MB, development)
node scripts/fetch-models.mjs --check    # verify sizes + SHA-256, exit 1 on problems
node scripts/fetch-models.mjs --strict   # an optional model that cannot be fetched is an error (installers)
```

The script needs Node ≥ 20 and no packages, is idempotent (files already
present with the right checksum are skipped), honours `HTTPS_PROXY`, tries
mirrors, and extracts archive members (Face Mesh ships inside a `.tar.gz`).
CI should run it before bundling.

`manifest.json` is the single source of truth: per model its `id`, the
`tag` written into each analysis (`models_id`), file name, download URL
(pinned revisions on Hugging Face), SHA-256, size, license + license URL,
source, purpose, `required` and `bundle` (`default` / `gpu` / `maximum`). It
is compiled into cull-ml, which re-verifies every file at load (verification
cached in `.verified.json` by size + mtime), and into the app, which
downloads the `maximum` bundle itself (Range resume, SHA-256, see
`crates/cull-engine/src/modelpack.rs`).

The three files converted by Pelli (blendshapes, RF-DETR, RetinaFace) have
no upstream ONNX: they are hosted by Pelli at
`https://pelli.s3.fr-par.scw.cloud/pelli-instant/models/<file>`, next to a
`README.txt` with their sources, licences and SHA-256. A hosted file never
changes: a new conversion gets a new file name. A local copy of the files
can be served to a debug build with `PELLI_MODELS_BASE_URL` (any static
server with HTTP range support).

| id | file | size | license |
|---|---|---|---|
| yunet (required) | face_detection_yunet_2023mar.onnx | 0.23 MB | MIT (OpenCV Zoo) |
| face-mesh | face_landmarks_detector_Nx3x256x256.onnx | 5.0 MB | Apache-2.0 (Google MediaPipe; ONNX by PINTO_model_zoo) |
| clip-vision | clip-vit-b32-vision-int8.onnx | 88.6 MB | MIT (OpenAI CLIP; ONNX by Xenova) |
| face-blendshapes | face_blendshapes_v2.onnx | 1.9 MB | Apache-2.0 (Google MediaPipe; ONNX by Pelli) |
| clip-vision-fp16 (gpu) | clip-vit-b32-vision-fp16.onnx | 176 MB | MIT |
| clip-text | clip-vit-b32-text-fp16.onnx | 127 MB | MIT |

Maximum pack (`bundle: "maximum"`, every file needed, ~777 MB):

| id | file | size | license |
|---|---|---|---|
| clip-l14-vision-int8 | clip-vit-l14-vision-int8.onnx | 306 MB | MIT (OpenAI CLIP; ONNX + int8 by Xenova) |
| clip-l14-text | clip-vit-l14-text-fp16.onnx | 248 MB | MIT |
| dinov2-b14-int8 | dinov2-base-int8.onnx | 91 MB | Apache-2.0 (Meta DINOv2; ONNX by onnx-community) |
| rfdetr-seg-s | rfdetr-seg-small-384-w16.onnx | 77 MB | Apache-2.0 (Roboflow RF-DETR; ONNX by Pelli) |
| retinaface-r50 | retinaface-r50-w16.onnx | 55 MB | MIT (biubug6 RetinaFace; ONNX by Pelli) |

Plus, compiled into the crate: the LAION aesthetic linear heads (V1 for
ViT-B/32, MIT; V2 for ViT-L/14 collapsed into one linear map, Apache-2.0;
`crates/cull-ml/assets/`), the Maximum pack's reference outputs
(`maximum-check.json`) and the CLIP BPE vocabulary (MIT, in
`instant-clip-tokenizer`).

Distribution obligations: include the MIT notices (OpenCV Zoo/Shiqi Yu,
OpenAI, LAION AI, instant-labs, biubug6, Microsoft ONNX Runtime, pyke
`ort`) and the Apache-2.0 license text (MediaPipe Face Mesh and
Blendshapes, LAION V2, DINOv2, RF-DETR) in the app's third-party notices. Never add weights under non-commercial or GPL/AGPL terms here (e.g.
InsightFace "buffalo" models). See `crates/cull-ml/README.md` for model
choices, accuracy and packaging.
