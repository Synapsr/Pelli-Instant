//! XMP sidecar round trips against Lightroom Classic, Capture One, Bridge
//! (old `xap:` prefix, BOM, xpacket) and darktable samples.

use cull_analysis::internals::{xmp_properties, xmp_sidecar_path};
use cull_analysis::{write_xmp_sidecar, AnalysisError, XmpPayload, XmpSource};
use cull_types::{Assessment, ColorLabel, Flag, LabelLanguage, Reason, ScoreBreakdown, Verdict};
use std::path::{Path, PathBuf};

const XMP: &str = "http://ns.adobe.com/xap/1.0/";
const PELLI: &str = "https://pelli.io/ns/cull/1.0/";
const DM: &str = "http://ns.adobe.com/xmp/1.0/DynamicMedia/";
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

/// Rating, label and flag chosen by the photographer (labels in English).
fn payload(flag: Flag, rating: i8, label: ColorLabel, a: Option<Assessment>) -> XmpPayload {
    XmpPayload {
        rating,
        rating_source: XmpSource::Photographer,
        label,
        label_source: XmpSource::Photographer,
        label_language: LabelLanguage::En,
        flag,
        flag_source: if flag == Flag::None { XmpSource::Decision } else { XmpSource::Photographer },
        assessment: a,
    }
}

/// Rating and label mapped from Pelli's decision, no flag.
fn decision(rating: i8, label: ColorLabel) -> XmpPayload {
    XmpPayload {
        rating_source: XmpSource::Decision,
        label_source: XmpSource::Decision,
        ..payload(Flag::None, rating, label, Some(assessment(64.0, Verdict::Keep, &[])))
    }
}

/// Lightroom's flag from Pelli's decision.
fn flagged(flag: Flag) -> XmpPayload {
    XmpPayload { flag, flag_source: XmpSource::Decision, ..decision(0, ColorLabel::None) }
}

fn props_of(path: &Path) -> Vec<(String, String, String)> {
    xmp_properties(&std::fs::read(path).unwrap()).unwrap()
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
    assert_eq!((one(&p, DM, "pick").as_deref(), one(&p, DM, "good").as_deref()), (Some("1"), Some("true")));
    assert_eq!((one(&p, PELLI, "Label").as_deref(), one(&p, PELLI, "Pick").as_deref()), (Some("green"), Some("1")));
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("xmlns:pelli=\"https://pelli.io/ns/cull/1.0/\""));
    assert!(text.contains("\n    xmlns:xmpDM=\"http://ns.adobe.com/xmp/1.0/DynamicMedia/\"\n"), "{text}");
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
    let path = write_xmp_sidecar(&photo, &payload(Flag::Reject, -1, ColorLabel::None, None)).unwrap();
    assert_eq!(path, dir.path().join("IMG_0001.CR3.xmp"));
    assert!(!dir.path().join("IMG_0001.xmp").exists());
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("   xmp:Rating=\"-1\"\n"), "{after}");
    assert!(after.contains("darktable:blendop_params=\"gz12eJxjYGBgkGAAgRNODGiAEV0AJ2iwh+CRfOyAAYIHAQ==\"/>"));
    assert!(after.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    let p = xmp_properties(after.as_bytes()).unwrap();
    assert_eq!(one(&p, PELLI, "Flag").as_deref(), Some("reject"));
    assert_eq!((one(&p, DM, "pick").as_deref(), one(&p, DM, "good").as_deref()), (Some("-1"), Some("false")));
}

