# cull-analysis

How Pelli Instant judges a photo: focus, motion blur, exposure, duplicates,
and the Keep / Maybe / Reject decision. Everything here is classic image
analysis (no neural network): fast, deterministic and explainable. The face,
eye and aesthetic models live in `cull-ml`. This crate only uses what they
measured.

The first part is written for photographers (it is the source of the in-app
help). The second part lists the exact rules and constants for developers.

---

## For photographers

### Focus: "is it sharp?"

Pelli Instant looks at the **edges** in your photo (the border of an eyelid, a
strand of hair, the edge of a glass) and measures **how wide they are**. A
sharp edge goes from dark to light within a pixel or two. A blurred edge
spreads over many pixels. This is measured on a 1024-pixel version of every
photo, so all your cameras are judged the same way.

Because it measures edge *width* and not contrast, a soft-light portrait with
gentle contrast is not mistaken for a blurry one, and a blurred street full
of neon signs is not mistaken for a sharp one.

* **Shallow depth of field is fine.** The photo is cut into ~40 tiles and
  only the two sharpest count: a photo is "blurry" only when **nothing** in
  it is sharp. A sharp face on creamy bokeh, a product detail whose sharp
  zone is a few percent of the frame, a ring at f/2 are sharp.
* **High ISO is fine.** Grain is measured on every photo and taken out of the
  measurement, so noise neither hides blur nor fakes sharpness.
* **Nothing to measure is not blur.** A foggy sea, a plain sky or a white wall
  have almost no edges. They get a neutral score, never "blurry".
* **The eyes come first.** When there is a face big enough, its focus is
  measured on its **eyes** (lids, lashes, iris), the sharper eye deciding,
  always at the same scale: as if the eyes were 64 pixels apart, whatever
  the camera, the lens and the framing. A smile, a tilted head, soft cheeks
  or hair at f/1.4 no longer make a sharp face look soft. Small faces (a
  group, a figure far away) are read on the full-size image (the RAW's
  full-size JPEG, or the JPEG itself). A sharp main face means the photo is
  in focus: the rest of the frame (bokeh, hair, a textured wall) can no
  longer make it "blurry" or "motion-blurred".
* **Subject out of focus** flags a soft main face while the frame is sharp
  elsewhere, and never when another face of the photo is sharp (a blurred
  figure in the foreground, the subject sharp behind it), nor when the head
  is bent over the work, turned away or strongly tilted. A soft face smaller
  than about 1% of the frame only sends the photo to review.
* **A face is a face.** The face detector sometimes sees a face in a
  bicycle part or a hand. Only a confident detection that the face model
  also recognises counts as the subject.

### Motion blur: "did the camera move?"

Camera shake and moving subjects smear edges in **one direction**: edges
across the movement get wide, edges along it stay sharp. Pelli Instant compares
the edges of each direction with the perpendicular direction, across the
whole frame and tile by tile. The direction must be consistent across the
picture. If some parts of the photo are sharp in every direction, the camera
did not move. In that case the effect comes from oriented texture (long hair,
fur, fabric, wood) and is ignored.

### Exposure: "how many stops off?"

Pelli Instant estimates **how many stops a photo is off**, not how far it is
from an average grey. Dark and bright photos made on purpose stay fine:

* **Low key** (dark background, lit face, a stage in a spotlight, a black
  lens on black velvet): judged on its highlights. If the brightest parts are
  well exposed and not blown out, the dark rest is a choice.
* **High key** (a white dress on a white background): judged on how much of
  the picture is blown out *and* how bright its midtones are.
* **Portraits are judged on the face.** When there is a face, its skin
  decides: a backlit portrait on a white sky, a lit face in a dark workshop
  are well exposed. A face blown to white, or a face lost in the shade, go to
  review even when the rest of the frame is fine (they are rejected only
  when the whole frame is badly exposed too). The frame still counts a little (a
  whole frame far too dark is still found), but never more than a stop or so
  beyond what the face says.
