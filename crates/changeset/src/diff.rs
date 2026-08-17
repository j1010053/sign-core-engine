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
use std::collections::{BTreeMap, BTreeSet};

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

/// trait 那一側的葉節點:**四個原始計數 + 影響範圍旁註**(裁定丙)。
///
/// # 為什麼 reach 不住在 [`DiffCounts`] 裡面
///
/// 「改了幾處」與「影響多廣」是**兩個問題**,壓成一個數字必然有一邊說謊
/// (§3.1)。裁定丙要求這一點**在型別上看得出來**:`reach_*` 與 `counts`
/// 同層並列,而不是 `DiffCounts` 的第五、第六個欄位——後者會讓逐欄位加總的
/// 呼叫端把波及面加進差異裡,那就退化成裁定乙(一次編輯灌出上百個計數)。
///
/// # reach 只描述**這個 leaf 記到的那些 trait**
///
/// 沒有事件的 trait 不進 reach:兩邊都沒動的 trait 有多少詞 `belongs` 它,
/// 跟這次的差異無關。故 `counts.is_identical()` 時 reach 恆為 `(0, 0)`。
///
/// 反過來,**零波及不等於沒改**:改一個沒人用的 trait,reach 是 0 而
/// `changed` 仍是 1。那正是丙相對於乙的全部意義。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TraitDiff {
    /// §2 的四元組。
    pub counts: DiffCounts,
    /// **前**狀態下,`belongs` 閉包涵蓋到本 leaf 這些 trait 的 sign 數。
    pub reach_before: usize,
    /// **後**狀態下同上。
    ///
    /// 發兩個數而不是一個:改一條 `belongs` 邊本身就會移動閉包,前後的波及面
    /// 不同;只發一個就得先挑「用哪一邊」,那又是一次隱形的合成選擇
    /// (§6.4 已裁定引擎不做那件事)。要 max、要平均,呼叫端自己決定。
    pub reach_after: usize,
}

impl TraitDiff {
    /// 這個葉節點上兩邊毫無差別。
    ///
    /// **只看 `counts`。** reach 是旁註不是分量——而且它本來就由 `counts` 的
    /// 事件集合決定,沒有事件就沒有 reach。
    pub fn is_identical(&self) -> bool {
        self.counts.is_identical()
    }
}

/// 一個維度**內部**的分層(裁定 ①)。
///
/// 外層維持 phon/syn/sem/prag 四維(§6.1 的「各一分量」不變),每一維內部
/// 再分子節點。**不把詞與規則壓進同一個整數**:`phon: 3` 若同時含「3 個詞
/// 變了」與「3 條規則變了」,那個數字就說不出自己是什麼。
///
/// # 子節點分的是「宣告處」,不是「維度化的容器」
///
/// 三個 leaf 的軸是**這段內容宣告在哪裡**:sign 上、trait 上、或 trait 上
/// 而且是規則。**不是**「這一維的 trait」——按 P38 v0.2,分類樹是**單一且
/// 維度中立**的,`TraitDef` 沒有維度欄位,沒有所謂 syn trait 或 sem trait。
/// 這裡的維度一律來自**項目自己**(與 sign 共用同一個 `item_dimension`);
/// sign 同樣不是 syn 或 sem,`signs` leaf 是一樣的投影動作。
///
/// 為什麼要分宣告處:改一條 trait 上的規則會經 `belongs` 閉包作用到一整片
/// 詞,改一個 sign 只動那一個詞。§6.1 明說詞彙差異與規則性音變的權重不同
/// ——分不開就沒得加權。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DimensionDiff {
    /// **sign 上宣告**的該維內容(依 `SignId` 對齊)。
    ///
    /// **sign 自帶的規則留在這裡**,不移到 `trait_rules`——掛在某個 sign 上的
    /// 規則是「那個詞的內容」,而 `trait_rules` 收的是經 `belongs` 閉包作用到
    /// 一整片詞的語言級規則。範圍不同,不該混在一個計數裡。
    pub signs: DiffCounts,
    /// **trait 上宣告**的該維內容,依 **trait 名**對齊。
    ///
    /// **不含規則**——規則另計於 `trait_rules`,否則同一條規則會在這裡與那裡
    /// 各記一次。
    pub trait_content: TraitDiff,
    /// **trait 上宣告的規則**,依 **`RuleId`**(P25 穩定 id)對齊。
    ///
    /// 這是 §0 那個 bug 的正主:一條音變宣告在 `global trait Core` 底下,
    /// 舊實作只走 `signs`,於是整個 diff 全零、互通度 1.0。
    pub trait_rules: TraitDiff,
}

