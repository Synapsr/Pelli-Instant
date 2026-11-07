//! Shoot-level decisions: groups, scenes, scores and verdicts.

mod common;

use common::Rng;
use cull_analysis::{assess, group_similar, split_scenes, EmbeddingSpace, PhotoFacts};
use cull_types::{
    Assessment, CullSettings, ExifInfo, FaceObservation, GroupKind, HeadPose, NormPoint, NormRect, Reason, Sensitivity,
    SimilarGroup, SubjectKind, SubjectObservation, TechnicalMetrics, Verdict,
};
use std::time::Instant;

#[derive(Clone)]
struct P {
    id: String,
    exif: ExifInfo,
    metrics: TechnicalMetrics,
    faces: Vec<FaceObservation>,
    embedding: Option<Vec<f32>>,
    space: EmbeddingSpace,
    aesthetic: Option<f32>,
    subject: Option<SubjectObservation>,
}

impl P {
    fn new(id: &str, t_s: Option<f64>, focus: f32, phash: u64) -> P {
        let mut metrics = TechnicalMetrics::default();
        metrics.sharpness.score = focus;
        metrics.exposure.score = 0.95;
        metrics.exposure.mean_luma = 0.45;
        metrics.hashes.phash = phash;
        metrics.hashes.dhash = phash.rotate_left(17) ^ 0x5555;
        P {
            id: id.to_string(),
            exif: ExifInfo { captured_at_ms: t_s.map(|t| (t * 1000.0) as i64), ..Default::default() },
            metrics,
            faces: Vec::new(),
            embedding: None,
            space: EmbeddingSpace::ClipB32,
            aesthetic: None,
            subject: None,
        }
    }

    fn facts(&self) -> PhotoFacts<'_> {
        PhotoFacts {
            id: &self.id,
            exif: &self.exif,
            metrics: &self.metrics,
            faces: &self.faces,
            embedding: self.embedding.as_deref(),
            embedding_space: self.space,
            aesthetic: self.aesthetic,
            subject: self.subject.as_ref(),
        }
    }

    fn serial(mut self, s: &str) -> P {
        self.exif.camera_serial = Some(s.to_string());
        self
    }

    fn face(mut self, area: f32, sharpness: f32, eyes_open: f32) -> P {
        self.faces.push(face(area, sharpness, eyes_open));
        self
    }
}

/// A face the mesh recognised, looking at the camera.
fn face(area: f32, sharpness: f32, eyes_open: f32) -> FaceObservation {
    let side = area.sqrt();
    FaceObservation {
        bbox: NormRect { x: 0.4, y: 0.3, w: side, h: side },
        confidence: 0.9,
        landmarks: [NormPoint::default(); 5],
        right_eye_open: Some(eyes_open),
        left_eye_open: Some(eyes_open),
        sharpness: Some(sharpness),
        area_ratio: area,
        mesh_presence: Some(0.99),
        pose: Some(HeadPose::default()),
        expression: None,
        skin_luma: None,
        skin_clipped: None,
        body_hash: None,
    }
}

fn facts(ps: &[P]) -> Vec<PhotoFacts<'_>> {
    ps.iter().map(P::facts).collect()
}

fn settings(s: Sensitivity) -> CullSettings {
    CullSettings { sensitivity: s, ..Default::default() }
}

fn run(ps: &[P], s: Sensitivity) -> (Vec<SimilarGroup>, Vec<Assessment>) {
    let f = facts(ps);
    let st = settings(s);
    let g = group_similar(&f, &st);
    let a = assess(&f, &g, &st);
    (g, a)
}

/// Unrelated scenes, each with its own random hash.
fn filler(n: usize, start_s: f64, seed: u64) -> Vec<P> {
    let mut rng = Rng::new(seed);
    (0..n)
        .map(|i| {
            P::new(&format!("f{seed}-{i:04}"), Some(start_s + 60.0 * i as f64), rng.range(0.85, 1.0), rng.next_u64())
        })
        .collect()
}

#[test]
fn burst_keeps_the_best_and_ranks_best_first() {
    let base = 0xA5A5_F0F0_1234_5678u64;
    let mut ps = vec![
        P::new("b1", Some(100.0), 0.70, base),
        P::new("b2", Some(100.3), 0.97, base ^ 0b1),
        P::new("b3", Some(100.6), 0.80, base ^ 0b110),
        P::new("b4", Some(100.9), 0.05, base ^ 0b1000), // blurry
        P::new("b5", Some(101.2), 0.75, base ^ 0b10000),
    ];
    ps.extend(filler(30, 1000.0, 1));
    let (groups, a) = run(&ps, Sensitivity::Balanced);
    assert_eq!(groups.len(), 1, "{groups:?}");
    let g = &groups[0];
    assert_eq!((g.id, g.kind), (1, GroupKind::Burst));
    assert_eq!(g.photo_ids[0], "b2");
    assert_eq!(g.photo_ids.last().map(String::as_str), Some("b4"));
    assert_eq!(a[1].verdict, Verdict::Keep);
    assert_eq!((a[1].group_id, a[1].group_rank), (Some(1), 0));
    for i in [0, 2, 4] {
        assert_ne!(a[i].verdict, Verdict::Keep);
        assert!(a[i].reasons.contains(&Reason::Duplicate), "{:?}", a[i]);
    }
    assert_eq!(a[3].verdict, Verdict::Reject);
    assert!(a[3].reasons.contains(&Reason::Blurry));
    assert!(a[0].score < a[1].score && a[3].score < a[0].score);
    // Uniqueness: burst members vs singles.
    assert!(a[1].breakdown.uniqueness < 0.5 && a[10].breakdown.uniqueness == 1.0);
}

#[test]
fn close_second_is_maybe_and_keep_per_group_is_honoured() {
    let base = 0x0F0F_0F0F_AAAA_5555u64;
    let mut ps = vec![
        P::new("a", Some(10.0), 0.96, base),
        P::new("b", Some(10.5), 0.955, base ^ 1),
        P::new("c", Some(11.0), 0.70, base ^ 2),
    ];
    ps.extend(filler(20, 500.0, 2));
    let (_, a) = run(&ps, Sensitivity::Balanced);
    assert_eq!(a[0].verdict, Verdict::Keep);
    assert_eq!(a[1].verdict, Verdict::Maybe, "{:?}", a[1]);
    assert_eq!(a[2].verdict, Verdict::Reject);
    let f = facts(&ps);
    let st = CullSettings { keep_per_group: 2, ..Default::default() };
    let g = group_similar(&f, &st);
    let a = assess(&f, &g, &st);
    assert_eq!((a[0].verdict, a[1].verdict), (Verdict::Keep, Verdict::Keep));
    assert_ne!(a[2].verdict, Verdict::Keep);
}

