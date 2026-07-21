//! 四維 typed patch 接口(步驟 12e;修補07 P30/P39,I27)。
//!
//! **僅介面 + 資料欄位**(擁有者定案):`Sign × Patch → Sign'`([`apply`],**保留原
//! Sign**,不就地破壞);**不含 entrenchment/固化的動力學**(頻率驅動固化留 M2/B)。
//!
//! **維度隔離(P44)**:一個 [`Patch`] 綁定單一 [`Dim`],其 ops 的 path 皆該維前綴
//! (builder 自動加前綴)——型別層保證一個 syn patch 只碰 `syn.*`。**entrenchment**
//! 為跨維 meta 資料欄位(非維度),獨立 accessor / setter(唯讀視圖 + 不可變 setter)。
//!
//! 序列化:[`Patch::render`] / [`Patch::parse`] 一行文字互為 round-trip(trace / 存檔)。

use crate::{Def, Dim, SignDef, SignItem};

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
    pub dim: Dim,
    pub ops: Vec<PatchOp>,
}

impl Patch {
    pub fn for_dim(dim: Dim) -> Patch {
        Patch { dim, ops: Vec::new() }
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

    /// set 本維欄位(`field` 為 bare;自動加維度前綴,維度隔離)。builder 風格。
    pub fn set(mut self, field: &str, value: &str) -> Patch {
        self.ops.push(PatchOp::Set {
            path: format!("{}.{}", self.dim.keyword(), field),
            value: value.to_owned(),
        });
        self
    }
    /// unset 本維欄位(移除本地 Def)。
    pub fn unset(mut self, field: &str) -> Patch {
        self.ops.push(PatchOp::Unset {
            path: format!("{}.{}", self.dim.keyword(), field),
        });
        self
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
    pub fn parse(text: &str) -> Result<Patch, String> {
        let (dimkw, rest) = text.split_once(':').ok_or("patch must be `<dim>: …`")?;
        let dim = Dim::parse(dimkw.trim()).ok_or_else(|| format!("unknown dim {dimkw:?}"))?;
        let mut p = Patch::for_dim(dim);
        for seg in rest.split(';') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            if let Some(rest) = seg.strip_prefix("set ") {
                let (f, v) = rest.split_once('=').ok_or("`set` needs `field=value`")?;
                p = p.set(f.trim(), v.trim());
            } else if let Some(f) = seg.strip_prefix("unset ") {
                p = p.unset(f.trim());
            } else {
                return Err(format!("malformed op {seg:?}"));
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
                let existing = s.items.iter_mut().find_map(|it| match it {
                    SignItem::Def(d) if &d.path == path => Some(d),
                    _ => None,
                });
                match existing {
                    Some(d) => d.value = value.clone(),
                    None => s.items.push(SignItem::Def(Def {
                        path: path.clone(),
                        value: value.clone(),
                    })),
                }
            }
            PatchOp::Unset { path } => {
                s.items
                    .retain(|it| !matches!(it, SignItem::Def(d) if &d.path == path));
            }
        }
    }
    s
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
        match s.items.iter_mut().find_map(|it| match it {
            SignItem::Def(d) if d.path == "entrenchment" => Some(d),
            _ => None,
        }) {
            Some(d) => d.value = v.to_string(),
            None => s.items.push(SignItem::Def(Def {
                path: "entrenchment".to_owned(),
                value: v.to_string(),
            })),
        }
        s
    }
}
