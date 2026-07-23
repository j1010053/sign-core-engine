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

use crate::{Language, Rule, SignItem};

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
    #[error("trait expansion cycle in {sign:?}: {path}")]
    TraitExpansionCycle { sign: String, path: String },
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

fn expand_expression_contexts(
    src: &Language,
    sign: &str,
    expression: &mut crate::Expression,
    active: &mut Vec<String>,
) -> Result<(), CompileError> {
    match expression {
        crate::Expression::SignFragment(items) => {
            *items = expand_item_sequence(src, sign, items, active)?;
        }
        crate::Expression::DimFragment { items, .. } => {
            for item in items {
                expand_item_expressions(src, sign, item, active)?;
            }
        }
        crate::Expression::Case(case) => {
            for branch in &mut case.branches {
                expand_expression_contexts(src, sign, &mut branch.result, active)?;
            }
        }
        crate::Expression::Projection { value, .. } => {
            expand_expression_contexts(src, sign, value, active)?;
        }
        _ => {}
    }
    Ok(())
}

fn expand_item_expressions(
    src: &Language,
    sign: &str,
    item: &mut SignItem,
    active: &mut Vec<String>,
) -> Result<(), CompileError> {
    match item {
        SignItem::SignExpression(expression) => {
            expand_expression_contexts(src, sign, &mut expression.expression, active)
        }
        SignItem::FeatureExpression(expression) => {
            expand_expression_contexts(src, sign, &mut expression.expression, active)
        }
        SignItem::RoleExpression(expression) => {
            expand_expression_contexts(src, sign, &mut expression.expression, active)
        }
        SignItem::Realization(realization) => {
            if let Some(case) = &mut realization.expression {
                for branch in &mut case.branches {
                    expand_expression_contexts(src, sign, &mut branch.result, active)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn expand_item_sequence(
    src: &Language,
    sign: &str,
    source: &[SignItem],
    active: &mut Vec<String>,
) -> Result<Vec<SignItem>, CompileError> {
    let mut used: BTreeMap<&str, (bool, Vec<u32>)> = BTreeMap::new();
    for item in source {
        if let SignItem::TraitUse { name, block } = item {
            let entry = used.entry(name).or_default();
            match block {
                None => entry.0 = true,
                Some(index) => entry.1.push(*index),
            }
        }
    }
    for (name, (whole, indices)) in &used {
        let trait_def = src
            .traits
            .iter()
            .find(|trait_def| &trait_def.name == name)
            .ok_or_else(|| CompileError::UnknownTrait {
                sign: sign.to_owned(),
                name: (*name).to_owned(),
            })?;
        for index in indices {
            if *index as usize >= trait_def.blocks.len() {
                return Err(CompileError::BlockOutOfRange {
                    sign: sign.to_owned(),
                    name: (*name).to_owned(),
                    block: *index,
                    blocks: trait_def.blocks.len(),
                });
            }
        }
        if !whole {
            for wanted in 0..trait_def.blocks.len() as u32 {
                if !indices.contains(&wanted) {
                    return Err(CompileError::IncompleteTraitUse {
                        sign: sign.to_owned(),
                        name: (*name).to_owned(),
                        missing: wanted,
                    });
                }
            }
        }
    }

    let mut output = Vec::new();
    for item in source {
        match item {
            SignItem::TraitUse { name, block } => {
                if let Some(start) = active.iter().position(|candidate| candidate == name) {
                    let mut path = active[start..].to_vec();
                    path.push(name.clone());
                    return Err(CompileError::TraitExpansionCycle {
                        sign: sign.to_owned(),
                        path: path.join(" -> "),
                    });
                }
                let trait_def = src
                    .traits
                    .iter()
                    .find(|trait_def| trait_def.name == *name)
                    .ok_or_else(|| CompileError::UnknownTrait {
                        sign: sign.to_owned(),
                        name: name.clone(),
                    })?;
                let selected = match block {
                    None => trait_def
                        .blocks
                        .iter()
                        .flat_map(|block| block.items.iter().cloned())
                        .collect::<Vec<_>>(),
                    Some(index) => trait_def.blocks[*index as usize].items.clone(),
                };
                active.push(name.clone());
                output.extend(expand_item_sequence(src, sign, &selected, active)?);
                active.pop();
            }
            other => {
                let mut expanded = other.clone();
                expand_item_expressions(src, sign, &mut expanded, active)?;
                output.push(expanded);
            }
        }
    }
    Ok(output)
}

/// Pass ①→②:Trait Expansion(I16-a/b)。
/// Trait uses in a typed SignContext fragment follow the exact same expansion
/// path as top-level Sign items.  The fragment remains anonymous: expansion
/// contributes Sign items but never creates another Sign node.
pub fn expand_traits(src: &Language) -> Result<Language, CompileError> {
    check_names(src)?;
    let mut out = src.clone();
    out.signs.clear();
    for sign in &src.signs {
        let mut expanded = sign.clone();
        expanded.items = expand_item_sequence(src, &sign.name, &sign.items, &mut Vec::new())?;
        out.signs.push(expanded);
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
    fn reorder_rule_items(items: &mut [SignItem]) {
        // `feature:` rules are ordinary dimension rules at runtime.  Sort one
        // combined stream, preserving source order for equal stages, then
        // restore the AST variant from its stable RuleId.
        let feature_ids = items
            .iter()
            .filter_map(|item| match item {
                SignItem::FeatureRule(rule) => Some(rule.id.clone()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        let rules = items
            .iter()
            .filter_map(|item| match item {
                SignItem::Rule(rule) | SignItem::FeatureRule(rule) => Some(rule.clone()),
                _ => None,
            })
            .collect();
        let mut next = sorted_rules(rules);
        for slot in items {
            if matches!(slot, SignItem::Rule(_) | SignItem::FeatureRule(_)) {
                let rule = next.next().expect("rule slot count");
                *slot = if feature_ids.contains(&rule.id) {
                    SignItem::FeatureRule(rule)
                } else {
                    SignItem::Rule(rule)
                };
            }
        }
    }
    let mut out = resolved.clone();
    for t in out.traits.iter_mut() {
        let mut items: Vec<SignItem> = t.blocks.drain(..).flat_map(|b| b.items).collect();
        reorder_rule_items(&mut items);
        t.blocks = vec![Block { items }];
    }
    for s in out.signs.iter_mut() {
        reorder_rule_items(&mut s.items);
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