#[test]
fn grouping_respects_cameras_windows_and_max_span() {
    let base = 0x1234_5678_9ABC_DEF0u64;
    // Two bodies shooting the same moment: never grouped together.
    let ps = vec![
        P::new("a1", Some(0.0), 0.9, base).serial("A"),
        P::new("b1", Some(0.2), 0.9, base ^ 1).serial("B"),
        P::new("a2", Some(0.5), 0.9, base ^ 2).serial("A"),
        P::new("b2", Some(0.7), 0.9, base ^ 3).serial("B"),
    ];
    let g = group_similar(&facts(&ps), &CullSettings::default());
    assert_eq!(g.len(), 2);
    for grp in &g {
        assert!(
            grp.photo_ids.iter().all(|id| id.starts_with('a')) || grp.photo_ids.iter().all(|id| id.starts_with('b'))
        );
    }
    // Similar shots further apart than the window do not group.
    let ps = vec![P::new("x", Some(0.0), 0.9, base), P::new("y", Some(100.0), 0.9, base ^ 1)];
    assert!(group_similar(&facts(&ps), &CullSettings::default()).is_empty());
    // A chain of similar portraits every 30 s for 12 minutes: groups ≤ 5 min.
    // (Focus differs, so shots sharing a hash are not exact copies.)
    let ps: Vec<P> = (0..25)
        .map(|i| P::new(&format!("c{i:02}"), Some(30.0 * i as f64), 0.5 + 0.015 * i as f32, base ^ (i as u64 & 3)))
        .collect();
    let g = group_similar(&facts(&ps), &CullSettings::default());
    assert!(g.len() >= 3, "{}", g.len());
    for grp in &g {
        let t: Vec<i64> = grp
            .photo_ids
            .iter()
            .map(|id| ps.iter().find(|p| &p.id == id).unwrap().exif.captured_at_ms.unwrap())
            .collect();
        assert!(t.iter().max().unwrap() - t.iter().min().unwrap() <= 5 * 60 * 1000);
        assert_eq!(grp.kind, GroupKind::Similar);
    }
    let ids: Vec<u32> = g.iter().map(|g| g.id).collect();
    assert_eq!(ids, (1..=g.len() as u32).collect::<Vec<_>>(), "ids from 1, in time order");
    // Different pHash (distance > 10) within the window: not similar.
    let ps = vec![P::new("p", Some(0.0), 0.9, 0), P::new("q", Some(1.0), 0.9, 0xFFF)];
    assert!(group_similar(&facts(&ps), &CullSettings::default()).is_empty());
}

#[test]
fn embeddings_decide_similarity_when_present() {
    let unit = |v: [f32; 4]| {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / n).collect::<Vec<f32>>()
    };
    let mut a = P::new("a", Some(0.0), 0.9, 1);
    let mut b = P::new("b", Some(2.0), 0.9, u64::MAX); // hashes far apart
    let mut c = P::new("c", Some(4.0), 0.5, 1); // same hash as a (not a copy: other focus)
    a.embedding = Some(unit([1.0, 0.1, 0.0, 0.0]));
    b.embedding = Some(unit([1.0, 0.12, 0.02, 0.0])); // cos ≈ 0.999
    c.embedding = Some(unit([0.0, 0.0, 1.0, 0.0])); // cos 0
    let g = group_similar(&facts(&[a, b, c]), &CullSettings::default());
    assert_eq!(g.len(), 1);
    let mut ids = g[0].photo_ids.clone();
    ids.sort();
    assert_eq!(ids, vec!["a", "b"]);
}

#[test]
fn exact_copies_group_whatever_the_time_and_untimed_photos() {
    let h = 0xDEAD_BEEF_0BAD_F00Du64;
    let mut ps =
        vec![P::new("orig", Some(0.0), 0.9, h), P::new("copy", Some(7200.0), 0.9, h), P::new("nodate", None, 0.9, h)];
    // Same hashes but a different focus: a blurred frame of a static scene is not a copy.
    ps.push(P::new("blurred-same-scene", Some(3600.0), 0.3, h));
    ps.extend(filler(10, 100.0, 3));
    let (g, a) = run(&ps, Sensitivity::Balanced);
    let exact: Vec<&SimilarGroup> = g.iter().filter(|g| g.kind == GroupKind::Exact).collect();
    assert_eq!(exact.len(), 1, "{g:?}");
    assert_eq!(exact[0].photo_ids.len(), 3);
    assert!(!exact[0].photo_ids.contains(&"blurred-same-scene".to_string()));
    let copies = a[..3].iter().filter(|x| x.reasons.contains(&Reason::ExactDuplicate)).count();
    assert_eq!(copies, 2);
    assert_eq!(a[..3].iter().filter(|x| x.verdict == Verdict::Keep).count(), 1);
    assert!(!a[3].reasons.contains(&Reason::ExactDuplicate));
    // Grouping off: the same groups, and copies are still rejected.
    let st = CullSettings { group_similar: false, ..Default::default() };
    assert_eq!(group_similar(&facts(&ps), &st), g);
    let a = assess(&facts(&ps), &g, &st);
    assert_eq!(a[..3].iter().filter(|x| x.reasons.contains(&Reason::ExactDuplicate)).count(), 2);
}

#[test]
fn grouping_off_stacks_bursts_without_rejecting() {
    let base = 0xA5A5_F0F0_1234_5678u64;
    let mut ps = vec![
        P::new("b1", Some(100.0), 0.70, base),
        P::new("b2", Some(100.3), 0.97, base ^ 0b1),
        P::new("b3", Some(100.6), 0.80, base ^ 0b110),
        P::new("b4", Some(100.9), 0.05, base ^ 0b1000), // blurry
    ];
    ps.extend(filler(20, 1000.0, 5));
    let f = facts(&ps);
    let on = CullSettings::default();
    let off = CullSettings { group_similar: false, ..Default::default() };
    // The same burst either way: the review stacks it, best first.
    let g = group_similar(&f, &off);
    assert_eq!(g, group_similar(&f, &on));
    assert_eq!((g.len(), g[0].kind, g[0].photo_ids[0].as_str()), (1, GroupKind::Burst, "b2"));
    let a = assess(&f, &g, &off);
    assert!(a[..4].iter().all(|x| x.group_id == Some(1)), "{a:?}");
    assert_eq!(a[1].group_rank, 0);
    // Off: every photo is judged as a single, none is a duplicate.
    let singles = assess(&f, &[], &off);
    for (x, single) in a.iter().zip(&singles) {
        assert_eq!((x.verdict, &x.reasons, x.score), (single.verdict, &single.reasons, single.score));
    }
    assert!(a.iter().all(|x| !x.reasons.contains(&Reason::Duplicate)));
    assert_eq!((a[2].verdict, a[3].verdict), (Verdict::Keep, Verdict::Reject));
    // On: the frames after the best one are duplicates.
    let a = assess(&f, &g, &on);
    assert!([0, 2].iter().all(|&i| a[i].reasons.contains(&Reason::Duplicate)), "{a:?}");
}

