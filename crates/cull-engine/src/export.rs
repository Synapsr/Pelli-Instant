//! Export of the cull: XMP sidecars, copy of the selection, move of the
//! rejects, file-name list. Originals are never modified or deleted.
//!
//! The folders the export fills (rejects folders, a copy destination that
//! was new or empty) get [`cull_decode::EXPORT_MARKER`], so the scan never
//! imports their photos again.

use crate::view::{ExportError, ExportOptions, ExportReport};
use crate::{Engine, EngineError, Result};
use cull_analysis::{XmpPayload, XmpSource};
use cull_types::{final_decision, ColorLabel, FileKind, FinalDecision, Flag, PhotoAsset, UserMark};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Name of the selection list written at the project root.
pub const FILE_LIST_NAME: &str = "Pelli-selection.txt";
/// Largest modification-time difference of an earlier copy: FAT and exFAT
/// disks keep it to 2 s.
const MTIME_TOLERANCE: Duration = Duration::from_secs(2);

impl Engine {
    pub fn export_project(&self, project_id: &str, opts: &ExportOptions) -> Result<ExportReport> {
        let _awake = crate::system::KeepAwake::new("Pelli Instant: exporting photos");
        let _export = crate::Counted::new(&self.exports);
        let project = self.project_row(project_id)?;
        let state = self.state(project_id)?;
        let items: Vec<(PhotoAsset, FinalDecision, UserMark, Option<cull_types::Assessment>)> = {
            let st = state.lock();
            st.assets
                .iter()
                .map(|a| {
                    let mark = st.marks.get(&a.id).copied().unwrap_or_default();
                    let assessment = st.assessments.get(&a.id).cloned();
                    let d = final_decision(assessment.as_ref().map(|x| x.verdict), &mark);
                    (a.clone(), d, mark, assessment)
                })
                .collect()
        };
        let mut report = ExportReport::default();
        let fail = |report: &mut ExportReport, id: &str, e: String| {
            report.errors.push(ExportError { photo_id: id.to_string(), message: e });
        };

        // 1. XMP sidecars, next to RAW files only: Lightroom Classic and Bridge
        //    read a JPEG's ratings inside the file and ignore a sidecar, so a
        //    JPEG shoot would only get thousands of useless `.xmp` files (the
        //    selection list covers it). A RAW + JPEG pair has the RAW as
        //    primary: its sidecar is the one Lightroom reads.
        if opts.xmp.enabled {
            for (asset, decision, mark, assessment) in items.iter().filter(|i| i.0.primary.kind == FileKind::Raw) {
                // Stars and label set in Pelli are the photographer's; the
                // others come from the decision and never replace those of
                // another tool.
                let use_user = opts.xmp.keep_user_marks;
                let own_rating = use_user && mark.rating > 0;
                let own_label = use_user && mark.label != ColorLabel::None;
                let rating = if own_rating { mark.rating as i8 } else { opts.xmp.rating.get(*decision) };
                let label = if own_label { mark.label } else { opts.xmp.label.get(*decision) };
                let source = |own: bool| if own { XmpSource::Photographer } else { XmpSource::Decision };
                let payload = XmpPayload {
                    mark: UserMark {
                        flag: if rating < 0 { Flag::Reject } else { mark.flag },
                        rating: rating.clamp(0, 5) as u8,
                        label,
                    },
                    rating_source: source(own_rating),
                    label_source: source(own_label),
                    assessment: assessment.clone(),
                };
                match cull_analysis::write_xmp_sidecar(&asset.primary.path, &payload) {
                    Ok(path) => {
                        report.xmp_written += 1;
                        if asset.xmp_sidecar.as_deref() != Some(path.as_path()) {
                            let mut updated = asset.clone();
                            updated.xmp_sidecar = Some(path);
                            self.replace_asset(project_id, updated)?;
                        }
                    }
                    Err(e) => fail(&mut report, &asset.id, e.to_string()),
                }
            }
        }
        // Reload assets: sidecar paths may have changed.
        let assets: Vec<PhotoAsset> = state.lock().assets.clone();
        let find = |id: &str| assets.iter().find(|a| a.id == id).cloned();

        // 2. Copy the selection.
        if opts.copy_selected.enabled {
            let dest = opts
                .copy_selected
                .destination
                .as_deref()
                .map(PathBuf::from)
                .ok_or_else(|| EngineError::FolderNotFound("(no destination)".into()))?;
            // A new or empty destination holds only the export: marked, so
            // a destination inside the shoot is not analysed again. A folder
            // that already holds photos is left as it is.
            let fresh = std::fs::read_dir(&dest).map_or(true, |mut d| d.next().is_none());
            if fresh {
                export_folder(&dest)?;
            }
            for (asset, ..) in items.iter().filter(|i| i.1 == FinalDecision::Selected) {
                let asset = find(&asset.id).unwrap_or_else(|| asset.clone());
                let (media, sidecar) =
                    files_to_copy(&asset, opts.copy_selected.include_raw, opts.copy_selected.include_jpeg);
                let sub = if opts.copy_selected.keep_structure {
                    Path::new(&asset.rel_path).parent().map(Path::to_path_buf).unwrap_or_default()
                } else {
                    PathBuf::new()
                };
                match copy_photo(&media, sidecar.as_deref(), &asset.stem, &dest.join(sub)) {
                    Ok(()) => report.copied += 1,
                    Err(e) => fail(&mut report, &asset.id, e.to_string()),
                }
            }
        }

        // 3. Move the rejects into a sub-folder next to them.
        if opts.move_rejected.enabled {
            let name = sanitize_folder_name(&opts.move_rejected.folder_name);
            for (asset, ..) in items.iter().filter(|i| i.1 == FinalDecision::Rejected) {
                let asset = find(&asset.id).unwrap_or_else(|| asset.clone());
                match move_photo(&asset, &name) {
                    Ok(moved) => {
                        report.moved += 1;
                        self.replace_asset(project_id, moved)?;
                    }
                    Err(e) => fail(&mut report, &asset.id, e.to_string()),
                }
            }
        }

        // 4. File-name list for Lightroom's filter ("Filename contains").
        if opts.file_list {
            let stems: Vec<&str> =
                items.iter().filter(|i| i.1 == FinalDecision::Selected).map(|i| i.0.stem.as_str()).collect();
            let path = project.root_path.join(FILE_LIST_NAME);
            match crate::pipeline::write_atomic(&path, format!("{}\n", stems.join(", ")).as_bytes()) {
                Ok(()) => report.file_list_path = Some(path.to_string_lossy().into_owned()),
                Err(e) => fail(&mut report, "", e.to_string()),
            }
        }
        Ok(report)
    }

