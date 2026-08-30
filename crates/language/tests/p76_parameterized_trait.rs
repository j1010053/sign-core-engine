use conlang_language::{compile, Language};

fn parse(source: &str) -> Language {
    conlang_language::parser::parse(source).unwrap()
}

fn parse_and_expand(source: &str) -> Language {
    let lang = parse(source);
    compile::expand_traits(&lang).unwrap()
}

// ── 解析 ──

#[test]
fn parse_trait_with_type_params() {
    let lang = parse(
        "\
trait Agreement<C, T>:
    syn:
        slots:
            controller [C]
            target [T]
",
    );
    assert_eq!(lang.traits.len(), 1);
    let t = &lang.traits[0];
    assert_eq!(t.name, "Agreement");
    assert_eq!(t.type_params.len(), 2);
    assert_eq!(t.type_params[0].name, "C");
    assert!(t.type_params[0].bound.is_none());
    assert_eq!(t.type_params[1].name, "T");
    assert!(t.type_params[1].bound.is_none());
}

#[test]
fn parse_trait_with_bounded_params() {
    let lang = parse(
        "\
trait Schema<C: Nominal>:
    syn:
        slots:
            head [C]
",
    );
    let t = &lang.traits[0];
    assert_eq!(t.type_params.len(), 1);
    assert_eq!(t.type_params[0].name, "C");
    assert_eq!(t.type_params[0].bound.as_deref(), Some("Nominal"));
}

#[test]
fn parse_belongs_with_type_args() {
    let lang = parse(
        "\
trait Agreement<C, T>:
    pass

sign SubjectVerb:
    belongs Agreement<Nominal, Predicate>
    Agreement
",
    );
    let sign = &lang.signs[0];
    let decl = sign
        .items
        .iter()
        .find(|item| {
            matches!(
                item,
                conlang_language::SignItem::TraitMount { kind, .. }
                if kind.is_declaration()
            )
        })
        .unwrap();
    if let conlang_language::SignItem::TraitMount { args, .. } = decl {
        assert_eq!(args, &["Nominal", "Predicate"]);
    } else {
        panic!("expected TraitMount");
    }
}

// ── 展開(替換) ──

#[test]
fn expand_substitutes_slot_constraint() {
    let expanded = parse_and_expand(
        "\
trait Schema<C>:
    syn:
        slots:
            head [C]

sign NominalProjection:
    belongs Schema<Noun>
    Schema
",
    );
    let sign = expanded
        .signs
        .iter()
        .find(|s| s.name == "NominalProjection")
        .unwrap();
    let slot = sign.items.iter().find_map(|item| {
        if let conlang_language::SignItem::Slot(slot) = item {
            Some(slot)
        } else {
            None
        }
    });
    assert!(slot.is_some(), "expected a slot after expansion");
    let slot = slot.unwrap();
    assert_eq!(slot.name, "head");
    assert_eq!(
        slot.constraint,
        conlang_language::SlotConstraint::Category("Noun".to_owned())
    );
}

#[test]
fn expand_substitutes_multiple_params() {
    let expanded = parse_and_expand(
        "\
trait Agreement<C, T>:
    syn:
        slots:
            controller [C]
            target [T]

sign SubjectVerb:
    belongs Agreement<Nominal, Predicate>
    Agreement
",
    );
    let sign = expanded
        .signs
        .iter()
        .find(|s| s.name == "SubjectVerb")
        .unwrap();
    let slots: Vec<_> = sign
        .items
        .iter()
        .filter_map(|item| {
            if let conlang_language::SignItem::Slot(slot) = item {
                Some(slot)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].name, "controller");
    assert_eq!(
        slots[0].constraint,
        conlang_language::SlotConstraint::Category("Nominal".to_owned())
    );
    assert_eq!(slots[1].name, "target");
    assert_eq!(
        slots[1].constraint,
        conlang_language::SlotConstraint::Category("Predicate".to_owned())
    );
}

// ── 傳播(trait 部分填入) ──

#[test]
fn propagation_partial_fill_in_trait() {
    let expanded = parse_and_expand(
        "\
trait Agreement<C, T>:
    syn:
        slots:
            controller [C]
            target [T]

trait SubjectAgreement<T>:
    belongs Agreement<Nominal, T>
    Agreement

sign EnglishSVA:
    belongs SubjectAgreement<Predicate>
    SubjectAgreement
",
    );
    let sign = expanded
        .signs
        .iter()
        .find(|s| s.name == "EnglishSVA")
        .unwrap();
    let slots: Vec<_> = sign
        .items
        .iter()
        .filter_map(|item| {
            if let conlang_language::SignItem::Slot(slot) = item {
                Some(slot)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(slots.len(), 2, "should have 2 slots from Agreement");
    assert_eq!(slots[0].name, "controller");
    assert_eq!(
        slots[0].constraint,
        conlang_language::SlotConstraint::Category("Nominal".to_owned()),
        "C was fixed to Nominal in SubjectAgreement"
    );
    assert_eq!(slots[1].name, "target");
    assert_eq!(
        slots[1].constraint,
        conlang_language::SlotConstraint::Category("Predicate".to_owned()),
        "T propagated from SubjectAgreement and filled by sign"
    );
}

// ── 非泛型 trait 不受影響 ──

#[test]
fn non_generic_trait_unchanged() {
    let expanded = parse_and_expand(
        "\
trait Plain:
    syn:
        slots:
            head [Noun]

sign Foo:
    belongs Plain
    Plain
",
    );
    let sign = expanded.signs.iter().find(|s| s.name == "Foo").unwrap();
    let slot = sign.items.iter().find_map(|item| {
        if let conlang_language::SignItem::Slot(slot) = item {
            Some(slot)
        } else {
            None
        }
    });
    assert!(slot.is_some());
    assert_eq!(
        slot.unwrap().constraint,
        conlang_language::SlotConstraint::Category("Noun".to_owned())
    );
}

// ── arity 不符 ──

#[test]
fn arity_mismatch_is_error() {
    let lang = parse(
        "\
trait Schema<C>:
    syn:
        slots:
            head [C]

sign Bad:
    belongs Schema<Noun, Verb>
    Schema
",
    );
    let result = compile::expand_traits(&lang);
    assert!(result.is_err(), "should reject arity mismatch");
    let err = result.unwrap_err();
    assert!(
        format!("{err}").contains("1"),
        "error should mention expected count: {err}"
    );
}

// ── canonical roundtrip ──

#[test]
fn canonical_roundtrip_with_type_params() {
    let source = "\
trait Agreement<C, T>:
    syn:
        slots:
            controller [C]
            target [T]

sign SubjectVerb:
    belongs Agreement<Nominal, Predicate>
    Agreement
";
    let lang = parse(source);
    let dumped = lang.dump();
    let reparsed = parse(&dumped);
    assert_eq!(reparsed.dump(), dumped, "canonical form should roundtrip");

    let t = &reparsed.traits[0];
    assert_eq!(t.type_params.len(), 2);
    assert_eq!(t.type_params[0].name, "C");

    let sign = &reparsed.signs[0];
    let decl = sign.items.iter().find(|item| {
        matches!(
            item,
            conlang_language::SignItem::TraitMount { kind, .. }
            if kind.is_declaration()
        )
    });
    if let Some(conlang_language::SignItem::TraitMount { args, .. }) = decl {
        assert_eq!(args, &["Nominal", "Predicate"]);
    }
}

// ── diagnostics ──

#[test]
fn marker_trait_rejects_type_params() {
    let lang = parse(
        "\
marker trait Bad<C>:
    pass
",
    );
    let report = conlang_language::check_language(&lang);
    let codes: Vec<_> = report.diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&"TYPE_PARAM_ON_MARKER_TRAIT"),
        "marker trait with type params should be flagged: {codes:?}"
    );
}

#[test]
fn global_trait_rejects_type_params() {
    let lang = parse(
        "\
global trait Bad<C>:
    pass
",
    );
    let report = conlang_language::check_language(&lang);
    let codes: Vec<_> = report.diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&"TYPE_PARAM_ON_GLOBAL_TRAIT"),
        "global trait with type params should be flagged: {codes:?}"
    );
}

