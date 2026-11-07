//! Persistent library: one SQLite database for every project.
//!
//! Nothing is ever written in the photographer's folders by the analysis;
//! measures, decisions and marks live here, previews in the cache dir.

use crate::EngineError;
use cull_types::{
    ColorLabel, CullSettings, ExifInfo, FaceObservation, Flag, PhotoAsset, Precision, SourceFile, SubjectObservation,
    TechnicalMetrics, UserMark,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Version of [`SCHEMA`], kept in `kv.schema_version`.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS project (
    id              TEXT PRIMARY KEY,
    root_path       TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    recursive       INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    last_opened_at  INTEGER NOT NULL,
    analyzed_at     INTEGER,
    settings_json   TEXT NOT NULL,
    -- 'standard' or 'maximum': precision of the last analysis.
    precision       TEXT
);
CREATE TABLE IF NOT EXISTS photo (
    project_id      TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    rel_path        TEXT NOT NULL,
    asset_json      TEXT NOT NULL,
    present         INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (project_id, id)
);
CREATE TABLE IF NOT EXISTS analysis (
    project_id      TEXT NOT NULL,
    photo_id        TEXT NOT NULL,
    file_size       INTEGER NOT NULL,
    file_mtime      INTEGER NOT NULL,
    models          TEXT NOT NULL,
    exif_json       TEXT,
    metrics_json    TEXT,
    faces_json      TEXT,
    aesthetic       REAL,
    -- Grouping embedding (CLIP B/32 in Standard, DINOv2 in Maximum), and the
    -- CLIP L/14 embedding for text search in Maximum (f32 little-endian).
    embedding       BLOB,
    search_embedding BLOB,
    subject_json    TEXT,
    thumb_path      TEXT,
    error           TEXT,
    duration_ms     INTEGER NOT NULL,
    analyzed_at     INTEGER NOT NULL,
    PRIMARY KEY (project_id, photo_id),
    FOREIGN KEY (project_id, photo_id) REFERENCES photo(project_id, id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS mark (
    project_id      TEXT NOT NULL,
    photo_id        TEXT NOT NULL,
    flag            TEXT NOT NULL,
    rating          INTEGER NOT NULL,
    label           TEXT NOT NULL,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (project_id, photo_id),
    FOREIGN KEY (project_id, photo_id) REFERENCES photo(project_id, id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Photos already sent to a Pelli gallery (fingerprint of the primary file),
-- so sending again only uploads what is new or changed.
CREATE TABLE IF NOT EXISTS upload (
    project_id      TEXT NOT NULL,
    photo_id        TEXT NOT NULL,
    gallery_id      TEXT NOT NULL,
    file_size       INTEGER NOT NULL,
    file_mtime      INTEGER NOT NULL,
    uploaded_at     INTEGER NOT NULL,
    PRIMARY KEY (project_id, gallery_id, photo_id),
    FOREIGN KEY (project_id, photo_id) REFERENCES photo(project_id, id) ON DELETE CASCADE
);
"#;

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: String,
    pub root_path: PathBuf,
    pub name: String,
    pub recursive: bool,
    pub created_at: i64,
    pub last_opened_at: i64,
    pub analyzed_at: Option<i64>,
    pub settings: CullSettings,
    /// Precision of the last analysis.
    pub precision: Option<Precision>,
}

/// Everything stored about one analysed photo.
#[derive(Debug, Clone)]
pub struct AnalysisRow {
    pub photo_id: String,
    pub file_size: u64,
    pub file_mtime: i64,
    pub models: String,
    pub exif: Option<ExifInfo>,
    pub metrics: Option<TechnicalMetrics>,
    pub faces: Vec<FaceObservation>,
    pub aesthetic: Option<f32>,
    pub embedding: Option<Vec<f32>>,
    pub search_embedding: Option<Vec<f32>>,
    pub subject: Option<SubjectObservation>,
    pub thumb_path: Option<PathBuf>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub analyzed_at: i64,
}

pub struct Store {
    conn: Connection,
}

fn flag_str(f: Flag) -> &'static str {
    match f {
        Flag::None => "none",
        Flag::Pick => "pick",
        Flag::Reject => "reject",
    }
}

fn parse_flag(s: &str) -> Flag {
    match s {
        "pick" => Flag::Pick,
        "reject" => Flag::Reject,
        _ => Flag::None,
    }
}

fn label_str(l: ColorLabel) -> &'static str {
    match l {
        ColorLabel::None => "none",
        ColorLabel::Red => "red",
        ColorLabel::Yellow => "yellow",
        ColorLabel::Green => "green",
        ColorLabel::Blue => "blue",
        ColorLabel::Purple => "purple",
    }
}

fn parse_label(s: &str) -> ColorLabel {
    match s {
        "red" => ColorLabel::Red,
        "yellow" => ColorLabel::Yellow,
        "green" => ColorLabel::Green,
        "blue" => ColorLabel::Blue,
        "purple" => ColorLabel::Purple,
        _ => ColorLabel::None,
    }
}

fn embedding_to_blob(e: &[f32]) -> Vec<u8> {
    e.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn blob_to_embedding(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, EngineError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| EngineError::io(parent, e))?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Store, EngineError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store, EngineError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT OR IGNORE INTO kv(key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(Store { conn })
    }

    // -- key/value ---------------------------------------------------------

    pub fn kv_get(&self, key: &str) -> Result<Option<String>, EngineError> {
        Ok(self.conn.query_row("SELECT value FROM kv WHERE key = ?1", params![key], |r| r.get(0)).optional()?)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<(), EngineError> {
        self.conn.execute(
            "INSERT INTO kv(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn kv_delete(&self, key: &str) -> Result<(), EngineError> {
        self.conn.execute("DELETE FROM kv WHERE key = ?1", params![key])?;
        Ok(())
    }

    // -- projects ----------------------------------------------------------

    fn project_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
        let settings_json: String = r.get(7)?;
        let precision: Option<String> = r.get(8)?;
        Ok(ProjectRow {
            id: r.get(0)?,
            root_path: PathBuf::from(r.get::<_, String>(1)?),
            name: r.get(2)?,
            recursive: r.get::<_, i64>(3)? != 0,
            created_at: r.get(4)?,
            last_opened_at: r.get(5)?,
            analyzed_at: r.get(6)?,
            settings: serde_json::from_str(&settings_json).unwrap_or_default(),
            precision: precision.and_then(|p| serde_json::from_value(serde_json::Value::String(p)).ok()),
        })
    }

    const PROJECT_COLS: &'static str = "id, root_path, name, recursive, created_at, \
        last_opened_at, analyzed_at, settings_json, precision";

    pub fn projects(&self) -> Result<Vec<ProjectRow>, EngineError> {
        let mut st =
            self.conn.prepare(&format!("SELECT {} FROM project ORDER BY last_opened_at DESC", Self::PROJECT_COLS))?;
        let rows = st.query_map([], Self::project_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn project(&self, id: &str) -> Result<Option<ProjectRow>, EngineError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {} FROM project WHERE id = ?1", Self::PROJECT_COLS),
                params![id],
                Self::project_from_row,
            )
            .optional()?)
    }

    pub fn upsert_project(&self, p: &ProjectRow) -> Result<(), EngineError> {
        self.conn.execute(
            "INSERT INTO project (id, root_path, name, recursive, created_at, last_opened_at,
                                  analyzed_at, settings_json, precision)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name, recursive = excluded.recursive,
                last_opened_at = excluded.last_opened_at, analyzed_at = excluded.analyzed_at,
                settings_json = excluded.settings_json, precision = excluded.precision",
            params![
                p.id,
                p.root_path.to_string_lossy(),
                p.name,
                p.recursive as i64,
                p.created_at,
                p.last_opened_at,
                p.analyzed_at,
                serde_json::to_string(&p.settings)?,
                p.precision.map(|p| if p == Precision::Maximum { "maximum" } else { "standard" }),
            ],
        )?;
        Ok(())
    }

    pub fn delete_project(&self, id: &str) -> Result<(), EngineError> {
        self.conn.execute("DELETE FROM project WHERE id = ?1", params![id])?;
        Ok(())
    }

    // -- photos ------------------------------------------------------------

    /// Replace the photo list of a project with a fresh scan: new photos are
    /// inserted, known ones updated, vanished ones marked absent (their marks
    /// and analysis are kept in case the card is plugged back in).
    pub fn sync_photos(&mut self, project_id: &str, assets: &[PhotoAsset]) -> Result<(), EngineError> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE photo SET present = 0 WHERE project_id = ?1", params![project_id])?;
        {
            let mut st = tx.prepare(
                "INSERT INTO photo (project_id, id, rel_path, asset_json, present)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT(project_id, id) DO UPDATE SET
                    rel_path = excluded.rel_path, asset_json = excluded.asset_json, present = 1",
            )?;
            for a in assets {
                st.execute(params![project_id, a.id, a.rel_path, serde_json::to_string(a)?])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Present photos of a project, in scan order (natural `rel_path` order).
    pub fn photos(&self, project_id: &str) -> Result<Vec<PhotoAsset>, EngineError> {
        let mut st = self.conn.prepare("SELECT asset_json FROM photo WHERE project_id = ?1 AND present = 1")?;
        let rows = st.query_map(params![project_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for json in rows {
            if let Ok(asset) = serde_json::from_str::<PhotoAsset>(&json?) {
                out.push(asset);
            }
        }
        out.sort_by(|a, b| natural_cmp(&a.rel_path, &b.rel_path));
        Ok(out)
    }

    pub fn update_asset(&self, project_id: &str, asset: &PhotoAsset) -> Result<(), EngineError> {
        self.conn.execute(
            "UPDATE photo SET asset_json = ?3 WHERE project_id = ?1 AND id = ?2",
            params![project_id, asset.id, serde_json::to_string(asset)?],
        )?;
        Ok(())
    }

    // -- analysis ----------------------------------------------------------

    /// Saves analyses. A photo without a mark, for which `from_camera` holds,
    /// takes the rating written in its file ([`camera_mark`]) in the same
    /// transaction: the marks seeded are returned.
    pub fn save_analyses(
        &mut self,
        project_id: &str,
        rows: &[AnalysisRow],
        from_camera: impl Fn(&str) -> bool,
        now: i64,
    ) -> Result<Vec<(String, UserMark)>, EngineError> {
        let tx = self.conn.transaction()?;
        let mut seeded = Vec::new();
        {
            let mut st = tx.prepare(
                "INSERT OR REPLACE INTO analysis (project_id, photo_id, file_size, file_mtime,
                    models, exif_json, metrics_json, faces_json, aesthetic, embedding,
                    search_embedding, subject_json, thumb_path, error, duration_ms, analyzed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )?;
            let mut seed = tx.prepare(
                "INSERT OR IGNORE INTO mark (project_id, photo_id, flag, rating, label, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for a in rows {
                st.execute(params![
                    project_id,
                    a.photo_id,
                    a.file_size as i64,
                    a.file_mtime,
                    a.models,
                    a.exif.as_ref().map(serde_json::to_string).transpose()?,
                    a.metrics.as_ref().map(serde_json::to_string).transpose()?,
                    serde_json::to_string(&a.faces)?,
                    a.aesthetic,
                    a.embedding.as_deref().map(embedding_to_blob),
                    a.search_embedding.as_deref().map(embedding_to_blob),
                    a.subject.as_ref().map(serde_json::to_string).transpose()?,
                    a.thumb_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    a.error,
                    a.duration_ms as i64,
                    a.analyzed_at,
                ])?;
                let camera = if from_camera(&a.photo_id) { a.exif.as_ref().and_then(|e| e.rating) } else { None };
                if let Some(m) = camera_mark(camera) {
                    let row =
                        params![project_id, a.photo_id, flag_str(m.flag), m.rating as i64, label_str(m.label), now];
                    if seed.execute(row)? > 0 {
                        seeded.push((a.photo_id.clone(), m));
                    }
                }
            }
        }
        tx.commit()?;
        Ok(seeded)
    }

    pub fn analyses(&self, project_id: &str) -> Result<Vec<AnalysisRow>, EngineError> {
        let mut st = self.conn.prepare(
            "SELECT photo_id, file_size, file_mtime, models, exif_json, metrics_json,
                    faces_json, aesthetic, embedding, thumb_path, error, duration_ms, analyzed_at,
                    search_embedding, subject_json
             FROM analysis WHERE project_id = ?1",
        )?;
        let rows = st.query_map(params![project_id], |r| {
            let exif: Option<String> = r.get(4)?;
            let metrics: Option<String> = r.get(5)?;
            let faces: Option<String> = r.get(6)?;
            let embedding: Option<Vec<u8>> = r.get(8)?;
            let search_embedding: Option<Vec<u8>> = r.get(13)?;
            let subject: Option<String> = r.get(14)?;
            Ok(AnalysisRow {
                photo_id: r.get(0)?,
                file_size: r.get::<_, i64>(1)? as u64,
                file_mtime: r.get(2)?,
                models: r.get(3)?,
                exif: exif.and_then(|s| serde_json::from_str(&s).ok()),
                metrics: metrics.and_then(|s| serde_json::from_str(&s).ok()),
                faces: faces.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                aesthetic: r.get(7)?,
                embedding: embedding.as_deref().map(blob_to_embedding),
                search_embedding: search_embedding.as_deref().map(blob_to_embedding),
                subject: subject.and_then(|s| serde_json::from_str(&s).ok()),
                thumb_path: r.get::<_, Option<String>>(9)?.map(PathBuf::from),
                error: r.get(10)?,
                duration_ms: r.get::<_, i64>(11)? as u64,
                analyzed_at: r.get(12)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // -- uploads -----------------------------------------------------------

    /// Photos of a project already in `gallery_id`: id → (size, mtime) of
    /// the primary file when it was sent.
    pub fn uploaded(&self, project_id: &str, gallery_id: &str) -> Result<HashMap<String, (u64, i64)>, EngineError> {
        let mut st = self
            .conn
            .prepare("SELECT photo_id, file_size, file_mtime FROM upload WHERE project_id = ?1 AND gallery_id = ?2")?;
        let rows = st.query_map(params![project_id, gallery_id], |r| {
            Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)?)))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mark_uploaded(
        &self,
        project_id: &str,
        gallery_id: &str,
        photo_id: &str,
        (size, mtime): (u64, i64),
        now: i64,
    ) -> Result<(), EngineError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO upload (project_id, photo_id, gallery_id, file_size, file_mtime, uploaded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![project_id, photo_id, gallery_id, size as i64, mtime, now],
        )?;
        Ok(())
    }

    // -- marks -------------------------------------------------------------

    pub fn marks(&self, project_id: &str) -> Result<Vec<(String, UserMark)>, EngineError> {
        let mut st = self.conn.prepare("SELECT photo_id, flag, rating, label FROM mark WHERE project_id = ?1")?;
        let rows = st.query_map(params![project_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                UserMark {
                    flag: parse_flag(&r.get::<_, String>(1)?),
                    rating: r.get::<_, i64>(2)?.clamp(0, 5) as u8,
                    label: parse_label(&r.get::<_, String>(3)?),
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Runs raw SQL (tests that break the database on purpose).
    #[cfg(test)]
    pub(crate) fn execute_for_tests(&self, sql: &str) {
        self.conn.execute_batch(sql).unwrap();
    }

    pub fn save_marks(&mut self, project_id: &str, marks: &[(String, UserMark)], now: i64) -> Result<(), EngineError> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare(
                "INSERT OR REPLACE INTO mark (project_id, photo_id, flag, rating, label, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (id, m) in marks {
                st.execute(params![project_id, id, flag_str(m.flag), m.rating as i64, label_str(m.label), now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

/// The mark a photo starts with from the rating its camera (or another tool)
/// wrote in the file: 1 to 5 stars become the photographer's stars, -1 a
/// reject flag; no rating or 0 stars, none. The AI's verdict still decides
/// the photos with stars (only a flag overrides it).
pub fn camera_mark(rating: Option<i8>) -> Option<UserMark> {
    match rating? {
        -1 => Some(UserMark { flag: Flag::Reject, ..Default::default() }),
        r @ 1..=5 => Some(UserMark { rating: r as u8, ..Default::default() }),
        _ => None,
    }
}

/// Natural order: `IMG_2` < `IMG_10`, case-insensitive.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let ta = na.trim_start_matches('0');
                let tb = nb.trim_start_matches('0');
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Size/mtime of a file to detect changes since the last analysis.
pub fn file_fingerprint(f: &SourceFile) -> (u64, i64) {
    (f.size, f.modified_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cull_types::FileKind;

    fn asset(id: &str, rel: &str) -> PhotoAsset {
        PhotoAsset {
            id: id.into(),
            rel_path: rel.into(),
            stem: rel.into(),
            primary: SourceFile { path: PathBuf::from(rel), kind: FileKind::Jpeg, size: 10, modified_ms: 1 },
            companions: vec![],
            xmp_sidecar: None,
        }
    }

    #[test]
    fn camera_ratings() {
        assert_eq!(camera_mark(Some(4)), Some(UserMark { rating: 4, ..Default::default() }));
        assert_eq!(camera_mark(Some(-1)), Some(UserMark { flag: Flag::Reject, ..Default::default() }));
        assert_eq!(camera_mark(Some(0)), None);
        assert_eq!(camera_mark(Some(9)), None);
        assert_eq!(camera_mark(None), None);
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["IMG_10.jpg", "img_2.jpg", "IMG_1.jpg"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["IMG_1.jpg", "img_2.jpg", "IMG_10.jpg"]);
    }

    #[test]
    fn roundtrip() {
        let mut s = Store::open_in_memory().unwrap();
        let p = ProjectRow {
            id: "p".into(),
            root_path: "/x".into(),
            name: "x".into(),
            recursive: true,
            created_at: 1,
            last_opened_at: 2,
            analyzed_at: None,
            settings: CullSettings::default(),
            precision: Some(Precision::Maximum),
        };
        s.upsert_project(&p).unwrap();
        s.sync_photos("p", &[asset("a", "IMG_10.jpg"), asset("b", "IMG_2.jpg")]).unwrap();
        assert_eq!(s.photos("p").unwrap()[0].id, "b");
        let row = AnalysisRow {
            photo_id: "a".into(),
            file_size: 10,
            file_mtime: 1,
            models: "m".into(),
            exif: Some(ExifInfo { rating: Some(3), ..Default::default() }),
            metrics: Some(TechnicalMetrics::default()),
            faces: vec![],
            aesthetic: Some(5.5),
            embedding: Some(vec![0.5, -1.0]),
            search_embedding: Some(vec![0.25]),
            subject: None,
            thumb_path: None,
            error: None,
            duration_ms: 3,
            analyzed_at: 4,
        };
        // The camera's 3 stars become the photo's mark.
        // Not for a photo whose ratings another tool already keeps.
        assert!(s.save_analyses("p", std::slice::from_ref(&row), |_| false, 6).unwrap().is_empty());
        assert!(s.marks("p").unwrap().is_empty());
        let seeded = s.save_analyses("p", std::slice::from_ref(&row), |_| true, 7).unwrap();
        assert_eq!(seeded, vec![("a".to_string(), UserMark { rating: 3, ..Default::default() })]);
        assert_eq!(s.marks("p").unwrap(), seeded);
        let a = s.analyses("p").unwrap();
        assert_eq!(a[0].embedding.as_deref(), Some(&[0.5f32, -1.0][..]));
        assert_eq!(a[0].search_embedding.as_deref(), Some(&[0.25f32][..]));
        assert_eq!(s.project("p").unwrap().unwrap().precision, Some(Precision::Maximum));
        s.save_marks("p", &[("a".into(), UserMark { flag: Flag::Pick, rating: 4, label: ColorLabel::Green })], 9)
            .unwrap();
        assert_eq!(s.marks("p").unwrap()[0].1.rating, 4);
        // Analysed again: the photographer's mark stays.
        assert!(s.save_analyses("p", &[row], |_| true, 8).unwrap().is_empty());
        assert_eq!(s.marks("p").unwrap()[0].1, UserMark { flag: Flag::Pick, rating: 4, label: ColorLabel::Green });
        // Uploads are remembered per gallery.
        s.mark_uploaded("p", "g1", "a", (10, 1), 5).unwrap();
        assert_eq!(s.uploaded("p", "g1").unwrap().get("a"), Some(&(10, 1)));
        assert!(s.uploaded("p", "g2").unwrap().is_empty());
        // A rescan without "a" keeps its data but hides it.
        s.sync_photos("p", &[asset("b", "IMG_2.jpg")]).unwrap();
        assert_eq!(s.photos("p").unwrap().len(), 1);
        s.delete_project("p").unwrap();
        assert!(s.projects().unwrap().is_empty());
    }
}
