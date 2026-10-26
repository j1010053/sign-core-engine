//! Sign and context adapters for the shared typed-case evaluator.
use super::{CaseBranchStatus, CaseRecord, CompiledSystem, SignValue, SystemError, UnitRuleRecord};
use crate::case_eval::{self, BranchValue, CaseEvent, CaseHost, FeatureCaseError};
use crate::{CaseBranch, CaseCondition, Expression, TypedCase};

impl From<CaseEvent> for CaseRecord {
    fn from(event: CaseEvent) -> Self {
        Self {
            selection: event.selection,
            branch: event.branch,
            status: if event.matched {
                CaseBranchStatus::Matched
            } else {
                CaseBranchStatus::Unmatched
            },
            source: event.source,
            diagnostic_code: event.diagnostic_code,
            paradigm: None,
        }
    }
}

impl From<FeatureCaseError> for SystemError {
    fn from(error: FeatureCaseError) -> Self {
        match error {
            FeatureCaseError::Undeclared { dim, name } => {
                Self::UndeclaredDerivationFeature { dim, name }
            }
            FeatureCaseError::OutOfDomain {
                dim,
                name,
                value,
                domain,
            } => Self::DerivationFeatureOutOfDomain {
                dim,
                name,
                value,
                domain,
            },
            FeatureCaseError::DefaultMissing { context } => Self::CaseDefaultMissing { context },
            FeatureCaseError::Invalid(message) => Self::InvalidSignExpression(message),
        }
    }
}

struct SignCaseHost<'a> {
    system: &'a CompiledSystem,
    current: SignValue,
    stack: &'a [String],
    records: &'a mut Vec<CaseRecord>,
    rules: &'a mut Vec<UnitRuleRecord>,
}

impl CaseHost for SignCaseHost<'_> {
    type Value = SignValue;
    type Error = SystemError;

    fn matches(&self, case: &TypedCase, condition: &CaseCondition) -> Result<bool, Self::Error> {
        self.system
            .case_condition_matches(&self.current, case, condition)
    }
    fn base(&self) -> Result<SignValue, SystemError> {
        Ok(self.current.clone())
    }
    fn leaf(&mut self, expression: &Expression) -> Result<BranchValue<SignValue>, SystemError> {
        let value = match expression {
            Expression::SignApplication(application) => self.system.apply_sign_application(
                &self.current,
                application,
                self.stack,
                self.rules,
                self.records,
            )?,
            Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
                self.system.apply_sign_fragment(
                    self.current.clone(),
                    items,
                    self.records,
                    self.stack,
                    self.rules,
                )?
            }
            Expression::SelfSign => self.current.clone(),
            other => {
                return Err(
                    self.invalid(format!("Sign case returned non-Sign expression {other:?}"))
                )
            }
        };
        Ok(value.into())
    }
    fn finish(&mut self, value: SignValue, branch: &CaseBranch) -> Result<SignValue, SystemError> {
        self.system.apply_case_memberships(
            value,
            &branch.belongs,
            self.records,
            self.stack,
            self.rules,
        )
    }
    fn accumulate(&mut self, branches: &[&CaseBranch]) -> Result<SignValue, SystemError> {
        let mut items = Vec::new();
        for branch in branches {
            if !branch.belongs.is_empty() {
                return Err(self.invalid(
                    "`when` memberships must be part of its anonymous SignContext fragment"
                        .to_owned(),
                ));
            }
            match &branch.result {
                Expression::SignFragment(fragment)
                | Expression::DimFragment {
                    items: fragment, ..
                } => items.extend(fragment.iter().cloned()),
                other => {
                    return Err(self.invalid(format!(
                        "`when` returned a non-fragment expression {other:?}"
                    )))
                }
            }
        }
        self.system.apply_sign_fragment(
            self.current.clone(),
            &items,
            self.records,
            self.stack,
            self.rules,
        )
    }
    fn invalid(&self, message: String) -> SystemError {
        SystemError::InvalidSignExpression(message)
    }
    fn record(&mut self, _case: &TypedCase, event: CaseEvent) {
        self.records.push(event.into());
    }
}

impl CompiledSystem {
    pub(super) fn evaluate_sign_case(
        &self,
        current: SignValue,
        case: &TypedCase,
        records: &mut Vec<CaseRecord>,
        stack: &[String],
        rules: &mut Vec<UnitRuleRecord>,
    ) -> Result<SignValue, SystemError> {
        // Failed branches/fragment merges commit neither values nor traces.
        let mut staged_records = Vec::new();
        let mut staged_rules = Vec::new();
        let result = case_eval::evaluate(
            case,
            &mut SignCaseHost {
                system: self,
                current,
                stack,
                records: &mut staged_records,
                rules: &mut staged_rules,
            },
        )?;
        records.extend(staged_records);
        rules.extend(staged_rules);
        Ok(result.value)
    }
}
