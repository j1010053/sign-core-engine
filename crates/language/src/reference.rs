//! 統一的 `$` 引用文法(Phase 0)。
//!
//! `$self` / `$slot.` 的解析原本散在八個位置,每處各自做前綴比對,接受的
//! 子集互不相同(有的鎖死維度、有的鎖死路徑長度、有的允許省略 sigil)。
//! 本模組把那套文法收成單一實作:
//!
//! ```text
//! Reference := Subject ( '.' Dim ( '.' Path )? )?
//! Subject   := '$self' | '$slot' '.' Name | Name
//! ```
//!
//! 呼叫點不再自己比對前綴,而是宣告一份 [`RefSpec`]——「我接受哪個子集」。
//! 接受/拒絕的集合因此變成**可讀的宣告**,而不是散落的 `strip_prefix` 串接。
//!
//! **Phase 0 不改任何接受/拒絕的集合,也不改 canonical 印出。**
//! [`Reference::render`] 會還原輸入當時的寫法(見 `explicit_sigil`),所以
//! rename 之後排出來的原始碼與舊實作逐字相同,`base_source` digest 不動。
//! 統一印出面(裸形一律補回 `$`)是 Phase 1,需 P 系列裁定。
//!
//! **不在本模組範圍**:`.chg` function 層的 guard 主體(`x.syn.category`)。
//! 那裡的 `x` 是 function 參數而非 slot,主體命名空間不同;要不要與此合流
//! 是 Phase 1 的語意決定,不是 Phase 0 的重構。

use crate::path::parse_path;
use crate::Dim;

/// 求值時才綁定的主體。`$` 引用永不跨 sign 邊界——只有這兩個。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// `$self`:當前 sign。
    SelfSign,
    /// `$slot.<name>`:某個 slot 的填充者。
    Slot(String),
}

/// 一個解析完成的 `$` 引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub subject: Subject,
    /// 主體之後的維度。`None` = 引用主體自身(如 `$self == [Trait]`)。
    pub dim: Option<Dim>,
    /// 維度之後的欄位路徑,**不含**維度前綴。`None` = 只到維度為止。
    pub path: Option<String>,
    /// 來源是否顯式寫了 sigil。`render` 依此還原原樣,故 Phase 0 的
    /// canonical 輸出逐字不變。
    explicit_sigil: bool,
}

/// 省略 sigil 時如何決定主體。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sigil {
    /// 必須寫 `$self` / `$slot.`。
    Required,
    /// 可省略;省略時主體是**同名 slot**。
    OptionalSlot,
    /// 可省略;省略時主體是 `$self`(故首段必須是維度關鍵字)。
    OptionalSelf,
    /// 可省略;省略時看首段:**是維度關鍵字 → `$self`,否則 → 同名 slot**。
    ///
    /// 這條是 case scrutinee 的既有行為:`syn.number` 讀自己的欄位,
    /// `stem.phon` 讀 slot `stem`。把判準寫在這裡,而不是讓兩個呼叫點
    /// 各自用 `split_once` 猜。
    OptionalHeadDim,
}

/// 該位置對「維度」那一段的要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimPolicy {
    Forbidden,
    Optional,
    Required,
    /// 必填,且只能是這一維。
    Only(Dim),
}

/// 該位置對「路徑」那一段的要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPolicy {
    Forbidden,
    /// 可有可無;不額外驗證。
    Optional,
    /// 必填,且與維度前綴合併後須通過 [`parse_path`]。
    RequiredValidated,
    /// 必填且恰 n 段,每段為裸識別字。(slot 名的識別字要求另由
    /// [`RefSpec::ident_subject`] 決定,兩者互不牽連。)
    Identifiers(usize),
}

