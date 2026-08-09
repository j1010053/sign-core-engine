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

/// 一個葉節點的**四個原始計數**(裁定 ②)。
///
/// # 為什麼是四個原始數,而不是一個比例
///
/// 引擎**不挑分母**。「以對齊數為分母」這個框架本來就對生滅無效——新增的詞
/// 不跟任何東西對齊;而 §6.4 已裁定**引擎不定義評分合成公式**,挑一個分母
/// 就是挑一個公式。
///
/// 呼叫端要什麼自己算,這四個數足夠:
///
/// ```text
/// 比例式  = changed / both
/// Jaccard = (changed + only_before + only_after) / (both + only_before + only_after)
/// 總數    = both + only_before(前)  /  both + only_after(後)
/// ```
///
/// 所以這裡**刻意不提供**任何比例方法——提供了就等於引擎在背書一個公式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffCounts {
    /// 兩邊都有(依 id 對齊)。
    pub both: usize,
    /// `both` 之中內容不同的。
    pub changed: usize,
    /// 只在 `before`:滅。
    pub only_before: usize,
    /// 只在 `after`:生。
    pub only_after: usize,
}

impl DiffCounts {
    /// 這個葉節點上兩邊毫無差別。
    ///
    /// `both` 刻意不參與判斷:兩邊都有 100 個一模一樣的東西,差異仍是零。
    pub fn is_identical(&self) -> bool {
        self.changed == 0 && self.only_before == 0 && self.only_after == 0
    }
}

/// 一個維度**內部**的分層(裁定 ①)。
///
/// 外層維持 phon/syn/sem/prag 四維(§6.1 的「各一分量」不變),每一維內部
/// 再分子節點。**不把詞與規則壓進同一個整數**:`phon: 3` 若同時含「3 個詞
/// 變了」與「3 條規則變了」,那個數字就說不出自己是什麼。
///
/// 子節點**按需長出**,不預先造滿(依《共時lang語法與資料貼合度》
/// 「不先造無消費者語法」)。目前只有 `signs`;`rules` 與 trait 容器是下一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DimensionDiff {
    /// 詞在該維上的投影差異。
    pub signs: DiffCounts,
}

impl DimensionDiff {
    /// 這一維底下所有子節點都無差異。
    pub fn is_identical(&self) -> bool {
        self.signs.is_identical()
    }
}

/// 分層差異向量(docs/06 §6.1)。
///
/// **不是單一數字**——規格明令差異是分層向量,把它壓成一個標量是互通度
/// (§6.2)的工作,且那是**可替換函數**,不屬於這一層。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffVector {
    pub phon: DimensionDiff,
    pub syn: DimensionDiff,
    pub sem: DimensionDiff,
    pub prag: DimensionDiff,
    /// 不屬於任何維的 sign 層項目(`belongs` / trait 引用 / 無維 Def / 約束)。
    ///
    /// 獨立成一個分量而非塞進某一維:它們**跨維**(一個 `belongs` 同時影響 syn 與 sem
    /// 的繼承),硬歸給某一維會讓那一維的數字說謊。
    pub structural: DimensionDiff,
}

impl DiffVector {
    /// 兩邊完全一致。
    pub fn is_identical(&self) -> bool {
        self.phon.is_identical()
            && self.syn.is_identical()
            && self.sem.is_identical()
            && self.prag.is_identical()
            && self.structural.is_identical()
    }

    /// 某一維的分層。
    pub fn dimension(&self, dim: Dim) -> &DimensionDiff {
        match dim {
            Dim::Phon => &self.phon,
            Dim::Syn => &self.syn,
            Dim::Sem => &self.sem,
            Dim::Prag => &self.prag,
        }
    }

    /// 兩邊都有的 sign 數。
    ///
    /// # 為什麼要有這組 accessor
    ///
    /// sign 集合怎麼對齊是**集合的性質,不是維的性質**——五個 leaf 的
    /// `both`/`only_before`/`only_after` 必然相同,只有 `changed` 因維而異。
    /// 每個 leaf 仍各自帶滿四個數是為了**介面一致**(呼叫端逐 leaf 迭代時
    /// 不必特例),但要讀「對齊了幾個 sign」時應走這裡,而不是隨手挑
    /// `.phon.signs.both`——那會讓「為什麼是 phon 不是 syn」變成一個沒有
    /// 答案的問題。
    pub fn aligned_signs(&self) -> usize {
        self.phon.signs.both
    }

    /// 只在 `after` 有的 sign 數:生。
    pub fn born_signs(&self) -> usize {
        self.phon.signs.only_after
    }

    /// 只在 `before` 有的 sign 數:滅。
    pub fn died_signs(&self) -> usize {
        self.phon.signs.only_before
    }
}

/// 算兩份文件的分層差異向量。
///
/// **對稱性**:`diff_vector(a, b)` 與 `diff_vector(b, a)` 的生滅互換、各維分量相同
/// ——docs/06 §6.2【M】的對稱版互通度即建立在此之上。
pub fn diff_vector(before: &LanguageDocument, after: &LanguageDocument) -> DiffVector {
    let old = signs_by_id(before);
    let new = signs_by_id(after);

    let mut both = 0usize;
    let mut changed = [0usize; 5]; // phon, syn, sem, prag, structural
    let mut died = 0usize;

    for (id, sign) in &old {
        match new.get(id) {
            None => died += 1,
            Some(other) => {
                both += 1;
                for (slot, dim) in [Dim::Phon, Dim::Syn, Dim::Sem, Dim::Prag]
                    .into_iter()
                    .enumerate()
                {
                    if projection(sign, Some(dim)) != projection(other, Some(dim)) {
                        changed[slot] += 1;
                    }
                }
                if projection(sign, None) != projection(other, None) {
                    changed[4] += 1;
                }
            }
        }
    }
    let born = new.keys().filter(|id| !old.contains_key(*id)).count();

    // sign 集合的對齊結果對五個 leaf 都一樣(見 `DiffVector::aligned_signs`)
    // ——只有 `changed` 因維而異。集中在這裡組裝,duplication 就是 derived 的,
    // 不會各自漂移。
    let leaf = |changed: usize| DimensionDiff {
        signs: DiffCounts {
            both,
            changed,
            only_before: died,
            only_after: born,
        },
    };
    DiffVector {
        phon: leaf(changed[0]),
        syn: leaf(changed[1]),
        sem: leaf(changed[2]),
        prag: leaf(changed[3]),
        structural: leaf(changed[4]),
    }
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