#[test]
fn reject_flag_and_stars() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("a.NEF");
    // Rejected: Bridge's -1 rating, Lightroom's reject flag.
    let path = write_xmp_sidecar(&photo, &payload(Flag::Reject, -1, ColorLabel::None, None)).unwrap();
    let p = props_of(&path);
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("-1"));
    assert_eq!((one(&p, DM, "pick").as_deref(), one(&p, DM, "good").as_deref()), (Some("-1"), Some("false")));
    // Stars and the flag are independent: rejected with 3 stars.
    write_xmp_sidecar(&photo, &payload(Flag::Reject, 3, ColorLabel::None, None)).unwrap();
    let p = props_of(&path);
    assert_eq!((one(&p, XMP, "Rating").as_deref(), one(&p, DM, "pick").as_deref()), (Some("3"), Some("-1")));
    // A rating out of range is clamped.
    write_xmp_sidecar(&photo, &payload(Flag::Reject, -4, ColorLabel::None, None)).unwrap();
    assert_eq!(one(&props_of(&path), XMP, "Rating").as_deref(), Some("-1"));
    // Unflagged, no stars: rating 0, and the flag Pelli wrote goes back to
    // none, the way Lightroom writes it (pick 0, no good).
    write_xmp_sidecar(&photo, &payload(Flag::None, 0, ColorLabel::None, None)).unwrap();
    let p = props_of(&path);
    assert_eq!(one(&p, XMP, "Rating").as_deref(), Some("0"));
    assert_eq!(one(&p, PELLI, "Flag"), None);
    assert_eq!((one(&p, DM, "pick").as_deref(), one(&p, DM, "good")), (Some("0"), None));
    assert_eq!(one(&p, PELLI, "Pick").as_deref(), Some("0"));
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
    assert_eq!((one(&p, PELLI, "Rating").as_deref(), one(&p, PELLI, "Label").as_deref()), (Some("3"), Some("green")));
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

#[test]
fn labels_are_written_in_lightrooms_language() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("IMG_0001.CR3");
    let in_language = |language, rating, label| XmpPayload { label_language: language, ..decision(rating, label) };
    let path = write_xmp_sidecar(&photo, &in_language(LabelLanguage::Fr, 3, ColorLabel::Green)).unwrap();
    let p = props_of(&path);
    assert_eq!((one(&p, XMP, "Label").as_deref(), one(&p, PELLI, "Label").as_deref()), (Some("Vert"), Some("green")));
    // Writing again changes nothing.
    let first = std::fs::read(&path).unwrap();
    write_xmp_sidecar(&photo, &in_language(LabelLanguage::Fr, 3, ColorLabel::Green)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), first);
    // Another language later: Pelli's label, in French, is Pelli's still.
    write_xmp_sidecar(&photo, &in_language(LabelLanguage::De, 3, ColorLabel::Green)).unwrap();
    assert_eq!(one(&props_of(&path), XMP, "Label").as_deref(), Some("Grün"));
    write_xmp_sidecar(&photo, &in_language(LabelLanguage::Pl, 3, ColorLabel::Yellow)).unwrap();
    assert_eq!(one(&props_of(&path), XMP, "Label").as_deref(), Some("Żółty"));
    // A decision without label removes it, in whatever language it is.
    write_xmp_sidecar(&photo, &in_language(LabelLanguage::It, 3, ColorLabel::None)).unwrap();
    let p = props_of(&path);
    assert_eq!((one(&p, XMP, "Label"), one(&p, PELLI, "Label")), (None, None));

    // The photographer's own label in Pelli is written in the language too.
    let own =
        XmpPayload { label_source: XmpSource::Photographer, ..in_language(LabelLanguage::Nl, 0, ColorLabel::Purple) };
    write_xmp_sidecar(&photo, &own).unwrap();
    assert_eq!(one(&props_of(&path), XMP, "Label").as_deref(), Some("Paars"));
}

#[test]
fn english_labels_of_earlier_versions_are_replaced() {
    let dir = tempfile::tempdir().unwrap();
    // What the first versions wrote: English name, recorded as such.
    let xml = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\"\n    xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n    xmlns:pelli=\"https://pelli.io/ns/cull/1.0/\"\n   xmp:Rating=\"3\"\n   xmp:Label=\"Green\"\n   pelli:Rating=\"3\"\n   pelli:Label=\"Green\"/>\n </rdf:RDF>\n</x:xmpmeta>\n";
    let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
    let fr = XmpPayload { label_language: LabelLanguage::Fr, ..decision(3, ColorLabel::Green) };
    write_xmp_sidecar(&photo, &fr).unwrap();
    let after = std::fs::read_to_string(dir.path().join("IMG_0001.xmp")).unwrap();
    assert!(after.contains("   xmp:Label=\"Vert\"\n   pelli:Rating=\"3\"\n   pelli:Label=\"green\"\n"), "{after}");
}