/// 一個呼叫點接受的引用子集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefSpec {
    pub sigil: Sigil,
    pub allow_self: bool,
    pub allow_slot: bool,
    /// slot 名是否必須是裸識別字(不含 `.`、空白、括號)。
    pub ident_subject: bool,
    pub dim: DimPolicy,
    pub path: PathPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefError {
    /// 該位置要求顯式 sigil,但輸入沒有。
    MissingSigil,
    /// 主體型別在該位置不允許(例如只收 slot 的位置寫了 `$self`)。
    SubjectNotAllowed,
    EmptySlotName,
    /// slot 名不是裸識別字(僅 [`RefSpec::ident_subject`] 為真時檢查)。
    SlotNotIdentifier,
    UnknownDim(String),
    DimRequired,
    DimForbidden,
    WrongDim(Dim),
    PathRequired,
    PathForbidden,
    WrongSegmentCount(usize),
    PathNotIdentifier,
    BadPath(String),
}

impl std::fmt::Display for RefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefError::MissingSigil => f.write_str("expected `$self` or `$slot.` prefix"),
            RefError::SubjectNotAllowed => f.write_str("this subject is not allowed here"),
            RefError::EmptySlotName => f.write_str("`$slot.` needs a slot name"),
            RefError::SlotNotIdentifier => f.write_str("slot name must be an identifier"),
            RefError::UnknownDim(found) => write!(f, "unknown dimension {found:?}"),
            RefError::DimRequired => f.write_str("a phon|syn|sem|prag dimension is required"),
            RefError::DimForbidden => f.write_str("no dimension is allowed here"),
            RefError::WrongDim(dim) => write!(f, "only the {} dimension is allowed", dim.keyword()),
            RefError::PathRequired => f.write_str("a field path is required"),
            RefError::PathForbidden => f.write_str("no field path is allowed here"),
            RefError::WrongSegmentCount(n) => write!(f, "path must have exactly {n} segment(s)"),
            RefError::PathNotIdentifier => f.write_str("path segments must be identifiers"),
            RefError::BadPath(msg) => f.write_str(msg),
        }
    }
}

fn ident_ok(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
}

/// 切出主體,回傳 (主體, 是否顯式寫了 sigil, 剩下的尾段)。
fn split_subject<'a>(spec: &RefSpec, value: &'a str) -> Result<(Subject, bool, &'a str), RefError> {
    if value == "$self" {
        return Ok((Subject::SelfSign, true, ""));
    }
    if let Some(rest) = value.strip_prefix("$self.") {
        return Ok((Subject::SelfSign, true, rest));
    }
    if let Some(rest) = value.strip_prefix("$slot.") {
        let (name, tail) = match rest.split_once('.') {
            Some((name, tail)) => (name, tail),
            None => (rest, ""),
        };
        if name.is_empty() {
            return Err(RefError::EmptySlotName);
        }
        return Ok((Subject::Slot(name.to_owned()), true, tail));
    }
    let head = value.split('.').next().unwrap_or_default();
    match spec.sigil {
        Sigil::Required => Err(RefError::MissingSigil),
        Sigil::OptionalSelf => Ok((Subject::SelfSign, false, value)),
        Sigil::OptionalSlot => {
            if head.is_empty() {
                return Err(RefError::EmptySlotName);
            }
            let tail = value.get(head.len() + 1..).unwrap_or("");
            Ok((Subject::Slot(head.to_owned()), false, tail))
        }
        Sigil::OptionalHeadDim => {
            if Dim::parse(head).is_some() {
                Ok((Subject::SelfSign, false, value))
            } else {
                if head.is_empty() {
                    return Err(RefError::EmptySlotName);
                }
                let tail = value.get(head.len() + 1..).unwrap_or("");
                Ok((Subject::Slot(head.to_owned()), false, tail))
            }
        }
    }
}

// ── 共用的 spec:同一個形狀原本被四處各抄一份 ──────────────────────────

/// `slot_features:` 的凍結 syn 讀取:`$slot.<name>.syn.<feature>`。
/// 維度鎖死 syn、路徑恰一段,是這個位置的既有契約(P41)。
pub const SLOT_SYN_FEATURE: RefSpec = RefSpec {
    sigil: Sigil::Required,
    allow_self: false,
    allow_slot: true,
    ident_subject: true,
    dim: DimPolicy::Only(Dim::Syn),
    path: PathPolicy::Identifiers(1),
};

