//! Focused public-contract coverage for typed Syn/Sem features, semantic
//! roles, `$self`, derivation constraints, phon realization, and semantic
//! interchange.  The fixture intentionally stays independent of the English
//! library so each primitive has a small, inspectable counterexample.

use conlang_language::construction::{self, CxgError, FillerProvenance, SlotFiller, SlotMap};
use conlang_language::{
    compile_system, compile_with_libraries, CompileSystemError, DerivationContext, Dim, Language,
    LibraryId, LibraryKind, LibrarySpec, SemanticDocumentV1, SemanticNodeV1, SemanticSourceV1,
    SystemError, SEMANTIC_SCHEMA_V1,
};
use std::collections::BTreeMap;

fn semantic_parent() -> &'static str {
    // `Semantic` is allowed to be supplied by std in a later library revision.
    // Keep this fixture valid both before and after that migration, without
    // redefining an imported trait.
    if compile_system(Language::new())
        .expect("empty std system compiles")
        .ontology
        .has("Semantic")
    {
        "    belongs Semantic\n"
    } else {
        ""
    }
}

fn fixture() -> String {
    r#"Symbol a
Symbol d
Symbol s
Class vowel {a}

trait TestSemantic:
{SEMANTIC_PARENT}trait TestEntity:
    belongs TestSemantic
    syn:
        feature:
            number = enum(singular, plural)
trait TestHuman:
    belongs TestEntity
trait TestTransferFrame:
    belongs TestSemantic
    sem:
        feature:
            number = enum(singular, plural)
        roles:
            agent [TestHuman]
            theme [TestEntity]
            recipient [TestHuman]?

sign agent:
    belongs TestHuman
    phon:
        /a/
sign theme:
    belongs TestEntity
    syn:
        feature:
            number = singular
    phon:
        /d/

sign TestCountTransfer:
    belongs TestTransferFrame
    syn:
        slots:
            agent [TestHuman]
            theme [TestEntity]
        feature:
            number = enum(singular, plural)
            agreement = enum(singular, plural)
            agreement => $slot.theme.syn.number
    sem:
        feature:
            number => $self.syn.number / $self == [TestTransferFrame]
        roles:
            agent = {agent}
            theme = {theme}
    prag:
        realized-number => $self.sem.number
    phon:
        /{agent}{theme}/
        realization:
            case:
                $self.syn.number == plural:
                    /{agent}{theme}s/
                else:
                    /{agent}{theme}/

sign FixedCountForm:
    syn:
        slots:
            stem [TestEntity]
        feature:
            number = enum(singular, plural)
            number = singular
    phon:
        /{stem}/
"#
    .replace("{SEMANTIC_PARENT}", semantic_parent())
}

fn system() -> conlang_language::CompiledSystem {
    compile_system(Language::parse(&fixture()).expect("fixture parses")).expect("fixture compiles")
}

fn count_fillers() -> Vec<SlotFiller<'static>> {
    vec![
        SlotFiller::sign("agent", "agent"),
        SlotFiller::sign("theme", "theme"),
    ]
}

