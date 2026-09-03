//! C1/C3:分支內的規則要有**穩定身分**與可定址路徑,`.chg` 才編輯得到它。
//!
//! 這一層是**免費得到的**:P93 決定不新增 `Expression` variant、改用既有的
//! `DimFragment`,而 identity 列舉早已把 `SignFragment | DimFragment` 的 items
//! 當作分支的直接子節點處理。本檔把「免費」釘成契約,免得日後有人改回去。

use conlang_language::{AddressSegment, LanguageDocument, NodeKind};

const SOURCE: &str = r#"Feature Type(*cons, vowel)
Symbol a [vowel]
Symbol i [vowel]
Symbol n [cons]
Symbol g [cons]
Symbol s [cons]
Class vowel {a, i}

trait Ablauting:
    syn:
        feature:
            k = enum(one)

sign sing:
    belongs Ablauting
    phon:
        /sing/
        realization:
            case:
                $self == [Ablauting]:
                    /singing/
                    i => a / _ n g
                else:
                    /sing/
"#;

/// 🔑 分支裡的規則是一個有 id 的節點,位址 = 分支底下的 `Items(n)`。
#[test]
fn a_rule_inside_a_branch_is_an_addressable_node() {
    let document = LanguageDocument::import_new_root(SOURCE, "evo:proto").expect("import");
    let manifest = document.manifest_json().expect("manifest");

    let rule_entries: Vec<_> = document
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.kind == NodeKind::Rule)
        .collect();
    assert!(
        !rule_entries.is_empty(),
        "分支內的規則應成為節點:\n{manifest}"
    );

    // 位址必須經過 CaseBranches —— 證明它掛在分支下,而不是被當成 sign 的頂層規則。
    assert!(
        rule_entries.iter().any(|entry| entry
            .address
            .0
            .iter()
            .any(|segment| matches!(segment, AddressSegment::CaseBranches(_)))),
        "規則位址應含 CaseBranches:\n{:?}",
        rule_entries.iter().map(|e| &e.address).collect::<Vec<_>>()
    );
}

/// 身分要跨 dump/open 穩定,否則 `.chg` 的定址會在往返後失效。
#[test]
fn branch_rule_identity_survives_a_round_trip() {
    let first = LanguageDocument::import_new_root(SOURCE, "evo:proto").expect("import");
    let (source, manifest) = first.dump_pair().expect("dump");
    let reopened = LanguageDocument::open(&source, &manifest).expect("open");
    assert_eq!(first, reopened, "分支內規則的身分應往返不變");
}
