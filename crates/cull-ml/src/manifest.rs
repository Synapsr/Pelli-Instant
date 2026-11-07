//! Model manifest (`models/manifest.json`, compiled into the
//! binary so the expected checksums can't be swapped on disk) and SHA-256
//! verification of the model files.
//!
//! Hashing ~250 MB of models takes ~0.15 s with SHA-NI / ARMv8 crypto
//! extensions but ~0.6 s without (Intel Macs before 2020), so a verified
//! file is remembered in `<models_dir>/.verified.json`, keyed by file name +
//! size + modification time. The cache is best-effort: a read-only models
//! directory (signed app bundle) just means hashing at each launch.

use crate::{MlError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

const MANIFEST_JSON: &str = include_str!("../../../models/manifest.json");
const CACHE_FILE: &str = ".verified.json";

/// Where the checksum cache lives when the models directory must stay
/// untouched (models inside a signed app bundle): set from
/// `RuntimeOptions::checksum_cache_dir`.
static CACHE_DIR: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

pub(crate) fn set_cache_dir(dir: Option<PathBuf>) {
    if let Ok(mut d) = CACHE_DIR.write() {
        *d = dir;
    }
}

fn cache_file(models_dir: &Path) -> PathBuf {
    match CACHE_DIR.read().ok().and_then(|d| d.clone()) {
        Some(d) => d.join(CACHE_FILE),
        None => models_dir.join(CACHE_FILE),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelEntry {
    pub id: String,
    /// Short identifier stored in `MlObservations::models`.
    pub tag: String,
    /// File name inside the models directory.
    pub file: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub required: bool,
    /// `default` (shipped with the app), `gpu`, `maximum` (optional pack).
    pub bundle: String,
    pub url: String,
    #[serde(default)]
    pub mirrors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    models: Vec<ModelEntry>,
}

static MANIFEST: OnceLock<std::result::Result<Vec<ModelEntry>, String>> = OnceLock::new();

fn manifest() -> Result<&'static [ModelEntry]> {
    MANIFEST
        .get_or_init(|| serde_json::from_str::<Manifest>(MANIFEST_JSON).map(|m| m.models).map_err(|e| e.to_string()))
        .as_deref()
        .map_err(|e| MlError::Runtime(format!("invalid embedded models/manifest.json: {e}")))
}

/// Every model of the manifest.
pub(crate) fn entries() -> Result<&'static [ModelEntry]> {
    manifest()
}

/// Entries of one bundle (`maximum`: the optional Maximum pack).
pub(crate) fn bundle(name: &str) -> Result<Vec<&'static ModelEntry>> {
    Ok(manifest()?.iter().filter(|e| e.bundle == name).collect())
}

/// Manifest entry of model `id`.
pub(crate) fn entry(id: &str) -> Result<&'static ModelEntry> {
    manifest()?
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| MlError::Runtime(format!("model '{id}' not in models/manifest.json")))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheEntry {
    size: u64,
    mtime_ns: u128,
    sha256: String,
}

fn mtime_ns(meta: &std::fs::Metadata) -> u128 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0)
}