#[test]
fn feature_enum_self_roles_and_realization_execute_as_one_deep_sign() {
    let source = fixture();
    let dumped = Language::parse(&source).expect("parse").dump();
    assert!(dumped.contains("number = enum(singular, plural)"));
    assert!(dumped.contains("number => $self.syn.number / $self == [TestTransferFrame]"));
    assert!(dumped.contains("agent [TestHuman]"));
    assert!(dumped.contains("$self.syn.number == plural:"));
    assert_eq!(
        Language::parse(&dumped).expect("round-trip parse").dump(),
        dumped
    );

    let system = system();
    let fillers = count_fillers();
    let singular = system
        .derive_with_context(
            "TestCountTransfer",
            &fillers,
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "singular"),
        )
        .expect("singular derivation");
    let plural = system
        .derive_with_context(
            "TestCountTransfer",
            &fillers,
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "plural"),
        )
        .expect("plural derivation");

    // Both outputs are environments of the same deep construction sign; only
    // the finalized feature constraint selects the surface template.
    assert_eq!(singular.token.construction, "TestCountTransfer");
    assert_eq!(plural.token.construction, singular.token.construction);
    assert_eq!(singular.realization.input.as_str(), "ad");
    assert_eq!(plural.realization.input.as_str(), "ads");
    assert_eq!(singular.surface, "ad");
    assert_eq!(plural.surface, "ads");
    assert_eq!(singular.realization.branch, Some(1));
    assert_eq!(plural.realization.branch, Some(0));
    assert!(plural.realization.source.line > 0);
    // Typed realization records the guard evaluation as a matched branch in
    // `cases` (the former flat `self_reads` recording was removed with the V1
    // `RealizationBranch` form); selecting branch 0 for `== plural` proves the
    // `syn.number == plural` guard read.
    assert!(plural
        .realization
        .cases
        .iter()
        .any(|record| record.branch == 0));

    assert_eq!(
        plural.token.sem.features.get("number"),
        Some(&"plural".to_owned())
    );
    assert!(plural
        .token
        .sem
        .types
        .iter()
        .any(|ty| ty == "TestTransferFrame"));
    assert_eq!(
        plural
            .token
            .sem
            .role("agent")
            .map(|node| node.source.sign.as_str()),
        Some("agent")
    );
    assert_eq!(
        plural
            .token
            .sem
            .role("theme")
            .map(|node| node.source.sign.as_str()),
        Some("theme")
    );
    assert_eq!(plural.token.sem.role("recipient"), None);
    assert_eq!(
        plural
            .token
            .prag
            .iter()
            .find(|(path, _)| path == "prag.realized-number")
            .map(|(_, value)| value.as_str()),
        Some("plural"),
        "Prag runs after Sem and sees its committed typed feature"
    );
    assert_eq!(
        plural
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.agreement")
            .map(|(_, value)| value.as_str()),
        Some("singular"),
        "a feature rule reads the frozen filler snapshot, not a mutable filler"
    );

    let sem_record = plural
        .rules
        .iter()
        .map(|entry| &entry.record)
        .find(|record| record.dim == Dim::Sem && !record.self_reads.is_empty())
        .expect("the Sem feature rule records its $self read");
    assert!(sem_record.source.line > 0);
    assert!(sem_record
        .self_reads
        .iter()
        .any(|read| read.dim == Dim::Syn && read.path == "number"));
    let syn_record = plural
        .rules
        .iter()
        .map(|entry| &entry.record)
        .find(|record| record.dim == Dim::Syn && !record.slot_reads.is_empty())
        .expect("the Syn feature rule records its frozen $slot read");
    assert!(syn_record.slot_reads.iter().any(|read| {
        read.slot == "theme"
            && read.dim == Dim::Syn
            && read.path == "number"
            && read.value.as_deref() == Some("singular")
    }));

    // The legacy direct surface helper must not bypass realization selection.
    assert!(matches!(
        construction::surface(&system.artifacts.grammar.program, &plural.token),
        Err(CxgError::RealizationRequiresSystem)
    ));
}

#[test]
fn derivation_context_is_a_typed_constraint_not_a_surface_override() {
    let system = system();
    let error = system
        .derive_with_context(
            "FixedCountForm",
            &[SlotFiller::sign("stem", "theme")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "plural"),
        )
        .expect_err("a context cannot overwrite a fixed feature value");
    assert!(matches!(
        error,
        SystemError::DerivationFeatureConflict {
            dim: Dim::Syn,
            ref name,
            ref expected,
            ref actual,
        } if name == "number" && expected == "plural" && actual == "singular"
    ));

    let error = system
        .derive_with_context(
            "TestCountTransfer",
            &count_fillers(),
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "dual"),
        )
        .expect_err("context values are checked against the declared enum");
    assert!(matches!(
        error,
        SystemError::DerivationFeatureOutOfDomain {
            dim: Dim::Syn,
            ref name,
            ref value,
            ..
        } if name == "number" && value == "dual"
    ));

    let error = system
        .derive_with_context(
            "TestCountTransfer",
            &count_fillers(),
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "agreement", "plural"),
        )
        .expect_err("a frozen filler rule cannot be overwritten by the context");
    assert!(matches!(
        error,
        SystemError::DerivationFeatureConflict {
            dim: Dim::Syn,
            ref name,
            ref expected,
            ref actual,
        } if name == "agreement" && expected == "plural" && actual == "singular"
    ));
}

