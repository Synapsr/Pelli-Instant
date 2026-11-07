//! Groups of similar shots and scenes.
//!
//! # Groups
//!
//! 1. Exact copies (identical pHash *and* dHash, flat frames excepted, and
//!    the same focus score and mean luma within 0.02 / 0.01 — a blurred or
//!    re-exposed frame of a static scene keeps the hashes but is not a copy)
//!    are collapsed onto one representative (the earliest timed copy).
//! 2. Two photos are **linkable** when they are
//!    * at most `group_window_s` apart (when both have a capture time),
//!    * from the same camera: different serial numbers never link; when a
//!      serial is missing, different make/model never link,
//!    * visually similar: when both photos have an embedding, cosine ≥
//!      `similarity_threshold` **and** the same layout (dHash Hamming distance
//!      ≤ `LAYOUT_DHASH_MAX`), or cosine ≥ `STRONG_SIMILARITY` alone;
//!      otherwise pHash Hamming distance ≤ 10;
//!    * in the **same pose** when both have a main face (the subject, see
//!      `score`): the face at the same place and size in the frame (centre
//!      within `POSE_SHIFT` face widths, sizes within `POSE_SCALE`), the
//!      head turned, nodded and tilted alike (`POSE_YAW` / `POSE_PITCH` /
//!      `POSE_ROLL` degrees), the mouth opened alike (`POSE_MOUTH`), and the
//!      person around the face alike (body hash, anchored on the face,
//!      within `POSE_BODY` bits: a hand moved to the hair or the pockets);
//!      a main face of at least 0.5% of the frame on one photo and none on
//!      the other (the head turned away, hair across the face) is another
//!      pose too.
//!
//!    The layout and pose tests come from real shoots: CLIP sees "the same
//!    person in the same place" as similar (≈0.90-0.93) across different
//!    poses, outfits or backgrounds, and the frame's dHash only sees the
//!    background when the subject fills 10-20% of the frame: a 4-minute
//!    portrait session became one group of 50 "duplicates", a series of
//!    expressions one group of 17.
//! 3. Groups are built by **complete linkage**: photos are walked in
//!    capture-time order (then id) and a photo joins the most recent group
//!    whose members it is linkable to — *all* of them —, otherwise starts a
//!    new one. (Single linkage chained A–B–C into one group when A and C
//!    had nothing in common.) A group never spans more than
//!    `MAX_GROUP_SPAN_MS` (one minute): a group is a moment, never a whole
//!    sequence of poses.
//! 4. Photos without capture time (exported without EXIF…) are walked in the
//!    order given — the engine passes them in natural file-name order, which
//!    is shooting order — and may join a group holding one of their previous
//!    `UNTIMED_NEIGHBOURS` photos, with the same tests plus a layout check
//!    (pHash distance ≤ `UNTIMED_PHASH_MAX`: without the time window, the
//!    embedding alone can match two different shots of the same kind of
//!    subject). A group of them never spans more than `MAX_UNTIMED_SPAN`
//!    positions.
//! 5. Copies rejoin their representative's group. A set of copies whose
//!    representative stayed alone becomes an `Exact` group.
//! 6. Kind: `Exact` when every member is a copy of the same image, `Burst`
//!    when every member has a capture time and consecutive members are at
//!    most `BURST_GAP_MS` apart (one moment: only bursts and copies can
//!    reject a frame as a duplicate), otherwise `Similar`. Members are
//!    ordered best first (score, then time, then id); groups are ordered by
//!    their first capture time (groups without time last), then by input
//!    position, and numbered from 1.
//!
//! When `settings.group_similar` is off only `Exact` groups are returned.
//! When `settings.detect_exact_duplicates` is off there is no `Exact` group:
//! a set of copies is a `Burst` or `Similar` group like any near-identical
//! shots (none at all with grouping off too).

