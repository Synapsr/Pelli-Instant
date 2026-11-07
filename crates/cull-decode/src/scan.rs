//! Folder scanning: photo files, RAW+JPEG pairing, XMP sidecars.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use cull_types::{FileKind, PhotoAsset, SourceFile};

use crate::{DecodeError, Result, ScanOptions};

/// Deepest folder nesting we walk (protects against pathological trees).
const MAX_DEPTH: usize = 64;

/// System folders never worth walking, whatever the options say.
const SYSTEM_DIRS: &[&str] = &["$recycle.bin", "system volume information", "@eadir", "#recycle", "lost+found"];

/// Files that are never photos even with an image extension.
fn is_ignored_file(name: &str) -> bool {
    name.starts_with('.') // hidden, `._*` AppleDouble, `.DS_Store`
        || name.eq_ignore_ascii_case("thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
}

#[derive(Debug, Clone)]
struct Found {
    name: String,
    stem: String,
    file: SourceFile,
}

struct Walk<'a> {
    opts: &'a ScanOptions,
    skip_prefixes: Vec<String>,
    /// Canonical root, to keep symlinked files inside the tree.
    canon_root: PathBuf,
    assets: Vec<PhotoAsset>,
}

pub(crate) fn scan_folder(root: &Path, opts: &ScanOptions) -> Result<Vec<PhotoAsset>> {
    let io_err = |source| DecodeError::Io { path: root.display().to_string(), source };
    let canon_root = fs::canonicalize(root).map_err(io_err)?;
    if !canon_root.is_dir() {
        return Err(DecodeError::Io {
            path: root.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a folder"),
        });
    }
    let mut walk = Walk {
        opts,
        skip_prefixes: opts.skip_dir_prefixes.iter().map(|p| p.to_lowercase()).filter(|p| !p.is_empty()).collect(),
        canon_root,
        assets: Vec::new(),
    };
    walk.dir(root, "", 0, true)?;
    let mut assets = walk.assets;
    assets.sort_by(|a, b| natural_cmp(&a.rel_path, &b.rel_path));
    Ok(assets)
}

impl Walk<'_> {
    fn dir(&mut self, dir: &Path, rel: &str, depth: usize, is_root: bool) -> Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(source) if is_root => return Err(DecodeError::Io { path: dir.display().to_string(), source }),
            Err(e) => {
                log::warn!("skipping unreadable folder {}: {e}", dir.display());
                return Ok(());
            }
        };
        let mut photos: Vec<Found> = Vec::new();
        // Lower-cased sidecar file name → path.
        let mut sidecars: HashMap<String, PathBuf> = HashMap::new();
        let mut subdirs: Vec<(String, PathBuf)> = Vec::new();

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            let meta = if ft.is_symlink() {
                // Symlinked folders are never followed (outside the tree they
                // escape it, inside it they duplicate it). Symlinked files are
                // kept when they resolve inside the tree.
                match fs::canonicalize(&path) {
                    Ok(target) if target.starts_with(&self.canon_root) => match fs::metadata(&target) {
                        Ok(m) if m.is_file() => m,
                        _ => continue,
                    },
                    _ => continue,
                }
            } else if ft.is_dir() {
                if self.opts.recursive
                    && depth < MAX_DEPTH
                    && !self.skip_dir(&name)
                    && !is_hidden_entry(&entry)
                    && !path.join(crate::EXPORT_MARKER).exists()
                {
                    subdirs.push((name, path));
                }
                continue;
            } else if ft.is_file() {
                match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                }
            } else {
                continue;
            };
            if is_ignored_file(&name) || is_hidden_meta(&meta) {
                continue;
            }
            let (stem, ext) = match name.rfind('.') {
                Some(i) if i > 0 => (&name[..i], &name[i + 1..]),
                _ => continue,
            };
            if ext.eq_ignore_ascii_case("xmp") {
                sidecars.insert(name.to_lowercase(), path);
                continue;
            }
            let Some(kind) = FileKind::from_extension(ext) else { continue };
            let modified_ms = meta
                .modified()
                .ok()
                .map(|t| match t.duration_since(UNIX_EPOCH) {
                    Ok(d) => d.as_millis() as i64,
                    Err(e) => -(e.duration().as_millis() as i64),
                })
                .unwrap_or(0);
            photos.push(Found {
                stem: stem.to_string(),
                name: name.clone(),
                file: SourceFile { path, kind, size: meta.len(), modified_ms },
            });
        }

        self.emit(rel, photos, &sidecars);

        for (name, path) in subdirs {
            let sub_rel = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            self.dir(&path, &sub_rel, depth + 1, false)?;
        }
        Ok(())
    }

    fn skip_dir(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        SYSTEM_DIRS.contains(&lower.as_str()) || self.skip_prefixes.iter().any(|p| lower.starts_with(p.as_str()))
    }

    /// Turns the photos of one folder into assets.
    fn emit(&mut self, rel: &str, mut photos: Vec<Found>, sidecars: &HashMap<String, PathBuf>) {
        photos.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        let mut groups: Vec<(String, Vec<Found>)> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for p in photos {
            let key = p.stem.to_lowercase();
            match index.get(&key) {
                Some(&i) => groups[i].1.push(p),
                None => {
                    index.insert(key.clone(), groups.len());
                    groups.push((key, vec![p]));
                }
            }
        }
        for (key, files) in groups {
            let assets =
                if self.opts.pair_raw_jpeg { pair(files) } else { files.into_iter().map(|f| (f, vec![])).collect() };
            // `IMG_1.xmp` belongs to the RAW when RAW and JPEG are separate
            // assets (that is where Lightroom/Capture One write it).
            let stem_owner = assets.iter().position(|(p, _)| p.file.kind == FileKind::Raw).unwrap_or(0);
            for (i, (primary, companions)) in assets.into_iter().enumerate() {
                let exact = sidecars.get(&format!("{}.xmp", primary.name.to_lowercase()));
                let by_stem = (i == stem_owner).then(|| sidecars.get(&format!("{key}.xmp"))).flatten();
                let rel_path = if rel.is_empty() { primary.name.clone() } else { format!("{rel}/{}", primary.name) };
                self.assets.push(PhotoAsset {
                    id: asset_id(&rel_path),
                    rel_path,
                    stem: primary.stem.clone(),
                    primary: primary.file,
                    companions: companions.into_iter().map(|c| c.file).collect(),
                    xmp_sidecar: exact.or(by_stem).cloned(),
                });
            }
        }
    }
}

