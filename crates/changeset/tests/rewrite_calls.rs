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
        feature:
            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)
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
        &statement("reanalyze(sign(\"book\"), target: category, to: Aux)"),
        "evo:re",
    );
    assert!(lang.contains("belongs Aux"), "{lang}");
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
        concat!(
            "sign kaffe:\n    belongs LocalNoun\n    sem:\n        senses:\n            core = COFFEE\n\n",
            "sign vin:\n    belongs LocalNoun\n    sem:\n        senses:\n            core = WINE\n",
        ),
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
    // 解析層的錯誤也要傳出去 —— 用 `unwrap` 會讓它變成 panic 而非可斷言的 `Err`。
    let resolved = UnresolvedChangeSet::parse(&source)?.resolve_with(&base, &spec, &donors)?;
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
        &statement("reanalyze(sign(\"book\"), target: category, to: Aux)"),
        "evo:dump",
    )
    .expect("resolve");
    let dump = resolved.dump().expect("dump");
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
    assert_eq!(round.dump().expect("dump"), dump, "dump→parse→resolve 逐位元穩定");
}

#[test]
fn a_call_and_its_hand_written_primitive_agree() {
    // 差分:呼叫降階的結果,必須和直接手寫那條原語一模一樣。
    let via_call = apply(
        &statement("reanalyze(sign(\"book\"), target: category, to: Aux)"),
        "evo:a",
    );
    let via_primitive = apply(
        &statement("update sign(\"book\").belongs[\"LocalNoun\"].target = Aux"),
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

// ── 提案 B:清單形(語法糖)────────────────────────────────────────────────

#[test]
fn adopt_takes_a_list_of_sign_names() {
    // **與 `clone` 同構的語法糖**:只活在未解析層,`resolve` 就降成 N 個 Insert,
    // `ResolvedChangeSet` 維持 primitive-only(步驟 14 契約)。
    let lang = apply_with_donor(
        concat!(
            "\n    statement 0:\n        adopt(from: fr, source: loan):\n",
            "            kaffe\n",
            "            vin\n",
        ),
        "evo:adopt",
    )
    .expect("borrows both");
    assert!(lang.contains("sign kaffe:"), "{lang}");
    assert!(lang.contains("sign vin:"), "{lang}");
    assert_eq!(
        lang.matches("provenance = loan").count(),
        2,
        "兩個都要標記來源:{lang}"
    );
}

#[test]
fn the_list_form_lowers_to_plain_primitives() {
    // 語法糖的檢驗:降階後**看不出**用的是 A 還是 B —— 兩者都是 N 個 Insert。
    let base = base();
    let spec = LibrarySpec::default();
    let mut donors = DonorSpec::new();
    donors.insert(DONOR_NODE, donor_language());

    let resolve_one = |body: &str| {
        let mut source = change_set_prelude(&base, &spec, "evo:adopt").unwrap();
        source.push_str(&format!("    donor fr {DONOR_NODE}\n"));
        source.push_str(body);
        UnresolvedChangeSet::parse(&source)
            .unwrap()
            .resolve_with(&base, &spec, &donors)
            .expect("resolves")
    };
    let listed = resolve_one(concat!(
        "\n    statement 0:\n        adopt(from: fr, source: loan):\n",
        "            kaffe\n            vin\n",
    ));
    let named = resolve_one(concat!(
        "\n    statement 0:\n        adopt(fr.sign(\"kaffe\"), source: loan)\n",
        "        adopt(fr.sign(\"vin\"), source: loan)\n",
    ));
    assert_eq!(listed.statements, named.statements, "B 就是 A 的批次寫法");
}

#[test]
fn mixing_the_two_adopt_forms_is_rejected() {
    // 同時給了位置參數與 `from:`,「以哪個為準」會變成任意的 —— 而那是靜默的。
    let err = apply_with_donor(
        concat!(
            "\n    statement 0:\n        adopt(fr.sign(\"kaffe\"), from: fr, source: loan):\n",
            "            vin\n",
        ),
        "evo:adopt",
    )
    .expect_err("兩種形式混用");
    assert!(format!("{err}").contains("not both"), "{err}");
}

#[test]
fn a_blank_adopt_list_is_rejected() {
    // 空白清單被**更早的 parser** 擋下(它視為「沒有 block」),故 `adopt_list` 裡的
    // 空清單檢查從表面走不到——誠實標記,不假裝那條分支有被測到。
    let err = apply_with_donor(
        "\n    statement 0:\n        adopt(from: fr, source: loan):\n            \n",
        "evo:adopt",
    )
    .expect_err("空清單");
    assert!(format!("{err}").contains("block"), "{err}");
}

#[test]
fn a_repeated_name_in_the_adopt_list_is_rejected() {
    // 借入兩份同名 sign。之後名字唯一性也會擋,但在這裡說離成因近得多。
    let err = apply_with_donor(
        concat!(
            "\n    statement 0:\n        adopt(from: fr, source: loan):\n",
            "            kaffe\n            kaffe\n",
        ),
        "evo:adopt",
    )
    .expect_err("重複");
    assert!(format!("{err}").contains("repeats"), "{err}");
}

#[test]
fn a_lang_fragment_in_the_adopt_list_is_rejected() {
    // 清單是**每行一個名字**,不是 `.lang` 片段。誤寫要給明確訊息。
    let err = apply_with_donor(
        concat!(
            "\n    statement 0:\n        adopt(from: fr, source: loan):\n",
            "            sign kaffe:\n",
        ),
        "evo:adopt",
    )
    .expect_err("誤寫成 .lang");
    assert!(format!("{err}").contains("one sign name per line"), "{err}");
}

// ── 來源派生的驗證(docs/06 §3.1/§3.2)──────────────────────────────────────

#[test]
fn the_source_of_each_loan_is_derivable_from_the_reference_structure() {
    // 規格說「**`source` 不是獨立欄位**……由引用結構派生」。這裡**實際做一次派生**
    // ——不是靠推理:從 `.chg` 原文推回「kaffe 來自哪個節點」。
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:adopt").unwrap();
    source.push_str(&format!("    donor fr {DONOR_NODE}\n"));
    source.push_str(concat!(
        "\n    statement 0:\n        adopt(from: fr, source: loan):\n",
        "            kaffe\n            vin\n",
    ));
    let parsed = UnresolvedChangeSet::parse(&source).unwrap();

    let adoptions = parsed.adoptions().expect("派生得出來");
    assert_eq!(adoptions.len(), 2, "兩個借入事件都要在");
    assert_eq!(adoptions[0].sign, "kaffe");
    assert_eq!(
        adoptions[0].donor.node, DONOR_NODE,
        "推回的是**來源節點**,不只是「這是借的」"
    );
    assert_eq!(adoptions[1].sign, "vin");
    assert_eq!(adoptions[1].donor.node, DONOR_NODE);
}

#[test]
fn the_named_form_derives_the_same_way() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:adopt").unwrap();
    source.push_str(&format!("    donor fr {DONOR_NODE}\n"));
    source.push_str(&statement("adopt(fr.sign(\"kaffe\"), source: loan)"));
    let adoptions = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .adoptions()
        .expect("派生");
    assert_eq!(adoptions.len(), 1);
    assert_eq!(adoptions[0].sign, "kaffe");
    assert_eq!(adoptions[0].donor.node, DONOR_NODE);
}

#[test]
fn a_changeset_that_borrows_nothing_derives_nothing() {
    // 判別性:沒有 adopt 的 changeset 不該憑空生出借用事件。
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:plain").unwrap();
    source.push_str(&statement("entrench(sign(\"book\"), delta: 0.1)"));
    assert!(UnresolvedChangeSet::parse(&source)
        .unwrap()
        .adoptions()
        .unwrap()
        .is_empty());
}

#[test]
fn the_derivation_needs_the_authored_text_not_the_resolved_form() {
    // **界線**:`resolve` 之後 `adopt` 已降階成 Insert,引用結構就沒了。
    // P56 存在邊上、rebase 也刻意保留的正是**原文**,故派生永遠做得到 ——
    // 但若有人拿 `dump()` 的結果去派生,會得到空的。這裡把那條界線釘住。
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:adopt").unwrap();
    source.push_str(&format!("    donor fr {DONOR_NODE}\n"));
    source.push_str(&statement("adopt(fr.sign(\"kaffe\"), source: loan)"));
    let mut donors = DonorSpec::new();
    donors.insert(DONOR_NODE, donor_language());

    let authored = UnresolvedChangeSet::parse(&source).unwrap();
    assert_eq!(authored.adoptions().unwrap().len(), 1, "原文派生得出來");

    let dumped = authored
        .resolve_with(&base, &spec, &donors)
        .expect("resolves")
        .dump()
        .expect("dump");
    let reparsed = UnresolvedChangeSet::parse(&dumped).unwrap();
    assert!(
        reparsed.adoptions().unwrap().is_empty(),
        "降階後引用結構已消失 —— 派生只能用作者原文"
    );
    assert_eq!(
        reparsed.donors, authored.donors,
        "但 donor 宣告仍在:節點層級的『讀了哪些語言』不會遺失"
    );
}

#[test]
fn each_loan_is_traced_to_its_own_donor() {
    // **判別性**:兩個 donor 各借一個詞。只有一個 donor 的測試證明不了別名有被解析
    // ——「永遠取第一個 donor」那種改壞法在單 donor 下完全看不出來。
    let other = DONOR_NODE.replace("3", "9");
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:adopt").unwrap();
    source.push_str(&format!("    donor fr {DONOR_NODE}\n"));
    source.push_str(&format!("    donor wo {other}\n"));
    source.push_str(concat!(
        "\n    statement 0:\n        adopt(fr.sign(\"kaffe\"), source: loan)\n",
        "        adopt(wo.sign(\"ndox\"), source: loan)\n",
    ));

    let adoptions = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .adoptions()
        .expect("派生");
    assert_eq!(adoptions.len(), 2);
    assert_eq!(adoptions[0].sign, "kaffe");
    assert_eq!(adoptions[0].donor.node, DONOR_NODE, "kaffe 來自 fr");
    assert_eq!(adoptions[1].sign, "ndox");
    assert_eq!(adoptions[1].donor.node, other, "ndox 來自 wo,不是 fr");
}