* A photo **3 stops under** has even its brightest details dull, and is
  flagged **Underexposed**. A photo **3 stops over** has half the picture at
  pure white, and is flagged **Overexposed**. ±1 stop is never flagged.
  ±2 stops is usually "to review": it is recoverable from RAW.

### Duplicates and bursts

* **Similar shots**: photos taken within 45 s of each other (configurable)
  by the same camera body that look alike are grouped. "Look alike" means
  the AI image fingerprint is close **and** the framing matches (same
  layout), or the fingerprint is nearly identical; without the AI models, a
  perceptual hash that survives resizing, recompression and small exposure
  changes. **And the pose must match**: the face at the same place and size,
  the head turned and tilted alike, the mouth opened alike (a smile, a
  laugh), the arms and hands where they were. Every photo of a group must
  look like every other one, not just like its neighbour, so a session of
  poses is no longer chained into one long group. A group is one moment: it
  never spans more than one minute. Photos without a capture date (exported
  without EXIF) are compared with their 8 neighbours in file-name order.
  Their framing must match too, so two different photos of a similar
  subject are not mixed. Two cameras shooting the same moment are never
  mixed.
* In each group, the **best shot** (or the best *N*, your choice) is kept.
  * In a **burst** (frames at most 2 s apart: one moment), the others are
    rejected as **Duplicate**, or left to review when they are almost as
    good.
  * In a **series** of similar shots taken further apart (the same set-up,
    other expressions), nothing is rejected: at least one shot in five is
    kept and the others wait in **to review**, stacked under the best one.
    Choosing the expression is your call.
* Groups are always **stacked** in the review, the best shot on top. With
  the similar photos check off, nothing is set aside as **Duplicate**: each
  shot of a group is judged on its own.
* **Exact duplicates** (the same picture copied or re-exported twice) are
  found whatever their date.

### Closed eyes

The eye model gives each eye a probability of being open. Eyes narrowed by a
smile or a laugh read open. When the head faces the camera the less open eye
counts (a laugh closes both); when it is turned or looks down, the other one.

* On a **group photo** (3 faces or more), a good share of closed eyes is
  flagged **Eyes closed**: "to review" on Balanced and Lenient.
* On a **portrait**, only **Strict** flags (and rejects) closed eyes. On
  Balanced and Lenient a single photo is never flagged for its eyes: a kiss,
  a prayer, a look down at the bouquet, a laugh or the sun often closes them
  on purpose, and the eye model cannot tell.
* In a burst, the frame with open eyes simply wins; the others carry **Eyes
  closed** next to **Duplicate** so you see why.

### Score and verdicts

Every photo gets a **score from 0 to 100**. It mixes focus (50%), exposure
(20%), eyes (15%), uniqueness (5%) and the aesthetic score of the AI made
relative to *your* shoot (10%: a generic "pretty picture" model prefers
landscapes to product details and calm faces to laughter, so it only breaks
ties). A technical failure cannot be compensated by a nice composition: a
very blurry or very dark photo is capped. Closed or squinting eyes are not a
technical failure: they lower the score a little, nothing more.

* **Reject**: a clear technical problem (Blurry, Motion blur, Subject out of
  focus, Under/Overexposed, Eyes closed on a Strict portrait), a worse shot
  of a burst (Duplicate), or a copy (Exact duplicate).
* **Maybe** ("to review"): a mild problem (slightly soft, a bit dark, closed
  eyes in a group photo), a burst shot almost as good as the kept one, the
  other shots of a series of poses.
* **Keep**: everything else. A composition the AI finds weak is never
  flagged on its own (on delivered shoots it flagged product details,
  changing rooms and laughs).

All thresholds are **relative to your shoot** as well as absolute: a dim
reception shot entirely at ISO 12 800 with a soft vintage lens is not
mass-rejected, but the one frame far blurrier than its siblings is.

### Sensitivity

* **Strict**: for event coverage with thousands of frames. Rejects soft and
  slightly dark photos and closed eyes.
* **Balanced** (default): rejects clear defects; mild ones go to review.
  Keeps a little under half of a typical wedding.