impl DimensionDiff {
    /// 這一維底下所有子節點都無差異。
    pub fn is_identical(&self) -> bool {
        self.signs.is_identical()
            && self.trait_content.is_identical()
            && self.trait_rules.is_identical()
    }
}

/// 跨維分量的分層。軸與 [`DimensionDiff`] 相同:**宣告處**。
///
/// # 為什麼不共用 [`DimensionDiff`]
///
/// 因為它**沒有 `trait_rules`**,而那不是省略,是型別上的事實:`Rule::dim` 是
/// `Dim` 而非 `Option<Dim>`,每條規則必然屬於四維之一,**跨維的規則不存在**。
///
/// 若沿用 `DimensionDiff`,`structural.trait_rules` 會是一個永遠為零的欄位——
/// 而永遠為零的計數會被下游當成「這裡沒事發生」而加總、比較、畫進圖表。
/// 一個結構上不可能非零的數字,比沒有這個數字更糟。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StructuralDiff {
    /// **sign 上宣告**的無維項目(`belongs` / trait 引用 / 無維 Def / 約束)。
    pub signs: DiffCounts,
    /// **trait 上宣告**的無維項目——最重要的是 **trait 之間的 `belongs` 邊**。
    ///
    /// §0 的第二個例子就住在這裡:`trait Noun` 加一行 `belongs Animate`,
    /// 所有 `belongs Noun` 的詞其 role 約束、slot 授權、語意型別統統跟著變。
    ///
    /// **這才是 trait 作為分類節點的本體**(P38 v0.2:單一維度中立分類樹)。
    /// 四維裡的 `trait_content` 收的是 trait **攜帶**的內容,那些內容有維;
    /// 分類關係本身沒有,所以在這裡。
    pub trait_content: TraitDiff,
}

