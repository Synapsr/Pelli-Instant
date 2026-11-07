//! Random-access byte sources with minimal I/O.
//!
//! Photographers cull straight from USB card readers and NAS shares, where
//! every read is a round trip. Container parsers therefore never read a file
//! sequentially: they ask for small ranges (headers, IFDs, tag values) which
//! [`FileSource`] serves from a tiny cache of 64 KiB blocks, and for the one
//! big range they need (the chosen embedded JPEG), which bypasses the cache.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::fault::{FResult, Fault};

/// Size of a cached block. Large enough to hold the TIFF header and first IFDs
/// of every RAW format in one read, small enough to stay cheap on a NAS.
const BLOCK: u64 = 64 * 1024;
/// Number of cached blocks (1 MiB in total).
const CACHE_BLOCKS: usize = 16;
/// Ranges at least this large skip the cache and are read directly.
const DIRECT_READ: usize = 2 * BLOCK as usize;

/// Random access to the bytes of a file or of an in-memory buffer.
pub(crate) trait ByteSource {
    /// Total length in bytes.
    fn len(&self) -> u64;

    /// Exactly `len` bytes at `offset`; a range past the end is corrupt data.
    fn read_at(&mut self, offset: u64, len: usize) -> FResult<Vec<u8>>;

    /// Up to `len` bytes at `offset`, truncated at the end of the source.
    fn read_upto(&mut self, offset: u64, len: usize) -> FResult<Vec<u8>> {
        let total = self.len();
        if offset >= total {
            return Ok(Vec::new());
        }
        let avail = usize::try_from(total - offset).unwrap_or(usize::MAX);
        self.read_at(offset, len.min(avail))
    }

    #[cfg(test)]
    fn read_u16_be(&mut self, offset: u64) -> FResult<u16> {
        let b = self.read_at(offset, 2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn read_u32_be(&mut self, offset: u64) -> FResult<u32> {
        let b = self.read_at(offset, 4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u64_be(&mut self, offset: u64) -> FResult<u64> {
        let b = self.read_at(offset, 8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(&b);
        Ok(u64::from_be_bytes(a))
    }
}

/// Checks that `offset..offset+len` lies inside a source of `total` bytes.
pub(crate) fn check_range(offset: u64, len: u64, total: u64) -> FResult<()> {
    match offset.checked_add(len) {
        Some(end) if end <= total => Ok(()),
        _ => Err(Fault::Corrupt(format!("range {offset}+{len} is past the end of the data ({total} bytes)"))),
    }
}

/// A file read through a small block cache.
pub(crate) struct FileSource {
    file: File,
    len: u64,
    /// `(block index, bytes)`, most recently used last.
    cache: Vec<(u64, Vec<u8>)>,
    /// Bytes actually read from the file (diagnostics and I/O tests).
    bytes_read: u64,
    reads: u32,
}

impl FileSource {
    pub(crate) fn open(path: &Path) -> io::Result<FileSource> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(FileSource { file, len, cache: Vec::with_capacity(CACHE_BLOCKS), bytes_read: 0, reads: 0 })
    }

    /// Bytes read from disk so far.
    pub(crate) fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Number of read calls issued to the OS so far.
    #[cfg(test)]
    pub(crate) fn reads(&self) -> u32 {
        self.reads
    }

    fn read_direct(&mut self, offset: u64, len: usize) -> FResult<Vec<u8>> {
        self.file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len];
        self.file.read_exact(&mut buf).map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                Fault::Corrupt("file truncated while reading".into())
            } else {
                Fault::Io(e)
            }
        })?;
        self.bytes_read += len as u64;
        self.reads += 1;
        Ok(buf)
    }

    fn block(&mut self, index: u64) -> FResult<&[u8]> {
        if let Some(pos) = self.cache.iter().position(|(i, _)| *i == index) {
            let entry = self.cache.remove(pos);
            self.cache.push(entry);
        } else {
            let start = index * BLOCK;
            let len = usize::try_from(BLOCK.min(self.len.saturating_sub(start))).unwrap_or(0);
            let data = self.read_direct(start, len)?;
            if self.cache.len() == CACHE_BLOCKS {
                self.cache.remove(0);
            }
            self.cache.push((index, data));
        }
        // The entry we just touched is the last one.
        Ok(self.cache.last().map(|(_, d)| d.as_slice()).unwrap_or(&[]))
    }
}

impl ByteSource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&mut self, offset: u64, len: usize) -> FResult<Vec<u8>> {
        check_range(offset, len as u64, self.len)?;
        if len >= DIRECT_READ {
            return self.read_direct(offset, len);
        }
        let mut out = Vec::with_capacity(len);
        let mut pos = offset;
        let end = offset + len as u64;
        while pos < end {
            let index = pos / BLOCK;
            let within = (pos - index * BLOCK) as usize;
            let block = self.block(index)?;
            if within >= block.len() {
                return Err(Fault::Corrupt("file changed while reading".into()));
            }
            let take = (block.len() - within).min((end - pos) as usize);
            out.extend_from_slice(&block[within..within + take]);
            pos += take as u64;
        }
        Ok(out)
    }
}

/// An in-memory buffer (embedded TIFF blocks, whole small files, tests).
pub(crate) struct MemSource<'a> {
    data: &'a [u8],
}

impl<'a> MemSource<'a> {
    pub(crate) fn new(data: &'a [u8]) -> MemSource<'a> {
        MemSource { data }
    }
}

impl ByteSource for MemSource<'_> {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&mut self, offset: u64, len: usize) -> FResult<Vec<u8>> {
        check_range(offset, len as u64, self.data.len() as u64)?;
        let start = offset as usize;
        Ok(self.data[start..start + len].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn mem_source_bounds() {
        let data = [1u8, 2, 3, 4];
        let mut s = MemSource::new(&data);
        assert_eq!(s.read_at(1, 2).unwrap(), vec![2, 3]);
        assert!(s.read_at(3, 2).is_err());
        assert!(s.read_at(u64::MAX, 2).is_err());
        assert_eq!(s.read_upto(2, 10).unwrap(), vec![3, 4]);
        assert!(s.read_upto(10, 10).unwrap().is_empty());
        assert_eq!(s.read_u16_be(0).unwrap(), 0x0102);
    }

    #[test]
    fn file_source_caches_blocks() {
        let dir = std::env::temp_dir().join(format!("cull-decode-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blocks.bin");
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::File::create(&path).unwrap().write_all(&data).unwrap();

        let mut s = FileSource::open(&path).unwrap();
        assert_eq!(s.len(), data.len() as u64);
        // Two small reads in the same block: one disk read.
        assert_eq!(s.read_at(10, 4).unwrap(), data[10..14].to_vec());
        assert_eq!(s.read_at(1000, 8).unwrap(), data[1000..1008].to_vec());
        assert_eq!(s.reads(), 1);
        // A read spanning two blocks.
        let span = s.read_at(BLOCK - 3, 6).unwrap();
        assert_eq!(span, data[(BLOCK - 3) as usize..(BLOCK + 3) as usize].to_vec());
        assert_eq!(s.reads(), 2);
        // A big read goes direct.
        let big = s.read_at(5, 200_000).unwrap();
        assert_eq!(big[..], data[5..200_005]);
        // Last partial block and past-the-end reads.
        let tail = s.read_upto(299_990, 100).unwrap();
        assert_eq!(tail[..], data[299_990..]);
        assert!(s.read_at(299_990, 11).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
