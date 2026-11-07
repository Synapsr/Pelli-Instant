//! XMP sidecar round trips against Lightroom Classic, Capture One, Bridge
//! (old `xap:` prefix, BOM, xpacket) and darktable samples.

use cull_analysis::internals::{xmp_properties, xmp_sidecar_path};
use cull_analysis::{write_xmp_sidecar, AnalysisError, XmpPayload, XmpSource};
use cull_types::{Assessment, ColorLabel, Flag, Reason, ScoreBreakdown, UserMark, Verdict};
use std::path::{Path, PathBuf};

const XMP: &str = "http://ns.adobe.com/xap/1.0/";
const PELLI: &str = "https://pelli.io/ns/cull/1.0/";
const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xmp").join(name)).unwrap()
}

fn assessment(score: f32, verdict: Verdict, reasons: &[Reason]) -> Assessment {
    Assessment {
        score,
        verdict,
        reasons: reasons.to_vec(),
        group_id: Some(3),
        group_rank: 1,
        breakdown: ScoreBreakdown::default(),
    }
}

/// Rating and label chosen by the photographer.
fn payload(flag: Flag, rating: u8, label: ColorLabel, a: Option<Assessment>) -> XmpPayload {
    XmpPayload {
        mark: UserMark { flag, rating, label },
        rating_source: XmpSource::Photographer,
        label_source: XmpSource::Photographer,
        assessment: a,
    }
}

/// Rating and label mapped from Pelli's decision.
fn decision(rating: u8, label: ColorLabel) -> XmpPayload {
    XmpPayload {
        rating_source: XmpSource::Decision,
        label_source: XmpSource::Decision,
        ..payload(Flag::None, rating, label, Some(assessment(64.0, Verdict::Keep, &[])))
    }
}

fn get(props: &[(String, String, String)], ns: &str, local: &str) -> Vec<String> {
    props.iter().filter(|p| p.0 == ns && p.1 == local).map(|p| p.2.clone()).collect()
}

fn one(props: &[(String, String, String)], ns: &str, local: &str) -> Option<String> {
    let v = get(props, ns, local);
    assert!(v.len() <= 1, "{local} present {} times: {v:?}", v.len());
    v.into_iter().next()
}

/// Lines of `before` that the update must not touch are all in `after`, in order.
fn assert_lines_preserved(before: &str, after: &str, owned: &[&str]) {
    let mut rest = after.lines();
    for line in before.lines() {
        if owned.iter().any(|o| line.contains(o)) {
            continue;
        }
        assert!(rest.any(|l| l == line), "line lost or changed: {line:?}\n---\n{after}");
    }
}

fn setup(dir: &Path, sidecar_name: &str, content: &[u8]) -> PathBuf {
    std::fs::write(dir.join(sidecar_name), content).unwrap();
    dir.join("IMG_0001.CR3")
}

#[test]
fn creates_a_new_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("IMG_0001.CR3");
    let a = assessment(72.44, Verdict::Keep, &[]);
    let path = write_xmp_sidecar(&photo, &payload(Flag::Pick, 4, ColorLabel::Green, Some(a))).unwrap();
    assert_eq!(path, dir.path().join("IMG_0001.xmp"));
    let bytes = std::fs::read(&path).unwrap();
    let p = xmp_properties(&bytes).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("4"));
    assert_eq!(one(&p, XMP, "Label").as_deref(), Some("Green"));
    assert_eq!(one(&p, PELLI, "Score").as_deref(), Some("72.4"));
    assert_eq!(one(&p, PELLI, "Verdict").as_deref(), Some("keep"));
    assert_eq!(one(&p, PELLI, "Reasons").as_deref(), Some(""));
    assert_eq!(one(&p, PELLI, "Flag").as_deref(), Some("pick"));
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("xmlns:pelli=\"https://pelli.io/ns/cull/1.0/\""));
    assert!(text.starts_with("<x:xmpmeta"));
}