#[test]
fn semantic_document_v1_is_deterministic_recursive_and_revalidated() {
    let system = system();
    let derivation = system
        .derive_with_context(
            "TestCountTransfer",
            &count_fillers(),
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "plural"),
        )
        .expect("derivation");
    let document = SemanticDocumentV1::from_sem_node(&derivation.token.sem);
    let json = document.to_json().expect("v1 JSON");
    assert!(json.contains("\"schema\": \"conlang.semantic/v1\""));
    assert!(json.contains("\"agent\""));
    assert!(json.contains("\"number\": \"plural\""));

    let parsed = SemanticDocumentV1::from_json(&json).expect("v1 JSON re-parses");
    assert_eq!(parsed, document);
    assert_eq!(parsed.to_json().expect("stable JSON"), json);
    let detached = system
        .validate_semantic_document(&parsed)
        .expect("typed semantic document validates without changing Language");
    assert_eq!(detached, derivation.token.sem);

    let mut invalid_value = parsed.clone();
    invalid_value
        .root
        .features
        .insert("number".to_owned(), "dual".to_owned());
    assert!(system.validate_semantic_document(&invalid_value).is_err());

    let mut invalid_trait = parsed.clone();
    invalid_trait
        .root
        .types
        .push("NotADeclaredTrait".to_owned());
    assert!(system.validate_semantic_document(&invalid_trait).is_err());

    assert!(SemanticDocumentV1::from_json(
        r#"{"schema":"conlang.semantic/v2","root":{"source":{"sign":"x"},"types":[],"features":{},"roles":{}}}"#
    )
    .is_err());
    assert!(SemanticDocumentV1::from_json(
        r#"{"schema":"conlang.semantic/v1","root":{"source":{"sign":"x"},"types":[],"features":{},"roles":{}},"unknown":true}"#
    )
    .is_err());
}

#[test]
fn semantic_document_v1_canonicalizes_types_and_object_keys_at_every_boundary() {
    let document = SemanticDocumentV1 {
        schema: SEMANTIC_SCHEMA_V1.to_owned(),
        root: SemanticNodeV1 {
            source: SemanticSourceV1 {
                package: Some("natural:fixture".to_owned()),
                sign: "root".to_owned(),
            },
            types: vec!["Zeta".to_owned(), "Alpha".to_owned(), "Zeta".to_owned()],
            features: BTreeMap::from([
                ("zeta".to_owned(), "two".to_owned()),
                ("alpha".to_owned(), "one".to_owned()),
            ]),
            roles: BTreeMap::from([
                (
                    "z_role".to_owned(),
                    SemanticNodeV1 {
                        source: SemanticSourceV1 {
                            package: None,
                            sign: "z".to_owned(),
                        },
                        types: vec!["Child".to_owned(), "Child".to_owned()],
                        features: BTreeMap::new(),
                        roles: BTreeMap::new(),
                        fields: BTreeMap::new(),
                        senses: Vec::new(),
                        edges: Vec::new(),
                    },
                ),
                (
                    "a_role".to_owned(),
                    SemanticNodeV1 {
                        source: SemanticSourceV1 {
                            package: None,
                            sign: "a".to_owned(),
                        },
                        types: vec!["Omega".to_owned(), "Beta".to_owned()],
                        features: BTreeMap::new(),
                        roles: BTreeMap::new(),
                        fields: BTreeMap::new(),
                        senses: Vec::new(),
                        edges: Vec::new(),
                    },
                ),
            ]),
            fields: BTreeMap::new(),
            senses: Vec::new(),
            edges: Vec::new(),
        },
    };

    // Serialization must normalize even a DTO constructed directly by a
    // caller, rather than only one produced from an internal SemNode.
    let json = document.to_json().expect("canonical JSON");
    let reparsed = SemanticDocumentV1::from_json(&json).expect("canonical JSON parses");
    assert_eq!(reparsed.root.types, ["Alpha", "Zeta"]);
    assert_eq!(
        reparsed.root.roles["z_role"].types,
        ["Child"],
        "nested type arrays are canonicalized recursively"
    );
    assert_eq!(reparsed.root.roles["a_role"].types, ["Beta", "Omega"]);
    assert!(
        json.find("\"alpha\"").expect("alpha feature")
            < json.find("\"zeta\"").expect("zeta feature")
    );
    assert!(json.find("\"a_role\"").expect("a role") < json.find("\"z_role\"").expect("z role"));
    assert_eq!(reparsed.to_json().expect("stable re-serialization"), json);

    let decoded_noncanonical = SemanticDocumentV1::from_json(
        r#"{"schema":"conlang.semantic/v1","root":{"source":{"sign":"decoded"},"types":["Zeta","Alpha","Zeta"],"features":{},"roles":{}}}"#,
    )
    .expect("external JSON is normalized at decode time");
    assert_eq!(decoded_noncanonical.root.types, ["Alpha", "Zeta"]);

    // Detached semantic values returned by the DTO boundary inherit the same
    // normalization rather than reintroducing duplicate type membership.
    let detached = document.root.into_sem_node();
    assert_eq!(detached.types, ["Alpha", "Zeta"]);
    assert_eq!(detached.role("z_role").expect("z role").types, ["Child"]);
}

