//! Shared case-condition lowering for Sign, token, Paradigm and filler callers.
use super::{
    guard_matches, parse_guard, token_sign, EvalContext, Guard, GuardBindings, GuardResult,
    SelfAccess, SlotAccess,
};
use crate::construction::slots_of;
use crate::ontology::OntologyRegistry;
use crate::reference;
use crate::{CaseCondition, Dim, SignDef};

/// Case syntax lowers to the existing guard evaluator, including P75 reads.
/// Wildcards perform no read; conjunctions retain left-to-right short circuiting.
fn case_guard(case: &crate::TypedCase, condition: &CaseCondition) -> Result<Guard, String> {
    let equality = |source: &str, expected: &str| -> Result<Guard, String> {
        let read = reference::parse(&reference::SCRUTINEE, source)
            .map_err(|error| format!("case scrutinee {source:?}: {error}"))?;
        match (read.subject, read.dim, read.path) {
            (reference::Subject::SelfSign, Some(dim), Some(path)) =>
                Ok(Guard::SelfFieldEq(SelfAccess { dim, path }, expected.to_owned())),
            (reference::Subject::Slot(slot), Some(dim), Some(path)) =>
                Ok(Guard::SlotFieldEq(SlotAccess { slot, dim, path }, expected.to_owned())),
            _ => Err(format!("case scrutinee {source:?} needs a field: `$slot.NAME.DIM.FIELD`; to test a filler's category use a guard case (`$slot.NAME == [Trait]`)")),
        }
    };
    match condition {
        CaseCondition::Base if case.selection == crate::CaseSelection::FirstMatch => {
            Ok(Guard::All(vec![]))
        }
        CaseCondition::Else if case.selection == crate::CaseSelection::Accumulate => {
            Ok(Guard::All(vec![]))
        }
        CaseCondition::Base | CaseCondition::Else => Err(format!(
            "{:?} uses the wrong fallback condition {condition:?}",
            case.selection
        )),
        CaseCondition::Guard(source) => parse_guard(source),
        CaseCondition::Equals(expected) => equality(
            case.scrutinee
                .as_deref()
                .ok_or("equality case is missing its scrutinee")?,
            expected,
        ),
        CaseCondition::Bundle(pattern) => {
            if pattern.len() != case.match_features.len() || case.match_features.is_empty() {
                return Err("bundle match row/header arity mismatch".to_owned());
            }
            let mut guards = Vec::new();
            for (path, expected) in case.match_features.iter().zip(pattern) {
                if let Some(expected) = expected {
                    let path = if path.starts_with('$') {
                        path.clone()
                    } else {
                        format!("$self.{path}")
                    };
                    guards.push(equality(&path, expected)?);
                }
            }
            Ok(Guard::All(guards))
        }
    }
}

pub(crate) fn sign_case_matches(
    sign: &SignDef,
    case: &crate::TypedCase,
    condition: &CaseCondition,
    registry: &OntologyRegistry,
) -> Result<bool, String> {
    let slots = slots_of(sign);
    case_matches(
        sign,
        case,
        condition,
        registry,
        &EvalContext {
            fillers: &[],
            slots: &slots,
        },
    )
}

pub(crate) fn token_case_matches(
    token: &crate::construction::DerivedToken,
    case: &crate::TypedCase,
    condition: &CaseCondition,
    registry: &OntologyRegistry,
) -> Result<bool, String> {
    let sign = token_sign(token);
    let slots = slots_of(&token.rule_sign);
    case_matches(
        &sign,
        case,
        condition,
        registry,
        &EvalContext {
            fillers: &token.fillers,
            slots: &slots,
        },
    )
}

fn case_matches(
    sign: &SignDef,
    case: &crate::TypedCase,
    condition: &CaseCondition,
    registry: &OntologyRegistry,
    context: &EvalContext<'_>,
) -> Result<bool, String> {
    let guard = case_guard(case, condition)?;
    match guard_matches(
        &guard,
        sign,
        Dim::Phon,
        registry,
        context,
        &mut Vec::new(),
        &mut Vec::new(),
        &GuardBindings::new(),
        &std::collections::BTreeMap::new(),
    ) {
        GuardResult::Matched => Ok(true),
        GuardResult::Unmatched => Ok(false),
        GuardResult::Error(error) => Err(error),
    }
}