#[test]
fn labels_that_are_not_pellis_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let fr = |label| XmpPayload { label_language: LabelLanguage::Fr, ..decision(2, label) };
    let sidecar = |label: &str, record: &str| {
        format!(
            "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:pelli=\"https://pelli.io/ns/cull/1.0/\" xmp:Label=\"{label}\"{record}/></rdf:RDF>"
        )
    };
    let label_after = |xml: String, payload: XmpPayload| {
        let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
        write_xmp_sidecar(&photo, &payload).unwrap();
        one(&props_of(&dir.path().join("IMG_0001.xmp")), XMP, "Label")
    };
    // Set in Lightroom before Pelli, whatever the text: a colour of a French
    // Lightroom, a custom label set's name.
    assert_eq!(label_after(sidecar("Rouge", ""), fr(ColorLabel::Green)).as_deref(), Some("Rouge"));
    assert_eq!(label_after(sidecar("Client pick", ""), fr(ColorLabel::Green)).as_deref(), Some("Client pick"));
    // Changed in Lightroom after Pelli: another colour, or other text.
    let pellis = " pelli:Label=\"green\"";
    assert_eq!(label_after(sidecar("Bleu", pellis), fr(ColorLabel::Yellow)).as_deref(), Some("Bleu"));
    assert_eq!(label_after(sidecar("Client pick", pellis), fr(ColorLabel::Yellow)).as_deref(), Some("Client pick"));
    assert_eq!(label_after(sidecar("Client pick", pellis), fr(ColorLabel::None)).as_deref(), Some("Client pick"));
    // Still Pelli's green, in Italian: replaced.
    assert_eq!(label_after(sidecar("Verde", pellis), fr(ColorLabel::Yellow)).as_deref(), Some("Jaune"));
    // The photographer's own label in Pelli wins over Lightroom's.
    let own = XmpPayload { label_source: XmpSource::Photographer, ..fr(ColorLabel::Blue) };
    assert_eq!(label_after(sidecar("Client pick", ""), own).as_deref(), Some("Bleu"));
}

#[test]
fn lightroom_flags_follow_the_ownership_rule() {
    let dir = tempfile::tempdir().unwrap();
    let flag_of = |p: &[(String, String, String)]| (one(p, DM, "pick"), one(p, DM, "good"));
    let some = |pick: &str, good: Option<&str>| (Some(pick.to_string()), good.map(str::to_string));

    // A new sidecar: the decision's flag, recorded.
    let photo = dir.path().join("IMG_0001.CR3");
    let path = write_xmp_sidecar(&photo, &flagged(Flag::Reject)).unwrap();
    let p = props_of(&path);
    assert_eq!(flag_of(&p), some("-1", Some("false")));
    assert_eq!((one(&p, PELLI, "Pick").as_deref(), one(&p, PELLI, "Flag")), (Some("-1"), None), "no own flag");
    // Another decision: Pelli's flag follows it.
    write_xmp_sidecar(&photo, &flagged(Flag::Pick)).unwrap();
    assert_eq!(flag_of(&props_of(&path)), some("1", Some("true")));
    // To review: back to none, the way Lightroom writes it.
    write_xmp_sidecar(&photo, &flagged(Flag::None)).unwrap();
    assert_eq!(flag_of(&props_of(&path)), some("0", None));
    // The photographer rejects it in Lightroom: a decision no longer touches it.
    write_xmp_sidecar(&photo, &flagged(Flag::Pick)).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("xmpDM:pick=\"1\"", "xmpDM:pick=\"-1\"").replace("\"true\"", "\"false\""))
        .unwrap();
    write_xmp_sidecar(&photo, &flagged(Flag::Pick)).unwrap();
    write_xmp_sidecar(&photo, &flagged(Flag::None)).unwrap();
    assert_eq!(flag_of(&props_of(&path)), some("-1", Some("false")));
    // Unless they flag it in Pelli Instant: their own flag always wins.
    let own = XmpPayload { flag_source: XmpSource::Photographer, ..flagged(Flag::Pick) };
    write_xmp_sidecar(&photo, &own).unwrap();
    let p = props_of(&path);
    assert_eq!(flag_of(&p), some("1", Some("true")));
    assert_eq!(one(&p, PELLI, "Flag").as_deref(), Some("pick"));

    // Sidecars Lightroom wrote before Pelli.
    let lightroom = |attrs: &str| {
        format!(
            "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:xmpDM=\"http://ns.adobe.com/xmp/1.0/DynamicMedia/\"{attrs}/></rdf:RDF>"
        )
    };
    let flag_after = |xml: String, payload: XmpPayload| {
        let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
        write_xmp_sidecar(&photo, &payload).unwrap();
        flag_of(&props_of(&dir.path().join("IMG_0001.xmp")))
    };
    // Rejected in Lightroom: kept.
    assert_eq!(
        flag_after(lightroom(" xmpDM:pick=\"-1\" xmpDM:good=\"false\""), flagged(Flag::Pick)),
        some("-1", Some("false"))
    );
    // Unflagged in Lightroom (pick 0): counts as none.
    assert_eq!(flag_after(lightroom(" xmpDM:pick=\"0\""), flagged(Flag::Pick)), some("1", Some("true")));
    // Picked in Lightroom desktop (good only): kept, nothing added.
    assert_eq!(flag_after(lightroom(" xmpDM:good=\"True\""), flagged(Flag::Reject)), (None, Some("True".into())));
    // To review, on a sidecar Pelli never flagged: nothing written.
    assert_eq!(flag_after(lightroom(""), flagged(Flag::None)), (None, None));
    let p = props_of(&dir.path().join("IMG_0001.xmp"));
    assert_eq!(one(&p, PELLI, "Pick"), None);
    // The photographer's reject in Pelli replaces Lightroom's pick.
    let own = XmpPayload { flag_source: XmpSource::Photographer, ..flagged(Flag::Reject) };
    assert_eq!(flag_after(lightroom(" xmpDM:pick=\"1\" xmpDM:good=\"true\""), own), some("-1", Some("false")));
}