/// Files sharing a stem → assets. A RAW absorbs every other image of its
/// stem; a JPEG absorbs its HEIF twin; anything else stays separate.
fn pair(mut files: Vec<Found>) -> Vec<(Found, Vec<Found>)> {
    if files.len() == 1 {
        return files.into_iter().map(|f| (f, vec![])).collect();
    }
    let raw_rank = |f: &Found| -> Option<u8> {
        (f.file.kind == FileKind::Raw).then(|| {
            // Prefer the camera's own RAW over a converted DNG.
            let dng = f.name.to_lowercase().ends_with(".dng");
            u8::from(dng)
        })
    };
    if let Some(i) = (0..files.len()).filter(|&i| raw_rank(&files[i]).is_some()).min_by_key(|&i| raw_rank(&files[i])) {
        let primary = files.remove(i);
        return vec![(primary, files)];
    }
    let all_jpeg_heif = files.iter().all(|f| matches!(f.file.kind, FileKind::Jpeg | FileKind::Heif));
    let jpegs = files.iter().filter(|f| f.file.kind == FileKind::Jpeg).count();
    if all_jpeg_heif && jpegs == 1 {
        let i = files.iter().position(|f| f.file.kind == FileKind::Jpeg).unwrap_or(0);
        let primary = files.remove(i);
        return vec![(primary, files)];
    }
    files.into_iter().map(|f| (f, vec![])).collect()
}

/// Hex SHA-1 of the ASCII-lowercased relative path.
pub(crate) fn asset_id(rel_path: &str) -> String {
    sha1_smol::Sha1::from(rel_path.to_ascii_lowercase().as_bytes()).digest().to_string()
}

#[cfg(windows)]
fn is_hidden_meta(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden_meta(_meta: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn is_hidden_entry(entry: &fs::DirEntry) -> bool {
    entry.metadata().map(|m| is_hidden_meta(&m)).unwrap_or(false)
}

#[cfg(not(windows))]
fn is_hidden_entry(_entry: &fs::DirEntry) -> bool {
    false
}

/// Natural ordering of `/`-separated paths: folder by folder, digit runs
/// compared as numbers (IMG_2 < IMG_10), letters case-insensitively.
pub(crate) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.split('/');
    let mut bi = b.split('/');
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match natural_cmp_part(x, y) {
                Ordering::Equal => {}
                o => return o,
            },
        }
    }
}

fn natural_cmp_part(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na = trim_zeros(&a[si..i]);
            let nb = trim_zeros(&b[sj..j]);
            match na.len().cmp(&nb.len()).then_with(|| na.cmp(nb)) {
                Ordering::Equal => {}
                o => return o,
            }
        } else {
            match a[i].to_ascii_lowercase().cmp(&b[j].to_ascii_lowercase()) {
                Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
                o => return o,
            }
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

fn trim_zeros(d: &[u8]) -> &[u8] {
    let k = d.iter().position(|&c| c != b'0').unwrap_or(d.len());
    &d[k..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v =
            vec!["IMG_10.CR3", "img_2.cr3", "IMG_1.CR3", "a/IMG_3.jpg", "IMG_002.JPG", "B/x.jpg", "a/IMG_20.jpg"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            vec!["a/IMG_3.jpg", "a/IMG_20.jpg", "B/x.jpg", "IMG_1.CR3", "img_2.cr3", "IMG_002.JPG", "IMG_10.CR3"]
        );
        assert_eq!(natural_cmp("x", "x"), Ordering::Equal);
        assert_eq!(natural_cmp("DSC_0099", "DSC_0100"), Ordering::Less);
    }

    #[test]
    fn ids() {
        assert_eq!(asset_id("Day1/IMG_0001.CR3"), asset_id("day1/img_0001.cr3"));
        // SHA-1("abc")
        assert_eq!(asset_id("ABC"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
}
