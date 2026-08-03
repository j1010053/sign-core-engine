//! canonical printer(P21 + I22):`Language → text`,**確定性** colon+縮排 形式。
//!
//! 區段序固定:dsl 域 → distribution → global trait → trait → sign
//! (具名容器按名排序,I15-d)。容器 body **依維度分組**(I22):belongs → `Name[n]`
//! → 頂層 Def → 維度區塊(固定序 syn/phon/sem/prag);`syn:` 內 `slots:` 先於 Def;
//! 維度內 slot/Def/Rule 保插入序(有序語意)。縮排 4 空格/層。
//!
//! 這份輸出**就是** IR dump 格式(P21);對 canonical 輸入 round-trip 恆等,
//! 非 canonical 正規化為不動點(維度分組是冪等重排)。

use crate::{
    Block, CaseCondition, CaseSelection, Expression, ExpressionType, Language, SignArgumentValue,
    SignItem, SignProjection, SlotMapOp, Stage, TraitDef, TypedCase,
};

const DIMS: [&str; 4] = ["syn", "phon", "sem", "prag"];

fn stage_str(s: Stage) -> &'static str {
    match s {
        Stage::Stem => "stem",
        Stage::Word => "word",
        Stage::Phrase => "phrase",
    }
}

/// Def 的維度歸屬(path 前綴);None = 非維度(頂層,如 entrenchment)。
fn def_dim(path: &str) -> Option<&str> {
    let head = path.split_once('.').map(|(h, _)| h).unwrap_or(path);
    DIMS.contains(&head).then_some(head)
}

/// 印結構化 `PhonBlock`(P46 S2):leading block 直接印;後續冠 `Then:`/`Else:` 縮排。
fn push_phon_block(out: &mut String, block: &crate::PhonBlock, indent: &str) {
    match block {
        crate::PhonBlock::Leaf(stmts) => {
            for statement in stmts {
                out.push_str(indent);
                out.push_str(statement);
                out.push('\n');
            }
        }
        crate::PhonBlock::Then(blocks) | crate::PhonBlock::Else(blocks) => {
            let keyword = if matches!(block, crate::PhonBlock::Then(_)) {
                "Then"
            } else {
                "Else"
            };
            if let Some(first) = blocks.first() {
                push_phon_block(out, first, indent);
            }
            let inner = format!("{indent}    ");
            for sub in blocks.iter().skip(1) {
                // P46 S4: `Propagate` is a *modifier* on the element the boundary
                // introduces, so it prints as `Then propagate:` and the wrapped
                // block's own content follows (no extra nesting level).
                let (modifier, body) = match sub {
                    crate::PhonBlock::Propagate(inner) => (" propagate", inner.as_ref()),
                    other => ("", other),
                };
                out.push_str(&format!("{indent}{keyword}{modifier}:\n"));
                push_phon_block(out, body, &inner);
            }
        }
        // A `Propagate` reached directly (element 0, or a rule root) has no
        // surface form of its own — the boundary above carries the modifier.
        crate::PhonBlock::Propagate(inner) => push_phon_block(out, inner, indent),
    }
}

fn push_rule(out: &mut String, indent: &str, r: &crate::Rule) {
    // P46 S2: a structured phon block prints as `name:` + recursive block.
    if let Some(block) = &r.phon_block {
        // P46 S4: rule-level `propagate` is a header modifier (`name propagate:`).
        let modifier = if r.propagate { " propagate" } else { "" };
        out.push_str(&format!(
            "{indent}{}{modifier}:\n",
            r.name.as_deref().unwrap_or("")
        ));
        push_phon_block(out, block, &format!("{indent}    "));
        return;
    }
    // phon names use the Lexurgy-style `name:` prefix (P46 取徑 A); other
    // dimensions keep the `@name` suffix (P45).
    match (&r.name, r.dim) {
        (Some(name), crate::Dim::Phon) => out.push_str(&format!(
            "{indent}{name}: {} @stage {}\n",
            r.body,
            stage_str(r.stage)
        )),
        (Some(name), _) => out.push_str(&format!(
            "{indent}{} @name {name} @stage {}\n",
            r.body,
            stage_str(r.stage)
        )),
        (None, _) => out.push_str(&format!(
            "{indent}{} @stage {}\n",
            r.body,
            stage_str(r.stage)
        )),
    }
    for e in &r.else_chain {
        out.push_str(&format!("{indent}    else {e}\n")); // Lexurgy Else(P43)
    }
    for t in &r.then_chain {
        out.push_str(&format!("{indent}    then {t}\n")); // Lexurgy Then(I26);與 else 互斥
    }
}