    /// Update an asset (new paths) in the store and the loaded state.
    pub(crate) fn replace_asset(&self, project_id: &str, asset: PhotoAsset) -> Result<()> {
        self.store.lock().update_asset(project_id, &asset)?;
        let state = self.state(project_id)?;
        let mut st = state.lock();
        if let Some(slot) = st.assets.iter_mut().find(|a| a.id == asset.id) {
            *slot = asset;
        }
        Ok(())
    }
}

/// The photo files to copy (RAW and/or JPEG twin) and its sidecar.
fn files_to_copy(asset: &PhotoAsset, include_raw: bool, include_jpeg: bool) -> (Vec<PathBuf>, Option<PathBuf>) {
    let all: Vec<&cull_types::SourceFile> = std::iter::once(&asset.primary).chain(asset.companions.iter()).collect();
    let paired = all.len() > 1;
    let mut media: Vec<PathBuf> = all
        .iter()
        .filter(|f| !paired || (f.kind == FileKind::Raw && include_raw) || (f.kind != FileKind::Raw && include_jpeg))
        .map(|f| f.path.clone())
        .collect();
    if media.is_empty() {
        media.push(asset.primary.path.clone());
    }
    (media, asset.xmp_sidecar.clone().filter(|x| x.exists()))
}

fn sanitize_folder_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c })
        .collect();
    let cleaned = cleaned.trim_matches('.').trim().to_string();
    if cleaned.is_empty() {
        "_Rejets Pelli".to_string()
    } else {
        cleaned
    }
}

/// Create `dir` (if needed) as a folder of the export: with
/// [`cull_decode::EXPORT_MARKER`] (hidden), so the scan skips it.
fn export_folder(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| EngineError::io(dir, e))?;
    let marker = dir.join(cull_decode::EXPORT_MARKER);
    if marker.exists() {
        return Ok(());
    }
    let mut file = std::fs::OpenOptions::new();
    file.write(true).create(true).truncate(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        file.attributes(FILE_ATTRIBUTE_HIDDEN);
    }
    let text = "Folder filled by Pelli Instant: its analyses never import these photos again.\n\
                Delete this file to analyse them.\n";
    file.open(&marker)
        .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()))
        .map_err(|e| EngineError::io(&marker, e))
}