/// `constraints:` 的欄位運算元:同上,但 `$slot.` 可省。
pub const OPTIONAL_SLOT_SYN_FEATURE: RefSpec = RefSpec {
    sigil: Sigil::OptionalSlot,
    allow_self: false,
    allow_slot: true,
    ident_subject: false,
    dim: DimPolicy::Only(Dim::Syn),
    path: PathPolicy::Identifiers(1),
};

/// `constraints:` 的整體運算元:可以只是一個 slot 名(`before(a, b)`),
/// 也可以帶維度與欄位。只用來取出主體 slot。
pub const CONSTRAINT_OPERAND: RefSpec = RefSpec {
    sigil: Sigil::OptionalSlot,
    allow_self: false,
    allow_slot: true,
    ident_subject: false,
    dim: DimPolicy::Optional,
    path: PathPolicy::Optional,
};

/// `$self.<dim>.<field>` 的純量讀取,`$self.` 可省。
pub const SELF_SCALAR: RefSpec = RefSpec {
    sigil: Sigil::OptionalSelf,
    allow_self: true,
    allow_slot: false,
    ident_subject: false,
    dim: DimPolicy::Required,
    path: PathPolicy::Optional,
};

/// case scrutinee:首段是維度 → 讀自己的欄位;否則 → 讀同名 slot 的投影。
pub const SCRUTINEE: RefSpec = RefSpec {
    sigil: Sigil::OptionalHeadDim,
    allow_self: true,
    allow_slot: true,
    ident_subject: false,
    dim: DimPolicy::Required,
    path: PathPolicy::Optional,
};

/// application 實參與模板鍵:只有主體,沒有維度與路徑。
pub const SUBJECT_ONLY: RefSpec = RefSpec {
    sigil: Sigil::OptionalSlot,
    allow_self: true,
    allow_slot: true,
    ident_subject: true,
    dim: DimPolicy::Forbidden,
    path: PathPolicy::Forbidden,
};

/// 剝掉 case scrutinee 的遺留 `$self.` 前綴。
///
/// scrutinee 位置的 `$self.` 是**寫了也沒用**的裝飾:舊實作一律先 strip 再把
/// 首段當 slot 解讀,所以 `$self.stem.phon` 與 `stem.phon` 完全等價。這個行為
/// 本身可疑(見 Phase 1),但 Phase 0 只負責把它從兩個呼叫點收成一處,不改它。
pub fn strip_legacy_self_prefix(value: &str) -> &str {
    let value = value.trim();
    value.strip_prefix("$self.").unwrap_or(value)
}

/// 依 `spec` 解析一個 `$` 引用。
pub fn parse(spec: &RefSpec, value: &str) -> Result<Reference, RefError> {
    let value = value.trim();
    let (subject, explicit_sigil, tail) = split_subject(spec, value)?;
    match &subject {
        Subject::SelfSign if !spec.allow_self => return Err(RefError::SubjectNotAllowed),
        Subject::Slot(_) if !spec.allow_slot => return Err(RefError::SubjectNotAllowed),
        Subject::Slot(name) if spec.ident_subject && !ident_ok(name) => {
            return Err(RefError::SlotNotIdentifier)
        }
        _ => {}
    }

    let (dim, path) = if tail.is_empty() {
        (None, None)
    } else {
        let (head, rest) = match tail.split_once('.') {
            Some((head, rest)) => (head, Some(rest)),
            None => (tail, None),
        };
        let dim = Dim::parse(head).ok_or_else(|| RefError::UnknownDim(head.to_owned()))?;
        (Some(dim), rest.filter(|rest| !rest.is_empty()))
    };

    match spec.dim {
        DimPolicy::Forbidden if dim.is_some() => return Err(RefError::DimForbidden),
        DimPolicy::Required | DimPolicy::Only(_) if dim.is_none() => {
            return Err(RefError::DimRequired)
        }
        DimPolicy::Only(expected) if dim != Some(expected) => {
            return Err(RefError::WrongDim(expected))
        }
        _ => {}
    }

    match spec.path {
        PathPolicy::Forbidden if path.is_some() => return Err(RefError::PathForbidden),
        PathPolicy::RequiredValidated | PathPolicy::Identifiers(_) if path.is_none() => {
            return Err(RefError::PathRequired)
        }
        PathPolicy::RequiredValidated => {
            let dim = dim.ok_or(RefError::DimRequired)?;
            let path = path.expect("checked above");
            parse_path(&format!("{}.{}", dim.keyword(), path))
                .map_err(|error| RefError::BadPath(error.to_string()))?;
        }
        PathPolicy::Identifiers(expected) => {
            let path = path.expect("checked above");
            let segments = path.split('.').collect::<Vec<_>>();
            if segments.len() != expected {
                return Err(RefError::WrongSegmentCount(expected));
            }
            if !segments.iter().all(|segment| ident_ok(segment)) {
                return Err(RefError::PathNotIdentifier);
            }
        }
        _ => {}
    }

    Ok(Reference {
        subject,
        dim,
        path: path.map(str::to_owned),
        explicit_sigil,
    })
}

