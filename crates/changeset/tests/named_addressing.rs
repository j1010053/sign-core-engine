//! P 系列取徑 B:rule `@name <label>` → keyed 定址 `rule["label"]`(穩定、非序數)。

use conlang_changeset::{change_set_prelude, ChangeInterpreter, ReplayError, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol d
Symbol o
Symbol g

trait LocalNoun:

sign dog:
    belongs LocalNoun
    syn:
        feature:
            class = enum(transitive, other)
            class => transitive / [Verb] @name classify
    phon:
        /dog/
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").unwrap()
}

#[test]
fn a_named_rule_survives_import_and_round_trip() {
    // `@name` 進 IR 並在 canonical dump 保留。
    let doc = base();
    assert!(doc.source().contains("@name classify"), "{}", doc.source());
}

#[test]
fn rule_addressed_by_label_receives_an_else_branch() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:named").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into sign(\"dog\").rule[\"classify\"] at end:\n            else class => other\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    // label 定址 → stable node(rule,@…);round-trip 穩定。
    let dump = resolved.dump();
    assert!(dump.contains("insert into node(rule, @"));
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump);

    let doc = ChangeInterpreter::new(base, spec, "evo:named")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    let rendered = doc.source();
    assert!(rendered.contains("else class => other"));
    assert!(rendered.contains("@name classify"), "label 保留");
}

#[test]
fn unknown_rule_label_is_rejected() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:badlabel").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into sign(\"dog\").rule[\"nope\"] at end:\n            else class => other\n",
    );
    let err = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap_err();
    assert!(
        matches!(err, ReplayError::StatementSelector { ordinal: 0, .. }),
        "got {err}"
    );
}

const CASE_SOURCE: &str = r#"Symbol w

trait Zverb:
trait Zfinite:

sign walk:
    belongs Zverb
    syn:
        feature:
            number = enum(singular, plural)
    phon:
        /walk/
    case @name agr:
        $self.syn.number == singular @name sg:
            belongs Zfinite
        else @name other:
            belongs Zverb
"#;

#[test]
fn named_case_and_branch_round_trip_and_address() {
    let base = LanguageDocument::import_new_root(CASE_SOURCE, "evo:root").unwrap();
    // round-trip: case/branch 標籤在 canonical dump 保留。
    let src = base.source();
    assert!(src.contains("case @name agr:"), "{src}");
    assert!(src.contains("@name sg:"), "{src}");

    // case["agr"] 定址 → 改 selection。
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:case").unwrap();
    source.push_str(
        "\n    statement 0:\n        update sign(\"walk\").case[\"agr\"].selection = when\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert!(resolved.dump().contains("update node(case, @"));
    let doc = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:case")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert!(
        doc.source().contains("when @name agr:"),
        "selection→when:\n{}",
        doc.source()
    );

    // branch["sg"] 定址 → delete 該分支。
    let mut source2 = change_set_prelude(&base, &spec, "evo:branch").unwrap();
    source2.push_str(
        "\n    statement 0:\n        delete sign(\"walk\").case[\"agr\"].branch[\"sg\"]\n",
    );
    let resolved2 = UnresolvedChangeSet::parse(&source2)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert!(resolved2.dump().contains("delete node(case_branch, @"));
}

/// Step-14 補完:往具名 case 插入一個 case-branch(SignContext guard 分支),
/// 定址 case["agr"];降階 Insert{CaseBranch};round-trip 穩定。
#[test]
fn inserts_a_case_branch_into_a_named_case() {
    let base = LanguageDocument::import_new_root(CASE_SOURCE, "evo:root").unwrap();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:cb").unwrap();
    source.push_str(concat!(
        "\n    statement 0:\n",
        "        insert into sign(\"walk\").case[\"agr\"] at start:\n",
        "            $self.syn.number == plural:\n",
        "                belongs Zverb\n",
    ));
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(resolved.statements[0].edits.len(), 1);
    let dump = resolved.dump();
    assert!(dump.contains("insert into node(case, @"), "{dump}");
    assert!(dump.contains("$self.syn.number == plural"), "{dump}");

    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump, "case-branch round-trip 穩定");

    let doc = ChangeInterpreter::new(base, spec, "evo:cb")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert!(
        doc.source().contains("$self.syn.number == plural"),
        "分支寫入 case:\n{}",
        doc.source()
    );
}
