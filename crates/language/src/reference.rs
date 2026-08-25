//! 兩個正交的算子:`$` 選出對象,`{…}` 求值並嵌入。
//!
//! ```text
//! Reference     := Subject ( '.' Dim ( '.' Path )? )?
//! Subject       := '$self' | '$slot' '.' Name
//! Interpolation := '{' Subject '}'
//! ```
//!
//! **`$` 只建立引用,不求值。** `$slot.stem.syn.number` 是「slot `stem` 的
//! 填充者的 syn.number 這個欄位」——一個指名,規則拿它去比對或寫入。
//!
//! **`{…}` 要求求值後把結果嵌在這裡。** `/{$slot.stem}+s/` 是「把 `stem`
//! 的填充者算出來,結果放這個位置」。嵌入什麼型別由括號所在的位置決定
//! (phon 模板要 surface、sem role 要語意節點),故括號內只寫主體,不帶
//! 維度與路徑。
//!
//! 兩者可以疊:括號內永遠是一個引用。`{$self}` 之所以長這樣,正是因為它就
//! 是這條規則的一般情形,而不是特例。
//!
//! ## 主體一律顯式
//!
//! **`$` 不可省略,括號內也一樣。** 曾經合法的裸寫法已全部移除:
//!
//! - `stem.syn.number` → `$slot.stem.syn.number`(constraint 運算元)
//! - `stem.phon` → `$slot.stem.phon`(case scrutinee 讀 slot)
//! - `syn.number` → `$self.syn.number`(case scrutinee 讀自己)
//! - `/{stem}/` → `/{$slot.stem}/`(phon 模板、sem role、application 實參)
//!
//! 連帶移除的是**首段猜測**:裸形之下,`stem.phon` 與 `syn.number` 只能靠
//! 「首段是不是維度關鍵字」來決定主體是 slot 還是 `$self`——於是一個叫
//! `syn` 的 slot 會被靜默讀成自己的 syn 維。顯式主體之後這個歧義不存在。
//!
//! 同樣移除的還有 scrutinee 位置那個**寫了也沒用**的 `$self.` 裝飾:
//! `$self.stem.phon` 曾被 strip 掉再把 `stem` 當 slot 解讀,現在它就是
//! 字面意義——非法(`$self` 沒有叫 `stem` 的維度),要寫 `$slot.stem.phon`。
//!
//! 解析散在八個位置、`{…}` 的字元迴圈另外抄了三份的狀況,由本模組收束:
//! 呼叫點宣告一份 [`RefSpec`](我接受哪個子集),或呼叫
//! [`scan_interpolations`](把模板裡的括號一次掃出來)。
//!
//! **不在本模組範圍**:`.chg` function 層的 guard 主體(`x.syn.category`)。
//! 那裡的 `x` 是 function 參數而非 slot,主體命名空間不同,與此合流是另一
//! 個決定。

use crate::path::parse_path;
use crate::Dim;

/// 求值時才綁定的主體。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// `$self`:當前 sign。
    SelfSign,
    /// `$slot.<name>`:某個 slot 的填充者。
    Slot(String),
    /// `$<name>`:求值環境裡的具名綁定(P81)。`.lang` 側不用;
    /// `.chg` 的 function guard 用它指涉參數。
    ///
    /// `self` 與 `slot` 是保留字,故 `$x` 不會與前兩形相撞。
    Binding(String),
}

/// 一個解析完成的 `$` 引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub subject: Subject,
    /// 主體之後的維度。`None` = 引用主體自身(如 `$self == [Trait]`)。
    pub dim: Option<Dim>,
    /// 維度之後的欄位路徑,**不含**維度前綴。`None` = 只到維度為止。
    pub path: Option<String>,
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
    /// 必填且恰 n 段,每段為裸識別字。
    Identifiers(usize),
}

/// 一個呼叫點接受的引用子集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefSpec {
    pub allow_self: bool,
    pub allow_slot: bool,
    /// 是否接受 `$<name>` 具名綁定(P81)。`.lang` 的位置一律 false。
    pub allow_binding: bool,
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
    /// slot 名不是裸識別字(含 `.`、空白或括號)。
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

