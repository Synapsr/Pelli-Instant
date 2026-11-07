//! Internal error type: parsers report what went wrong without knowing the
//! file path; the public API attaches it when converting to [`DecodeError`].

use std::io;
use std::path::Path;

use crate::DecodeError;

#[derive(Debug)]
pub(crate) enum Fault {
    Io(io::Error),
    Corrupt(String),
    NoPreview,
    Unsupported(String),
}

pub(crate) type FResult<T> = std::result::Result<T, Fault>;

impl From<io::Error> for Fault {
    fn from(e: io::Error) -> Fault {
        Fault::Io(e)
    }
}

impl Fault {
    pub(crate) fn corrupt(reason: impl Into<String>) -> Fault {
        Fault::Corrupt(reason.into())
    }

    pub(crate) fn into_error(self, path: &Path) -> DecodeError {
        let p = path.display().to_string();
        match self {
            Fault::Io(source) => DecodeError::Io { path: p, source },
            Fault::Corrupt(reason) => DecodeError::Corrupt { path: p, reason },
            Fault::NoPreview => DecodeError::NoPreview(p),
            Fault::Unsupported(what) => DecodeError::Unsupported(format!("{what} ({p})")),
        }
    }
}

/// Runs a third-party decoder, turning a panic on hostile input into an error.
///
/// Our own parsers never panic on bad data; this is defence in depth around
/// external decoders fed with bytes straight from the file.
pub(crate) fn guard<T>(what: &str, f: impl FnOnce() -> FResult<T>) -> FResult<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => Err(Fault::Corrupt(format!("{what} decoder failed on malformed data"))),
    }
}
