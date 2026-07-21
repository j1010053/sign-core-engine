//! 臨時 Word 建構 + 循環套用(步驟 12;P1/P3/P4,裁決 docs/13 §4-2)。
//!
//! **Word 是臨時韻律域,不是儲存單位**(P1):由 sign 組合樹按需建構,
//! 表層按需導出、永不儲存。本模組是引擎協作規範 §3-1 所稱的「呼叫端」——
//! stage 先後與重跑由此處決定,引擎一次只跑一趟:
//!
//! ```text
//! Composition(組合樹) ─UR+cophonology 前趟─▶ pword 文字("root+affix …")
//!   → dsl::build_phrase(韻律域;`+` 詞幹縫、空白詞縫、括號由引擎維護)
//!   → P3 驅動:stem 切片 → word 切片 → phrase 切片(各自 run_program 一趟;
//!     引擎按規則的 stage 界定可見域)
//!   → dsl::surface_phrase(spell-out 純函數 C11)
//! ```
//!
//! **cophonology(P3「含 cophonology」/P4)**:sign 局部 stem 規則 =
//! 構式專屬小規則組,於該 sign 自己的葉上先跑(範圍天然限定於觸發構式,
//! 修補01 §1)。M1 子集 = **音段效果**(改寫/刪除):跑完後旋律 tier 須全空、
//! 骨架重新渲染為 UR 文字回進組合;旋律效果(浮游調等)需引擎的
//! Word 級拼接,**顯式拒絕**留步驟 18(I18)。組合樹巢狀環先展平
//! (深度序留步驟 18 的真組合造詞)。

use tshiatun_core::repr::word::Word;
use tshiatun_dsl::{build_phrase, run_program, surface_phrase, Program, StepRecord};

use crate::codegen::{self, Artifacts, CompiledSign, CodegenError};
use crate::{Rule, RuleId, Stage};

/// 組合樹節點:葉 = sign 引用(P24 Ref 精神,以名字定址);環 = 詞幹層組合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Component {
    Sign(String),
    Ring(Vec<Component>),
}

impl Component {
    pub fn sign(name: impl Into<String>) -> Component {
        Component::Sign(name.into())
    }
}

/// 一個片語:每個元素 = 一個韻律詞(ω;P1 預設域),sandhi 域為整個片語。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseSpec(pub Vec<Component>);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WordError {
    #[error("unknown sign {0:?} in composition")]
    UnknownSign(String),
    #[error("sign {0:?} has no `phon` Definition (UR required for word building)")]
    UrMissing(String),
    #[error("sign {sign:?}: `phon` must be /…/ form, got {value:?}")]
    UrMalformed { sign: String, value: String },
    /// P3 cophonology 僅界定於 stem 層;其他 stage 的 sign 局部規則尚無語意。
    #[error("sign {sign:?} local rule has stage {stage:?}: only stem-stage cophonology is defined (P3)")]
    UnsupportedSignRuleStage { sign: String, stage: &'static str },
    #[error("sign {sign:?}: cophonology rule-set rejected: {msg}")]
    CophonologyCompile { sign: String, msg: String },
    /// M1 子集:cophonology 只允許音段效果(I18);旋律殘留 = 需 Word 級拼接。
    #[error("sign {sign:?}: cophonology left melodic material on tiers (non-segmental cophonology = 步驟 18)")]
    CophonologyNonSegmental { sign: String },
    #[error(transparent)]
    Codegen(#[from] CodegenError),
    #[error("engine: {0}")]
    Engine(String),
}

/// 一次導出的完整證據(P1:用後即棄,不儲存)。
#[derive(Debug, Clone)]
pub struct Derivation {
    /// cophonology 前趟後、進 `build_phrase` 的文字("ap+xp ba" 形)。
    pub input_text: String,
    /// 三個 stage 切片各自的 StepRecord 數(stem/word/phrase)。
    pub steps_per_stage: [usize; 3],
    /// 全部推導步(依 stem → word → phrase 串接)。
    pub steps: Vec<StepRecord>,
    /// 末狀態(臨時 Word)。
    pub word: Word,
    /// 表層(spell-out 純函數;`+`/空白縫由引擎渲染)。
    pub surface: String,
}

fn stage_str(s: Stage) -> &'static str {
    match s {
        Stage::Stem => "stem",
        Stage::Word => "word",
        Stage::Phrase => "phrase",
    }
}

fn find_sign<'a>(a: &'a Artifacts, name: &str) -> Result<&'a CompiledSign, WordError> {
    a.signs
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| WordError::UnknownSign(name.to_owned()))
}

/// `phon = /…/` → UR 文字。
fn underlying_form(sign: &CompiledSign) -> Result<String, WordError> {
    let value = sign
        .defs
        .iter()
        .rev() // ③ 後者勝已套用;同 path 至多一筆,rev 僅防衛
        .find(|(p, _)| p == "phon")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| WordError::UrMissing(sign.name.clone()))?;
    value
        .strip_prefix('/')
        .and_then(|v| v.strip_suffix('/'))
        .map(str::to_owned)
        .ok_or(WordError::UrMalformed {
            sign: sign.name.clone(),
            value,
        })
}