#[test]
fn updates_lightroom_classic_attribute_form_and_keeps_develop_settings() {
    let dir = tempfile::tempdir().unwrap();
    let original = fixture("lightroom-classic.xmp");
    let photo = setup(dir.path(), "IMG_0001.xmp", &original);
    let a = assessment(31.0, Verdict::Reject, &[Reason::Blurry, Reason::EyesClosed]);
    let path = write_xmp_sidecar(&photo, &payload(Flag::None, 1, ColorLabel::Red, Some(a))).unwrap();
    assert_eq!(path, dir.path().join("IMG_0001.xmp"));
    let after = std::fs::read_to_string(&path).unwrap();
    let p = xmp_properties(after.as_bytes()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("1"));
    assert_eq!(one(&p, XMP, "Label").as_deref(), Some("Red"));
    assert_eq!(one(&p, PELLI, "Verdict").as_deref(), Some("reject"));
    assert_eq!(one(&p, PELLI, "Reasons").as_deref(), Some("blurry,eyesClosed"));
    assert_eq!(one(&p, PELLI, "Flag"), None);
    // Develop settings and everything else untouched, in place.
    assert_eq!(one(&p, CRS, "Exposure2012").as_deref(), Some("+0.35"));
    assert_eq!(one(&p, CRS, "CropAngle").as_deref(), Some("-0.62"));
    let before = String::from_utf8(original).unwrap();
    assert_lines_preserved(&before, &after, &["xmp:Rating=", "xmp:Label=", "crs:AlreadyApplied"]);
    assert!(after.contains("   xmp:Rating=\"1\"\n   xmp:Label=\"Red\"\n"), "updated in place:\n{after}");
    // New attributes follow the file's layout (one per line, same indent).
    assert!(after.contains(
        "crs:AlreadyApplied=\"False\"\n    xmlns:pelli=\"https://pelli.io/ns/cull/1.0/\"\n   pelli:Score=\"31.0\""
    ));
    // The nested crs:Look rdf:Description is not mistaken for the main one.
    assert_eq!(after.matches("pelli:Score").count(), 1);
    assert!(after.contains("<rdf:li>Brest &amp; Finistère</rdf:li>"));
}

#[test]
fn updates_capture_one_element_form_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let original = fixture("capture-one.xmp");
    let photo = setup(dir.path(), "IMG_0001.xmp", &original);
    let a = assessment(88.0, Verdict::Keep, &[]);
    write_xmp_sidecar(&photo, &payload(Flag::None, 2, ColorLabel::Yellow, Some(a))).unwrap();
    let after = std::fs::read_to_string(dir.path().join("IMG_0001.xmp")).unwrap();
    assert!(after.contains("         <xmp:Rating>2</xmp:Rating>\n"), "{after}");
    assert!(after.contains("         <xmp:Label>Yellow</xmp:Label>\n"));
    assert!(!after.contains("xmp:Rating=\""), "no duplicate attribute form");
    let p = xmp_properties(after.as_bytes()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("2"));
    assert_eq!(one(&p, PELLI, "Score").as_deref(), Some("88.0"));
    let before = String::from_utf8(original).unwrap();
    assert_lines_preserved(&before, &after, &["<xmp:Rating>", "<xmp:Label>", "xmlns:Iptc4xmpCore"]);
    assert!(after.starts_with("<?xpacket begin=\"\u{feff}\""));
    assert!(after.trim_end().ends_with("<?xpacket end=\"w\"?>"));
    assert!(after.contains("© Studio Lumière &amp; associés"));
}

#[test]
fn keeps_bom_and_old_xap_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let original = fixture("bridge-xap.xmp");
    let photo = setup(dir.path(), "IMG_0001.xmp", &original);
    write_xmp_sidecar(&photo, &payload(Flag::None, 5, ColorLabel::Purple, None)).unwrap();
    let after = std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap();
    assert!(after.starts_with(&[0xEF, 0xBB, 0xBF]), "BOM kept");
    let text = String::from_utf8(after[3..].to_vec()).unwrap();
    assert!(text.contains("<xap:Rating>5</xap:Rating>"), "{text}");
    assert!(text.contains("xap:Label=\"Purple\""), "label added with the file's prefix: {text}");
    assert!(!text.contains("xmlns:xmp="), "no second binding of the XMP namespace");
    assert!(text.contains("<!-- written by Bridge, keep me -->"));
    assert!(text.contains("\r\n"), "line endings kept");
    let p = xmp_properties(&after).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("5"));
    assert_eq!(one(&p, XMP, "Label").as_deref(), Some("Purple"));
    assert_eq!(one(&p, PELLI, "Score"), None);
}

