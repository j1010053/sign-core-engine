//! Stable semantic interchange boundary. This module intentionally contains
//! no provider, prompt, model, or mutation API.

use crate::sem::SemNode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SEMANTIC_SCHEMA_V1: &str = "conlang.semantic/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    pub sign: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNodeV1 {
    pub source: SemanticSourceV1,
    pub types: Vec<String>,
    pub features: BTreeMap<String, String>,
    pub roles: BTreeMap<String, SemanticNodeV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDocumentV1 {
    pub schema: String,
    pub root: SemanticNodeV1,
}

#[derive(Debug, thiserror::Error)]
pub enum SemanticDocumentError {
    #[error("semantic JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported semantic schema {0:?}")]
    UnknownSchema(String),
    #[error("invalid semantic document: {0}")]
    Invalid(String),
}

impl SemanticNodeV1 {
    /// Normalize the representation at the interchange boundary.  Types are
    /// a set semantically, while `BTreeMap` already gives feature and role
    /// objects a stable key order.  Recurse so a document is canonical even
    /// when it was constructed by an external caller rather than projected
    /// from an internal [`SemNode`].
    fn canonicalize(&mut self) {
        self.types.sort_unstable();
        self.types.dedup();
        for role in self.roles.values_mut() {
            role.canonicalize();
        }
    }

    pub fn from_sem_node(node: &SemNode) -> SemanticNodeV1 {
        let mut types = node.types.clone();
        types.sort_unstable();
        types.dedup();
        let mut value = SemanticNodeV1 {
            source: SemanticSourceV1 {
                package: node.source.package.clone(),
                sign: node.source.sign.clone(),
            },
            types,
            features: node.features.clone(),
            roles: node
                .roles
                .iter()
                .map(|(name, value)| (name.clone(), SemanticNodeV1::from_sem_node(value)))
                .collect(),
        };
        value.canonicalize();
        value
    }

    pub fn into_sem_node(mut self) -> SemNode {
        self.canonicalize();
        SemNode {
            types: self.types,
            features: self.features,
            source: crate::sem::SemanticSource {
                package: self.source.package,
                sign: self.source.sign,
            },
            fields: Vec::new(),
            roles: self
                .roles
                .into_iter()
                .map(|(name, value)| (name, value.into_sem_node()))
                .collect(),
        }
    }
}

impl SemanticDocumentV1 {
    fn canonicalize(&mut self) {
        self.root.canonicalize();
    }

    pub fn from_sem_node(node: &SemNode) -> SemanticDocumentV1 {
        let mut value = SemanticDocumentV1 {
            schema: SEMANTIC_SCHEMA_V1.to_owned(),
            root: SemanticNodeV1::from_sem_node(node),
        };
        value.canonicalize();
        value
    }

    pub fn to_json(&self) -> Result<String, SemanticDocumentError> {
        if self.schema != SEMANTIC_SCHEMA_V1 {
            return Err(SemanticDocumentError::UnknownSchema(self.schema.clone()));
        }
        // `SemanticDocumentV1` is public, so callers may construct or mutate
        // it without using `from_sem_node`/`from_json`.  Canonicalize a copy
        // immediately before serialization to keep the wire format stable.
        let mut canonical = self.clone();
        canonical.canonicalize();
        Ok(serde_json::to_string_pretty(&canonical)?)
    }

    pub fn from_json(source: &str) -> Result<SemanticDocumentV1, SemanticDocumentError> {
        let mut document: SemanticDocumentV1 = serde_json::from_str(source)?;
        if document.schema != SEMANTIC_SCHEMA_V1 {
            return Err(SemanticDocumentError::UnknownSchema(document.schema));
        }
        document.canonicalize();
        Ok(document)
    }
}