#[test]
fn declared_feature_and_role_contract_errors_are_not_silently_accepted() {
    let undeclared = r#"sign Bad:
    syn:
        feature:
            number = plural
"#;
    let CompileSystemError::Validation(report) =
        compile_system(Language::parse(undeclared).expect("parse")).expect_err("undeclared value")
    else {
        panic!("expected validation report");
    };
    assert!(report
        .errors()
        .any(|diagnostic| diagnostic.code == "FEATURE_UNDECLARED"));

    let out_of_domain = r#"sign Bad:
    syn:
        feature:
            number = enum(singular, plural)
            number = dual
"#;
    let CompileSystemError::Validation(report) =
        compile_system(Language::parse(out_of_domain).expect("parse"))
            .expect_err("out-of-domain value")
    else {
        panic!("expected validation report");
    };
    assert!(report
        .errors()
        .any(|diagnostic| diagnostic.code == "FEATURE_VALUE_OUT_OF_DOMAIN"));

    // A frame's argument contract is not an ordinary inherited default: two
    // incompatible declarations must be a compile error, rather than silently
    // choosing the last `belongs` source.
    let role_conflict = r#"trait TestEntity:
trait TestHuman:
    belongs TestEntity
trait FirstFrame:
    sem:
        roles:
            participant [TestHuman]
trait SecondFrame:
    sem:
        roles:
            participant [TestEntity]
sign ConflictedFrame:
    belongs FirstFrame
    belongs SecondFrame
"#;
    let CompileSystemError::Validation(report) =
        compile_system(Language::parse(role_conflict).expect("parse"))
            .expect_err("incompatible role contracts")
    else {
        panic!("expected validation report");
    };
    assert!(report
        .errors()
        .any(|diagnostic| diagnostic.code == "ROLE_SCHEMA_CONFLICT"));

    let declaration_shadow = r#"trait Earlier:
    syn:
        feature:
            number = enum(singular)
trait Later:
    syn:
        feature:
            number = enum(plural)
sign Winner:
    belongs Earlier
    belongs Later
"#;
    let resolved = compile_system(Language::parse(declaration_shadow).expect("parse"))
        .expect("feature declaration priority is a warning, not a hard failure");
    assert!(resolved
        .validation
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "FEATURE_DECLARATION_SHADOWED"));
}

#[test]
fn feature_rules_share_stable_stage_dispatch_with_ordinary_rules() {
    let source = r#"trait Typed:
    syn:
        feature:
            copied = enum(ready)
            copied => $self.syn.value @stage stem
trait Plain:
    syn:
        value => ready @stage phrase
sign Ordered:
    belongs Typed
    belongs Plain
"#;
    let system =
        compile_system(Language::parse(source).expect("fixture parses")).expect("fixture compiles");
    let evaluated = system.evaluate_sign("Ordered").expect("sign evaluates");

    // The typed stem rule executes before the ordinary phrase rule. It cannot
    // read a value committed by a later stage.
    assert_eq!(
        evaluated
            .sign
            .project(Dim::Syn, &system.ontology)
            .get("syn.value"),
        Some("ready")
    );
    assert_eq!(
        evaluated
            .sign
            .project(Dim::Syn, &system.ontology)
            .get("syn.copied"),
        None
    );
}

#[test]
fn realization_guard_reads_count_as_form_pole_slot_use() {
    let source = r#"sign filler:
    belongs Noun
    phon:
        /a/
sign GuardedForm:
    syn:
        slots:
            stem [Noun]
    prag:
        purpose = test
    phon:
        /x/
        realization:
            case:
                $slot.stem == [Noun]:
                    /x/
                else:
                    /y/
"#;
    let system = compile_system(Language::parse(source).expect("fixture parses"))
        .expect("realization guard is valid form-pole use");
    assert!(!system.validation.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == "CONSTRUCTION_SLOT_UNUSED" && diagnostic.message.contains("GuardedForm")
    }));
}

