//! UI-facing JSON views. They mirror `src/lib/contract.ts` field for field:
//! change both together.

use cull_types::{
    Assessment, ColorLabel, CullSettings, FileKind, FinalDecision, NormRect, Precision, Reason, Scene, SimilarGroup,
    UserMark,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FaceView {
    pub bbox: NormRect,
    pub confidence: f32,
    pub right_eye_open: Option<f32>,
    pub left_eye_open: Option<f32>,
    pub eyes_closed: Option<bool>,
    pub sharpness: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExifSummary {
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<u32>,
    pub shutter: Option<String>,
    pub aperture: Option<f32>,
    pub focal_length: Option<f32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PhotoStatus {
    Pending,
    Analyzed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhotoView {
    pub id: String,
    pub rel_path: String,
    pub file_name: String,
    pub kind: FileKind,
    pub has_raw: bool,
    pub has_jpeg: bool,
    pub size_bytes: u64,
    pub captured_at: Option<i64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub exif: ExifSummary,
    pub thumb_url: Option<String>,
    pub status: PhotoStatus,
    pub error: Option<String>,
    pub assessment: Option<Assessment>,
    pub faces: Vec<FaceView>,
    pub mark: UserMark,
    pub decision: FinalDecision,
    pub scene_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DecisionCounts {
    pub selected: u32,
    pub to_review: u32,
    pub rejected: u32,
}

impl DecisionCounts {
    pub fn add(&mut self, d: FinalDecision) {
        match d {
            FinalDecision::Selected => self.selected += 1,
            FinalDecision::ToReview => self.to_review += 1,
            FinalDecision::Rejected => self.rejected += 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub photo_count: u32,
    pub raw_count: u32,
    pub created_at: i64,
    pub last_opened_at: i64,
    pub analyzed_at: Option<i64>,
    pub cover_thumb_url: Option<String>,
    pub counts: DecisionCounts,
    pub settings: CullSettings,
    /// Precision of the last analysis, `None` before the first one.
    pub precision: Option<Precision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectData {
    pub project: ProjectSummary,
    pub photos: Vec<PhotoView>,
    pub groups: Vec<SimilarGroup>,
    pub scenes: Vec<Scene>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSummary {
    pub project_id: String,
    pub analyzed: u32,
    pub failed: u32,
    pub duration_ms: u64,
    pub counts: DecisionCounts,
    pub reasons: BTreeMap<String, u32>,
    pub groups: u32,
    /// Precision actually used (Standard when the Maximum pack was not
    /// ready).
    pub precision: Precision,
}

pub fn reason_key(r: Reason) -> String {
    serde_json::to_value(r).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub photo_id: String,
    pub score: f32,
}

/// Partial update of a [`UserMark`] (`setMarks` patch).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarkPatch {
    pub flag: Option<cull_types::Flag>,
    pub rating: Option<u8>,
    pub label: Option<ColorLabel>,
}

impl MarkPatch {
    pub fn apply(&self, mark: &mut UserMark) {
        if let Some(f) = self.flag {
            mark.flag = f;
        }
        if let Some(r) = self.rating {
            mark.rating = r.min(5);
        }
        if let Some(l) = self.label {
            mark.label = l;
        }
    }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PerDecision<T> {
    pub selected: T,
    pub to_review: T,
    pub rejected: T,
}

impl<T: Copy> PerDecision<T> {
    pub fn get(&self, d: FinalDecision) -> T {
        match d {
            FinalDecision::Selected => self.selected,
            FinalDecision::ToReview => self.to_review,
            FinalDecision::Rejected => self.rejected,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XmpExportOptions {
    pub enabled: bool,
    pub rating: PerDecision<i8>,
    pub label: PerDecision<ColorLabel>,
    pub keep_user_marks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CopySelectedOptions {
    pub enabled: bool,
    pub destination: Option<String>,
    pub include_raw: bool,
    pub include_jpeg: bool,
    pub keep_structure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MoveRejectedOptions {
    pub enabled: bool,
    pub folder_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub xmp: XmpExportOptions,
    pub copy_selected: CopySelectedOptions,
    pub move_rejected: MoveRejectedOptions,
    pub file_list: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    pub photo_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub xmp_written: u32,
    pub copied: u32,
    pub moved: u32,
    pub file_list_path: Option<String>,
    pub errors: Vec<ExportError>,
}

/// Format an exposure time the way photographers read it.
pub fn format_shutter(seconds: f32) -> Option<String> {
    if !(seconds.is_finite() && seconds > 0.0) {
        return None;
    }
    if seconds >= 0.3 {
        let rounded = (seconds * 10.0).round() / 10.0;
        if (rounded - rounded.round()).abs() < f32::EPSILON {
            Some(format!("{}\"", rounded.round() as u32))
        } else {
            Some(format!("{rounded}\""))
        }
    } else {
        Some(format!("1/{}", (1.0 / seconds).round() as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutter_formatting() {
        assert_eq!(format_shutter(0.004).as_deref(), Some("1/250"));
        assert_eq!(format_shutter(2.0).as_deref(), Some("2\""));
        assert_eq!(format_shutter(0.5).as_deref(), Some("0.5\""));
        assert_eq!(format_shutter(0.0), None);
    }

    #[test]
    fn mark_patch() {
        let mut m = UserMark::default();
        MarkPatch { rating: Some(9), ..Default::default() }.apply(&mut m);
        assert_eq!(m.rating, 5);
    }
}
