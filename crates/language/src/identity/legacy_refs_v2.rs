//! Frozen binding for sidecars with reference_semantics = 2.
use super::*;
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
        let Some(name) = declaration_name(language, &entry.address) else {
            continue;
        };
        match entry.kind {
            NodeKind::Trait => {
                traits.insert(name, entry.id.clone());
            }
            NodeKind::Sign => {
                signs.insert(name, entry.id.clone());
            }
            NodeKind::Paradigm => {
                paradigms.insert(name, entry.id.clone());
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
    let path = crate::type_ref::TypeRef::parse(spelling)
        .map(|t| t.path())
        .unwrap_or_else(|_| spelling.into());
    match locals.get(&path) {
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
    let path = crate::type_ref::TypeRef::parse(spelling)
        .map(|t| t.path())
        .unwrap_or_else(|_| spelling.into());
    if let Some(id) = signs.get(&path) {
        RefTargetV1::Local {
            target: NodeRef::new(id.clone(), NodeKind::Sign),
        }
    } else if let Some(id) = paradigms.get(&path) {
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

fn item_at<'a>(language: &'a Language, address: &NodeAddress) -> Option<&'a SignItem> {
    let (language, local_address) = language.scope_at_address(&address.0)?;
    let address = &NodeAddress(local_address.to_vec());

    fn expression_item_at<'a>(
        expression: &'a crate::Expression,
        path: &[AddressSegment],
    ) -> Option<&'a SignItem> {
        fn case_item_at<'a>(
            case: &'a crate::TypedCase,
            path: &[AddressSegment],
        ) -> Option<&'a SignItem> {
            let [AddressSegment::CaseBranches(branch), rest @ ..] = path else {
                return None;
            };
            let result = &case.branches.get(*branch)?.result;
            match rest {
                [AddressSegment::Items(item), tail @ ..] => {
                    let items = match result {
                        crate::Expression::SignFragment(items)
                        | crate::Expression::DimFragment { items, .. } => items,
                        _ => return None,
                    };
                    nested_item_at(items.get(*item)?, tail)
                }
                [AddressSegment::CaseResult, tail @ ..] => expression_item_at(result, tail),
                _ => None,
            }
        }

        match expression {
            crate::Expression::Projection { value, .. } => expression_item_at(value, path),
            crate::Expression::Case(case) => case_item_at(case, path),
            crate::Expression::SignFragment(items)
            | crate::Expression::DimFragment { items, .. } => {
                let [AddressSegment::Items(item), tail @ ..] = path else {
                    return None;
                };
                nested_item_at(items.get(*item)?, tail)
            }
            _ => None,
        }
    }

    fn nested_item_at<'a>(item: &'a SignItem, path: &[AddressSegment]) -> Option<&'a SignItem> {
        if path.is_empty() {
            return Some(item);
        }
        match item {
            SignItem::SignExpression(expression) => {
                expression_item_at(&expression.expression, path)
            }
            SignItem::FeatureExpression(expression) => {
                expression_item_at(&expression.expression, path)
            }
            SignItem::RoleExpression(expression) => {
                expression_item_at(&expression.expression, path)
            }
            SignItem::Realization(realization) => {
                let case = &realization.expression;
                let [AddressSegment::CaseBranches(branch), rest @ ..] = path else {
                    return None;
                };
                let result = &case.branches.get(*branch)?.result;
                match rest {
                    [AddressSegment::Items(index), nested @ ..] => {
                        let items = match result {
                            crate::Expression::SignFragment(items)
                            | crate::Expression::DimFragment { items, .. } => items,
                            _ => return None,
                        };
                        nested_item_at(items.get(*index)?, nested)
                    }
                    [AddressSegment::CaseResult, nested @ ..] => expression_item_at(result, nested),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    let (root, tail) = match address.0.as_slice() {
        [AddressSegment::Signs(sign), AddressSegment::Items(item), tail @ ..] => {
            (language.signs.get(*sign)?.items.get(*item)?, tail)
        }
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block), AddressSegment::Items(item), tail @ ..] => {
            (
                language
                    .traits
                    .get(*trait_index)?
                    .blocks
                    .get(*block)?
                    .items
                    .get(*item)?,
                tail,
            )
        }
        _ => return None,
    };
    nested_item_at(root, tail)
}

fn entry_at<'a>(
    entries: &'a [NodeEntryV1],
    address: &NodeAddress,
    kind: NodeKind,
) -> Option<&'a NodeEntryV1> {
    entries
        .iter()
        .find(|entry| entry.kind == kind && &entry.address == address)
}

