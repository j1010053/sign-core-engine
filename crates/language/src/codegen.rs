//! ⑤ Codegen(步驟 11):④Ordered Language → **Compiled Grammar + Compiled Sign**。
//!
//! 接口(P20 §1.3):**Compiled Grammar 的 phon 側 = DSL 引擎可直接吃的規則集**——
//! 本模組把 dsl 域宣告(verbatim)與 global trait 內的規則重新排版為 dsl 原始碼,
//! 交 `tshiatun_dsl::compile` 得可執行 [`Program`]。兩條路徑(A=純 .qy、
//! B=Language→⑤)最終進同一個 DSL 引擎;雙軌迴歸(P20 §1.4)= 步驟 11 出口。
//!
//! 責任分離(P8):Compiled Grammar 僅含執行表示(規則集原文 + Program),
//! **不保存 trait/priority/compile metadata**(trait 索引住 [`Pipeline`],
//! 是 Compile Artifact)。Compiled Sign = 解析後欄位(③ 後者勝已套用)+ sign 局部
//! 規則；規則保留 id、維度、stage、Then/Else 與來源位置供 M1++ runtime/trace 使用。
//!
//! 編碼細節(I17):Language Rule 的 body 以 `;` 連接同塊多語句(dsl 詞法無
//! `;`,分隔符不可能滲入語句);codegen 展開為「合成標籤 `rN:` + 每語句一行」,
//! 保 B5(同規則語句共享凍結快照)語意。`Scan ` 開頭的 body 以首個 `:` 切
//! 塊頭(Scan 頭文法無冒號),其後語句同上展開。

use crate::compile::{self, CompileError, Pipeline};
use crate::{Dim, Language, Rule, RuleId, SignItem, SourceLocation, Stage};
use tshiatun_dsl::Program;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodegenError {
    #[error(transparent)]
    Compile(#[from] CompileError),
    /// dsl 的 Scan 塊不承載 stage 語句(lower 固定 default)——顯式拒絕。
    #[error("Scan rule {body:?} has non-word stage {stage}: dsl Scan blocks carry no stage")]
    ScanStageUnsupported { body: String, stage: &'static str },
    /// 產出的規則集被 dsl 拒收(語句原文有誤);附完整產物供定位。
    #[error(
        "generated phon rule-set rejected by dsl: {msg}\n--- generated source ---\n{generated}"
    )]
    Dsl { msg: String, generated: String },
    /// P46 S4:`.qy` 的 propagate 只能掛在**邊界**(`Then propagate:`)或 **header**
    /// (`name propagate:`);block 的**首元素**在邊界之前,無處可掛修飾詞。
    /// 這種 IR(僅 S3 編輯可造成)**顯式拒絕**,不默默丟掉 propagate 語意。
    #[error(
        "phon rule {rule:?}: a leading block element cannot carry `propagate` \
         (no boundary to attach it to); use a rule-level `propagate:` header instead"
    )]
    LeadingPropagateUnsupported { rule: String },
}

/// Compiled Grammar(P8):共時執行表示,Engine 只讀這個。
#[derive(Debug, Clone)]
pub struct CompiledGrammar {
    /// phon 側規則集原文(dsl 可直接吃;P20 §1.3 接口,亦是人類可讀 dump)。
    pub phon_source: String,
    /// `tshiatun_dsl::compile(phon_source)` 產物(規則已解析、帶 stage 標記)。
    pub program: Program,
    /// Generated `.qy` line to physical `.lang` rule/branch line.
    pub source_map: Vec<PhonSourceMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhonSourceMap {
    pub generated_line: usize,
    pub rule_id: RuleId,
    /// `0` is the main branch, `1..` are Then/Else branches.
    pub branch: usize,
    pub source: SourceLocation,
}

/// Compiled Sign 內的一條 sign 局部規則(phon 或 syn/sem;消費者 = 步驟 12)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSignRule {
    pub rule_id: RuleId,
    pub body: String,
    pub stage: Stage,
    pub dim: Dim,
    pub else_chain: Vec<String>,
    pub then_chain: Vec<String>,
    pub source: SourceLocation,
    pub branch_sources: Vec<SourceLocation>,
}