* **Lenient**: for artistic work and intentional blur. Rejects only obvious
  failures (very blurry, nearly black, nearly white).

Moving the slider never re-analyses the photos. Only the decision changes,
instantly.

### Choosing what is checked

Each thing Pelli Instant flags has its own switch, grouped as you look at a
photo: **sharpness** (blurry photos, motion blur, subject out of focus),
**eyes** (closed eyes), **exposure** (underexposure, overexposure) and
**similar photos** (keeping the best of each burst, with the number of
photos kept per group, exact duplicates). A check you turn off never flags a
photo, and no longer weighs on its score. Turning off motion blur also means
a frame softened by motion is not called "blurry" instead: shoot creative
motion blur, turn it off. Turning off similar photos keeps every shot of a
burst, still stacked. Turning off exact duplicates leaves copies to the
similar photos (one kept, the others to review) or, with them off too, keeps
every copy.

The defaults for new shoots are in **Settings › Analysis**. Before a shoot is
analysed for the first time, Pelli Instant shows its criteria: sort with one
click, or turn a check on or off for this shoot (and keep the change for the
next ones if you like). In the review, the **Sensitivity** menu changes them
for the shoot at any time: the decisions are recomputed instantly, your own
picks and rejects never move.

### Respect artistic blur

For bokeh, a shallow depth of field, a blurred foreground and blur used on
purpose. Pelli Instant judges a frame on its sharpest areas, but when the
sharp area is tiny (a ring, an eye at f/1.4) the photo can still read as
soft, and the AI cannot tell a focus you placed elsewhere on purpose from a
missed one. So this option stays on the safe side and changes three things:

* a photo is **rejected** for blur or motion blur only when **nothing in it
  is sharp** (or the camera shake is strong);
* a clearly soft photo goes to **to review** instead of being rejected, and
  slight softness is no longer flagged;
* **subject out of focus** is not checked (its switch is inactive while the
  option is on).

The scores and the ranking inside bursts do not change. Limits: a missed
focus with a sharp background is no longer flagged, and a photo you made soft
on purpose everywhere (dreamy effect, intentional camera movement) is still
rejected when nothing in it is sharp: for such work, turn the blur checks off.

### Lightroom, Capture One, Bridge, darktable

