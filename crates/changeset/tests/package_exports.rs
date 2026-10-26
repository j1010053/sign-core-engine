use conlang_changeset::{
    apply_edit_with_packages, change_set_prelude_with_packages, ChangeInterpreter, NodeUpdate,
    PrimitiveEdit, UnresolvedChangeSet,
};
use conlang_language::construction::SlotMap;
use conlang_language::{
    compile_with_packages_ref, LanguageDocument, LibraryCatalog, PackageId, PackageResolver,
    PackageSources, PackageSpec, ResolvedPackages,
};

fn packages() -> ResolvedPackages {
    let source = PackageSources {
        config: "schema = 2\nid = \"natural:english\"\nversion = \"1\"\nlayer = \"overlay\"\ncapabilities = [\"traits\", \"signs\"]\ncode = [\"main.lang\"]\n".into(),
        code: "Symbol a\nSymbol b\nClass vowel {a, b}\n\nmarker trait Secret:\n\nexport marker trait Noun:\n\nexport sign word:\n    belongs Noun\n    phon:\n        /a/\n".into(),
        ..Default::default()
    };
    let mut spec =
        PackageSpec::default().with_root("natural:english".parse::<PackageId>().unwrap());
    spec.aliases
        .insert("English".into(), "natural:english".parse().unwrap());
    LibraryCatalog::with_packages([source])
        .unwrap()
        .resolve(&spec)
        .unwrap()
}

fn replay(base: &LanguageDocument, packages: &ResolvedPackages, body: &str) -> LanguageDocument {
    let mut source = change_set_prelude_with_packages(base, packages, "evo:exports").unwrap();
    source.push_str(body);
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve_packages(base, packages)
        .unwrap();
    let dump = resolved.dump().unwrap();
    let restored = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve_packages(base, packages)
        .unwrap();
    assert_eq!(restored.dump().unwrap(), dump);
    ChangeInterpreter::with_packages(base.clone(), packages.clone(), "evo:exports")
        .unwrap()
        .run(&restored)
        .unwrap()
        .document
}

#[test]
fn export_updates_reconstruct_and_replay_without_changing_identity() {
    let packages = LibraryCatalog::embedded()
        .unwrap()
        .resolve(&PackageSpec::default())
        .unwrap();
    let base = LanguageDocument::import_new_root("marker trait Noun:\n\nsign word:\n    belongs Noun\n    syn:\n        feature:\n            number = enum(sg, pl)\n            number = sg\n\nparadigm Words:\n    case [syn.number]:\n        base: word()\n", "exports:root").unwrap();
    let after = replay(&base, &packages, "\n    #0:\n        update trait(\"Noun\").exported = true\n    #1:\n        update sign(\"word\").exported = true\n    #2:\n        update paradigm(\"Words\").exported = true\n");
    assert!(after.language().traits[0].exported);
    assert!(after.language().signs[0].exported);
    assert!(after.language().paradigms[0].exported);
    assert_eq!(base.ref_for_trait("Noun"), after.ref_for_trait("Noun"));
    assert_eq!(base.ref_for_sign("word"), after.ref_for_sign("word"));
    assert_eq!(
        base.language().paradigms[0].id,
        after.language().paradigms[0].id
    );
    let edits = conlang_changeset::reconstruct::reconstruct(&base, &after).unwrap();
    assert_eq!(edits.len(), 3);
    assert!(edits.iter().all(|edit| matches!(
        edit,
        PrimitiveEdit::Update {
            change: NodeUpdate::Exported(true),
            ..
        }
    )));
    let mut rebuilt = base.fork("evo:exports").unwrap();
    for edit in edits {
        rebuilt = apply_edit_with_packages(&rebuilt, edit, &packages)
            .unwrap()
            .document;
    }
    assert_eq!(rebuilt.dump_pair().unwrap(), after.dump_pair().unwrap());
    let (source, identities) = after.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &identities).unwrap(), after);
}

