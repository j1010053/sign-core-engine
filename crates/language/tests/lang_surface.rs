//! 共時 `.lang` 表面語法完整性：每個已落地的共時資料型別都能由 source
//! 表達、canonical round-trip，並走 `compile_system` 進 runtime。

use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{
    compile_system, CompileSystemError, Language, SignItem, SignLifecycle, SignProvenance, SignRef,
    SlotMapOp,
};

const COMPLETE: &str = include_str!("fixtures/synchronic_complete.lang");

#[test]
fn nested_paths_metadata_and_slot_maps_round_trip() {
    let language = Language::parse(COMPLETE).expect("parse complete synchronic surface");
    let canonical = language.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    assert!(canonical.contains("concept = DEFINITE_NP"));
    assert!(canonical.contains("map head rename nucleus"));

    let np = language.sign_named("NP").unwrap();
    assert_eq!(np.origin(), Some(SignRef("proto_np".into())));
    assert_eq!(np.provenance(), Some(SignProvenance::Grammaticalized));
    assert_eq!(np.lifecycle(), Some(SignLifecycle::Active));
    assert!(np.items.iter().any(|item| {
        matches!(item, SignItem::SlotMap(SlotMapOp::Rename { slot, to }) if slot == "head" && to == "nucleus")
    }));
}

#[test]
fn source_declared_slot_map_reaches_construction_runtime() {
    let system = compile_system(Language::parse(COMPLETE).unwrap()).expect("compile system");
    let token = system
        .apply_construction(
            "NP",
            &[SlotFiller::sign("nucleus", "dog")],
            &SlotMap::identity(),
        )
        .expect("stored slot map must be applied");

    assert_eq!(token.phon_form().unwrap(), "ad");
    assert!(token.is_saturated());
    assert_eq!(token.sem.field("concept"), Some("DEFINITE_NP"));
    assert_eq!(
        token.sem.role("profile").unwrap().field("concept"),
        Some("DOG")
    );
    assert_eq!(
        token.provenance.fillers.len(),
        2,
        "autofill + explicit filler"
    );
}

#[test]
fn traits_can_supply_construction_slots_and_mapping_together() {
    let source = "\
Symbol d
trait MappedConstruction:
    syn:
        slots:
            head [Noun]
        map head rename item
sign dog:
    belongs Noun
    phon:
        /d/
sign Wrapped:
    belongs MappedConstruction
    phon:
        /{$slot.head}/
";
    let system = compile_system(Language::parse(source).unwrap()).unwrap();
    let token = system
        .apply_construction(
            "Wrapped",
            &[SlotFiller::sign("item", "dog")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(token.phon_form().unwrap(), "d");
}

#[test]
fn malformed_or_semantically_invalid_slot_maps_are_rejected() {
    let malformed = "sign C:\n    syn:\n        map x rename\n";
    assert!(
        Language::parse(malformed).is_err(),
        "malformed mapping is syntax error"
    );

    let unknown = "\
sign C:
    syn:
        slots:
            x [Noun]
        map ghost internalize
    phon:
        /{$slot.x}/
";
    let err = compile_system(Language::parse(unknown).unwrap()).unwrap_err();
    let CompileSystemError::Validation(report) = err else {
        panic!("expected validation report")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|d| d.code == "SLOT_MAP_INVALID"));
}

#[test]
fn sign_metadata_is_typed_and_origin_cycles_are_rejected() {
    let source = "\
sign a:
    origin = sign(b)
    provenance = loan
    lifecycle = obsolete
sign b:
    origin = sign(a)
    provenance = native
";
    let language = Language::parse(source).unwrap();
    let a = language.sign_named("a").unwrap();
    assert_eq!(a.provenance(), Some(SignProvenance::Loan));
    assert_eq!(a.lifecycle(), Some(SignLifecycle::Obsolete));

    let err = compile_system(language).unwrap_err();
    let CompileSystemError::Validation(report) = err else {
        panic!("expected validation report")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|d| d.code == "META_ORIGIN_CYCLE"));
}

#[test]
fn historical_attestation_fields_are_not_synchronic_metadata() {
    let source = "sign word:\n    attested_at = 1250\n";
    let err = compile_system(Language::parse(source).unwrap()).unwrap_err();
    let CompileSystemError::Validation(report) = err else {
        panic!("expected validation report")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|d| d.code == "DEF_INVALID_PATH_OR_VALUE"));
}
