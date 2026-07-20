//! Construction 與 slots(步驟 12b;修補07 P41/P42,I21)。
//!
//! **Construction 就是帶 slots 的 Sign**(P42):無 slot = 詞彙 sign,≥1 slot =
//! construction。**Valence = slots 結構**(P41):三價 = 三個必填 slot,不設數字
//! 欄位。填 slot → **新的 derived token**(暫態,不進庫,不就地改原 sign)。
//! filler 授權 = filler 的 **syn `belongs` 閉包**須含 slot 約束範疇(P40,複用 12a)。
//! optional slot 以 **`?`** 標記(I21)。
//!
//! phon 投影(12b 子集):construction 的 `phon` Def 是**模板**(`{slot}` 引用 +
//! 字面素材,如 `ge{stem}t` 環綴);application 代入 filler 的 UR、字面素材直通,
//! 產出 derived UR;經引擎(build_phrase → run → spell-out)得表層。完整四維
//! form-meaning 配對(derived syn/sem/prag)於 12c。

use crate::ontology::OntologyRegistry;
use crate::{Language, SignDef, SignItem, Slot};
use tshiatun_dsl::{build_phrase, run_program, surface_phrase, Program};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CxgError {
    #[error("no sign named {0:?}")]
    UnknownConstruction(String),
    #[error("sign {0:?} has no slots (not a construction)")]
    NotAConstruction(String),
    #[error("construction {construction:?} has no slot named {slot:?}")]
    UnknownSlot { construction: String, slot: String },
    #[error("slot {0:?} filled more than once")]
    DuplicateFill(String),
    #[error("no filler sign named {0:?}")]
    UnknownFiller(String),
    #[error("filler {0:?} has no phon UR (`phon = /…/`)")]
    FillerNoUr(String),
    /// filler 的 syn 範疇閉包不含 slot 要求的範疇(P40 授權失敗)。
    #[error("slot {slot:?} requires [{required}], but filler {filler:?} is not (has: {has:?})")]
    CategoryMismatch {
        slot: String,
        filler: String,
        required: String,
        has: Vec<String>,
    },
    /// 模板引用了不存在的 slot 名。
    #[error("construction {construction:?} phon template references unknown slot {slot:?}")]
    TemplateSlotUnknown { construction: String, slot: String },
    /// 對未飽和 token(仍有必填 slot 未填)求表層。
    #[error("token still needs required slot(s): {0:?}")]
    Unsaturated(Vec<String>),
    #[error("engine: {0}")]
    Engine(String),
}

/// derived token(P42:暫態,不進庫;殘餘 slots = 剩餘 valence)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedToken {
    pub construction: String,
    /// 依 construction 的 belongs 閉包(derived 是該範疇;如填滿的 PresentVerb 是 Verb)。
    pub syn_categories: Vec<String>,
    /// 已填:slot 名 → filler UR 內文(保 slot 宣告序)。
    filled: Vec<(String, String)>,
    /// phon 模板(construction 的 `phon` Def 值)。
    template: String,
    /// 未填 slots(= 剩餘 valence;必填未填 → 未飽和)。
    residual: Vec<Slot>,
}

impl DerivedToken {
    pub fn residual_slots(&self) -> &[Slot] {
        &self.residual
    }
    /// 飽和 = 無必填 slot 未填(optional 未填仍算飽和)。
    pub fn is_saturated(&self) -> bool {
        self.residual.iter().all(|s| s.optional)
    }
    /// 未飽和時的欠缺必填 slot 名。
    pub fn missing_required(&self) -> Vec<String> {
        self.residual
            .iter()
            .filter(|s| !s.optional)
            .map(|s| s.name.clone())
            .collect()
    }
    /// derived UR 形(模板代入;未填 optional → 空;未飽和 → Err)。
    pub fn phon_form(&self) -> Result<String, CxgError> {
        let missing = self.missing_required();
        if !missing.is_empty() {
            return Err(CxgError::Unsaturated(missing));
        }
        Ok(substitute(&self.template, &self.filled))
    }
}

