use aitrium_radiotherapy_server::anonymize;
use aitrium_radiotherapy_server::types::RtAnonymizeMetadataRequest;
use dicom_core::value::DataSetSequence;
use dicom_core::{DataElement, VR};
use dicom_dictionary_std::tags;
use dicom_dictionary_std::uids;
use dicom_object::meta::FileMetaTableBuilder;
use dicom_object::{open_file, InMemDicomObject};
use serde_json::json;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn write_test_dicom_with_date(
    path: &std::path::Path,
    patient_name: &str,
    series_uid: &str,
    study_date: &str,
) {
    let nested = InMemDicomObject::from_element_iter([DataElement::new(
        tags::PATIENT_NAME,
        VR::PN,
        "Nested^Name",
    )]);

    let ds = InMemDicomObject::from_element_iter([
        DataElement::new(tags::MODALITY, VR::CS, "RTSTRUCT"),
        DataElement::new(tags::SOP_CLASS_UID, VR::UI, uids::RT_STRUCTURE_SET_STORAGE),
        DataElement::new(
            tags::SOP_INSTANCE_UID,
            VR::UI,
            "1.2.826.0.1.3680043.2.1125.1",
        ),
        DataElement::new(
            tags::STUDY_INSTANCE_UID,
            VR::UI,
            "1.2.826.0.1.3680043.2.1125.2",
        ),
        DataElement::new(tags::SERIES_INSTANCE_UID, VR::UI, series_uid),
        DataElement::new(tags::PATIENT_NAME, VR::PN, patient_name),
        DataElement::new(tags::PATIENT_ID, VR::LO, "MRN123"),
        DataElement::new(tags::STUDY_ID, VR::SH, "MRN123"),
        DataElement::new(tags::STUDY_DATE, VR::DA, study_date),
        DataElement::new(
            tags::OTHER_PATIENT_I_DS_SEQUENCE,
            VR::SQ,
            DataSetSequence::from(vec![nested]),
        ),
    ]);

    let file = ds
        .with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .media_storage_sop_class_uid(uids::RT_STRUCTURE_SET_STORAGE)
                .media_storage_sop_instance_uid("1.2.826.0.1.3680043.2.1125.1"),
        )
        .expect("meta");
    file.write_to_file(path).expect("write dicom");
}

fn write_test_dicom(path: &std::path::Path, patient_name: &str, series_uid: &str) {
    write_test_dicom_with_date(path, patient_name, series_uid, "20240115");
}

fn find_single_output_dicom(root: &Path) -> PathBuf {
    let mut dicom_files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("dcm"))
            .unwrap_or(false)
        {
            dicom_files.push(entry.path().to_path_buf());
        }
    }
    assert_eq!(dicom_files.len(), 1, "expected exactly one output DICOM");
    dicom_files.remove(0)
}

#[test]
fn strict_template_rewrites_identifiers_and_preserves_nested_processing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("source");
    let output = temp.path().join("output");
    std::fs::create_dir_all(&source).expect("mkdir");

    write_test_dicom(
        &source.join("a.dcm"),
        "Patient^Original",
        "1.2.826.0.1.3680043.2.1125.100",
    );

    let request = RtAnonymizeMetadataRequest {
        source_path: source.display().to_string(),
        output_path: Some(output.display().to_string()),
        policy: None,
        policy_path: None,
        template: Some("strict_phi_safe".to_string()),
        policy_overrides: None,
        dry_run: false,
        allow_existing_output: false,
        report_path: None,
        max_workers: 1,
        fail_on_error: true,
        include_trace: true,
        deterministic_uid_secret: Some("stable-secret".to_string()),
    };

    let report = anonymize::execute(request).expect("anonymize should succeed");
    assert_eq!(report.mode, "write");
    assert_eq!(report.source_summary.total_files, 1);
    assert_eq!(report.output_summary.dicom_written, 1);

    let out_file = open_file(find_single_output_dicom(&output)).expect("open anonymized file");
    let patient_name = out_file
        .element(tags::PATIENT_NAME)
        .expect("patient name")
        .value()
        .to_str()
        .expect("to str");
    assert_eq!(patient_name, "");

    let sop_uid = out_file
        .element(tags::SOP_INSTANCE_UID)
        .expect("sop uid")
        .value()
        .to_str()
        .expect("uid");
    assert_ne!(sop_uid, "1.2.826.0.1.3680043.2.1125.1");
}

#[test]
fn explicit_tag_rule_overrides_vr_fallback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("source");
    let output = temp.path().join("output");
    std::fs::create_dir_all(&source).expect("mkdir");

    write_test_dicom(
        &source.join("b.dcm"),
        "Patient^Keep",
        "1.2.826.0.1.3680043.2.1125.200",
    );

    let policy = json!({
        "tag_rules": {
            "PatientName": {
                "action": "keep"
            }
        },
        "vr_rules": {
            "PN": {
                "action": "empty"
            }
        },
        "defaults": {
            "private_tag_default": "remove",
            "unknown_public_default": "keep"
        }
    });

    let request = RtAnonymizeMetadataRequest {
        source_path: source.display().to_string(),
        output_path: Some(output.display().to_string()),
        policy: Some(policy),
        policy_path: None,
        template: None,
        policy_overrides: None,
        dry_run: false,
        allow_existing_output: false,
        report_path: None,
        max_workers: 1,
        fail_on_error: true,
        include_trace: false,
        deterministic_uid_secret: None,
    };

    anonymize::execute(request).expect("anonymize should succeed");

    let out_file = open_file(find_single_output_dicom(&output)).expect("open anonymized file");
    let patient_name = out_file
        .element(tags::PATIENT_NAME)
        .expect("patient name")
        .value()
        .to_str()
        .expect("to str");
    assert_eq!(patient_name, "Patient^Keep");
}