/// cophonology 前趟(P3/P4):sign 局部 stem 規則在自己的葉上跑,
/// 回傳演化後的 UR 文字。非 stem 局部規則、非音段效果 → 顯式拒絕(I18)。
fn leaf_text(a: &Artifacts, sign: &CompiledSign) -> Result<String, WordError> {
    let ur = underlying_form(sign)?;
    if sign.rules.is_empty() {
        return Ok(ur);
    }
    for r in &sign.rules {
        if r.stage != Stage::Stem {
            return Err(WordError::UnsupportedSignRuleStage {
                sign: sign.name.clone(),
                stage: stage_str(r.stage),
            });
        }
    }
    // 小規則組 = dsl 域宣告 verbatim + 該 sign 的 stem 規則(排放同 codegen,I18)
    let mut src = String::new();
    for line in &a.pipeline.ordered.dsl_decls {
        src.push_str(line);
        src.push('\n');
    }
    src.push('\n');
    let mut n = 1u32;
    for r in &sign.rules {
        let rule = Rule {
            id: RuleId(0), // 不入排放(I15-b);僅為型別完整
            body: r.body.clone(),
            stage: r.stage,
            dim: crate::Dim::Phon, // cophonology = phon 音段效果(I18)
            else_chain: r.else_chain.clone(),
            then_chain: Vec::new(),
        };
        codegen::emit_rule(&mut src, &mut n, &rule)?;
    }
    let prog = tshiatun_dsl::compile(&src).map_err(|e| WordError::CophonologyCompile {
        sign: sign.name.clone(),
        msg: e.to_string(),
    })?;
    let w = tshiatun_dsl::build_word(&prog, &ur).map_err(|e| WordError::CophonologyCompile {
        sign: sign.name.clone(),
        msg: e.to_string(),
    })?;
    let fallback = w.clone();
    let steps = run_program(&prog, w).map_err(|e| WordError::Engine(e.to_string()))?;
    let out = steps.last().map(|s| &s.word).unwrap_or(&fallback);
    if out.melodies.iter().any(|t| !t.seq.is_empty()) {
        return Err(WordError::CophonologyNonSegmental {
            sign: sign.name.clone(),
        });
    }
    // 骨架 → 文字(音段效果子集:重新分詞必然成功,符號皆為宣告名)
    Ok(out
        .skeleton
        .iter()
        .filter_map(|s| prog.env.syms.resolve(s.sym))
        .collect())
}

/// 展平一個韻律詞的組合樹 → 詞幹單元序列(巢狀環深度序展平,I18)。
fn flatten<'a>(c: &'a Component, out: &mut Vec<&'a str>) {
    match c {
        Component::Sign(n) => out.push(n),
        Component::Ring(parts) => {
            for p in parts {
                flatten(p, out);
            }
        }
    }
}

/// 組合樹 → `build_phrase` 文字(cophonology 前趟已套用)。
pub fn phrase_text(a: &Artifacts, spec: &PhraseSpec) -> Result<String, WordError> {
    let mut pwords = Vec::with_capacity(spec.0.len());
    for pword in &spec.0 {
        let mut leaves = Vec::new();
        flatten(pword, &mut leaves);
        let mut units = Vec::with_capacity(leaves.len());
        for name in leaves {
            units.push(leaf_text(a, find_sign(a, name)?)?);
        }
        pwords.push(units.join("+"));
    }
    Ok(pwords.join(" "))
}

/// 臨時 Word 建構(P1):組合樹 → 韻律域(括號由引擎維護)。
pub fn build_word(a: &Artifacts, spec: &PhraseSpec) -> Result<Word, WordError> {
    let text = phrase_text(a, spec)?;
    build_phrase(&a.grammar.program, &text).map_err(|e| WordError::Engine(e.to_string()))
}

/// Program 的 stage 切片(呼叫端驅動的兌現;切片保 ④ 書寫序)。
fn stage_slice(p: &Program, stage: Stage) -> Program {
    let mut sliced = p.clone();
    sliced.rules.retain(|r| r.stage == stage);
    sliced
}

/// P3 循環套用驅動:stem 切片 → word 切片 → phrase 切片 → spell-out。
///
/// 對展平組合(M1)而言,三切片串跑 ≡ 對 ④ 排序後規則集單趟 `run_program`
/// (切片保書寫序;等價性由測試釘住)——但驅動權在呼叫端,後續步驟
/// (真組合樹逐環重跑)在此擴充,引擎不變。
pub fn derive(a: &Artifacts, spec: &PhraseSpec) -> Result<Derivation, WordError> {
    let input_text = phrase_text(a, spec)?;
    let mut w = build_phrase(&a.grammar.program, &input_text)
        .map_err(|e| WordError::Engine(e.to_string()))?;
    let mut steps = Vec::new();
    let mut steps_per_stage = [0usize; 3];
    for (i, stage) in [Stage::Stem, Stage::Word, Stage::Phrase].into_iter().enumerate() {
        let slice = stage_slice(&a.grammar.program, stage);
        if slice.rules.is_empty() {
            continue;
        }
        let pass = run_program(&slice, w.clone()).map_err(|e| WordError::Engine(e.to_string()))?;
        if let Some(last) = pass.last() {
            w = last.word.clone();
        }
        steps_per_stage[i] = pass.len();
        steps.extend(pass);
    }
    let surface =
        surface_phrase(&a.grammar.program, &w).map_err(|e| WordError::Engine(e.to_string()))?;
    Ok(Derivation {
        input_text,
        steps_per_stage,
        steps,
        word: w,
        surface,
    })
}