use crate::hash::{exact_copy_sets, hamming, CopyKey};
use crate::score::{main_face, mesh_ran, rank_cmp, ranking_scores};
use crate::{EmbeddingSpace, PhotoFacts};
use cull_types::{CullSettings, FaceObservation, GroupKind, Scene, SimilarGroup};
use std::collections::HashMap;

/// pHash Hamming distance at or below which two shots are similar.
pub(crate) const PHASH_SIMILAR_MAX: u32 = 10;
/// A group never spans more than this (one moment, not a session).
pub(crate) const MAX_GROUP_SPAN_MS: i64 = 60 * 1000;
/// dHash Hamming distance at or below which two frames share a layout.
pub(crate) const LAYOUT_DHASH_MAX: u32 = 14;
/// Embedding cosine at or above which two frames are near-copies whatever
/// the layout hash says (slight reframing, zoom).
pub(crate) const STRONG_SIMILARITY: f32 = 0.965;
/// DINOv2 cosines (Maximum precision) spread more than CLIP's: duplicates
/// and different scenes are 0.25 apart instead of 0.11. The user threshold
/// `t` (calibrated on CLIP B/32, 0.80 loose … 0.97 near-identical) maps to
/// `DINO_AT_090 + DINO_SLOPE × (t − 0.90)`, and the near-copy threshold to
/// `DINO_STRONG`: the DINOv2 cosines at the same quantiles of the candidate
/// pairs of the four calibration shoots (3 288 pairs within the time window
/// or among the 8 neighbours of untimed photos; CLIP 0.85 → 0.64, 0.90 →
/// 0.76, 0.94 → 0.87, 0.965 → 0.948, 0.97 → 0.96).
pub(crate) const DINO_AT_090: f32 = 0.76;
pub(crate) const DINO_SLOPE: f32 = 2.6;
pub(crate) const DINO_STRONG: f32 = 0.948;

/// `(similar, near-copy)` cosine thresholds of an embedding space for the
/// user's similarity threshold.
pub(crate) fn cosine_thresholds(space: EmbeddingSpace, user: f32) -> (f32, f32) {
    match space {
        EmbeddingSpace::ClipB32 => (user, STRONG_SIMILARITY.max(user)),
        EmbeddingSpace::DinoV2 => {
            let t = (DINO_AT_090 + DINO_SLOPE * (user - 0.90)).clamp(0.0, 0.995);
            (t, DINO_STRONG.max(t))
        }
    }
}
/// Consecutive members at most this far apart make a `Burst`.
pub(crate) const BURST_GAP_MS: i64 = 2_000;
/// Same pose: main-face centre shift (face widths), size ratio, head angles
/// (degrees), mouth opening, body hash distance (bits of 64).
pub(crate) const POSE_SHIFT: f32 = 0.8;
pub(crate) const POSE_SCALE: f32 = 1.3;
pub(crate) const POSE_YAW: f32 = 12.0;
pub(crate) const POSE_PITCH: f32 = 12.0;
pub(crate) const POSE_ROLL: f32 = 15.0;
pub(crate) const POSE_MOUTH: f32 = 0.15;
pub(crate) const POSE_BODY: u32 = 20;
/// A main face at least this large (share of the frame) on one photo and
/// none on the other: the subject turned away or hid the face, another pose.
pub(crate) const POSE_FACE_GONE_AREA: f32 = 0.005;
/// Upper bound of earlier photos compared with each photo (pathological
/// folders where thousands of files share one timestamp).
const MAX_LOOKBACK: usize = 256;
/// Photos without capture time: neighbours compared (in input order), and
/// the largest distance in input positions inside one group.
pub(crate) const UNTIMED_NEIGHBOURS: usize = 8;
pub(crate) const MAX_UNTIMED_SPAN: usize = 24;
/// Without capture time, the embedding alone can match two different shots
/// of the same kind of subject (two lion profiles): the layout must match
/// too. Different photos are ≥ 26 bits apart (5th percentile), a 4% reframe
/// of the same shot ≤ 12-16.
pub(crate) const UNTIMED_PHASH_MAX: u32 = 16;

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Same camera body as far as we can tell.
fn same_camera(a: &PhotoFacts<'_>, b: &PhotoFacts<'_>) -> bool {
    fn clean(s: &Option<String>) -> Option<&str> {
        s.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }
    if let (Some(x), Some(y)) = (clean(&a.exif.camera_serial), clean(&b.exif.camera_serial)) {
        return x == y;
    }
    let model = |p: &PhotoFacts<'_>| {
        Some((clean(&p.exif.camera_make)?.to_ascii_lowercase(), clean(&p.exif.camera_model)?.to_ascii_lowercase()))
    };
    match (model(a), model(b)) {
        (Some(x), Some(y)) => x == y,
        _ => true,
    }
}

