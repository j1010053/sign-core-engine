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
pub struct SemanticSenseV1 {
    pub name: String,
    pub gloss: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEdgeV1 {
    pub to: String,
    pub from: String,
    pub kind: String,
    pub transparency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNodeV1 {
    pub source: SemanticSourceV1,
    pub types: Vec<String>,
    pub features: BTreeMap<String, String>,
    pub roles: BTreeMap<String, SemanticNodeV1>,
    /// 自由純量語意欄位(`sem.*` Def,如 gloss)。**15a 前這些不出境**——
    /// `from_sem_node` 不讀、`into_sem_node` 給空,是已修的缺口。
    /// 空時不序列化 ⇒ 既有無欄位文件逐位元不變(v1 相容)。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    /// 義項網絡(§10.3)。空時不序列化(v1 相容)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub senses: Vec<SemanticSenseV1>,
    /// 衍生邊(§10.3)。空時不序列化(v1 相容)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<SemanticEdgeV1>,
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
            fields: node
                .fields
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            senses: node
                .senses
                .iter()
                .map(|sense| SemanticSenseV1 {
                    name: sense.name.clone(),
                    gloss: sense.gloss.clone(),
                })
                .collect(),
            edges: node
                .edges
                .iter()
                .map(|edge| SemanticEdgeV1 {
                    to: edge.to.clone(),
                    from: edge.from.clone(),
                    kind: edge.kind.keyword().to_owned(),
                    transparency: edge.transparency.keyword().to_owned(),
                })
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
            fields: self.fields.into_iter().collect(),
            roles: self
                .roles
                .into_iter()
                .map(|(name, value)| (name, value.into_sem_node()))
                .collect(),
            senses: self
                .senses
                .into_iter()
                .map(|sense| crate::sem::SenseView {
                    name: sense.name,
                    gloss: sense.gloss,
                })
                .collect(),
            edges: self
                .edges
                .into_iter()
                .filter_map(|edge| {
                    // 未知 kind/transparency 不默默近似:整條邊丟棄由呼叫端的
                    // validation 抓(DTO 是純資料邊界,不 panic)。
                    Some(crate::sem::DerivationEdge {
                        to: edge.to,
                        from: edge.from,
                        kind: crate::DerivationKind::parse(&edge.kind)?,
                        transparency: crate::SenseTransparency::parse(&edge.transparency)?,
                    })
                })
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
