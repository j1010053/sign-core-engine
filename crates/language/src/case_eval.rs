//! One control flow for typed cases. Hosts supply reads, typed results and
//! their existing default policy; a selected result can never restart selection.
use crate::{CaseBranch, CaseCondition, CaseSelection, Expression, SourceLocation, TypedCase};

#[derive(Debug, Clone, Copy)]
pub(crate) struct CaseEvent {
    pub selection: CaseSelection,
    pub branch: usize,
    pub matched: bool,
    pub source: SourceLocation,
    pub diagnostic_code: Option<&'static str>,
}

pub(crate) struct BranchValue<T> {
    pub value: T,
    pub diagnostic_code: Option<&'static str>,
}

impl<T> From<T> for BranchValue<T> {
    fn from(value: T) -> Self {
        Self {
            value,
            diagnostic_code: None,
        }
    }
}

pub(crate) struct CaseOutcome<T> {
    pub value: T,
    pub branch: Option<usize>,
    pub source: SourceLocation,
}

pub(crate) trait CaseHost {
    type Value;
    type Error;

    fn matches(&self, case: &TypedCase, condition: &CaseCondition) -> Result<bool, Self::Error>;
    fn base(&self) -> Result<Self::Value, Self::Error>;
    fn leaf(&mut self, expression: &Expression) -> Result<BranchValue<Self::Value>, Self::Error>;
    fn invalid(&self, message: String) -> Self::Error;
    fn record(&mut self, case: &TypedCase, event: CaseEvent);

    fn finish(
        &mut self,
        value: Self::Value,
        _branch: &CaseBranch,
    ) -> Result<Self::Value, Self::Error> {
        Ok(value)
    }

    fn accumulate(&mut self, _branches: &[&CaseBranch]) -> Result<Self::Value, Self::Error> {
        Err(self.invalid("`when` requires a SignContext result".to_owned()))
    }
}