#[test]
fn updates_darktable_raw_dot_xmp_name() {
    let dir = tempfile::tempdir().unwrap();
    let photo = setup(dir.path(), "IMG_0001.CR3.xmp", &fixture("darktable.xmp"));
    assert_eq!(xmp_sidecar_path(&photo), Some(dir.path().join("IMG_0001.CR3.xmp")));
    let path = write_xmp_sidecar(&photo, &payload(Flag::Reject, 0, ColorLabel::None, None)).unwrap();
    assert_eq!(path, dir.path().join("IMG_0001.CR3.xmp"));
    assert!(!dir.path().join("IMG_0001.xmp").exists());
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("   xmp:Rating=\"-1\"\n"), "{after}");
    assert!(after.contains("darktable:blendop_params=\"gz12eJxjYGBgkGAAgRNODGiAEV0AJ2iwh+CRfOyAAYIHAQ==\"/>"));
    assert!(after.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    let p = xmp_properties(after.as_bytes()).unwrap();
    assert_eq!(one(&p, PELLI, "Flag").as_deref(), Some("reject"));
}

#[test]
fn reject_flag_and_stars() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("a.NEF");
    let path = write_xmp_sidecar(&photo, &payload(Flag::Reject, 0, ColorLabel::None, None)).unwrap();
    let p = xmp_properties(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("-1"));
    // Stars set by the photographer win over the reject flag.
    write_xmp_sidecar(&photo, &payload(Flag::Reject, 3, ColorLabel::None, None)).unwrap();
    let p = xmp_properties(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("3"));
    // Unflagged, no stars: rating 0.
    write_xmp_sidecar(&photo, &payload(Flag::None, 0, ColorLabel::None, None)).unwrap();
    let p = xmp_properties(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("0"));
    assert_eq!(one(&p, PELLI, "Flag"), None);
}

#[test]
fn assessment_none_removes_pelli_values_and_label_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let photo = setup(dir.path(), "IMG_0001.xmp", &fixture("capture-one.xmp"));
    let a = assessment(40.0, Verdict::Maybe, &[Reason::Duplicate]);
    write_xmp_sidecar(&photo, &payload(Flag::Pick, 4, ColorLabel::None, Some(a))).unwrap();
    let mid = std::fs::read_to_string(dir.path().join("IMG_0001.xmp")).unwrap();
    assert!(mid.contains("pelli:Reasons=\"duplicate\""));
    write_xmp_sidecar(&photo, &payload(Flag::None, 4, ColorLabel::None, None)).unwrap();
    let after = std::fs::read_to_string(dir.path().join("IMG_0001.xmp")).unwrap();
    let p = xmp_properties(after.as_bytes()).unwrap();
    for local in ["Score", "Verdict", "Reasons", "Flag"] {
        assert_eq!(one(&p, PELLI, local), None, "{local} removed:\n{after}");
    }
    // No label in the payload: the one set in Capture One stays.
    assert_eq!(one(&p, XMP, "Label").as_deref(), Some("Red"));
}

#[test]
fn removes_element_form_pelli_values_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let xml = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\" xmlns:p=\"https://pelli.io/ns/cull/1.0/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n   <p:Score>12</p:Score>\n   <p:Reasons><rdf:Bag><rdf:li>blurry</rdf:li></rdf:Bag></p:Reasons>\n   <xmp:Rating/>\n  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n";
    let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
    write_xmp_sidecar(&photo, &payload(Flag::None, 2, ColorLabel::None, None)).unwrap();
    let after = std::fs::read_to_string(dir.path().join("IMG_0001.xmp")).unwrap();
    assert!(!after.contains("p:Score") && !after.contains("p:Reasons"), "{after}");
    assert!(after.contains("   <xmp:Rating>2</xmp:Rating>\n  </rdf:Description>"), "{after}");
}

#[test]
fn writing_twice_is_idempotent_and_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let photo = setup(dir.path(), "IMG_0001.xmp", &fixture("lightroom-classic.xmp"));
    let p = payload(Flag::Pick, 5, ColorLabel::Blue, Some(assessment(91.25, Verdict::Keep, &[])));
    write_xmp_sidecar(&photo, &p).unwrap();
    let first = std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap();
    write_xmp_sidecar(&photo, &p).unwrap();
    let second = std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap();
    assert_eq!(first, second);
    let names: Vec<String> =
        std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["IMG_0001.xmp".to_string()], "no temp file left");
}

