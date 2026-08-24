//! 量測用探針:`Reference` 的 `Path` 段在**查找端**到底怎麼被使用。
//!
//! 文法(承 P22 / 修補05 §3.5)宣稱 Path 支援 `.名` / `[鍵]` / `~tier`,
//! 而引用的求值一路是 `sign.project(dim).get(&path)`(`read_self`)與
//! `FillerSnapshot::scalar`(`read_slot`)——兩者都以**整條路徑當字串鍵**
//! 查表。本檔把「宣稱」與「實際」的差距量出來。
//!
//! `cargo test -p conlang-language --test reference_path_probe -- --nocapture`

use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::ontology::OntologyRegistry;
use conlang_language::synchronic::{self, RuleStatus};
use conlang_language::{compile_system, Dim, Language};

/// `syn:` 下宣告 `decl`,再用一條同維規則把 `reference` 讀進 `probe`。
/// 回傳 (probe 的最終值, 該規則的狀態)。
fn probe(decl: &str, reference: &str) -> (Option<String>, Option<RuleStatus>) {
    let source = format!(
        "trait ProbeThing:\n\nsign subject:\n    belongs ProbeThing\n    syn:\n        {decl}\n        probe => {reference}\n"
    );
    let Ok(language) = Language::parse(&source) else {
        return (None, None);
    };
    let (registry, _) = OntologyRegistry::build(&[&language]);
    let Some(sign) = language.sign_named("subject") else {
        return (None, None);
    };
    let (out, records) = synchronic::run_sign_dim_rules(sign, Dim::Syn, &registry);
    let value = out
        .project(Dim::Syn, &registry)
        .get("syn.probe")
        .map(str::to_owned);
    (value, records.last().map(|record| record.status))
}

fn report(label: &str, decl: &str, reference: &str) -> bool {
    let (value, status) = probe(decl, reference);
    let hit = value.as_deref() == Some("READ");
    println!(
        "  {label:<34} {}   status={:<9} probe={}",
        if hit { "讀到 " } else { "讀不到" },
        status.map(|s| format!("{s:?}")).unwrap_or("—".into()),
        value.as_deref().unwrap_or("—")
    );
    hit
}

#[test]
fn measure_what_the_path_segment_actually_supports() {
    println!("\n── Path 段:文法宣稱 vs 查找端實際 ────────────────────────\n");
    println!("[逐字相同的宣告與引用]");
    let single = report("單段  a", "a = READ", "$self.syn.a");
    let multi = report("多段  a.b", "a.b = READ", "$self.syn.a.b");
    let key = report("鍵    a[k]", "a[k] = READ", "$self.syn.a[k]");
    let tier = report("tier  t~tone", "t~tone = READ", "$self.syn.t~tone");

    println!("\n[結構性解讀:宣告與引用不逐字相同]");
    let index = report("宣告 a,引用 a[0]", "a = READ", "$self.syn.a[0]");
    let navigate = report("宣告 a.b,引用 a", "a.b = READ", "$self.syn.a");
    let descend = report("宣告 a,引用 a.b", "a = READ", "$self.syn.a.b");

    println!("\n── 量測結論 ──────────────────────────────────────────────");
    println!("  逐字相同:單段={single} 多段={multi}");
    println!("  已移除  :鍵={key} tier={tier}(兩者皆應為 false)");
    println!("  結構性  :索引={index} 上行={navigate} 下行={descend}");
    println!(
        "  → Path 段在查找端是{}\n",
        if index || navigate || descend {
            "被結構性解讀的"
        } else {
            "**不透明字串鍵**:點分只是鍵名裡的字元,沒有導覽語意"
        }
    );

    // 釘住量測結果——日後若查找端改為結構性解讀,這裡會紅。
    assert!(single, "單段純識別字必須讀得到");
    assert!(multi, "多段點分路徑逐字相同時讀得到,佐證字串鍵語意");
    assert_eq!(
        (key, tier),
        (false, false),
        "`[鍵]`/`~tier` 已移出 Path 文法(修補13 ⑨)"
    );
    assert_eq!(
        (index, navigate, descend),
        (false, false, false),
        "沒有任何結構性解讀:索引、上行、下行都不成立"
    );
}

// ── `$slot.` 側:走 compile_system 的真實路徑 ──────────────────────────
//
// 上面的 `$self` 探針直接呼叫 `run_sign_dim_rules`,繞過 P71 封閉清單。
// 這裡走完整的 compile,故路徑必須是清單上的**套件座標前綴**
// (`syn.alignment.…` 等)——那正是現實中多段路徑唯一的來源。

