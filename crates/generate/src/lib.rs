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

use conlang_changeset::state::EvolutionState;
pub use conlang_changeset::state::{Contact, ContactIntensity};
pub use conlang_stats::{DistributionProvider, WeightTable};
use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Debug, thiserror::Error)]
pub enum GenerationError {
    #[error("GENERATE_DISTRIBUTION_EMPTY: no weighted segments were supplied")]
    EmptyDistribution,
    #[error(
        "GENERATE_DISTRIBUTION_NO_POSITIVE_WEIGHT: no weighted segment has a positive weight"
    )]
    NoPositiveDistributionWeight,
    #[error(
        "GENERATE_DISTRIBUTION_INVALID: segment {segment:?} has non-finite or negative weight {weight}"
    )]
    InvalidWeight { segment: String, weight: f64 },
    #[error("GENERATE_TEMPLATE_CLASS_MISSING: slot {slot} requires Class {class_name}")]
    MissingClass { slot: char, class_name: &'static str },
    #[error("GENERATE_TEMPLATE_NO_SEGMENTS: slot {slot} has no matching weighted segments")]
    NoSegments { slot: char },
    #[error("GENERATE_TEMPLATE_NO_POSITIVE_WEIGHT: slot {slot} has no positive matching weight")]
    NoPositiveSlotWeight { slot: char },
    #[error("GENERATE_SAMPLING: {0}")]
    Sampling(#[from] WeightedSampleError),
}

/// 唯讀提議者。可多實作(規則/借詞/逆構詞/LLM),共用同一個 [`build`]。
pub trait Generator: std::fmt::Debug {
    /// **唯讀**:讀 Need 與已編譯系統,不得寫入任何東西。
    fn propose(
        &self,
        need: &Need,
        system: &CompiledSystem,
    ) -> Result<Vec<Proposal>, GenerationError>;
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
        items.push(SignItem::TraitMount { name: category.clone(), kind: conlang_language::TraitMountKind::Declaration });
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

// ── phonotactics 過濾:**注入式**(統計先驗 §6.3)────────────────────────

/// 硬約束把關者。**由呼叫端注入**,`generate` 不依賴音變引擎。
///
/// 統計先驗 §4 的分工:**E 只管加權抽樣;硬約束(音位配列)由既有 phonotactics
/// 驗證器過濾抽樣結果**。兩職責分立——E 不碰約束、驗證器不碰頻率。
///
/// 之所以是 trait 而非直接依賴:驗證器住 tshiatun submodule,而本 crate
/// **不依賴引擎**。注入式另有兩個好處——「不同語言不同 phonotactics」天然成立
/// (換一個實作即可),測試可塞兩行假實作而不必備妥完整 `Artifacts`。
///
/// 與 crate 內既有形狀一致([`Strategies`]、`DistributionProvider`、DSL 的 D28),
/// 不是新機制。
pub trait PhonotacticFilter: std::fmt::Debug {
    /// 這個候選形式在本語言合不合法。
    fn admits(&self, phon: &str) -> bool;
}

/// 全部放行。用於「還沒宣告 phonotactics」的語言。
#[derive(Debug)]
pub struct AdmitAll;

impl PhonotacticFilter for AdmitAll {
    fn admits(&self, _phon: &str) -> bool {
        true
    }
}

/// 列舉後**事後過濾**(§4:E 抽樣在前,硬約束在後)。
///
/// 刻意不在 `Generator` 內部過濾:提議與把關是兩個職責,混在一起就無法
/// 回答「提了幾個、被擋掉幾個」——那是自動模式要能審計的數字。
pub fn admissible(proposals: Vec<Proposal>, filter: &dyn PhonotacticFilter) -> Vec<Proposal> {
    proposals
        .into_iter()
        .filter(|proposal| filter.admits(&proposal.phon))
        .collect()
}

// ── 依有效分佈造形式的 Generator(流 C 圖上的「Generator + E 抽樣」)──────

/// 依**有效分佈**抽音素、依模板組形式的提議者。
///
/// 流 C 的圖上那格寫的是「Generator(唯讀 Language **+ E 抽樣**)」
/// ——抽樣在提議側,不是事後對候選加權。本型別即該接點的最小實作。
///
/// 決定性(P26):同 `seed`、同分佈、同模板 ⇒ 逐位元同結果。每個候選以
/// `seed + 候選序號` 派生子種子,故候選之間互不相同而整體可重現。
#[derive(Debug)]
pub struct DistributionGenerator<'a> {
    /// 疊完的三層有效分佈(手動 > provider > E1)。
    pub distribution: &'a WeightTable,
    /// 音節模板:`C` 從 `Class consonant`、`V` 從 `Class vowel` 抽樣;
    /// 其餘字元原樣輸出。
    pub template: &'a str,
    /// 要提幾個候選。
    pub count: usize,
    pub seed: u64,
}

impl Generator for DistributionGenerator<'_> {
    fn propose(
        &self,
        _need: &Need,
        system: &CompiledSystem,
    ) -> Result<Vec<Proposal>, GenerationError> {
        if self.distribution.is_empty() {
            return Err(GenerationError::EmptyDistribution);
        }
        for (segment, weight) in self.distribution.iter() {
            if !weight.is_finite() || weight < 0.0 {
                return Err(GenerationError::InvalidWeight {
                    segment: segment.to_owned(),
                    weight,
                });
            }
        }
        if !self
            .distribution
            .iter()
            .any(|(_, weight)| weight > 0.0)
        {
            return Err(GenerationError::NoPositiveDistributionWeight);
        }

        // 先把模板會用到的類別解析一次。這既避免同一個類別在每一個候選、每一個
        // 位置反覆掃描，也讓「模板宣告了 C/V 但語言沒有對應 Class」在開始抽樣前
        // 就成為明確錯誤。
        let consonants = self
            .template
            .contains('C')
            .then(|| self.slot_sampler('C', "consonant", system))
            .transpose()?;
        let vowels = self
            .template
            .contains('V')
            .then(|| self.slot_sampler('V', "vowel", system))
            .transpose()?;

        (0..self.count)
            .map(|index| {
                let mut form = String::new();
                for (position, slot) in self.template.chars().enumerate() {
                    let sampler = match slot {
                        'C' => consonants.as_ref().ok_or(GenerationError::MissingClass {
                            slot,
                            class_name: "consonant",
                        })?,
                        'V' => vowels.as_ref().ok_or(GenerationError::MissingClass {
                            slot,
                            class_name: "vowel",
                        })?,
                        literal => {
                            form.push(literal);
                            continue;
                        }
                    };
                    // 每個位置派生一個子種子:同 seed 可重現,位置間不相關。
                    let derived = self
                        .seed
                        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                        .wrapping_add((index as u64) << 32)
                        .wrapping_add(position as u64);
                    let sample = sample_weighted_index(&sampler.1, derived)?;
                    form.push_str(sampler.0[sample.selected_index]);
                }
                Ok(Proposal {
                    // 評分:引擎不定義怎麼算(統計先驗 §6.4)。此實作給等權
                    // ——它已依分佈抽樣,候選之間無進一步高下可言。
                    score: 1.0,
                    rationale: format!("模板 {} 依有效分佈抽樣", self.template),
                    phon: format!("/{form}/"),
                })
            })
            .collect()
    }
}