/// Compiled Sign:③ 解析後的欄位(同 path 已後者勝)+ 局部規則,無 trait 痕跡。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSign {
    pub name: String,
    /// `(path, value)`,保 ④ 槽位序(Def 原地不動)。
    pub defs: Vec<(String, String)>,
    pub feature_declarations: Vec<crate::FeatureDecl>,
    pub feature_values: Vec<crate::FeatureValue>,
    pub slot_feature_bindings: Vec<crate::SlotFeatureBinding>,
    pub role_declarations: Vec<crate::RoleDecl>,
    pub role_bindings: Vec<crate::RoleBinding>,
    pub realization: Option<crate::Realization>,
    pub rules: Vec<CompiledSignRule>,
}

/// ①–⑤ 全管線產物。
#[derive(Debug, Clone)]
pub struct Artifacts {
    pub pipeline: Pipeline,
    pub grammar: CompiledGrammar,
    pub signs: Vec<CompiledSign>,
}

fn stage_str(s: Stage) -> &'static str {
    match s {
        Stage::Stem => "stem",
        Stage::Word => "word",
        Stage::Phrase => "phrase",
    }
}

/// 一條 ④ 規則 → dsl 規則塊文字。`n` 為合成標籤計數器(決定性,P26 精神)。
/// (`pub(crate)`:word 模組的 cophonology 小規則組沿用同一排放,I18。)
pub(crate) fn emit_rule(out: &mut String, n: &mut u32, r: &Rule) -> Result<(), CodegenError> {
    emit_rule_mapped(out, n, r, &mut Vec::new())
}

/// 排放結構化 `PhonBlock`(P46 S2)→ `.qy`:leading block 直接印;後續 block 冠
/// `Then:`/`Else:` 邊界並縮排。
/// A `Then`/`Else` block whose element must become a **braced group** in `.qy`
/// rather than a bare statement list. A `Leaf` element is a plain expression
/// list (needs no braces); a nested `Then`/`Else` must be grouped so its inner
/// boundaries don't read as flat-mixing at the parent level (P46 L1; engine
/// grouped-block parser, tshiatūn wuc-claudecode / PR #1).
fn is_grouped_element(block: &crate::PhonBlock) -> bool {
    match block {
        crate::PhonBlock::Leaf(_) => false,
        crate::PhonBlock::Then(_) | crate::PhonBlock::Else(_) => true,
        crate::PhonBlock::Propagate(inner) => is_grouped_element(inner),
    }
}

/// Split a block element into its `propagate` modifier and the block the
/// modifier applies to (P46 S4). `Propagate` is a *modifier*, not a level.
fn split_propagate(block: &crate::PhonBlock) -> (bool, &crate::PhonBlock) {
    match block {
        crate::PhonBlock::Propagate(inner) => (true, inner.as_ref()),
        other => (false, other),
    }
}

fn emit_phon_block(
    out: &mut String,
    block: &crate::PhonBlock,
    indent: &str,
    rule_label: &str,
) -> Result<(), CodegenError> {
    match block {
        crate::PhonBlock::Leaf(stmts) => {
            for statement in stmts {
                out.push_str(indent);
                out.push_str(statement);
                out.push('\n');
            }
        }
        crate::PhonBlock::Then(blocks) | crate::PhonBlock::Else(blocks) => {
            let keyword = if matches!(block, crate::PhonBlock::Then(_)) {
                "Then"
            } else {
                "Else"
            };
            let inner = format!("{indent}    ");
            for (index, sub) in blocks.iter().enumerate() {
                let (propagate, body) = split_propagate(sub);
                if index == 0 {
                    // The leading element precedes every boundary, so there is no
                    // place to hang a modifier: reject rather than drop it.
                    if propagate {
                        return Err(CodegenError::LeadingPropagateUnsupported {
                            rule: rule_label.to_owned(),
                        });
                    }
                    if is_grouped_element(body) {
                        // Leading nested block: a `{ … }` group on its own lines.
                        out.push_str(&format!("{indent}{{\n"));
                        emit_phon_block(out, body, &inner, rule_label)?;
                        out.push_str(&format!("{indent}}}\n"));
                    } else {
                        // Leading bare leaf: statements straight at this indent.
                        emit_phon_block(out, body, indent, rule_label)?;
                    }
                    continue;
                }
                let modifier = if propagate { " propagate" } else { "" };
                if is_grouped_element(body) {
                    // Boundary then a nested block: `Keyword[ propagate]: { … }`.
                    out.push_str(&format!("{indent}{keyword}{modifier}: {{\n"));
                    emit_phon_block(out, body, &inner, rule_label)?;
                    out.push_str(&format!("{indent}}}\n"));
                } else {
                    // Boundary then a bare leaf (flat single-level form — without
                    // `propagate` this is byte-identical to the pre-S4 output).
                    out.push_str(&format!("{indent}{keyword}{modifier}:\n"));
                    emit_phon_block(out, body, &inner, rule_label)?;
                }
            }
        }
        // A `Propagate` reached directly (a rule root) carries no boundary of its
        // own; the rule-level `propagate:` header expresses it instead.
        crate::PhonBlock::Propagate(inner) => emit_phon_block(out, inner, indent, rule_label)?,
    }
    Ok(())
}