/// What a copy target holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Free,
    /// An earlier copy of the same file (same size and modification time).
    Same,
    Other,
}

fn target_state(src: &Metadata, target: &Path) -> Target {
    let Ok(t) = std::fs::metadata(target) else { return Target::Free };
    let close = match (src.modified(), t.modified()) {
        (Ok(a), Ok(b)) => a.duration_since(b).unwrap_or_else(|e| e.duration()) <= MTIME_TOLERANCE,
        _ => false,
    };
    if t.len() == src.len() && close {
        Target::Same
    } else {
        Target::Other
    }
}

/// `src`'s file name with `-n` after the photo's `stem` (`n` = 0: as is).
fn numbered_name(src: &Path, stem: &str, n: u32) -> String {
    let name = src.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if n == 0 {
        return name;
    }
    let starts_with_stem =
        name.is_char_boundary(stem.len()) && name[..stem.len()].to_lowercase() == stem.to_lowercase();
    let cut = if starts_with_stem { stem.len() } else { name.find('.').unwrap_or(name.len()) };
    format!("{}-{n}{}", &name[..cut], &name[cut..])
}

/// Copy one photo's files into `dir` under one name for all of them: their
/// own (`IMG_1.CR3`, `IMG_1.JPG`, `IMG_1.xmp`), else the first free
/// `IMG_1-1.*`, `IMG_1-2.*`… Files already there from an earlier export
/// are kept, and the sidecar of such a copy is replaced when it changed
/// (new ratings).
fn copy_photo(media: &[PathBuf], sidecar: Option<&Path>, stem: &str, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| EngineError::io(dir, e))?;
    let meta = |p: &Path| std::fs::metadata(p).map_err(|e| EngineError::io(p, e));
    let media: Vec<(&Path, Metadata)> = media.iter().map(|p| Ok((p.as_path(), meta(p)?))).collect::<Result<_>>()?;
    let sidecar = sidecar.map(|p| Ok::<_, EngineError>((p, meta(p)?))).transpose()?;
    for n in 0..10_000 {
        let plan = |(src, m): &(&Path, Metadata)| {
            let target = dir.join(numbered_name(src, stem, n));
            let state = target_state(m, &target);
            (src.to_path_buf(), target, state)
        };
        let media_plan: Vec<(PathBuf, PathBuf, Target)> = media.iter().map(plan).collect();
        if media_plan.iter().any(|p| p.2 == Target::Other) {
            continue;
        }
        let earlier_copy = media_plan.iter().any(|p| p.2 == Target::Same);
        let sidecar_plan = sidecar.as_ref().map(plan);
        if sidecar_plan.as_ref().is_some_and(|p| p.2 == Target::Other && !earlier_copy) {
            continue; // someone else's sidecar
        }
        for (src, target, state) in media_plan.iter().chain(sidecar_plan.iter()) {
            if *state != Target::Same {
                copy_atomic(src, target)?;
            }
        }
        return Ok(());
    }
    Err(EngineError::io(&dir.join(stem), std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free file name")))
}

/// Copy `src` to `target` through a hidden temporary file in the same
/// folder, renamed into place once complete (modification time kept): a
/// full disk or an unplugged drive never leaves a truncated file under the
/// photo's name.
fn copy_atomic(src: &Path, target: &Path) -> Result<()> {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = target.with_file_name(format!(".{name}.pelli-tmp"));
    let result = (|| {
        std::fs::copy(src, &tmp)?;
        if let Ok(modified) = std::fs::metadata(src)?.modified() {
            let _ = std::fs::File::options().write(true).open(&tmp)?.set_modified(modified);
        }
        std::fs::rename(&tmp, target)
    })();
    result.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        EngineError::io(target, e)
    })
}