/// 抽出一個 sign 的 slots(保宣告序)。
pub fn slots_of(sign: &SignDef) -> Vec<Slot> {
    sign.items
        .iter()
        .filter_map(|it| match it {
            SignItem::Slot(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

pub fn is_construction(sign: &SignDef) -> bool {
    sign.items.iter().any(|it| matches!(it, SignItem::Slot(_)))
}

/// sign 本地 `phon` Def 值(construction 為模板;無 → 空)。同 path 取最後(P6)。
fn phon_value(sign: &SignDef) -> String {
    sign.items
        .iter()
        .rev()
        .find_map(|it| match it {
            SignItem::Def(d) if d.path == "phon" => Some(d.value.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// filler 的 UR 內文(`/sag/` → `sag`);非 `/…/` → None。
fn ur_inner(sign: &SignDef) -> Option<String> {
    let v = phon_value(sign);
    v.strip_prefix('/')
        .and_then(|x| x.strip_suffix('/'))
        .map(str::to_owned)
}

/// `{slot}` 代入 + 字面素材直通。未填的鍵代入空(飽和性由呼叫端先驗)。
fn substitute(template: &str, filled: &[(String, String)]) -> String {
    let lookup = |name: &str| {
        filled
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    };
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
                name.push(c2);
            }
            out.push_str(lookup(name.trim()));
        } else {
            out.push(c);
        }
    }
    out
}

/// 模板引用的 slot 名集合(驗證用)。
fn template_refs(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
                name.push(c2);
            }
            out.push(name.trim().to_owned());
        }
    }
    out
}

/// Construction application(P42):construction + fillers → derived token。
/// 部分套用合法(殘餘 slots = 剩餘 valence);**不就地改任何來源 sign**。
pub fn apply(
    lang: &Language,
    reg: &OntologyRegistry,
    construction: &str,
    fillers: &[(&str, &str)],
) -> Result<DerivedToken, CxgError> {
    let cx = lang
        .sign_named(construction)
        .ok_or_else(|| CxgError::UnknownConstruction(construction.to_owned()))?;
    let slots = slots_of(cx);
    if slots.is_empty() {
        return Err(CxgError::NotAConstruction(construction.to_owned()));
    }
    // phon 模板包於 `/…/`(I22 phon 表徵);剝外層再代入 `{slot}`。
    let raw = phon_value(cx);
    let template = raw
        .strip_prefix('/')
        .and_then(|s| s.strip_suffix('/'))
        .unwrap_or(&raw)
        .to_owned();
    // 模板引用須 ⊆ slot 名
    for r in template_refs(&template) {
        if !slots.iter().any(|s| s.name == r) {
            return Err(CxgError::TemplateSlotUnknown {
                construction: construction.to_owned(),
                slot: r,
            });
        }
    }

    let mut filled: Vec<(String, String)> = Vec::new();
    for (slot_name, filler_name) in fillers {
        let slot = slots
            .iter()
            .find(|s| &s.name == slot_name)
            .ok_or_else(|| CxgError::UnknownSlot {
                construction: construction.to_owned(),
                slot: (*slot_name).to_owned(),
            })?;
        if filled.iter().any(|(k, _)| k == slot_name) {
            return Err(CxgError::DuplicateFill((*slot_name).to_owned()));
        }
        let filler = lang
            .sign_named(filler_name)
            .ok_or_else(|| CxgError::UnknownFiller((*filler_name).to_owned()))?;
        // 授權:filler 的 syn belongs 閉包須含 slot 約束(P40)
        let cats = reg.sign_categories(filler); // 分類維度中立(P38 v0.2)
        if !cats.iter().any(|c| c == &slot.filler) {
            return Err(CxgError::CategoryMismatch {
                slot: slot.name.clone(),
                filler: (*filler_name).to_owned(),
                required: slot.filler.clone(),
                has: cats,
            });
        }
        let inner =
            ur_inner(filler).ok_or_else(|| CxgError::FillerNoUr((*filler_name).to_owned()))?;
        filled.push((slot.name.clone(), inner));
    }

    let residual: Vec<Slot> = slots
        .iter()
        .filter(|s| !filled.iter().any(|(k, _)| k == &s.name))
        .cloned()
        .collect();

    Ok(DerivedToken {
        construction: construction.to_owned(),
        syn_categories: reg.sign_categories(cx),
        filled,
        template,
        residual,
    })
}

/// derived token → 表層(經引擎:build_phrase → run → spell-out)。飽和才可求。
pub fn surface(program: &Program, tok: &DerivedToken) -> Result<String, CxgError> {
    let form = tok.phon_form()?;
    let w = build_phrase(program, &form).map_err(|e| CxgError::Engine(e.to_string()))?;
    let fallback = w.clone();
    let steps = run_program(program, w).map_err(|e| CxgError::Engine(e.to_string()))?;
    let last = steps.last().map(|s| &s.word).unwrap_or(&fallback);
    match surface_phrase(program, last) {
        Ok(s) => Ok(s.replace(' ', "")),
        Err(_) => Ok(last
            .skeleton
            .iter()
            .filter_map(|s| program.env.syms.resolve(s.sym).map(str::to_owned))
            .collect()),
    }
}