#[test]
fn scenes_split_on_gaps_untimed_last() {
    let ps = vec![
        P::new("a", Some(0.0), 0.9, 1),
        P::new("b", Some(600.0), 0.9, 2),
        P::new("c", Some(600.0 + 16.0 * 60.0), 0.9, 3),
        P::new("z", None, 0.9, 4),
        P::new("d", Some(600.0 + 20.0 * 60.0), 0.9, 5),
    ];
    let s = split_scenes(&facts(&ps), &CullSettings::default());
    assert_eq!(s.len(), 3);
    assert_eq!(s[0].photo_ids, vec!["a", "b"]);
    assert_eq!((s[0].id, s[0].start_ms, s[0].end_ms), (1, Some(0), Some(600_000)));
    assert_eq!(s[1].photo_ids, vec!["c", "d"]);
    assert_eq!((s[2].id, s[2].start_ms), (3, None));
    assert_eq!(s[2].photo_ids, vec!["z"]);
    assert!(split_scenes(&[], &CullSettings::default()).is_empty());
}

fn verdict_of(p: P, s: Sensitivity) -> Assessment {
    let mut ps = filler(40, 10_000.0, 9);
    ps.insert(0, p);
    run(&ps, s).1.remove(0)
}

#[test]
fn blur_thresholds_follow_sensitivity() {
    let obvious = P::new("x", Some(0.0), 0.03, 7);
    for s in [Sensitivity::Strict, Sensitivity::Balanced, Sensitivity::Lenient] {
        let a = verdict_of(obvious.clone(), s);
        assert_eq!(a.verdict, Verdict::Reject, "{s:?}");
        assert_eq!(a.reasons, vec![Reason::Blurry]);
    }
    let soft = P::new("x", Some(0.0), 0.25, 7);
    assert_eq!(verdict_of(soft.clone(), Sensitivity::Strict).verdict, Verdict::Reject);
    assert_eq!(verdict_of(soft.clone(), Sensitivity::Balanced).verdict, Verdict::Reject);
    assert_ne!(verdict_of(soft, Sensitivity::Lenient).verdict, Verdict::Reject, "Lenient rejects only obvious blur");
    let mild = P::new("x", Some(0.0), 0.6, 7);
    assert_eq!(verdict_of(mild.clone(), Sensitivity::Strict).verdict, Verdict::Maybe);
    assert_eq!(verdict_of(mild, Sensitivity::Lenient).verdict, Verdict::Keep);
}

#[test]
fn focus_is_judged_against_the_shoot() {
    // A dim reception shot with a soft lens: every frame around 0.35.
    let mut rng = Rng::new(4);
    let mut ps: Vec<P> = (0..60)
        .map(|i| P::new(&format!("s{i}"), Some(60.0 * i as f64), rng.range(0.32, 0.45), rng.next_u64()))
        .collect();
    ps.push(P::new("really-blurry", Some(99_999.0), 0.08, rng.next_u64()));
    let (_, a) = run(&ps, Sensitivity::Balanced);
    let rejected = a[..60].iter().filter(|x| x.verdict == Verdict::Reject).count();
    assert!(rejected <= 6, "a soft-looking shoot is not mass-rejected ({rejected})");
    assert_eq!(a[60].verdict, Verdict::Reject);
}

#[test]
fn motion_blur_and_exposure_defects() {
    let mut p = P::new("m", Some(0.0), 0.8, 7);
    p.metrics.sharpness.motion_blur = 0.9;
    let a = verdict_of(p, Sensitivity::Balanced);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Reject, vec![Reason::MotionBlur]));

    let mut dark = P::new("d", Some(0.0), 0.9, 7);
    dark.metrics.exposure.score = 0.05;
    dark.metrics.exposure.mean_luma = 0.04;
    let a = verdict_of(dark, Sensitivity::Balanced);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Reject, vec![Reason::Underexposed]));

    let mut bright = P::new("b", Some(0.0), 0.9, 7);
    bright.metrics.exposure.score = 0.15;
    bright.metrics.exposure.mean_luma = 0.9;
    assert_eq!(verdict_of(bright.clone(), Sensitivity::Balanced).reasons, vec![Reason::Overexposed]);
    assert_ne!(verdict_of(bright, Sensitivity::Lenient).verdict, Verdict::Reject);
}

#[test]
fn closed_eyes_and_subject_focus() {
    // Portrait, main face eyes closed: only Strict flags (and rejects) a
    // unique moment; a laugh, a kiss, a prayer close the eyes on purpose.
    let blink = P::new("e", Some(0.0), 0.95, 7).face(0.04, 0.9, 0.05);
    let a = verdict_of(blink.clone(), Sensitivity::Strict);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Reject, vec![Reason::EyesClosed]));
    assert!(a.breakdown.eyes.unwrap() < 0.1);
    for s in [Sensitivity::Balanced, Sensitivity::Lenient] {
        let a = verdict_of(blink.clone(), s);
        assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Keep, vec![]), "{s:?}");
    }

    // Group photo: 1 of 8 faces blinking is soft on Balanced, 3 of 8 block.
    let mut group = P::new("g", Some(0.0), 0.95, 7);
    for k in 0..8 {
        group.faces.push(face(0.004, 0.8, if k == 0 { 0.05 } else { 0.95 }));
    }
    assert_eq!(verdict_of(group.clone(), Sensitivity::Balanced).verdict, Verdict::Keep);
    assert_eq!(verdict_of(group.clone(), Sensitivity::Strict).verdict, Verdict::Maybe);
    group.faces[1].right_eye_open = Some(0.1);
    group.faces[1].left_eye_open = Some(0.1);
    assert_eq!(verdict_of(group.clone(), Sensitivity::Strict).verdict, Verdict::Reject);
    assert_eq!(verdict_of(group.clone(), Sensitivity::Balanced).verdict, Verdict::Maybe);
    group.faces[2].right_eye_open = Some(0.1);
    group.faces[2].left_eye_open = Some(0.1);
    let a = verdict_of(group, Sensitivity::Balanced);
    assert_eq!((a.verdict, a.reasons), (Verdict::Maybe, vec![Reason::EyesClosed]));

    // Background sharp, face soft.
    let missed = P::new("f", Some(0.0), 0.95, 7).face(0.03, 0.08, 0.9);
    let a = verdict_of(missed, Sensitivity::Balanced);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Reject, vec![Reason::SubjectOutOfFocus]));
    // Tiny faces never reject on their focus.
    let small = P::new("s", Some(0.0), 0.95, 7).face(0.002, 0.05, 0.9);
    assert_ne!(verdict_of(small, Sensitivity::Balanced).verdict, Verdict::Reject);
    // Eye detection disabled.
    let blink = P::new("e", Some(0.0), 0.95, 7).face(0.04, 0.9, 0.05);
    let mut ps = filler(10, 1000.0, 5);
    ps.insert(0, blink);
    let f = facts(&ps);
    let st = CullSettings { detect_eyes_closed: false, sensitivity: Sensitivity::Strict, ..Default::default() };
    assert_eq!(assess(&f, &[], &st)[0].verdict, Verdict::Keep);
}