/// Move a photo's files (twins, sidecar) into `<their folder>/<folder>/`,
/// all or none: every target is checked first, and the files already moved
/// go back when one fails.
fn move_photo(asset: &PhotoAsset, folder: &str) -> Result<PhotoAsset> {
    let sidecar = asset.xmp_sidecar.as_deref().filter(|x| x.exists());
    let sources = std::iter::once(asset.primary.path.as_path())
        .chain(asset.companions.iter().map(|c| c.path.as_path()))
        .chain(sidecar);
    // Files already in a rejects folder stay where they are.
    let plan: Vec<(PathBuf, PathBuf)> = sources
        .filter_map(|p| {
            let dir = p.parent()?;
            if dir.file_name().is_some_and(|n| n.to_string_lossy() == folder) {
                return None;
            }
            Some((p.to_path_buf(), dir.join(folder).join(p.file_name()?)))
        })
        .collect();
    if let Some((_, target)) = plan.iter().find(|(_, t)| t.exists()) {
        return Err(EngineError::io(target, std::io::Error::new(std::io::ErrorKind::AlreadyExists, "file exists")));
    }
    for (i, (from, to)) in plan.iter().enumerate() {
        let moved = to.parent().map_or(Ok(()), export_folder).and_then(|()| move_file(from, to));
        if let Err(e) = moved {
            for (back_to, back_from) in plan[..i].iter().rev() {
                if let Err(undo) = move_file(back_from, back_to) {
                    log::error!("{} could not go back to {}: {undo}", back_from.display(), back_to.display());
                }
            }
            return Err(e);
        }
    }
    let new_path = |p: &Path| plan.iter().find(|(f, _)| f == p).map_or_else(|| p.to_path_buf(), |(_, t)| t.clone());
    let mut moved = asset.clone();
    moved.primary.path = new_path(&asset.primary.path);
    for c in &mut moved.companions {
        c.path = new_path(&c.path);
    }
    if let Some(x) = sidecar {
        moved.xmp_sidecar = Some(new_path(x));
    }
    Ok(moved)
}

