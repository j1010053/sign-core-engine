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

/// 一個組合語意節點(meaning 極)。**唯讀視圖**;修改走 patch(12e)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemNode {
    /// Ontological semantic/frame types. Frame identity is never a free scalar.
    pub types: Vec<String>,
    /// Values of declared `sem: feature:` enums.
    pub features: BTreeMap<String, String>,
    pub source: SemanticSource,
    /// 純量語意欄位(`sem.*` Def:concept/gloss/ref/frame…)。保插入序,同名後者勝
    /// (projection 已解析)。**多義(polysemy)= 多個 sense 欄位**合法、不去重。
    pub fields: Vec<(String, String)>,
    /// role → 子語意節點(construction 的 `{slot}` 引用解析;**持節點非字串**)。
    /// 遞迴組合:filler 為 derived token 時其 `SemNode` 亦於此。
    pub roles: Vec<(String, SemNode)>,
    // 未來擴充位(不破壞上列 API):
    //   senses: Vec<Sense>            // 義項網絡節點(docs/07 §5)
    //   edges:  Vec<DerivationEdge>   // 衍生邊 隱喻/換喻/窄化/寬化 + 固化標記
    //   frame:  Option<FrameId>       // 受控 frame 本體引用
}

impl SemNode {
    /// 純量欄位查值(便利)。
    pub fn field(&self, name: &str) -> Option<&str> {
        if let Some(value) = self.features.get(name) {
            return Some(value);
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
                !features.contains_key(name)
            })
            .map(|(p, v)| (p.strip_prefix("sem.").unwrap_or(p).to_owned(), v.clone()))
            .collect();
        let categories = reg.sign_categories(&effective);
        let mut types = categories
            .iter()
            .filter(|category| reg.has("Semantic") && reg.category_is_a(category, "Semantic"))
            .cloned()
            .collect::<Vec<_>>();
        // The core ontology deliberately keeps syntactic categories out of
        // the semantic inheritance tree.  These conventional bridges give
        // ordinary Nominal/Predicate/Adposition fillers semantic node types
        // without making `belongs Verb` mutate the public syn category
        // closure.
        let bridge = |syn_category: &str, semantic_type: &str, types: &mut Vec<String>| {
            if reg.has(semantic_type) && categories.iter().any(|item| item == syn_category) {
                types.push(semantic_type.to_owned());
            }
        };
        bridge("Nominal", "Entity", &mut types);
        bridge("Predicate", "Event", &mut types);
        bridge("Adposition", "Relation", &mut types);
        // Every executable sign contributes a semantic node.  The base
        // `Semantic` type makes an intentionally unconstrained `[*]` filler
        // usable for schema roles that only demand a meaning-bearing node;
        // stricter Entity/Event/Animate requirements still validate against
        // ontology membership below it.
        if reg.has("Semantic") {
            types.push("Semantic".to_owned());
        }
        types.sort();
        types.dedup();
        SemNode {
            types,
            features,
            source: SemanticSource {
                package: sign.source_package().map(str::to_owned),
                sign: sign.name.clone(),
            },
            fields,
            roles: Vec::new(),
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