/// Two main faces in the same pose (see the module docs). Each signal only
/// counts when both faces have it.
fn same_pose(a: &FaceObservation, b: &FaceObservation) -> bool {
    let centre = |f: &FaceObservation| (f.bbox.x + f.bbox.w / 2.0, f.bbox.y + f.bbox.h / 2.0);
    let ((ax, ay), (bx, by)) = (centre(a), centre(b));
    let width = a.bbox.w.max(b.bbox.w).max(1e-4);
    if ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt() / width > POSE_SHIFT {
        return false;
    }
    let (sa, sb) = (a.area_ratio.sqrt(), b.area_ratio.sqrt());
    if sa.max(sb) > POSE_SCALE * sa.min(sb) {
        return false;
    }
    if let (Some(p), Some(q)) = (a.pose, b.pose) {
        let far = |x: f32, y: f32, limit: f32| (x - y).abs() > limit;
        if far(p.yaw, q.yaw, POSE_YAW)
            || far(p.pitch, q.pitch, POSE_PITCH)
            || far(p.roll, q.roll, POSE_ROLL)
            || far(p.mouth_open, q.mouth_open, POSE_MOUTH)
        {
            return false;
        }
    }
    match (a.body_hash, b.body_hash) {
        (Some(x), Some(y)) => hamming(x, y) <= POSE_BODY,
        _ => true,
    }
}

/// Complete-linkage clusters under construction.
struct Clusters {
    members: Vec<Vec<usize>>,
    of: Vec<Option<usize>>,
}

impl Clusters {
    fn add(&mut self, i: usize, to: Option<usize>) {
        match to {
            Some(c) => self.members[c].push(i),
            None => {
                self.members.push(vec![i]);
                self.of[i] = Some(self.members.len() - 1);
                return;
            }
        }
        self.of[i] = to;
    }
}