fn collect_application_refs(
    application: &crate::SignApplication,
    address: &NodeAddress,
    owner: &NodeId,
    entries: &[NodeEntryV1],
    traits: &BTreeMap<String, NodeId>,
    signs: &BTreeMap<String, NodeId>,
    paradigms: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
) {
    refs.push(RefBindingV1 {
        owner: owner.clone(),
        field: "application.callee".to_owned(),
        target: callable_reference_target(&application.callee, signs, paradigms),
    });
    if application.callee.contains('<') {
        collect_reference_type_parts(
            &application.callee,
            "application.type_ref",
            owner,
            traits,
            refs,
            false,
        );
    }
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
            traits,
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
                    traits,
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
            if category.contains('<') || category.contains('.') {
                collect_type_reference_refs(
                    category,
                    &format!("case.belongs[{belongs_index}].type_ref"),
                    &branch_entry.id,
                    traits,
                    refs,
                );
            }
        }
        match &branch.condition {
            crate::CaseCondition::Guard(guard) => {
                for (guard_index, category) in guard_category_references(guard) {
                    refs.push(RefBindingV1 {
                        owner: branch_entry.id.clone(),
                        field: format!("case.guard[{guard_index}].category"),
                        target: reference_target(&category, NodeKind::Trait, traits),
                    });
                    if category.contains('<') || category.contains('.') {
                        collect_type_reference_refs(
                            &category,
                            &format!("case.guard[{guard_index}].type_ref"),
                            &branch_entry.id,
                            traits,
                            refs,
                        );
                    }
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

/// Extract category-valued conjuncts from the closed guard grammar.  Scalar
/// equality conjuncts intentionally produce no Trait Ref.
fn guard_category_references(source: &str) -> Vec<(usize, String)> {
    source
        .split("&&")
        .enumerate()
        .filter_map(|(index, conjunct)| {
            let conjunct = conjunct.trim();
            let category = conjunct
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
                .or_else(|| {
                    let (field, value) = conjunct.split_once("==")?;
                    let field = field.trim();
                    let category_valued = field == "$self"
                        || field
                            .strip_prefix("$slot.")
                            .is_some_and(|slot| !slot.contains('.'));
                    category_valued.then(|| value.trim()).and_then(|value| {
                        value
                            .strip_prefix('[')
                            .and_then(|value| value.strip_suffix(']'))
                    })
                })?
                .trim();
            (!category.is_empty()).then(|| (index, category.to_owned()))
        })
        .collect()
}

fn collect_type_reference_refs(
    spelling: &str,
    field: &str,
    owner: &NodeId,
    traits: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
) {
    collect_reference_type_parts(spelling, field, owner, traits, refs, true);
}

fn collect_reference_type_parts(
    spelling: &str,
    field: &str,
    owner: &NodeId,
    traits: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
    terminal_is_trait: bool,
) {
    let Ok(reference) = crate::type_ref::TypeRef::parse(spelling) else {
        return;
    };
    fn walk(
        reference: &crate::type_ref::TypeRef,
        field: &str,
        owner: &NodeId,
        traits: &BTreeMap<String, NodeId>,
        refs: &mut Vec<RefBindingV1>,
        terminal_is_trait: bool,
    ) {
        let mut path = String::new();
        for (i, segment) in reference.segments.iter().enumerate() {
            if i != 0 {
                path.push('.');
            }
            path.push_str(&segment.name);
            if terminal_is_trait || i + 1 < reference.segments.len() {
                refs.push(RefBindingV1 {
                    owner: owner.clone(),
                    field: format!("{field}.segment.{i}"),
                    target: reference_target(&path, NodeKind::Trait, traits),
                });
            }
            for (j, argument) in segment.arguments.iter().enumerate() {
                walk(
                    &argument.value,
                    &format!("{field}.segment.{i}.argument.{j}"),
                    owner,
                    traits,
                    refs,
                    true,
                );
            }
        }
    }
    walk(&reference, field, owner, traits, refs, terminal_is_trait);
}

pub(super) fn collect_refs(language: &Language, entries: &[NodeEntryV1]) -> Vec<RefBindingV1> {
    let (traits, signs, paradigms) = node_ids_by_name(language, entries);
    let mut refs = Vec::new();
    for entry in entries {
        let (mut traits, mut signs, mut paradigms) =
            (traits.clone(), signs.clone(), paradigms.clone());
        let mut prefix = NodeAddress::root();
        let mut parameters = std::collections::BTreeSet::new();
        for segment in &entry.address.0 {
            if !matches!(segment, AddressSegment::Traits(_)) {
                break;
            }
            prefix.0.push(segment.clone());
            if let Some((scope, [AddressSegment::Traits(index)])) =
                language.scope_at_address(&prefix.0)
            {
                parameters.extend(
                    scope.traits[*index]
                        .type_params
                        .iter()
                        .map(|p| p.name.clone()),
                );
            }
            if let Some(owner) = declaration_name(language, &prefix) {
                for map in [&mut traits, &mut signs, &mut paradigms] {
                    let aliases = map
                        .iter()
                        .filter_map(|(name, id)| {
                            name.strip_prefix(&format!("{owner}."))
                                .filter(|name| !name.contains('.'))
                                .map(|name| (name.to_owned(), id.clone()))
                        })
                        .collect::<Vec<_>>();
                    map.extend(aliases);
                }
            }
        }
        for parameter in parameters {
            traits.remove(&parameter);
        }
        if entry.kind == NodeKind::Trait {
            if let Some((scope, [AddressSegment::Traits(index)])) =
                language.scope_at_address(&entry.address.0)
            {
                for (index, parameter) in scope.traits[*index].type_params.iter().enumerate() {
                    if let Some(bound) = &parameter.bound {
                        collect_type_reference_refs(
                            bound,
                            &format!("type_params.{index}.bound"),
                            &entry.id,
                            &traits,
                            &mut refs,
                        );
                    }
                }
            }
        }
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
            SignItem::Def(def) if def.path == "origin" => {
                super::legacy_refs::parse_origin(&def.value).map(|origin| {
                    (
                        "origin".to_owned(),
                        reference_target(&origin.0, NodeKind::Sign, &signs),
                    )
                })
            }
            _ => None,
        };
        if let Some((field, target)) = binding {
            refs.push(RefBindingV1 {
                owner: entry.id.clone(),
                field,
                target,
            });
        }
        let direct_type = match item {
            SignItem::TraitMount { name, args, .. } => {
                Some(crate::type_ref::TypeRef::mount(name, args).map(|r| r.to_string()))
            }
            SignItem::Slot(slot) => slot.constraint.category().map(|name| Ok(name.to_owned())),
            SignItem::RoleDecl(role) => role.constraint.category().map(|name| Ok(name.to_owned())),
            _ => None,
        };
        if let Some(Ok(name)) = direct_type {
            if name.contains('<') || name.contains('.') {
                collect_type_reference_refs(&name, "type_ref.0", &entry.id, &traits, &mut refs);
            }
        }
        match item {
            SignItem::SlotMap(crate::SlotMapOp::AutoFillCall { filler, .. })
                if filler.callee.contains('<') =>
            {
                collect_reference_type_parts(
                    &filler.callee,
                    "slot_map.type_ref",
                    &entry.id,
                    &traits,
                    &mut refs,
                    false,
                )
            }
            SignItem::SlotMap(crate::SlotMapOp::AutoFill { filler, .. })
                if filler.contains('<') =>
            {
                collect_reference_type_parts(
                    filler,
                    "slot_map.type_ref",
                    &entry.id,
                    &traits,
                    &mut refs,
                    false,
                )
            }
            _ => {}
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
    for entry in entries.iter().filter(|e| e.kind == NodeKind::Paradigm) {
        let Some((scope, [AddressSegment::Paradigms(index)])) =
            language.scope_at_address(&entry.address.0)
        else {
            continue;
        };
        let paradigm = &scope.paradigms[*index];
        let case_address = entry.address.child(AddressSegment::CaseExpression);
        let case_owner = entry_at(entries, &case_address, NodeKind::Case).map(|entry| &entry.id);
        let (mut traits, mut signs, mut paradigms) =
            (traits.clone(), signs.clone(), paradigms.clone());
        let mut prefix = NodeAddress::root();
        for segment in &entry.address.0 {
            if !matches!(segment, AddressSegment::Traits(_)) {
                break;
            }
            prefix.0.push(segment.clone());
            if let Some((scope, [AddressSegment::Traits(index)])) =
                language.scope_at_address(&prefix.0)
            {
                for parameter in &scope.traits[*index].type_params {
                    traits.remove(&parameter.name);
                }
            }
            if let Some(owner) = declaration_name(language, &prefix) {
                for map in [&mut traits, &mut signs, &mut paradigms] {
                    let aliases = map
                        .iter()
                        .filter_map(|(name, id)| {
                            name.strip_prefix(&format!("{owner}."))
                                .filter(|name| !name.contains('.'))
                                .map(|name| (name.to_owned(), id.clone()))
                        })
                        .collect::<Vec<_>>();
                    map.extend(aliases);
                }
            }
        }
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
