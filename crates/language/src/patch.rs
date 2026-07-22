//! 四維 typed patch 接口(步驟 12e;修補07 P30/P39,I27)。
//!
//! **僅介面 + 資料欄位**(擁有者定案):`Sign × Patch → Sign'`([`apply`],**保留原
//! Sign**,不就地破壞);**不含 entrenchment/固化的動力學**(頻率驅動固化留 M2/B)。
//!
//! **維度隔離(P44)**:一個 [`Patch`] 綁定單一 [`Dim`],其 ops 的 path 皆該維前綴
//! (validated builder 自動加前綴，欄位私有)——一個 syn patch 只碰 `syn.*`。**entrenchment**
//! 為跨維 meta 資料欄位(非維度),獨立 accessor / setter(唯讀視圖 + 不可變 setter)。
//!
//! 序列化:[`Patch::render`] / [`Patch::parse`] 一行文字互為 round-trip(trace / 存檔)。

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, Severity};
use crate::path::parse_path;
use crate::{Def, Dim, FeatureValue, SignDef, SignItem};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatchError {
    #[error("patch field must be a non-empty relative path, got {0:?}")]
    InvalidField(String),
    #[error("patch for {expected:?} cannot contain path {path:?}")]
    CrossDimension { expected: Dim, path: String },
    #[error("malformed patch operation {0:?}")]
    MalformedOperation(String),
}

impl PatchError {
    pub fn code(&self) -> &'static str {
        match self {
            PatchError::InvalidField(_) => "PATCH_INVALID_FIELD",
            PatchError::CrossDimension { .. } => "PATCH_CROSS_DIMENSION",
            PatchError::MalformedOperation(_) => "PATCH_MALFORMED_OPERATION",
        }
    }

    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(Severity::Error, self.code(), self.to_string())
    }
}

impl From<PatchError> for Diagnostic {
    fn from(error: PatchError) -> Diagnostic {
        error.diagnostic()
    }
}

/// patch 單一操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOp {
    /// upsert:設 `path = value`(有則改值、無則加本地 Def)。
    Set { path: String, value: String },
    /// delete:移除該 path 的**本地** Def(繼承值不受影響,由 projection 重算)。
    Unset { path: String },
}

/// 某維 typed patch(P30/P39)。`dim` 綁定 + ops path 皆該維前綴(維度隔離)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    dim: Dim,
    ops: Vec<PatchOp>,
}

impl Patch {
    pub fn for_dim(dim: Dim) -> Patch {
        Patch {
            dim,
            ops: Vec::new(),
        }
    }
    /// 四維具名建構(SynPatch/PhonPatch/SemPatch/PragPatch 的統一形)。
    pub fn syn() -> Patch {
        Patch::for_dim(Dim::Syn)
    }
    pub fn phon() -> Patch {
        Patch::for_dim(Dim::Phon)
    }
    pub fn sem() -> Patch {
        Patch::for_dim(Dim::Sem)
    }
    pub fn prag() -> Patch {
        Patch::for_dim(Dim::Prag)
    }

    pub fn dim(&self) -> Dim {
        self.dim
    }

    pub fn ops(&self) -> &[PatchOp] {
        &self.ops
    }

    fn checked_path(&self, field: &str) -> Result<String, PatchError> {
        let field = field.trim();
        if field.is_empty() || parse_path(field).is_err() {
            return Err(PatchError::InvalidField(field.to_owned()));
        }
        if self.dim == Dim::Phon && field == "phon" {
            return Ok("phon".to_owned());
        }
        let first = field.split(['.', '[', '~']).next().unwrap_or_default();
        if Dim::parse(first).is_some() || matches!(first, "entrenchment" | "lexicalized") {
            return Err(PatchError::CrossDimension {
                expected: self.dim,
                path: field.to_owned(),
            });
        }
        Ok(format!("{}.{}", self.dim.keyword(), field))
    }

    /// set 本維欄位(`field` 為 bare;自動加維度前綴,維度隔離)。builder 風格。
    pub fn set(mut self, field: &str, value: &str) -> Patch {
        let path = self
            .checked_path(field)
            .unwrap_or_else(|e| panic!("invalid typed patch: {e}"));
        self.ops.push(PatchOp::Set {
            path,
            value: value.to_owned(),
        });
        self
    }