impl StructuralDiff {
    pub fn is_identical(&self) -> bool {
        self.signs.is_identical() && self.trait_content.is_identical()
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
    /// 跨維者(`belongs` / trait 引用 / 無維 Def / 約束)。
    ///
    /// 獨立成一個分量而非塞進某一維:它們**跨維**(一個 `belongs` 同時影響 syn 與 sem
    /// 的繼承),硬歸給某一維會讓那一維的數字說謊。
    pub structural: StructuralDiff,
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
    let sign_leaf = |changed: usize| DiffCounts {
        both,
        changed,
        only_before: died,
        only_after: born,
    };

    let closures = SignClosures {
        before: reach_index(before),
        after: reach_index(after),
    };
    let content = trait_content_counts(before, after, &closures);
    let rules = trait_rule_counts(before, after, &closures);

    DiffVector {
        phon: DimensionDiff {
            signs: sign_leaf(changed[0]),
            trait_content: content[0],
            trait_rules: rules[0],
        },
        syn: DimensionDiff {
            signs: sign_leaf(changed[1]),
            trait_content: content[1],
            trait_rules: rules[1],
        },
        sem: DimensionDiff {
            signs: sign_leaf(changed[2]),
            trait_content: content[2],
            trait_rules: rules[2],
        },
        prag: DimensionDiff {
            signs: sign_leaf(changed[3]),
            trait_content: content[3],
            trait_rules: rules[3],
        },
        structural: StructuralDiff {
            signs: sign_leaf(changed[4]),
            trait_content: content[4],
        },
    }
}

/// 一份文件裡每個 sign 的 **`belongs` 閉包**(裁定丙的 reach 就靠它)。
///
/// 用的是 [`OntologyRegistry`](conlang_language::ontology::OntologyRegistry)
/// ——閉包的菱形去重與循環安全已經在那裡解決過一次,這裡不另寫一套,否則兩邊
/// 對「涵蓋」的定義會慢慢分岔。診斷刻意丟棄:diff 不是驗證器,一份有問題的
/// 文件仍應算得出差異。
///
/// **只認文件自己宣告的 trait。** 套件裡的 trait 不在這棵樹上,故經由它們的
/// 邊算不進來——這不影響 reach 的正確性:reach 的目標集合同樣只來自本文件
/// (套件的 trait 不會 `belongs` 一個下游文件的 trait,依賴方向不允許)。
fn reach_index(document: &LanguageDocument) -> ReachIndex {
    let (registry, _) =
        conlang_language::ontology::OntologyRegistry::build(&[document.language()]);
    ReachIndex {
        closures: document
            .language()
            .signs
            .iter()
            .map(|sign| registry.sign_categories(sign).into_iter().collect())
            .collect(),
        globals: document
            .language()
            .traits
            .iter()
            .filter(|def| def.global)
            .map(|def| def.name.clone())
            .collect(),
    }
}

/// 涵蓋到 `targets` 任一個的 sign 數。
///
/// **取聯集大小,不是逐 trait 相加**:一個詞同時 `belongs` 兩個被改的 trait
/// 時,它是**一個**受影響的詞。相加會說成兩個,那正是裁定乙的膨脹病。
///
/// # `global` trait 涵蓋全部
///
/// `global trait` 是**預設自動引用**的 phon-rule macro(P6)——沒有任何詞需要
/// `belongs` 它,它就已經作用在整份語言上。只數 `belongs` 閉包會讓一條全語言
/// 音變的波及面報成 0,而那是本檔 §0 的第一個例子、最該被看見的那一種改動。
/// 一個結構上永遠是 0 的波及面比沒有這個數字更糟。
fn reach(index: &ReachIndex, targets: &BTreeSet<String>) -> usize {
    if targets.is_empty() {
        return 0;
    }
    if targets.iter().any(|name| index.globals.contains(name)) {
        return index.closures.len();
    }
    index
        .closures
        .iter()
        .filter(|categories| !categories.is_disjoint(targets))
        .count()
}

/// trait 上宣告的**非規則內容**差異,依 **trait 名**對齊,
/// 回傳 `[phon, syn, sem, prag, structural]`。
///
/// 第五格不是「跨維的 trait」——trait 本來就不分維(P38 v0.2)——而是
/// trait 上那些**沒有維度的項目**,主要是 `belongs`(分類關係本身)。
///
/// # 生滅的 trait 記在哪個 leaf(裁定丙的落地選擇)
///
/// `both`/`only_before`/`only_after` 是 **trait 集合**的性質,五個 leaf 必然
/// 相同(同 `signs` leaf,見 [`DiffVector::aligned_signs`])。但 reach 不能跟著
/// 五份都算——那會讓一個只帶 phon 內容的 trait 被刪掉時,連 sem 的波及面都
/// 記上一筆。
///
/// 故生滅的 trait 依**它的內容住在哪裡**歸戶:某維有內容就進該維,
/// 外加**一律進 structural**——因為一個分類節點的生滅,無論帶不帶內容,
/// 都是分類樹本身的改變。
///
/// # 為什麼用名字對齊
///
/// `TraitDef` 沒有 `SignId` 的等價物。名字在 catalog 層已強制唯一
/// (`ONTOLOGY_DUPLICATE_TRAIT`),故它是這一層唯一可用且穩定的鍵。
///
/// 代價要說清楚:**改名會被算成一生一滅**,而 sign 改名不會(sign 有 id)。
/// 這不是疏漏,是資料模型的既有事實——在 `TraitDef` 長出穩定 id 之前,沒有
/// 別的鍵可用,而假裝有(例如靠內容雜湊配對)會讓「改名」與「刪一個加一個」
/// 這兩件真的不同的事變得分不開。
fn trait_content_counts(
    before: &LanguageDocument,
    after: &LanguageDocument,
    closures: &SignClosures,
) -> [TraitDiff; 5] {
    let old = traits_by_name(before);
    let new = traits_by_name(after);

    let mut counts = [DiffCounts::default(); 5];
    // 每個 leaf 記到事件的 trait 名——reach 只看這些(見 [`TraitDiff`])。
    let mut touched: [BTreeSet<String>; 5] = Default::default();
    let both = old.keys().filter(|name| new.contains_key(*name)).count();
    let only_before = old.len() - both;
    let only_after = new.len() - both;

    for (name, old_trait) in &old {
        let Some(new_trait) = new.get(name) else {
            // 只在 before:滅。依內容所在的維歸戶,並一律進 structural。
            for slot in dims_with_content(old_trait) {
                touched[slot].insert((*name).to_owned());
            }
            continue;
        };
        for (slot, dim) in [Dim::Phon, Dim::Syn, Dim::Sem, Dim::Prag]
            .into_iter()
            .enumerate()
        {
            if trait_projection(old_trait, Some(dim)) != trait_projection(new_trait, Some(dim)) {
                counts[slot].changed += 1;
                touched[slot].insert((*name).to_owned());
            }
        }
        // 跨維:無維項目(`belongs` 等)**加上** `global` 旗標。
        // `global` 決定這個 trait 是不是自動引用的 phon-rule macro(P6),
        // 那是跨維的身分改變,不屬於任何單一維。
        if trait_projection(old_trait, None) != trait_projection(new_trait, None)
            || old_trait.global != new_trait.global
        {
            counts[4].changed += 1;
            touched[4].insert((*name).to_owned());
        }
    }
    for (name, new_trait) in &new {
        if old.contains_key(name) {
            continue;
        }
        // 只在 after:生。
        for slot in dims_with_content(new_trait) {
            touched[slot].insert((*name).to_owned());
        }
    }

    std::array::from_fn(|slot| {
        let mut leaf = counts[slot];
        leaf.both = both;
        leaf.only_before = only_before;
        leaf.only_after = only_after;
        TraitDiff {
            counts: leaf,
            reach_before: reach(&closures.before, &touched[slot]),
            reach_after: reach(&closures.after, &touched[slot]),
        }
    })
}

/// 一個 trait 的內容住在哪幾個 leaf:有內容的維,**外加一律進 structural**。
///
/// 生滅的 trait 靠這個歸戶(見 [`trait_content_counts`] 的說明)。規則不算
/// ——它們在 `trait_rules` 那邊自有 `RuleId` 對齊,由 [`trait_rule_counts`]
/// 歸戶。
fn dims_with_content(def: &conlang_language::TraitDef) -> Vec<usize> {
    let mut slots: Vec<usize> = [Dim::Phon, Dim::Syn, Dim::Sem, Dim::Prag]
        .into_iter()
        .enumerate()
        .filter(|(_, dim)| !trait_projection(def, Some(*dim)).is_empty())
        .map(|(slot, _)| slot)
        .collect();
    slots.push(4);
    slots
}

/// trait 宣告的規則,依 `RuleId` 對齊,回傳 `[phon, syn, sem, prag]`。
///
/// 規則必然有維(`Rule::dim` 是 `Dim` 不是 `Option<Dim>`),故沒有第五格
/// ——見 [`StructuralDiff`] 為何不共用 [`DimensionDiff`]。
///
/// 每條規則按**自己的維**歸戶,而不是按所在區塊——兩者在合法文件裡一致,
/// 但以規則自帶的 `dim` 為準才不會在文件不合法時默默錯歸。
/// reach 歸戶到**宣告這條規則的 trait**:規則沒有自己的 `belongs` 閉包,
/// 它經由所在的 trait 作用到詞上。
fn trait_rule_counts(
    before: &LanguageDocument,
    after: &LanguageDocument,
    closures: &SignClosures,
) -> [TraitDiff; 4] {
    let old = trait_rules_by_id(before);
    let new = trait_rules_by_id(after);

    let mut counts = [DiffCounts::default(); 4];
    let mut touched: [BTreeSet<String>; 4] = Default::default();
    let slot_of = |dim: Dim| match dim {
        Dim::Phon => 0,
        Dim::Syn => 1,
        Dim::Sem => 2,
        Dim::Prag => 3,
    };

    for (id, entry) in &old {
        match new.get(id) {
            None => {
                counts[slot_of(entry.dim)].only_before += 1;
                touched[slot_of(entry.dim)].insert(entry.declared_in.clone());
            }
            Some(other) => {
                counts[slot_of(entry.dim)].both += 1;
                if entry.item != other.item {
                    counts[slot_of(entry.dim)].changed += 1;
                    // 前後都記:規則可能**連同它的 trait 一起搬家**,那樣兩邊
                    // 的波及面本來就不同,而 reach 發兩個數正是為了讓它現形。
                    touched[slot_of(entry.dim)].insert(entry.declared_in.clone());
                    touched[slot_of(other.dim)].insert(other.declared_in.clone());
                }
            }
        }
    }
    for (id, entry) in &new {
        if !old.contains_key(id) {
            counts[slot_of(entry.dim)].only_after += 1;
            touched[slot_of(entry.dim)].insert(entry.declared_in.clone());
        }
    }

    std::array::from_fn(|slot| TraitDiff {
        counts: counts[slot],
        reach_before: reach(&closures.before, &touched[slot]),
        reach_after: reach(&closures.after, &touched[slot]),
    })
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
        SignItem::Pass => None,
        SignItem::TraitMount { name: _, kind: conlang_language::TraitMountKind::Declaration } | SignItem::TraitMount { kind: conlang_language::TraitMountKind::Whole | conlang_language::TraitMountKind::Block(_), .. } => None,
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

/// 一個 trait 在某維(或無維)上的投影,**排除規則**。
///
/// 規則另計於 [`DimensionDiff::trait_rules`](依 `RuleId` 精確對齊)。若這裡也
/// 含規則,同一條規則會被記兩次——`trait_content` 一次、`trait_rules` 一次
/// ——而那兩個數字之後很可能被相加。
///
/// 與 sign 的投影同樣抹平 `SourceLocation`:行號是來源註記,不是內容。
fn trait_projection(def: &conlang_language::TraitDef, dim: Option<Dim>) -> Vec<SignItem> {
    def.blocks
        .iter()
        .flat_map(|block| block.items.iter())
        .filter(|item| !is_rule(item))
        .filter(|item| item_dimension(item) == dim)
        .map(SignItem::without_source_location)
        .collect()
}

/// 帶 `Rule` 酬載的項目——`trait_rules` leaf 收的就是這些。
///
/// 兩個變體都算:`Rule` 是一般規則,`FeatureRule` 是特徵預設規則,兩者都帶
/// P25 的穩定 `RuleId`,都能被精確對齊。
fn is_rule(item: &SignItem) -> bool {
    matches!(item, SignItem::Rule(_) | SignItem::FeatureRule(_))
}

/// trait 內宣告的規則,依 `RuleId` 索引,值是 `(維度, 抹平位置後的項目)`。
///
/// **只收 trait 的規則,不收 sign 的。** 掛在某個 sign 上的規則是那個詞的內容,
/// 已計於 `signs`;trait 宣告的規則經 `belongs` 閉包作用到一整片詞,是語言級的。
/// 範圍不同,混在一起就分不出「改了一個詞」與「改了一條全語言的音變」。
fn trait_rules_by_id(
    document: &LanguageDocument,
) -> BTreeMap<conlang_language::RuleId, TraitRuleEntry> {
    let mut out = BTreeMap::new();
    for def in &document.language().traits {
        for block in &def.blocks {
            for item in &block.items {
                let (SignItem::Rule(rule) | SignItem::FeatureRule(rule)) = item else {
                    continue;
                };
                out.insert(
                    rule.id.clone(),
                    TraitRuleEntry {
                        dim: rule.dim,
                        declared_in: def.name.clone(),
                        item: SignItem::without_source_location(item),
                    },
                );
            }
        }
    }
    out
}

/// 一條 trait 規則的索引條目。
struct TraitRuleEntry {
    dim: Dim,
    /// 宣告它的 trait 名——reach 要靠它找出這條規則作用到哪些詞。
    declared_in: String,
    /// 抹平位置後的項目,供內容比較。
    item: SignItem,
}

/// 一份文件算 reach 需要的東西:每個 sign 的 `belongs` 閉包 + global trait 名單。
struct ReachIndex {
    closures: Vec<BTreeSet<String>>,
    globals: BTreeSet<String>,
}

/// 兩份文件各自的 reach 索引,算一次共用。
///
/// 九個 leaf 各要算一次 reach,而索引只跟文件有關、與 leaf 無關——放進來
/// 就不會被算九遍。
struct SignClosures {
    before: ReachIndex,
    after: ReachIndex,
}

fn traits_by_name(document: &LanguageDocument) -> BTreeMap<&str, &conlang_language::TraitDef> {
    document
        .language()
        .traits
        .iter()
        .map(|def| (def.name.as_str(), def))
        .collect()
}

fn signs_by_id(document: &LanguageDocument) -> BTreeMap<NodeId, &SignDef> {
    document
        .language()
        .signs
        .iter()
        .map(|sign| (sign.id.0.clone(), sign))
        .collect()
}