#[test]
fn unreadable_sidecar_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in [
        ("broken.xmp", b"<x:xmpmeta><rdf:RDF><rdf:Description></x:xmpmeta>".as_slice()),
        ("not-xmp.xmp", b"<html><body>hello</body></html>".as_slice()),
        ("utf16.xmp", &[0xFF, 0xFE, b'<', 0, b'x', 0][..]),
    ] {
        let sidecar = dir.path().join(name);
        std::fs::write(&sidecar, content).unwrap();
        let photo = sidecar.with_extension("ARW");
        let err = write_xmp_sidecar(&photo, &payload(Flag::None, 3, ColorLabel::None, None)).unwrap_err();
        assert!(matches!(err, AnalysisError::InvalidXmp { .. }), "{name}: {err}");
        assert_eq!(std::fs::read(&sidecar).unwrap(), content, "{name} untouched");
    }
}

#[test]
fn empty_sidecar_is_replaced_and_missing_folder_is_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let photo = setup(dir.path(), "IMG_0001.xmp", b"  \n");
    write_xmp_sidecar(&photo, &payload(Flag::None, 3, ColorLabel::None, None)).unwrap();
    let p = xmp_properties(&std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("3"));
    let err = write_xmp_sidecar(&dir.path().join("missing/IMG_2.CR3"), &payload(Flag::None, 1, ColorLabel::None, None))
        .unwrap_err();
    assert!(matches!(err, AnalysisError::Io { .. }));
}

#[test]
fn rdf_without_description_gets_one() {
    let dir = tempfile::tempdir().unwrap();
    let xml = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"></rdf:RDF></x:xmpmeta>";
    let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
    write_xmp_sidecar(&photo, &payload(Flag::None, 2, ColorLabel::Green, None)).unwrap();
    let after = std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap();
    let p = xmp_properties(&after).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("2"));
    assert_eq!(one(&p, XMP, "Label").as_deref(), Some("Green"));

    let xml = "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>";
    let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
    write_xmp_sidecar(&photo, &payload(Flag::None, 4, ColorLabel::None, None)).unwrap();
    let p = xmp_properties(&std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("4"));
}

#[test]
fn prefix_clash_gets_a_fresh_prefix() {
    let dir = tempfile::tempdir().unwrap();
    // `pelli` already bound to someone else's namespace.
    let xml = "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:pelli=\"urn:other\" pelli:Score=\"keep me\"/></rdf:RDF>";
    let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
    write_xmp_sidecar(&photo, &payload(Flag::None, 1, ColorLabel::None, Some(assessment(55.0, Verdict::Maybe, &[]))))
        .unwrap();
    let after = std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap();
    let p = xmp_properties(&after).unwrap();
    assert_eq!(one(&p, "urn:other", "Score").as_deref(), Some("keep me"));
    assert_eq!(one(&p, PELLI, "Score").as_deref(), Some("55.0"));
    assert!(String::from_utf8(after).unwrap().contains("xmlns:pelli1=\"https://pelli.io/ns/cull/1.0/\""));
}

#[test]
fn a_decision_never_replaces_lightroom_rating_and_label() {
    let dir = tempfile::tempdir().unwrap();
    let photo = setup(dir.path(), "IMG_0001.xmp", &fixture("lightroom-classic.xmp"));
    write_xmp_sidecar(&photo, &decision(1, ColorLabel::Red)).unwrap();
    let p = xmp_properties(&std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("3"));
    assert_eq!(one(&p, XMP, "Label").as_deref(), Some("Green"));
    assert_eq!((one(&p, PELLI, "Rating"), one(&p, PELLI, "Label")), (None, None), "nothing written by Pelli");
    assert_eq!(one(&p, PELLI, "Verdict").as_deref(), Some("keep"));
    // Nor Capture One's, in element form.
    let photo = setup(dir.path(), "IMG_0001.xmp", &fixture("capture-one.xmp"));
    write_xmp_sidecar(&photo, &decision(1, ColorLabel::None)).unwrap();
    let p = xmp_properties(&std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap()).unwrap();
    assert_eq!((one(&p, XMP, "Rating").as_deref(), one(&p, XMP, "Label").as_deref()), (Some("4"), Some("Red")));

    // The photographer's own choice in Pelli wins.
    let photo = setup(dir.path(), "IMG_0001.xmp", &fixture("lightroom-classic.xmp"));
    let mut own = decision(5, ColorLabel::Green);
    own.rating_source = XmpSource::Photographer;
    write_xmp_sidecar(&photo, &own).unwrap();
    let p = xmp_properties(&std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap()).unwrap();
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("5"));
    assert_eq!(one(&p, PELLI, "Rating").as_deref(), Some("5"));
    assert_eq!(one(&p, PELLI, "Label"), None, "label still Lightroom's");

    // Unrated (0) counts as no rating.
    let xml = "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmp:Rating=\"0\"/></rdf:RDF>";
    let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
    write_xmp_sidecar(&photo, &decision(2, ColorLabel::Yellow)).unwrap();
    let p = xmp_properties(&std::fs::read(dir.path().join("IMG_0001.xmp")).unwrap()).unwrap();
    assert_eq!((one(&p, XMP, "Rating").as_deref(), one(&p, XMP, "Label").as_deref()), (Some("2"), Some("Yellow")));
}

