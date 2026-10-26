//! Frozen reference binding rules for v2 sidecars without reference_semantics.
//! Keep old document digests intact on read; new edits use the current binder.

use super::*;

// Preserve the original metadata grammar as well as its binding rules.
pub(super) fn parse_origin(value: &str) -> Option<crate::SignRef> {
    value
        .strip_prefix("sign(")
        .and_then(|s| s.strip_suffix(')'))
        .filter(|s| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '/'))
        })
        .map(|s| crate::SignRef(s.to_owned()))
}

fn node_ids_by_name(
    language: &Language,
    entries: &[NodeEntryV1],
) -> (
    BTreeMap<String, NodeId>,
    BTreeMap<String, NodeId>,
    BTreeMap<String, NodeId>,
) {
    let mut traits = BTreeMap::new();
    let mut signs = BTreeMap::new();
    let mut paradigms = BTreeMap::new();
    for entry in entries {
        match entry.address.0.as_slice() {
            [AddressSegment::Traits(index)] if entry.kind == NodeKind::Trait => {
                if let Some(trait_def) = language.traits.get(*index) {
                    traits.insert(trait_def.name.clone(), entry.id.clone());
                }
            }
            [AddressSegment::Signs(index)] if entry.kind == NodeKind::Sign => {
                if let Some(sign) = language.signs.get(*index) {
                    signs.insert(sign.name.clone(), entry.id.clone());
                }
            }
            [AddressSegment::Paradigms(index)] if entry.kind == NodeKind::Paradigm => {
                if let Some(paradigm) = language.paradigms.get(*index) {
                    paradigms.insert(paradigm.name.clone(), entry.id.clone());
                }
            }
            _ => {}
        }
    }
    (traits, signs, paradigms)
}

fn reference_target(
    spelling: &str,
    expected: NodeKind,
    locals: &BTreeMap<String, NodeId>,
) -> RefTargetV1 {
    match locals.get(spelling) {
        Some(id) => RefTargetV1::Local {
            target: NodeRef::new(id.clone(), expected),
        },
        None => RefTargetV1::External {
            spelling: spelling.to_owned(),
            expected,
        },
    }
}

fn callable_reference_target(
    spelling: &str,
    signs: &BTreeMap<String, NodeId>,
    paradigms: &BTreeMap<String, NodeId>,
) -> RefTargetV1 {
    if let Some(id) = signs.get(spelling) {
        RefTargetV1::Local {
            target: NodeRef::new(id.clone(), NodeKind::Sign),
        }
    } else if let Some(id) = paradigms.get(spelling) {
        RefTargetV1::Local {
            target: NodeRef::new(id.clone(), NodeKind::Paradigm),
        }
    } else {
        // External packages historically exported applications as Signs.
        // Keep that expectation for an unresolved spelling; a resolved local
        // Paradigm is always bound with its precise kind above.
        RefTargetV1::External {
            spelling: spelling.to_owned(),
            expected: NodeKind::Sign,
        }
    }
}

fn collect_application_refs(
    application: &crate::SignApplication,
    address: &NodeAddress,
    owner: &NodeId,
    entries: &[NodeEntryV1],
    signs: &BTreeMap<String, NodeId>,
    paradigms: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
) {
    refs.push(RefBindingV1 {
        owner: owner.clone(),
        field: "application.callee".to_owned(),
        target: callable_reference_target(&application.callee, signs, paradigms),
    });
    for (index, argument) in application.arguments.iter().enumerate() {
        let crate::SignArgumentValue::Application(nested) = &argument.value else {
            continue;
        };
        let nested_address = address.child(AddressSegment::ApplicationArguments(index));
        let Some(nested_entry) = entry_at(entries, &nested_address, NodeKind::Application) else {
            continue;
        };
        collect_application_refs(
            nested,
            &nested_address,
            &nested_entry.id,
            entries,
            signs,
            paradigms,
            refs,
        );
    }
}

fn collect_expression_refs(
    expression: &crate::Expression,
    address: &NodeAddress,
    root_owner: Option<&NodeId>,
    entries: &[NodeEntryV1],
    traits: &BTreeMap<String, NodeId>,
    signs: &BTreeMap<String, NodeId>,
    paradigms: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
) {
    match expression {
        crate::Expression::SignApplication(application)
        | crate::Expression::PhonInterpolation(application) => {
            let owner = root_owner.cloned().or_else(|| {
                entry_at(entries, address, NodeKind::Application).map(|entry| entry.id.clone())
            });
            if let Some(owner) = owner {
                collect_application_refs(
                    application,
                    address,
                    &owner,
                    entries,
                    signs,
                    paradigms,
                    refs,
                );
            }
        }
        crate::Expression::Projection { value, .. } => collect_expression_refs(
            value, address, root_owner, entries, traits, signs, paradigms, refs,
        ),
        crate::Expression::Case(case) => collect_case_refs(
            case, address, root_owner, entries, traits, signs, paradigms, refs,
        ),
        _ => {}
    }
}