    /// Fallible form for user-provided field names. Invalid input never yields
    /// a [`Patch`], so callers cannot construct a cross-dimensional patch.
    pub fn try_set(mut self, field: &str, value: &str) -> Result<Patch, PatchError> {
        let path = self.checked_path(field)?;
        self.ops.push(PatchOp::Set {
            path,
            value: value.to_owned(),
        });
        Ok(self)
    }
    /// unset 本維欄位(移除本地 Def)。
    pub fn unset(mut self, field: &str) -> Patch {
        let path = self
            .checked_path(field)
            .unwrap_or_else(|e| panic!("invalid typed patch: {e}"));
        self.ops.push(PatchOp::Unset { path });
        self
    }

    pub fn try_unset(mut self, field: &str) -> Result<Patch, PatchError> {
        let path = self.checked_path(field)?;
        self.ops.push(PatchOp::Unset { path });
        Ok(self)
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// 套用於 sign(P30):**保留原 Sign**;Set upsert、Unset 移除本地 Def。
    pub fn apply(&self, sign: &SignDef) -> SignDef {
        apply(sign, self)
    }

    /// 一行序列化(trace / 存檔):`syn: set class=verb; unset old`。
    pub fn render(&self) -> String {
        let mut s = format!("{}:", self.dim.keyword());
        let pfx = format!("{}.", self.dim.keyword());
        for (i, op) in self.ops.iter().enumerate() {
            s.push_str(if i == 0 { " " } else { "; " });
            match op {
                PatchOp::Set { path, value } => {
                    let f = path.strip_prefix(&pfx).unwrap_or(path);
                    s.push_str(&format!("set {f}={value}"));
                }
                PatchOp::Unset { path } => {
                    let f = path.strip_prefix(&pfx).unwrap_or(path);
                    s.push_str(&format!("unset {f}"));
                }
            }
        }
        s
    }

    /// 反序列化(round-trip:`parse(render(p)) == p`)。
    pub fn parse(text: &str) -> Result<Patch, PatchError> {
        let (dimkw, rest) = text
            .split_once(':')
            .ok_or_else(|| PatchError::MalformedOperation("patch must be `<dim>: …`".into()))?;
        let dim = Dim::parse(dimkw.trim())
            .ok_or_else(|| PatchError::MalformedOperation(format!("unknown dim {dimkw:?}")))?;
        let mut p = Patch::for_dim(dim);
        for seg in rest.split(';') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            if let Some(rest) = seg.strip_prefix("set ") {
                let (f, v) = rest.split_once('=').ok_or_else(|| {
                    PatchError::MalformedOperation("`set` needs `field=value`".into())
                })?;
                p = p.try_set(f.trim(), v.trim())?;
            } else if let Some(f) = seg.strip_prefix("unset ") {
                p = p.try_unset(f.trim())?;
            } else {
                return Err(PatchError::MalformedOperation(seg.to_owned()));
            }
        }
        Ok(p)
    }
}

/// `Sign × Patch → Sign'`(P30):保留原 Sign。Set upsert 本地 Def、Unset 移除本地 Def。
pub fn apply(sign: &SignDef, patch: &Patch) -> SignDef {
    let mut s = sign.clone();
    for op in &patch.ops {
        match op {
            PatchOp::Set { path, value } => {
                // Effective Def semantics are last-wins. Remove every stale
                // local occurrence and append one winner at the newest point.
                // Preserve a typed feature assignment when the path already
                // has one; otherwise a generic Def remains the representation.
                let typed = s.items.iter().find_map(|item| match item {
                    SignItem::FeatureValue(feature)
                        if format!("{}.{}", feature.dim.keyword(), feature.name) == *path =>
                    {
                        Some(feature.clone())
                    }
                    _ => None,
                });
                s.items.retain(|item| match item {
                    SignItem::Def(def) => &def.path != path,
                    SignItem::FeatureValue(feature) => {
                        format!("{}.{}", feature.dim.keyword(), feature.name) != *path
                    }
                    _ => true,
                });
                if let Some(feature) = typed {
                    s.items.push(SignItem::FeatureValue(FeatureValue {
                        value: value.clone(),
                        ..feature
                    }));
                } else {
                    s.items.push(SignItem::Def(Def {
                        path: path.clone(),
                        value: value.clone(),
                    }));
                }
            }
            PatchOp::Unset { path } => {
                s.items.retain(|item| match item {
                    SignItem::Def(def) => &def.path != path,
                    SignItem::FeatureValue(feature) => {
                        format!("{}.{}", feature.dim.keyword(), feature.name) != *path
                    }
                    _ => true,
                });
            }
        }
    }
    s
}

