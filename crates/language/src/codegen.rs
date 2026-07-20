//! ⑤ Codegen(步驟 11):④Ordered Language → **Compiled Grammar + Compiled Sign**。
//!
//! 接口(P20 §1.3):**Compiled Grammar 的 phon 側 = DSL 引擎可直接吃的規則集**——
//! 本模組把 dsl 域宣告(verbatim)與 global trait 內的規則重新排版為 dsl 原始碼,
//! 交 `tshiatun_dsl::compile` 得可執行 [`Program`]。兩條路徑(A=純 .qy、
//! B=Language→⑤)最終進同一個 DSL 引擎;雙軌迴歸(P20 §1.4)= 步驟 11 出口。
//!
//! 責任分離(P8):Compiled Grammar 僅含執行表示(規則集原文 + Program),
//! **不保存 trait/priority/compile metadata**(trait 索引住 [`Pipeline`],
//! 是 Compile Artifact)。Compiled Sign = 解析後欄位(③ 後者勝已套用)+
//! sign 局部規則(消費者 = 步驟 12 的臨時 Word 建構/循環套用)。
//!
//! 編碼細節(I17):Language Rule 的 body 以 `;` 連接同塊多語句(dsl 詞法無
//! `;`,分隔符不可能滲入語句);codegen 展開為「合成標籤 `rN:` + 每語句一行」,
//! 保 B5(同規則語句共享凍結快照)語意。`Scan ` 開頭的 body 以首個 `:` 切
//! 塊頭(Scan 頭文法無冒號),其後語句同上展開。

use crate::compile::{self, CompileError, Pipeline};
use crate::{Item, Language, Rule, SignItem, Stage};
use tshiatun_dsl::Program;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodegenError {
    #[error(transparent)]
    Compile(#[from] CompileError),
    /// P22 else 鏈的求值屬 language 擴充文法,dsl 尚無對應——顯式拒絕,不默默丟棄。
    #[error("rule {label} {body:?}: else-chain not lowerable to phon DSL (P22 evaluation = 步驟 12+)")]
    ElseUnsupported { label: String, body: String },
    /// dsl 的 Scan 塊不承載 stage 語句(lower 固定 default)——顯式拒絕。
    #[error("Scan rule {body:?} has non-word stage {stage}: dsl Scan blocks carry no stage")]
    ScanStageUnsupported { body: String, stage: &'static str },
    /// 產出的規則集被 dsl 拒收(語句原文有誤);附完整產物供定位。
    #[error("generated phon rule-set rejected by dsl: {msg}\n--- generated source ---\n{generated}")]
    Dsl { msg: String, generated: String },
}

/// Compiled Grammar(P8):共時執行表示,Engine 只讀這個。
#[derive(Debug, Clone)]
pub struct CompiledGrammar {
    /// phon 側規則集原文(dsl 可直接吃;P20 §1.3 接口,亦是人類可讀 dump)。
    pub phon_source: String,
    /// `tshiatun_dsl::compile(phon_source)` 產物(規則已解析、帶 stage 標記)。
    pub program: Program,
}

/// Compiled Sign 內的一條 sign 局部規則(phon 或 syn/sem;消費者 = 步驟 12)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSignRule {
    pub body: String,
    pub stage: Stage,
    pub else_chain: Vec<String>,
}

/// Compiled Sign:③ 解析後的欄位(同 path 已後者勝)+ 局部規則,無 trait 痕跡。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSign {
    pub name: String,
    /// `(path, value)`,保 ④ 槽位序(Def 原地不動)。
    pub defs: Vec<(String, String)>,
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
    if !r.else_chain.is_empty() {
        return Err(CodegenError::ElseUnsupported {
            label: format!("r{n}"),
            body: r.body.clone(),
        });
    }
    let stmts = |s: &str| {
        s.split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
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
                    out.push_str("    ");
                    out.push_str(&s);
                    out.push('\n');
                }
            }
            None => {
                out.push_str(&r.body);
                out.push('\n');
            }
        }
    } else {
        out.push_str(&format!("r{n}:\n"));
        *n += 1;
        if r.stage != Stage::Word {
            out.push_str(&format!("    stage: {}\n", stage_str(r.stage)));
        }
        for s in stmts(&r.body) {
            out.push_str("    ");
            out.push_str(&s);
            out.push('\n');
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
    for t in globals {
        for b in &t.blocks {
            for item in &b.items {
                if let Item::Rule(r) = item {
                    emit_rule(&mut src, &mut n, r)?;
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
            rules: s
                .items
                .iter()
                .filter_map(|i| match i {
                    SignItem::Rule(r) => Some(CompiledSignRule {
                        body: r.body.clone(),
                        stage: r.stage,
                        else_chain: r.else_chain.clone(),
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
