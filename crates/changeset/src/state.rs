//! State — **演化發生時的外部環境**(修補04 §State;鳥瞰步驟 20)。
//!
//! 年代 / 地理 / 社會 / 語言接觸。**不屬 Language**,不參與共時 Grammar、
//! Compile 或 Grammar Engine。
//!
//! # 定位裁定 (A):State 只在**撰寫時**被讀,replay 不看它
//!
//! 規格原文說 State「使**相同的 ChangeSet 能在不同環境下產生不同的演化結果**」。
//! 那與現行 replay 契約**直接衝突**:`.chg` 有三道 digest
//! (base source / identity-manifest / library lock),P26 要求 replay 逐位元可重現;
//! 若 State 能改變同一份 ChangeSet 的結果,replay 就依賴一個不在任何 digest 裡的東西。
//!
//! 程式碼其實早已在結構上選了 (A):`.chg` 的 function 在 **resolve 時展開**
//! (而 replay = parse + resolve + run),但 `choose:` 在該處被**明確擋下**
//! (`CandidatesRequireSelection`)——**候選列舉不得發生在 replay 裡**。
//! 同理 `select_goal_candidate` 全庫只有測試呼叫,`src/` 零呼叫點:它是撰寫時工具。
//!
//! 故裁定(擁有者 2026-08-04):
//!
//! - State 是**撰寫時**的環境輸入 —— Goal / Generator 讀它來塑形候選與權重
//!   (見 `conlang_generate::ContactInfluence`);
//! - 選定結果寫死進 `.chg`;**replay 永不讀 State**;
//! - 故 State **雜湊外**、可自由編輯、改了不影響任何既有節點的重放產物。
//!
//! 這與 `GoalSelectionTrace` 記 `seed`/`ordered` 而非重抽是同一個模式:
//! **記錄決策依據,但不讓 replay 重新決策**。
//!
//! # 欄位取最小集合
//!
//! 規格寫「包括但不限於」。此處**不設自由擴充 map**——《共時lang語法與資料
//! 貼合度》明文「不先造無消費者語法」,且 P71 才剛清掉一個自由欄位逃生口
//! (裸 Def → 封閉清單),不該在 State 立刻開一個新的。加欄位是 additive
//! (serde `default`),等有實際需求再長。

use serde::{Deserialize, Serialize};

/// 一個演化節點所處的外部環境。**雜湊外**,不進 `manifest`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionState {
    /// 年代。**自由字串**:演化圖的 parent 邊已經表達先後,絕對年代只是給人看的
    /// 標註;若做成結構化型別,就得處理「A 是 B 的祖先卻標了較晚年代」這種
    /// 與圖拓撲矛盾的情形,而那在 (A) 之下沒有任何運算依賴它,不值得。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// 地理與區域。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// 社會環境(階級、群體)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub society: Vec<String>,
    /// 語言接觸。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contacts: Vec<Contact>,
}

/// 一段語言接觸的**環境事實**。
///
/// **不記錄「借了哪些詞」**——那已由 `AtomicRewrite::Adopt{donor, source}`、
/// sign 的 `provenance = loan` 與 `origin` 承載,重複儲存會違反單一資訊源。
/// 此處記的是「與誰、何時、多密切」,用來解釋為什麼會有那些借詞。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    /// 對方語言。**自由字串**,刻意不指向演化節點——接觸對象常是本專案外的
    /// 真實語言或未建模的族群;綁成節點引用會逼使用者為了記一筆接觸而先造一個節點。
    pub counterpart: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    pub intensity: ContactIntensity,
}

/// 接觸強度。**封閉枚舉**——這是要餵進權重計算的東西,自由字串會讓
/// 「打錯字 → 靜默當成無接觸」重演 P71 修掉的那個形態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactIntensity {
    /// 零星往來。
    Sporadic,
    /// 貿易接觸。
    Trade,
    /// 廣泛雙語。
    Bilingual,
    /// 語言取代壓力。
    Dominant,
}

impl ContactIntensity {
    /// 強度的**預設**係數,供權重推導使用。
    ///
    /// 這是本模組唯一一處帶語言學判斷的數字,故明確標為**預設**:
    /// 「接觸多密切 ⇒ 借詞音素該加權多少」是語言學判斷,不是引擎事實。
    /// 要換一套,寫自己的 `DistributionProvider` 即可——引擎不強迫這組係數
    /// (同 §6.4:引擎不定義評分合成公式)。
    pub fn default_factor(self) -> f64 {
        match self {
            ContactIntensity::Sporadic => 0.1,
            ContactIntensity::Trade => 0.3,
            ContactIntensity::Bilingual => 0.6,
            ContactIntensity::Dominant => 1.0,
        }
    }
}

impl EvolutionState {
    pub fn is_empty(&self) -> bool {
        self.time.is_none()
            && self.region.is_none()
            && self.society.is_empty()
            && self.contacts.is_empty()
    }
}