fn push_slot_map(out: &mut String, operation: &SlotMapOp) {
    let source = match operation {
        SlotMapOp::Preserve { slot } => format!("map {slot} preserve"),
        SlotMapOp::Rename { slot, to } => format!("map {slot} rename {to}"),
        SlotMapOp::AutoFill { slot, filler } => format!("map {slot} autofill {filler}"),
        SlotMapOp::Internalize { slot } => format!("map {slot} internalize"),
        SlotMapOp::Optional { slot, optional } => format!("map {slot} optional {optional}"),
    };
    out.push_str(&format!("        {source}\n"));
}

fn expression_source(expression: &Expression) -> String {
    match expression {
        Expression::SignApplication(application) => {
            let arguments = application
                .arguments
                .iter()
                .map(|argument| {
                    let value = match &argument.value {
                        SignArgumentValue::SelfSign => "{$self}".to_owned(),
                        SignArgumentValue::Slot(slot) => format!("{{{slot}}}"),
                        SignArgumentValue::Application(application) => {
                            expression_source(&Expression::SignApplication((**application).clone()))
                        }
                    };
                    argument
                        .name
                        .as_ref()
                        .map(|name| format!("{name}: {value}"))
                        .unwrap_or(value)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({arguments})", application.callee)
        }
        Expression::SignFragment(_) => "<SignContext>".to_owned(),
        Expression::DimFragment { dim, .. } => format!("<{}Context>", dim.keyword()),
        Expression::PhonInterpolation(application) => format!(
            "/{{{}.phon.ret}}/",
            expression_source(&Expression::SignApplication(application.clone()))
        ),
        Expression::Projection { value, dimension } => {
            let suffix = match dimension {
                SignProjection::Phon => "phon",
                SignProjection::Syn => "syn",
                SignProjection::Sem => "sem",
                SignProjection::Prag => "prag",
            };
            format!("{}.{suffix}.ret", expression_source(value))
        }
        Expression::PhonTemplate(template) => template.clone(),
        Expression::EnumValue(value) => value.clone(),
        Expression::SelfSign => "$self".to_owned(),
        Expression::Slot(slot) => format!("{{{slot}}}"),
        // Valid nested cases are emitted structurally by `push_case`.  This
        // marker is only reachable for an invalid programmatic AST (for
        // example, a case embedded inside a scalar projection), which the
        // validator rejects without panicking.
        Expression::Case(_) => "<invalid-nested-case-position>".to_owned(),
    }
}

fn push_case(out: &mut String, indent: &str, case: &TypedCase) {
    let keyword = match case.selection {
        CaseSelection::FirstMatch => "case",
        CaseSelection::Accumulate => "when",
    };
    let case_label = case
        .name
        .as_ref()
        .map(|name| format!(" @name {name}"))
        .unwrap_or_default();
    match &case.scrutinee {
        Some(scrutinee) => out.push_str(&format!("{indent}{keyword} {scrutinee}{case_label}:\n")),
        None => out.push_str(&format!("{indent}{keyword}{case_label}:\n")),
    }
    let branch_indent = format!("{indent}    ");
    let result_indent = format!("{branch_indent}    ");
    for branch in &case.branches {
        let condition = match &branch.condition {
            CaseCondition::Equals(value) => format!("== {value}"),
            CaseCondition::Guard(guard) => guard.clone(),
            CaseCondition::Else => "else".to_owned(),
        };
        let branch_label = branch
            .name
            .as_ref()
            .map(|name| format!(" @name {name}"))
            .unwrap_or_default();
        out.push_str(&format!("{branch_indent}{condition}{branch_label}:\n"));
        match &branch.result {
            Expression::Case(nested) => push_case(out, &result_indent, nested),
            Expression::SignFragment(items) => {
                let mut fragment = String::new();
                push_body(
                    &mut fragment,
                    std::slice::from_ref(&Block {
                        items: items.clone(),
                    }),
                );
                for line in fragment.lines() {
                    out.push_str(&result_indent);
                    out.push_str(line.strip_prefix("    ").unwrap_or(line));
                    out.push('\n');
                }
            }
            Expression::DimFragment { dim, items } => {
                let mut fragment = String::new();
                push_body(
                    &mut fragment,
                    std::slice::from_ref(&Block {
                        items: items.clone(),
                    }),
                );
                let header = format!("    {}:", dim.keyword());
                for line in fragment.lines().filter(|line| *line != header) {
                    out.push_str(&result_indent);
                    out.push_str(line.strip_prefix("        ").unwrap_or(line));
                    out.push('\n');
                }
            }
            result => out.push_str(&format!("{result_indent}{}\n", expression_source(result))),
        }
        for category in &branch.belongs {
            out.push_str(&format!("{result_indent}belongs {category}\n"));
        }
    }
}

/// 印一個容器 body(統一:trait/sign 同排版)。`blocks` 保 `==` 邊界(trait)。
fn push_body(out: &mut String, blocks: &[Block]) {
    for (bi, block) in blocks.iter().enumerate() {
        if bi > 0 {
            out.push_str("    ==\n"); // P27 block 邊界
        }
        let items = &block.items;
        // 1) belongs(保序)
        for it in items {
            if let SignItem::Belongs(n) = it {
                out.push_str(&format!("    belongs {n}\n"));
            }
        }
        // 2) trait macro 引用(保序):None = 裸 Name(整個 trait)、Some(n) = Name[n]
        for it in items {
            if let SignItem::TraitUse { name, block } = it {
                match block {
                    Some(n) => out.push_str(&format!("    {name}[{n}]\n")),
                    None => out.push_str(&format!("    {name}\n")),
                }
            }
        }
        // 3) 頂層非維度 Def(保序)
        for it in items {
            if let SignItem::Def(d) = it {
                if def_dim(&d.path).is_none() {
                    out.push_str(&format!("    {} = {}\n", d.path, d.value));
                }
            }
        }
        // 4) 維度區塊(固定序 syn/phon/sem/prag)
        for dim in DIMS {
            let has_slot = dim == "syn" && items.iter().any(|it| matches!(it, SignItem::Slot(_)));
            let has_slot_map =
                dim == "syn" && items.iter().any(|it| matches!(it, SignItem::SlotMap(_)));
            let has_slot_features = dim == "syn"
                && items
                    .iter()
                    .any(|it| matches!(it, SignItem::SlotFeatureBinding(_)));
            let has_def = items
                .iter()
                .any(|it| matches!(it, SignItem::Def(d) if def_dim(&d.path) == Some(dim)));
            let has_rule = items
                .iter()
                .any(|it| matches!(it, SignItem::Rule(r) if rule_dim(r) == dim));
            let parsed_dim = crate::Dim::parse(dim).expect("known dimension");
            let has_feature = items.iter().any(|item| {
                matches!(item,
                    SignItem::FeatureDecl(feature) if feature.dim == parsed_dim
                ) || matches!(item,
                    SignItem::FeatureValue(feature) if feature.dim == parsed_dim
                ) || matches!(item,
                    SignItem::FeatureRule(rule) if rule.dim == parsed_dim
                ) || matches!(item,
                    SignItem::FeatureExpression(expression) if expression.dim == parsed_dim
                )
            });
            let has_roles = dim == "sem"
                && items.iter().any(|item| {
                    matches!(
                        item,
                        SignItem::RoleDecl(_)
                            | SignItem::RoleBinding(_)
                            | SignItem::RoleExpression(_)
                    )
                });
            // §10.3:sem 的 senses / 衍生邊。
            let has_senses =
                dim == "sem" && items.iter().any(|item| matches!(item, SignItem::Sense(_)));
            let has_sense_edges = dim == "sem"
                && items
                    .iter()
                    .any(|item| matches!(item, SignItem::SenseEdge(_)));
            let has_realization = dim == "phon"
                && items
                    .iter()
                    .any(|item| matches!(item, SignItem::Realization(_)));
            let has_context_expression = items.iter().any(|item| {
                matches!(
                    item,
                    SignItem::SignExpression(expression)
                        if matches!(
                            &expression.expression,
                            Expression::Case(case)
                                if matches!(
                                    (&case.expected, parsed_dim),
                                    (ExpressionType::SynContext, crate::Dim::Syn)
                                        | (ExpressionType::SemContext, crate::Dim::Sem)
                                        | (ExpressionType::PragContext, crate::Dim::Prag)
                                )
                        )
                )
            });
            if !(has_slot
                || has_slot_map
                || has_slot_features
                || has_def
                || has_rule
                || has_feature
                || has_roles
                || has_senses
                || has_sense_edges
                || has_realization
                || has_context_expression)
            {
                continue;
            }
            out.push_str(&format!("    {dim}:\n"));
            if has_slot {
                out.push_str("        slots:\n");
                for it in items {
                    if let SignItem::Slot(s) = it {
                        out.push_str(&format!(
                            "            {} [{}]{}\n",
                            s.name,
                            s.constraint.display_name(),
                            if s.optional { "?" } else { "" }
                        ));
                    }
                }
            }
            if has_slot_features {
                out.push_str("        slot_features:\n");
                for it in items {
                    if let SignItem::SlotFeatureBinding(binding) = it {
                        out.push_str(&format!(
                            "            {}.{} = {}\n",
                            binding.slot, binding.feature, binding.value
                        ));
                    }
                }
            }
            if has_feature {
                out.push_str("        feature:\n");
                for item in items {
                    match item {
                        SignItem::FeatureDecl(feature) if feature.dim == parsed_dim => {
                            out.push_str(&format!(
                                "            {} = enum({})\n",
                                feature.name,
                                feature.values.join(", ")
                            ));
                        }
                        SignItem::FeatureValue(feature) if feature.dim == parsed_dim => {
                            out.push_str(&format!(
                                "            {} = {}\n",
                                feature.name, feature.value
                            ));
                        }
                        SignItem::FeatureRule(rule) if rule.dim == parsed_dim => {
                            push_rule(out, "            ", rule);
                        }
                        SignItem::FeatureExpression(expression) if expression.dim == parsed_dim => {
                            out.push_str(&format!("            {} =>\n", expression.name));
                            if let Expression::Case(case) = &expression.expression {
                                push_case(out, "                ", case);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if has_roles {
                out.push_str("        roles:\n");
                for item in items {
                    match item {
                        SignItem::RoleDecl(role) => out.push_str(&format!(
                            "            {} [{}]{}\n",
                            role.name,
                            role.constraint.display_name(),
                            if role.optional { "?" } else { "" }
                        )),
                        SignItem::RoleBinding(role) => out
                            .push_str(&format!("            {} = {{{}}}\n", role.name, role.slot)),
                        SignItem::RoleExpression(role) => {
                            out.push_str(&format!("            {} =\n", role.name));
                            if let Expression::Case(case) = &role.expression {
                                push_case(out, "                ", case);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if has_senses {
                out.push_str("        senses:\n");
                for item in items {
                    if let SignItem::Sense(sense) = item {
                        out.push_str(&format!("            {} = {}\n", sense.name, sense.gloss));
                    }
                }
            }
            if has_sense_edges {
                out.push_str("        edges:\n");
                for item in items {
                    if let SignItem::SenseEdge(edge) = item {
                        // `to from from-sense kind [opaque]`;transparent 為預設,省略。
                        let transparency = match edge.transparency {
                            crate::SenseTransparency::Transparent => String::new(),
                            crate::SenseTransparency::Opaque => " opaque".to_owned(),
                        };
                        out.push_str(&format!(
                            "            {} from {} {}{}\n",
                            edge.to,
                            edge.from,
                            edge.kind.keyword(),
                            transparency
                        ));
                    }
                }
            }
            if has_realization {
                out.push_str("        realization:\n");
                for item in items {
                    if let SignItem::Realization(realization) = item {
                        if let Some(case) = &realization.expression {
                            push_case(out, "            ", case);
                        }
                    }
                }
            }
            // Def / Rule 保插入序(逐項掃,屬本維者印)
            for it in items {
                match it {
                    SignItem::Def(d) if def_dim(&d.path) == Some(dim) => {
                        let field = d.path.strip_prefix(&format!("{dim}.")).unwrap_or(&d.path);
                        if d.path == "phon" {
                            out.push_str(&format!("        {}\n", d.value)); // phon UR/模板
                        } else {
                            out.push_str(&format!("        {field} = {}\n", d.value));
                        }
                    }
                    SignItem::Rule(r) if rule_dim(r) == dim => push_rule(out, "        ", r),
                    SignItem::SlotMap(operation) if dim == "syn" => push_slot_map(out, operation),
                    _ => {}
                }
            }
            if has_context_expression {
                for item in items {
                    if let SignItem::SignExpression(expression) = item {
                        if let Expression::Case(case) = &expression.expression {
                            let expected = match parsed_dim {
                                crate::Dim::Syn => ExpressionType::SynContext,
                                crate::Dim::Sem => ExpressionType::SemContext,
                                crate::Dim::Prag => ExpressionType::PragContext,
                                crate::Dim::Phon => ExpressionType::PhonContext,
                            };
                            if case.expected == expected {
                                push_case(out, "        ", case);
                            }
                        }
                    }
                }
            }
        }
        let constraints = items
            .iter()
            .filter_map(|item| match item {
                SignItem::Constraint(constraint) => Some(constraint),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !constraints.is_empty() {
            out.push_str("    constraints:\n");
            for constraint in constraints {
                out.push_str(&format!(
                    "        {}({}, {})\n",
                    constraint.predicate.keyword(),
                    constraint.left,
                    constraint.right
                ));
            }
        }
        for item in items {
            if let SignItem::SignExpression(expression) = item {
                if let Expression::Case(case) = &expression.expression {
                    if case.expected == ExpressionType::SignContext {
                        push_case(out, "    ", case);
                    }
                }
            }
        }
    }
}

/// 規則歸維(I25/P44):由 `Rule.dim` 決定(phon/syn/sem/prag)。
fn rule_dim(r: &crate::Rule) -> &'static str {
    match r.dim {
        crate::Dim::Phon => "phon",
        crate::Dim::Syn => "syn",
        crate::Dim::Sem => "sem",
        crate::Dim::Prag => "prag",
    }
}

fn push_trait(out: &mut String, t: &TraitDef) {
    let kw = if t.global { "global trait" } else { "trait" };
    out.push_str(&format!("{kw} {}:\n", t.name));
    push_body(out, &t.blocks);
}

/// canonical 印出;空 Language → 空字串。
pub fn print(l: &Language) -> String {
    let mut sections: Vec<String> = Vec::new();

    if !l.dsl_decls.is_empty() {
        let body: String = l
            .dsl_decls
            .iter()
            .map(|line| format!("{}\n", line.trim_end()))
            .collect();
        sections.push(body);
    }
    if !l.distribution.is_empty() {
        let mut entries = l.distribution.clone();
        entries.sort();
        let mut s = String::from("distribution:\n");
        for (k, v) in entries {
            s.push_str(&format!("    {k} = {v}\n"));
        }
        sections.push(s);
    }
    // 具名容器按名排序;global trait 先於一般 trait(區段序固定)
    let mut traits: Vec<&TraitDef> = l.traits.iter().collect();
    traits.sort_by(|a, b| a.name.cmp(&b.name));
    for t in traits.iter().filter(|t| t.global) {
        let mut s = String::new();
        push_trait(&mut s, t);
        sections.push(s);
    }
    for t in traits.iter().filter(|t| !t.global) {
        let mut s = String::new();
        push_trait(&mut s, t);
        sections.push(s);
    }
    let mut signs: Vec<_> = l.signs.iter().collect();
    signs.sort_by(|a, b| a.name.cmp(&b.name));
    for sg in signs {
        let mut s = format!("sign {}:\n", sg.name);
        push_body(
            &mut s,
            std::slice::from_ref(&Block {
                items: sg.items.clone(),
            }),
        );
        sections.push(s);
    }

    sections.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::*;

    /// P21 確定性:構造順序不同 → canonical 相同(具名容器排序)。
    #[test]
    fn canonical_is_order_insensitive_for_named_containers() {
        let mk = |flip: bool| {
            let mut l = Language::new();
            let a = TraitDef {
                name: "Alpha".into(),
                global: false,
                blocks: vec![Block {
                    items: vec![SignItem::Belongs("Beta".into())],
                }],
            };
            let b = TraitDef {
                name: "Beta".into(),
                global: false,
                blocks: vec![Block::default()],
            };
            if flip {
                l.add_trait(b.clone());
                l.add_trait(a.clone());
            } else {
                l.add_trait(a);
                l.add_trait(b);
            }
            l.dump()
        };
        assert_eq!(mk(false), mk(true));
    }

    #[test]
    fn prints_syn_slot_feature_bindings_in_their_own_section() {
        let mut language = Language::new();
        language.add_sign(
            "Clause",
            vec![
                SignItem::Slot(Slot {
                    name: "target".into(),
                    constraint: SlotConstraint::Category("Nominal".into()),
                    optional: false,
                }),
                SignItem::SlotFeatureBinding(SlotFeatureBinding {
                    slot: "target".into(),
                    feature: "number".into(),
                    value: "$slot.source.syn.number".into(),
                    source: SourceLocation::line(9),
                }),
            ],
        );

        assert_eq!(
            language.dump(),
            r#"sign Clause:
    syn:
        slots:
            target [Nominal]
        slot_features:
            target.number = $slot.source.syn.number
"#
        );
    }
}
