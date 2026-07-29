//! 步驟 15c — **層②③④ 的統一呼叫語法**:`name(位置參數, key: value, …)`。
//!
//! 層級由**名字解析**決定,不靠關鍵字前綴——現階段只有 P16 的 12 個內建 Atomic
//! Rewrite 解析得出來(封閉內建集);Recipe/Goal(步驟 16–17)落地後沿用同一文法。
//! 與 `clone` 同構:呼叫只活在未解析層,`resolve` 就降成四原語,
//! **`ResolvedChangeSet` 維持 primitive-only**(步驟 14 契約)。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, DonorSpec, ReplayError, ResolvedChangeSet,
    UnresolvedChangeSet,
};
use conlang_language::{Language, LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol b
Symbol o
Symbol k

trait LocalNoun:

global trait Core:

sign book:
    belongs LocalNoun
    entrenchment = 0.5
    phon:
        /bo/
    syn:
        category = noun
    sem:
        senses:
            core = BOOK
            log = LOGBOOK
        edges:
            log from core metonymy

sign kobo:
    belongs LocalNoun
    phon:
        /ko/
    sem:
        senses:
            only = KOBO
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("base parses")
}

fn resolve(chg: &str, ns: &str) -> Result<ResolvedChangeSet, ReplayError> {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(chg);
    UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
}

fn apply(chg: &str, ns: &str) -> String {
    let base = base();
    let spec = LibrarySpec::default();
    let resolved = resolve(chg, ns).expect("resolve");
    let lang = ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .expect("replay")
        .document
        .source()
        .to_owned();
    Language::parse(&lang).expect("result .lang re-parses");
    lang
}

fn statement(body: &str) -> String {
    format!("\n    statement 0:\n        {body}\n")
}

// ── 呼叫語法 ──────────────────────────────────────────────────────────────

#[test]
fn a_bare_call_lowers_to_primitives() {
    let lang = apply(
        &statement("reanalyze(sign(\"book\"), target: category, to: aux)"),
        "evo:re",
    );
    assert!(lang.contains("category = aux"), "{lang}");
}

#[test]
fn named_arguments_use_colons() {
    let lang = apply(
        &statement("drift(sign(\"book\"), sense: log, gloss: JOURNAL)"),
        "evo:drift",
    );
    assert!(lang.contains("log = JOURNAL"), "{lang}");
    assert!(lang.contains("core = BOOK"), "sibling untouched:\n{lang}");
}

#[test]
fn a_call_can_expand_to_several_primitives() {
    let resolved = resolve(
        &statement(
            "derive_sense(sign(\"book\"), from: core, name: fig, gloss: FIGURATIVE, kind: metaphor)",
        ),
        "evo:derive",
    )
    .expect("resolve");
    // derive_sense = insert(sense) + insert(edge)
    assert_eq!(resolved.statements[0].edits.len(), 2);
    let lang = apply(
        &statement(
            "derive_sense(sign(\"book\"), from: core, name: fig, gloss: FIGURATIVE, kind: metaphor)",
        ),
        "evo:derive2",
    );
    assert!(lang.contains("fig = FIGURATIVE") && lang.contains("fig from core metaphor"));
}

#[test]
fn a_call_with_a_block_carries_a_whole_sign() {
    // 驗的是**尾接 block 的機制**。原本用 `adopt` 示範,但 v0.3 起 `adopt` 改為指名
    // (內容來自 donor,不再由呼叫端寫在 block 裡),故改用仍收 block 的 `create`。
    let lang = apply(
        concat!(
            "\n    statement 0:\n        create():\n",
            "            sign kaffe:\n",
            "                belongs LocalNoun\n",
            "                sem:\n                    senses:\n                        core = COFFEE\n",
        ),
        "evo:create",
    );
    assert!(lang.contains("sign kaffe:"), "{lang}");
}

// ── donor 指名借入(P62 §7;提案 A)────────────────────────────────────────

fn donor_language() -> LanguageDocument {
    LanguageDocument::import_new_root(
        "sign kaffe:\n    belongs LocalNoun\n    sem:\n        senses:\n            core = COFFEE\n",
        "fr",
    )
    .expect("donor parses")
}

const DONOR_NODE: &str = "3f2a9b7c5d1e8046a2b3c4d5e6f70819a2b3c4d5e6f708192a3b4c5d6e7f8091";

/// 帶 donor 的 `.chg`:prelude 宣告別名 → node-id,內容由 `DonorSpec` 注入。
fn apply_with_donor(body: &str, ns: &str) -> Result<String, ReplayError> {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(&format!("    donor fr {DONOR_NODE}\n"));
    source.push_str(body);
    let mut donors = DonorSpec::new();
    donors.insert(DONOR_NODE, donor_language());
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve_with(&base, &spec, &donors)?;
    Ok(ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .expect("replay")
        .document
        .source()
        .to_owned())
}

#[test]
fn adopt_names_a_sign_in_a_declared_donor() {
    // **提案 A**:`adopt(<別名>.sign("x"), source: …)`。「怎麼挑」從此在引擎裡
    // ——先前是呼叫端把整個 sign 寫在 block 裡遞進來(P62 §7.1 的自白)。
    let lang = apply_with_donor(
        &statement("adopt(fr.sign(\"kaffe\"), source: loan)"),
        "evo:adopt",
    )
    .expect("borrows");
    assert!(lang.contains("sign kaffe:"), "{lang}");
    assert!(
        lang.contains("provenance = loan"),
        "借來的詞要標記來源:{lang}"
    );
    assert!(lang.contains("COFFEE"), "內容要真的從 donor 帶過來:{lang}");
}

