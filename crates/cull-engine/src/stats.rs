//! Statistics across every shoot of the library, for the Statistics view.
//!
//! Computed on this computer, never sent anywhere. The AI's verdicts are not
//! stored: they are decided again from the stored measures when a project
//! is loaded, and the home screen loads every project for its cards. The
//! statistics read those decisions in memory, in one pass over the photos,
//! and add the measured speed, summed in SQL from the analysis runs the
//! library keeps ([`crate::store::Store::run_totals`]).
//!
//! The totals (photos, shoots, since when) are for everyone; the details are
//! part of Pelli Instant Plus.

use crate::store::ProjectRow;
use crate::view::{reason_key, DecisionCounts};
use crate::{camera_name, Engine, ProjectState, Result};
use cull_types::{final_decision, FinalDecision, Flag, Precision, Reason, Verdict};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Cameras listed, the most used first.
const TOP_CAMERAS: usize = 5;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatsOverview {
    /// Photos analysed, in every shoot.
    pub photos: u32,
    /// Shoots with at least one photo analysed.
    pub shoots: u32,
    /// Date of the first of them ([`ShootStats::shot_at`]).
    pub since: Option<i64>,
    /// The details, with Plus only.
    pub detail: Option<StatsDetail>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatsDetail {
    /// Final decisions of the photos analysed.
    pub counts: DecisionCounts,
    /// Rejected photos by the main reason the AI gave (its most severe).
    /// `byYou`: rejected by the photographer, not by the AI; `other`:
    /// rejected by the AI without a reason (its score alone).
    pub rejections: BTreeMap<String, u32>,
    pub changes: AiChanges,
    pub groups: GroupStats,
    /// Photos per month of capture ("2026-07"), oldest first. Photos without
    /// a capture date are left out.
    pub months: Vec<MonthStats>,
    /// The most used cameras.
    pub cameras: Vec<CameraStats>,
    /// Measured speed, per precision used.
    pub speed: Vec<SpeedStats>,
    /// Every shoot with a photo analysed, the latest first.
    pub shoots: Vec<ShootStats>,
}

/// What the photographer did with the AI's verdicts.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiChanges {
    /// Photos the AI kept or rejected.
    pub calls: u32,
    /// Rejected by the AI, kept by the photographer.
    pub rescued: u32,
    /// Kept by the AI, rejected by the photographer.
    pub dropped: u32,
    /// Photos the AI left to review.
    pub to_review: u32,
    /// Of those, the ones the photographer kept or rejected.
    pub decided: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroupStats {
    /// Groups of similar shots: bursts, near-identical frames, copies.
    pub count: u32,
    /// Photos in them.
    pub photos: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MonthStats {
    pub month: String,
    pub photos: u32,
    pub selected: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CameraStats {
    pub name: String,
    pub photos: u32,
    pub selected: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpeedStats {
    pub precision: Precision,
    pub photos: u64,
    /// Time the analyses took, from the scan of the folder to the decisions.
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShootStats {
    pub id: String,
    pub name: String,
    /// Capture time of its first photo (EXIF, read as UTC), else when the
    /// project was created.
    pub shot_at: i64,
    /// Photos analysed.
    pub photos: u32,
    pub counts: DecisionCounts,
    pub precision: Option<Precision>,
}

/// The sums, one project after the other (no allocation per photo: names
/// and keys are made once per project or at the end).
#[derive(Default)]
struct Totals {
    counts: DecisionCounts,
    /// Rejected by the AI, by main reason (`None`: without a reason).
    rejected_by_ai: BTreeMap<Option<Reason>, u32>,
    rejected_by_you: u32,
    changes: AiChanges,
    groups: GroupStats,
    /// (Year, month) → (photos, selected).
    months: BTreeMap<(i64, i64), (u32, u32)>,
    /// Camera → (photos, selected).
    cameras: HashMap<String, (u32, u32)>,
    shoots: Vec<ShootStats>,
}

impl Totals {
    fn add(&mut self, project: &ProjectRow, st: &ProjectState) {
        let mut shoot = ShootStats {
            id: project.id.clone(),
            name: project.name.clone(),
            shot_at: i64::MAX,
            photos: 0,
            counts: DecisionCounts::default(),
            precision: project.precision,
        };
        // (Make, model) → (photos, selected).
        let mut cameras: HashMap<(&str, &str), (u32, u32)> = HashMap::new();
        for asset in &st.assets {
            // Photos analysed: measured (not pending, not failed).
            let Some(row) = st.analyses.get(&asset.id).filter(|r| r.metrics.is_some()) else { continue };
            let mark = st.marks.get(&asset.id).copied().unwrap_or_default();
            let assessment = st.assessments.get(&asset.id);
            let verdict = assessment.map(|a| a.verdict);
            let decision = final_decision(verdict, &mark);
            let selected = u32::from(decision == FinalDecision::Selected);
            shoot.photos += 1;
            shoot.counts.add(decision);

            let changes = &mut self.changes;
            match verdict {
                Some(Verdict::Keep) => {
                    changes.calls += 1;
                    changes.dropped += u32::from(mark.flag == Flag::Reject);
                }
                Some(Verdict::Reject) => {
                    changes.calls += 1;
                    changes.rescued += u32::from(mark.flag == Flag::Pick);
                }
                Some(Verdict::Maybe) => {
                    changes.to_review += 1;
                    changes.decided += u32::from(mark.flag != Flag::None);
                }
                None => {}
            }
            if decision == FinalDecision::Rejected {
                if verdict == Some(Verdict::Reject) {
                    let reason = assessment.and_then(|a| a.reasons.first().copied());
                    *self.rejected_by_ai.entry(reason).or_default() += 1;
                } else {
                    self.rejected_by_you += 1;
                }
            }

            let Some(exif) = &row.exif else { continue };
            if let Some(t) = exif.captured_at_ms {
                shoot.shot_at = shoot.shot_at.min(t);
                let month = self.months.entry(year_month(t)).or_default();
                month.0 += 1;
                month.1 += selected;
            }
            let body = (exif.camera_make.as_deref().unwrap_or(""), exif.camera_model.as_deref().unwrap_or(""));
            if body != ("", "") {
                let camera = cameras.entry(body).or_default();
                camera.0 += 1;
                camera.1 += selected;
            }
        }
        if shoot.photos == 0 {
            return;
        }
        for ((make, model), (photos, selected)) in cameras {
            if let Some(name) = camera_name(non_empty(make), non_empty(model)) {
                let camera = self.cameras.entry(name).or_default();
                camera.0 += photos;
                camera.1 += selected;
            }
        }
        if shoot.shot_at == i64::MAX {
            shoot.shot_at = project.created_at;
        }
        for group in &st.groups {
            self.groups.count += 1;
            self.groups.photos += group.photo_ids.len() as u32;
        }
        self.counts.selected += shoot.counts.selected;
        self.counts.to_review += shoot.counts.to_review;
        self.counts.rejected += shoot.counts.rejected;
        self.shoots.push(shoot);
    }

    /// `runs`: [`crate::store::Store::run_totals`].
    fn overview(mut self, plus: bool, runs: Vec<(Precision, u64, u64)>) -> StatsOverview {
        let photos = self.shoots.iter().map(|s| s.photos).sum();
        let shoots = self.shoots.len() as u32;
        let since = self.shoots.iter().map(|s| s.shot_at).min();
        let detail = plus.then(|| {
            self.shoots.sort_by(|a, b| b.shot_at.cmp(&a.shot_at).then_with(|| a.name.cmp(&b.name)));
            let mut cameras: Vec<CameraStats> = self
                .cameras
                .into_iter()
                .map(|(name, (photos, selected))| CameraStats { name, photos, selected })
                .collect();
            cameras.sort_by(|a, b| b.photos.cmp(&a.photos).then_with(|| a.name.cmp(&b.name)));
            cameras.truncate(TOP_CAMERAS);
            let mut rejections: BTreeMap<String, u32> = self
                .rejected_by_ai
                .into_iter()
                .map(|(reason, n)| (reason.map(reason_key).unwrap_or_else(|| "other".into()), n))
                .collect();
            if self.rejected_by_you > 0 {
                rejections.insert("byYou".into(), self.rejected_by_you);
            }
            StatsDetail {
                counts: self.counts,
                rejections,
                changes: self.changes,
                groups: self.groups,
                months: self
                    .months
                    .into_iter()
                    .map(|((y, m), (photos, selected))| MonthStats {
                        month: format!("{y:04}-{m:02}"),
                        photos,
                        selected,
                    })
                    .collect(),
                cameras,
                speed: runs
                    .into_iter()
                    .map(|(precision, photos, duration_ms)| SpeedStats { precision, photos, duration_ms })
                    .collect(),
                shoots: self.shoots,
            }
        });
        StatsOverview { photos, shoots, since, detail }
    }
}

fn non_empty(s: &str) -> Option<&str> {
    Some(s).filter(|s| !s.is_empty())
}

/// (Year, month) of a capture time (EXIF, read as UTC).
fn year_month(ms: i64) -> (i64, i64) {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month)
}

impl Engine {
    /// Statistics across every shoot ([`StatsOverview`]): the totals for
    /// everyone, the details with Plus.
    pub fn stats_overview(&self) -> Result<StatsOverview> {
        let projects = self.store.lock().projects()?;
        let mut totals = Totals::default();
        for project in &projects {
            let state = self.state(&project.id)?;
            totals.add(project, &state.lock());
        }
        let plus = self.plus_active();
        let runs = if plus { self.store.lock().run_totals()? } else { Vec::new() };
        Ok(totals.overview(plus, runs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::AnalysisRow;
    use crate::testing::test_engine;
    use cull_types::{
        Assessment, CullSettings, ExifInfo, FileKind, GroupKind, PhotoAsset, SimilarGroup, SourceFile,
        TechnicalMetrics, UserMark,
    };
    use std::path::PathBuf;
    use std::sync::Arc;

    const DAY: i64 = 86_400_000;
    /// 2026-07-25 10:00 UTC.
    const JULY: i64 = 1_784_973_600_000;

    /// A photo of a test shoot.
    #[derive(Clone)]
    struct P {
        verdict: Option<Verdict>,
        reasons: Vec<Reason>,
        flag: Flag,
        captured: Option<i64>,
        camera: Option<&'static str>,
        measured: bool,
    }

    fn p(verdict: Verdict, flag: Flag) -> P {
        P { verdict: Some(verdict), reasons: vec![], flag, captured: Some(JULY), camera: None, measured: true }
    }

    impl P {
        fn reasons(mut self, reasons: &[Reason]) -> P {
            self.reasons = reasons.to_vec();
            self
        }
        fn camera(mut self, camera: &'static str) -> P {
            self.camera = Some(camera);
            self
        }
        fn at(mut self, captured: Option<i64>) -> P {
            self.captured = captured;
            self
        }
    }

    fn asset(id: &str) -> PhotoAsset {
        PhotoAsset {
            id: id.into(),
            rel_path: format!("{id}.jpg"),
            stem: id.into(),
            primary: SourceFile {
                path: PathBuf::from(format!("{id}.jpg")),
                kind: FileKind::Jpeg,
                size: 1,
                modified_ms: 1,
            },
            companions: vec![],
            xmp_sidecar: None,
        }
    }

    fn project(id: &str, created_at: i64) -> ProjectRow {
        ProjectRow {
            id: id.into(),
            root_path: format!("/photos/{id}").into(),
            name: id.into(),
            recursive: true,
            created_at,
            last_opened_at: created_at,
            analyzed_at: Some(created_at),
            settings: CullSettings::default(),
            precision: Some(Precision::Standard),
        }
    }

    /// A shoot as the engine holds it once decided, plus one photo waiting
    /// for its analysis.
    fn state(photos: &[P]) -> ProjectState {
        let mut st = ProjectState::default();
        for (i, ph) in photos.iter().enumerate() {
            let id = format!("p{i}");
            st.assets.push(asset(&id));
            let exif = ExifInfo {
                captured_at_ms: ph.captured,
                camera_make: ph.camera.map(|_| "Canon".to_string()),
                camera_model: ph.camera.map(str::to_string),
                ..Default::default()
            };
            let row = AnalysisRow {
                photo_id: id.clone(),
                file_size: 1,
                file_mtime: 1,
                models: "metrics-only".into(),
                exif: Some(exif),
                metrics: ph.measured.then(TechnicalMetrics::default),
                faces: vec![],
                aesthetic: None,
                embedding: None,
                search_embedding: None,
                subject: None,
                thumb_path: None,
                error: (!ph.measured).then(|| "unreadable".to_string()),
                duration_ms: 1,
                analyzed_at: 1,
            };
            st.analyses.insert(id.clone(), row);
            if let Some(verdict) = ph.verdict {
                let assessment = Assessment {
                    score: 50.0,
                    verdict,
                    reasons: ph.reasons.clone(),
                    group_id: None,
                    group_rank: 0,
                    breakdown: Default::default(),
                };
                st.assessments.insert(id.clone(), assessment);
            }
            if ph.flag != Flag::None {
                st.marks.insert(id, UserMark { flag: ph.flag, ..Default::default() });
            }
        }
        st.assets.push(asset("pending"));
        st
    }

    fn counts(selected: u32, to_review: u32, rejected: u32) -> DecisionCounts {
        DecisionCounts { selected, to_review, rejected }
    }

    #[test]
    fn months_of_capture_times() {
        assert_eq!(year_month(0), (1970, 1));
        assert_eq!(year_month(JULY), (2026, 7));
        assert_eq!(year_month(-1), (1969, 12));
        // 2024-02-29 23:59:59 and the next second, UTC.
        assert_eq!(year_month(1_709_251_199_000), (2024, 2));
        assert_eq!(year_month(1_709_251_200_000), (2024, 3));
        assert_eq!(year_month(1_767_225_600_000 - 1), (2025, 12));
    }

    #[test]
    fn totals_across_shoots() {
        use Flag::{None as Unflagged, Pick, Reject};
        use Verdict::{Keep, Maybe, Reject as Rejected};
        let mut wedding = state(&[
            p(Keep, Unflagged).camera("Canon EOS R5"),
            // Kept by the AI, rejected by the photographer.
            p(Keep, Reject).camera("Canon EOS R5"),
            p(Rejected, Unflagged).reasons(&[Reason::MotionBlur, Reason::Underexposed]).camera("Canon EOS R6"),
            // Rejected by the AI, kept by the photographer.
            p(Rejected, Pick).reasons(&[Reason::Blurry]).camera("Canon EOS R6"),
            p(Maybe, Unflagged),
            p(Maybe, Pick),
            p(Maybe, Reject),
            p(Rejected, Unflagged).at(None),
            // Unreadable: not analysed.
            P { verdict: None, reasons: vec![], flag: Flag::None, captured: None, camera: None, measured: false },
        ]);
        wedding.groups =
            vec![SimilarGroup { id: 0, kind: GroupKind::Burst, photo_ids: vec!["p0".into(), "p1".into()] }];
        let portraits = state(&[
            p(Keep, Unflagged).at(Some(JULY + 40 * DAY)).camera("Canon EOS R5"),
            p(Keep, Unflagged).at(Some(JULY + 40 * DAY)).camera("Canon EOS R5"),
        ]);
        let undated = state(&[p(Keep, Unflagged).at(None)]);
        let never_analysed = state(&[]);

        let mut totals = Totals::default();
        totals.add(&project("wedding", JULY + 30 * DAY), &wedding);
        totals.add(&project("portraits", JULY), &portraits);
        totals.add(&project("undated", JULY + 100 * DAY), &undated);
        totals.add(&project("empty", JULY), &never_analysed);
        let runs = vec![(Precision::Standard, 120, 10_000), (Precision::Maximum, 30, 9_000)];
        let overview = totals.overview(true, runs);

        assert_eq!((overview.photos, overview.shoots, overview.since), (11, 3, Some(JULY)));
        let detail = overview.detail.unwrap();
        assert_eq!(detail.counts, counts(6, 1, 4));
        assert_eq!(
            detail.rejections,
            BTreeMap::from([("byYou".to_string(), 2), ("motionBlur".to_string(), 1), ("other".to_string(), 1)])
        );
        assert_eq!(detail.changes, AiChanges { calls: 8, rescued: 1, dropped: 1, to_review: 3, decided: 2 });
        assert_eq!(detail.groups, GroupStats { count: 1, photos: 2 });
        assert_eq!(
            detail.months,
            vec![
                MonthStats { month: "2026-07".into(), photos: 7, selected: 3 },
                MonthStats { month: "2026-09".into(), photos: 2, selected: 2 },
            ]
        );
        assert_eq!(
            detail.cameras,
            vec![
                CameraStats { name: "Canon EOS R5".into(), photos: 4, selected: 3 },
                CameraStats { name: "Canon EOS R6".into(), photos: 2, selected: 1 },
            ]
        );
        assert_eq!(
            detail.speed,
            vec![
                SpeedStats { precision: Precision::Standard, photos: 120, duration_ms: 10_000 },
                SpeedStats { precision: Precision::Maximum, photos: 30, duration_ms: 9_000 },
            ]
        );
        // The latest first; a shoot without capture dates is dated by its project.
        let shoots: Vec<_> = detail.shoots.iter().map(|s| (s.id.as_str(), s.shot_at, s.photos, s.counts)).collect();
        assert_eq!(
            shoots,
            vec![
                ("undated", JULY + 100 * DAY, 1, counts(1, 0, 0)),
                ("portraits", JULY + 40 * DAY, 2, counts(2, 0, 0)),
                ("wedding", JULY, 8, counts(3, 1, 4)),
            ]
        );
    }

    #[test]
    fn the_details_are_part_of_plus() {
        let mut totals = Totals::default();
        totals.add(&project("wedding", JULY), &state(&[p(Verdict::Keep, Flag::None)]));
        let overview = totals.overview(false, vec![]);
        assert_eq!(overview, StatsOverview { photos: 1, shoots: 1, since: Some(JULY), detail: None });
    }

    fn sign_in_with_plus(engine: &Engine) {
        let account = crate::pelli::Account { plan: crate::pelli::Plan::Pro, plus: true, ..Default::default() };
        engine.kv_set("pelli.token", "bearer").unwrap();
        engine.kv_set("pelli.account", &serde_json::to_string(&account).unwrap()).unwrap();
    }

    #[test]
    fn an_empty_library() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = test_engine(tmp.path(), Arc::new(crate::NullSink));
        assert_eq!(engine.stats_overview().unwrap(), StatsOverview::default());
        sign_in_with_plus(&engine);
        let overview = engine.stats_overview().unwrap();
        assert_eq!(overview, StatsOverview { detail: Some(StatsDetail::default()), ..Default::default() });
    }

    #[test]
    fn an_analysis_measures_its_speed() {
        let tmp = tempfile::tempdir().unwrap();
        let shoot = tmp.path().join("Shoot");
        std::fs::create_dir_all(&shoot).unwrap();
        for i in 0..3u32 {
            let img = image::RgbImage::from_fn(640, 480, |x, y| {
                let v = ((x / 9 + y / 7 + i * 5) % 2) * 150 + ((x * 13 + y * 7 + i * 31) % 70);
                image::Rgb([v as u8, (v / 2 + 30) as u8, (255 - v) as u8])
            });
            std::fs::write(shoot.join(format!("IMG_{i}.jpg")), cull_decode::encode_jpeg(&img, 90)).unwrap();
        }
        let engine = test_engine(tmp.path(), Arc::new(crate::NullSink));
        let project = engine.open_folder(&shoot, true).unwrap();
        // Not analysed yet: no photo sorted.
        assert_eq!(engine.stats_overview().unwrap().photos, 0);
        let summary = engine.analyze_blocking(&project.id, CullSettings::default()).unwrap();
        let free = engine.stats_overview().unwrap();
        assert_eq!((free.photos, free.shoots, free.detail.is_none()), (3, 1, true));

        sign_in_with_plus(&engine);
        let speed = engine.stats_overview().unwrap().detail.unwrap().speed;
        assert_eq!(
            speed,
            vec![SpeedStats { precision: Precision::Standard, photos: 3, duration_ms: summary.duration_ms }]
        );
        // Nothing left to analyse: nothing measured.
        engine.analyze_blocking(&project.id, CullSettings::default()).unwrap();
        assert_eq!(engine.stats_overview().unwrap().detail.unwrap().speed[0].photos, 3);
    }
}
