//! Ontology registry + `belongs` 閉包(步驟 12a;修補07 P38 **v0.2:單一分類樹**)。
//!
//! **一棵維度中立的分類樹**(owner 裁決 2026-07-20):`trait` 是分類節點,`belongs`
//! (P40)構成單一繼承網絡(CxG 單一 constructicon)。**無 `syn trait` 維度標記**——
//! syn/phon/sem/prag 是 sign/trait 身上的**內容面向**(Def 區塊),由 [`crate::projection`]
//! 按維度讀取;分類本身不分維。registry 自一組 [`Language`] 建成;**最小本體為額外
//! 引用的 stdlib `.lang`**(I20;見 [`std_ontology`])。
//!
//! `belongs` 三義(P40):成員 + 展開來源 + 保留標記。本模組管**成員 + 標記**(閉包);
//! **展開**(繼承 Def 的有效值)由 projection 於讀取時解析(P39 Defs 單一源,不複製)。
//! 診斷分級回報、不 panic(承 B9/P26)。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::diagnostic::{Diagnostic, DiagnosticSource, Severity, SourceLocation, ValidationReport};
use crate::{Dim, Language, SignDef, SignItem, Slot, Stage, TraitDef};

/// ontology 建構期診斷(全為 error 級;分級資料,不 panic)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntologyDiag {
    /// `belongs` 目標不是任何已宣告 trait。
    UnknownTrait { referrer: String, target: String },
    /// `belongs` 邊成環。
    Cycle { path: Vec<String> },
    /// 重複定義的 trait 名。
    DuplicateTrait { name: String },
}

/// 一個分類節點。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntoNode {
    /// 直接父邊(`belongs` 目標,保序去重)。
    pub parents: Vec<String>,
    /// 節點自帶 Defs(全維度,含 dim 前綴路徑;繼承給後代,projection 按維讀)。
    pub defs: Vec<(String, String)>,
    /// Inheritable trait body in source order. Classification edges are held
    /// in `parents`; macro uses are compile-time only.
    pub items: Vec<SignItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitProvenance {
    pub trait_name: String,
    /// `0` is a directly named `belongs`, `1` its parent, and so on.
    pub distance: usize,
    /// Source-order precedence token. Larger wins at equal distance.
    pub precedence: usize,
}

/// 值項的合併鍵:泛型 `Def` 用路徑,`FeatureValue` 用 `dim.name`——兩者共用同一個
/// 有效值命名空間(本地泛型賦值可以蓋掉繼承來的 typed 賦值,反之亦然)。
fn value_key(item: &SignItem) -> String {
    match item {
        SignItem::Def(def) => def.path.clone(),
        SignItem::FeatureValue(feature) => {
            format!("{}.{}", feature.dim.keyword(), feature.name)
        }
        _ => unreachable!("effective value stream contains only values"),
    }
}

/// 把一個項目併進累積器。
///
/// `sibling = true` 表示來源是**並列掛載的另一個包**:此時同路徑的兩個
/// `FeatureValue` 取候選聯集(未定案),而不是挑一個贏家。`false` 表示來源是
/// 本層自己寫的值,直接取代(P6:sign 顯式 / 本層宣告最高)。
///
/// 泛型 `Def` 沒有候選集合可以裝未定案,並列分歧時仍是後者勝——那條路上既有的
/// `ONTOLOGY_DEF_CONFLICT_RESOLVED` warning 會報出來。
fn merge_value(
    acc: &mut BTreeMap<String, (usize, SignItem)>,
    counter: &mut usize,
    item: SignItem,
    sibling: bool,
) {
    let key = value_key(&item);
    *counter += 1;
    let index = *counter;
    match (acc.get_mut(&key), &item) {
        (Some((_, SignItem::FeatureValue(kept))), SignItem::FeatureValue(incoming))
            if sibling =>
        {
            for candidate in &incoming.values {
                if !kept.values.contains(candidate) {
                    kept.values.push(candidate.clone());
                }
            }
        }
        _ => {
            acc.insert(key, (index, item));
        }
    }
}

/// 依插入序攤平累積器。
fn flatten_values(acc: BTreeMap<String, (usize, SignItem)>) -> Vec<SignItem> {
    let mut out: Vec<_> = acc.into_values().collect();
    out.sort_by_key(|(index, _)| *index);
    out.into_iter().map(|(_, item)| item).collect()
}

/// 單一分類樹 registry(P38 v0.2)。
#[derive(Debug, Clone, Default)]
pub struct OntologyRegistry {
    tree: BTreeMap<String, OntoNode>,
    /// 名字 → **匯出它但未被載入**的 package(R13)。
    ///
    /// 只在「名字查無」時才用得上,故凡命中必然是尚未宣告的套件。空的時候
    /// 一切照舊——不強制呼叫端提供 catalog。
    available: BTreeMap<String, crate::LibraryId>,
}

fn trait_belongs(t: &TraitDef) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for b in &t.blocks {
        for it in &b.items {
            if let SignItem::TraitMount { name: name, kind: crate::TraitMountKind::Declaration, .. } = it {
                if seen.insert(name.clone()) {
                    out.push(name.clone());
                }
            }
        }
    }
    out
}

fn trait_defs(t: &TraitDef) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for b in &t.blocks {
        for it in &b.items {
            if let SignItem::Def(d) = it {
                out.push((d.path.clone(), d.value.clone()));
            }
        }
    }
    out
}

fn trait_items(t: &TraitDef) -> Vec<SignItem> {
    t.blocks
        .iter()
        .flat_map(|block| block.items.iter())
        .filter(|item| !matches!(item, SignItem::TraitMount { name: _, kind: crate::TraitMountKind::Declaration, .. } | SignItem::TraitMount { kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_), .. }))
        .cloned()
        .collect()
}


