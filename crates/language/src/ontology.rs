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

/// 單一分類樹 registry(P38 v0.2)。
#[derive(Debug, Clone, Default)]
pub struct OntologyRegistry {
    tree: BTreeMap<String, OntoNode>,
}

fn trait_belongs(t: &TraitDef) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for b in &t.blocks {
        for it in &b.items {
            if let SignItem::Belongs(name) = it {
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
        .filter(|item| !matches!(item, SignItem::Belongs(_) | SignItem::TraitUse { .. }))
        .cloned()
        .collect()
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
                    if let SignItem::Belongs(target) = it {
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
                SignItem::Belongs(name) if self.has(name) => Some(name.clone()),
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

    /// Materialize inherited Def/slot/rule content for runtime evaluation.
    /// Classification markers stay local and remain queryable through the
    /// registry; inherited content is a compile artifact.
    pub fn effective_sign(&self, sign: &SignDef) -> SignDef {
        // Generic Defs and typed FeatureValues share one effective value
        // namespace.  Keep them in a single precedence stream so a local
        // generic assignment can beat an inherited typed assignment (and
        // vice versa) according to the normal inheritance order.
        let mut inherited_values = Vec::new();
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
                        SignItem::Def(def) => inherited_values.push(SignItem::Def(def.clone())),
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
                        SignItem::FeatureValue(feature) => {
                            inherited_values.push(SignItem::FeatureValue(feature.clone()))
                        }
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

        let value_path = |item: &SignItem| match item {
            SignItem::Def(def) => def.path.clone(),
            SignItem::FeatureValue(feature) => {
                format!("{}.{}", feature.dim.keyword(), feature.name)
            }
            _ => unreachable!("effective value stream contains only values"),
        };
        let mut values = BTreeMap::<String, (usize, SignItem)>::new();
        for (index, item) in inherited_values
            .into_iter()
            .chain(local_values.cloned())
            .enumerate()
        {
            values.insert(value_path(&item), (index, item));
        }
        let mut values: Vec<_> = values.into_values().collect();
        values.sort_by_key(|(index, _)| *index);

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
            .filter(|item| matches!(item, SignItem::Belongs(_)))
            .cloned()
            .collect();
        items.extend(senses.into_iter().map(|(_, sense)| SignItem::Sense(sense)));
        items.extend(sense_edges.into_iter().map(SignItem::SenseEdge));
        items.extend(values.into_iter().map(|(_, item)| item));
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
        for diagnostic in legacy {
            match diagnostic {
                OntologyDiag::UnknownTrait { referrer, target } => report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "ONTOLOGY_UNKNOWN_TRAIT",
                        format!("{referrer:?} refers to unknown trait {target:?}"),
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
                        items: vec![SignItem::Belongs(trait_def.name.clone())],
                    }),
            );
            for sign in &candidates {
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
                    for (path, sources) in by_path {
                        let distinct: BTreeSet<_> =
                            sources.iter().map(|(_, value)| value).collect();
                        if sources.len() > 1 && distinct.len() > 1 {
                            let (winner_owner, winner_value) = sources.last().expect("non-empty");
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
                                        "slot {name:?} in {owner:?} requires unknown category {:?}",
                                        required
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
            if let SignItem::Belongs(target) = it {
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