pub(crate) fn group_similar(photos: &[PhotoFacts<'_>], settings: &CullSettings) -> Vec<SimilarGroup> {
    let n = photos.len();
    if n < 2 {
        return Vec::new();
    }
    let phash: Vec<u64> = photos.iter().map(|p| p.metrics.hashes.phash).collect();
    let dhash: Vec<u64> = photos.iter().map(|p| p.metrics.hashes.dhash).collect();
    let time = |i: usize| photos[i].exif.captured_at_ms;
    let order_key = |i: usize| (time(i).is_none(), time(i).unwrap_or(0), photos[i].id);

    // 1. Exact copies → representative.
    let keys: Vec<CopyKey> = photos
        .iter()
        .map(|p| CopyKey {
            phash: p.metrics.hashes.phash,
            dhash: p.metrics.hashes.dhash,
            focus: p.metrics.sharpness.score,
            luma: p.metrics.exposure.mean_luma,
        })
        .collect();
    let mut rep: Vec<usize> = (0..n).collect();
    let mut copies: Vec<Vec<usize>> = Vec::new();
    for mut set in exact_copy_sets(&keys) {
        set.sort_by_key(|&i| order_key(i));
        for &i in &set {
            rep[i] = set[0];
        }
        copies.push(set);
    }

    // 2-4. Complete linkage over representatives.
    let mut cl = Clusters { members: Vec::new(), of: vec![None; n] };
    if settings.group_similar {
        let window_ms = (settings.group_window_s.max(0.0) as f64 * 1000.0).min(1e15) as i64;
        let norms: Vec<f32> = photos.iter().map(|p| p.embedding.map(norm).unwrap_or(0.0)).collect();
        let mesh = mesh_ran(photos);
        let mains: Vec<Option<&FaceObservation>> = photos.iter().map(|p| main_face(p.faces, mesh)).collect();
        let similar = |a: usize, b: usize| -> bool {
            let (pa, pb) = (&photos[a], &photos[b]);
            match (pa.embedding, pb.embedding) {
                (Some(x), Some(y))
                    if pa.embedding_space == pb.embedding_space
                        && x.len() == y.len()
                        && !x.is_empty()
                        && norms[a] > 0.0
                        && norms[b] > 0.0 =>
                {
                    let (cos_min, strong) = cosine_thresholds(pa.embedding_space, settings.similarity_threshold);
                    let dot: f32 = x.iter().zip(y).map(|(p, q)| p * q).sum();
                    let cos = dot / (norms[a] * norms[b]);
                    cos.is_finite()
                        && ((cos >= cos_min && hamming(dhash[a], dhash[b]) <= LAYOUT_DHASH_MAX) || cos >= strong)
                }
                _ => hamming(phash[a], phash[b]) <= PHASH_SIMILAR_MAX,
            }
        };
        let linkable = |a: usize, b: usize| -> bool {
            if let (Some(ta), Some(tb)) = (time(a), time(b)) {
                if ta.abs_diff(tb) > window_ms as u64 {
                    return false;
                }
            } else if hamming(phash[a], phash[b]) > UNTIMED_PHASH_MAX {
                return false;
            }
            let pose = match (mains[a], mains[b]) {
                (Some(x), Some(y)) => same_pose(x, y),
                (Some(f), None) | (None, Some(f)) => f.area_ratio < POSE_FACE_GONE_AREA,
                (None, None) => true,
            };
            pose && same_camera(&photos[a], &photos[b]) && similar(a, b)
        };
        // Time span (timed members) and input-position span (untimed ones)
        // of a cluster with `j` added.
        let spans_ok = |members: &[usize], j: usize| -> bool {
            let ts = members.iter().chain(std::iter::once(&j)).filter_map(|&i| time(i));
            let (lo, hi) = ts.fold((i64::MAX, i64::MIN), |(lo, hi), t| (lo.min(t), hi.max(t)));
            let untimed = members.iter().chain(std::iter::once(&j)).filter(|&&i| time(i).is_none());
            let (plo, phi) = untimed.fold((usize::MAX, 0), |(lo, hi), &i| (lo.min(i), hi.max(i)));
            (lo > hi || hi.saturating_sub(lo) <= MAX_GROUP_SPAN_MS) && (plo > phi || phi - plo <= MAX_UNTIMED_SPAN)
        };
        // Join the first candidate cluster (most recent first) whose every
        // member is linkable to `j`.
        let join = |cl: &mut Clusters, j: usize, candidates: Vec<usize>| {
            let target = candidates
                .into_iter()
                .find(|&c| spans_ok(&cl.members[c], j) && cl.members[c].iter().all(|&m| linkable(m, j)));
            cl.add(j, target);
        };

        let mut timed: Vec<usize> = (0..n).filter(|&i| rep[i] == i && time(i).is_some()).collect();
        timed.sort_by_key(|&i| order_key(i));
        for j in 0..timed.len() {
            let tj = time(timed[j]).unwrap_or(0);
            let mut candidates: Vec<usize> = Vec::new();
            for k in (j.saturating_sub(MAX_LOOKBACK)..j).rev() {
                let i = timed[k];
                if tj.saturating_sub(time(i).unwrap_or(0)) > window_ms {
                    break;
                }
                if let Some(c) = cl.of[i] {
                    if !candidates.contains(&c) {
                        candidates.push(c);
                    }
                }
            }
            join(&mut cl, timed[j], candidates);
        }

        // Photos without capture time: groups of their previous neighbours.
        for i in (0..n).filter(|&i| rep[i] == i && time(i).is_none()) {
            let mut candidates: Vec<usize> = Vec::new();
            for k in (i.saturating_sub(UNTIMED_NEIGHBOURS)..i).rev() {
                if let Some(c) = cl.of[rep[k]] {
                    if !candidates.contains(&c) {
                        candidates.push(c);
                    }
                }
            }
            join(&mut cl, i, candidates);
        }
    }

    // 5. Collect members (copies follow their representative).
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &r) in rep.iter().enumerate() {
        // A representative not clustered (grouping off) is its own root.
        let root = cl.of[r].map_or(n + r, |c| c);
        by_root.entry(root).or_default().push(i);
    }
    let copy_of: HashMap<usize, usize> =
        copies.iter().enumerate().flat_map(|(c, set)| set.iter().map(move |&i| (i, c))).collect();

    let scores = ranking_scores(photos, settings);
    let mut groups: Vec<(GroupKind, Vec<usize>)> = Vec::new();
    for (_, mut members) in by_root {
        if members.len() < 2 {
            continue;
        }
        // Copies of one image are an `Exact` group only while their detection
        // is on; otherwise they are ordinary similar shots.
        let one_image = settings.detect_exact_duplicates && {
            let c = copy_of.get(&members[0]);
            c.is_some() && members.iter().all(|i| copy_of.get(i) == c)
        };
        if !settings.group_similar && !one_image {
            continue;
        }
        let kind = if one_image {
            GroupKind::Exact
        } else {
            let mut ts: Vec<i64> = members.iter().filter_map(|&i| time(i)).collect();
            ts.sort_unstable();
            if ts.len() == members.len() && ts.windows(2).all(|w| w[1] - w[0] <= BURST_GAP_MS) {
                GroupKind::Burst
            } else {
                GroupKind::Similar
            }
        };
        members.sort_by(|&a, &b| rank_cmp((scores[a], time(a), photos[a].id), (scores[b], time(b), photos[b].id)));
        groups.push((kind, members));
    }
    groups.sort_by_key(|(_, m)| {
        let first = m.iter().filter_map(|&i| time(i)).min();
        let min_pos = m.iter().copied().min().unwrap_or(0);
        (first.is_none(), first.unwrap_or(0), min_pos)
    });
    groups
        .into_iter()
        .enumerate()
        .map(|(k, (kind, members))| SimilarGroup {
            id: k as u32 + 1,
            kind,
            photo_ids: members.iter().map(|&i| photos[i].id.to_string()).collect(),
        })
        .collect()
}