/// `pass` / 空塊 / `marker` 三條塊形狀規則。
///
/// - **空塊發警告**(不是錯誤):空塊一直是合法的,問題是它**啞**——看不出是刻意
///   留白還是寫到一半。寫 `pass` 就閉嘴。用警告而非錯誤,是因為既有檔案裡的空塊
///   是合法且 round-trip 穩定的,一刀變錯誤會讓它們全部失效;而 B9 本來就有分級
///   診斷,「該說但不該擋」正是警告的用途。
/// - **`pass` 與內容互斥**:兩者同時出現代表作者自相矛盾,那是錯誤。
/// - **`marker trait` 不得有內容**:它承諾的就是這件事,不強制就只是註解。
///   marker 的塊當然是空的,故**豁免空塊警告**——宣告行已經說明理由了。
/// [A] 第 1 步:**`belongs X` 卻沒有引用 X 的內容**。
///
/// 兩階段設計裡 `belongs X` 是**宣告本 sign 使用哪一種 trait**(身分與實參),
/// `X[n]` 才是內容的落點。今天 `belongs` 仍然經投影供給內容,所以這條只發
/// **警告**——行為零改變,它的用途是把遷移清單列出來。
///
/// 第 3 步會把投影關掉、警告升為錯誤;在那之前,寫了 `belongs` 而沒引用的地方
/// 仍然拿得到內容,只是拿的方式即將改變。
///
/// # 豁免:沒有內容可引用的 trait
///
/// `marker trait`(承諾永不帶內容)與**所有塊都空**的 trait 都不要求引用
/// ——強迫寫一行引用進來一個空集合是純噪音。空塊那側自有
/// `BLOCK_EMPTY_WITHOUT_PASS` 在說話,不必在這裡重複。
///
/// # 為什麼「部分引用」不在這裡
///
/// 少寫一塊已經是 `IncompleteTraitUse`(compile error,P5 全 block 完整性);
/// 這條只管**一塊都沒寫**的情形。
///
/// # 必須餵 ① Source
///
/// 展開(`expand_traits`)會把 `TraitUse` **消去**——餵展開後的語言,引用一律
/// 看不見,於是每一個 `belongs` 都會誤報。故呼叫點在 `system.rs` 拿得到
/// `effective_source` 的地方,不掛在 `validation_report` 裡(那裡兩條路徑
/// 餵的東西不同,一條是 Source、一條是 ④ Ordered)。
pub(crate) fn belongs_reference_diagnostics(langs: &[&Language]) -> Vec<Diagnostic> {
    let mut content_bearing: BTreeMap<&str, bool> = BTreeMap::new();
    for lang in langs {
        for def in &lang.traits {
            let has_content = !def.marker
                && def
                    .blocks
                    .iter()
                    .any(|block| block.items.iter().any(|item| !matches!(item, SignItem::Pass)));
            content_bearing.insert(def.name.as_str(), has_content);
        }
    }

    let mut out = Vec::new();
    let mut check = |owner: &str, items: &[SignItem]| {
        let used: BTreeSet<&str> = items
            .iter()
            .filter_map(|item| match item {
                SignItem::TraitMount { name, kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_), .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        let declared: BTreeSet<&str> = items
            .iter()
            .filter_map(|item| match item {
                SignItem::TraitMount { name: name, kind: crate::TraitMountKind::Declaration, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        // **`X[n]` 不得獨立出現。** 身分(以及日後 P76 的實參)只存在 `belongs`
        // 上一份,`X[n]` 是指向它的展開點——沒有那一份宣告,展開點就沒有可指的
        // 東西,而它自己帶的名字只是個巧合般的重複。
        //
        // **暫為警告。** 這條的終點是把 `Belongs` 與 `TraitUse` 併成同一個
        // `SignItem`(`block: None` = 宣告、`Some(n)` = 展開點),屆時它會變成
        // **型別的良構條件**而不是外掛的檢查。在那之前發錯誤會擋掉兩個既有
        // 用法(`.chg` 獨立改 trait_use 目標、結構化編輯的「插入 trait use」),
        // 而那兩個正是合併時要一起修的東西——現在擋住只會逼出臨時修法。
        for name in &used {
            if !declared.contains(name) {
                out.push(Diagnostic::new(
                    Severity::Warning,
                    "TRAIT_USE_WITHOUT_BELONGS",
                    format!(
                        "{owner:?} expands {name}[…] without declaring `belongs {name}`; \
                         the mount is declared by `belongs`, and an expansion point cannot \
                         stand on its own"
                    ),
                ));
            }
        }
        for item in items {
            let SignItem::TraitMount { name: target, kind: crate::TraitMountKind::Declaration, .. } = item else {
                continue;
            };
            if used.contains(target.as_str()) {
                continue;
            }
            // 未知 trait 由 `ONTOLOGY_UNKNOWN_TRAIT` 負責;這裡不重複報。
            if content_bearing.get(target.as_str()).copied().unwrap_or(false) {
                out.push(Diagnostic::new(
                    Severity::Warning,
                    "BELONGS_WITHOUT_REFERENCE",
                    format!(
                        "{owner:?} declares `belongs {target}` but never references {target}'s \
                         blocks; content still arrives by projection today, but the two-phase \
                         design makes `{target}[n]` the only content channel",
                    ),
                ));
            }
        }
    };

    for lang in langs {
        for sign in &lang.signs {
            check(&sign.name, &sign.items);
        }
        for def in &lang.traits {
            // trait 的項目住在 block 裡,而 `belongs` 與引用可以分屬不同塊
            // ——身分是容器層的事實,故合起來看。
            let items: Vec<SignItem> = def
                .blocks
                .iter()
                .flat_map(|block| block.items.iter().cloned())
                .collect();
            check(&def.name, &items);
        }
    }
    out
}

fn block_shape_diagnostics(langs: &[&Language]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for lang in langs {
        for def in &lang.traits {
            // P76:marker trait 不得有型別參數
            if def.marker && !def.type_params.is_empty() {
                out.push(Diagnostic::new(
                    Severity::Error,
                    "TYPE_PARAM_ON_MARKER_TRAIT",
                    format!(
                        "marker trait {:?} 不得有型別參數(marker 無內容可供替換)",
                        def.name
                    ),
                ));
            }
            // P76:型別參數不得重名
            {
                let mut seen = std::collections::BTreeSet::new();
                for p in &def.type_params {
                    if !seen.insert(&p.name) {
                        out.push(Diagnostic::new(
                            Severity::Error,
                            "TYPE_PARAM_DUPLICATE_NAME",
                            format!(
                                "trait {:?} 的型別參數 {:?} 重複宣告",
                                def.name, p.name
                            ),
                        ));
                    }
                }
            }
            // P76:global trait 不得有型別參數
            if def.global && !def.type_params.is_empty() {
                out.push(Diagnostic::new(
                    Severity::Error,
                    "TYPE_PARAM_ON_GLOBAL_TRAIT",
                    format!(
                        "global trait {:?} 不得有型別參數(global trait 自動引用,無法自動填實參)",
                        def.name
                    ),
                ));
            }
            for (index, block) in def.blocks.iter().enumerate() {
                let passes = block
                    .items
                    .iter()
                    .filter(|item| matches!(item, SignItem::Pass))
                    .count();
                let others = block.items.len() - passes;
                if def.marker && others > 0 {
                    out.push(Diagnostic::new(
                        Severity::Error,
                        "TRAIT_MARKER_HAS_CONTENT",
                        format!(
                            "marker trait {:?} 不得有內容(block {index} 有 {others} 項);\
                             要帶內容請改成一般 `trait`",
                            def.name
                        ),
                    ));
                }
                if passes > 0 && others > 0 {
                    out.push(Diagnostic::new(
                        Severity::Error,
                        "BLOCK_PASS_WITH_CONTENT",
                        format!(
                            "{:?} 的 block {index} 同時有 `pass` 與內容;`pass` 的意思是這一塊\
                             故意留白",
                            def.name
                        ),
                    ));
                }
                if block.items.is_empty() && !def.marker {
                    out.push(Diagnostic::new(
                        Severity::Warning,
                        "BLOCK_EMPTY_WITHOUT_PASS",
                        format!(
                            "{:?} 的 block {index} 是空的;若是刻意留白請寫 `pass`,\
                             若這個 trait 純粹是分類節點請改宣告為 `marker trait`",
                            def.name
                        ),
                    ));
                }
            }
        }
    }
    out
}

impl OntologyRegistry {
    /// 自一組 Language 建 registry(順序 = 覆蓋層:stdlib 在前,使用者在後)。
    pub fn build(langs: &[&Language]) -> (OntologyRegistry, Vec<OntologyDiag>) {
        let mut reg = OntologyRegistry::default();
        let mut diags = Vec::new();

        for lang in langs {
            for t in &lang.traits {
                if reg.tree.contains_key(&t.name) {
                    diags.push(OntologyDiag::DuplicateTrait {
                        name: t.name.clone(),
                    });
                    continue;
                }
                reg.tree.insert(
                    t.name.clone(),
                    OntoNode {
                        parents: trait_belongs(t),
                        defs: trait_defs(t),
                        items: trait_items(t),
                    },
                );
            }
        }

        // belongs 目標須存在
        for (name, node) in &reg.tree {
            for parent in &node.parents {
                if !reg.tree.contains_key(parent) {
                    diags.push(OntologyDiag::UnknownTrait {
                        referrer: name.clone(),
                        target: parent.clone(),
                    });
                }
            }
        }
        // sign 的 belongs 目標須存在(不靜默丟分類)
        for lang in langs {
            for s in &lang.signs {
                for it in &s.items {
                    if let SignItem::TraitMount { name: target, kind: crate::TraitMountKind::Declaration, .. } = it {
                        if !reg.tree.contains_key(target) {
                            diags.push(OntologyDiag::UnknownTrait {
                                referrer: s.name.clone(),
                                target: target.clone(),
                            });
                        }
                    }
                }
            }
        }
        detect_cycles(&reg.tree, &mut diags);
        (reg, diags)
    }

    pub fn has(&self, name: &str) -> bool {
        self.tree.contains_key(name)
    }

    pub fn category_is_a(&self, category: &str, ancestor: &str) -> bool {
        self.has(category)
            && self.has(ancestor)
            && self.closure(category).iter().any(|item| item == ancestor)
    }

    /// 一組範疇是否滿足「必須是 `required`(或其後裔)」——**約束判定的唯一出處**。
    ///
    /// 此前 slot 與 role 各寫一套(前者字串相等、後者 `category_is_a`),
    /// 還有兩處寫成 `c == required || category_is_a(..)` 的雙保險。三種寫法對
    /// 「已知範疇的閉包」等價,但那要靠手推才知道——而手推是會錯的。集中在此,
    /// 讓「slot 與 role 的判定是否一致」不再是需要推導的問題。
    ///
    /// 採 `category_is_a` 而非字串相等:對閉包輸入兩者同義,但輸入未必總是閉包
    /// ——`SemanticDocumentV1` 由外部文件反序列化,可能只給葉範疇。
    pub fn categories_satisfy(&self, categories: &[String], required: &str) -> bool {
        categories
            .iter()
            .any(|category| self.category_is_a(category, required))
    }

    /// 掛上「可解析但未宣告」的匯出索引,供 R13 的指路訊息使用。
    pub fn with_available(mut self, available: BTreeMap<String, crate::LibraryId>) -> Self {
        self.available = available;
        self
    }

    /// 名字查無時的指路後綴。找不到出處就回空字串。
    ///
    /// **不為 `std:*` 設專用診斷碼**——裁定 S 之下,「沒宣告 `std:core` 卻用了
    /// `Noun`」與「沒宣告任何定義 `Noun` 的套件」是同一個錯誤。此處只是把
    /// catalog 已知的出處附上,對使用者自己的 plugin 一視同仁
    /// (對映 C++ 的 `did you forget to #include <vector>?`)。
    pub fn missing_name_hint(&self, name: &str) -> String {
        match self.available.get(name) {
            Some(package) => {
                format!("; exported by {package}, add it to your import table")
            }
            None => String::new(),
        }
    }

    pub fn node(&self, name: &str) -> Option<&OntoNode> {
        self.tree.get(name)
    }

    /// Low-to-high inheritance precedence for one sign. Far ancestors are
    /// applied before near ancestors; at equal distance, later source
    /// `belongs` paths apply later. Diamond nodes occur exactly once.
    pub fn inheritance_order(&self, sign: &SignDef) -> Vec<TraitProvenance> {
        let roots: Vec<String> = sign
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::TraitMount { name: name, kind: crate::TraitMountKind::Declaration, .. } if self.has(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        let mut queue = VecDeque::new();
        let mut best: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut sequence = 0usize;
        for root in roots {
            sequence += 1;
            best.insert(root.clone(), (0, sequence));
            queue.push_back((root, 0usize, sequence));
        }
        while let Some((name, distance, precedence)) = queue.pop_front() {
            if best.get(&name) != Some(&(distance, precedence)) {
                continue;
            }
            let Some(node) = self.tree.get(&name) else {
                continue;
            };
            for parent in &node.parents {
                if !self.has(parent) {
                    continue;
                }
                sequence += 1;
                let candidate = (distance + 1, sequence);
                let replace = match best.get(parent) {
                    None => true,
                    Some((old_distance, old_precedence)) => {
                        candidate.0 < *old_distance
                            || (candidate.0 == *old_distance && candidate.1 > *old_precedence)
                    }
                };
                if replace {
                    best.insert(parent.clone(), candidate);
                    queue.push_back((parent.clone(), candidate.0, candidate.1));
                }
            }
        }
        let mut order: Vec<_> = best
            .into_iter()
            .map(|(trait_name, (distance, precedence))| TraitProvenance {
                trait_name,
                distance,
                precedence,
            })
            .collect();
        order.sort_by(|a, b| {
            b.distance
                .cmp(&a.distance)
                .then(a.precedence.cmp(&b.precedence))
                .then(a.trait_name.cmp(&b.trait_name))
        });
        order
    }

    /// 某 sign 繼承來的有效值(`Def` + `FeatureValue`),**逐包解析**。
    ///
    /// 每個直接掛載的 trait 先在自己那層解完(它的祖先鏈、它自己的覆寫),sign
    /// 只看得到解完的包;於是「後代覆寫祖先」在包內部就結束了,不會冒到 sign
    /// 這一層來假裝成兩個 trait 的分歧。
    ///
    /// 包與包之間對同一 feature 給不同值時取**候選聯集**(未定案),因為那多半
    /// 不是矛盾而是這個 sign 在該維度本來就沒收斂——英語 *police* 同時是可數
    /// 名詞(承繼 sg)與集合名詞(pl),而 *the police are* / *is* 的英美分歧
    /// 正是這個未定案本身。挑一個贏家會把這個事實刪掉。
    ///
    /// 聯集可交換,所以**不需要任何優先序**(距離、書寫序、套件 priority)來裁
    /// 並列的包誰贏。
    pub fn inherited_values(&self, sign: &SignDef) -> Vec<SignItem> {
        let mut memo = BTreeMap::new();
        let mut acc = BTreeMap::new();
        let mut counter = 0usize;
        for mount in self.direct_mounts(sign) {
            let mut stack = BTreeSet::new();
            for item in self.resolve_node_values(&mount, &mut memo, &mut stack) {
                merge_value(&mut acc, &mut counter, item, true);
            }
        }
        flatten_values(acc)
    }

    /// sign 直接 `belongs` 的 trait,依書寫序。
    fn direct_mounts(&self, sign: &SignDef) -> Vec<String> {
        sign.items
            .iter()
            .filter_map(|item| match item {
                SignItem::TraitMount {
                    name,
                    kind: crate::TraitMountKind::Declaration,
                    ..
                } if self.has(name) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// 解析單一 trait 節點的值包:先併入各祖先的包(並列取聯集),再套用本層
    /// 自己寫的值(取代)。循環由 registry 的建構診斷負責報告,這裡只保證不會
    /// 無限遞迴。
    fn resolve_node_values(
        &self,
        name: &str,
        memo: &mut BTreeMap<String, Vec<SignItem>>,
        stack: &mut BTreeSet<String>,
    ) -> Vec<SignItem> {
        if let Some(cached) = memo.get(name) {
            return cached.clone();
        }
        if !stack.insert(name.to_owned()) {
            return Vec::new();
        }
        let mut acc = BTreeMap::new();
        let mut counter = 0usize;
        if let Some(node) = self.tree.get(name) {
            for parent in &node.parents {
                if !self.has(parent) {
                    continue;
                }
                for item in self.resolve_node_values(parent, memo, stack) {
                    merge_value(&mut acc, &mut counter, item, true);
                }
            }
            for item in &node.items {
                if matches!(item, SignItem::Def(_) | SignItem::FeatureValue(_)) {
                    merge_value(&mut acc, &mut counter, item.clone(), false);
                }
            }
        }
        stack.remove(name);
        let resolved = flatten_values(acc);
        memo.insert(name.to_owned(), resolved.clone());
        resolved
    }

    /// Materialize inherited Def/slot/rule content for runtime evaluation.
    /// Classification markers stay local and remain queryable through the
    /// registry; inherited content is a compile artifact.
    pub fn effective_sign(&self, sign: &SignDef) -> SignDef {
        // Generic Defs and typed FeatureValues share one effective value
        // namespace.  Keep them in a single precedence stream so a local
        // generic assignment can beat an inherited typed assignment (and
        // vice versa) according to the normal inheritance order.
        // 值走逐包解析(見 `inherited_values`);slot/rule/宣告等其餘內容仍走
        // 閉包序,它們不是「同路徑選一個值」的問題,沒有這裡要解的分歧。
        let inherited_values = self.inherited_values(sign);
        let mut inherited_slots = Vec::new();
        let mut inherited_slot_maps = Vec::new();
        let mut inherited_slot_features = Vec::new();
        let mut inherited_rules = Vec::new();
        let mut inherited_features = Vec::new();
        let mut inherited_feature_expressions = Vec::new();
        let mut inherited_role_decls = Vec::new();
        let mut inherited_role_bindings = Vec::new();
        let mut inherited_role_expressions = Vec::new();
        let mut inherited_senses = Vec::new();
        let mut inherited_sense_edges = Vec::new();
        let mut inherited_realizations = Vec::new();
        let mut inherited_expressions = Vec::new();
        let mut inherited_constraints = Vec::new();
        for source in self.inheritance_order(sign) {
            if let Some(node) = self.node(&source.trait_name) {
                for item in &node.items {
                    match item {
                        // Def/FeatureValue 已由 `inherited_values` 逐包解析,這裡不再收。
                        SignItem::Def(_) => {}
                        SignItem::Slot(slot) => inherited_slots.push(slot.clone()),
                        SignItem::SlotMap(operation) => inherited_slot_maps.push(operation.clone()),
                        SignItem::SlotFeatureBinding(binding) => {
                            inherited_slot_features.push(binding.clone())
                        }
                        SignItem::Rule(rule) => inherited_rules.push(SignItem::Rule(rule.clone())),
                        SignItem::FeatureRule(rule) => {
                            inherited_rules.push(SignItem::FeatureRule(rule.clone()))
                        }
                        SignItem::FeatureDecl(feature) => inherited_features.push(feature.clone()),
                        SignItem::FeatureValue(_) => {}
                        SignItem::FeatureExpression(expression) => {
                            inherited_feature_expressions.push(expression.clone())
                        }
                        SignItem::RoleDecl(role) => inherited_role_decls.push(role.clone()),
                        SignItem::RoleBinding(role) => inherited_role_bindings.push(role.clone()),
                        SignItem::RoleExpression(expression) => {
                            inherited_role_expressions.push(expression.clone())
                        }
                        SignItem::Sense(sense) => inherited_senses.push(sense.clone()),
                        SignItem::SenseEdge(edge) => inherited_sense_edges.push(edge.clone()),
                        SignItem::Realization(realization) => {
                            inherited_realizations.push(realization.clone())
                        }
                        SignItem::SignExpression(expression) => {
                            inherited_expressions.push(expression.clone())
                        }
                        SignItem::Constraint(constraint) => {
                            inherited_constraints.push(constraint.clone())
                        }
                        _ => {}
                    }
                }
            }
        }

        let local_values = sign
            .items
            .iter()
            .filter(|item| matches!(item, SignItem::Def(_) | SignItem::FeatureValue(_)));
        let local_slots = sign.items.iter().filter_map(|item| match item {
            SignItem::Slot(slot) => Some(slot.clone()),
            _ => None,
        });
        let local_rules = sign.items.iter().filter_map(|item| match item {
            SignItem::Rule(rule) => Some(SignItem::Rule(rule.clone())),
            SignItem::FeatureRule(rule) => Some(SignItem::FeatureRule(rule.clone())),
            _ => None,
        });
        let local_slot_maps = sign.items.iter().filter_map(|item| match item {
            SignItem::SlotMap(operation) => Some(operation.clone()),
            _ => None,
        });
        let local_slot_features = sign.items.iter().filter_map(|item| match item {
            SignItem::SlotFeatureBinding(binding) => Some(binding.clone()),
            _ => None,
        });
        let local_features = sign.items.iter().filter_map(|item| match item {
            SignItem::FeatureDecl(feature) => Some(feature.clone()),
            _ => None,
        });
        let local_feature_expressions = sign.items.iter().filter_map(|item| match item {
            SignItem::FeatureExpression(expression) => Some(expression.clone()),
            _ => None,
        });
        let local_role_decls = sign.items.iter().filter_map(|item| match item {
            SignItem::RoleDecl(role) => Some(role.clone()),
            _ => None,
        });
        let local_role_bindings = sign.items.iter().filter_map(|item| match item {
            SignItem::RoleBinding(role) => Some(role.clone()),
            _ => None,
        });
        let local_role_expressions = sign.items.iter().filter_map(|item| match item {
            SignItem::RoleExpression(expression) => Some(expression.clone()),
            _ => None,
        });
        let local_senses = sign.items.iter().filter_map(|item| match item {
            SignItem::Sense(sense) => Some(sense.clone()),
            _ => None,
        });
        let local_sense_edges = sign.items.iter().filter_map(|item| match item {
            SignItem::SenseEdge(edge) => Some(edge.clone()),
            _ => None,
        });
        let local_realizations = sign.items.iter().filter_map(|item| match item {
            SignItem::Realization(realization) => Some(realization.clone()),
            _ => None,
        });
        let local_expressions = sign.items.iter().filter_map(|item| match item {
            SignItem::SignExpression(expression) => Some(expression.clone()),
            _ => None,
        });
        let local_constraints = sign.items.iter().filter_map(|item| match item {
            SignItem::Constraint(constraint) => Some(constraint.clone()),
            _ => None,
        });


        // 繼承段已在 `inherited_values` 逐包解析完;本地值一律取代(P6 最高階)。
        let mut value_acc = BTreeMap::new();
        let mut value_counter = 0usize;
        for item in inherited_values {
            merge_value(&mut value_acc, &mut value_counter, item, false);
        }
        for item in local_values.cloned() {
            merge_value(&mut value_acc, &mut value_counter, item, false);
        }
        let values = flatten_values(value_acc);

        let mut slots = BTreeMap::<String, (usize, Slot)>::new();
        for (index, slot) in inherited_slots.into_iter().chain(local_slots).enumerate() {
            slots.insert(slot.name.clone(), (index, slot));
        }
        let mut slots: Vec<_> = slots.into_values().collect();
        slots.sort_by_key(|(index, _)| *index);

        let mut slot_features = BTreeMap::new();
        for (index, binding) in inherited_slot_features
            .into_iter()
            .chain(local_slot_features)
            .enumerate()
        {
            slot_features.insert(
                (binding.slot.clone(), binding.feature.clone()),
                (index, binding),
            );
        }
        let mut slot_features: Vec<_> = slot_features.into_values().collect();
        slot_features.sort_by_key(|(index, _)| *index);

        let mut features = BTreeMap::new();
        for (index, feature) in inherited_features
            .into_iter()
            .chain(local_features)
            .enumerate()
        {
            features.insert((feature.dim, feature.name.clone()), (index, feature));
        }
        let mut features: Vec<_> = features.into_values().collect();
        features.sort_by_key(|(index, _)| *index);

        let mut feature_expressions = BTreeMap::new();
        for (index, expression) in inherited_feature_expressions
            .into_iter()
            .chain(local_feature_expressions)
            .enumerate()
        {
            feature_expressions.insert(
                (expression.dim, expression.name.clone()),
                (index, expression),
            );
        }
        let mut feature_expressions: Vec<_> = feature_expressions.into_values().collect();
        feature_expressions.sort_by_key(|(index, _)| *index);

        let mut role_decls = BTreeMap::new();
        for (index, role) in inherited_role_decls
            .into_iter()
            .chain(local_role_decls)
            .enumerate()
        {
            role_decls.insert(role.name.clone(), (index, role));
        }
        let mut role_decls: Vec<_> = role_decls.into_values().collect();
        role_decls.sort_by_key(|(index, _)| *index);

        let mut role_bindings = BTreeMap::new();
        for (index, role) in inherited_role_bindings
            .into_iter()
            .chain(local_role_bindings)
            .enumerate()
        {
            role_bindings.insert(role.name.clone(), (index, role));
        }
        let mut role_bindings: Vec<_> = role_bindings.into_values().collect();
        role_bindings.sort_by_key(|(index, _)| *index);

        let mut role_expressions = BTreeMap::new();
        for (index, expression) in inherited_role_expressions
            .into_iter()
            .chain(local_role_expressions)
            .enumerate()
        {
            role_expressions.insert(expression.name.clone(), (index, expression));
        }
        let mut role_expressions: Vec<_> = role_expressions.into_values().collect();
        role_expressions.sort_by_key(|(index, _)| *index);

        let realization = inherited_realizations
            .into_iter()
            .chain(local_realizations)
            .last();

        // 義項依名字合併(本地覆寫繼承,保順序);衍生邊無名字,直接串接。
        let mut senses = BTreeMap::new();
        for (index, sense) in inherited_senses.into_iter().chain(local_senses).enumerate() {
            senses.insert(sense.name.clone(), (index, sense));
        }
        let mut senses: Vec<_> = senses.into_values().collect();
        senses.sort_by_key(|(index, _)| *index);
        let sense_edges: Vec<_> = inherited_sense_edges
            .into_iter()
            .chain(local_sense_edges)
            .collect();

        let mut rules: Vec<_> = inherited_rules.into_iter().chain(local_rules).collect();
        let rank = |stage: Stage| match stage {
            Stage::Stem => 0,
            Stage::Word => 1,
            Stage::Phrase => 2,
        };
        rules.sort_by_key(|item| match item {
            SignItem::Rule(rule) | SignItem::FeatureRule(rule) => rank(rule.stage),
            _ => unreachable!("effective rule stream contains only rules"),
        });

        let mut items: Vec<SignItem> = sign
            .items
            .iter()
            .filter(|item| matches!(item, SignItem::TraitMount { name: _, kind: crate::TraitMountKind::Declaration, .. }))
            .cloned()
            .collect();
        items.extend(senses.into_iter().map(|(_, sense)| SignItem::Sense(sense)));
        items.extend(sense_edges.into_iter().map(SignItem::SenseEdge));
        items.extend(values);
        items.extend(slots.into_iter().map(|(_, slot)| SignItem::Slot(slot)));
        items.extend(
            slot_features
                .into_iter()
                .map(|(_, binding)| SignItem::SlotFeatureBinding(binding)),
        );
        items.extend(
            features
                .into_iter()
                .map(|(_, feature)| SignItem::FeatureDecl(feature)),
        );
        items.extend(
            feature_expressions
                .into_iter()
                .map(|(_, expression)| SignItem::FeatureExpression(expression)),
        );
        items.extend(
            role_decls
                .into_iter()
                .map(|(_, role)| SignItem::RoleDecl(role)),
        );
        items.extend(
            role_bindings
                .into_iter()
                .map(|(_, role)| SignItem::RoleBinding(role)),
        );
        items.extend(
            role_expressions
                .into_iter()
                .map(|(_, expression)| SignItem::RoleExpression(expression)),
        );
        if let Some(realization) = realization {
            items.push(SignItem::Realization(realization));
        }
        items.extend(
            inherited_constraints
                .into_iter()
                .chain(local_constraints)
                .map(SignItem::Constraint),
        );
        items.extend(
            inherited_expressions
                .into_iter()
                .chain(local_expressions)
                .map(SignItem::SignExpression),
        );
        items.extend(
            inherited_slot_maps
                .into_iter()
                .chain(local_slot_maps)
                .map(SignItem::SlotMap),
        );
        items.extend(rules);
        SignDef {
            id: sign.id.clone(),
            name: sign.name.clone(),
            items,
        }
    }

    /// Structured P38–P40 validation. Resolved Def conflicts are warnings
    /// with winner-first provenance; unknown/cyclic ontology edges and slot
    /// conflicts are errors.
    pub fn validation_report(
        &self,
        langs: &[&Language],
        legacy: &[OntologyDiag],
    ) -> ValidationReport {
        let mut report = ValidationReport::new();
        report.extend(block_shape_diagnostics(langs));
        for diagnostic in legacy {
            match diagnostic {
                OntologyDiag::UnknownTrait { referrer, target } => report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "ONTOLOGY_UNKNOWN_TRAIT",
                        format!(
                            "{referrer:?} refers to unknown trait {target:?}{}",
                            self.missing_name_hint(target)
                        ),
                    )
                    .with_sources(vec![DiagnosticSource {
                        owner: referrer.clone(),
                        path: Some(format!("belongs {target}")),
                        location: SourceLocation::unknown(),
                    }]),
                ),
                OntologyDiag::Cycle { path } => report.push(Diagnostic::new(
                    Severity::Error,
                    "ONTOLOGY_CYCLE",
                    format!("ontology cycle: {}", path.join(" -> ")),
                )),
                OntologyDiag::DuplicateTrait { name } => report.push(Diagnostic::new(
                    Severity::Error,
                    "ONTOLOGY_DUPLICATE_TRAIT",
                    format!("duplicate trait {name:?}"),
                )),
            }
        }

        for lang in langs {
            let mut candidates = lang.signs.clone();
            candidates.extend(
                lang.traits
                    .iter()
                    .filter(|item| !item.global)
                    .map(|trait_def| SignDef {
                        id: crate::SignId::synthetic(),
                        name: trait_def.name.clone(),
                        items: vec![SignItem::TraitMount { name: trait_def.name.clone(), kind: crate::TraitMountKind::Declaration, args: vec![] }],
                    }),
            );
            for sign in &candidates {
                // 未定案值域:並列掛載的 trait 對同一 feature 給了不同的值,已取
                // 候選聯集。**這是合法狀態**(*fish* 單複同形、*police* 的英美
                // 一致性分歧),所以是 Info 不是 Warning——它報告的是一個推導出來
                // 的事實,不是要作者去修的缺陷。真要定案,在 sign 上顯式寫值即可
                // (P6 最高階),或讓構式求交收斂。
                for item in self.inherited_values(sign) {
                    let SignItem::FeatureValue(feature) = item else {
                        continue;
                    };
                    if !feature.is_undecided() {
                        continue;
                    }
                    let path = format!("{}.{}", feature.dim.keyword(), feature.name);
                    // sign 自己寫了值就已經定案(P6 最高階),繼承層的分歧已被取代
                    // ——此時再報未定案是報一個不存在的狀態。
                    let decided_locally = sign.items.iter().any(|item| match item {
                        SignItem::FeatureValue(local) => {
                            local.dim == feature.dim && local.name == feature.name
                        }
                        SignItem::Def(def) => def.path == path,
                        _ => false,
                    });
                    if decided_locally {
                        continue;
                    }
                    report.push(
                        Diagnostic::new(
                            Severity::Info,
                            "FEATURE_UNRESOLVED_ACROSS_TRAITS",
                            format!(
                                "{} {path} is undecided across its mounted traits ({}); \
                                 a construction can narrow it, or assign the value on the \
                                 sign itself to decide it here",
                                sign.name,
                                feature.values.join(" | ")
                            ),
                        )
                        .with_sources(vec![DiagnosticSource {
                            owner: sign.name.clone(),
                            path: Some(path),
                            location: feature.source,
                        }]),
                    );
                }
                let order = self.inheritance_order(sign);
                for dim in Dim::all() {
                    let mut by_path: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
                    for source in &order {
                        if let Some(node) = self.node(&source.trait_name) {
                            for (path, value) in &node.defs {
                                if path_dim(path) == Some(dim) {
                                    by_path
                                        .entry(path.clone())
                                        .or_default()
                                        .push((source.trait_name.clone(), value.clone()));
                                }
                            }
                        }
                    }
                    for item in &sign.items {
                        if let SignItem::Def(def) = item {
                            if path_dim(&def.path) == Some(dim) {
                                by_path
                                    .entry(def.path.clone())
                                    .or_default()
                                    .push((sign.name.clone(), def.value.clone()));
                            }
                        }
                    }
                    // winner 必須取自**實際的合併結果**,不能自己用閉包序再算一次。
                    // 值走的是逐包解析(`inherited_values`),與這裡的攤平序不是同
                    // 一套;各算各的會讓警告報 X、引擎實際用 Y。
                    let effective: BTreeMap<String, String> = self
                        .inherited_values(sign)
                        .into_iter()
                        .chain(sign.items.iter().cloned())
                        .filter_map(|item| match item {
                            SignItem::Def(def) => Some((def.path, def.value)),
                            _ => None,
                        })
                        .collect();
                    for (path, sources) in by_path {
                        let distinct: BTreeSet<_> =
                            sources.iter().map(|(_, value)| value).collect();
                        if sources.len() > 1 && distinct.len() > 1 {
                            let fallback = sources.last().expect("non-empty").clone();
                            let winner_value = effective.get(&path).unwrap_or(&fallback.1);
                            // owner 取最後一個「值與實際結果相同」的來源,讓訊息
                            // 裡的值與出處指向同一件事。
                            let winner_owner = sources
                                .iter()
                                .rev()
                                .find(|(_, value)| value == winner_value)
                                .map(|(owner, _)| owner)
                                .unwrap_or(&fallback.0);
                            let mut provenance: Vec<_> = sources
                                .iter()
                                .rev()
                                .map(|(owner, _)| DiagnosticSource {
                                    owner: owner.clone(),
                                    path: Some(path.clone()),
                                    location: SourceLocation::unknown(),
                                })
                                .collect();
                            provenance.dedup_by(|a, b| a.owner == b.owner && a.path == b.path);
                            report.push(
                                Diagnostic::new(
                                    Severity::Warning,
                                    "ONTOLOGY_DEF_CONFLICT_RESOLVED",
                                    format!(
                                        "{} {path} conflict resolved to {winner_value:?} from {winner_owner:?}",
                                        sign.name
                                    ),
                                )
                                .with_sources(provenance),
                            );
                        }
                    }
                }

                let mut slots: BTreeMap<String, Vec<(String, Slot)>> = BTreeMap::new();
                for source in &order {
                    if let Some(node) = self.node(&source.trait_name) {
                        for item in &node.items {
                            if let SignItem::Slot(slot) = item {
                                slots
                                    .entry(slot.name.clone())
                                    .or_default()
                                    .push((source.trait_name.clone(), slot.clone()));
                            }
                        }
                    }
                }
                for item in &sign.items {
                    if let SignItem::Slot(slot) = item {
                        slots
                            .entry(slot.name.clone())
                            .or_default()
                            .push((sign.name.clone(), slot.clone()));
                    }
                }
                for (name, definitions) in slots {
                    let distinct: BTreeSet<_> = definitions
                        .iter()
                        .map(|(_, slot)| (slot.constraint.clone(), slot.optional))
                        .collect();
                    if distinct.len() > 1 {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "SLOT_CONFLICT",
                                format!(
                                    "{} inherits incompatible definitions for slot {name:?}",
                                    sign.name
                                ),
                            )
                            .with_sources(
                                definitions
                                    .iter()
                                    .rev()
                                    .map(|(owner, _)| DiagnosticSource {
                                        owner: owner.clone(),
                                        path: Some(format!("slot {name}")),
                                        location: SourceLocation::unknown(),
                                    })
                                    .collect(),
                            ),
                        );
                    }
                    for (owner, slot) in definitions {
                        let Some(required) = slot.constraint.category() else {
                            continue;
                        };
                        if !self.has(required) {
                            report.push(
                                Diagnostic::new(
                                    Severity::Error,
                                    "SLOT_UNKNOWN_CATEGORY",
                                    format!(
                                        "slot {name:?} in {owner:?} requires unknown category {:?}{}",
                                        required,
                                        self.missing_name_hint(required)
                                    ),
                                )
                                .with_sources(vec![
                                    DiagnosticSource {
                                        owner,
                                        path: Some(format!("slot {name}")),
                                        location: SourceLocation::unknown(),
                                    },
                                ]),
                            );
                        }
                    }
                }
            }
        }
        report
    }

    /// 範疇節點的 **belongs 閉包**:self → 父 → 祖 … → 根(nearest-first、
    /// pre-order DFS、首見去重、循環安全)。未知 → 空。
    pub fn closure(&self, name: &str) -> Vec<String> {
        if !self.tree.contains_key(name) {
            return Vec::new();
        }
        fn visit(
            tree: &BTreeMap<String, OntoNode>,
            name: &str,
            out: &mut Vec<String>,
            seen: &mut BTreeSet<String>,
        ) {
            if !seen.insert(name.to_owned()) {
                return;
            }
            out.push(name.to_owned());
            if let Some(node) = tree.get(name) {
                for p in &node.parents {
                    if tree.contains_key(p) {
                        visit(tree, p, out, seen);
                    }
                }
            }
        }
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        visit(&self.tree, name, &mut out, &mut seen);
        out
    }

    /// 一個 sign 的分類閉包:各 `belongs` 目標的閉包依序併接、全域去重。
    pub fn sign_categories(&self, sign: &SignDef) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for it in &sign.items {
            if let SignItem::TraitMount { name: target, kind: crate::TraitMountKind::Declaration, .. } = it {
                if self.has(target) {
                    for c in self.closure(target) {
                        if seen.insert(c.clone()) {
                            out.push(c);
                        }
                    }
                }
            }
        }
        out
    }

    /// 全部節點名(測試/inspection;決定性排序)。
    pub fn names(&self) -> Vec<&str> {
        self.tree.keys().map(String::as_str).collect()
    }
}

fn path_dim(path: &str) -> Option<Dim> {
    let head = path.split_once('.').map(|(head, _)| head).unwrap_or(path);
    Dim::parse(head)
}

/// DFS 三色循環偵測。
fn detect_cycles(tree: &BTreeMap<String, OntoNode>, diags: &mut Vec<OntologyDiag>) {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: BTreeMap<String, Color> =
        tree.keys().map(|k| (k.clone(), Color::White)).collect();
    let mut reported: BTreeSet<Vec<String>> = BTreeSet::new();

    fn dfs(
        node: &str,
        tree: &BTreeMap<String, OntoNode>,
        color: &mut BTreeMap<String, Color>,
        stack: &mut Vec<String>,
        reported: &mut BTreeSet<Vec<String>>,
        diags: &mut Vec<OntologyDiag>,
    ) {
        color.insert(node.to_owned(), Color::Gray);
        stack.push(node.to_owned());
        if let Some(n) = tree.get(node) {
            for p in &n.parents {
                if !tree.contains_key(p) {
                    continue;
                }
                match color.get(p).copied().unwrap_or(Color::White) {
                    Color::Gray => {
                        if let Some(pos) = stack.iter().position(|x| x == p) {
                            let cycle = stack[pos..].to_vec();
                            if reported.insert(cycle.clone()) {
                                diags.push(OntologyDiag::Cycle { path: cycle });
                            }
                        }
                    }
                    Color::White => dfs(p, tree, color, stack, reported, diags),
                    Color::Black => {}
                }
            }
        }
        stack.pop();
        color.insert(node.to_owned(), Color::Black);
    }

    let names: Vec<String> = tree.keys().cloned().collect();
    for name in names {
        if color.get(&name).copied() == Some(Color::White) {
            let mut stack = Vec::new();
            dfs(&name, tree, &mut color, &mut stack, &mut reported, diags);
        }
    }
}

/// 最小本體:額外引用的 stdlib `.lang` trait(I20;資料層,非 Rust 硬編碼)。
/// Load the embedded official ontology and Grambank packages without panicking.
pub fn try_std_ontology() -> Result<Language, crate::stdlib::StdLoadError> {
    crate::stdlib::load_default()
}

/// Compatibility wrapper for callers that treat the embedded std library as
/// a release-time invariant. Public compile entry points use the fallible API.
pub fn std_ontology() -> Language {
    try_std_ontology().expect("embedded std library must validate")
}

/// 便利:以 stdlib 本體 + 使用者 Language 建 registry(額外引用語意)。
pub fn with_std(user: &Language) -> (OntologyRegistry, Vec<OntologyDiag>) {
    let std = std_ontology();
    OntologyRegistry::build(&[&std, user])
}