/// 切出主體,回傳 (主體, 剩下的尾段)。主體一律顯式,故不需要任何 spec
/// ——沒有 sigil 就沒有主體,不猜。
fn split_subject(value: &str) -> Result<(Subject, &str), RefError> {
    if value == "$self" {
        return Ok((Subject::SelfSign, ""));
    }
    if let Some(rest) = value.strip_prefix("$self.") {
        return Ok((Subject::SelfSign, rest));
    }
    if let Some(rest) = value.strip_prefix("$slot.") {
        let (name, tail) = rest.split_once('.').unwrap_or((rest, ""));
        if name.is_empty() {
            return Err(RefError::EmptySlotName);
        }
        return Ok((Subject::Slot(name.to_owned()), tail));
    }
    // `$<name>`:`self` 與 `slot` 已在上面攔掉,故此處必是具名綁定。
    if let Some(rest) = value.strip_prefix('$') {
        let (name, tail) = rest.split_once('.').unwrap_or((rest, ""));
        if name.is_empty() {
            return Err(RefError::EmptySlotName);
        }
        return Ok((Subject::Binding(name.to_owned()), tail));
    }
    Err(RefError::MissingSigil)
}

// ── 共用的 spec:同一個形狀原本被四處各抄一份 ──────────────────────────

/// `slot_features:` 的凍結 syn 讀取,以及 `constraints:` 的欄位運算元:
/// `$slot.<name>.syn.<feature>`。維度鎖死 syn、路徑恰一段(P41)。
pub const SLOT_SYN_FEATURE: RefSpec = RefSpec {
    allow_self: false,
    allow_slot: true,
    allow_binding: false,
    dim: DimPolicy::Only(Dim::Syn),
    path: PathPolicy::Identifiers(1),
};

/// `constraints:` 的整體運算元:可以只是一個 slot(`before($slot.a, $slot.b)`),
/// 也可以帶維度與欄位。只用來取出主體 slot。
pub const CONSTRAINT_OPERAND: RefSpec = RefSpec {
    allow_self: false,
    allow_slot: true,
    allow_binding: false,
    dim: DimPolicy::Optional,
    path: PathPolicy::Optional,
};

/// `before(…)` / `adjacent(…)` 的運算元:只有 slot,沒有維度也沒有欄位。
pub const CONSTRAINT_SLOT: RefSpec = RefSpec {
    allow_self: false,
    allow_slot: true,
    allow_binding: false,
    dim: DimPolicy::Forbidden,
    path: PathPolicy::Forbidden,
};

/// case scrutinee:`<主體>.<維度>.<欄位>`。
///
/// **欄位必填**——scrutinee 搭配的是 `== VALUE` 純量比對,沒有欄位就沒有可比
/// 的值。範疇比對不走這裡:那是本體樹成員關係,記法是 `[Trait]`,寫成 guard
/// 形 `case:` + `$slot.NAME == [Trait]:`(見 `synchronic::Guard::SlotIsA`)。
///
/// 把「欄位必填」放在 spec 而不是求值器,是為了讓 `$slot.head.phon` 這種缺
/// 欄位的寫法在 **compile 期**就被擋下,而不是通過編譯後在求值時才炸。
pub const SCRUTINEE: RefSpec = RefSpec {
    allow_self: true,
    allow_slot: true,
    allow_binding: false,
    dim: DimPolicy::Required,
    path: PathPolicy::RequiredValidated,
};

/// `.chg` function guard 的主體:`$<參數名>`(P81)。不接受 `$self`/`$slot.`
/// ——function 層沒有 ambient sign,也沒有自己的 slot。
pub const BINDING_FIELD: RefSpec = RefSpec {
    allow_self: false,
    allow_slot: false,
    allow_binding: true,
    dim: DimPolicy::Required,
    path: PathPolicy::RequiredValidated,
};

/// 同上,但只到主體為止(`$x == [Trait]` 的左端)。
pub const BINDING_ONLY: RefSpec = RefSpec {
    allow_self: false,
    allow_slot: false,
    allow_binding: true,
    dim: DimPolicy::Forbidden,
    path: PathPolicy::Forbidden,
};

/// `{…}` 內容:只有主體。嵌入什麼由括號所在的位置決定,故不帶維度與路徑。
pub const SUBJECT_ONLY: RefSpec = RefSpec {
    allow_self: true,
    allow_slot: true,
    allow_binding: false,
    dim: DimPolicy::Forbidden,
    path: PathPolicy::Forbidden,
};

/// 依 `spec` 解析一個 `$` 引用。
pub fn parse(spec: &RefSpec, value: &str) -> Result<Reference, RefError> {
    let value = value.trim();
    let (subject, tail) = split_subject(value)?;
    match &subject {
        Subject::SelfSign if !spec.allow_self => return Err(RefError::SubjectNotAllowed),
        Subject::Slot(_) if !spec.allow_slot => return Err(RefError::SubjectNotAllowed),
        Subject::Binding(_) if !spec.allow_binding => return Err(RefError::SubjectNotAllowed),
        Subject::Slot(name) | Subject::Binding(name) if !ident_ok(name) => {
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
    })
}