#[test]
fn duplicate_param_name_is_error() {
    let lang = parse(
        "\
trait Bad<C, C>:
    syn:
        slots:
            x [C]
",
    );
    let report = conlang_language::check_language(&lang);
    let codes: Vec<_> = report.diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&"TYPE_PARAM_DUPLICATE_NAME"),
        "duplicate param name should be flagged: {codes:?}"
    );
}

// ── trait_view ──

#[test]
fn trait_view_of_parameterized_trait_succeeds() {
    let lang = parse(
        "\
trait Schema<C>:
    syn:
        slots:
            head [C]
",
    );
    let view = compile::trait_view(&lang, &[], "Schema");
    assert!(
        view.is_ok(),
        "trait_view should not fail on parameterized traits: {}",
        view.unwrap_err()
    );
    let items = view.unwrap();
    let slot = items.iter().find_map(|item| {
        if let conlang_language::SignItem::Slot(slot) = item {
            Some(slot)
        } else {
            None
        }
    });
    assert!(slot.is_some(), "trait_view should contain the slot");
    assert_eq!(
        slot.unwrap().constraint,
        conlang_language::SlotConstraint::Category("C".to_owned()),
        "abstract param name should be preserved in trait_view"
    );
}

#[test]
fn trait_view_inherits_from_parameterized_parent() {
    let lang = parse(
        "\
trait Base<C>:
    syn:
        slots:
            head [C]

trait Child:
    belongs Base<Noun>
    Base
    syn:
        slots:
            extra [Verb]
",
    );
    let view = compile::trait_view(&lang, &[], "Child").unwrap();
    let slots: Vec<_> = view
        .iter()
        .filter_map(|item| {
            if let conlang_language::SignItem::Slot(slot) = item {
                Some(slot)
            } else {
                None
            }
        })
        .collect();
    let head = slots.iter().find(|s| s.name == "head");
    assert!(head.is_some(), "should inherit head slot from Base");
    assert_eq!(
        head.unwrap().constraint,
        conlang_language::SlotConstraint::Category("Noun".to_owned()),
        "inherited slot should have C substituted to Noun"
    );
}

#[test]
fn bound_violation_is_diagnosed() {
    let lang = parse(
        "\
trait Animal:
    pass

trait Mammal:
    belongs Animal
    Animal

trait Schema<C: Mammal>:
    syn:
        slots:
            head [C]

sign Bad:
    belongs Schema<Animal>
    Schema
",
    );
    let report = conlang_language::check_language(&lang);
    let codes: Vec<_> = report.diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&"TYPE_PARAM_BOUND_VIOLATION"),
        "Animal is not a subtype of Mammal, should be flagged: {codes:?}"
    );
}

#[test]
fn bound_satisfied_produces_no_violation() {
    let lang = parse(
        "\
trait Animal:
    pass

trait Mammal:
    belongs Animal
    Animal

trait Schema<C: Animal>:
    syn:
        slots:
            head [C]

sign Good:
    belongs Schema<Mammal>
    Schema
",
    );
    let report = conlang_language::check_language(&lang);
    let violations: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.code == "TYPE_PARAM_BOUND_VIOLATION")
        .collect();
    assert!(
        violations.is_empty(),
        "Mammal is a subtype of Animal, should pass: {violations:?}"
    );
}