fn local_dim_values(sign: &SignDef, dim: Dim) -> BTreeMap<String, String> {
    let prefix = format!("{}.", dim.keyword());
    let mut out = BTreeMap::new();
    for item in &sign.items {
        match item {
            SignItem::Def(def) if def.path == dim.keyword() || def.path.starts_with(&prefix) => {
                out.insert(def.path.clone(), def.value.clone());
            }
            SignItem::FeatureValue(feature) if feature.dim == dim => {
                out.insert(
                    format!("{}.{}", dim.keyword(), feature.name),
                    feature.value.clone(),
                );
            }
            _ => {}
        }
    }
    out
}

/// Compute a per-dimension patch between local Def states. Applying the
/// result makes the target dimension observationally equal to `after` while
/// leaving the other three dimensions and metadata untouched.
pub fn diff(before: &SignDef, after: &SignDef, dim: Dim) -> Patch {
    let old = local_dim_values(before, dim);
    let new = local_dim_values(after, dim);
    let mut patch = Patch::for_dim(dim);
    let prefix = format!("{}.", dim.keyword());
    let keys: BTreeSet<_> = old.keys().chain(new.keys()).cloned().collect();
    for path in keys {
        match (old.get(&path), new.get(&path)) {
            (Some(_), None) if path == dim.keyword() => {
                patch.ops.push(PatchOp::Unset { path });
            }
            (left, Some(value)) if left != Some(value) && path == dim.keyword() => {
                patch.ops.push(PatchOp::Set {
                    path,
                    value: value.clone(),
                });
            }
            (Some(_), None) => {
                let field = path.strip_prefix(&prefix).unwrap_or(&path);
                patch = patch.unset(field);
            }
            (left, Some(value)) if left != Some(value) => {
                let field = path.strip_prefix(&prefix).unwrap_or(&path);
                patch = patch.set(field, value);
            }
            _ => {}
        }
    }
    patch
}

// ── entrenchment:跨維 meta 資料欄位(12e 僅介面/欄位,無固化動力學)──

impl SignDef {
    /// 固著度(usage-based;07 §4)。**僅資料**:讀 `entrenchment` Def 解析為 f64;
    /// 頻率驅動固化(threshold/fossilization)= B 引擎(M2 後),此處不實作。
    pub fn entrenchment(&self) -> Option<f64> {
        self.items.iter().rev().find_map(|it| match it {
            SignItem::Def(d) if d.path == "entrenchment" => d.value.parse().ok(),
            _ => None,
        })
    }

    /// 不可變設定固著度(upsert `entrenchment` 頂層 Def;保留原 Sign)。
    pub fn with_entrenchment(&self, v: f64) -> SignDef {
        let mut s = self.clone();
        s.items
            .retain(|it| !matches!(it, SignItem::Def(d) if d.path == "entrenchment"));
        s.items.push(SignItem::Def(Def {
            path: "entrenchment".to_owned(),
            value: v.to_string(),
        }));
        s
    }

    /// Whether this sign has been lexicalized. This is data only; no usage
    /// threshold or diachronic transition is run in M1++.
    pub fn lexicalized(&self) -> Option<bool> {
        self.items.iter().rev().find_map(|it| match it {
            SignItem::Def(d) if d.path == "lexicalized" => match d.value.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            _ => None,
        })
    }

    /// Immutable data setter for `lexicalized`.
    pub fn with_lexicalized(&self, value: bool) -> SignDef {
        let mut s = self.clone();
        s.items
            .retain(|it| !matches!(it, SignItem::Def(d) if d.path == "lexicalized"));
        s.items.push(SignItem::Def(Def {
            path: "lexicalized".to_owned(),
            value: value.to_string(),
        }));
        s
    }
}
