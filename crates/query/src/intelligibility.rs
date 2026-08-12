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
use conlang_changeset::diff::{diff_vector, DiffVector, TraitDiff};
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
    /// **一條 trait 規則改動,對它波及到的每個詞造成多少傷害**——以「那個詞
    /// 整個不一樣」為 1.0。
    ///
    /// §6.1:「詞彙差異最傷互通,**規則性音變其次**」。這個係數就是「其次」
    /// 的量化,故必然 < 1:一個詞被音變改了發音,仍比它換成另一個詞好懂。
    pub trait_rule: f64,
    /// **一條 trait 內容改動**(分類 `belongs`、slot、義項)同上。
    ///
    /// 比音變重:它改的是那些詞**是什麼**(role 約束、slot 授權、語意型別),
    /// 不只是怎麼唸。
    pub trait_content: f64,
}

/// **可探索用**的對稱互通度,不是引擎對互通度的定義。
///
/// 公式(刻意寫在文件裡,因為它是可被反駁的主張而非事實):
///
/// ```text
/// 直接傷害ᵢ = 該維有差異的對齊詞數
/// 間接傷害ᵢ = ρ_rule    × 該維規則事件數   × 那些規則波及的詞數
///           + ρ_content × 該維 trait 內容改動數 × 那些 trait 波及的詞數
/// 相異度   = Σ(wᵢ × (直接ᵢ + 間接ᵢ)) / (Σwᵢ × 對齊詞數)
///            再與 生滅比 依 birth_death 權重混合
/// 互通度   = 1 − 相異度
/// ```
///
/// 兩份完全相同的文件得 1.0;毫無共同 sign 的兩份得 0.0。
///
/// # 間接傷害為什麼要乘 reach
///
/// 一條音變**宣告一次、作用到一整片詞**(裁定丙:diff 在宣告處記 1,波及面
/// 另記於 `reach_*`)。若只看「改了幾條規則」,一條作用於全語言的音變與一條
/// 作用於三個詞的音變會得到同一個數字——而互通度問的正是「有多少詞聽不懂」。
///
/// **reach 取前後的 max**:裁定 §3.1 明說引擎發兩個數、由呼叫端決定怎麼合成
/// (要 max、要平均、要 union 大小)。這裡是呼叫端,選 max ——「這次改動牽動過
/// 的詞」取兩邊的較大者,不會因為一條 `belongs` 邊剛好被移走就低報。
///
/// # trait 的生滅只從 `structural` 算一次
///
/// `trait_content` 的 `both`/`only_*` 是 **trait 集合**的性質,五個 leaf 必然
/// 相同(見 `DiffVector::aligned_signs` 的同一個道理);逐維相加會讓「新增一個
/// trait」被記五次。故四維只取各自的 `changed`,集合層級的生滅一律走
/// `structural`——第 3 步的歸戶規則保證每個生滅的 trait 都會出現在那裡。
#[derive(Debug, Clone)]
pub struct ExploratoryHeuristicV1 {
    pub weights: DimensionWeights,
}

