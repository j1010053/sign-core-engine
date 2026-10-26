use conlang_changeset::{
    apply_edit, change_set_prelude, ChangeInterpreter, NodeUpdate, PrimitiveEdit,
    UnresolvedChangeSet,
};
use conlang_language::construction::SlotMap;
use conlang_language::{compile_document, LanguageDocument, LibrarySpec, RefTargetV1};

const PHRASES: &str = include_str!("../../../tutorials/named-type-arguments/phrases.lang");

#[test]
fn renaming_type_values_preserves_labels_and_replays_named_references() {
    let source = format!("{PHRASES}\ntrait Wrap<VP>:\n    export sign leaf:\n        syn:\n            slots:\n                item [VP]\n        phon:\n            /{{$slot.item}}/\nsign Wrapper:\n    belongs Wrap<VP=VP>\n    Wrap[0]\nsign Test:\n    syn:\n        slots:\n            item [Wrap<VP=VP>.leaf]\n        map item autofill Wrap<VP=VP>.leaf(item: VP(head: verb()))\n    sem:\n        roles:\n            item [Wrap<VP=VP>.leaf]\n            item = {{$slot.item}}\n    phon:\n        /{{$slot.item}}/\n");
    let base = LanguageDocument::import_new_root(&source, "named:rename").unwrap();
    let vp = base.ref_for_sign("VP").unwrap();
    let referenced_fields = base
        .identities()
        .refs
        .iter()
        .filter(|r| matches!(&r.target, RefTargetV1::Local { target } if target == &vp))
        .map(|r| r.field.as_str())
        .collect::<Vec<_>>();
    assert!(
        referenced_fields
            .iter()
            .any(|field| field.contains("argument")),
        "{referenced_fields:?}"
    );
    let mut text = change_set_prelude(&base, &LibrarySpec::default(), "named:child").unwrap();
    text.push_str("\n    #0:\n        update sign(\"VP\").name = VerbPhrase\n");
    let changes = UnresolvedChangeSet::parse(&text)
        .unwrap()
        .resolve(&base, &LibrarySpec::default())
        .unwrap();
    let after = ChangeInterpreter::new(base.clone(), LibrarySpec::default(), "named:child")
        .unwrap()
        .run(&changes)
        .unwrap()
        .document;
    assert_eq!(after.ref_for_sign("VerbPhrase").unwrap(), vp);
    for spelling in [
        "Comp=VerbPhrase",
        "Wrap<VP=VerbPhrase>",
        "trait Wrap<VP>",
        "item [VP]",
    ] {
        assert!(
            after.source().contains(spelling),
            "{spelling}\n{}",
            after.source()
        );
    }
    assert!(!after.source().contains("Wrap<VerbPhrase="));
    let mut replay = base.fork("named:child").unwrap();
    for edit in conlang_changeset::reconstruct::reconstruct(&base, &after).unwrap() {
        replay = apply_edit(&replay, edit, &LibrarySpec::default())
            .unwrap()
            .document;
    }
    assert_eq!(replay.dump_pair().unwrap(), after.dump_pair().unwrap());
    let (source, ids) = after.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&source, &ids).unwrap();
    let system = compile_document(&reopened, &LibrarySpec::default()).unwrap();
    assert_eq!(
        system
            .derive("demo", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a u b"
    );
    assert_eq!(
        system
            .derive("Test", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "b"
    );
    assert!(apply_edit(
        &reopened,
        PrimitiveEdit::Delete { node: vp },
        &LibrarySpec::default()
    )
    .is_err());
}

#[test]
fn parameter_labels_do_not_create_dependencies_on_same_named_globals() {
    let source = "Symbol a\nClass vowel {a}\nsign Label:\n    phon:\n        /a/\ntrait Box<Label>:\n    export sign leaf:\n        syn:\n            slots:\n                item [Label]\n        phon:\n            /{$slot.item}/\nsign Value:\n    phon:\n        /a/\nsign Owner:\n    belongs Box<Label=Value>\n";
    let base = LanguageDocument::import_new_root(source, "named:labels").unwrap();
    let label = base.ref_for_sign("Label").unwrap();
    assert!(!base
        .identities()
        .refs
        .iter()
        .any(|r| matches!(&r.target, RefTargetV1::Local { target } if target == &label)));
    let renamed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: label.clone(),
            change: NodeUpdate::Rename("Unrelated".into()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert!(renamed.source().contains("Box<Label=Value>"));
    let deleted = apply_edit(
        &renamed,
        PrimitiveEdit::Delete { node: label },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let (source, ids) = deleted.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &ids).unwrap(), deleted);
}
