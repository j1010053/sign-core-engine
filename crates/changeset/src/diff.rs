//! 步驟 16 ③ —— **分層差異向量**(docs/06 §6.1;docs/12 §3.2 的 `query::diff_vector`)。
//!
//! ## 層的清單依 docs/07,不依 docs/06 的舉例
//!
//! docs/06 §6.1 舉例說「音系距離、詞彙距離、型態距離、句法距離」,但同一句話也說
//! 「這直接依賴模組 C……**C 定義有哪些層**」。而 docs/07 §6 定得很明確:
//!
//! > **diff 統一在 sign 上**:因層是 sign 的投影,分層差異向量的每個分量都是
//! > 「**同一 sign 集合的某維 diff**」,**非七套異質函數**
//!
//! 故本模組**不是**四個異質度量,而是**一組 sign 上的四維投影 + 生滅**。
//!
//! 兩點說明:
//!
//! - **「型態」沒有獨立分量**。docs/07 §6 把型態視圖定義為「sign 組合運算在『詞內』
//!   尺度的呈現」——它是**組合的投影**,不是第五個維。強行給它一個分量會製造一個
//!   四維以外的本體。
//! - **`prag` 有分量**,雖然 §6.1 的舉例沒點名——四維是 phon/syn/sem/prag(docs/07 §1),
//!   漏掉它等於讓一整個維的改動在差異上隱形。
//!
//! ## 對齊鍵 = `SignId`
//!
//! docs/06 §6.1:「兩狀態的 sign 以 `SignId` 對齊……同 id = 同一 sign 的兩個演化階段;
//! **無對應者計為生/滅**」。這是合併(P61)與 `LanguageDiff` 用的**同一把鑰匙**——
//! 三處共用,不是三套。
//!
//! ## 度量:先數個數
//!
//! docs/07 §6 點名了更細的度量(phon = 形式編輯距離、sem = 義項網絡差異、
//! syn = trait/構式集合差異)。本刀**只數「有幾個 sign 在該維上不同」**:
//!
//! - 完全機械、決定性(P26),沒有任何需要調的權重;
//! - **不假裝有語言學上的「距離」**——編輯距離與網絡差異需要各自的度量設計,
//!   隨便湊一個數字比沒有更糟(它會被當真)。
//!
//! 更細的度量是【M+】。docs/06 §6.2 已經把互通度定為「**可替換函數**」,
//! 同一個精神:先定形狀與接口,度量之後換。

use crate::{def_dimension, LanguageDocument};
use conlang_language::{Dim, NodeId, SignDef, SignItem};
use std::collections::BTreeMap;

/// 分層差異向量(docs/06 §6.1)。
///
/// **不是單一數字**——規格明令差異是分層向量,把它壓成一個標量是互通度
/// (§6.2)的工作,且那是**可替換函數**,不屬於這一層。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffVector {
    /// 兩邊都有(以 `SignId` 對齊)的 sign 數——各維分量的分母。
    pub aligned: usize,
    /// 只在 `after` 有:生。
    pub born: usize,
    /// 只在 `before` 有:滅。
    pub died: usize,
    /// 對齊的 sign 中,該維投影**有差異**的個數。
    pub phon: usize,
    pub syn: usize,
    pub sem: usize,
    pub prag: usize,
    /// 不屬於任何維的 sign 層項目(`belongs` / trait 引用 / 無維 Def / 約束)有差異的個數。
    ///
    /// 獨立成一個分量而非塞進某一維:它們**跨維**(一個 `belongs` 同時影響 syn 與 sem
    /// 的繼承),硬歸給某一維會讓那一維的數字說謊。
    pub structural: usize,
}

impl DiffVector {
    /// 兩邊完全一致。
    pub fn is_identical(&self) -> bool {
        self.born == 0
            && self.died == 0
            && self.phon == 0
            && self.syn == 0
            && self.sem == 0
            && self.prag == 0
            && self.structural == 0
    }