fn generated_line(out: &str) -> usize {
    out.bytes().filter(|byte| *byte == b'\n').count() + 1
}

fn push_mapped_statement(
    out: &mut String,
    indent: &str,
    statement: &str,
    rule: &Rule,
    branch: usize,
    source: SourceLocation,
    source_map: &mut Vec<PhonSourceMap>,
) {
    source_map.push(PhonSourceMap {
        generated_line: generated_line(out),
        rule_id: rule.id.clone(),
        branch,
        source,
    });
    out.push_str(indent);
    out.push_str(statement);
    out.push('\n');
}

fn emit_rule_mapped(
    out: &mut String,
    n: &mut u32,
    r: &Rule,
    source_map: &mut Vec<PhonSourceMap>,
) -> Result<(), CodegenError> {
    let stmts = |s: &str| {
        s.split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    if let Some(block) = &r.phon_block {
        // P46 S2: structured Lexurgy block → `.qy` verbatim.
        let label = r.name.clone().unwrap_or_else(|| format!("r{n}"));
        *n += 1;
        // P46 S4: rule-level propagate → engine header modifier `name propagate:`.
        let modifier = if r.propagate { " propagate" } else { "" };
        out.push_str(&format!("{label}{modifier}:\n"));
        if r.stage != Stage::Word {
            out.push_str(&format!("    stage: {}\n", stage_str(r.stage)));
        }
        emit_phon_block(out, block, "    ", &label)?;
        return Ok(());
    }
    if r.body.starts_with("Scan ") {
        if r.stage != Stage::Word {
            return Err(CodegenError::ScanStageUnsupported {
                body: r.body.clone(),
                stage: stage_str(r.stage),
            });
        }
        // 塊頭止於首個 `:`;無 `:` 者原樣送出,由 dsl 回報(附產物原文)
        match r.body.split_once(':') {
            Some((head, tail)) => {
                out.push_str(head.trim_end());
                out.push_str(":\n");
                for s in stmts(tail) {
                    push_mapped_statement(out, "    ", &s, r, 0, r.source, source_map);
                }
            }
            None => {
                out.push_str(&r.body);
                out.push('\n');
            }
        }
    } else {
        // A named phon rule (P46 取徑 A) emits its Lexurgy `name:` label; an
        // unnamed rule keeps the synthetic `rN:` label.
        out.push_str(&format!(
            "{}:\n",
            r.name.clone().unwrap_or_else(|| format!("r{n}"))
        ));
        *n += 1;
        if r.stage != Stage::Word {
            out.push_str(&format!("    stage: {}\n", stage_str(r.stage)));
        }
        for s in stmts(&r.body) {
            push_mapped_statement(out, "    ", &s, r, 0, r.source, source_map);
        }
    }
    let (keyword, branches) = if !r.else_chain.is_empty() {
        ("Else", &r.else_chain)
    } else {
        ("Then", &r.then_chain)
    };
    for (index, branch) in branches.iter().enumerate() {
        out.push_str("    ");
        out.push_str(keyword);
        out.push_str(":\n");
        let source = r.branch_sources.get(index).copied().unwrap_or(r.source);
        for statement in stmts(branch) {
            push_mapped_statement(
                out,
                "        ",
                &statement,
                r,
                index + 1,
                source,
                source_map,
            );
        }
    }
    Ok(())
}

/// ④Ordered → Compiled Grammar + Compiled Sign。
///
/// phon 側收錄:dsl 域宣告(verbatim,含 Spell-out/Parse 塊——dsl 要求宣告
/// 先於規則,verbatim 區恰在最前)+ **global trait 的規則**(canonical 名稱序,
/// 塊內書寫序 = ④ 的 stage dispatch 結果)。global trait 的 Def 與 sign 局部
/// 規則**不進** phon 側(前者非 phon 概念、後者屬臨時 Word 建構,皆步驟 12+)。
pub fn codegen(ordered: &Language) -> Result<(CompiledGrammar, Vec<CompiledSign>), CodegenError> {
    let mut src = String::new();
    for line in &ordered.dsl_decls {
        src.push_str(line);
        src.push('\n');
    }
    if !src.is_empty() {
        src.push('\n');
    }
    let mut globals: Vec<_> = ordered.traits.iter().filter(|t| t.global).collect();
    globals.sort_by(|a, b| a.name.cmp(&b.name)); // canonical 序(printer 同序,決定性)
    let mut n = 1u32;
    let mut source_map = Vec::new();
    for t in globals {
        for b in &t.blocks {
            for item in &b.items {
                // phon 側只收 phon 規則(P44 維度隔離;syn/sem/prag 規則求值於 Sign,12d)
                if let SignItem::Rule(r) = item {
                    if r.dim == crate::Dim::Phon {
                        emit_rule_mapped(&mut src, &mut n, r, &mut source_map)?;
                    }
                }
            }
        }
    }
    let program = tshiatun_dsl::compile(&src).map_err(|e| CodegenError::Dsl {
        msg: e.to_string(),
        generated: src.clone(),
    })?;

    let signs = ordered
        .signs
        .iter()
        .map(|s| CompiledSign {
            name: s.name.clone(),
            defs: s
                .items
                .iter()
                .filter_map(|i| match i {
                    SignItem::Def(d) => Some((d.path.clone(), d.value.clone())),
                    _ => None,
                })
                .collect(),
            feature_declarations: s
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::FeatureDecl(feature) => Some(feature.clone()),
                    _ => None,
                })
                .collect(),
            feature_values: s
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::FeatureValue(feature) => Some(feature.clone()),
                    _ => None,
                })
                .collect(),
            slot_feature_bindings: s
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::SlotFeatureBinding(binding) => Some(binding.clone()),
                    _ => None,
                })
                .collect(),
            role_declarations: s
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::RoleDecl(role) => Some(role.clone()),
                    _ => None,
                })
                .collect(),
            role_bindings: s
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::RoleBinding(role) => Some(role.clone()),
                    _ => None,
                })
                .collect(),
            realization: s.items.iter().find_map(|item| match item {
                SignItem::Realization(realization) => Some(realization.clone()),
                _ => None,
            }),
            rules: s
                .items
                .iter()
                .filter_map(|i| match i {
                    SignItem::Rule(r) | SignItem::FeatureRule(r) => Some(CompiledSignRule {
                        rule_id: r.id.clone(),
                        body: r.body.clone(),
                        stage: r.stage,
                        dim: r.dim,
                        else_chain: r.else_chain.clone(),
                        then_chain: r.then_chain.clone(),
                        source: r.source,
                        branch_sources: r.branch_sources.clone(),
                    }),
                    _ => None,
                })
                .collect(),
        })
        .collect();

    Ok((
        CompiledGrammar {
            phon_source: src,
            program,
            source_map,
        },
        signs,
    ))
}

/// ①–⑤ 一步到位(compile 管線 + codegen)。
pub fn compile_full(source: &Language) -> Result<Artifacts, CodegenError> {
    let pipeline = compile::compile(source)?;
    let (grammar, signs) = codegen(&pipeline.ordered)?;
    Ok(Artifacts {
        pipeline,
        grammar,
        signs,
    })
}
