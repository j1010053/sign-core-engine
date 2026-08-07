//! 語意接口:form-meaning pair 的 **meaning 極**(步驟 12c;修補07 §12c,docs/07 §5)。
//!
//! **可擴充語意接口**——現為 feature-structure / frame(純量欄位 + role→子節點的
//! 遞迴組合)。刻意設計為容納**未來複雜語意模型**:義項網絡(senses)、衍生邊
//! (隱喻/換喻/窄化/寬化)、frame roles、事件結構、多義/同義——皆以**新增欄位或
//! `SemValue` variant** 擴充此結構,**不破壞** `fields`/`roles` 既有唯讀 API
//! (承 P30 typed projection:視圖唯讀,修改走 patch)。組合遞迴:role 綁定持
//! **語意節點,非字串**(修補07 §12c 硬要求)——filler 若為 derived token,其
//! `SemNode` 亦遞迴組合。
//!
//! 12c 子集 = 純量欄位 + role;senses/edges 的**位置已預留**(見 [`SemNode`] 欄位
//! 註記),行為留 B(語法化/語意漂移引擎)。

use crate::ontology::OntologyRegistry;
use crate::{Dim, SignDef, SignItem};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticSource {
    pub package: Option<String>,
    pub sign: String,
}

/// 義項的**唯讀投影視圖**(對映 AST `crate::Sense`,刻意不帶 `SourceLocation`——
/// 投影是語意內容,不該因行號不同而不等)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenseView {
    pub name: String,
    pub gloss: String,
}

/// 衍生邊的唯讀投影視圖(對映 AST `crate::SenseEdge`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationEdge {
    pub to: String,
    pub from: String,
    pub kind: crate::DerivationKind,
    pub transparency: crate::SenseTransparency,
}

/// 一個組合語意節點(meaning 極)。**唯讀視圖**;修改走 patch(12e)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemNode {
    /// Ontological semantic/frame types. Frame identity is never a free scalar.
    pub types: Vec<String>,
    /// Values of declared `sem: feature:` enums.
    pub features: BTreeMap<String, String>,
    pub source: SemanticSource,
    /// 純量語意欄位(`sem.*` Def)。保插入序,同名後者勝(projection 已解析)。
    ///
    /// P71 §4.1 後**不含 `gloss`**:義項內容住 [`SemNode::senses`],
    /// `field("gloss")` 自主義項投影。**多義(polysemy)= 多個義項節點**,
    /// 不再是「多個 sense 欄位」。
    pub fields: Vec<(String, String)>,
    /// role → 子語意節點(construction 的 `{slot}` 引用解析;**持節點非字串**)。
    /// 遞迴組合:filler 為 derived token 時其 `SemNode` 亦於此。
    pub roles: Vec<(String, SemNode)>,
    /// **義項網絡節點**(docs/07 §5;《修補05》§10.3)。多義 = 多個義項,各有身分。
    /// 15a 起由 `sem: senses:` 的一級節點填實(此前僅為預留註解)。
    pub senses: Vec<SenseView>,
    /// **衍生邊**(隱喻/換喻/窄化/寬化 + 固化標記),由 `sem: edges:` 填實。
    pub edges: Vec<DerivationEdge>,
    // 未來擴充位(不破壞上列 API):
    //   frame:  Option<FrameId>       // 受控 frame 本體引用
}

impl SemNode {
    /// 義項網絡的**主義項**:名為 `core` 者優先,否則取宣告序首項。
    ///
    /// P71 §4.1:`gloss` 退出 `Def`,義項內容一律住 [`SemNode::senses`]。`core` 優先
    /// 而非單純取首項,是因為 Atomic Rewrite `drift(sign, sense: core, …)` 以名字定址;
    /// 若 `derive_sense` 之後義項序有變,主義項仍須穩定指向 `core`。
    pub fn primary_sense(&self) -> Option<&SenseView> {
        self.senses
            .iter()
            .find(|sense| sense.name == "core")
            .or_else(|| self.senses.first())
    }