#[test]
fn exported_insert_blocks_preserve_all_three_kinds() {
    let packages = LibraryCatalog::embedded()
        .unwrap()
        .resolve(&PackageSpec::default())
        .unwrap();
    let base = LanguageDocument::import_new_root("", "exports:root").unwrap();
    let after = replay(&base, &packages, "\n    #0:\n        insert into language at end:\n            export marker trait Noun:\n    #1:\n        insert into language at end:\n            export sign word:\n                belongs Noun\n                syn:\n                    feature:\n                        number = enum(sg, pl)\n                        number = sg\n    #2:\n        insert into language at end:\n            export paradigm Words:\n                case [syn.number]:\n                    base: word()\n");
    assert!(after.language().traits[0].exported);
    assert!(after.language().signs[0].exported);
    assert!(after.language().paradigms[0].exported);
}

#[test]
fn qualified_public_override_survives_changeset_replay_and_reopen() {
    let packages = packages();
    let base = LanguageDocument::import_new_root("", "exports:root").unwrap();
    let after = replay(&base, &packages, "\n    #0:\n        insert into language at end:\n            sign English.word:\n                belongs English.Noun\n                phon:\n                    /b/\n");
    let (source, ids) = after.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&source, &ids).unwrap();
    let system = compile_with_packages_ref(reopened.language(), &packages).unwrap();
    assert_eq!(
        system
            .derive("English.word", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "b"
    );
    assert!(packages.packages()[0].code.contains("/a/"));
    let edits = conlang_changeset::reconstruct::reconstruct(&base, &after).unwrap();
    let mut rebuilt = base.fork("evo:exports").unwrap();
    for edit in edits {
        rebuilt = apply_edit_with_packages(&rebuilt, edit, &packages)
            .unwrap()
            .document;
    }
    assert_eq!(rebuilt.dump_pair().unwrap(), after.dump_pair().unwrap());
}

#[test]
fn private_qualified_override_is_rejected_during_edit_validation() {
    let packages = packages();
    let base = LanguageDocument::import_new_root("marker trait Local:\n", "exports:root").unwrap();
    let edit = PrimitiveEdit::Update {
        node: base.ref_for_trait("Local").unwrap(),
        change: NodeUpdate::Rename("English.Secret".into()),
    };
    let error = apply_edit_with_packages(&base, edit, &packages).unwrap_err();
    assert!(
        matches!(error, conlang_changeset::EditError::Validation(report)
        if report.diagnostics().iter().any(|d| d.message.contains("PACKAGE_PRIVATE_SYMBOL")))
    );
    assert!(base.language().trait_named("Local").is_some());
}

#[test]
fn alias_changes_are_rejected_before_resolution_and_replay() {
    let first = packages();
    let source = PackageSources {
        config: "schema = 2\nid = \"natural:english\"\nversion = \"1\"\nlayer = \"overlay\"\ncapabilities = [\"traits\", \"signs\"]\ncode = [\"main.lang\"]\n".into(),
        code: first.packages()[0].code.clone(), ..Default::default()
    };
    let catalog = LibraryCatalog::with_packages([source]).unwrap();
    let mut renamed_spec = first.intent.clone();
    renamed_spec.aliases.remove("English");
    renamed_spec
        .aliases
        .insert("Changed".into(), "natural:english".parse().unwrap());
    let renamed = catalog.resolve(&renamed_spec).unwrap();
    assert_eq!(
        first.lock_digest(),
        renamed.lock_digest(),
        "identical package bytes"
    );
    let base = LanguageDocument::import_new_root("", "exports:root").unwrap();
    let source = change_set_prelude_with_packages(&base, &first, "evo:exports").unwrap();
    assert!(source.contains("package_aliases = sha256:"));
    let unresolved = UnresolvedChangeSet::parse(&source).unwrap();
    let resolved = unresolved.resolve_packages(&base, &first).unwrap();
    assert!(matches!(unresolved.resolve_packages(&base, &renamed),
        Err(conlang_changeset::ReplayError::LibraryLockMismatch(message)) if message.contains("alias")));
    assert!(
        matches!(ChangeInterpreter::with_packages(base, renamed, "evo:exports").unwrap().run(&resolved),
        Err(conlang_changeset::ReplayError::LibraryLockMismatch(message)) if message.contains("alias"))
    );
}
