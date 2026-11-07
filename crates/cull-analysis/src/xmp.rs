//! XMP sidecars for Lightroom Classic, Capture One, Bridge and darktable.
//!
//! The existing sidecar is never re-serialised: it is tokenised with
//! `quick-xml` (which also checks it is well-formed) and only the bytes of
//! the properties we own are spliced, so develop settings, keywords, history,
//! comments, the `<?xpacket?>` wrapper, padding, indentation and the BOM are
//! kept byte for byte.
//!
//! Owned properties, matched by namespace URI (whatever prefix the file
//! binds, e.g. the old `xap:` prefix) in both XMP serialisations — attribute
//! form `<rdf:Description xmp:Rating="3">` and element form
//! `<xmp:Rating>3</xmp:Rating>`:
//!
//! * `xmp:Rating` — the stars. Lightroom has no XMP field for its
//!   pick/reject flags, so a photo the photographer rejected *without*
//!   giving it stars is written `-1`, the "rejected" rating of Bridge and
//!   Lightroom's "Rejected" rating filter; stars win over the reject flag.
//! * `xmp:Label` — the colour label.
//!
//!   Both may have been set in Lightroom, Capture One… A value the
//!   photographer chose in Pelli Instant ([`XmpSource::Photographer`]) is
//!   always written. A value mapped from Pelli's decision
//!   ([`XmpSource::Decision`]) only replaces Pelli's own: every value Pelli
//!   writes is recorded in `pelli:Rating` / `pelli:Label`, and a decision's
//!   value is written when the sidecar still holds the one Pelli wrote last,
//!   or when Pelli never wrote one and the sidecar has none (rating `0`
//!   counts as none). A value set in another tool before Pelli, or changed
//!   there since, is kept. A decision without label removes the label Pelli
//!   wrote, if it is still there; the photographer's choice without label
//!   leaves the label alone.
//! * `pelli:Flag` (`pick` / `reject`, removed when unflagged), `pelli:Score`
//!   (0..100), `pelli:Verdict` (`keep` / `maybe` / `reject`), `pelli:Reasons`
//!   (comma-separated, e.g. `blurry,eyesClosed`) in
//!   `xmlns:pelli="https://pelli.io/ns/cull/1.0/"`; the last three are
//!   removed when the payload has no assessment.
//!
//! Existing values are updated in place, in the form they already have;
//! new properties are added as attributes of the first `rdf:Description`
//! (declaring their namespace there when needed), separated by the same
//! whitespace as the attributes before them. The result is parsed again
//! before being written; the file is replaced atomically (temp file in the
//! same folder, fsync, rename). An unreadable sidecar (not XML, no
//! `rdf:RDF`, UTF-16) is left untouched and reported as `InvalidXmp`; a
//! read-only one (locked on purpose, and Windows cannot replace it) is left
//! untouched too, as a `PermissionDenied` I/O error.

use crate::{AnalysisError, Result, XmpPayload, XmpSource};
use cull_types::{Flag, Reason, Verdict};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const XMP_NS: &str = "http://ns.adobe.com/xap/1.0/";
pub(crate) const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub(crate) const PELLI_NS: &str = "https://pelli.io/ns/cull/1.0/";
const X_NS: &str = "adobe:ns:meta/";

/// One owned property: `None` removes it.
#[derive(Debug, Clone)]
pub(crate) struct Prop {
    pub ns: &'static str,
    pub local: &'static str,
    pub value: Option<String>,
}

fn verdict_name(v: Verdict) -> &'static str {
    match v {
        Verdict::Keep => "keep",
        Verdict::Maybe => "maybe",
        Verdict::Reject => "reject",
    }
}

fn reason_name(r: Reason) -> &'static str {
    match r {
        Reason::Blurry => "blurry",
        Reason::MotionBlur => "motionBlur",
        Reason::SubjectOutOfFocus => "subjectOutOfFocus",
        Reason::EyesClosed => "eyesClosed",
        Reason::Underexposed => "underexposed",
        Reason::Overexposed => "overexposed",
        Reason::Duplicate => "duplicate",
        Reason::ExactDuplicate => "exactDuplicate",
        Reason::LowAesthetic => "lowAesthetic",
    }
}