impl Reference {
    /// 引用的 slot 名(主體是 `$self` 時為 `None`)。
    pub fn slot(&self) -> Option<&str> {
        match &self.subject {
            Subject::Slot(name) => Some(name),
            Subject::SelfSign => None,
        }
    }

    /// 維度 + 路徑合併成的完整欄位路徑(`syn.number`)。無維度時為 `None`。
    pub fn dim_path(&self) -> Option<String> {
        let dim = self.dim?;
        Some(match &self.path {
            Some(path) => format!("{}.{}", dim.keyword(), path),
            None => dim.keyword().to_owned(),
        })
    }

    /// 換掉 slot 名。主體不是該 slot 時原樣回傳。
    pub fn with_renamed_slot(&self, old: &str, new: &str) -> Reference {
        match &self.subject {
            Subject::Slot(name) if name == old => Reference {
                subject: Subject::Slot(new.to_owned()),
                ..self.clone()
            },
            _ => self.clone(),
        }
    }

    /// 還原成原始寫法——**顯式 sigil 進、顯式 sigil 出;裸形進、裸形出**。
    /// Phase 0 靠這條保住 canonical 逐字不變。
    pub fn render(&self) -> String {
        let mut out = match (&self.subject, self.explicit_sigil) {
            (Subject::SelfSign, true) => "$self".to_owned(),
            (Subject::SelfSign, false) => String::new(),
            (Subject::Slot(name), true) => format!("$slot.{name}"),
            (Subject::Slot(name), false) => name.clone(),
        };
        if let Some(dim) = self.dim {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(dim.keyword());
        }
        if let Some(path) = &self.path {
            out.push('.');
            out.push_str(path);
        }
        out
    }
}

// ── rename:兩個入口,依「整串 vs 內嵌」區分 ────────────────────────────
//
// 這個二分**是對的,不是巧合**:整串的位置(constraint 運算元、scrutinee)
// 可以安全地把裸形 `stem.syn.f` 當成 slot 引用;內嵌的位置(規則本文、模板)
// 不行——那裡的 `stem.` 可能是任何東西,只有顯式 `$slot.` 與 `{…}` 才是引用。
// 舊實作有三個重寫器,而「哪個位置該套哪幾個」全靠呼叫點自己記得。

/// 整串就是一個引用時的 rename(constraint 運算元、case scrutinee)。
///
/// 解析不出來就原樣回傳——這個位置的合法性由各自的驗證負責,rename 不越權。
pub fn rename_slot_in_operand(source: &str, old: &str, new: &str) -> String {
    const SPEC: RefSpec = RefSpec {
        sigil: Sigil::OptionalSlot,
        allow_self: true,
        allow_slot: true,
        ident_subject: false,
        dim: DimPolicy::Optional,
        path: PathPolicy::Optional,
    };
    match parse(&SPEC, source) {
        Ok(reference) if reference.slot() == Some(old) => {
            let renamed = reference.with_renamed_slot(old, new);
            let rendered = renamed.render();
            // 前後空白不是引用的一部分,原樣保留。
            let leading = &source[..source.len() - source.trim_start().len()];
            let trailing = &source[source.trim_end().len()..];
            format!("{leading}{rendered}{trailing}")
        }
        _ => source.to_owned(),
    }
}

