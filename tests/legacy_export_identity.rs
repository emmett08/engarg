//! Persisted-format compatibility is independent of the new method vocabulary.
//!
//! The public fixture was saved by EngArg 0.10.0 before typed warrant bindings.
//! It contains no observations and makes no supported catalogue claim. Retaining
//! this artifact catches dependency feature changes that alter JSON object order
//! inside the exact exported EAL source or its recorded revision identity.

use engarg::{eal_acquisition::reconstruct_bundle, eal_export::EalExport};

#[test]
fn persisted_v3_export_retains_its_original_producer_identity_and_source_bytes() {
    let saved: EalExport =
        serde_json::from_str(include_str!("fixtures/persisted-export-v3.json")).unwrap();
    assert_eq!(saved.schema_version, "engarg.eal-export/3");
    assert_eq!(
        saved.semantics_version.as_deref(),
        Some("engarg.eal-projection/3")
    );
    assert_eq!(saved.exporter_version, "0.10.0");
    assert!(
        saved
            .original_case
            .warrants
            .iter()
            .all(|warrant| warrant.reasoning_method.is_none())
    );
    assert!(!saved.source.contains("\"accepted\":true"));

    let rebuilt = reconstruct_bundle(&saved).expect("admit the original persisted v3 export");
    assert_eq!(rebuilt.revision, saved.revision);
    assert_eq!(rebuilt.source.as_bytes(), saved.source.as_bytes());
    assert_eq!(
        serde_json::to_value(rebuilt).unwrap(),
        serde_json::to_value(saved).unwrap()
    );
}