#[test]
fn english_count_noun_uses_one_deep_sign_for_dog_and_dogs() {
    let system = compile_with_libraries(
        Language::new(),
        LibrarySpec::natural(LibraryId::new(LibraryKind::Natural, "en-standard")),
    )
    .expect("English library compiles through the public loader");
    let deep_sign = system
        .effective_language()
        .sign_named("EnglishCountNounForm")
        .expect("one English count-noun construction sign");
    assert!(
        system.effective_language().sign_named("dogs").is_none(),
        "plural is a realization of the deep sign, not an old sibling lexical sign"
    );

    let singular = system
        .derive_with_context(
            "EnglishCountNounForm",
            &[SlotFiller::sign("stem", "dog")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "singular"),
        )
        .expect("singular dog");
    let plural = system
        .derive_with_context(
            "EnglishCountNounForm",
            &[SlotFiller::sign("stem", "dog")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "plural"),
        )
        .expect("plural dogs");

    assert_eq!(singular.token.construction_id, deep_sign.id);
    assert_eq!(plural.token.construction_id, deep_sign.id);
    assert_eq!(plural.token.construction_id, singular.token.construction_id);
    assert_eq!(singular.realization.input.as_str(), "dog");
    assert_eq!(plural.realization.input.as_str(), "dogs");
    assert_eq!(singular.surface, "dog");
    assert_eq!(plural.surface, "dogs");
    assert_eq!(
        plural.token.sem.features.get("number"),
        Some(&"plural".to_owned())
    );

    let referent = plural
        .token
        .sem
        .role("referent")
        .expect("role binding creates a recursive semantic node");
    assert_eq!(referent.source.sign, "dog");
    assert!(referent.types.iter().any(|ty| ty == "Entity"));
    let stem_provenance = plural
        .token
        .provenance
        .fillers
        .iter()
        .find(|filler| filler.slot == "stem")
        .expect("slot provenance");
    assert!(matches!(
        stem_provenance.source,
        FillerProvenance::StoredSign(ref sign) if sign == "dog"
    ));
    assert!(plural.rules.iter().any(|entry| {
        entry.record.dim == Dim::Sem
            && entry.record.source_package.as_deref() == Some("natural:en-standard")
    }));

    let repeated = system
        .derive_with_context(
            "EnglishCountNounForm",
            &[SlotFiller::sign("stem", "dog")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "plural"),
        )
        .expect("repeat plural dogs");
    assert_eq!(repeated.surface, plural.surface);
    assert_eq!(repeated.token, plural.token);
    assert_eq!(repeated.realization, plural.realization);
}

/// 相容性:V1 硬移除後,扁平 `realization:` 分支語法(`/tmpl/ / guard` + `else`)
/// 被明確拒絕;唯一支援形式是 typed `case:`(共用機制)。
#[test]
fn flat_realization_branch_syntax_is_rejected_after_v1_removal() {
    let flat = "\
Symbol x
Symbol y

trait TReal:
    syn:
        feature:
            n = enum(a, b)

sign s:
    belongs TReal
    phon:
        /x/
        realization:
            /x/ / $self.syn.n == a
            else /y/
";
    let err = Language::parse(flat).expect_err("flat realization must not parse");
    assert!(
        format!("{err:?}").contains("realization must contain a `case:`"),
        "expected flat-realization rejection, got {err:?}"
    );

    // 對照:等價的 typed 形式解析成功。
    let typed = "\
Symbol x
Symbol y

trait TReal:
    syn:
        feature:
            n = enum(a, b)

sign s:
    belongs TReal
    phon:
        /x/
        realization:
            case:
                $self.syn.n == a:
                    /x/
                else:
                    /y/
";
    assert!(
        Language::parse(typed).is_ok(),
        "typed realization must parse"
    );
}

/// `phon:` 不再接受 latent 的 `field = value` Def(無消費者);只收 `/…/` UR、
/// 規則、`realization:`。
#[test]
fn phon_field_equals_value_definition_is_rejected() {
    let src = "\
Symbol x

sign s:
    phon:
        /x/
        stress = high
";
    let err = Language::parse(src).expect_err("phon field=value must not parse");
    assert!(
        format!("{err:?}").contains("field = value"),
        "expected phon field-def rejection, got {err:?}"
    );
    // 對照:UR + 規則仍可解析。
    assert!(Language::parse(
        "Symbol x\nSymbol y\n\nsign s:\n    phon:\n        /x/\n        x => y\n"
    )
    .is_ok());
}
