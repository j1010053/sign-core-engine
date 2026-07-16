//! canonical printer(P21):`Language → text`,**確定性**——
//! 區段序固定(dsl 域 → prosody → distribution → global trait → trait → sign)、
//! 具名容器按名稱排序、規則/block/sign 項目保插入序(有序語意)、distribution 按鍵
//! 排序、縮排 4 空格、區段間單一空行(I15-d)。
//!
//! 這份輸出**就是** IR dump 格式(P21:不必發明,dump = Language 源文字 canonical
//! form);compile 的 ①–④ 每個 pass 產物皆以本 printer 印出,`diff` 即 pass 報告。
//! round-trip(text→IR→text 恆等)於步驟 9(parser)接上後成為 golden 恆等式。

use crate::{Block, Item, Language, SignItem, Stage, TraitDef};

fn stage_str(s: Stage) -> &'static str {
    match s {
        Stage::Stem => "stem",
        Stage::Word => "word",
        Stage::Phrase => "phrase",
    }
}

fn push_item(out: &mut String, item: &Item) {
    match item {
        Item::Def(d) => out.push_str(&format!("    {} = {}\n", d.path, d.value)),
        Item::Rule(r) => out.push_str(&format!("    {} @stage {}\n", r.body, stage_str(r.stage))),
    }
}

fn push_blocks(out: &mut String, blocks: &[Block]) {
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push_str("    ==\n"); // P27:Block 節點邊界
        }
        for item in &b.items {
            push_item(out, item);
        }
    }
}

fn push_trait(out: &mut String, t: &TraitDef) {
    let kw = if t.global { "global trait" } else { "trait" };
    out.push_str(&format!("{kw} {} {{\n", t.name));
    push_blocks(out, &t.blocks);
    out.push_str("}\n");
}

/// canonical 印出;空 Language → 空字串。
pub fn print(l: &Language) -> String {
    let mut sections: Vec<String> = Vec::new();

    if !l.dsl_decls.is_empty() {
        // dsl 域宣告:不透明 verbatim(I15-a;僅正規化尾端空白)
        let body: String = l
            .dsl_decls
            .iter()
            .map(|line| format!("{}\n", line.trim_end()))
            .collect();
        sections.push(body);
    }
    if !l.prosody.is_empty() {
        sections.push(format!("prosody = {}\n", l.prosody.join(" ")));
    }
    if !l.distribution.is_empty() {
        let mut entries = l.distribution.clone();
        entries.sort(); // I15-d:按鍵排序
        let mut s = String::from("distribution {\n");
        for (k, v) in entries {
            s.push_str(&format!("    {k} = {v}\n"));
        }
        s.push_str("}\n");
        sections.push(s);
    }
    // 具名容器按名排序(I15-d);global 先於一般 trait(區段序固定)
    let mut sorted: Vec<&TraitDef> = l.traits.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for t in sorted.iter().filter(|t| t.global) {
        let mut s = String::new();
        push_trait(&mut s, t);
        sections.push(s);
    }
    for t in sorted.iter().filter(|t| !t.global) {
        let mut s = String::new();
        push_trait(&mut s, t);
        sections.push(s);
    }
    let mut signs: Vec<_> = l.signs.iter().collect();
    signs.sort_by(|a, b| a.name.cmp(&b.name));
    for sg in signs {
        let mut s = format!("sign {} {{\n", sg.name);
        for item in &sg.items {
            match item {
                SignItem::TraitUse { name, block } => {
                    s.push_str(&format!("    {name}[{block}]\n"))
                }
                SignItem::Def(d) => s.push_str(&format!("    {} = {}\n", d.path, d.value)),
                SignItem::Rule(r) => {
                    s.push_str(&format!("    {} @stage {}\n", r.body, stage_str(r.stage)))
                }
            }
        }
        s.push_str("}\n");
        sections.push(s);
    }

    sections.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::*;

    /// P21 確定性:構造順序不同 → canonical 輸出相同(具名容器排序)。
    #[test]
    fn canonical_is_order_insensitive_for_named_containers() {
        let mk = |flip: bool| {
            let mut l = Language::new();
            let a = TraitDef {
                name: "Alpha".into(),
                global: false,
                blocks: vec![Block {
                    items: vec![Item::Def(Def {
                        path: "syn.provides".into(),
                        value: "VERB".into(),
                    })],
                }],
            };
            let b = TraitDef {
                name: "Beta".into(),
                global: true,
                blocks: vec![Block { items: vec![] }],
            };
            if flip {
                l.add_trait(b.clone());
                l.add_trait(a.clone());
            } else {
                l.add_trait(a);
                l.add_trait(b);
            }
            l.dump()
        };
        assert_eq!(mk(false), mk(true));
    }

    /// 規則順序是語意(P18 同 stage 內書寫序):printer 必須保序,不得排序。
    #[test]
    fn rule_order_is_preserved() {
        let mut l = Language::new();
        let r1 = l.rule("b => c", Stage::Word);
        let r2 = l.rule("a => b", Stage::Word);
        l.add_trait(TraitDef {
            name: "T".into(),
            global: false,
            blocks: vec![Block {
                items: vec![Item::Rule(r1), Item::Rule(r2)],
            }],
        });
        let out = l.dump();
        let i1 = out.find("b => c").unwrap();
        let i2 = out.find("a => b").unwrap();
        assert!(i1 < i2, "書寫序不得被 canonical 排序破壞");
    }
}