/// 引用內嵌在自由文字裡時的 rename(規則本文、guard、Def 值、phon 模板)。
///
/// 認兩種顯式寫法:`$slot.<name>` 與模板的 `{<name>}`。取代舊的
/// `rewrite_slot_accesses` + `rewrite_slot_template` 兩個重寫器。
pub fn rename_slot_in_text(source: &str, old: &str, new: &str) -> String {
    let templated = source.replace(&format!("{{{old}}}"), &format!("{{{new}}}"));
    rename_slot_accesses(&templated, old, new)
}

fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | ':' | '/')
}

fn rename_slot_accesses(source: &str, old: &str, new: &str) -> String {
    let needle = format!("$slot.{old}");
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(offset) = source[cursor..].find(&needle) {
        let start = cursor + offset;
        let end = start + needle.len();
        // 只有整個 slot 名相符才算命中:`$slot.stem` 不得吃掉 `$slot.stemma`。
        let boundary = source[end..]
            .chars()
            .next()
            .is_none_or(|character| character == '.' || !is_identifier_character(character));
        output.push_str(&source[cursor..start]);
        if boundary {
            output.push_str("$slot.");
            output.push_str(new);
        } else {
            output.push_str(&source[start..end]);
        }
        cursor = end;
    }
    output.push_str(&source[cursor..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRICT: RefSpec = RefSpec {
        sigil: Sigil::Required,
        allow_self: true,
        allow_slot: true,
        ident_subject: false,
        dim: DimPolicy::Required,
        path: PathPolicy::RequiredValidated,
    };

    #[test]
    fn a_subject_is_either_self_or_a_slot() {
        let r = parse(&STRICT, "$self.syn.number").unwrap();
        assert_eq!(r.subject, Subject::SelfSign);
        assert_eq!(r.dim, Some(Dim::Syn));
        assert_eq!(r.path.as_deref(), Some("number"));

        let r = parse(&STRICT, "$slot.stem.sem.frame").unwrap();
        assert_eq!(r.subject, Subject::Slot("stem".into()));
        assert_eq!(r.dim, Some(Dim::Sem));
        assert_eq!(r.path.as_deref(), Some("frame"));
    }

    #[test]
    fn the_path_may_use_the_full_path_grammar() {
        let r = parse(&STRICT, "$slot.agent.syn.slot[key].deep").unwrap();
        assert_eq!(r.path.as_deref(), Some("slot[key].deep"));
        assert!(parse(&STRICT, "$self.syn.a..b").is_err(), "畸形路徑要擋下");
    }

    #[test]
    fn a_missing_sigil_is_rejected_only_where_the_spec_requires_one() {
        assert_eq!(
            parse(&STRICT, "stem.syn.number"),
            Err(RefError::MissingSigil)
        );

        const LENIENT: RefSpec = RefSpec {
            sigil: Sigil::OptionalSlot,
            ..STRICT
        };
        let r = parse(&LENIENT, "stem.syn.number").unwrap();
        assert_eq!(r.subject, Subject::Slot("stem".into()));
    }

    /// case scrutinee 的既有判準:首段是維度 → `$self`;否則 → 同名 slot。
    #[test]
    fn a_bare_head_that_names_a_dimension_means_self() {
        const SCRUTINEE: RefSpec = RefSpec {
            sigil: Sigil::OptionalHeadDim,
            allow_self: true,
            allow_slot: true,
            ident_subject: false,
            dim: DimPolicy::Required,
            path: PathPolicy::Optional,
        };
        assert_eq!(
            parse(&SCRUTINEE, "syn.number").unwrap().subject,
            Subject::SelfSign
        );
        assert_eq!(
            parse(&SCRUTINEE, "stem.phon").unwrap().subject,
            Subject::Slot("stem".into())
        );
        // 遺留寫法:scrutinee 位置的 `$self.` 是裝飾,剝掉之後首段仍是 slot。
        assert_eq!(
            parse(&SCRUTINEE, strip_legacy_self_prefix("$self.stem.phon"))
                .unwrap()
                .subject,
            Subject::Slot("stem".into()),
            "與舊實作的 strip-and-ignore 行為一致"
        );
    }

    #[test]
    fn a_spec_can_lock_the_dimension_and_the_path_length() {
        const SLOT_FEATURE: RefSpec = RefSpec {
            sigil: Sigil::Required,
            allow_self: false,
            allow_slot: true,
            ident_subject: false,
            dim: DimPolicy::Only(Dim::Syn),
            path: PathPolicy::Identifiers(1),
        };
        assert!(parse(&SLOT_FEATURE, "$slot.source.syn.number").is_ok());
        assert_eq!(
            parse(&SLOT_FEATURE, "$slot.source.sem.number"),
            Err(RefError::WrongDim(Dim::Syn))
        );
        assert_eq!(
            parse(&SLOT_FEATURE, "$slot.source.syn.a.b"),
            Err(RefError::WrongSegmentCount(1))
        );
        assert_eq!(
            parse(&SLOT_FEATURE, "$self.syn.number"),
            Err(RefError::SubjectNotAllowed)
        );
    }

    #[test]
    fn a_subject_only_reference_carries_no_dimension() {
        const SUBJECT_ONLY: RefSpec = RefSpec {
            sigil: Sigil::Required,
            allow_self: true,
            allow_slot: true,
            ident_subject: false,
            dim: DimPolicy::Forbidden,
            path: PathPolicy::Forbidden,
        };
        assert_eq!(
            parse(&SUBJECT_ONLY, "$self").unwrap().subject,
            Subject::SelfSign
        );
        assert_eq!(
            parse(&SUBJECT_ONLY, "$slot.stem").unwrap().subject,
            Subject::Slot("stem".into())
        );
        assert_eq!(
            parse(&SUBJECT_ONLY, "$self.syn.number"),
            Err(RefError::DimForbidden)
        );
    }

    /// Phase 0 的關鍵不變式:**寫進去什麼形,排出來就是什麼形**。
    /// 若這條紅了,canonical source 會變,`base_source` digest 跟著變。
    #[test]
    fn rendering_reproduces_the_spelling_it_was_given() {
        const LENIENT: RefSpec = RefSpec {
            sigil: Sigil::OptionalSlot,
            allow_self: true,
            allow_slot: true,
            ident_subject: false,
            dim: DimPolicy::Optional,
            path: PathPolicy::Optional,
        };
        for source in [
            "$self.syn.number",
            "$slot.stem.syn.number",
            "stem.syn.number",
            "stem.phon",
            "stem",
            "$slot.stem",
            "$self",
        ] {
            assert_eq!(parse(&LENIENT, source).unwrap().render(), source);
        }
    }

    #[test]
    fn renaming_an_operand_keeps_the_original_spelling() {
        assert_eq!(
            rename_slot_in_operand("stem.syn.number", "stem", "base"),
            "base.syn.number"
        );
        // 舊的 `rewrite_slot_operand` 只認裸形,顯式形整個漏掉。
        assert_eq!(
            rename_slot_in_operand("$slot.stem.syn.number", "stem", "base"),
            "$slot.base.syn.number"
        );
        assert_eq!(rename_slot_in_operand("stem", "stem", "base"), "base");
        assert_eq!(
            rename_slot_in_operand("other.syn.number", "stem", "base"),
            "other.syn.number"
        );
    }

    #[test]
    fn renaming_in_text_covers_both_embedded_spellings() {
        assert_eq!(
            rename_slot_in_text("number => $slot.stem.syn.number", "stem", "base"),
            "number => $slot.base.syn.number"
        );
        assert_eq!(
            rename_slot_in_text("/{stem}+s/", "stem", "base"),
            "/{base}+s/"
        );
        // 前綴不得被吃掉:`stem` 不是 `stemma`。
        assert_eq!(
            rename_slot_in_text("$slot.stemma.syn.x", "stem", "base"),
            "$slot.stemma.syn.x"
        );
        assert_eq!(rename_slot_in_text("{stemma}", "stem", "base"), "{stemma}");
    }
}