#[test]
fn adopting_from_an_undeclared_donor_is_rejected() {
    // §7.3 的第一道硬錯:body 引用了 prelude 沒宣告的別名。
    let err = apply_with_donor(
        &statement("adopt(wo.sign(\"kaffe\"), source: loan)"),
        "evo:adopt",
    )
    .expect_err("wo 沒宣告");
    assert!(format!("{err}").contains("UNDECLARED_DONOR"), "{err}");
}

#[test]
fn adopting_a_sign_the_donor_does_not_have_is_rejected() {
    // **指名借入的價值就在這裡**:借不到會**當場報錯**,不會靜默少借一個。
    // (條件式篩選就沒有這個性質——選不到只是選出比較少,不報錯。)
    let err = apply_with_donor(
        &statement("adopt(fr.sign(\"ndox\"), source: loan)"),
        "evo:adopt",
    )
    .expect_err("fr 裡沒有 ndox");
    assert!(format!("{err}").contains("DONOR_SIGN_NOT_FOUND"), "{err}");
}

#[test]
fn adopt_without_a_donor_prefix_says_so() {
    // 舊寫法(裸 selector)要給明確訊息,不是含糊的「解析失敗」。
    let err = apply_with_donor(
        &statement("adopt(sign(\"kaffe\"), source: loan)"),
        "evo:adopt",
    )
    .expect_err("缺 donor 前綴");
    assert!(format!("{err}").contains("donor alias"), "{err}");
}

#[test]
fn a_list_argument_is_space_separated() {
    // 逗號是參數分隔符,故清單用空白分隔。
    let lang = apply(
        &statement("split(sign(\"kobo\"), name: kobolet, senses: only)"),
        "evo:split",
    );
    assert!(lang.contains("sign kobolet:"), "{lang}");
    assert!(lang.contains("origin = sign(kobo)"), "{lang}");
}

#[test]
fn a_rule_home_argument_accepts_trait_and_sign() {
    let lang = apply(
        &statement("sound_change(trait(\"Core\"), body: \"b => k\")"),
        "evo:sc",
    );
    assert!(lang.contains("b => k"), "{lang}");
}

// ── 與步驟 14 契約的關係 ──────────────────────────────────────────────────

#[test]
fn the_resolved_changeset_stays_primitive_only() {
    // 呼叫是**未解析層**的糖:dump 出來只有四原語,沒有 `reanalyze(` 字樣。
    let resolved = resolve(
        &statement("reanalyze(sign(\"book\"), target: category, to: aux)"),
        "evo:dump",
    )
    .expect("resolve");
    let dump = resolved.dump();
    assert!(
        !dump.contains("reanalyze("),
        "the call must not survive into the resolved form:\n{dump}"
    );
    assert!(dump.contains("update "), "lowered to a primitive:\n{dump}");
    // round-trip 沿用既有機制。
    let base = base();
    let spec = LibrarySpec::default();
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump, "dump→parse→resolve 逐位元穩定");
}

#[test]
fn a_call_and_its_hand_written_primitive_agree() {
    // 差分:呼叫降階的結果,必須和直接手寫那條原語一模一樣。
    let via_call = apply(
        &statement("reanalyze(sign(\"book\"), target: category, to: aux)"),
        "evo:a",
    );
    let via_primitive = apply(
        &statement("update sign(\"book\").def[syn.category].value = aux"),
        "evo:b",
    );
    assert_eq!(via_call, via_primitive);
}

// ── 封閉內建集 ────────────────────────────────────────────────────────────

#[test]
fn an_unknown_name_is_rejected_as_a_closed_set() {
    let err = resolve(&statement("grammaticalize(sign(\"book\"))"), "evo:unk").unwrap_err();
    let text = format!("{err}");
    assert!(
        text.contains("closed built-in set"),
        "expected the closed-set error, got {text}"
    );
}

#[test]
fn a_missing_or_malformed_argument_is_rejected() {
    for (body, why) in [
        ("drift(sign(\"book\"), sense: log)", "missing gloss:"),
        (
            "entrench(sign(\"book\"), delta: lots)",
            "delta is not a number",
        ),
        (
            "derive_sense(sign(\"book\"), from: core, name: n, gloss: G, kind: sideways)",
            "unknown derivation kind",
        ),
        (
            "reanalyze(sign(\"book\"), target: colour, to: x)",
            "unknown target",
        ),
        (
            "drift(book, sense: log, gloss: G)",
            "positional is not a selector",
        ),
    ] {
        assert!(
            resolve(&statement(body), "evo:bad").is_err(),
            "expected rejection ({why}): {body}"
        );
    }
}

#[test]
fn a_primitive_statement_is_still_a_primitive_statement() {
    // 迴歸:層① 的關鍵字語句不因新增呼叫文法而被誤判。
    let lang = apply(
        &statement("update sign(\"book\").sense[\"log\"].gloss = JOURNAL"),
        "evo:prim",
    );
    assert!(lang.contains("log = JOURNAL"), "{lang}");
}