    /// 某一維的分量。
    pub fn dimension(&self, dim: Dim) -> usize {
        match dim {
            Dim::Phon => self.phon,
            Dim::Syn => self.syn,
            Dim::Sem => self.sem,
            Dim::Prag => self.prag,
        }
    }
}

/// 算兩份文件的分層差異向量。
///
/// **對稱性**:`diff_vector(a, b)` 與 `diff_vector(b, a)` 的生滅互換、各維分量相同
/// ——docs/06 §6.2【M】的對稱版互通度即建立在此之上。
pub fn diff_vector(before: &LanguageDocument, after: &LanguageDocument) -> DiffVector {
    let old = signs_by_id(before);
    let new = signs_by_id(after);
    let mut vector = DiffVector::default();
    for (id, sign) in &old {
        match new.get(id) {
            None => vector.died += 1,
            Some(other) => {
                vector.aligned += 1;
                for dim in [Dim::Phon, Dim::Syn, Dim::Sem, Dim::Prag] {
                    if projection(sign, Some(dim)) != projection(other, Some(dim)) {
                        match dim {
                            Dim::Phon => vector.phon += 1,
                            Dim::Syn => vector.syn += 1,
                            Dim::Sem => vector.sem += 1,
                            Dim::Prag => vector.prag += 1,
                        }
                    }
                }
                if projection(sign, None) != projection(other, None) {
                    vector.structural += 1;
                }
            }
        }
    }
    vector.born = new.keys().filter(|id| !old.contains_key(*id)).count();
    vector
}

/// 一個 sign 在某維(或無維)上的投影。
///
/// 承 docs/07 §6「層是 sign 的投影」——投影就是**挑出屬於該維的項目**,不另建結構。
/// 項目是正規排序的(`item_group`),故同內容必然同序,直接比相等即可。
///
/// **比較必須對行號不敏感**:多數項目型別(`FeatureDecl`/`FeatureValue`/`Sense`/
/// `SenseEdge`/…)帶 `SourceLocation` 且參與 `PartialEq`,而 `Def` 不帶。若直接比
/// 整個項目,任何插入都會位移後續行號,使**內容未變的維度**被算成有差異——正是本檔
/// 開頭說的「那一維的數字說謊」。差異向量衡量的是內容,故比較前一律抹平位置。
fn projection(sign: &SignDef, dim: Option<Dim>) -> Vec<SignItem> {
    sign.items
        .iter()
        .filter(|item| item_dimension(item) == dim)
        .map(SignItem::without_source_location)
        .collect()
}

/// 一個 sign 項目屬於哪一維。`None` = 跨維的 sign 層項目。
///
/// 與正規排序用的 `item_group` **同一套歸屬**(那裡把維度編進排序鍵)——若兩處分頭
/// 判定,「排在 sem 區段」和「算進 sem 分量」會慢慢對不起來。
fn item_dimension(item: &SignItem) -> Option<Dim> {
    match item {
        SignItem::Belongs(_) | SignItem::TraitUse { .. } => None,
        SignItem::Constraint(_) | SignItem::SignExpression(_) => None,
        SignItem::Def(def) => def_dimension(&def.path),
        SignItem::Slot(_) | SignItem::SlotFeatureBinding(_) | SignItem::SlotMap(_) => {
            Some(Dim::Syn)
        }
        SignItem::FeatureDecl(value) => Some(value.dim),
        SignItem::FeatureValue(value) => Some(value.dim),
        SignItem::FeatureExpression(value) => Some(value.dim),
        SignItem::FeatureRule(rule) => Some(rule.dim),
        SignItem::RoleDecl(_) | SignItem::RoleBinding(_) | SignItem::RoleExpression(_) => {
            Some(Dim::Sem)
        }
        SignItem::Sense(_) | SignItem::SenseEdge(_) => Some(Dim::Sem),
        SignItem::Realization(_) => Some(Dim::Phon),
        SignItem::Rule(rule) => Some(rule.dim),
    }
}

fn signs_by_id(document: &LanguageDocument) -> BTreeMap<NodeId, &SignDef> {
    document
        .language()
        .signs
        .iter()
        .map(|sign| (sign.id.0.clone(), sign))
        .collect()
}
