//! conlang-generate — 模組 A:**造詞流水線**(鳥瞰步驟 18;`Sign生成引擎本體論`)。
//!
//! ```text
//! Need ──▶ Generator(唯讀)──▶ Vec<Proposal>(帶評分)
//!                                  │  選擇(候選列舉與選擇分離,P70)
//!                                  ▼
//!                          Builder(純協調)──▶ AtomicRewrite ──▶ Vec<PrimitiveEdit>
//!                                  │ 委派 Strategy
//!                                  └─ validate / blocking / resolve
//! ```
//!
//! # 兩條決定形狀的裁定
//!
//! **C1(增修 A,本體論 §12)**:規格原文把「身分/共享/fork」判給一個 `Store`,
//! 但 2.0 之後**語言知識只住 `Language`**(P2/P10),「Grammar Store 容器」計畫
//! 已作廢。故 Builder **不改 `Language`**,而是產出 `Vec<PrimitiveEdit>`;
//! 且降階**複用既有機制**——構造 `AtomicRewrite` 交
//! [`conlang_changeset::rewrite::expand`],不另闢平行路徑。
//!
//! 效果:造出來的詞自動可 replay、進得了演化圖、受 `.chg` 三道 digest 保護
//! ——步驟 13–17 的機器全部免費繼承。
//!
//! **P70**:候選**列舉**與**選擇**分離,**零候選是合法結果**(`Ok(None)`),
//! 不是錯誤。本 crate 的 [`Generator`] 只列舉,選擇交
//! [`select_proposal`] 或呼叫端。
//!
//! # 職責紅線(本體論 §0)
//!
//! - `Generator` **唯讀**:讀 Need + Language,不寫、不配 id、不做 validate/blocking;
//! - `Proposal` 是**幻影**:不進 Language、無 id,帶評分且**排序在提議側**;
//! - `Builder` **純協調**:不內建語言學,validate/blocking/resolve 一律委派 Strategy。
//!   「Builder 開始懂語言學」是規格明列的紅線。
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use conlang_changeset::rewrite::{expand, AtomicRewrite, DonorScope, RewriteError, ServiceContext};
use conlang_changeset::PrimitiveEdit;
use conlang_language::{
    sample_weighted_index, CompiledSystem, LanguageDocument, SignDef, SignItem, SignRef,
    SourceLocation, WeightedSampleError,
};

pub mod strategy;

pub use strategy::{
    BlockingStrategy, NoBlocking, RejectDuplicateName, ResolveStrategy, Strategies, StrategyError,
    ValidateStrategy,
};

/// 造詞**意圖**。只說「要什麼」,不說形式(本體論 §0:Need 不決定形式)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Need {
    /// 目標 sign 名。
    pub name: String,
    /// 要掛入的分類(`belongs`)。
    pub categories: Vec<String>,
    /// 語意目標——現為主義項的 gloss(P71 §4.1:義項住 `senses:`)。
    pub gloss: Option<String>,
    /// 構詞方式。
    pub origin: NeedOrigin,
}

/// 這個詞**怎麼來的**。對映本體論 §5 的兩種構詞與借入。
///
/// 三者都經 [`AtomicRewrite::Create`] 落地,差別只在隨附的 metadata
/// (`origin` / `provenance`)。
///
/// **不使用 `AtomicRewrite::Adopt`**:那是「從 donor **文件**搬一個既有 sign」,
/// 需要 `DonorScope` 帶進另一份 `LanguageDocument`——與「在本語言造一個標記為
/// 借詞的新 sign」是兩件事。真正的 donor 借入沿用既有 `Adopt`,不在本模組重造。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedOrigin {
    /// 從零生成(無來源)。
    Coined,
    /// 由既有 sign 組合而來(§5 組合造詞);記入 `origin`。
    Composed { from: String },
    /// 在本語言造一個標記為借詞的新 sign;記入 `provenance = loan`。
    Borrowed,
}