/// Value of a property of the sidecar (from [`read_props`]; empty = none).
fn value<'a>(existing: &'a [(String, String, String)], ns: &str, local: &str) -> Option<&'a str> {
    existing.iter().find(|p| p.0 == ns && p.1 == local).map(|p| p.2.trim()).filter(|v| !v.is_empty())
}

/// Whether a value mapped from the decision may replace `xmp:<local>`: the
/// sidecar still holds the one Pelli wrote last (`pelli:<local>`), or Pelli
/// never wrote one and the sidecar has none (`unset`: values meaning none).
fn decision_may_write(existing: &[(String, String, String)], local: &str, unset: &[&str]) -> bool {
    let current = value(existing, XMP_NS, local);
    match value(existing, PELLI_NS, local) {
        Some(written) => current == Some(written),
        None => current.is_none_or(|v| unset.contains(&v)),
    }
}

/// The properties to write over `existing` (the sidecar's current ones).
pub(crate) fn props_for(payload: &XmpPayload, existing: &[(String, String, String)]) -> Vec<Prop> {
    let m = &payload.mark;
    let mut props = Vec::new();
    // `xmp:` values written, recorded in `pelli:` (after the others, so new
    // attributes keep the order score, verdict, reasons).
    let mut written = Vec::new();
    let mut write = |local: &'static str, value: Option<String>| {
        props.push(Prop { ns: XMP_NS, local, value: value.clone() });
        written.push(Prop { ns: PELLI_NS, local, value });
    };
    let rating: i32 = if m.flag == Flag::Reject && m.rating == 0 { -1 } else { m.rating.min(5) as i32 };
    if payload.rating_source == XmpSource::Photographer || decision_may_write(existing, "Rating", &["0"]) {
        write("Rating", Some(rating.to_string()));
    }
    match (m.label.xmp_name(), payload.label_source) {
        (Some(label), XmpSource::Photographer) => write("Label", Some(label.to_string())),
        (Some(label), XmpSource::Decision) if decision_may_write(existing, "Label", &[]) => {
            write("Label", Some(label.to_string()))
        }
        // No label for this decision: the one Pelli wrote goes.
        (None, XmpSource::Decision)
            if value(existing, PELLI_NS, "Label").is_some() && decision_may_write(existing, "Label", &[]) =>
        {
            write("Label", None)
        }
        _ => {}
    }
    let flag = match m.flag {
        Flag::None => None,
        Flag::Pick => Some("pick".to_string()),
        Flag::Reject => Some("reject".to_string()),
    };
    props.push(Prop { ns: PELLI_NS, local: "Flag", value: flag });
    let a = payload.assessment.as_ref();
    let score = a.map(|a| {
        let s = if a.score.is_finite() { a.score.clamp(0.0, 100.0) } else { 0.0 };
        format!("{s:.1}")
    });
    props.push(Prop { ns: PELLI_NS, local: "Score", value: score });
    props.push(Prop { ns: PELLI_NS, local: "Verdict", value: a.map(|a| verdict_name(a.verdict).to_string()) });
    props.push(Prop {
        ns: PELLI_NS,
        local: "Reasons",
        value: a.map(|a| a.reasons.iter().map(|&r| reason_name(r)).collect::<Vec<_>>().join(",")),
    });
    props.extend(written);
    props
}

fn preferred_prefix(ns: &str) -> &'static str {
    match ns {
        XMP_NS => "xmp",
        PELLI_NS => "pelli",
        RDF_NS => "rdf",
        _ => "ns",
    }
}

// ---------------------------------------------------------------------------
// Paths and atomic write
// ---------------------------------------------------------------------------