impl DistributionGenerator<'_> {
    /// 取出一個模板槽能抽樣的分佈子集。類別歸屬以**已編譯的 DSL program**為準，
    /// 不以字串命名或 Unicode 啟發式猜測母音／子音。
    fn slot_sampler(
        &self,
        slot: char,
        class_name: &'static str,
        system: &CompiledSystem,
    ) -> Result<(Vec<&str>, Vec<f64>), GenerationError> {
        let program = &system.artifacts.grammar.program;
        let members = program
            .classes
            .get(class_name)
            .ok_or(GenerationError::MissingClass { slot, class_name })?;
        let member_names: BTreeSet<&str> = members
            .iter()
            .filter_map(|symbol| program.env.syms.resolve(*symbol))
            .collect();
        let (segments, weights): (Vec<_>, Vec<_>) = self
            .distribution
            .iter()
            .filter(|(segment, _)| member_names.contains(*segment))
            .unzip();

        if segments.is_empty() {
            return Err(GenerationError::NoSegments { slot });
        }
        if !weights.iter().any(|weight| *weight > 0.0) {
            return Err(GenerationError::NoPositiveSlotWeight { slot });
        }
        Ok((segments, weights))
    }
}

// ── State → 抽樣權重(裁定 A:只在撰寫時)────────────────────────────────

/// 由 **State 的語言接觸事實**推導出一份抽樣權重,餵給有效分佈的 provider 層。
///
/// 裁定 (A):State 只在**撰寫時**被讀。此型別即那個接點——它產出的權重會
/// 影響**新造的詞**,但選定結果寫死進 `.chg`,**replay 永不讀 State**。
///
/// # 為什麼是 provider 而不是第四層
///
/// `DistributionProvider` 是既有接點(統計先驗 §3),接上去零新機制;
/// 而把 State 做成有效分佈的獨立一層,等於宣稱它是抽樣棧的常駐成員
/// ——那會誘使日後有人在 replay 路徑上讀它。走 provider 則語意清楚:
/// **這是撰寫時注入的一份 snapshot**(§3:「導入 = 複製 snapshot 進覆寫層,
/// 非外部活連結」)。
///
/// # 對方語言的分佈由呼叫端給
///
/// 引擎不知道「古諾斯語」長什麼樣。`counterpart_distributions` 由呼叫端提供
/// ——可能來自另一個演化節點(`OtherNode`)、E1 的某個真實語言,或使用者手填。
#[derive(Debug)]
pub struct ContactInfluence<'a> {
    pub state: &'a EvolutionState,
    /// 對方語言名 → 其音素分佈。名字對應 [`Contact::counterpart`]。
    pub counterpart_distributions: &'a BTreeMap<String, WeightTable>,
}

impl DistributionProvider for ContactInfluence<'_> {
    /// 各接觸語言的分佈 × 強度係數,逐鍵累加。
    ///
    /// 係數取 [`ContactIntensity::default_factor`]——那是**預設**,
    /// 「接觸多密切 ⇒ 借詞音素該加權多少」是語言學判斷不是引擎事實。
    /// 要換一套,寫自己的 `DistributionProvider` 即可(同 §6.4 不定義評分公式)。
    ///
    /// 名字在 `counterpart_distributions` 裡查無的接觸**靜靜略過**——
    /// 那表示呼叫端還沒提供那個語言的分佈,不是錯誤;State 是環境標註,
    /// 記了一段接觸不代表一定要拿它來抽樣。
    fn distribution(&self) -> WeightTable {
        let mut table = WeightTable::new();
        for contact in &self.state.contacts {
            let Some(source) = self.counterpart_distributions.get(&contact.counterpart) else {
                continue;
            };
            let factor = contact.intensity.default_factor();
            for (segment, weight) in source.iter() {
                let next = table.get(segment).unwrap_or(0.0) + weight * factor;
                table.set(segment, next);
            }
        }
        table
    }
}