fn read_cache(dir: &Path) -> BTreeMap<String, CacheEntry> {
    std::fs::read(cache_file(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_cache(dir: &Path, cache: &BTreeMap<String, CacheEntry>) {
    let Ok(json) = serde_json::to_vec_pretty(cache) else { return };
    let target = cache_file(dir);
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = target.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &target).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// A model file read from disk and verified.
pub(crate) struct VerifiedModel {
    pub entry: &'static ModelEntry,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// Verify the file of model `id` in `dir` without keeping it in memory
/// (large models: hashed in 1 MiB blocks; sessions are then built from the
/// file). `Ok(None)` when the file does not exist.
pub(crate) fn verify_file(dir: &Path, id: &str) -> Result<Option<(&'static ModelEntry, PathBuf)>> {
    let entry = entry(id)?;
    let path = dir.join(&entry.file);
    let meta = match std::fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        _ => return Ok(None),
    };
    if meta.len() != entry.size {
        return Err(MlError::Checksum(format!("{} (size {} instead of {})", entry.file, meta.len(), entry.size)));
    }
    let mtime = mtime_ns(&meta);
    let mut cache = read_cache(dir);
    let cached = cache
        .get(&entry.file)
        .is_some_and(|c| c.size == entry.size && c.mtime_ns == mtime && c.sha256.eq_ignore_ascii_case(&entry.sha256));
    if !cached {
        let actual =
            sha256_file(&path).map_err(|e| MlError::Runtime(format!("cannot read {}: {e}", path.display())))?;
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(MlError::Checksum(entry.file.clone()));
        }
        cache.insert(entry.file.clone(), CacheEntry { size: entry.size, mtime_ns: mtime, sha256: actual });
        write_cache(dir, &cache);
    }
    Ok(Some((entry, path)))
}

/// SHA-256 (hex) of a file, read in 1 MiB blocks.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

fn hex(digest: &[u8]) -> String {
    let mut s = String::with_capacity(2 * digest.len());
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Read and verify the file of model `id` in `dir`. `Ok(None)` when the file
/// does not exist; `Err(Checksum)` when it exists but differs from the
/// manifest (truncated download, wrong version…).
pub(crate) fn load_verified(dir: &Path, id: &str) -> Result<Option<VerifiedModel>> {
    let entry = entry(id)?;
    let path = dir.join(&entry.file);
    let meta = match std::fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        _ => return Ok(None),
    };
    if meta.len() != entry.size {
        return Err(MlError::Checksum(format!("{} (size {} instead of {})", entry.file, meta.len(), entry.size)));
    }
    let bytes = std::fs::read(&path).map_err(|e| MlError::Runtime(format!("cannot read {}: {e}", path.display())))?;
    if bytes.len() as u64 != entry.size {
        return Err(MlError::Checksum(entry.file.clone()));
    }
    let mtime = mtime_ns(&meta);
    let mut cache = read_cache(dir);
    let cached = cache
        .get(&entry.file)
        .is_some_and(|c| c.size == entry.size && c.mtime_ns == mtime && c.sha256.eq_ignore_ascii_case(&entry.sha256));
    if !cached {
        let t = std::time::Instant::now();
        let actual = sha256_hex(&bytes);
        log::debug!("cull-ml: sha256 of {} in {:?}", entry.file, t.elapsed());
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(MlError::Checksum(entry.file.clone()));
        }
        cache.insert(entry.file.clone(), CacheEntry { size: entry.size, mtime_ns: mtime, sha256: actual });
        write_cache(dir, &cache);
    }
    Ok(Some(VerifiedModel { entry, path, bytes }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_and_has_required_yunet() {
        let m = manifest().expect("manifest");
        let yunet = m.iter().find(|e| e.id == "yunet").expect("yunet entry");
        assert!(yunet.required);
        for e in m {
            assert_eq!(e.sha256.len(), 64, "{}", e.id);
            assert!(e.size > 0);
            assert!(!e.tag.is_empty());
            assert!(["default", "gpu", "maximum"].contains(&e.bundle.as_str()), "{}", e.id);
            assert!(e.url.starts_with("https://"), "{}", e.id);
            assert!(!e.required || e.bundle == "default", "{}", e.id);
        }
        let pack: Vec<&str> = bundle("maximum").unwrap().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            pack,
            ["clip-l14-vision-int8", "clip-l14-text", "dinov2-b14-int8", "rfdetr-seg-s", "retinaface-r50"]
        );
    }

    #[test]
    fn streamed_hash_matches() {
        let dir = std::env::temp_dir().join(format!("cull-ml-sha-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bytes: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(dir.join("f"), &bytes).unwrap();
        assert_eq!(sha256_file(&dir.join("f")).unwrap(), sha256_hex(&bytes));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sha256_known_vector() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn missing_and_corrupt_files() {
        let dir = std::env::temp_dir().join(format!("cull-ml-manifest-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load_verified(&dir, "yunet").unwrap().is_none());
        let e = entry("yunet").unwrap();
        std::fs::write(dir.join(&e.file), b"not a model").unwrap();
        assert!(matches!(load_verified(&dir, "yunet"), Err(MlError::Checksum(_))));
        // Same size, wrong content.
        std::fs::write(dir.join(&e.file), vec![0u8; e.size as usize]).unwrap();
        assert!(matches!(load_verified(&dir, "yunet"), Err(MlError::Checksum(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