#[test]
fn flags_declare_the_dynamic_media_namespace_once() {
    let dir = tempfile::tempdir().unwrap();
    // A sidecar Lightroom Classic wrote without flags: xmpDM is not bound.
    let original = fixture("lightroom-classic.xmp");
    let photo = setup(dir.path(), "IMG_0001.xmp", &original);
    let own = XmpPayload { flag_source: XmpSource::Photographer, ..payload(Flag::Pick, 4, ColorLabel::None, None) };
    write_xmp_sidecar(&photo, &own).unwrap();
    let after = std::fs::read_to_string(dir.path().join("IMG_0001.xmp")).unwrap();
    assert!(
        after.contains(
            "crs:AlreadyApplied=\"False\"\n    xmlns:xmpDM=\"http://ns.adobe.com/xmp/1.0/DynamicMedia/\"\n    xmlns:pelli=\"https://pelli.io/ns/cull/1.0/\"\n   xmpDM:pick=\"1\"\n   xmpDM:good=\"true\"\n   pelli:Flag=\"pick\""
        ),
        "{after}"
    );
    assert_eq!(after.matches("xmlns:xmpDM=").count(), 1);
    let before = String::from_utf8(original).unwrap();
    assert_lines_preserved(&before, &after, &["xmp:Rating=", "crs:AlreadyApplied"]);
    let p = xmp_properties(after.as_bytes()).unwrap();
    assert_eq!((one(&p, DM, "pick").as_deref(), one(&p, XMP, "Label").as_deref()), (Some("1"), Some("Green")));
    // Again, then rejected: updated in place, never declared twice.
    write_xmp_sidecar(&photo, &own).unwrap();
    let reject = XmpPayload { flag: Flag::Reject, ..own.clone() };
    write_xmp_sidecar(&photo, &reject).unwrap();
    let again = std::fs::read_to_string(dir.path().join("IMG_0001.xmp")).unwrap();
    assert_eq!(again.matches("xmlns:xmpDM=").count(), 1, "{again}");
    assert!(again.contains("   xmpDM:pick=\"-1\"\n   xmpDM:good=\"false\"\n"), "{again}");

    // Element form, under another prefix, as some tools write it.
    let xml = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\" xmlns:dm=\"http://ns.adobe.com/xmp/1.0/DynamicMedia/\">\n   <dm:pick>0</dm:pick>\n   <dm:videoFrameRate>25</dm:videoFrameRate>\n  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n";
    let photo = setup(dir.path(), "IMG_0001.xmp", xml.as_bytes());
    write_xmp_sidecar(&photo, &flagged(Flag::Reject)).unwrap();
    let after = std::fs::read_to_string(dir.path().join("IMG_0001.xmp")).unwrap();
    assert!(after.contains("   <dm:pick>-1</dm:pick>\n   <dm:videoFrameRate>25</dm:videoFrameRate>\n"), "{after}");
    assert!(after.contains(" dm:good=\"false\"") && !after.contains("xmlns:xmpDM"), "{after}");
}
