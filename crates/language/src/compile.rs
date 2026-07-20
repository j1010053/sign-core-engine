//! Compile pipeline ①–④(步驟 10;P21 progressive lowering)。
//!
//! ```text
//! ① Source   ──Trait Expansion(P5 全 block 完整性)──▶ ② Expanded
//! ② Expanded ──Name+Priority Resolution(P6 欄位級)──▶ ③ Resolved
//! ③ Resolved ──Stage 排序(P18)─────────────────────▶ ④ Ordered
//! (⑤ Codegen → Compiled Grammar/Sign = 步驟 11)
//! ```
//! ①–④ **全是合法 Language**(同一文字語法逐步降階):每 pass 純函數
//! `Language → Language`,無隱藏狀態——單獨跑一個 pass = 在完整 pipeline 裡跑它;
//! `diff(dump(②), dump(③))` 即該 pass 的人類可讀報告。實作界定見 I16(docs/05 §9)。
//! Index Generation 為 Compile Artifact(IDE/搜尋),不影響語意(P8)。

use std::collections::BTreeMap;

use crate::{Item, Language, Rule, SignItem};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("sign {sign:?} references unknown trait {name:?}")]
    UnknownTrait { sign: String, name: String },
    #[error("sign {sign:?} references {name:?} block {block}, but it has {blocks} block(s)")]
    BlockOutOfRange {
        sign: String,
        name: String,
        block: u32,
        blocks: usize,
    },
    /// P5:全 block 強制顯式——要嘛完整展開、要嘛完全不用。
    #[error("sign {sign:?} uses trait {name:?} but omits block {missing} (P5: all blocks must be placed explicitly)")]
    IncompleteTraitUse {
        sign: String,
        name: String,
        missing: u32,
    },
    #[error("duplicate trait name {0:?}")]
    DuplicateTrait(String),
    #[error("duplicate sign name {0:?}")]
    DuplicateSign(String),
}

/// 全管線產物:各 stage 的 Language(皆可 dump)+ Trait 索引(Compile Artifact)。
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub expanded: Language,
    pub resolved: Language,
    pub ordered: Language,
    /// trait 名 → 引用它的 sign 名(展開前收集;不影響語意)。
    pub trait_index: BTreeMap<String, Vec<String>>,
}