/// 一個候選。**幻影**:不進 Language、無 id(本體論 §3)。
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// 形式(UR)。
    pub phon: String,
    /// 評分。**引擎不定義它怎麼算**(統計先驗 §6.4:合成公式永久擱置)。
    ///
    /// 各 Generator 自行決定並填一個數;引擎只拿它排序([`ranked`],手動模式)
    /// 或作抽樣權重([`sample_proposal`],自動模式)。本體論 §3 說「音韻/文化/
    /// 詞頻分量」,但**如何合成是提議側的事**——同節也寫明「排序是提議側的事」。
    ///
    /// 文化分量另有既有約束:旁註層「不參與 replay、**不約束生成**(軟提示)」,
    /// 若寫進引擎級公式就等於在約束生成。
    pub score: f64,
    /// 提議者自述的理由,進 trace 供 UI 顯示;引擎不解讀。
    pub rationale: String,
}

/// 唯讀提議者。可多實作(規則/借詞/逆構詞/LLM),共用同一個 [`build`]。
pub trait Generator: std::fmt::Debug {
    /// **唯讀**:讀 Need 與已編譯系統,不得寫入任何東西。
    fn propose(&self, need: &Need, system: &CompiledSystem) -> Vec<Proposal>;
}

// ── 選擇層:**兩個模式,不是一個** ────────────────────────────────────────
//
// 架構書 §0 的核心賣點是「可調式人為干涉程度」——每個模組可獨立選手動/輔助/自動。
// P12 把抽樣器的角色寫死在括號裡:「Weight DB 決定機率(**自動模式**的抽樣權重)
// → 抽樣器選擇」,而 `function分支語意與選擇層_v1.0` §221 補充:
// 「互動模式是使用者從候選面板挑,**全程不碰抽樣器**」。
//
// 故此處提供兩個入口,對應兩個模式;**沒有一個「預設選擇層」**:
//
// | 模式 | 入口 | 抽樣器 |
// |---|---|---|
// | 手動 / 輔助 | [`ranked`] —— 引擎只排序,選擇權交出去 | 不碰 |
// | 自動 | [`sample_proposal`] —— seeded 加權抽樣 | 走 `sample_weighted_index` |
//
// 兩者共通:候選**列舉**與**選擇**分離,**零候選是合法結果**(P70)。

/// 一次自動模式選擇的完整軌跡。比照步驟 17 的 `GoalSelectionTrace`
/// ——自動造出來的詞必須能回答「為什麼是這個」。
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalSelectionTrace {
    /// 抽樣演算法名。升版必須顯式改 golden。
    pub algorithm: &'static str,
    pub seed: u64,
    /// 觸發這次生成的 Need 名。
    pub need: String,
    /// 候選與**實際餵給抽樣器的權重**。今日權重即 `Proposal::score`;
    /// 日後若疊上有效分佈(步驟 19),兩者才會分歧。
    pub ordered: Vec<(Proposal, f64)>,
    pub selected_index: usize,
    pub selected: Proposal,
}