pub(crate) fn evaluate<H: CaseHost>(
    case: &TypedCase,
    host: &mut H,
) -> Result<CaseOutcome<H::Value>, H::Error> {
    let event = |branch: usize, matched, diagnostic_code| CaseEvent {
        selection: case.selection,
        branch,
        matched,
        source: case.branches[branch].source,
        diagnostic_code,
    };
    if case.selection == CaseSelection::Accumulate {
        // Freeze every decision before any fragment is evaluated or merged.
        let mut any = false;
        let mut statuses = Vec::with_capacity(case.branches.len());
        for branch in &case.branches {
            let matched = if branch.condition == CaseCondition::Else {
                !any
            } else {
                host.matches(case, &branch.condition)?
            };
            any |= matched;
            statuses.push(matched);
        }
        let selected = case
            .branches
            .iter()
            .zip(&statuses)
            .filter_map(|(branch, matched)| matched.then_some(branch))
            .collect::<Vec<_>>();
        // Hosts that commit mutable traces stage the entire evaluation. Record
        // the outer selection before the inner merge, preserving trace order.
        for (index, matched) in statuses.into_iter().enumerate() {
            host.record(case, event(index, matched, None));
        }
        let value = if any {
            host.accumulate(&selected)?
        } else {
            host.base()?
        };
        return Ok(CaseOutcome {
            value,
            branch: None,
            source: case.source,
        });
    }

    for (index, branch) in case.branches.iter().enumerate() {
        if !host.matches(case, &branch.condition)? {
            host.record(case, event(index, false, None));
            continue;
        }
        let result = match &branch.result {
            Expression::ImplicitBase => host.base()?.into(),
            Expression::Case(nested) => evaluate(nested, host)?.value.into(),
            expression => host.leaf(expression)?,
        };
        let value = host.finish(result.value, branch)?;
        host.record(case, event(index, true, result.diagnostic_code));
        return Ok(CaseOutcome {
            value,
            branch: Some(index),
            source: branch.source,
        });
    }
    Ok(CaseOutcome {
        value: host.base()?,
        branch: None,
        source: SourceLocation::unknown(),
    })
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FeatureCaseError {
    #[error("feature {dim:?}.{name} is undeclared")]
    Undeclared { dim: crate::Dim, name: String },
    #[error("feature {dim:?}.{name} value {value:?} is outside enum({domain})")]
    OutOfDomain {
        dim: crate::Dim,
        name: String,
        value: String,
        domain: String,
    },
    #[error("CASE_DEFAULT_MISSING: {context} has no matching branch and no base value")]
    DefaultMissing { context: String },
    #[error("{0}")]
    Invalid(String),
}

pub(crate) struct FeatureCaseHost<M, R> {
    pub matches: M,
    pub record: R,
    pub dim: crate::Dim,
    pub name: String,
    pub domain: Vec<String>,
    pub base: Option<String>,
}

impl<M, R> CaseHost for FeatureCaseHost<M, R>
where
    M: Fn(&TypedCase, &CaseCondition) -> Result<bool, String>,
    R: FnMut(CaseEvent),
{
    type Value = Option<String>;
    type Error = FeatureCaseError;

    fn matches(&self, case: &TypedCase, condition: &CaseCondition) -> Result<bool, Self::Error> {
        (self.matches)(case, condition).map_err(FeatureCaseError::Invalid)
    }
    fn base(&self) -> Result<Self::Value, Self::Error> {
        Ok(self.base.clone())
    }
    fn leaf(&mut self, expression: &Expression) -> Result<BranchValue<Self::Value>, Self::Error> {
        let Expression::EnumValue(value) = expression else {
            return Err(self.invalid(format!(
                "feature case {}.{} returned a non-enum expression",
                self.dim.keyword(),
                self.name
            )));
        };
        if !self.domain.contains(value) {
            return Err(FeatureCaseError::OutOfDomain {
                dim: self.dim,
                name: self.name.clone(),
                value: value.clone(),
                domain: self.domain.join(", "),
            });
        }
        // An inherited base is already a feature value, potentially a set of
        // candidates. Only newly selected enum literals undergo this check.
        Ok(Some(value.clone()).into())
    }
    fn invalid(&self, message: String) -> Self::Error {
        FeatureCaseError::Invalid(message)
    }
    fn record(&mut self, _case: &TypedCase, event: CaseEvent) {
        (self.record)(event);
    }
}

/// Phon leaves share template/default/rule semantics as well as case control
/// flow. The host provides environment-dependent expansion and Sign calls.
pub(crate) trait PhonHost: CaseHost<Value = String> {
    fn expand(&self, template: &str) -> Result<String, Self::Error>;
    fn rules(&self, input: &str, rules: &[&crate::Rule]) -> Result<String, Self::Error>;
    fn interpolate(&mut self, application: &crate::SignApplication) -> Result<String, Self::Error>;
}

pub(crate) fn phon_leaf<H: PhonHost>(
    host: &mut H,
    expression: &Expression,
) -> Result<BranchValue<String>, H::Error> {
    match expression {
        Expression::PhonTemplate(template) => Ok(host.expand(template)?.into()),
        Expression::PhonInterpolation(application) => Ok(host.interpolate(application)?.into()),
        fragment @ Expression::DimFragment {
            dim: crate::Dim::Phon,
            ..
        } => {
            let input = match fragment.phon_base_template() {
                Some(template) => host.expand(template)?,
                None => host.base()?,
            };
            let rules = fragment
                .phon_branch_rules()
                .iter()
                .filter_map(|item| match item {
                    crate::SignItem::Rule(rule) if rule.dim == crate::Dim::Phon => Some(rule),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if rules.is_empty() {
                return Ok(input.into());
            }
            let output = host.rules(&input, &rules)?;
            Ok(BranchValue {
                diagnostic_code: (output == input).then_some("REALIZATION_RULES_INERT"),
                value: output,
            })
        }
        other => Err(host.invalid(format!("phon case returned non-Phon expression {other:?}"))),
    }
}