#[test]
fn values_pelli_wrote_follow_the_decision() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("IMG_0001.CR3");
    let path = write_xmp_sidecar(&photo, &decision(3, ColorLabel::Green)).unwrap();
    let props = || xmp_properties(&std::fs::read(&path).unwrap()).unwrap();
    let p = props();
    assert_eq!((one(&p, XMP, "Rating").as_deref(), one(&p, XMP, "Label").as_deref()), (Some("3"), Some("Green")));
    assert_eq!((one(&p, PELLI, "Rating").as_deref(), one(&p, PELLI, "Label").as_deref()), (Some("3"), Some("Green")));
    // Re-export after another decision: Pelli's values are updated.
    write_xmp_sidecar(&photo, &decision(2, ColorLabel::Yellow)).unwrap();
    let p = props();
    assert_eq!((one(&p, XMP, "Rating").as_deref(), one(&p, XMP, "Label").as_deref()), (Some("2"), Some("Yellow")));
    assert_eq!(one(&p, PELLI, "Rating").as_deref(), Some("2"));
    // A decision without label removes the one Pelli wrote.
    write_xmp_sidecar(&photo, &decision(2, ColorLabel::None)).unwrap();
    let p = props();
    assert_eq!((one(&p, XMP, "Label"), one(&p, PELLI, "Label")), (None, None));
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("2"));
}

#[test]
fn values_changed_in_lightroom_after_pelli_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("IMG_0001.CR3");
    let path = write_xmp_sidecar(&photo, &decision(3, ColorLabel::Green)).unwrap();
    // The photographer gives 5 stars and a blue label in Lightroom.
    let text = std::fs::read_to_string(&path).unwrap();
    let edited =
        text.replace("xmp:Rating=\"3\"", "xmp:Rating=\"5\"").replace("xmp:Label=\"Green\"", "xmp:Label=\"Blue\"");
    assert_ne!(edited, text);
    std::fs::write(&path, edited).unwrap();
    write_xmp_sidecar(&photo, &decision(2, ColorLabel::Yellow)).unwrap();
    let p = xmp_properties(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!((one(&p, XMP, "Rating").as_deref(), one(&p, XMP, "Label").as_deref()), (Some("5"), Some("Blue")));
    // Removing them in Lightroom is a choice too.
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace(" xmp:Label=\"Blue\"", "").replace("\n   xmp:Label=\"Blue\"", "")).unwrap();
    write_xmp_sidecar(&photo, &decision(2, ColorLabel::Yellow)).unwrap();
    let p = xmp_properties(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(one(&p, XMP, "Label"), None);
}

#[test]
fn read_only_sidecar_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let original = fixture("lightroom-classic.xmp");
    let photo = setup(dir.path(), "IMG_0001.xmp", &original);
    let sidecar = dir.path().join("IMG_0001.xmp");
    let mut perms = std::fs::metadata(&sidecar).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&sidecar, perms).unwrap();
    let err = write_xmp_sidecar(&photo, &payload(Flag::Pick, 5, ColorLabel::Blue, None)).unwrap_err();
    assert!(
        matches!(&err, AnalysisError::Io { source, .. } if source.kind() == std::io::ErrorKind::PermissionDenied),
        "{err}"
    );
    assert_eq!(std::fs::read(&sidecar).unwrap(), original);
    let names: Vec<String> =
        std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["IMG_0001.xmp"], "no temp file left");
    #[allow(clippy::permissions_set_readonly_false)]
    {
        let mut perms = std::fs::metadata(&sidecar).unwrap().permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(&sidecar, perms).unwrap();
    }
}