impl ExploratoryHeuristicV1 {
    /// §6.1 的敘述「詞彙差異最傷互通,規則性音變其次」的一組**建議**係數。
    ///
    /// 這組數字沒有實證來源,是把那句話直譯成權重。要嚴肅使用請換掉。
    ///
    /// # `trait_rule` = 0.25 是怎麼來的(裁定 §4 的重新校準)
    ///
    /// 原本這組係數是在**規則變動恆為 0** 的前提下訂的——那時 `diff_vector`
    /// 只走 `signs`,一次音變之後互通度是 `1.0`。第 1–3 步把規則與 trait 內容
    /// 補進差異向量之後,它們第一次真的會壓低分數,係數必須重訂。
    ///
    /// 裁定 §4 給了一個**硬約束**:
    ///
    /// > 若加一條音變就讓互通度掉到預設閾值以下,則每演化一步就分裂一次
    /// > 方言群,分群功能等於報廢。
    ///
    /// 預設閾值是 `0.6`(`default_grouping_threshold`,桌面 UI 亦同)。把一條
    /// 波及全部 N 個詞的音變代進公式:
    ///
    /// ```text
    /// 相異度 = w_phon × ρ_rule × N / ((Σw + w_bd) × N)
    ///        = 2 × 0.25 / (7.5 + 3) = 0.0476
    /// 互通度 = 0.952
    /// ```
    ///
    /// 離 0.6 還有很大餘裕:同一個 changeset 裡 **8 條音變仍在同一個方言群、
    /// 第 9 條才切開**(`0.0476 × 9 > 0.4`;`grouping.rs` 的
    /// `the_calibration_margin_sits_between_eight_and_nine_sound_changes`
    /// 把這個邊界釘住)。一步一條音變的演化鏈永遠不會自己裂開,而一次改九條
    /// 音變的躍進被判成方言分化是合理的。
    ///
    /// `trait_content` = 0.5:改一個 trait 的分類或 slot,改的是那些詞**是
    /// 什麼**,比改它們怎麼唸更傷——但仍 < 1.0,因為那不等於換一個詞。
    ///
    /// 這兩個數字**沒有實證來源**,和其他係數一樣是可被反駁的主張。反駁的
    /// 方式是換掉它們,不需要改任何程式碼。
    ///
    /// # 已知的粗糙處:傷害以**規則節點**為單位
    ///
    /// 一個 P46 結構化 block(`rules:` / `Then:` / `Else:`)不論裝幾條語句都是
    /// **一個** `Rule` 節點,依 `RuleId` 對齊時算一個事件。故十二條語句塞進同
    /// 一個 block,傷害與一條相同。這是差異向量的葉節點粒度決定的(裁定 §1 的
    /// 階層到 `rules` 為止,不下探規則內部),不是這裡挑係數能補的——要補得先
    /// 讓 diff 數得出 block 內的語句。
    pub fn suggested() -> ExploratoryHeuristicV1 {
        ExploratoryHeuristicV1 {
            weights: DimensionWeights {
                sem: 3.0,
                phon: 2.0,
                syn: 1.0,
                prag: 0.5,
                structural: 1.0,
                birth_death: 3.0,
                trait_rule: 0.25,
                trait_content: 0.5,
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
            // 一個 trait 那側的 leaf 造成多少「詞的傷害」:事件數 × 波及的詞數
            // × 傷害比。reach 取前後 max(見型別說明)。
            let indirect = |leaf: &TraitDiff, events: usize, ratio: f64| -> f64 {
                ratio * events as f64 * leaf.reach_before.max(leaf.reach_after) as f64
            };

            let mut weighted = 0.0;
            let mut mass = 0.0;
            for (dim, weight) in per_dim {
                let layer = diff.dimension(dim);
                let rules = &layer.trait_rules;
                weighted += weight
                    * (layer.signs.changed as f64
                        // 規則依 `RuleId` 對齊,生滅是這一維自己的事,可以全算。
                        + indirect(
                            rules,
                            rules.counts.changed
                                + rules.counts.only_before
                                + rules.counts.only_after,
                            w.trait_rule,
                        )
                        // trait 內容只取 `changed`——生滅是 trait **集合**的性質,
                        // 五個 leaf 的數字相同,逐維相加會記五次。
                        + indirect(
                            &layer.trait_content,
                            layer.trait_content.counts.changed,
                            w.trait_content,
                        ));
                mass += weight * aligned as f64;
            }
            let structural_traits = &diff.structural.trait_content;
            weighted += w.structural
                * (diff.structural.signs.changed as f64
                    // trait 的生滅在這裡算,且只算這一次(第 3 步的歸戶規則
                    // 保證每個生滅的 trait 都會出現在 structural)。
                    + indirect(
                        structural_traits,
                        structural_traits.counts.changed
                            + structural_traits.counts.only_before
                            + structural_traits.counts.only_after,
                        w.trait_content,
                    ));
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