/// filler 在 `<prefix>.<leaf>` 放一個值,construction 用 `$slot.item.…` 讀它。
fn slot_probe(prefix: &str, leaf: &str) -> Result<bool, String> {
    let dim = prefix
        .split('.')
        .next()
        .expect("prefix carries its dimension");
    let source = format!(
        "Symbol a\nClass vowel {{a}}\n\ntrait ProbeThing:\n\n\
         sign filler:\n    belongs ProbeThing\n    phon:\n        /a/\n    {dim}:\n        {rest}.{leaf} = READ\n\n\
         sign holder:\n    belongs ProbeThing\n    syn:\n        slots:\n            item [ProbeThing]\n    \
         phon:\n        /{{$slot.item}}/\n    sem:\n        senses:\n            core = HOLD\n    {dim}:\n        \
         {rest}.probe => $slot.item.{prefix}.{leaf}\n",
        rest = prefix.split_once('.').expect("prefix is <dim>.<head>").1,
    );
    let language = Language::parse(&source).map_err(|error| format!("parse: {error}"))?;
    let system = compile_system(language).map_err(|error| {
        format!(
            "compile: {}",
            format!("{error}").chars().take(90).collect::<String>()
        )
    })?;
    let derivation = system
        .derive(
            "holder",
            &[SlotFiller::sign("item", "filler")],
            &SlotMap::identity(),
        )
        .map_err(|error| format!("derive: {error}"))?;
    let key = format!("{prefix}.probe");
    Ok(match dim {
        "syn" => derivation
            .token
            .syn
            .iter()
            .any(|(k, v)| *k == key && v == "READ"),
        "prag" => derivation
            .token
            .prag
            .iter()
            .any(|(k, v)| *k == key && v == "READ"),
        "sem" => {
            derivation
                .token
                .sem
                .fields
                .iter()
                .any(|(k, v)| k.ends_with("probe") && v == "READ")
                || derivation.token.sem.features.values().any(|v| v == "READ")
        }
        _ => false,
    })
}

fn slot_report(label: &str, prefix: &str, leaf: &str) {
    match slot_probe(prefix, leaf) {
        Ok(true) => println!("  {label:<40} 讀到"),
        Ok(false) => println!("  {label:<40} 讀不到(解析與 compile 都過)"),
        Err(why) => println!("  {label:<40} 擋下  {why}"),
    }
}

#[test]
fn measure_the_slot_side_and_the_closed_list() {
    println!("\n── `$slot.` 側:多段路徑與各維 ───────────────────────────\n");
    println!("[封閉清單上的套件座標;這是多段路徑現實中唯一的來源]");
    slot_report("syn.alignment.a", "syn.alignment", "a");
    slot_report("sem.aspect.a", "sem.aspect", "a");
    slot_report("prag.evidence.a", "prag.evidence", "a");

    println!("\n[更深的段數]");
    slot_report("syn.alignment.a.b(三段)", "syn.alignment", "a.b");

    println!("\n[修補13 ⑨ 移除的兩種段]");
    slot_report("syn.alignment.a[k](鍵)", "syn.alignment", "a[k]");
    slot_report("syn.alignment.t~tone(tier)", "syn.alignment", "t~tone");

    println!("\n[不在清單上的自造路徑]");
    slot_report("syn.made-up.a", "syn.made-up", "a");
    println!();

    // 釘住量測:三個非 phon 維對稱,且段數與段型別對查找端無差別。
    for (prefix, leaf) in [
        ("syn.alignment", "a"),
        ("sem.aspect", "a"),
        ("prag.evidence", "a"),
        ("syn.alignment", "a.b"),
    ] {
        assert_eq!(
            slot_probe(prefix, leaf),
            Ok(true),
            "{prefix}.{leaf} 應可經 $slot 讀到"
        );
    }
    for leaf in ["a[k]", "t~tone"] {
        assert!(
            slot_probe("syn.alignment", leaf).is_err(),
            "`{leaf}` 已移出 Path 文法(修補13 ⑨)"
        );
    }
    // 封閉清單之外的自造路徑進不來——這正是多段路徑只能來自套件座標的原因。
    assert!(
        slot_probe("syn.made-up", "a").is_err(),
        "不在封閉清單上的路徑必須被擋下"
    );
}

// ── 封閉清單本身的形狀 ────────────────────────────────────────────────

#[test]
fn measure_what_shapes_the_closed_list_admits() {
    // 清單是 crate 私有的,這裡以行為反推:對代表性路徑問「Def 合不合法」,
    // 用的是與 authoring 相同的入口(parse + compile)。
    let cases = [
        ("phon", "引擎自有"),
        ("syn.alignment.x", "套件座標 + 一段"),
        ("syn.alignment.x.y.z", "套件座標 + 三段"),
        ("syn.ghost.x", "不在清單上"),
    ];
    println!("\n── Def 路徑封閉清單(P71)接受什麼 ───────────────────────\n");
    for (path, note) in cases {
        let dim = path.split('.').next().expect("dimension");
        let body = match path.split_once('.') {
            Some((_, rest)) => format!("        {rest} = V\n"),
            None => "        /a/\n".to_owned(),
        };
        let source = format!(
            "trait T:\n\nsign s:\n    belongs T\n    phon:\n        /a/\n    {dim}:\n{body}"
        );
        let verdict = Language::parse(&source)
            .map_err(|error| format!("parse: {error}"))
            .and_then(|language| {
                compile_system(language).map_err(|error| {
                    format!(
                        "{}",
                        format!("{error}").chars().take(70).collect::<String>()
                    )
                })
            });
        println!(
            "  {path:<24} {note:<16} {}",
            if verdict.is_ok() { "接受" } else { "拒絕" }
        );
    }
    println!();
}