#[test]
fn eyes_are_read_on_the_less_open_eye_unless_the_head_turns_or_looks_down() {
    // A laugh: one eye misread as open must not hide the closed one.
    let mut laugh = face(0.04, 0.9, 0.95);
    laugh.right_eye_open = Some(0.05);
    let a = verdict_of(P::new("l", Some(0.0), 0.95, 7).face(0.04, 0.9, 0.95), Sensitivity::Strict);
    assert_eq!(a.verdict, Verdict::Keep);
    let mut p = P::new("l", Some(0.0), 0.95, 7);
    p.faces.push(laugh.clone());
    let a = verdict_of(p, Sensitivity::Strict);
    assert_eq!(a.reasons, vec![Reason::EyesClosed]);
    assert!(a.breakdown.eyes.unwrap() < 0.1);
    // Head turned 40° or looking down: the far eye / lowered lids read
    // closed, the other eye decides.
    for pose in [HeadPose { yaw: 40.0, ..Default::default() }, HeadPose { pitch: 20.0, ..Default::default() }] {
        let mut turned = laugh.clone();
        turned.pose = Some(pose);
        let mut p = P::new("t", Some(0.0), 0.95, 7);
        p.faces.push(turned);
        let a = verdict_of(p, Sensitivity::Strict);
        assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Keep, vec![]), "{pose:?}");
    }
    // The eye probabilities decide, not the raw blendshapes (cull-ml folds
    // blink, squint and smile into them): a laugh with blink 0.5 is open.
    let mut x = face(0.04, 0.9, 0.95);
    x.expression = Some(cull_types::FaceExpression {
        eye_blink_right: Some(0.5),
        eye_blink_left: Some(0.5),
        ..Default::default()
    });
    let mut p = P::new("x", Some(0.0), 0.95, 7);
    p.faces.push(x);
    assert_eq!(verdict_of(p, Sensitivity::Strict).reasons, vec![]);
}

#[test]
fn the_subject_is_a_confident_face_the_mesh_recognises() {
    // A bicycle part boxed at 0.75, or at 0.95 but not a face for the mesh:
    // not a subject, nothing judged on its "focus".
    for fake in [
        FaceObservation { confidence: 0.75, ..face(0.05, 0.05, 0.9) },
        FaceObservation { mesh_presence: Some(0.1), ..face(0.05, 0.05, 0.9) },
    ] {
        let mut p = P::new("bike", Some(0.0), 0.95, 7);
        p.faces.push(fake.clone());
        let a = verdict_of(p, Sensitivity::Balanced);
        assert_eq!((a.verdict, a.reasons.clone(), a.breakdown.eyes), (Verdict::Keep, vec![], None), "{fake:?}");
    }
    // Shoots analysed without the mesh: the confidence alone decides.
    let mut ps = filler(40, 10_000.0, 9);
    let mut p = P::new("old", Some(0.0), 0.95, 7);
    p.faces.push(FaceObservation { mesh_presence: None, ..face(0.05, 0.05, 0.9) });
    ps.insert(0, p);
    assert_eq!(run(&ps, Sensitivity::Balanced).1[0].reasons, vec![Reason::SubjectOutOfFocus]);
}

#[test]
fn no_subject_out_of_focus_when_another_face_is_sharp_or_the_head_is_away() {
    // Blurred figure in the foreground, the subject sharp behind it.
    let mut p = P::new("fg", Some(0.0), 0.95, 7).face(0.06, 0.1, 0.9);
    p.faces.push(FaceObservation { bbox: NormRect { x: 0.7, y: 0.2, w: 0.08, h: 0.08 }, ..face(0.006, 0.9, 0.9) });
    assert_eq!(verdict_of(p, Sensitivity::Balanced).verdict, Verdict::Keep);
    // Head bent over the work, turned in profile, strongly tilted.
    for pose in [
        HeadPose { pitch: 35.0, ..Default::default() },
        HeadPose { yaw: -70.0, ..Default::default() },
        HeadPose { roll: 50.0, ..Default::default() },
    ] {
        let mut p = P::new("bent", Some(0.0), 0.95, 7);
        p.faces.push(FaceObservation { pose: Some(pose), ..face(0.05, 0.1, 0.9) });
        assert_eq!(verdict_of(p, Sensitivity::Balanced).verdict, Verdict::Keep, "{pose:?}");
    }
    // A soft face of 1% of the frame (a figure behind the subject seen from
    // the back?) only goes to review; from 1.2% it is rejected.
    let a = verdict_of(P::new("mid", Some(0.0), 0.95, 7).face(0.01, 0.1, 0.9), Sensitivity::Balanced);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Maybe, vec![Reason::SubjectOutOfFocus]));
    let a = verdict_of(P::new("big", Some(0.0), 0.95, 7).face(0.02, 0.1, 0.9), Sensitivity::Balanced);
    assert_eq!(a.verdict, Verdict::Reject);
}

#[test]
fn portraits_are_exposed_on_the_face_skin() {
    // Backlit portrait on a blown sky: the frame reads 2.5 stops over, the
    // face is well exposed. Kept.
    let mut backlit = P::new("backlit", Some(0.0), 0.95, 7).face(0.03, 0.9, 0.95);
    backlit.metrics.exposure.score = 0.15;
    backlit.metrics.exposure.mean_luma = 0.8;
    assert_eq!(verdict_of(backlit.clone(), Sensitivity::Balanced).reasons, vec![Reason::Overexposed], "frame only");
    backlit.faces[0].skin_luma = Some(0.62);
    backlit.faces[0].skin_clipped = Some(0.0);
    let a = verdict_of(backlit.clone(), Sensitivity::Balanced);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Keep, vec![]));
    assert!(a.breakdown.exposure > 0.9);
    // Low-key: a dark workshop, a lit face.
    let mut lowkey = P::new("lowkey", Some(0.0), 0.95, 7).face(0.03, 0.9, 0.95);
    lowkey.metrics.exposure.score = 0.3;
    lowkey.metrics.exposure.mean_luma = 0.12;
    lowkey.faces[0].skin_luma = Some(0.5);
    assert_eq!(verdict_of(lowkey, Sensitivity::Balanced).verdict, Verdict::Keep);
    // Silhouette: a black face on a fine sky. To review: rejecting needs the
    // frame to agree.
    let mut dark = P::new("dark", Some(0.0), 0.95, 7).face(0.03, 0.9, 0.95);
    dark.faces[0].skin_luma = Some(0.04);
    let a = verdict_of(dark.clone(), Sensitivity::Balanced);
    assert_eq!((a.verdict, a.reasons), (Verdict::Maybe, vec![Reason::Underexposed]));
    let mut both = dark.clone();
    both.metrics.exposure.score = 0.05;
    both.metrics.exposure.mean_luma = 0.05;
    assert_eq!(verdict_of(both, Sensitivity::Balanced).verdict, Verdict::Reject);
    // Unless the mesh is unsure of the face (a camera held in front of it).
    dark.faces[0].mesh_presence = Some(0.6);
    assert_eq!(verdict_of(dark, Sensitivity::Balanced).verdict, Verdict::Keep);
}

