use conlang_changeset::{
    apply_edit, change_set_prelude, ChangeInterpreter, NodeUpdate, PrimitiveEdit,
    UnresolvedChangeSet,
};
use conlang_language::construction::SlotMap;
use conlang_language::{compile_system_ref, LanguageDocument, LibrarySpec, RefTargetV1};

const PHRASES: &str = include_str!("../../../tutorials/instance-types/phrases.lang");
const INSPECTOR: &str = r#"
marker trait Tag<C>:
sign Inspector:
    syn:
        slots:
            item [VP]
        map item autofill VP(head: verb())
        feature:
            selected = enum(yes, no)
            selected = no
            selected => yes / $slot.item == [VP]
            selected => no / $slot.item == [Tag<VP>]
    sem:
        roles:
            item [VP]
            item = {$slot.item}
    phon:
        /{$slot.item}/
        realization:
            case:
                $slot.item == [VP]:
                    /a/
                base:
"#;

#[test]
fn sign_type_rename_rewrites_all_positions_and_survives_reconstruction_and_replay() {
    let base = LanguageDocument::import_new_root(&format!("{PHRASES}{INSPECTOR}"), "instance:root")
        .unwrap();
    let vp = base.ref_for_sign("VP").unwrap();
    assert_eq!(base.identities().reference_semantics, Some(4));
    for field in [
        "slot.constraint",
        "role.constraint",
        "case.guard[0]",
        "rule.type.",
    ] {
        assert!(
            base.identities()
                .refs
                .iter()
                .any(|r| r.field.starts_with(field)
                    && matches!(&r.target, RefTargetV1::Local { target } if target == &vp)),
            "{field}"
        );
    }
    let mut text = change_set_prelude(&base, &LibrarySpec::default(), "instance:child").unwrap();
    text.push_str("\n    #0:\n        update sign(\"VP\").name = VerbPhraseInstance\n");
    let changes = UnresolvedChangeSet::parse(&text)
        .unwrap()
        .resolve(&base, &LibrarySpec::default())
        .unwrap();
    let after = ChangeInterpreter::new(base.clone(), LibrarySpec::default(), "instance:child")
        .unwrap()
        .run(&changes)
        .unwrap()
        .document;
    assert_eq!(after.ref_for_sign("VerbPhraseInstance").unwrap(), vp);
    let source = after.source();
    for spelling in [
        "[VerbPhraseInstance]",
        "XP<TenseHead, DP, VerbPhraseInstance>",
        "VerbPhraseInstance(head: verb())",
        "Tag<VerbPhraseInstance>",
    ] {
        assert!(source.contains(spelling), "{source}");
    }
    assert!(!source.contains("[VP]"));
    let mut replay = base.fork("instance:child").unwrap();
    for edit in conlang_changeset::reconstruct::reconstruct(&base, &after).unwrap() {
        replay = apply_edit(&replay, edit, &LibrarySpec::default())
            .unwrap()
            .document;
    }
    assert_eq!(replay.dump_pair().unwrap(), after.dump_pair().unwrap());
    let (source, ids) = after.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&source, &ids).unwrap();
    let system = compile_system_ref(reopened.language()).unwrap();
    assert_eq!(
        system
            .derive("demo", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a u b"
    );
    assert_eq!(
        system
            .derive("Inspector", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    assert_eq!(
        system
            .evaluate_sign("VerbPhraseInstance")
            .unwrap()
            .sign
            .id
            .0,
        vp.id
    );
    assert!(apply_edit(
        &reopened,
        PrimitiveEdit::Delete { node: vp },
        &LibrarySpec::default()
    )
    .is_err());
}

#[test]
fn global_sign_rename_and_delete_do_not_capture_generic_parameters() {
    let source = "Symbol a\nClass vowel {a}\nsign T:\n    phon:\n        /a/\ntrait Box<C>:\n    export sign leaf:\n        syn:\n            slots:\n                item [C]\n        phon:\n            /{$slot.item}/\ntrait Owner<T>:\n    belongs Box<T>\n";
    let base = LanguageDocument::import_new_root(source, "instance:shadow").unwrap();
    let global = base.ref_for_sign("T").unwrap();
    assert!(!base
        .identities()
        .refs
        .iter()
        .any(|r| matches!(&r.target, RefTargetV1::Local { target } if target == &global)));
    let renamed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: global.clone(),
            change: NodeUpdate::Rename("Global".into()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert!(renamed.source().contains("trait Owner<T>"));
    assert!(renamed.source().contains("Box<T>"));
    let deleted = apply_edit(
        &renamed,
        PrimitiveEdit::Delete { node: global },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let (source, ids) = deleted.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &ids).unwrap(), deleted);
}
