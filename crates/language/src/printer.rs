//! canonical printer(P21 + I22):`Language → text`,**確定性** colon+縮排 形式。
//!
//! 區段序固定:dsl 域 → prosody → distribution → global trait → trait → sign
//! (具名容器按名排序,I15-d)。容器 body **依維度分組**(I22):belongs → `Name[n]`
//! → 頂層 Def → 維度區塊(固定序 syn/phon/sem/prag);`syn:` 內 `slots:` 先於 Def;
//! 維度內 slot/Def/Rule 保插入序(有序語意)。縮排 4 空格/層。
//!
//! 這份輸出**就是** IR dump 格式(P21);對 canonical 輸入 round-trip 恆等,
//! 非 canonical 正規化為不動點(維度分組是冪等重排)。

use crate::{Block, Language, SignItem, Stage, TraitDef};

const DIMS: [&str; 4] = ["syn", "phon", "sem", "prag"];

fn stage_str(s: Stage) -> &'static str {
    match s {
        Stage::Stem => "stem",
        Stage::Word => "word",
        Stage::Phrase => "phrase",
    }
}

/// Def 的維度歸屬(path 前綴);None = 非維度(頂層,如 entrenchment)。
fn def_dim(path: &str) -> Option<&str> {
    let head = path.split_once('.').map(|(h, _)| h).unwrap_or(path);
    DIMS.contains(&head).then_some(head)
}

fn push_rule(out: &mut String, indent: &str, r: &crate::Rule) {
    out.push_str(&format!("{indent}{} @stage {}\n", r.body, stage_str(r.stage)));
    for e in &r.else_chain {
        out.push_str(&format!("{indent}    else {e}\n")); // 分支共享 stage(P22)
    }
}

/// 印一個容器 body(統一:trait/sign 同排版)。`blocks` 保 `==` 邊界(trait)。
fn push_body(out: &mut String, blocks: &[Block]) {
    for (bi, block) in blocks.iter().enumerate() {
        if bi > 0 {
            out.push_str("    ==\n"); // P27 block 邊界
        }
        let items = &block.items;
        // 1) belongs(保序)
        for it in items {
            if let SignItem::Belongs(n) = it {
                out.push_str(&format!("    belongs {n}\n"));
            }
        }
        // 2) trait macro 引用(保序):None = 裸 Name(整個 trait)、Some(n) = Name[n]
        for it in items {
            if let SignItem::TraitUse { name, block } = it {
                match block {
                    Some(n) => out.push_str(&format!("    {name}[{n}]\n")),
                    None => out.push_str(&format!("    {name}\n")),
                }
            }
        }
        // 3) 頂層非維度 Def(保序)
        for it in items {
            if let SignItem::Def(d) = it {
                if def_dim(&d.path).is_none() {
                    out.push_str(&format!("    {} = {}\n", d.path, d.value));
                }
            }
        }
        // 4) 維度區塊(固定序 syn/phon/sem/prag)
        for dim in DIMS {
            let has_slot = dim == "syn" && items.iter().any(|it| matches!(it, SignItem::Slot(_)));
            let has_def = items.iter().any(
                |it| matches!(it, SignItem::Def(d) if def_dim(&d.path) == Some(dim)),
            );
            let has_rule = items
                .iter()
                .any(|it| matches!(it, SignItem::Rule(r) if rule_dim(r) == dim));
            if !(has_slot || has_def || has_rule) {
                continue;
            }
            out.push_str(&format!("    {dim}:\n"));
            if has_slot {
                out.push_str("        slots:\n");
                for it in items {
                    if let SignItem::Slot(s) = it {
                        out.push_str(&format!(
                            "            {} [{}]{}\n",
                            s.name,
                            s.filler,
                            if s.optional { "?" } else { "" }
                        ));
                    }
                }
            }
            // Def / Rule 保插入序(逐項掃,屬本維者印)
            for it in items {
                match it {
                    SignItem::Def(d) if def_dim(&d.path) == Some(dim) => {
                        let field = d.path.strip_prefix(&format!("{dim}.")).unwrap_or(&d.path);
                        if d.path == "phon" {
                            out.push_str(&format!("        {}\n", d.value)); // phon UR/模板
                        } else {
                            out.push_str(&format!("        {field} = {}\n", d.value));
                        }
                    }
                    SignItem::Rule(r) if rule_dim(r) == dim => push_rule(out, "        ", r),
                    _ => {}
                }
            }
        }
    }
}

/// 規則歸維(I25/P44):由 `Rule.dim` 決定(phon/syn/sem/prag)。
fn rule_dim(r: &crate::Rule) -> &'static str {
    match r.dim {
        crate::Dim::Phon => "phon",
        crate::Dim::Syn => "syn",
        crate::Dim::Sem => "sem",
        crate::Dim::Prag => "prag",
    }
}

fn push_trait(out: &mut String, t: &TraitDef) {
    let kw = if t.global { "global trait" } else { "trait" };
    out.push_str(&format!("{kw} {}:\n", t.name));
    push_body(out, &t.blocks);
}

/// canonical 印出;空 Language → 空字串。
pub fn print(l: &Language) -> String {
    let mut sections: Vec<String> = Vec::new();

    if !l.dsl_decls.is_empty() {
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
        entries.sort();
        let mut s = String::from("distribution:\n");
        for (k, v) in entries {
            s.push_str(&format!("    {k} = {v}\n"));
        }
        sections.push(s);
    }
    // 具名容器按名排序;global trait 先於一般 trait(區段序固定)
    let mut traits: Vec<&TraitDef> = l.traits.iter().collect();
    traits.sort_by(|a, b| a.name.cmp(&b.name));
    for t in traits.iter().filter(|t| t.global) {
        let mut s = String::new();
        push_trait(&mut s, t);
        sections.push(s);
    }
    for t in traits.iter().filter(|t| !t.global) {
        let mut s = String::new();
        push_trait(&mut s, t);
        sections.push(s);
    }
    let mut signs: Vec<_> = l.signs.iter().collect();
    signs.sort_by(|a, b| a.name.cmp(&b.name));
    for sg in signs {
        let mut s = format!("sign {}:\n", sg.name);
        push_body(&mut s, std::slice::from_ref(&Block {
            items: sg.items.clone(),
        }));
        sections.push(s);
    }

    sections.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::*;

    /// P21 確定性:構造順序不同 → canonical 相同(具名容器排序)。
    #[test]
    fn canonical_is_order_insensitive_for_named_containers() {
        let mk = |flip: bool| {
            let mut l = Language::new();
            let a = TraitDef {
                name: "Alpha".into(),
                global: false,
                blocks: vec![Block {
                    items: vec![SignItem::Belongs("Beta".into())],
                }],
            };
            let b = TraitDef {
                name: "Beta".into(),
                global: false,
                blocks: vec![Block::default()],
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
}
