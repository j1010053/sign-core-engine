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
    let dump = resolved.dump().expect("dump");
    assert!(dump.contains("insert into node(rule, @"));
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump().expect("dump"), dump);

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
    assert!(resolved.dump().expect("dump").contains("update node(case, @"));
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
    assert!(resolved2.dump().expect("dump").contains("delete node(case_branch, @"));
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
    let dump = resolved.dump().expect("dump");
    assert!(dump.contains("insert into node(case, @"), "{dump}");
    assert!(dump.contains("$self.syn.number == plural"), "{dump}");

    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump().expect("dump"), dump, "case-branch round-trip 穩定");

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

/// 🔑 **realization 內的 `@name` 現在定得到址**(取徑 A,2026-08-18)。
///
/// 此前:標籤 parse 得過、canonical dump 印得出來,但六條路徑全部解不到——
/// `sign("x").case["X"]` 回 `cannot resolve … below <sign>`,
/// `sign("x").realization[0]` 回 `unknown path selector "realization"`。
/// 成因是 `SignItem::Realization` 保留了一層 V1 遺留的 wrapper 節點,
/// 使 `Case` 成為 sign 的**孫**節點,而 `resolve_path_child` 只取直屬子節點。
///
/// 取徑 A 讓它比照另外三個帶運算式的 SignItem 塌陷成 expression root,
/// 於是 realization 的 case 與 sign 層的 case **走完全同一條定址路徑**,
/// selector 清單維持 15 支不新增。
const REALIZATION_SOURCE: &str = r#"Symbol s
Symbol h
Symbol e
Symbol r

trait LocalPronoun:

sign she:
    belongs LocalPronoun
    syn:
        feature:
            case = enum(nominative, accusative)
    phon:
        /she/
        realization:
            case @name pron_case:
                $self.syn.case == accusative @name acc:
                    /her/
                else @name nom:
                    /she/
"#;

#[test]
fn a_named_realization_case_is_addressable_by_label() {
    let base = LanguageDocument::import_new_root(REALIZATION_SOURCE, "evo:root").unwrap();
    assert!(base.source().contains("@name pron_case"), "標籤進 IR");

    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:realization").unwrap();
    source.push_str(
        "\n    statement 0:\n        delete sign(\"she\").case[\"pron_case\"].branch[\"acc\"]\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .expect("realization 的具名 case/branch 必須解得到");

    let doc = ChangeInterpreter::new(base, spec, "evo:realization")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    let rendered = doc.source();
    assert!(!rendered.contains("/her/"), "acc 分支已刪:\n{rendered}");
    assert!(rendered.contains("@name nom"), "其餘分支保留:\n{rendered}");
}