fn check_names(l: &Language) -> Result<(), CompileError> {
    let mut seen = std::collections::BTreeSet::new();
    for t in &l.traits {
        if !seen.insert(&t.name) {
            return Err(CompileError::DuplicateTrait(t.name.clone()));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for s in &l.signs {
        if !seen.insert(&s.name) {
            return Err(CompileError::DuplicateSign(s.name.clone()));
        }
    }
    Ok(())
}

/// Pass ①→②:Trait Expansion(I16-a/b)。
/// 消去非 global trait 與全部 `TraitUse`(inline 至引用位置,保序);
/// global trait 存續(展開對其恆等);P5 完整性:引用者須寫出該 trait 全部 block。
pub fn expand_traits(src: &Language) -> Result<Language, CompileError> {
    check_names(src)?;
    let mut out = src.clone();
    out.signs.clear();
    out.traits.retain(|t| t.global); // 非 global 宣告消失(P5)

    for sign in &src.signs {
        // P5 完整性檢查:每個被引用 trait 的 block 集合必須完整
        let mut used: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
        for it in &sign.items {
            if let SignItem::TraitUse { name, block } = it {
                used.entry(name).or_default().push(*block);
            }
        }
        for (name, blocks) in &used {
            let t = src
                .traits
                .iter()
                .find(|t| &t.name == name)
                .ok_or_else(|| CompileError::UnknownTrait {
                    sign: sign.name.clone(),
                    name: (*name).to_owned(),
                })?;
            for b in blocks {
                if *b == 0 || *b as usize > t.blocks.len() {
                    return Err(CompileError::BlockOutOfRange {
                        sign: sign.name.clone(),
                        name: (*name).to_owned(),
                        block: *b,
                        blocks: t.blocks.len(),
                    });
                }
            }
            for want in 1..=t.blocks.len() as u32 {
                if !blocks.contains(&want) {
                    return Err(CompileError::IncompleteTraitUse {
                        sign: sign.name.clone(),
                        name: (*name).to_owned(),
                        missing: want,
                    });
                }
            }
        }
        // inline:引用位置展開(位置即語意,P5)
        let mut items = Vec::new();
        for it in &sign.items {
            match it {
                SignItem::TraitUse { name, block } => {
                    let t = src.traits.iter().find(|t| &t.name == name).expect("checked");
                    for item in &t.blocks[(*block - 1) as usize].items {
                        items.push(match item {
                            Item::Def(d) => SignItem::Def(d.clone()),
                            Item::Rule(r) => SignItem::Rule(r.clone()),
                        });
                    }
                }
                other => items.push(other.clone()),
            }
        }
        let mut s2 = sign.clone();
        s2.items = items;
        out.signs.push(s2);
    }
    Ok(out)
}

/// Pass ②→③:Name + Priority Resolution(I16-c)。
/// 同 path 的 Def 依文件序「後者勝」(位置語意 = P6 欄位級 priority 的實現);
/// Rule 不合併不去重(有序語意)。
pub fn resolve(expanded: &Language) -> Language {
    let mut out = expanded.clone();
    for sign in out.signs.iter_mut() {
        // 由後往前掃:保留每個 path 的最後一次出現(於其原位置)
        let mut seen = std::collections::BTreeSet::new();
        let mut keep: Vec<bool> = vec![true; sign.items.len()];
        for (i, it) in sign.items.iter().enumerate().rev() {
            if let SignItem::Def(d) = it {
                if !seen.insert(d.path.clone()) {
                    keep[i] = false; // 較早的同 path Def 被覆蓋
                }
            }
        }
        let mut i = 0;
        sign.items.retain(|_| {
            let k = keep[i];
            i += 1;
            k
        });
    }
    out
}

/// Pass ③→④:Stage 排序(P18/I16-d)。
/// 各容器內 Rule 依 stem→word→phrase 穩定排序(同 stage 保書寫序,P18:
/// stage 分派優先於書寫序;同 stage 內書寫序有效)。**只在 Rule 槽位間重排,
/// Def 原地不動**;global trait 的 blocks 展平為單 block(macro 結構已無語意)。
pub fn order_stages(resolved: &Language) -> Language {
    use crate::{Block, Stage};
    fn stage_rank(s: Stage) -> u8 {
        match s {
            Stage::Stem => 0,
            Stage::Word => 1,
            Stage::Phrase => 2,
        }
    }
    fn sorted_rules(rules: Vec<Rule>) -> std::vec::IntoIter<Rule> {
        let mut rules = rules;
        rules.sort_by_key(|r| stage_rank(r.stage)); // stable:同 stage 保序
        rules.into_iter()
    }
    let mut out = resolved.clone();
    for t in out.traits.iter_mut() {
        let mut items: Vec<Item> = t.blocks.drain(..).flat_map(|b| b.items).collect();
        let rules = items
            .iter()
            .filter_map(|i| match i {
                Item::Rule(r) => Some(r.clone()),
                Item::Def(_) => None,
            })
            .collect();
        let mut next = sorted_rules(rules);
        for slot in items.iter_mut() {
            if matches!(slot, Item::Rule(_)) {
                *slot = Item::Rule(next.next().expect("rule slot count"));
            }
        }
        t.blocks = vec![Block { items }];
    }
    for s in out.signs.iter_mut() {
        let rules = s
            .items
            .iter()
            .filter_map(|i| match i {
                SignItem::Rule(r) => Some(r.clone()),
                _ => None,
            })
            .collect();
        let mut next = sorted_rules(rules);
        for slot in s.items.iter_mut() {
            if matches!(slot, SignItem::Rule(_)) {
                *slot = SignItem::Rule(next.next().expect("rule slot count"));
            }
        }
    }
    out
}

/// 全管線(① → ④)+ Trait 索引。
pub fn compile(src: &Language) -> Result<Pipeline, CompileError> {
    // Index Generation(Compile Artifact;自 ① 收集,展開後即不可得)
    let mut trait_index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in src.traits.iter().filter(|t| !t.global) {
        trait_index.entry(t.name.clone()).or_default();
    }
    for s in &src.signs {
        for it in &s.items {
            if let SignItem::TraitUse { name, .. } = it {
                let v = trait_index.entry(name.clone()).or_default();
                if !v.contains(&s.name) {
                    v.push(s.name.clone());
                }
            }
        }
    }
    let expanded = expand_traits(src)?;
    let resolved = resolve(&expanded);
    let ordered = order_stages(&resolved);
    Ok(Pipeline {
        expanded,
        resolved,
        ordered,
        trait_index,
    })
}