#[test]
fn a_series_of_poses_never_rejects_only_a_burst_does() {
    // The same five frames, 0.5 s apart (one burst) or 8 s apart (a series of
    // poses of one set-up, grouped as similar shots).
    let base = 0x5A5A_1234_8765_4321u64;
    let frames = |gap: f64| -> Vec<P> {
        let mut ps: Vec<P> = [0.95f32, 0.70, 0.72, 0.68, 0.71]
            .iter()
            .enumerate()
            .map(|(k, &f)| P::new(&format!("s{k}"), Some(100.0 + gap * k as f64), f, base ^ (1 << k)))
            .collect();
        ps.extend(filler(30, 2000.0, 17));
        ps
    };
    let (g, a) = run(&frames(0.5), Sensitivity::Balanced);
    assert_eq!(g[0].kind, GroupKind::Burst);
    assert_eq!(a[0].verdict, Verdict::Keep);
    assert!(a[1..5].iter().all(|x| x.verdict == Verdict::Reject && x.reasons == vec![Reason::Duplicate]), "{a:?}");
    let (g, a) = run(&frames(8.0), Sensitivity::Balanced);
    assert_eq!(g[0].kind, GroupKind::Similar);
    assert_eq!(a[0].verdict, Verdict::Keep);
    assert!(a[1..5].iter().all(|x| x.verdict == Verdict::Maybe && x.reasons == vec![Reason::Duplicate]), "{a:?}");
    // Strict too: a series is the photographer's choice of expressions.
    let f = frames(8.0);
    let st = settings(Sensitivity::Strict);
    let a = assess(&facts(&f), &group_similar(&facts(&f), &st), &st);
    assert!(a[..5].iter().all(|x| x.verdict != Verdict::Reject));
}

#[test]
fn groups_are_complete_linkage_and_split_on_pose() {
    let unit = |v: [f32; 4]| {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / n).collect::<Vec<f32>>()
    };
    // A ~ B and B ~ C (cosine 0.95), but A and C differ (0.81): single
    // linkage chained them, complete linkage does not.
    let mut ps: Vec<P> = (0..3).map(|k| P::new(&format!("c{k}"), Some(k as f64), 0.9, 0xABCD ^ k as u64)).collect();
    ps[0].embedding = Some(unit([1.0, 0.0, 0.0, 0.0]));
    ps[1].embedding = Some(unit([1.0, 0.33, 0.0, 0.0]));
    ps[2].embedding = Some(unit([1.0, 0.72, 0.0, 0.0]));
    let g = group_similar(&facts(&ps), &CullSettings::default());
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].photo_ids.len(), 2, "{g:?}");
    // Same moment for CLIP and the hashes, but the subject moved her hands
    // (body hash), laughed (mouth), turned her head: no longer one group.
    let posed = |id: &str, t: f64, f: FaceObservation| {
        let k = (t * 10.0) as u64;
        let mut p = P::new(id, Some(t), 0.9 - 0.01 * k as f32, 0x7777 ^ (1 << k));
        p.embedding = Some(unit([1.0, 0.1, 0.0, 0.0]));
        p.faces.push(f);
        p
    };
    let still = FaceObservation { body_hash: Some(0), ..face(0.03, 0.9, 0.95) };
    let variants = [
        FaceObservation { body_hash: Some(u64::MAX >> 32), ..still.clone() },
        FaceObservation { pose: Some(HeadPose { mouth_open: 0.4, ..Default::default() }), ..still.clone() },
        FaceObservation { pose: Some(HeadPose { yaw: 30.0, ..Default::default() }), ..still.clone() },
        FaceObservation { bbox: NormRect { x: 0.1, ..still.bbox }, ..still.clone() },
    ];
    for v in variants {
        let ps = vec![posed("a", 0.0, still.clone()), posed("b", 0.4, still.clone()), posed("c", 0.8, v.clone())];
        let g = group_similar(&facts(&ps), &CullSettings::default());
        assert_eq!(g.len(), 1, "{v:?}");
        assert_eq!(g[0].photo_ids.len(), 2, "{v:?}: {g:?}");
    }
    // The head turned away (no main face any more): another pose.
    let away = P::new("away", Some(1.2), 0.9, 0x7777 ^ (1 << 20));
    let ps = vec![posed("a", 0.0, still.clone()), posed("b", 0.4, still.clone()), {
        let mut p = away;
        p.embedding = Some(unit([1.0, 0.1, 0.0, 0.0]));
        p
    }];
    assert_eq!(group_similar(&facts(&ps), &CullSettings::default())[0].photo_ids.len(), 2);
    // Small changes stay one burst.
    let near = FaceObservation {
        body_hash: Some(0b1011),
        pose: Some(HeadPose { yaw: 5.0, pitch: 4.0, roll: 3.0, mouth_open: 0.05 }),
        ..still.clone()
    };
    let ps = vec![posed("a", 0.0, still.clone()), posed("b", 0.4, near)];
    assert_eq!(group_similar(&facts(&ps), &CullSettings::default())[0].kind, GroupKind::Burst);
}

#[test]
fn blink_in_a_burst_loses_to_the_open_eyes_frame() {
    let base = 0x3C3C_3C3C_0F0F_0F0Fu64;
    let mut ps = vec![
        P::new("blink", Some(1.0), 0.95, base).face(0.05, 0.9, 0.05),
        P::new("open", Some(1.4), 0.93, base ^ 1).face(0.05, 0.88, 0.95),
    ];
    ps.extend(filler(20, 1000.0, 13));
    let (_, a) = run(&ps, Sensitivity::Balanced);
    assert_eq!(a[1].verdict, Verdict::Keep);
    assert_eq!(a[0].verdict, Verdict::Reject, "{:?}", a[0]);
    assert!(a[0].reasons.contains(&Reason::Duplicate) && a[0].reasons.contains(&Reason::EyesClosed));
}