    /// 純量欄位查值(便利)。
    ///
    /// `"gloss"` 是**唯讀投影**而非真實欄位:P71 §4.1 已把 `gloss` 移出 `Def` 路徑
    /// 封閉清單,內容住 [`SemNode::senses`]。此處自主義項投影,使既有讀者
    /// (含 guard 的 `$self.sem.gloss`)不必全體改寫,且 `drift` 改義項後立即可見。
    pub fn field(&self, name: &str) -> Option<&str> {
        if let Some(value) = self.features.get(name) {
            return Some(value);
        }
        if name == "gloss" {
            return self.primary_sense().map(|sense| sense.gloss.as_str());
        }
        self.fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    /// role 子節點(便利)。
    pub fn role(&self, name: &str) -> Option<&SemNode> {
        self.roles.iter().find(|(k, _)| k == name).map(|(_, n)| n)
    }
    /// 原子語意(無 role 綁定;詞彙 sign 的預設)。
    pub fn is_atomic(&self) -> bool {
        self.roles.is_empty()
    }

    /// 由一個 sign 的 **sem projection** 建原子 `SemNode`(欄位 = `sem.*` Def,
    /// 去維度前綴;role 空——詞彙 filler 為原子)。construction 的 role 組合於
    /// [`crate::construction`] 解析。
    pub fn of_sign(sign: &SignDef, reg: &OntologyRegistry) -> SemNode {
        let effective = reg.effective_sign(sign);
        let proj = effective.project(Dim::Sem, reg);
        let feature_names = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::FeatureDecl(feature) if feature.dim == Dim::Sem => {
                    Some(feature.name.clone())
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        let features = proj
            .defs
            .iter()
            .filter_map(|(path, value)| {
                let name = path.strip_prefix("sem.").unwrap_or(path);
                feature_names
                    .contains(name)
                    .then(|| (name.to_owned(), value.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let fields = proj
            .defs
            .iter()
            .filter(|(path, _)| {
                let name = path.strip_prefix("sem.").unwrap_or(path);
                // P71 §4.1:`gloss` 不再是 Def 欄位,`field("gloss")` 改投影自
                // `senses`。此處排除是為了讓兩者不會並存出兩份真相。
                !features.contains_key(name) && name != "gloss"
            })
            .map(|(p, v)| (p.strip_prefix("sem.").unwrap_or(p).to_owned(), v.clone()))
            .collect();
        let categories = reg.sign_categories(&effective);
        // P71-S:`types` 就是**完整範疇閉包**。此處曾以 `reg.has("Semantic")` 過濾出
        // 「語意型別」,但 `Semantic` 只是 `std:core` 定義的一個 trait——由引擎硬寫它,
        // 與已移除的 bridge 是同一類越界。單一中立樹(P38 v0.2)下只有一組範疇。
        let mut types = categories.to_vec();
        // P71-S:此處**曾經**硬寫三條語言學推論(Nominal→Entity、Predicate→Event、
        // Adposition→Relation)。那是語言學分析不是引擎功能,已搬進 `std:core` 寫成
        // 一般的 `belongs`;換一個 std 就換一套推論。原註解稱 bridge 是為了避免污染
        // syn 閉包,但單一中立樹(P38 v0.2)下那件事本來就在發生,並經 `tests/ontology.rs`
        // 明確承認,故該理由不成立。`Adposition→Relation` 依裁定不重建——
        // 格標記類介系詞未必表達 Relation,應由作者顯式 `belongs Relation`。
        types.sort();
        types.dedup();
        // 義項/衍生邊取自 effective items(隨 belongs 繼承,與其他內容一致)。
        let senses = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::Sense(sense) => Some(SenseView {
                    name: sense.name.clone(),
                    gloss: sense.gloss.clone(),
                }),
                _ => None,
            })
            .collect();
        let edges = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::SenseEdge(edge) => Some(DerivationEdge {
                    to: edge.to.clone(),
                    from: edge.from.clone(),
                    kind: edge.kind,
                    transparency: edge.transparency,
                }),
                _ => None,
            })
            .collect();
        SemNode {
            types,
            features,
            source: SemanticSource {
                package: sign.source_package().map(str::to_owned),
                sign: sign.name.clone(),
            },
            fields,
            roles: Vec::new(),
            senses,
            edges,
        }
    }
}

/// `{slot}` 引用?回傳被引用的 slot 名(去空白)。
pub(crate) fn slot_ref(value: &str) -> Option<&str> {
    value
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .map(str::trim)
}