/// Sidecar of `photo`: an existing `IMG_1.CR3.xmp`, else an existing
/// `IMG_1.xmp`, else `IMG_1.xmp` (upper-case `.XMP` variants are found too).
pub(crate) fn sidecar_path(photo: &Path) -> Option<PathBuf> {
    photo.file_name()?;
    let full = |ext: &str| {
        let mut s: OsString = photo.as_os_str().to_owned();
        s.push(ext);
        PathBuf::from(s)
    };
    let candidates = [full(".xmp"), full(".XMP"), photo.with_extension("xmp"), photo.with_extension("XMP")];
    for c in &candidates {
        if c.is_file() {
            return Some(c.clone());
        }
    }
    Some(photo.with_extension("xmp"))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let existing = std::fs::metadata(path).ok();
    if existing.as_ref().is_some_and(|m| m.permissions().readonly()) {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "read-only sidecar"));
    }
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let nonce =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let tmp = dir.join(format!(
        ".{name}.pelli-{}-{}-{nonce}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if let Some(meta) = &existing {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub(crate) fn write_sidecar(photo_path: &Path, payload: &XmpPayload) -> Result<PathBuf> {
    let path = sidecar_path(photo_path).ok_or_else(|| AnalysisError::Io {
        path: photo_path.display().to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "photo path has no file name"),
    })?;
    let io_err = |source| AnalysisError::Io { path: path.display().to_string(), source };
    let invalid = |reason| AnalysisError::InvalidXmp { path: path.display().to_string(), reason };
    let existing = match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_err(e)),
    };
    let bytes = match existing {
        Some(b) if !b.iter().all(|c| c.is_ascii_whitespace()) => {
            let current = read_props(&b).map_err(invalid)?;
            update_xmp(&b, &props_for(payload, &current)).map_err(invalid)?
        }
        _ => new_xmp(&props_for(payload, &[])),
    };
    atomic_write(&path, &bytes).map_err(io_err)?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Serialisation helpers
// ---------------------------------------------------------------------------

fn escape(v: &str, attr: bool) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn new_xmp(props: &[Prop]) -> Vec<u8> {
    let mut s = String::new();
    s.push_str(&format!("<x:xmpmeta xmlns:x=\"{X_NS}\" x:xmptk=\"Pelli Instant\">\n"));
    s.push_str(&format!(" <rdf:RDF xmlns:rdf=\"{RDF_NS}\">\n"));
    s.push_str("  <rdf:Description rdf:about=\"\"");
    s.push_str(&format!("\n    xmlns:xmp=\"{XMP_NS}\"\n    xmlns:pelli=\"{PELLI_NS}\""));
    for p in props {
        if let Some(v) = &p.value {
            s.push_str(&format!("\n   {}:{}=\"{}\"", preferred_prefix(p.ns), p.local, escape(v, true)));
        }
    }
    s.push_str("/>\n </rdf:RDF>\n</x:xmpmeta>\n");
    s.into_bytes()
}

// ---------------------------------------------------------------------------
// Start-tag scanner
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TagAttr {
    /// Start of the whitespace before the attribute.
    ws_start: usize,
    name: (usize, usize),
    /// Inside the quotes.
    value: (usize, usize),
    /// Just after the closing quote.
    end: usize,
}

#[derive(Debug, Clone)]
struct Tag {
    name: (usize, usize),
    attrs: Vec<TagAttr>,
}

impl Tag {
    /// Where new attributes go: after the last attribute (or the name).
    fn insert_at(&self) -> usize {
        self.attrs.last().map(|a| a.end).unwrap_or(self.name.1)
    }
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

/// Scans `<name attr="v" ...>` / `<name .../>` (offsets relative to `t`).
fn scan_tag(t: &[u8]) -> Option<Tag> {
    if t.first() != Some(&b'<') {
        return None;
    }
    let mut i = 1;
    let name_start = i;
    while i < t.len() && !is_ws(t[i]) && t[i] != b'/' && t[i] != b'>' {
        i += 1;
    }
    let name = (name_start, i);
    let mut attrs = Vec::new();
    loop {
        let ws_start = i;
        while i < t.len() && is_ws(t[i]) {
            i += 1;
        }
        if i >= t.len() {
            return None;
        }
        if t[i] == b'/' || t[i] == b'>' {
            return Some(Tag { name, attrs });
        }
        let ns = i;
        while i < t.len() && !is_ws(t[i]) && t[i] != b'=' {
            i += 1;
        }
        let ne = i;
        while i < t.len() && is_ws(t[i]) {
            i += 1;
        }
        if t.get(i) != Some(&b'=') {
            return None;
        }
        i += 1;
        while i < t.len() && is_ws(t[i]) {
            i += 1;
        }
        let q = *t.get(i)?;
        if q != b'"' && q != b'\'' {
            return None;
        }
        i += 1;
        let vs = i;
        while i < t.len() && t[i] != q {
            i += 1;
        }
        if i >= t.len() {
            return None;
        }
        let ve = i;
        i += 1;
        attrs.push(TagAttr { ws_start, name: (ns, ne), value: (vs, ve), end: i });
    }
}

fn split_qname(q: &str) -> (Option<&str>, &str) {
    match q.split_once(':') {
        Some((p, l)) => (Some(p), l),
        None => (None, q),
    }
}

// ---------------------------------------------------------------------------
// Update of an existing sidecar
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Other,
    Rdf,
    Description,
    Owned(usize),
}