fn collect_case_refs(
    case: &crate::TypedCase,
    address: &NodeAddress,
    root_owner: Option<&NodeId>,
    entries: &[NodeEntryV1],
    traits: &BTreeMap<String, NodeId>,
    signs: &BTreeMap<String, NodeId>,
    paradigms: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
) {
    if root_owner.is_none() && entry_at(entries, address, NodeKind::Case).is_none() {
        return;
    }
    for (branch_index, branch) in case.branches.iter().enumerate() {
        let branch_address = address.child(AddressSegment::CaseBranches(branch_index));
        let Some(branch_entry) = entry_at(entries, &branch_address, NodeKind::CaseBranch) else {
            continue;
        };
        for (belongs_index, category) in branch.belongs.iter().enumerate() {
            refs.push(RefBindingV1 {
                owner: branch_entry.id.clone(),
                field: format!("case.belongs[{belongs_index}]"),
                target: reference_target(category, NodeKind::Trait, traits),
            });
        }
        match &branch.condition {
            crate::CaseCondition::Guard(guard) => {
                for (guard_index, category) in guard_category_references(guard) {
                    refs.push(RefBindingV1 {
                        owner: branch_entry.id.clone(),
                        field: format!("case.guard[{guard_index}].category"),
                        target: reference_target(&category, NodeKind::Trait, traits),
                    });
                }
            }
            crate::CaseCondition::Equals(category)
                if case
                    .scrutinee
                    .as_deref()
                    .and_then(|value| value.split_once('.'))
                    .is_some_and(|(_, projection)| projection == "phon") =>
            {
                refs.push(RefBindingV1 {
                    owner: branch_entry.id.clone(),
                    field: "case.equals.category".to_owned(),
                    target: reference_target(category, NodeKind::Trait, traits),
                });
            }
            crate::CaseCondition::Equals(_)
            | crate::CaseCondition::Bundle(_)
            | crate::CaseCondition::Base
            | crate::CaseCondition::Else => {}
        }
        collect_expression_refs(
            &branch.result,
            &branch_address.child(AddressSegment::CaseResult),
            None,
            entries,
            traits,
            signs,
            paradigms,
            refs,
        );
    }
}

pub(super) fn collect_refs(language: &Language, entries: &[NodeEntryV1]) -> Vec<RefBindingV1> {
    let (traits, signs, paradigms) = node_ids_by_name(language, entries);
    let mut refs = Vec::new();
    for entry in entries {
        let Some(item) = item_at(language, &entry.address) else {
            continue;
        };
        let binding = match item {
            SignItem::TraitMount {
                name,
                kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_),
                ..
            } => Some((
                "trait_use.name".to_owned(),
                reference_target(name, NodeKind::Trait, &traits),
            )),
            SignItem::TraitMount {
                name,
                kind: crate::TraitMountKind::Declaration,
                ..
            } => Some((
                "belongs".to_owned(),
                reference_target(name, NodeKind::Trait, &traits),
            )),
            SignItem::Slot(slot) => match &slot.constraint {
                SlotConstraint::Category(name) => Some((
                    "slot.constraint".to_owned(),
                    reference_target(name, NodeKind::Trait, &traits),
                )),
                SlotConstraint::AnySign => None,
            },
            SignItem::SlotMap(crate::SlotMapOp::AutoFill { filler, .. }) => Some((
                "slot_map.autofill".to_owned(),
                reference_target(filler, NodeKind::Sign, &signs),
            )),
            SignItem::SlotMap(crate::SlotMapOp::AutoFillCall { filler, .. }) => Some((
                "slot_map.autofill".to_owned(),
                callable_reference_target(&filler.callee, &signs, &paradigms),
            )),
            // `[*]` 不指名 trait,無引用可綁。
            SignItem::RoleDecl(role) => role.constraint.category().map(|category| {
                (
                    "role.constraint".to_owned(),
                    reference_target(category, NodeKind::Trait, &traits),
                )
            }),
            SignItem::Def(def) if def.path == "origin" => parse_origin(&def.value).map(|origin| {
                (
                    "origin".to_owned(),
                    reference_target(&origin.0, NodeKind::Sign, &signs),
                )
            }),
            _ => None,
        };
        if let Some((field, target)) = binding {
            refs.push(RefBindingV1 {
                owner: entry.id.clone(),
                field,
                target,
            });
        }
        match item {
            SignItem::SignExpression(expression) => collect_expression_refs(
                &expression.expression,
                &entry.address,
                Some(&entry.id),
                entries,
                &traits,
                &signs,
                &paradigms,
                &mut refs,
            ),
            SignItem::FeatureExpression(expression) => collect_expression_refs(
                &expression.expression,
                &entry.address,
                Some(&entry.id),
                entries,
                &traits,
                &signs,
                &paradigms,
                &mut refs,
            ),
            SignItem::RoleExpression(expression) => collect_expression_refs(
                &expression.expression,
                &entry.address,
                Some(&entry.id),
                entries,
                &traits,
                &signs,
                &paradigms,
                &mut refs,
            ),
            SignItem::Realization(Realization { expression: case }) => collect_case_refs(
                case,
                &entry.address,
                None,
                entries,
                &traits,
                &signs,
                &paradigms,
                &mut refs,
            ),
            _ => {}
        }
    }
    for (index, paradigm) in language.paradigms.iter().enumerate() {
        let case_address = NodeAddress(vec![
            AddressSegment::Paradigms(index),
            AddressSegment::CaseExpression,
        ]);
        let case_owner = entry_at(entries, &case_address, NodeKind::Case).map(|entry| &entry.id);
        collect_case_refs(
            &paradigm.selector,
            &case_address,
            case_owner,
            entries,
            &traits,
            &signs,
            &paradigms,
            &mut refs,
        );
    }
    refs.sort_by(|left, right| (&left.owner, &left.field).cmp(&(&right.owner, &right.field)));
    refs
}