#[test]
fn sharp_main_face_overrides_frame_level_blur_and_motion() {
    // Long hair / textured wall: frame-level motion high, face tack sharp.
    let mut hair = P::new("hair", Some(0.0), 0.52, 7).face(0.18, 0.93, 0.98);
    hair.metrics.sharpness.motion_blur = 0.7;
    for s in [Sensitivity::Strict, Sensitivity::Balanced, Sensitivity::Lenient] {
        let a = verdict_of(hair.clone(), s);
        assert!(!a.reasons.contains(&Reason::MotionBlur) && !a.reasons.contains(&Reason::Blurry), "{s:?}: {a:?}");
        assert_eq!(a.verdict, Verdict::Keep, "{s:?}");
    }
    // Same frame, face soft: the frame-level measures stand.
    let mut shaken = P::new("shaken", Some(0.0), 0.52, 7).face(0.18, 0.3, 0.98);
    shaken.metrics.sharpness.motion_blur = 0.7;
    assert!(verdict_of(shaken, Sensitivity::Balanced).reasons.contains(&Reason::MotionBlur));
    // A big blurred background head detected with low confidence is not the
    // main face: the confident, sharp subject is.
    let mut bg = P::new("bg", Some(0.0), 0.9, 7).face(0.05, 0.9, 0.95);
    bg.faces.push(FaceObservation { confidence: 0.6, ..face(0.12, 0.1, 0.9) });
    assert_eq!(verdict_of(bg, Sensitivity::Balanced).verdict, Verdict::Keep);
    // A small face does not override.
    let mut small = P::new("small", Some(0.0), 0.05, 7).face(0.003, 0.95, 0.98);
    small.metrics.sharpness.motion_blur = 0.0;
    assert_eq!(verdict_of(small, Sensitivity::Balanced).reasons, vec![Reason::Blurry]);
}

fn verdict_with(p: P, st: &CullSettings) -> Assessment {
    let mut ps = filler(40, 10_000.0, 9);
    ps.insert(0, p);
    let f = facts(&ps);
    let g = group_similar(&f, st);
    assess(&f, &g, st).remove(0)
}

#[test]
fn each_detection_can_be_turned_off_on_its_own() {
    let all = CullSettings::default();
    let off = |edit: fn(&mut CullSettings)| {
        let mut s = CullSettings::default();
        edit(&mut s);
        s
    };
    // Creative motion blur: with its detection off, the softness motion
    // caused is not called "blurry" instead.
    let mut shaken = P::new("m", Some(0.0), 0.25, 7);
    shaken.metrics.sharpness.motion_blur = 0.95;
    assert_eq!(verdict_with(shaken.clone(), &all).reasons, vec![Reason::MotionBlur]);
    let no_motion = off(|s| s.detect_motion_blur = false);
    assert_eq!(verdict_with(shaken, &no_motion).verdict, Verdict::Keep);
    // Missed focus still rejects without motion detection, not without its own.
    let blurry = P::new("b", Some(0.0), 0.05, 7);
    assert_eq!(verdict_with(blurry.clone(), &no_motion).reasons, vec![Reason::Blurry]);
    let no_blur = off(|s| s.detect_blur = false);
    let a = verdict_with(blurry, &no_blur);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Keep, vec![]), "a blurry frame is not a subject defect");
    // Subject out of focus.
    let missed = P::new("f", Some(0.0), 0.95, 7).face(0.03, 0.06, 0.9);
    assert_eq!(verdict_with(missed.clone(), &no_blur).reasons, vec![Reason::SubjectOutOfFocus]);
    assert_eq!(verdict_with(missed, &off(|s| s.detect_subject_focus = false)).verdict, Verdict::Keep);
    // Exposure, one direction at a time.
    let mut dark = P::new("d", Some(0.0), 0.9, 7);
    dark.metrics.exposure.score = 0.05;
    dark.metrics.exposure.mean_luma = 0.04;
    let mut bright = P::new("w", Some(0.0), 0.9, 7);
    bright.metrics.exposure.score = 0.15;
    bright.metrics.exposure.mean_luma = 0.9;
    let no_under = off(|s| s.detect_underexposed = false);
    let a = verdict_with(dark.clone(), &no_under);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Keep, vec![]));
    assert!(a.score > verdict_with(dark.clone(), &all).score, "a dark frame is not held against the score");
    assert_eq!(verdict_with(bright.clone(), &no_under).reasons, vec![Reason::Overexposed]);
    let no_over = off(|s| s.detect_overexposed = false);
    assert_eq!(verdict_with(bright, &no_over).verdict, Verdict::Keep);
    assert_eq!(verdict_with(dark, &no_over).reasons, vec![Reason::Underexposed]);
}

#[test]
fn artistic_blur_only_rejects_frames_with_nothing_sharp() {
    let artistic = CullSettings { artistic_blur: true, ..Default::default() };
    let balanced = CullSettings::default();
    // Small sharp subject on bokeh: the frame reads clearly soft. To review,
    // not rejected.
    let bokeh = P::new("dof", Some(0.0), 0.28, 7).face(0.02, 0.62, 0.95);
    assert_eq!(verdict_with(bokeh.clone(), &balanced).verdict, Verdict::Reject);
    let a = verdict_with(bokeh.clone(), &artistic);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Maybe, vec![Reason::Blurry]), "{a:?}");
    // The score does not change: the option only shapes the verdict.
    assert_eq!(a.score, verdict_with(bokeh, &balanced).score);
    // Mildly soft: not flagged at all.
    let mild = P::new("mild", Some(0.0), 0.45, 7);
    assert_eq!(verdict_with(mild.clone(), &balanced).reasons, vec![Reason::Blurry]);
    let a = verdict_with(mild, &artistic);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Keep, vec![]));
    // Nothing sharp at all: still rejected.
    let lost = P::new("lost", Some(0.0), 0.03, 7).face(0.03, 0.05, 0.9);
    assert_eq!(verdict_with(lost, &artistic).reasons, vec![Reason::Blurry]);
    // The face measure decides nothing: no "subject out of focus".
    let face = P::new("face", Some(0.0), 0.95, 7).face(0.03, 0.05, 0.9);
    assert_eq!(verdict_with(face.clone(), &balanced).reasons, vec![Reason::SubjectOutOfFocus]);
    let a = verdict_with(face, &artistic);
    assert_eq!((a.verdict, a.reasons.clone()), (Verdict::Keep, vec![]));
    // Motion blur follows the same rule.
    let mut pan = P::new("pan", Some(0.0), 0.8, 7);
    pan.metrics.sharpness.motion_blur = 0.7;
    assert_eq!(verdict_with(pan.clone(), &balanced).verdict, Verdict::Reject);
    assert_eq!(verdict_with(pan.clone(), &artistic).verdict, Verdict::Maybe);
    pan.metrics.sharpness.motion_blur = 0.5;
    assert_eq!(verdict_with(pan.clone(), &artistic).verdict, Verdict::Keep);
    pan.metrics.sharpness.motion_blur = 0.97;
    assert_eq!(verdict_with(pan, &artistic).reasons, vec![Reason::MotionBlur]);
    // A sharp main face overrides the frame as without the option.
    let mut hair = P::new("hair", Some(0.0), 0.52, 7).face(0.18, 0.93, 0.98);
    hair.metrics.sharpness.motion_blur = 0.95;
    assert_eq!(verdict_with(hair, &artistic).verdict, Verdict::Keep);
}