Pelli Instant writes your stars, colour label and its own verdict into the
**XMP sidecar** next to each RAW (`IMG_0001.xmp`, or `IMG_0001.CR3.xmp` when
that already exists). Every other setting in the file is kept exactly as it
was: develop settings, keywords, history. Lightroom has no XMP field for its
Pick/Reject flags, so a photo you rejected without giving it stars gets
**−1 stars** (shown as "Rejected" by Bridge and Lightroom's rating filters).
In Lightroom Classic, use *Metadata › Read Metadata from Files* to pick
changes up.

---

## For developers

Public API: `src/lib.rs` (contract with `cull-engine`). Each module documents its algorithm in detail:

| module         | what                                                                       |
|----------------|----------------------------------------------------------------------------|
| `prep`         | luma (Rec. 709), exact area downscaling to 1024 px                          |
| `sharpness`    | edge width, tiles, noise model, motion blur, region focus                  |
| `exposure`     | stops-off estimate, low-key / high-key handling                            |
| `hash`         | 64-bit DCT pHash and dHash, exact-copy detection                           |
| `group`        | groups of similar shots, scenes                                            |
| `score`        | sub-scores, overall score, defect rules, verdicts (all constants in a table) |
| `xmp`          | sidecar update by byte splicing (quick-xml tokeniser), atomic write        |

### Measures

* **Focus score** `score = 1 / (1 + (s / 1.0)^4)` where `s` is the edge
  width (Gaussian σ, px at 1024 px) of the 2 sharpest tiles (geometric mean
  of their median widths; the confidence counts the edges of the sharpest
  15%). `s` comes
  from the ratio of gradients before/after a known re-blur (binomial 9,
  σ0 = √2) at edge maxima, inverted through a table that simulates the exact
  operators. Noise: measured per operator on the flattest quarter of the
  image. Raw `global` / `peak`: noise-corrected variance of the Laplacian
  (frame, 90th percentile of tiles).
* **Region focus** (`region_sharpness`): sharpest 20% of the region's edges,
  floored by its blurrier direction (10th percentile per orientation, ≥ 40
  edges each), native resolution, crops < 32 px enlarged, > 640 px
  downscaled; a crop < 128 px with < 25 edges is measured again with the
  noise of a 3× context (its own flattest quarter was skin, not noise).
* **Eye focus** (`eye_sharpness`, stored as the face's `sharpness`): the
  bounding box of both eye boxes (1.0 × 0.55 inter-ocular distances each),
  area-resampled to 64 px between the eyes (never enlarged), region focus
  (sharpest 20% of its edges) with the noise of a 2.4 × 1.6 IOD context;
  fewer than 12 edges → the context itself. The engine reads faces whose
  IOD is under 64 px at 1440 px on the detail image when it decoded one.
* **Main face**: the largest face with detector confidence ≥ 0.8 **and**
  face-mesh presence ≥ 0.5 (confidence alone when the shoot has no mesh
  data), at least 0.15% of the frame. When it covers ≥ 0.5% and its focus is
  ≥ 0.75 (≥ 0.9 under frame motion at the blocking level; 0.75-0.9 caps the
  frame defects at Maybe), frame-level Blurry / MotionBlur are not raised.
* **Subject out of focus**: not raised when another subject face (≥ 0.15%)
  is ≥ 0.75, nor when `|yaw| > 50°`, `|pitch| > 25°` or `|roll| > 35°`;
  blocks only from 1.2% of the frame.
* **Face exposure**: skin luma = 65th percentile of the face core (60% × 60%
  of the box, centred low), blown share ≥ 250. Fine between 0.30 and 0.80;
  stops beyond, plus 3 stops per unit of blown skin over 10%; the frame's
  stops count up to +1 (over) / +1.5 (under) beyond the skin's. Used for the
  main face when it covers ≥ 0.3% and the mesh presence is ≥ 0.9.
* **Motion blur**: perpendicular-orientation width ratio, frame-wide and per
  tile (consistent direction required), vetoed when ≥ 2 tiles (≥ 15%) are
  sharp in both directions; maps ratio 1.35 → 0, 2.2 → 1.
* **Groups**: complete linkage in time order; linkable = time window + same
  body + (embedding cosine ≥ threshold and dHash ≤ 14, or cosine ≥ 0.965)
  (else pHash ≤ 10) + same pose when both have a main face (centre shift
  ≤ 0.8 face width, size ratio ≤ 1.3, |Δyaw| ≤ 12°, |Δpitch| ≤ 12°,
  |Δroll| ≤ 15°, |Δmouth| ≤ 0.15, body hash ≤ 20 bits; a main face ≥ 0.5%
  on one photo and none on the other is another pose); ≤ 60 s per group;
  photos without time: groups holding one of their 8 previous neighbours in
  input order, same tests plus pHash ≤ 16, ≤ 24 positions per group.
  `Burst` (consecutive gaps ≤ 2 s) rejects Duplicates; `Similar` sends them
  to Maybe. Groups are formed whatever `group_similar` says; off, their
  members are judged as singles (uniqueness 1).
* **Body hash**: dHash of a 27 × 36 grey canvas of the person around the
  main face (±2 face widths, 0.4 above to 3.5 below the face box), the part
  outside the photo filled with the crop's mean.
* **Closed eyes**: openness = min of both eyes, max when `|yaw| ≥ 25°` or
  `pitch ≥ 15°`; each eye's probability comes from cull-ml (eye aspect
  ratio combined with the blendshapes' blink, squint and smile).
  Portraits: Strict only. Group photos (≥ 3 comparable faces): every
  sensitivity, Maybe except on Strict. Only faces ≥ 0.3% of the frame.
* **Score**: focus 0.50, exposure 0.20, eyes 0.15, uniqueness 0.05, the
  aesthetic weighted to 10% of the mean; the lock `0.6 + 0.8 × weakest`
  below 0.5 takes focus and exposure only. `LowAesthetic` is never raised.
* **Subject region** (Maximum precision, photos without a main face): the
  largest person or animal instance (RF-DETR mask, ≥ 1% of the frame).
  `subject_measures`: focus of its bounding box (region focus) and
  `ev_over` of its masked pixels (same formula as the frame). Its focus
  stands in for the main face's (a sharp subject ≥ 0.75 overrules
  frame-level `Blurry` / `MotionBlur`, `0.6 × subject + 0.4 × frame` in the
  focus sub-score; it never raises `SubjectOutOfFocus`); the frame's
  overexposure counts at most +0.5 stop beyond the subject's own
  (`exposure::with_subject`), its underexposure is unchanged.
* **Embedding spaces**: CLIP ViT-B/32 (Standard) and DINOv2 (Maximum)
  cosines are not comparable; photos of different spaces never link on
  their embeddings (pHash instead). DINOv2 thresholds map the user's
  similarity threshold `t` (CLIP scale) to `0.76 + 2.6 × (t − 0.90)` and
  the near-copy threshold 0.965 to 0.948: the same quantiles of the
  candidate pairs of the calibration shoots (3 288 pairs).
* **Exposure**: `ev_under` from the 98th percentile (or, when the top 0.1%
  is unclipped, from it), plus 0.1-0.3 × the extra darkness of the median;
  `ev_over` from the median beyond +0.5 stop over mid-grey plus 2.5 stops per
  unit of blown share beyond 12%; `score = 1/(1+exp((max(ev) − 2.0)/0.35))`.
* **Hashes**: pHash = 32×32 DCT, coefficients 1..=8 × 1..=8 vs their median;
  dHash = 9×8 horizontal gradient. Exact copies = same pHash and dHash and
  the same focus score (±0.02) and mean luma (±0.01).

### Calibration (`examples/calibrate.rs`)

```sh
scripts/fetch-calibration-photos.sh     # 39 CC0 / CC BY / CC BY-SA photos into samples/photos (gitignored)
cargo run --release -p cull-analysis --example calibrate [dir] [--verbose]
```

Prints per photo and per generated variant (Gaussian blur, motion blur,
noise, ±EV, crops, recompression): focus, edge width, motion, exposure,
verdicts on the three sensitivities. It also prints the separation (AUC) of
sharp vs blurred photos for the edge-width score against the classic
energies, the pHash robustness, face focus on hand-placed faces, and timings.

### Performance (release, one core)

`compute_metrics` ≈ 12 ms for a 3000×2000 or 1620×1080 preview (CPU time,
measured on a loaded shared machine); `region_sharpness` < 1 ms per face;
`assess` ≈ 1.5 ms and `group_similar` ≈ 10 ms for 5 000 photos with 512-d
embeddings.

### Known limits

* Without a face, a studio portrait on a backdrop blown to pure white over
  more than half of the frame (or a person seen from the back against the
  light) reads like an overexposed frame: the histogram cannot tell. The
  shoot-relative thresholds protect a whole high-key session.
* Skin tone and exposure cannot be told apart from luma alone: a fair face
  3 stops under reads like a deep skin tone 1 stop under, so such a frame
  goes to review instead of being rejected.
* Ghosting (flash + slow shutter double image) keeps crisp edges: when the
  face of such a frame measures sharp, motion blur only sends it to review
  (sample `event-speech`).
* The eye focus is read on the analysis preview (1440-2400 px) for faces
  large in it: a focus missed by a few centimetres at f/1.4, visible at
  100% on a 36 MP file, is not always visible there.
* A blink on a single portrait is not flagged on Balanced: closed eyes on
  purpose (laughs, poses) were most of the alerts on real sessions.
* Faces smaller than ~1.2% of the frame never reject on their focus.
* Without capture times, groups rely on the file order given by the engine,
  and are never bursts (no frame is rejected as a duplicate).