/// Rename `from` to `to`. Across disks: copy, flush to the disk, check the
/// length, and only then delete the source.
fn move_file(from: &Path, to: &Path) -> Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            let copied = (|| {
                std::fs::copy(from, to)?;
                let out = std::fs::File::options().write(true).open(to)?;
                out.sync_all()?;
                if out.metadata()?.len() != std::fs::metadata(from)?.len() {
                    return Err(std::io::Error::other("incomplete copy"));
                }
                Ok(())
            })();
            if let Err(e) = copied {
                let _ = std::fs::remove_file(to);
                return Err(EngineError::io(to, e));
            }
            std::fs::remove_file(from).map_err(|e| {
                let _ = std::fs::remove_file(to);
                EngineError::io(from, e)
            })
        }
        Err(e) => Err(EngineError::io(from, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cull_types::SourceFile;

    #[test]
    fn folder_names() {
        assert_eq!(sanitize_folder_name("  ../Rejets:2 "), "-Rejets-2");
        assert_eq!(sanitize_folder_name(""), "_Rejets Pelli");
    }

    fn write(path: &Path, data: &[u8]) -> PathBuf {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
        path.to_path_buf()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn copies_are_idempotent_and_conflicts_get_one_suffix_per_photo() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = write(&tmp.path().join("a/IMG_1.CR3"), b"raw-a");
        let jpg = write(&tmp.path().join("a/IMG_1.JPG"), b"jpeg-a");
        let xmp = write(&tmp.path().join("a/IMG_1.xmp"), b"<x Rating=\"3\"/>");
        let dest = tmp.path().join("out");
        let media = [raw.clone(), jpg.clone()];
        copy_photo(&media, Some(&xmp), "IMG_1", &dest).unwrap();
        copy_photo(&media, Some(&xmp), "IMG_1", &dest).unwrap();
        assert_eq!(names(&dest), ["IMG_1.CR3", "IMG_1.JPG", "IMG_1.xmp"]);

        // Same name, same size, another camera: its own suffix, for all its files.
        let raw_b = write(&tmp.path().join("b/IMG_1.CR3"), b"raw-b");
        let jpg_b = write(&tmp.path().join("b/IMG_1.JPG"), b"jpeg-b");
        let xmp_b = write(&tmp.path().join("b/IMG_1.CR3.xmp"), b"<x Rating=\"3\"/>");
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        for f in [&raw_b, &jpg_b] {
            std::fs::File::options().write(true).open(f).unwrap().set_modified(old).unwrap();
        }
        copy_photo(&[raw_b, jpg_b], Some(&xmp_b), "IMG_1", &dest).unwrap();
        assert_eq!(
            names(&dest),
            ["IMG_1-1.CR3", "IMG_1-1.CR3.xmp", "IMG_1-1.JPG", "IMG_1.CR3", "IMG_1.JPG", "IMG_1.xmp"]
        );
        assert_eq!(std::fs::read(dest.join("IMG_1-1.CR3")).unwrap(), b"raw-b");
    }

    #[test]
    fn a_changed_sidecar_replaces_the_earlier_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = write(&tmp.path().join("IMG_1.CR3"), b"raw");
        let xmp = write(&tmp.path().join("IMG_1.xmp"), b"<x Rating=\"3\"/>");
        let dest = tmp.path().join("out");
        copy_photo(std::slice::from_ref(&raw), Some(&xmp), "IMG_1", &dest).unwrap();
        // New stars: same size, later modification time.
        std::fs::write(&xmp, b"<x Rating=\"5\"/>").unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(60);
        std::fs::File::options().write(true).open(&xmp).unwrap().set_modified(later).unwrap();
        copy_photo(std::slice::from_ref(&raw), Some(&xmp), "IMG_1", &dest).unwrap();
        assert_eq!(names(&dest), ["IMG_1.CR3", "IMG_1.xmp"], "no temp file, no numbered copy");
        assert_eq!(std::fs::read(dest.join("IMG_1.xmp")).unwrap(), b"<x Rating=\"5\"/>");

        // A sidecar of another photo, alone at the destination, is never replaced.
        let other = tmp.path().join("other");
        write(&other.join("IMG_1.xmp"), b"someone else's");
        copy_photo(std::slice::from_ref(&raw), Some(&xmp), "IMG_1", &other).unwrap();
        assert_eq!(names(&other), ["IMG_1-1.CR3", "IMG_1-1.xmp", "IMG_1.xmp"]);
        assert_eq!(std::fs::read(other.join("IMG_1.xmp")).unwrap(), b"someone else's");
    }

    fn asset(dir: &Path, stem: &str, sidecar: bool) -> PhotoAsset {
        let file = |ext: &str, kind| SourceFile {
            path: write(&dir.join(format!("{stem}.{ext}")), ext.as_bytes()),
            kind,
            size: 3,
            modified_ms: 1,
        };
        PhotoAsset {
            id: stem.into(),
            rel_path: format!("{stem}.CR3"),
            stem: stem.into(),
            primary: file("CR3", FileKind::Raw),
            companions: vec![file("JPG", FileKind::Jpeg)],
            xmp_sidecar: sidecar.then(|| write(&dir.join(format!("{stem}.xmp")), b"xmp")),
        }
    }

    #[test]
    fn moves_are_all_or_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let a = asset(tmp.path(), "IMG_1", true);
        let moved = move_photo(&a, "_Rejets").unwrap();
        let rejects = tmp.path().join("_Rejets");
        assert_eq!(moved.primary.path, rejects.join("IMG_1.CR3"));
        assert_eq!(moved.companions[0].path, rejects.join("IMG_1.JPG"));
        assert_eq!(moved.xmp_sidecar.as_deref(), Some(rejects.join("IMG_1.xmp").as_path()));
        assert_eq!(names(&rejects), [cull_decode::EXPORT_MARKER, "IMG_1.CR3", "IMG_1.JPG", "IMG_1.xmp"]);
        // Again: already there, nothing to do.
        assert_eq!(move_photo(&moved, "_Rejets").unwrap().primary.path, moved.primary.path);

        // The JPEG twin's target is taken: nothing moves.
        let b = asset(tmp.path(), "IMG_2", true);
        write(&rejects.join("IMG_2.JPG"), b"someone else's");
        assert!(move_photo(&b, "_Rejets").is_err());
        for p in [&b.primary.path, &b.companions[0].path, b.xmp_sidecar.as_ref().unwrap()] {
            assert!(p.exists(), "{} left in place", p.display());
        }
        assert!(!rejects.join("IMG_2.CR3").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_move_puts_the_moved_files_back() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: no arguments, no side effect.
        if unsafe { libc::geteuid() } == 0 {
            return; // root ignores folder permissions
        }
        let tmp = tempfile::tempdir().unwrap();
        let mut c = asset(tmp.path(), "IMG_3", false);
        // The twin sits in a read-only folder: it cannot leave it.
        let locked = tmp.path().join("locked");
        let twin = write(&locked.join("IMG_3.JPG"), b"jpg");
        std::fs::create_dir_all(locked.join("_Rejets")).unwrap();
        c.companions[0].path = twin.clone();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = move_photo(&c, "_Rejets");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
        assert!(c.primary.path.exists(), "the RAW went back");
        assert!(twin.exists());
        assert!(!tmp.path().join("_Rejets/IMG_3.CR3").exists());
    }
}