#[test]
fn exact_copies_can_be_left_to_the_photographer() {
    let h = 0xC0FF_EE00_1234_5678u64;
    let mut ps =
        vec![P::new("orig", Some(0.0), 0.9, h), P::new("copy", Some(7200.0), 0.9, h), P::new("nodate", None, 0.9, h)];
    ps.extend(filler(10, 100.0, 4));
    let f = facts(&ps);
    // Grouping on: copies are similar shots, the extra ones go to review.
    let st = CullSettings { detect_exact_duplicates: false, ..Default::default() };
    let g = group_similar(&f, &st);
    assert!(g.iter().all(|g| g.kind != GroupKind::Exact), "{g:?}");
    let a = assess(&f, &g, &st);
    assert!(a.iter().all(|x| !x.reasons.contains(&Reason::ExactDuplicate)));
    assert_eq!(a[..3].iter().filter(|x| x.verdict == Verdict::Keep).count(), 1);
    assert!(a[..3].iter().all(|x| x.verdict != Verdict::Reject), "{a:?}");
    // Grouping off too: every copy is kept, still stacked together.
    let st = CullSettings { detect_exact_duplicates: false, group_similar: false, ..Default::default() };
    let g = group_similar(&f, &st);
    assert_eq!(g.len(), 1, "{g:?}");
    let a = assess(&f, &g, &st);
    assert!(a[..3].iter().all(|x| x.verdict == Verdict::Keep && x.group_id == Some(g[0].id)), "{a:?}");
}

#[test]
fn photos_without_capture_time_group_with_their_neighbours() {
    let mut rng = Rng::new(21);
    let mut ps = Vec::new();
    // Pairs exported without EXIF, in file-name order: a, b near-identical.
    for k in 0..5 {
        let h = rng.next_u64();
        ps.push(P::new(&format!("{k}_a"), None, 0.9, h));
        ps.push(P::new(&format!("{k}_b"), None, 0.85, h ^ 0b101));
    }
    let g = group_similar(&facts(&ps), &CullSettings::default());
    assert_eq!(g.len(), 5, "{g:?}");
    for (k, grp) in g.iter().enumerate() {
        assert_eq!(grp.id, k as u32 + 1);
        assert_eq!(grp.kind, GroupKind::Similar);
        assert_eq!(grp.photo_ids, vec![format!("{k}_a"), format!("{k}_b")], "best first, file order");
    }
    let a = assess(&facts(&ps), &g, &CullSettings::default());
    assert_eq!(a.iter().filter(|x| x.verdict == Verdict::Keep).count(), 5);
    // Similar shots far apart in the file order do not group, nor do two
    // bodies, and a long run of similar files is cut.
    let h = rng.next_u64();
    let mut far = vec![P::new("x", None, 0.9, h)];
    far.extend((0..10).map(|i| P::new(&format!("f{i}"), None, 0.9, rng.next_u64())));
    far.push(P::new("y", None, 0.8, h ^ 1));
    assert!(group_similar(&facts(&far), &CullSettings::default()).is_empty());
    // Same kind of subject (embeddings alike), different layout: not grouped.
    let mut lion_a = P::new("lion_a", None, 0.9, h);
    let mut lion_b = P::new("lion_b", None, 0.8, !h);
    lion_a.embedding = Some(vec![0.6, 0.8, 0.0]);
    lion_b.embedding = Some(vec![0.62, 0.78, 0.05]);
    assert!(group_similar(&facts(&[lion_a.clone(), lion_b.clone()]), &CullSettings::default()).is_empty());
    lion_b.metrics.hashes.phash = h ^ 0b111;
    assert_eq!(group_similar(&facts(&[lion_a, lion_b]), &CullSettings::default()).len(), 1);
    let bodies = vec![P::new("a", None, 0.9, h).serial("A"), P::new("b", None, 0.8, h ^ 1).serial("B")];
    assert!(group_similar(&facts(&bodies), &CullSettings::default()).is_empty());
    let run: Vec<P> =
        (0..60).map(|i| P::new(&format!("r{i:02}"), None, 0.3 + 0.01 * i as f32, h ^ (i as u64 & 7))).collect();
    let g = group_similar(&facts(&run), &CullSettings::default());
    assert!(g.len() >= 3, "{}", g.len());
    for grp in &g {
        let pos: Vec<usize> = grp.photo_ids.iter().map(|id| id[1..].parse().unwrap()).collect();
        assert!(pos.iter().max().unwrap() - pos.iter().min().unwrap() <= 24);
    }
}

#[test]
fn aesthetic_is_relative_to_the_shoot_and_only_ranks() {
    let mut rng = Rng::new(6);
    let mut ps: Vec<P> = (0..80)
        .map(|i| {
            let mut p = P::new(&format!("p{i}"), Some(60.0 * i as f64), 0.95, rng.next_u64());
            p.aesthetic = Some(6.5 + 0.4 * rng.normal()); // a great shoot
            p
        })
        .collect();
    ps[0].aesthetic = Some(5.0); // poor for this shoot, average in absolute terms
    for s in [Sensitivity::Strict, Sensitivity::Balanced, Sensitivity::Lenient] {
        let (_, a) = run(&ps, s);
        assert_eq!((a[0].verdict, a[0].reasons.clone()), (Verdict::Keep, vec![]), "{s:?}: never flagged");
        let best = a.iter().map(|x| x.breakdown.aesthetic.unwrap()).fold(0.0, f32::max);
        assert!(best > 0.85 && a[0].breakdown.aesthetic.unwrap() < 0.1);
        // It weighs 10% of the score at most: from worst to best ≤ 10 points.
        let top = a.iter().map(|x| x.score).fold(0.0, f32::max);
        assert!(a[0].score < top && top - a[0].score <= 10.0, "{} vs {top}", a[0].score);
    }
}

#[test]
fn whole_moment_is_never_silently_dropped() {
    let base = 0x7777_0000_FFFF_1111u64;
    let mut ps: Vec<P> = (0..4)
        .map(|i| P::new(&format!("k{i}"), Some(5.0 + 0.4 * i as f64), 0.22 + 0.01 * i as f32, base ^ i as u64))
        .collect();
    ps.extend(filler(30, 1000.0, 8));
    let (_, a) = run(&ps, Sensitivity::Balanced);
    assert_eq!(a[3].verdict, Verdict::Maybe, "best of an all-soft burst is left to the photographer: {:?}", a[3]);
    assert!(a[..3].iter().all(|x| x.verdict == Verdict::Reject));
}

#[test]
fn assess_is_pure_robust_and_sorted() {
    assert!(assess(&[], &[], &CullSettings::default()).is_empty());
    let mut weird = P::new("nan", Some(0.0), f32::NAN, 0);
    weird.metrics.exposure.score = f32::INFINITY;
    weird.metrics.sharpness.motion_blur = f32::NAN;
    weird.aesthetic = Some(f32::NAN);
    weird.faces.push(FaceObservation { area_ratio: f32::NAN, ..face(0.05, f32::NAN, f32::NAN) });
    let mut both = P::new("both", Some(1.0), 0.02, 9);
    both.metrics.exposure.score = 0.01;
    both.metrics.exposure.mean_luma = 0.02;
    let ps = vec![weird, both];
    let bogus = vec![SimilarGroup { id: 9, kind: GroupKind::Burst, photo_ids: vec!["nope".into(), "nan".into()] }];
    let f = facts(&ps);
    let a1 = assess(&f, &bogus, &CullSettings::default());
    let a2 = assess(&f, &bogus, &CullSettings::default());
    assert_eq!(a1, a2);
    assert_eq!(a1.len(), 2);
    assert!(a1.iter().all(|a| a.score.is_finite() && (0.0..=100.0).contains(&a.score)));
    assert_eq!(a1[0].group_id, None, "a one-member group is no group");
    assert_eq!(a1[1].reasons, vec![Reason::Blurry, Reason::Underexposed], "sorted by severity");
}