struct Frame {
    role: Role,
    /// Namespace declarations of this element: (prefix, uri), "" = default.
    bindings: Vec<(String, String)>,
    start: usize,
    open_end: usize,
}

/// The `rdf:RDF` element: start of its start tag, end of its start tag,
/// start of its end tag (`None` while open or when self-closing), the
/// namespace bindings in scope inside it, its qualified name.
struct RdfInfo {
    start: usize,
    open_end: usize,
    close_start: Option<usize>,
    scope: Vec<(String, String)>,
    qname: String,
}

struct DescInfo {
    start: usize,
    tag: Tag,
    /// Bindings in scope inside this element (innermost last).
    scope: Vec<(String, String)>,
}

struct ElemInfo {
    prop: usize,
    qname: String,
    start: usize,
    /// Content span; `None` for an empty element.
    content: Option<(usize, usize)>,
    end: usize,
}

fn lookup<'a>(scope: &'a [(String, String)], prefix: &str) -> Option<&'a str> {
    scope.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.as_str())
}

fn scope_of(stack: &[Frame]) -> Vec<(String, String)> {
    stack.iter().flat_map(|f| f.bindings.iter().cloned()).collect()
}

/// Prefix bound to `ns` in `scope`, or a new free one to declare.
fn prefix_for(scope: &[(String, String)], ns: &str, declared: &mut Vec<(String, String)>) -> String {
    let all: Vec<(String, String)> = scope.iter().chain(declared.iter()).cloned().collect();
    // Innermost binding of a prefix wins: check the prefix still maps to ns.
    for (p, u) in all.iter().rev() {
        if u == ns && !p.is_empty() && lookup(&all, p) == Some(ns) {
            return p.clone();
        }
    }
    let base = preferred_prefix(ns);
    let mut candidate = base.to_string();
    let mut k = 1;
    while lookup(&all, &candidate).is_some() {
        candidate = format!("{base}{k}");
        k += 1;
    }
    declared.push((candidate.clone(), ns.to_string()));
    candidate
}