#[derive(Debug, thiserror::Error)]
pub enum SelectionError {
    #[error("GENERATE_SAMPLING: {0}")]
    Sampling(#[from] WeightedSampleError),
}

/// **手動 / 輔助模式**:引擎只把候選排好,**不做選擇**。
///
/// 依評分由高到低;同分保**列舉序**(穩定排序),故同輸入同輸出(P26)。
/// 回傳借用,呼叫端(候選面板)自己挑。
pub fn ranked(proposals: &[Proposal]) -> Vec<&Proposal> {
    let mut ordered: Vec<&Proposal> = proposals.iter().collect();
    ordered.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ordered
}

/// **自動模式**:依 `score` 作權重 seeded 加權抽樣。
///
/// 走與步驟 17 Goal 選擇**同一個** `sample_weighted_index`(注入式
/// `ChaCha20Rng`,§4 禁環境隨機源),同 seed 同輸入逐位元可重現。
///
/// 零候選回 `Ok(None)`——「提不出東西」是合法結果(P70),不是錯誤。
/// 但**有候選卻全部零權重**是提議者的問題,回 `Err`:自動模式無從抽起。
pub fn sample_proposal(
    need: &Need,
    proposals: &[Proposal],
    seed: u64,
) -> Result<Option<ProposalSelectionTrace>, SelectionError> {
    if proposals.is_empty() {
        return Ok(None);
    }
    let weights = proposals.iter().map(|p| p.score).collect::<Vec<_>>();
    let sample = sample_weighted_index(&weights, seed)?;
    let selected = proposals[sample.selected_index].clone();
    Ok(Some(ProposalSelectionTrace {
        algorithm: sample.algorithm,
        seed,
        need: need.name.clone(),
        ordered: proposals.iter().cloned().zip(weights).collect(),
        selected_index: sample.selected_index,
        selected,
    }))
}

/// 便利函數:取評分最高者。
///
/// **不服務任何一個模式**——自動模式該抽樣(否則同一個 Need 永遠得到同一個詞,
/// 而自動造 200 個詞要的是可重現的**變化**),手動模式該把選擇權交出去。
/// 保留它只為了測試與「我就是要最好的那個」這種一次性查詢。
pub fn highest_scoring(proposals: &[Proposal]) -> Option<&Proposal> {
    ranked(proposals).first().copied()
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("GENERATE_STRATEGY: {0}")]
    Strategy(#[from] StrategyError),
    #[error("GENERATE_LOWERING: {0}")]
    Lowering(#[from] RewriteError),
}

/// **唯一的編輯產生點**(C1-b)。把選定的 proposal 翻成四原語。
///
/// Builder 自己不懂語言學:良構、阻擋、衝突一律先問 [`Strategies`],
/// 通過後才構造 [`AtomicRewrite`] 並交既有降階(C1-c)。
pub fn build(
    need: &Need,
    chosen: &Proposal,
    document: &LanguageDocument,
    strategies: &Strategies,
) -> Result<Vec<PrimitiveEdit>, BuildError> {
    let sign = materialize(need, chosen);

    // 委派,不內建(本體論 §0 紅線)
    strategies.validate.check(&sign, document)?;
    strategies.blocking.check(need, &sign, document)?;
    let sign = strategies.resolve.resolve(sign, document)?;

    // 三種來源都落在同一個 Atomic Rewrite 上,差別已寫進 metadata(見 `materialize`)。
    let rewrite = AtomicRewrite::Create { sign };
    Ok(expand(
        &rewrite,
        document,
        &ServiceContext::offline(),
        &DonorScope::new(),
    )?)
}

/// Need + Proposal → `SignDef`。**純機械組裝**,不做任何判斷。
fn materialize(need: &Need, chosen: &Proposal) -> SignDef {
    let mut items = Vec::new();
    for category in &need.categories {
        items.push(SignItem::Belongs(category.clone()));
    }
    items.push(SignItem::Def(conlang_language::Def {
        path: "phon".to_owned(),
        value: chosen.phon.clone(),
    }));
    if let Some(gloss) = &need.gloss {
        // P71 §4.1:義項住 `senses:`,不是 `sem.gloss` Def。
        items.push(SignItem::Sense(conlang_language::Sense {
            name: "core".to_owned(),
            gloss: gloss.clone(),
            source: SourceLocation::unknown(),
        }));
    }
    match &need.origin {
        NeedOrigin::Coined => {}
        NeedOrigin::Composed { from } => {
            items.push(SignItem::Def(conlang_language::Def {
                path: "origin".to_owned(),
                value: format!("sign({})", SignRef(from.clone()).0),
            }));
            items.push(SignItem::Def(conlang_language::Def {
                path: "provenance".to_owned(),
                value: "derived".to_owned(),
            }));
        }
        NeedOrigin::Borrowed => {
            items.push(SignItem::Def(conlang_language::Def {
                path: "provenance".to_owned(),
                value: "loan".to_owned(),
            }));
        }
    }
    SignDef {
        id: conlang_language::SignId::synthetic(),
        name: need.name.clone(),
        items,
    }
}