impl Reference {
    /// 引用的 slot 名(主體是 `$self` 時為 `None`)。
    pub fn slot(&self) -> Option<&str> {
        match &self.subject {
            Subject::Slot(name) => Some(name),
            Subject::SelfSign | Subject::Binding(_) => None,
        }
    }

    /// 具名綁定的名字(主體不是綁定時為 `None`)。
    pub fn binding(&self) -> Option<&str> {
        match &self.subject {
            Subject::Binding(name) => Some(name),
            Subject::SelfSign | Subject::Slot(_) => None,
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

    /// 排成 canonical 形——主體一律顯式。`{…}` 內的裸名不走這裡
    /// (printer 自己組 `{name}`)。
    pub fn render(&self) -> String {
        let mut out = match &self.subject {
            Subject::SelfSign => "$self".to_owned(),
            Subject::Slot(name) => format!("$slot.{name}"),
            Subject::Binding(name) => format!("${name}"),
        };
        if let Some(dim) = self.dim {
            out.push('.');
            out.push_str(dim.keyword());
        }
        if let Some(path) = &self.path {
            out.push('.');
            out.push_str(path);
        }
        out
    }
}

// ── `{…}` 插值 ─────────────────────────────────────────────────────────

/// 模板裡的一處 `{…}`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpolation {
    /// 含括號的位元組範圍。
    pub start: usize,
    pub end: usize,
    /// 括號內的原文(已 trim)。
    pub inner: String,
    /// 括號內解析出的主體;內容非法時為錯誤。
    pub subject: Result<Subject, RefError>,
}

impl Interpolation {
    /// 這處插值引用的 slot 名(主體是 `$self` 或內容非法時為 `None`)。
    pub fn slot(&self) -> Option<&str> {
        match &self.subject {
            Ok(Subject::Slot(name)) => Some(name),
            _ => None,
        }
    }
}

/// 掃出模板裡的每一處 `{…}`。
///
/// 三個幾乎相同的字元迴圈(代入、符號式代入、引用收集)原本各抄一份,
/// 只在「命中之後做什麼」不同;掃描本身收在這裡。
pub fn scan_interpolations(template: &str) -> Vec<Interpolation> {
    let mut found = Vec::new();
    let bytes = template.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
            continue;
        }
        let Some(offset) = template[index..].find('}') else {
            break;
        };
        let end = index + offset + 1;
        let inner = template[index + 1..end - 1].trim();
        found.push(Interpolation {
            start: index,
            end,
            inner: inner.to_owned(),
            subject: parse(&SUBJECT_ONLY, inner).map(|read| read.subject),
        });
        index = end;
    }
    found
}

/// 依 `replace` 對每處插值取代;回傳 `None` 表示該處原樣保留。
pub fn substitute_interpolations(
    template: &str,
    mut replace: impl FnMut(&Interpolation) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(template.len());
    let mut cursor = 0usize;
    for found in scan_interpolations(template) {
        out.push_str(&template[cursor..found.start]);
        match replace(&found) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&template[found.start..found.end]),
        }
        cursor = found.end;
    }
    out.push_str(&template[cursor..]);
    out
}

/// 把一個主體排成 `{…}` 插值的 canonical 形。
pub fn render_interpolation(subject: &Subject) -> String {
    match subject {
        Subject::SelfSign => "{$self}".to_owned(),
        Subject::Slot(name) => format!("{{$slot.{name}}}"),
        Subject::Binding(name) => format!("{{${name}}}"),
    }
}

// ── rename ─────────────────────────────────────────────────────────────