pub(crate) fn update_xmp(input: &[u8], props: &[Prop]) -> std::result::Result<Vec<u8>, String> {
    if input.starts_with(&[0xFF, 0xFE]) || input.starts_with(&[0xFE, 0xFF]) {
        return Err("UTF-16 sidecars are not supported".into());
    }
    let (bom, src) = match input.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => (&input[..3], rest),
        None => (&input[..0], input),
    };
    let text = |a: usize, b: usize| String::from_utf8_lossy(&src[a..b]).into_owned();

    let mut reader = Reader::from_reader(src);
    {
        let cfg = reader.config_mut();
        cfg.trim_text(false);
        cfg.expand_empty_elements = false;
        cfg.check_end_names = true;
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut rdf: Option<RdfInfo> = None;
    let mut rdf_empty = false;
    let mut descs: Vec<DescInfo> = Vec::new();
    let mut attr_hits: Vec<(usize, usize, usize)> = Vec::new(); // desc, attr, prop
    let mut elems: Vec<ElemInfo> = Vec::new();

    loop {
        let before = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| format!("malformed XML: {e}"))?;
        let after = reader.buffer_position() as usize;
        match event {
            Event::Start(_) | Event::Empty(_) => {
                let is_empty = matches!(event, Event::Empty(_));
                let raw = src.get(before..after).ok_or("bad offsets")?;
                let tag = scan_tag(raw).ok_or_else(|| format!("unreadable tag at byte {before}"))?;
                let mut bindings = Vec::new();
                for a in &tag.attrs {
                    let name = text(before + a.name.0, before + a.name.1);
                    let value = text(before + a.value.0, before + a.value.1);
                    if name == "xmlns" {
                        bindings.push((String::new(), value));
                    } else if let Some(p) = name.strip_prefix("xmlns:") {
                        bindings.push((p.to_string(), value));
                    }
                }
                let mut scope = scope_of(&stack);
                scope.extend(bindings.iter().cloned());
                let qname = text(before + tag.name.0, before + tag.name.1);
                let (prefix, local) = split_qname(&qname);
                let uri = lookup(&scope, prefix.unwrap_or("")).unwrap_or("");
                let parent = stack.last().map(|f| f.role);
                let role = if uri == RDF_NS && local == "RDF" {
                    Role::Rdf
                } else if uri == RDF_NS && local == "Description" && parent == Some(Role::Rdf) {
                    Role::Description
                } else if parent == Some(Role::Description) {
                    props.iter().position(|p| p.ns == uri && p.local == local).map(Role::Owned).unwrap_or(Role::Other)
                } else {
                    Role::Other
                };
                match role {
                    Role::Rdf if rdf.is_none() => {
                        rdf = Some(RdfInfo {
                            start: before,
                            open_end: after,
                            close_start: None,
                            scope: scope.clone(),
                            qname: qname.clone(),
                        });
                        rdf_empty = is_empty;
                    }
                    Role::Description => {
                        let d = descs.len();
                        for (k, a) in tag.attrs.iter().enumerate() {
                            let an = text(before + a.name.0, before + a.name.1);
                            if let (Some(p), l) = split_qname(&an) {
                                if p == "xmlns" {
                                    continue;
                                }
                                if let Some(u) = lookup(&scope, p) {
                                    if let Some(pi) = props.iter().position(|x| x.ns == u && x.local == l) {
                                        attr_hits.push((d, k, pi));
                                    }
                                }
                            }
                        }
                        descs.push(DescInfo { start: before, tag: tag.clone(), scope: scope.clone() });
                    }
                    Role::Owned(pi) if is_empty => {
                        elems.push(ElemInfo {
                            prop: pi,
                            qname: qname.clone(),
                            start: before,
                            content: None,
                            end: after,
                        });
                    }
                    _ => {}
                }
                if !is_empty {
                    stack.push(Frame { role, bindings, start: before, open_end: after });
                }
            }
            Event::End(_) => {
                let frame = stack.pop().ok_or("unbalanced end tag")?;
                match frame.role {
                    Role::Rdf => {
                        if let Some(r) = rdf.as_mut() {
                            if r.start == frame.start {
                                r.close_start = Some(before);
                            }
                        }
                    }
                    Role::Owned(pi) => {
                        let qname = {
                            let raw = &src[frame.start..frame.open_end];
                            let t = scan_tag(raw).ok_or("unreadable tag")?;
                            text(frame.start + t.name.0, frame.start + t.name.1)
                        };
                        elems.push(ElemInfo {
                            prop: pi,
                            qname,
                            start: frame.start,
                            content: Some((frame.open_end, before)),
                            end: after,
                        });
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("unclosed elements".into());
    }
    let Some(RdfInfo {
        start: rdf_start,
        open_end: rdf_open_end,
        close_start: rdf_close,
        scope: rdf_scope,
        qname: rdf_qname,
    }) = rdf
    else {
        return Err("no rdf:RDF element".into());
    };

    // Build the edits.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut to_add: Vec<(usize, String)> = Vec::new();
    for (pi, prop) in props.iter().enumerate() {
        let hits: Vec<&(usize, usize, usize)> = attr_hits.iter().filter(|h| h.2 == pi).collect();
        let ehits: Vec<&ElemInfo> = elems.iter().filter(|e| e.prop == pi).collect();
        match &prop.value {
            Some(v) => {
                for &&(d, k, _) in &hits {
                    let a = &descs[d].tag.attrs[k];
                    let base = descs[d].start;
                    edits.push((base + a.value.0, base + a.value.1, escape(v, true)));
                }
                for e in &ehits {
                    match e.content {
                        Some((a, b)) => edits.push((a, b, escape(v, false))),
                        None => edits.push((e.start, e.end, format!("<{q}>{}</{q}>", escape(v, false), q = e.qname))),
                    }
                }
                if hits.is_empty() && ehits.is_empty() {
                    to_add.push((pi, v.clone()));
                }
            }
            None => {
                for &&(d, k, _) in &hits {
                    let a = &descs[d].tag.attrs[k];
                    let base = descs[d].start;
                    edits.push((base + a.ws_start, base + a.end, String::new()));
                }
                for e in &ehits {
                    // Also drop the indentation (and newline) before the element.
                    let mut s = e.start;
                    while s > 0 && matches!(src[s - 1], b' ' | b'\t') {
                        s -= 1;
                    }
                    if s > 0 && src[s - 1] == b'\n' {
                        s -= 1;
                        if s > 0 && src[s - 1] == b'\r' {
                            s -= 1;
                        }
                    } else {
                        s = e.start;
                    }
                    edits.push((s, e.end, String::new()));
                }
            }
        }
    }

    if !to_add.is_empty() {
        if let Some(d) = descs.first() {
            let mut declared = Vec::new();
            let mut parts = Vec::new();
            for (pi, v) in &to_add {
                let p = prefix_for(&d.scope, props[*pi].ns, &mut declared);
                parts.push(format!("{p}:{}=\"{}\"", props[*pi].local, escape(v, true)));
            }
            let sep = d
                .tag
                .attrs
                .last()
                .map(|a| text(d.start + a.ws_start, d.start + a.name.0))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| " ".to_string());
            let decl_sep = if sep.contains('\n') { format!("{sep} ") } else { sep.clone() };
            let mut ins = String::new();
            for (p, u) in &declared {
                ins.push_str(&format!("{decl_sep}xmlns:{p}=\"{}\"", escape(u, true)));
            }
            for part in &parts {
                ins.push_str(&sep);
                ins.push_str(part);
            }
            let at = d.start + d.tag.insert_at();
            edits.push((at, at, ins));
        } else {
            // No rdf:Description: add one inside rdf:RDF.
            let (rdf_prefix, _) = split_qname(&rdf_qname);
            let rp = match rdf_prefix {
                Some(p) => format!("{p}:"),
                None => String::new(),
            };
            let mut declared = Vec::new();
            let mut parts = Vec::new();
            for (pi, v) in &to_add {
                let p = prefix_for(&rdf_scope, props[*pi].ns, &mut declared);
                parts.push(format!("{p}:{}=\"{}\"", props[*pi].local, escape(v, true)));
            }
            let mut desc = format!("<{rp}Description {rp}about=\"\"");
            for (p, u) in &declared {
                desc.push_str(&format!("\n    xmlns:{p}=\"{}\"", escape(u, true)));
            }
            for part in &parts {
                desc.push_str(&format!("\n   {part}"));
            }
            desc.push_str("/>");
            match rdf_close {
                Some(close) if !rdf_empty => edits.push((close, close, format!("  {desc}\n "))),
                _ => {
                    // `<rdf:RDF .../>`: reopen it.
                    let raw = &src[rdf_start..rdf_open_end];
                    let t = scan_tag(raw).ok_or("unreadable rdf:RDF tag")?;
                    let head = text(rdf_start, rdf_start + t.insert_at());
                    edits.push((rdf_start, rdf_open_end, format!("{head}>\n  {desc}\n </{rdf_qname}>")));
                }
            }
        }
    }

    edits.sort_by_key(|e| (e.0, e.1));
    let mut out = Vec::with_capacity(input.len() + 512);
    out.extend_from_slice(bom);
    let mut pos = 0;
    for (a, b, rep) in &edits {
        if *a < pos {
            return Err("overlapping edits (duplicate properties)".into());
        }
        out.extend_from_slice(&src[pos..*a]);
        out.extend_from_slice(rep.as_bytes());
        pos = *b;
    }
    out.extend_from_slice(&src[pos..]);

    // Safety net: never write a sidecar we cannot read back.
    check_well_formed(&out[bom.len()..])?;
    Ok(out)
}

fn check_well_formed(src: &[u8]) -> std::result::Result<(), String> {
    let mut reader = Reader::from_reader(src);
    reader.config_mut().check_end_names = true;
    let mut depth = 0i64;
    loop {
        match reader.read_event().map_err(|e| format!("rewrite produced malformed XML: {e}"))? {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 {
        return Err("rewrite produced unbalanced XML".into());
    }
    Ok(())
}

/// Reads back the direct properties (attribute or element form) of every
/// top-level `rdf:Description`: `(namespace, local name, value)`, values
/// unescaped; structured values (`rdf:Bag`…) come back as their trimmed raw
/// content.
pub(crate) fn read_props(input: &[u8]) -> std::result::Result<Vec<(String, String, String)>, String> {
    let src = input.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(input);
    let text = |a: usize, b: usize| String::from_utf8_lossy(&src[a..b]).into_owned();
    let mut reader = Reader::from_reader(src);
    reader.config_mut().trim_text(false);
    struct Open {
        bindings: Vec<(String, String)>,
        role: Role,
        open_end: usize,
        uri: String,
        local: String,
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut out = Vec::new();
    loop {
        let before = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let after = reader.buffer_position() as usize;
        match event {
            Event::Start(_) | Event::Empty(_) => {
                let raw = &src[before..after];
                let tag = scan_tag(raw).ok_or("unreadable tag")?;
                let mut bindings = Vec::new();
                let mut attrs = Vec::new();
                for a in &tag.attrs {
                    let name = text(before + a.name.0, before + a.name.1);
                    let value = text(before + a.value.0, before + a.value.1);
                    if name == "xmlns" {
                        bindings.push((String::new(), value));
                    } else if let Some(p) = name.strip_prefix("xmlns:") {
                        bindings.push((p.to_string(), value));
                    } else {
                        attrs.push((name, value));
                    }
                }
                let mut scope: Vec<(String, String)> = stack.iter().flat_map(|f| f.bindings.iter().cloned()).collect();
                scope.extend(bindings.iter().cloned());
                let qname = text(before + tag.name.0, before + tag.name.1);
                let (prefix, local) = split_qname(&qname);
                let uri = lookup(&scope, prefix.unwrap_or("")).unwrap_or("").to_string();
                let parent = stack.last().map(|f| f.role);
                let role = if uri == RDF_NS && local == "RDF" {
                    Role::Rdf
                } else if uri == RDF_NS && local == "Description" && parent == Some(Role::Rdf) {
                    Role::Description
                } else if parent == Some(Role::Description) {
                    Role::Owned(0)
                } else {
                    Role::Other
                };
                if role == Role::Description {
                    for (n, v) in attrs {
                        if let (Some(p), l) = split_qname(&n) {
                            if let Some(u) = lookup(&scope, p) {
                                out.push((
                                    u.to_string(),
                                    l.to_string(),
                                    quick_xml::escape::unescape(&v).map(|c| c.into_owned()).unwrap_or(v),
                                ));
                            }
                        }
                    }
                }
                if !matches!(event, Event::Empty(_)) {
                    stack.push(Open { bindings, role, open_end: after, uri, local: local.to_string() });
                } else if role == Role::Owned(0) {
                    out.push((uri, local.to_string(), String::new()));
                }
            }
            Event::End(_) => {
                if let Some(open) = stack.pop() {
                    if open.role == Role::Owned(0) {
                        let raw = text(open.open_end, before);
                        let v = quick_xml::escape::unescape(&raw).map(|c| c.into_owned()).unwrap_or(raw);
                        out.push((open.uri, open.local, v.trim().to_string()));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}