pub(crate) fn split_scenes(photos: &[PhotoFacts<'_>], settings: &CullSettings) -> Vec<Scene> {
    let mut timed: Vec<(i64, &str)> = photos.iter().filter_map(|p| p.exif.captured_at_ms.map(|t| (t, p.id))).collect();
    timed.sort_unstable();
    let gap_ms = (settings.scene_gap_s.max(0.0) as f64 * 1000.0).min(1e15) as i64;
    let mut scenes: Vec<Scene> = Vec::new();
    for (t, id) in timed {
        match scenes.last_mut() {
            Some(s) if t.saturating_sub(s.end_ms.unwrap_or(t)) <= gap_ms => {
                s.end_ms = Some(t);
                s.photo_ids.push(id.to_string());
            }
            _ => scenes.push(Scene {
                id: scenes.len() as u32 + 1,
                start_ms: Some(t),
                end_ms: Some(t),
                photo_ids: vec![id.to_string()],
            }),
        }
    }
    let mut untimed: Vec<&str> = photos.iter().filter(|p| p.exif.captured_at_ms.is_none()).map(|p| p.id).collect();
    if !untimed.is_empty() {
        untimed.sort_unstable();
        scenes.push(Scene {
            id: scenes.len() as u32 + 1,
            start_ms: None,
            end_ms: None,
            photo_ids: untimed.into_iter().map(str::to_string).collect(),
        });
    }
    scenes
}