/// A simulated wedding: bursts, singles, a few technical failures, faces,
/// aesthetic scores. Keep rate must be realistic and ordered by sensitivity.
fn wedding(n: usize, seed: u64) -> Vec<P> {
    let mut rng = Rng::new(seed);
    let mut ps = Vec::with_capacity(n);
    let mut t = 0.0f64;
    while ps.len() < n {
        t += rng.range(4.0, 40.0) as f64;
        let burst = if rng.f32() < 0.35 { 2 + (rng.f32() * 5.0) as usize } else { 1 };
        let hash = rng.next_u64();
        let moment_aes = 5.3 + 0.5 * rng.normal();
        // A face is there for the whole moment, or not at all.
        let moment_face = (rng.f32() < 0.6).then(|| rng.range(0.003, 0.05));
        for k in 0..burst.min(n - ps.len()) {
            let id = format!("w{:05}", ps.len());
            let r = rng.f32();
            let focus = if r < 0.06 {
                rng.range(0.02, 0.25)
            } else if r < 0.14 {
                rng.range(0.3, 0.6)
            } else {
                rng.range(0.75, 1.0)
            };
            let mut p = P::new(&id, Some(t + 0.3 * k as f64), focus, hash ^ (1u64 << (k * 3)));
            p.metrics.exposure.score = if rng.f32() < 0.04 { rng.range(0.0, 0.3) } else { rng.range(0.75, 1.0) };
            p.metrics.exposure.mean_luma = rng.range(0.1, 0.7);
            p.metrics.sharpness.motion_blur = if rng.f32() < 0.04 { rng.range(0.6, 1.0) } else { rng.range(0.0, 0.2) };
            if let Some(area) = moment_face {
                let eyes = if rng.f32() < 0.06 { rng.range(0.0, 0.2) } else { rng.range(0.7, 1.0) };
                let sharp = (focus + rng.range(-0.15, 0.05)).clamp(0.0, 1.0);
                p = p.face(area * rng.range(0.95, 1.05), sharp, eyes);
            }
            p.aesthetic = Some(moment_aes + 0.25 * rng.normal());
            ps.push(p);
        }
    }
    ps
}

#[test]
fn keep_rate_is_realistic_and_ordered_by_sensitivity() {
    let ps = wedding(3000, 11);
    let mut rates = Vec::new();
    for s in [Sensitivity::Strict, Sensitivity::Balanced, Sensitivity::Lenient] {
        let (_, a) = run(&ps, s);
        let keep = a.iter().filter(|x| x.verdict == Verdict::Keep).count() as f32 / a.len() as f32;
        let reject = a.iter().filter(|x| x.verdict == Verdict::Reject).count() as f32 / a.len() as f32;
        rates.push((keep, reject));
    }
    println!("keep/reject Strict {:?} Balanced {:?} Lenient {:?}", rates[0], rates[1], rates[2]);
    // A little under half: closed eyes on portraits and a low aesthetic no longer
    // send photos to review on Balanced.
    assert!((0.40..=0.60).contains(&rates[1].0), "Balanced keep rate {:?}", rates[1]);
    assert!(rates[0].0 < rates[1].0 && rates[1].0 < rates[2].0);
    assert!(rates[0].1 > rates[1].1 && rates[1].1 > rates[2].1);
}

#[test]
fn assess_is_fast_enough_for_a_live_slider() {
    let ps = wedding(5000, 12);
    let f = facts(&ps);
    let st = CullSettings::default();
    let g = group_similar(&f, &st);
    let t = Instant::now();
    let a = assess(&f, &g, &st);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("assess 5000 photos: {ms:.1} ms (test profile)");
    assert_eq!(a.len(), 5000);
    assert!(ms < 200.0, "{ms} ms");
}

#[test]
fn a_subject_region_judges_a_photo_without_face() {
    // A figure seen from the back against a blown sky: the frame reads
    // +3 stops over (score 0.03) and blurry-ish, the person is sharp and
    // exposed (+0.2 stop).
    let mut ps = filler(30, 10_000.0, 9);
    let mut p = P::new("back", Some(0.0), 0.35, 7);
    p.metrics.exposure.score = 0.03;
    p.metrics.exposure.mean_luma = 0.8;
    let subject = SubjectObservation {
        kind: SubjectKind::Person,
        score: 0.9,
        bbox: NormRect { x: 0.4, y: 0.3, w: 0.2, h: 0.6 },
        area_ratio: 0.08,
        sharpness: Some(0.9),
        ev_over: Some(0.2),
        mask: None,
    };
    ps.insert(0, p.clone());
    let without = run(&ps, Sensitivity::Balanced).1[0].clone();
    assert!(without.reasons.contains(&Reason::Overexposed), "{without:?}");
    p.subject = Some(subject.clone());
    ps[0] = p.clone();
    let with = run(&ps, Sensitivity::Balanced).1[0].clone();
    assert_eq!((with.verdict, with.reasons.clone()), (Verdict::Keep, vec![]), "{with:?}");
    assert!(with.score > without.score);
    // A soft subject does not hide the soft frame, and a dark frame stays
    // dark whatever the subject says.
    p.subject = Some(SubjectObservation { sharpness: Some(0.2), ..subject.clone() });
    p.metrics.exposure.mean_luma = 0.1;
    ps[0] = p;
    let soft = run(&ps, Sensitivity::Balanced).1[0].clone();
    assert!(soft.reasons.contains(&Reason::Underexposed), "{soft:?}");
}

#[test]
fn embeddings_of_different_spaces_never_link() {
    let e = vec![1.0, 0.0, 0.0];
    let mut a = P::new("a", Some(0.0), 0.9, 0x0f0f_0f0f_0f0f_0f0f);
    a.embedding = Some(e.clone());
    let mut b = P::new("b", Some(1.0), 0.9, 0xf0f0_f0f0_f0f0_f0f0);
    b.embedding = Some(e.clone());
    let both = [a.facts(), b.facts()];
    assert_eq!(group_similar(&both, &CullSettings::default()).len(), 1, "same space, same vector");
    b.space = EmbeddingSpace::DinoV2;
    let mixed = [a.facts(), b.facts()];
    assert!(group_similar(&mixed, &CullSettings::default()).is_empty(), "pHash fallback: far apart");
}