#[test]
fn replace_on_sequence_uses_keep_and_recurses_with_warning() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("source");
    let output = temp.path().join("output");
    std::fs::create_dir_all(&source).expect("mkdir");

    write_test_dicom(
        &source.join("seq-replace.dcm"),
        "Patient^Keep",
        "1.2.826.0.1.3680043.2.1125.250",
    );

    let policy = json!({
        "tag_rules": {
            "OtherPatientIDsSequence": {
                "action": "replace",
                "replace": {
                    "mode": "literal",
                    "literal": "IGNORED_FOR_SQ"
                }
            }
        },
        "vr_rules": {
            "PN": {
                "action": "empty"
            }
        },
        "defaults": {
            "private_tag_default": "remove",
            "unknown_public_default": "keep"
        }
    });

    let request = RtAnonymizeMetadataRequest {
        source_path: source.display().to_string(),
        output_path: Some(output.display().to_string()),
        policy: Some(policy),
        policy_path: None,
        template: None,
        policy_overrides: None,
        dry_run: false,
        allow_existing_output: false,
        report_path: None,
        max_workers: 1,
        fail_on_error: true,
        include_trace: true,
        deterministic_uid_secret: None,
    };

    let report = anonymize::execute(request).expect("anonymize should succeed");

    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("keep+recurse behavior")));
    assert!(report.decision_trace.iter().any(|entry| {
        entry.selector.contains("[0].(0010,0010)")
            && entry.action == "empty"
            && entry.rule_source == "vrfallback"
    }));
}

#[test]
fn aitrium_default_preserves_sop_class_uid_and_tokenizes_ids() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("source");
    let output = temp.path().join("output");
    std::fs::create_dir_all(&source).expect("mkdir");

    write_test_dicom(
        &source.join("c.dcm"),
        "Patient^Original",
        "1.2.826.0.1.3680043.2.1125.300",
    );

    let request = RtAnonymizeMetadataRequest {
        source_path: source.display().to_string(),
        output_path: Some(output.display().to_string()),
        policy: None,
        policy_path: None,
        template: Some("aitrium_default".to_string()),
        policy_overrides: None,
        dry_run: false,
        allow_existing_output: false,
        report_path: None,
        max_workers: 1,
        fail_on_error: true,
        include_trace: false,
        deterministic_uid_secret: Some("stable-secret".to_string()),
    };

    anonymize::execute(request).expect("anonymize should succeed");

    let out_path = find_single_output_dicom(&output);
    let out_file = open_file(&out_path).expect("open anonymized file");
    let sop_class_uid = out_file
        .element(tags::SOP_CLASS_UID)
        .expect("sop class uid")
        .value()
        .to_str()
        .expect("to str");
    assert_eq!(sop_class_uid, uids::RT_STRUCTURE_SET_STORAGE);

    let sop_instance_uid = out_file
        .element(tags::SOP_INSTANCE_UID)
        .expect("sop instance uid")
        .value()
        .to_str()
        .expect("to str");
    assert_ne!(sop_instance_uid, "1.2.826.0.1.3680043.2.1125.1");
    let out_name = out_path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("output filename");
    assert!(out_name.starts_with("RTSTRUCT."));
    assert!(out_name.ends_with(".dcm"));
    assert!(out_name.contains(sop_instance_uid.as_ref()));

    let patient_id = out_file
        .element(tags::PATIENT_ID)
        .expect("patient id")
        .value()
        .to_str()
        .expect("to str");
    let study_id = out_file
        .element(tags::STUDY_ID)
        .expect("study id")
        .value()
        .to_str()
        .expect("to str");
    assert_ne!(patient_id, "MRN123");
    assert!(patient_id.starts_with("NL"));
    assert_eq!(patient_id, study_id);
}

#[test]
fn aitrium_default_handles_empty_date_values() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("source");
    std::fs::create_dir_all(&source).expect("mkdir");

    write_test_dicom_with_date(
        &source.join("d.dcm"),
        "Patient^Original",
        "1.2.826.0.1.3680043.2.1125.400",
        "",
    );

    let request = RtAnonymizeMetadataRequest {
        source_path: source.display().to_string(),
        output_path: None,
        policy: None,
        policy_path: None,
        template: Some("aitrium_default".to_string()),
        policy_overrides: None,
        dry_run: true,
        allow_existing_output: false,
        report_path: None,
        max_workers: 1,
        fail_on_error: true,
        include_trace: false,
        deterministic_uid_secret: Some("stable-secret".to_string()),
    };

    let report = anonymize::execute(request).expect("anonymize should succeed");
    assert_eq!(report.mode, "dry_run");
    assert!(report.errors.is_empty());
}
