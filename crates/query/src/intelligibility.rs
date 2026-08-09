//! 互通度:**只定接口,不定公式**(`演化圖本體論` §6.2)。
//!
//! > 此「可替換函數」設計同 DSL 的 strategy 模組化(D28):本體**不綁死**
//! > 「互通度怎麼算」,只定接口。
//!
//! # 為什麼結果帶 `measure_id`
//!
//! 裸 `f64` 會讓 UI 顯示一個看似客觀的「A 與 B 互通度 73%」,但那只是某個
//! heuristic 的輸出。帶身分是本專案既有模式——`EffectiveDistribution::provenance()`
//! 就是「每一項查得出來自哪一層」。
//!
//! # 為什麼沒有 `IntelligibilityContext`
//!
//! §6.2 把**有向版**(A 懂 B ≠ B 懂 A,依接觸史與聲望)明列為
//! **【N】v-next 留白**,「待 multi-agent 上線」。現在造一個沒有生產者的
//! context 型別,就是《共時lang語法與資料貼合度》禁止的「先造無消費者語法」。
//!
//! 但接口**留了餘地**:[`IntelligibilityInput`] 拿的是兩份**文件**而非只有
//! `DiffVector`,故有向實作日後拿得到雙方的內容。真正還缺的是 `contact_history`
//! ——它住 `EvolutionState`(撰寫時、雜湊外),接上時再加欄位。
//!
//! # 引擎不定義評分合成公式
//!
//! §6.4 的裁定。故本模組唯一的內建實作叫 [`ExploratoryHeuristicV1`],
//! **不叫 `Default`**——名字要說出它是什麼:一組可探索用的權宜係數,
//! 不是引擎對「互通度」的主張。係數是**建構參數**,不是藏起來的常數。

use serde::{Deserialize, Serialize};
use conlang_changeset::diff::{diff_vector, DiffVector};
use conlang_language::{Dim, LanguageDocument};

/// 一次互通度查詢的輸入。
#[derive(Debug, Clone, Copy)]
pub struct IntelligibilityInput<'a> {
    /// 「聽的人」的語言。對稱實作忽略方向。
    pub source: &'a LanguageDocument,
    /// 「被聽的」那個。
    pub target: &'a LanguageDocument,
    pub diff: &'a DiffVector,
}

/// 一個互通度分數。**永遠帶著它是誰算的。**
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntelligibilityScore {
    /// 0.0(完全不通)–1.0(完全相通)。
    pub value: f64,
    pub measure_id: String,
    /// 對稱實作為 `true`;有向實作(【N】)為 `false`。
    pub symmetric: bool,
}

pub trait IntelligibilityMeasure: std::fmt::Debug {
    fn id(&self) -> &str;
    fn score(&self, input: &IntelligibilityInput<'_>) -> IntelligibilityScore;
}

/// 便利:直接對兩份文件求分數(自己算 diff)。
pub fn intelligibility(
    source: &LanguageDocument,
    target: &LanguageDocument,
    measure: &dyn IntelligibilityMeasure,
) -> IntelligibilityScore {
    let diff = diff_vector(source, target);
    measure.score(&IntelligibilityInput {
        source,
        target,
        diff: &diff,
    })
}

/// 各分量的權重。**這是語言學判斷,不是引擎事實**——故是資料,由呼叫端給。
///
/// `演化圖本體論` §6.1 提過「詞彙差異最傷互通,規則性音變其次」,
/// [`ExploratoryHeuristicV1::suggested`] 依此給一組**建議值**;但那是引用,
/// 不是引擎的主張,換掉不需要改任何程式碼。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DimensionWeights {
    pub phon: f64,
    pub syn: f64,
    pub sem: f64,
    pub prag: f64,
    pub structural: f64,
    /// 生滅(詞彙表本身的增減)相對於「對齊詞有差異」的權重。
    pub birth_death: f64,
}

/// **可探索用**的對稱互通度,不是引擎對互通度的定義。
///
/// 公式(刻意寫在文件裡,因為它是可被反駁的主張而非事實):
///
/// ```text
/// 相異度 = Σ(wᵢ × 該維有差異的對齊詞數) / (Σwᵢ × 對齊詞數)
///          再與 生滅比 依 birth_death 權重混合
/// 互通度 = 1 − 相異度
/// ```
///
/// 兩份完全相同的文件得 1.0;毫無共同 sign 的兩份得 0.0。
#[derive(Debug, Clone)]
pub struct ExploratoryHeuristicV1 {
    pub weights: DimensionWeights,
}

impl ExploratoryHeuristicV1 {
    /// §6.1 的敘述「詞彙差異最傷互通,規則性音變其次」的一組**建議**係數。
    ///
    /// 這組數字沒有實證來源,是把那句話直譯成權重。要嚴肅使用請換掉。
    pub fn suggested() -> ExploratoryHeuristicV1 {
        ExploratoryHeuristicV1 {
            weights: DimensionWeights {
                sem: 3.0,
                phon: 2.0,
                syn: 1.0,
                prag: 0.5,
                structural: 1.0,
                birth_death: 3.0,
            },
        }
    }
}

impl IntelligibilityMeasure for ExploratoryHeuristicV1 {
    fn id(&self) -> &str {
        "exploratory_heuristic_v1"
    }

    fn score(&self, input: &IntelligibilityInput<'_>) -> IntelligibilityScore {
        let w = &self.weights;
        let diff = input.diff;
        let aligned = diff.aligned_signs();
        let (born, died) = (diff.born_signs(), diff.died_signs());
        let total = aligned + born + died;
        let value = if total == 0 {
            // 兩邊都空:沒有任何可比之處,視為相通(而非 0)——否則兩份空文件
            // 會被判成互不相通,那是量測不到而非真的不通。
            1.0
        } else {
            let per_dim = [
                (Dim::Phon, w.phon),
                (Dim::Syn, w.syn),
                (Dim::Sem, w.sem),
                (Dim::Prag, w.prag),
            ];
            let mut weighted = 0.0;
            let mut mass = 0.0;
            for (dim, weight) in per_dim {
                weighted += weight * diff.dimension(dim).signs.changed as f64;
                mass += weight * aligned as f64;
            }
            weighted += w.structural * diff.structural.signs.changed as f64;
            mass += w.structural * aligned as f64;
            // 生滅:分母是全部 sign,不只對齊的那些
            weighted += w.birth_death * (born + died) as f64;
            mass += w.birth_death * total as f64;

            if mass == 0.0 {
                1.0
            } else {
                (1.0 - weighted / mass).clamp(0.0, 1.0)
            }
        };
        IntelligibilityScore {
            value,
            measure_id: self.id().to_owned(),
            symmetric: true,
        }
    }
}
