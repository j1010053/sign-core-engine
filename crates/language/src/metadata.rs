//! Sign 頂層共時資料。這些欄位不屬 phon/syn/sem/prag 任一維，也不在此
//! 實作歷時或 usage 動力學；日期、文本出處、可信度等 attestation 資料不屬
//! `.lang`。

use crate::{Def, SignDef, SignItem, SignRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignProvenance {
    Native,
    Loan,
    Grammaticalized,
    Suppletive,
    Derived,
    /// 開放擴充必須顯式寫 `custom(name)`，避免自由字串拼字錯誤靜默通過。
    Custom(String),
}

impl SignProvenance {
    pub(crate) fn parse(value: &str) -> Option<SignProvenance> {
        match value {
            "native" => Some(SignProvenance::Native),
            "loan" => Some(SignProvenance::Loan),
            "grammaticalized" => Some(SignProvenance::Grammaticalized),
            "suppletive" => Some(SignProvenance::Suppletive),
            "derived" => Some(SignProvenance::Derived),
            _ => value
                .strip_prefix("custom(")
                .and_then(|inner| inner.strip_suffix(')'))
                .filter(|inner| ident_ok(inner))
                .map(|inner| SignProvenance::Custom(inner.to_owned())),
        }
    }

    fn source_value(&self) -> String {
        match self {
            SignProvenance::Native => "native".to_owned(),
            SignProvenance::Loan => "loan".to_owned(),
            SignProvenance::Grammaticalized => "grammaticalized".to_owned(),
            SignProvenance::Suppletive => "suppletive".to_owned(),
            SignProvenance::Derived => "derived".to_owned(),
            SignProvenance::Custom(name) => format!("custom({name})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignLifecycle {
    Active,
    Obsolete,
}

impl SignLifecycle {
    pub(crate) fn parse(value: &str) -> Option<SignLifecycle> {
        match value {
            "active" => Some(SignLifecycle::Active),
            "obsolete" => Some(SignLifecycle::Obsolete),
            _ => None,
        }
    }

    fn source_value(self) -> &'static str {
        match self {
            SignLifecycle::Active => "active",
            SignLifecycle::Obsolete => "obsolete",
        }
    }
}

fn ident_ok(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | ':' | '/'))
}

pub(crate) fn parse_origin(value: &str) -> Option<SignRef> {
    value
        .strip_prefix("sign(")
        .and_then(|inner| inner.strip_suffix(')'))
        .filter(|inner| ident_ok(inner))
        .map(|inner| SignRef(inner.to_owned()))
}

fn last_meta<'a>(sign: &'a SignDef, path: &str) -> Option<&'a str> {
    sign.items.iter().rev().find_map(|item| match item {
        SignItem::Def(def) if def.path == path => Some(def.value.as_str()),
        _ => None,
    })
}

fn with_meta(sign: &SignDef, path: &str, value: String) -> SignDef {
    let mut out = sign.clone();
    out.items
        .retain(|item| !matches!(item, SignItem::Def(def) if def.path == path));
    out.items.push(SignItem::Def(Def {
        path: path.to_owned(),
        value,
    }));
    out
}

impl SignDef {
    pub fn source_package(&self) -> Option<&str> {
        last_meta(self, "source_package")
    }

    pub fn origin(&self) -> Option<SignRef> {
        last_meta(self, "origin").and_then(parse_origin)
    }

    pub fn with_origin(&self, origin: SignRef) -> SignDef {
        with_meta(self, "origin", format!("sign({})", origin.0))
    }

    pub fn provenance(&self) -> Option<SignProvenance> {
        last_meta(self, "provenance").and_then(SignProvenance::parse)
    }

    pub fn with_provenance(&self, provenance: SignProvenance) -> SignDef {
        with_meta(self, "provenance", provenance.source_value())
    }

    pub fn lifecycle(&self) -> Option<SignLifecycle> {
        last_meta(self, "lifecycle").and_then(SignLifecycle::parse)
    }

    pub fn with_lifecycle(&self, lifecycle: SignLifecycle) -> SignDef {
        with_meta(self, "lifecycle", lifecycle.source_value().to_owned())
    }
}
