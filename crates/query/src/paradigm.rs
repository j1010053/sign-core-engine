//! Separate read model for horizontal Paradigm links.
//!
//! A Paradigm deliberately does not enter the lexicon as a synthetic Sign.
//! This view exposes the selector, its stable identity, and each member link
//! while the ordinary lexicon continues to contain only concrete Signs.

use conlang_language::{
    printer::format_sign_application, CaseCondition, CompiledSystem, Expression,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParadigmCatalog {
    pub entries: Vec<ParadigmEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParadigmEntry {
    /// Stable horizontal-link identity, distinct from every member Sign ID.
    pub id: String,
    pub name: String,
    pub match_features: Vec<String>,
    /// The common residual callable interface exposed to consumers.
    pub parameters: Vec<ParadigmParameter>,
    pub branches: Vec<ParadigmBranch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParadigmParameter {
    pub name: String,
    pub constraint: String,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParadigmBranchSelector {
    Bundle {
        values: Vec<Option<String>>,
    },
    Base,
    /// Defensive representation for an unchecked system. Compiled systems
    /// reject this before a query can normally observe it.
    Invalid {
        source: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParadigmBranch {
    pub selector: ParadigmBranchSelector,
    /// Canonical callable syntax, including any pre-applied arguments.
    pub application: String,
    pub member: String,
    /// The selected Sign identity. `None` is only possible for an unchecked
    /// malformed language; compilation normally guarantees `Some`.
    pub member_id: Option<String>,
}

/// Pure, deterministic Paradigm view. Entries retain their own horizontal
/// identity and point to member Sign identities instead of masquerading as
/// lexicon entries.
pub fn paradigms(system: &CompiledSystem) -> ParadigmCatalog {
    let language = system.effective_language();
    let mut entries = language
        .paradigms
        .iter()
        .filter_map(|paradigm| {
            let name = system.public_symbol_name(
                &paradigm.name,
                conlang_language::LibraryExportKind::Paradigm,
            )?;
            let parameters = system
                .callable_parameters(&name)
                .unwrap_or_default()
                .into_iter()
                .map(|parameter| ParadigmParameter {
                    name: parameter.name,
                    constraint: parameter.constraint.display_name().to_owned(),
                    optional: parameter.optional,
                })
                .collect();
            let branches = paradigm
                .selector
                .branches
                .iter()
                .map(|branch| {
                    let selector = match &branch.condition {
                        CaseCondition::Bundle(values) => ParadigmBranchSelector::Bundle {
                            values: values.clone(),
                        },
                        CaseCondition::Base => ParadigmBranchSelector::Base,
                        other => ParadigmBranchSelector::Invalid {
                            source: format!("{other:?}"),
                        },
                    };
                    match &branch.result {
                        Expression::SignApplication(application) => ParadigmBranch {
                            selector,
                            application: format_sign_application(application),
                            member: application.callee.clone(),
                            member_id: language
                                .sign_named(&application.callee)
                                .map(|sign| sign.id.to_string()),
                        },
                        other => ParadigmBranch {
                            selector,
                            application: format!("{other:?}"),
                            member: String::new(),
                            member_id: None,
                        },
                    }
                })
                .collect();
            Some(ParadigmEntry {
                id: paradigm.id.to_string(),
                name,
                match_features: paradigm.selector.match_features.clone(),
                parameters,
                branches,
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    ParadigmCatalog { entries }
}
