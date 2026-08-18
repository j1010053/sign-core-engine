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
    #[error("sign {sign:?}: `belongs {name}` provides {given} type argument(s) but trait {name} expects {expected}")]
    TypeParamArityMismatch {
        sign: String,
        name: String,
        expected: usize,
        given: usize,
    },
    #[error("trait {name:?} is a marker trait and cannot have type parameters")]
    TypeParamOnMarkerTrait { name: String },
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
    externals: &[&Language],
    sign: &str,
    expression: &mut crate::Expression,
    active: &mut Vec<String>,
) -> Result<(), CompileError> {
    match expression {
        crate::Expression::SignFragment(items) => {
            *items = expand_item_sequence(src, externals, sign, items, active)?;
        }
        crate::Expression::DimFragment { items, .. } => {
            for item in items {
                expand_item_expressions(src, externals, sign, item, active)?;
            }
        }
        crate::Expression::Case(case) => {
            for branch in &mut case.branches {
                expand_expression_contexts(src, externals, sign, &mut branch.result, active)?;
            }
        }
        crate::Expression::Projection { value, .. } => {
            expand_expression_contexts(src, externals, sign, value, active)?;
        }
        _ => {}
    }
    Ok(())
}

fn expand_item_expressions(
    src: &Language,
    externals: &[&Language],
    sign: &str,
    item: &mut SignItem,
    active: &mut Vec<String>,
) -> Result<(), CompileError> {
    match item {
        SignItem::SignExpression(expression) => {
            expand_expression_contexts(src, externals, sign, &mut expression.expression, active)
        }
        SignItem::FeatureExpression(expression) => {
            expand_expression_contexts(src, externals, sign, &mut expression.expression, active)
        }
        SignItem::RoleExpression(expression) => {
            expand_expression_contexts(src, externals, sign, &mut expression.expression, active)
        }
        SignItem::Realization(realization) => {
            for branch in &mut realization.expression.branches {
                expand_expression_contexts(src, externals, sign, &mut branch.result, active)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 找一個 trait:**先看本語言,再看外部來源**(std / 套件)。
///
/// 展開原本只查 `src.traits`,而投影用的 registry 是 `[std, user]` 兩份建的
/// ——於是 `belongs 某個 std trait` 配上顯式的 `X[n]` 會報 `UnknownTrait`,
/// **顯式引用只對同一份文件裡宣告的 trait 有效**。那個缺口讓兩階段的主幹道不通。
///
/// 本語言優先:同名 trait 在別處已由 `ONTOLOGY_DUPLICATE_TRAIT` 擋下,這裡的順序
/// 只是讓「文件自己宣告的東西」在任何情況下都不會被套件蓋掉。
fn find_trait<'a>(
    src: &'a Language,
    externals: &[&'a Language],
    name: &str,
) -> Option<&'a crate::TraitDef> {
    src.traits
        .iter()
        .chain(externals.iter().flat_map(|lang| lang.traits.iter()))
        .find(|trait_def| trait_def.name == name)
}

/// P76:在項目向量中,把型別參數名替換為對應的實參。
///
/// 替換觸及 `SlotConstraint::Category`、`RoleDecl` constraint、
/// 以及巢狀 `TraitMount::Declaration` 的 args(傳播)。
fn substitute_type_params(items: &mut [SignItem], subst: &[(&str, &str)]) {
    if subst.is_empty() {
        return;
    }
    for item in items {
        match item {
            SignItem::Slot(slot) => {
                if let crate::SlotConstraint::Category(ref mut cat) = slot.constraint {
                    for &(param, arg) in subst {
                        if cat.as_str() == param {
                            *cat = arg.to_owned();
                            break;
                        }
                    }
                }
            }
            SignItem::RoleDecl(role) => {
                if let crate::SlotConstraint::Category(ref mut cat) = role.constraint {
                    for &(param, arg) in subst {
                        if cat.as_str() == param {
                            *cat = arg.to_owned();
                            break;
                        }
                    }
                }
            }
            SignItem::TraitMount { args, .. } => {
                for a in args.iter_mut() {
                    for &(param, arg) in subst {
                        if a.as_str() == param {
                            *a = arg.to_owned();
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// P76:從同容器的 Declaration 中找出某個 trait 的型別實參。
fn find_declaration_args<'a>(items: &'a [SignItem], trait_name: &str) -> Option<&'a [String]> {
    for item in items {
        if let SignItem::TraitMount { name, kind, args, .. } = item {
            if kind.is_declaration() && name == trait_name {
                return Some(args);
            }
        }
    }
    None
}

fn expand_item_sequence(
    src: &Language,
    externals: &[&Language],
    sign: &str,
    source: &[SignItem],
    active: &mut Vec<String>,
) -> Result<Vec<SignItem>, CompileError> {
    expand_item_sequence_with(src, externals, sign, source, active, false)
}

/// `expand_declarations`:`belongs X` 要不要也展開。
///
/// 正式的編譯路徑傳 `false`——宣告是分類邊,展開由 `X[n]` 負責(兩階段)。
/// [`trait_view`] 傳 `true`,因為視圖問的是「這個 trait **有效**有什麼」,
/// 而那包含它從父輩繼承來的;不展開宣告就只看得到它自己寫的那一層。
fn expand_item_sequence_with(
    src: &Language,
    externals: &[&Language],
    sign: &str,
    source: &[SignItem],
    active: &mut Vec<String>,
    expand_declarations: bool,
) -> Result<Vec<SignItem>, CompileError> {
    let mut used: BTreeMap<&str, (bool, Vec<u32>)> = BTreeMap::new();
    for item in source {
        if let SignItem::TraitMount { name, kind, .. } = item {
            // **宣告不進這張表。** 它不展開,故不參與完整性計算;更重要的是
            // 這張表的每個鍵稍後都會在 `src.traits` 裡查一次,而 `belongs` 指得到
            // **std 的 trait**(不在使用者語言裡)——把宣告放進來會讓每一個
            // 指向 std 的 `belongs` 都報 `UnknownTrait`。
            if kind.is_declaration() {
                continue;
            }
            let entry = used.entry(name).or_default();
            match kind {
                crate::TraitMountKind::Declaration => {}
                crate::TraitMountKind::Whole => entry.0 = true,
                crate::TraitMountKind::Block(index) => entry.1.push(*index),
            }
        }
    }
    for (name, (whole, indices)) in &used {
        let trait_def =
            find_trait(src, externals, name).ok_or_else(|| CompileError::UnknownTrait {
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
            // **宣告原樣留下。** `belongs X` 是分類邊,不是展開對象——它必須
            // 活到 ②③④ 與 ontology 建樹那一刻。把它當成「展開出空集合」會讓
            // 分類邊在展開後消失,整棵 ontology 樹跟著垮。
            SignItem::TraitMount { name, kind, args, .. } if kind.is_declaration() => {
                output.push(item.clone());
                if !expand_declarations {
                    continue;
                }
                let Some(trait_def) = find_trait(src, externals, name) else {
                    continue;
                };
                if active.iter().any(|candidate| candidate == name) {
                    continue;
                }
                let mut selected: Vec<SignItem> = trait_def
                    .blocks
                    .iter()
                    .flat_map(|block| block.items.iter().cloned())
                    .collect();
                if !trait_def.type_params.is_empty()
                    && args.len() == trait_def.type_params.len()
                {
                    let subst: Vec<(&str, &str)> = trait_def
                        .type_params
                        .iter()
                        .zip(args.iter())
                        .map(|(p, a)| (p.name.as_str(), a.as_str()))
                        .collect();
                    substitute_type_params(&mut selected, &subst);
                }
                active.push(name.clone());
                output.extend(expand_item_sequence_with(
                    src,
                    externals,
                    sign,
                    &selected,
                    active,
                    expand_declarations,
                )?);
                active.pop();
            }
            SignItem::TraitMount { name, kind, args: direct_args, .. } => {
                if let Some(start) = active.iter().position(|candidate| candidate == name) {
                    let mut path = active[start..].to_vec();
                    path.push(name.clone());
                    return Err(CompileError::TraitExpansionCycle {
                        sign: sign.to_owned(),
                        path: path.join(" -> "),
                    });
                }
                let trait_def =
                    find_trait(src, externals, name).ok_or_else(|| CompileError::UnknownTrait {
                        sign: sign.to_owned(),
                        name: name.clone(),
                    })?;
                // P76:取得型別實參——Declaration 的 args 或同容器 Declaration 的 args
                let args: &[String] = if !direct_args.is_empty() {
                    direct_args
                } else {
                    find_declaration_args(source, name).unwrap_or(&[])
                };
                // 驗證 arity
                if !trait_def.type_params.is_empty() || !args.is_empty() {
                    if args.len() != trait_def.type_params.len() {
                        return Err(CompileError::TypeParamArityMismatch {
                            sign: sign.to_owned(),
                            name: name.clone(),
                            expected: trait_def.type_params.len(),
                            given: args.len(),
                        });
                    }
                }
                let mut selected = match kind {
                    crate::TraitMountKind::Declaration => Vec::new(),
                    crate::TraitMountKind::Whole => trait_def
                        .blocks
                        .iter()
                        .flat_map(|block| block.items.iter().cloned())
                        .collect::<Vec<_>>(),
                    crate::TraitMountKind::Block(index) => {
                        trait_def.blocks[*index as usize].items.clone()
                    }
                };
                // P76:參數替換
                if !trait_def.type_params.is_empty() && !args.is_empty() {
                    let subst: Vec<(&str, &str)> = trait_def
                        .type_params
                        .iter()
                        .zip(args.iter())
                        .map(|(p, a)| (p.name.as_str(), a.as_str()))
                        .collect();
                    substitute_type_params(&mut selected, &subst);
                }
                active.push(name.clone());
                output.extend(expand_item_sequence_with(
                    src,
                    externals,
                    sign,
                    &selected,
                    active,
                    expand_declarations,
                )?);
                active.pop();
            }
            other => {
                let mut expanded = other.clone();
                expand_item_expressions(src, externals, sign, &mut expanded, active)?;
                output.push(expanded);
            }
        }
    }
    Ok(output)
}

/// 一個 **trait 的有效內容視圖**:把它當成「一個只掛載它的 sign」展開。
///
/// # 為什麼需要這個
///
/// trait 不是 sign,但 trait 上可以寫規則,而驗證那些規則得知道它引用的 slot /
/// feature 存不存在。舊做法是造一個只寫 `belongs X` 的合成 sign,再靠**投影**
/// 把 X 的內容攤出來——那用的是投影的「內容」那一半,而兩階段要把那一半關掉。
///
/// # 為什麼走展開而不是直接讀 X 的 blocks
///
/// 直接讀只看得到 **X 自己寫的**,看不到它從父輩繼承來的。若某條規則引用的 slot
/// 宣告在祖先上,直接讀會誤報「找不到」。走展開則遞迴把祖先的內容一併拉進來,
/// 而且**與真實編譯走同一條路**——驗證看到的與編譯產出的不會分岔。
pub fn trait_view(
    src: &Language,
    externals: &[&Language],
    trait_name: &str,
) -> Result<Vec<SignItem>, CompileError> {
    let args = find_trait(src, externals, trait_name)
        .map(|t| t.type_params.iter().map(|p| p.name.clone()).collect())
        .unwrap_or_default();
    let mount = SignItem::TraitMount {
        name: trait_name.to_owned(),
        kind: crate::TraitMountKind::Whole,
        args,
    };
    expand_item_sequence_with(src, externals, trait_name, &[mount], &mut Vec::new(), true)
}

/// Pass ①→②:Trait Expansion(I16-a/b)。
/// Trait uses in a typed SignContext fragment follow the exact same expansion
/// path as top-level Sign items.  The fragment remains anonymous: expansion
/// contributes Sign items but never creates another Sign node.
pub fn expand_traits(src: &Language) -> Result<Language, CompileError> {
    expand_traits_with(src, &[])
}

/// 同上,但可額外查**外部語言**的 trait(std / 套件)——見 [`find_trait`]。
pub fn expand_traits_with(
    src: &Language,
    externals: &[&Language],
) -> Result<Language, CompileError> {
    check_names(src)?;
    let mut out = src.clone();
    out.signs.clear();
    for sign in &src.signs {
        let mut expanded = sign.clone();
        expanded.items =
            expand_item_sequence(src, externals, &sign.name, &sign.items, &mut Vec::new())?;
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
    compile_with(src, &[])
}

/// 同上,但展開時可額外查**外部語言**的 trait(std / 套件)——見 [`find_trait`]。
pub fn compile_with(
    src: &Language,
    externals: &[&Language],
) -> Result<Pipeline, CompileError> {
    // Index Generation(Compile Artifact;自 ① 收集,展開後即不可得)
    let mut trait_index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in src.traits.iter().filter(|t| !t.global) {
        trait_index.entry(t.name.clone()).or_default();
    }
    for s in &src.signs {
        for it in &s.items {
            if let SignItem::TraitMount { name, kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_), .. } = it {
                let v = trait_index.entry(name.clone()).or_default();
                if !v.contains(&s.name) {
                    v.push(s.name.clone());
                }
            }
        }
    }
    let expanded = expand_traits_with(src, externals)?;
    let resolved = resolve(&expanded);
    let ordered = order_stages(&resolved);
    Ok(Pipeline {
        expanded,
        resolved,
        ordered,
        trait_index,
    })
}