/// 把 `source` 裡對 slot `old` 的引用改名為 `new`。
///
/// 主體一律顯式之後,引用只有一種可掃的形狀(`$slot.<name>`),而 `{…}` 內
/// 也是同一個形狀——於是**整串運算元、內嵌於規則本文、模板插值三者共用
/// 同一個掃描**。先前那三個重寫器(裸形 / `$slot.` / `{…}`)與「哪個位置
/// 該套哪幾個」的對應表,一起消失。
pub fn rename_slot(source: &str, old: &str, new: &str) -> String {
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

fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | ':' | '/')
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRICT: RefSpec = RefSpec {
        allow_self: true,
        allow_slot: true,
        allow_binding: false,
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

    /// P80:Path 只剩點分名段。
    #[test]
    fn the_path_is_dotted_names_only() {
        let r = parse(&STRICT, "$slot.agent.syn.deep.field").unwrap();
        assert_eq!(r.path.as_deref(), Some("deep.field"));
        assert!(parse(&STRICT, "$self.syn.a..b").is_err(), "畸形路徑要擋下");
        assert!(
            parse(&STRICT, "$slot.agent.syn.slot[key]").is_err(),
            "`[鍵]` 已不是 Path 語法"
        );
        assert!(
            parse(&STRICT, "$self.syn.t~tone").is_err(),
            "`~tier` 已不是 Path 語法"
        );
    }

    /// Phase 1 的核心:**沒有主體就沒有引用,不猜**。
    #[test]
    fn a_reference_without_a_sigil_is_rejected() {
        for bare in [
            "stem.syn.number", // 舊的 constraint 運算元
            "stem.phon",       // 舊的 scrutinee 讀 slot
            "syn.number",      // 舊的 scrutinee 讀自己
            "stem",
        ] {
            assert_eq!(parse(&STRICT, bare), Err(RefError::MissingSigil), "{bare}");
            assert_eq!(
                parse(&CONSTRAINT_OPERAND, bare),
                Err(RefError::MissingSigil),
                "{bare}"
            );
            assert_eq!(
                parse(&SCRUTINEE, bare),
                Err(RefError::MissingSigil),
                "{bare}"
            );
        }
    }

    /// 首段猜測隨裸形一起消失:一個叫 `syn` 的 slot 不再被讀成自己的 syn 維。
    #[test]
    fn a_slot_named_after_a_dimension_is_no_longer_ambiguous() {
        let read = parse(&SCRUTINEE, "$slot.syn.phon.length").unwrap();
        assert_eq!(read.subject, Subject::Slot("syn".into()));
        assert_eq!(read.dim, Some(Dim::Phon));

        let read = parse(&SCRUTINEE, "$self.syn.number").unwrap();
        assert_eq!(read.subject, Subject::SelfSign);
        assert_eq!(read.dim, Some(Dim::Syn));
    }

    /// scrutinee 的欄位必填:缺欄位在 spec 層就擋下,不留到求值期。
    #[test]
    fn a_scrutinee_without_a_field_is_rejected_by_the_spec() {
        assert_eq!(
            parse(&SCRUTINEE, "$slot.head.phon"),
            Err(RefError::PathRequired)
        );
        assert_eq!(parse(&SCRUTINEE, "$self.syn"), Err(RefError::PathRequired));
    }

    /// scrutinee 位置那個「寫了也沒用」的 `$self.` 裝飾已無容身處:
    /// `$self.stem.phon` 現在就是字面意義,而 `$self` 沒有叫 `stem` 的維度。
    #[test]
    fn a_decorative_self_prefix_in_front_of_a_slot_is_now_an_error() {
        assert_eq!(
            parse(&SCRUTINEE, "$self.stem.phon"),
            Err(RefError::UnknownDim("stem".into()))
        );
    }

    #[test]
    fn a_spec_can_lock_the_dimension_and_the_path_length() {
        assert!(parse(&SLOT_SYN_FEATURE, "$slot.source.syn.number").is_ok());
        assert_eq!(
            parse(&SLOT_SYN_FEATURE, "$slot.source.sem.number"),
            Err(RefError::WrongDim(Dim::Syn))
        );
        assert_eq!(
            parse(&SLOT_SYN_FEATURE, "$slot.source.syn.a.b"),
            Err(RefError::WrongSegmentCount(1))
        );
        assert_eq!(
            parse(&SLOT_SYN_FEATURE, "$self.syn.number"),
            Err(RefError::SubjectNotAllowed)
        );
    }

    #[test]
    fn a_slot_name_must_be_a_bare_identifier() {
        assert_eq!(
            parse(&CONSTRAINT_OPERAND, "$slot.a b"),
            Err(RefError::SlotNotIdentifier)
        );
    }

    #[test]
    fn a_subject_only_reference_carries_no_dimension() {
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

    /// `{…}` 內也要 `$`——括號是「求值並嵌入」的算子,不是主體的替代標記。
    #[test]
    fn inside_braces_the_subject_is_still_explicit() {
        assert_eq!(
            parse(&SUBJECT_ONLY, "$slot.stem").unwrap().subject,
            Subject::Slot("stem".into())
        );
        assert_eq!(parse(&SUBJECT_ONLY, "stem"), Err(RefError::MissingSigil));
    }

    #[test]
    fn interpolations_are_scanned_and_rendered_as_one_shape() {
        let found = scan_interpolations("/{$slot.head}+{$self}/");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].slot(), Some("head"));
        assert_eq!(found[1].subject, Ok(Subject::SelfSign));
        assert_eq!(found[1].slot(), None, "`$self` 不是 slot");

        assert_eq!(
            render_interpolation(&Subject::Slot("head".into())),
            "{$slot.head}"
        );
        assert_eq!(render_interpolation(&Subject::SelfSign), "{$self}");

        // 括號內容非法時掃得到位置,但主體是錯誤——驗證端才指得出是哪一處。
        let found = scan_interpolations("/{head}/");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].inner, "head");
        assert_eq!(found[0].subject, Err(RefError::MissingSigil));
    }

    #[test]
    fn substituting_leaves_unreplaced_interpolations_alone() {
        let out = substitute_interpolations("/{$slot.a}{$slot.b}/", |found| {
            (found.slot() == Some("a")).then(|| "X".to_owned())
        });
        assert_eq!(out, "/X{$slot.b}/");
    }

    /// P81:`$<name>` 是第三種主體——求值環境裡的具名綁定。
    /// `self` 與 `slot` 是保留字,故不會相撞。
    #[test]
    fn a_dollar_name_is_a_named_binding() {
        let r = parse(&BINDING_FIELD, "$x.syn.category").unwrap();
        assert_eq!(r.subject, Subject::Binding("x".into()));
        assert_eq!(r.binding(), Some("x"));
        assert_eq!(r.slot(), None, "綁定不是 slot");

        // 保留字仍走各自的形
        assert_eq!(
            parse(&SCRUTINEE, "$self.syn.n").unwrap().subject,
            Subject::SelfSign
        );
        assert_eq!(
            parse(&SCRUTINEE, "$slot.a.syn.n").unwrap().subject,
            Subject::Slot("a".into())
        );
    }

    /// `.lang` 的位置一律不收具名綁定——那是 `.chg` function 層的東西。
    #[test]
    fn lang_positions_reject_named_bindings() {
        for spec in [
            &SCRUTINEE,
            &CONSTRAINT_OPERAND,
            &SLOT_SYN_FEATURE,
            &SUBJECT_ONLY,
        ] {
            assert_eq!(
                parse(spec, "$x.syn.n"),
                Err(RefError::SubjectNotAllowed),
                "`.lang` 位置不得接受 `$x`"
            );
        }
    }

    #[test]
    fn a_binding_renders_back_to_its_dollar_form() {
        assert_eq!(
            parse(&BINDING_FIELD, "$x.syn.category").unwrap().render(),
            "$x.syn.category"
        );
        assert_eq!(parse(&BINDING_ONLY, "$x").unwrap().render(), "$x");
    }

    /// canonical 只有一種寫法:主體顯式。
    #[test]
    fn rendering_always_spells_the_subject_out() {
        for source in [
            "$self.syn.number",
            "$slot.stem.syn.number",
            "$slot.stem.phon",
            "$slot.stem",
            "$self",
        ] {
            assert_eq!(
                parse(&CONSTRAINT_OPERAND, source)
                    .unwrap_or_else(|_| parse(&SCRUTINEE, source).unwrap_or_else(|_| parse(
                        &SUBJECT_ONLY,
                        source
                    )
                    .unwrap()))
                    .render(),
                source
            );
        }
    }

    /// 一個掃描涵蓋全部三種位置:整串運算元、內嵌於本文、模板插值。
    #[test]
    fn renaming_covers_every_position_with_one_scan() {
        assert_eq!(
            rename_slot("$slot.stem.syn.number", "stem", "base"),
            "$slot.base.syn.number"
        );
        assert_eq!(rename_slot("$slot.stem", "stem", "base"), "$slot.base");
        assert_eq!(
            rename_slot("$slot.other.syn.number", "stem", "base"),
            "$slot.other.syn.number"
        );
        // 裸形不再是引用,rename 不得碰它。
        assert_eq!(
            rename_slot("stem.syn.number", "stem", "base"),
            "stem.syn.number"
        );
        assert_eq!(
            rename_slot("number => $slot.stem.syn.number", "stem", "base"),
            "number => $slot.base.syn.number"
        );
        assert_eq!(
            rename_slot("/{$slot.stem}+s/", "stem", "base"),
            "/{$slot.base}+s/"
        );
        // 前綴不得被吃掉:`stem` 不是 `stemma`。
        assert_eq!(
            rename_slot("$slot.stemma.syn.x", "stem", "base"),
            "$slot.stemma.syn.x"
        );
        assert_eq!(
            rename_slot("{$slot.stemma}", "stem", "base"),
            "{$slot.stemma}"
        );
    }
}
