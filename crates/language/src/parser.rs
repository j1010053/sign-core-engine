#![allow(clippy::doc_lazy_continuation)]
//! Language parser(步驟 9 + I22 語法重設計):`.lang` 原文 → [`Language`]。
//!
//! **colon + 縮排**風格(貼合 tshiatun/Lexurgy;取代 `{ }`)。統一 body 語法
//! (I22):`sign`/`trait`/`global trait` 三種容器頭 `Name:` 後縮排 body,body 語法
//! **trait 與 sign 完全相同**——
//! - `belongs X`(分類,維度中立單一樹,P38 v0.2);
//! - `Name[n]`(macro 引用,P5/P27);`==`(block 邊界);
//! - **維度區塊** `syn:` / `phon:` / `sem:` / `prag:`,內縮為該維內容:
//!   - `syn:` 下 `slots:` → slot 行(`name [Filler]` 尾綴 `?` = optional,I21)，
//!     `slot_features:` → `TARGET.FEATURE = VALUE`（literal 或 frozen
//!     `$slot.SOURCE.syn.FEATURE`）；`map SLOT OP [ARG]` → typed slot mapping；
//!     `path = value` → Def `syn.path`;
//!   - `phon:` 下 `/…/` → Def `phon`(UR/模板);其餘行 → phon 規則(`=>`/dsl 動詞;
//!     尾綴 `@stage`,`else` 續行掛前一規則,P22);
//!   - 各維 lhs 共用 Path 文法(`.`/`[key]`/`~tier`)。
//! - `distribution:`(縮排 `key = value`)為 language 級語句。
//! dsl 域宣告(Feature/Symbol/Class/Melody/Spell-out/Parse…)= 首個 language 頭
//! 之前的 verbatim 行(裁決 1)。id 依文件序決定性再生(I15-b/P26)。
//! **`/* … */` 區塊註解**(貼合 tshiatun dsl)可出現於任意位置(檔首/檔中/行尾/
//! 跨行),解析前剝除(保留行號);canonical 不保留(IR dump 慣例)。
//!
//! round-trip:對 canonical 輸入,`parse(src).dump() == src`(P21)。

use crate::path::parse_path;
use crate::{
    BinaryConstraint, Block, CaseBranch, CaseCondition, CaseSelection, ConstraintPredicate, Def,
    Dim, Expression, ExpressionType, FeatureDecl, FeatureExpression, FeatureValue, Language,
    PhonBlock, Realization, RoleBinding, RoleDecl, RoleExpression, Rule, SignApplication,
    SignArgument, SignArgumentValue, SignExpression, SignItem, SignProjection, Slot,
    SlotConstraint, SlotFeatureBinding, SlotMapOp, SourceLocation, Stage, TraitDef, TypedCase,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parse error at line {line}: {msg}")]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

fn err(line: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        msg: msg.into(),
    }
}

/// 一行:行號、縮排空格數、去空白內容。
#[derive(Clone)]
struct Line {
    no: usize,
    indent: usize,
    text: String,
}

fn indent_of(raw: &str) -> usize {
    raw.chars().take_while(|c| *c == ' ').count()
}

/// 容器頭 `<kw> Name:` → (kw, name)?
fn container_head(text: &str) -> Option<(&'static str, &str)> {
    for kw in ["global trait", "trait", "sign"] {
        if let Some(rest) = text.strip_prefix(kw) {
            if let Some(name) = rest.strip_suffix(':') {
                let name = name.trim();
                if !name.is_empty() && !name.contains(char::is_whitespace) {
                    return Some((kw, name));
                }
            }
        }
    }
    None
}

fn is_language_head(text: &str) -> bool {
    text == "distribution:" || container_head(text).is_some()
}

fn split_arguments(source: &str, line: usize) -> Result<Vec<&str>, ParseError> {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut output = Vec::new();
    for (index, character) in source.char_indices() {
        match character {
            '(' | '{' => depth += 1,
            ')' | '}' => {
                if depth == 0 {
                    return Err(err(line, "unbalanced application argument"));
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                output.push(source[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(err(line, "unbalanced application argument"));
    }
    if !source.trim().is_empty() {
        output.push(source[start..].trim());
    }
    Ok(output)
}

fn parse_argument_value(source: &str, line: usize) -> Result<SignArgumentValue, ParseError> {
    let value = source
        .trim()
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .unwrap_or(source.trim())
        .trim();
    if value == "$self" {
        return Ok(SignArgumentValue::SelfSign);
    }
    if let Some(slot) = value.strip_prefix("$slot.") {
        if ident_ok(slot) {
            return Ok(SignArgumentValue::Slot(slot.to_owned()));
        }
    }
    if ident_ok(value) {
        return Ok(SignArgumentValue::Slot(value.to_owned()));
    }
    parse_sign_application(value, line)
        .map(Box::new)
        .map(SignArgumentValue::Application)
}

/// 在**巢狀深度 0** 找具名引數的分隔符(`:`,或舊寫法 `=`)。
/// 巢狀 application/`{…}` 內的分隔符屬於內層,不能在這裡切。
fn split_argument_name(argument: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    for (index, character) in argument.char_indices() {
        match character {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth = depth.saturating_sub(1),
            ':' | '=' if depth == 0 => return Some((&argument[..index], &argument[index + 1..])),
            _ => {}
        }
    }
    None
}

fn parse_sign_application(source: &str, line: usize) -> Result<SignApplication, ParseError> {
    let Some(open) = source.find('(') else {
        return Err(err(line, "expected a Sign application `name(...)`"));
    };
    let Some(arguments) = source[open + 1..].strip_suffix(')') else {
        return Err(err(line, "Sign application must end with `)`"));
    };
    let callee = source[..open].trim();
    if !ident_ok(callee) {
        return Err(err(line, "Sign application callee must be an identifier"));
    }
    let mut parsed = Vec::new();
    for argument in split_arguments(arguments, line)? {
        // 具名引數 `slot: value`(步驟 15c:層②③④ 的呼叫與此處 sign 引用統一
        // 用 `key: value`)。`=` 為舊寫法,仍接受以免既有檔案失效。
        // **必須只在頂層切**:巢狀 application(`Outer(v: Wrap(v: {$self}))`)
        // 內層的分隔符不屬於這一層。
        let (name, value) = match split_argument_name(argument) {
            Some((name, value)) if ident_ok(name.trim()) => (Some(name.trim().to_owned()), value),
            Some(_) => return Err(err(line, "named argument must name a slot")),
            None => (None, argument),
        };
        parsed.push(SignArgument {
            name,
            value: parse_argument_value(value, line)?,
        });
    }
    Ok(SignApplication {
        callee: callee.to_owned(),
        arguments: parsed,
        source: SourceLocation::line(line),
    })
}

fn parse_expression(
    source: &str,
    expected: &ExpressionType,
    line: usize,
) -> Result<Expression, ParseError> {
    let source = source.trim();
    if source == "$self" && matches!(expected, ExpressionType::SignContext) {
        return Ok(Expression::SelfSign);
    }
    if matches!(expected, ExpressionType::PhonContext)
        && source.starts_with('/')
        && source.ends_with('/')
    {
        if let Some(inner) = source
            .strip_prefix("/{")
            .and_then(|value| value.strip_suffix("}/"))
            .and_then(|value| value.strip_suffix(".phon.ret"))
        {
            return Ok(Expression::PhonInterpolation(parse_sign_application(
                inner, line,
            )?));
        }
        return Ok(Expression::PhonTemplate(source.to_owned()));
    }
    for (suffix, dimension) in [
        (".phon.ret", SignProjection::Phon),
        (".syn.ret", SignProjection::Syn),
        (".sem.ret", SignProjection::Sem),
        (".prag.ret", SignProjection::Prag),
    ] {
        if let Some(value) = source.strip_suffix(suffix) {
            let expression = Expression::SignApplication(parse_sign_application(value, line)?);
            return Ok(Expression::Projection {
                value: Box::new(expression),
                dimension,
            });
        }
    }
    if source.contains('(') {
        return Ok(Expression::SignApplication(parse_sign_application(
            source, line,
        )?));
    }
    if matches!(expected, ExpressionType::Feature { .. }) && ident_ok(source) {
        return Ok(Expression::EnumValue(source.to_owned()));
    }
    if matches!(expected, ExpressionType::Role { .. }) {
        if let Some(slot) = source
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
            .map(str::trim)
        {
            if ident_ok(slot) {
                return Ok(Expression::Slot(slot.to_owned()));
            }
        }
    }
    Err(err(
        line,
        format!("expression {source:?} does not have the expected {expected:?} type"),
    ))
}

fn is_context_head(source: &str) -> bool {
    source == "case:"
        || source == "when:"
        || ((source.starts_with("case ") || source.starts_with("when ")) && source.ends_with(':'))
}

fn is_fragment_context(expected: &ExpressionType) -> bool {
    matches!(
        expected,
        ExpressionType::SignContext
            | ExpressionType::SynContext
            | ExpressionType::SemContext
            | ExpressionType::PragContext
    )
}

fn dim_context(dim: Dim) -> ExpressionType {
    match dim {
        Dim::Syn => ExpressionType::SynContext,
        Dim::Sem => ExpressionType::SemContext,
        Dim::Prag => ExpressionType::PragContext,
        Dim::Phon => ExpressionType::PhonContext,
    }
}

fn context_dim(expected: &ExpressionType) -> Option<Dim> {
    match expected {
        ExpressionType::SynContext => Some(Dim::Syn),
        ExpressionType::SemContext => Some(Dim::Sem),
        ExpressionType::PragContext => Some(Dim::Prag),
        _ => None,
    }
}

fn parse_dim_fragment(
    lang: &mut Language,
    body: &[Line],
    dim: Dim,
) -> Result<Vec<SignItem>, ParseError> {
    let first = body
        .first()
        .ok_or_else(|| err(0, "dimension context fragment is empty"))?;
    let mut wrapped = Vec::with_capacity(body.len() + 1);
    wrapped.push(Line {
        no: first.no,
        indent: first.indent.saturating_sub(1),
        text: format!("{}:", dim.keyword()),
    });
    wrapped.extend(body.iter().cloned());
    Ok(parse_body(lang, &wrapped)?
        .into_iter()
        .flat_map(|block| block.items)
        .collect())
}

fn parse_typed_case(
    lang: &mut Language,
    body: &[Line],
    start: usize,
    expected: ExpressionType,
) -> Result<(TypedCase, usize), ParseError> {
    let head = &body[start];
    let (selection, case_head) = if let Some(rest) = head.text.strip_prefix("case") {
        (CaseSelection::FirstMatch, rest)
    } else if let Some(rest) = head.text.strip_prefix("when") {
        (CaseSelection::Accumulate, rest)
    } else {
        return Err(err(
            head.no,
            "internal context parser called on a non-case/when line",
        ));
    };
    if selection == CaseSelection::Accumulate && !is_fragment_context(&expected) {
        return Err(err(
            head.no,
            "`when:` is only valid in SignContext, SynContext, SemContext, or PragContext",
        ));
    }
    let Some(scrutinee) = case_head.strip_suffix(':') else {
        return Err(err(head.no, "case header must end with `:`"));
    };
    let (scrutinee, case_name) = split_name_suffix(scrutinee);
    let case_name = validate_label(case_name, head.no)?;
    let scrutinee = (!scrutinee.is_empty()).then(|| scrutinee.to_owned());
    let mut index = start + 1;
    while index < body.len() && body[index].text.is_empty() {
        index += 1;
    }
    if index >= body.len() || body[index].indent <= head.indent {
        return Err(err(head.no, "case requires at least one branch"));
    }
    let branch_indent = body[index].indent;
    let mut branches = Vec::new();
    let mut saw_else = false;
    while index < body.len() && body[index].indent > head.indent {
        if body[index].text.is_empty() {
            index += 1;
            continue;
        }
        let branch = &body[index];
        if branch.indent != branch_indent {
            return Err(err(branch.no, "case branch has inconsistent indentation"));
        }
        let Some(label) = branch.text.strip_suffix(':') else {
            return Err(err(branch.no, "case branch header must end with `:`"));
        };
        let (label, branch_name) = split_name_suffix(label);
        let branch_name = validate_label(branch_name, branch.no)?;
        if saw_else && !matches!(label, "else" | "Else") {
            return Err(err(branch.no, "case branch cannot follow `else`"));
        }
        let condition = if matches!(label, "else" | "Else") {
            if saw_else {
                return Err(err(branch.no, "case may contain only one `else` branch"));
            }
            saw_else = true;
            CaseCondition::Else
        } else if scrutinee.is_some() {
            let Some(value) = label.strip_prefix("==") else {
                return Err(err(
                    branch.no,
                    "scrutinee case branch must be `== VALUE:` or `else:`",
                ));
            };
            CaseCondition::Equals(value.trim().to_owned())
        } else {
            CaseCondition::Guard(label.to_owned())
        };
        index += 1;
        while index < body.len() && body[index].text.is_empty() {
            index += 1;
        }
        if index >= body.len() || body[index].indent <= branch_indent {
            return Err(err(branch.no, "case branch requires a result expression"));
        }
        let result_line = &body[index];
        let branch_body_start = index;
        let mut branch_body_end = index;
        while branch_body_end < body.len() && body[branch_body_end].indent > branch_indent {
            branch_body_end += 1;
        }
        let scalar_result = if selection == CaseSelection::FirstMatch {
            parse_expression(&result_line.text, &expected, result_line.no).ok()
        } else {
            None
        };
        let result = if is_context_head(&result_line.text) {
            let (nested, next) = parse_typed_case(lang, body, index, expected.clone())?;
            index = next;
            Expression::Case(Box::new(nested))
        } else if let Some(result) = scalar_result {
            index += 1;
            result
        } else if matches!(expected, ExpressionType::SignContext) {
            let blocks = parse_body(lang, &body[branch_body_start..branch_body_end])?;
            let items = blocks
                .into_iter()
                .flat_map(|block| block.items)
                .collect::<Vec<_>>();
            if items.is_empty() {
                return Err(err(
                    result_line.no,
                    "SignContext fragment must contain at least one Sign item",
                ));
            }
            index = branch_body_end;
            Expression::SignFragment(items)
        } else if let Some(dim) = context_dim(&expected) {
            let items = parse_dim_fragment(lang, &body[branch_body_start..branch_body_end], dim)?;
            if items.is_empty() {
                return Err(err(
                    result_line.no,
                    format!("{expected:?} fragment must contain at least one item"),
                ));
            }
            index = branch_body_end;
            Expression::DimFragment { dim, items }
        } else {
            return Err(err(
                result_line.no,
                format!("case branch does not contain an expression accepted by <{expected:?}>"),
            ));
        };
        let mut belongs = Vec::new();
        while index < body.len() && body[index].indent > branch_indent {
            let line = &body[index];
            if line.text.is_empty() {
                index += 1;
                continue;
            }
            let Some(category) = belongs_target(&line.text, line.no)? else {
                return Err(err(
                    line.no,
                    "only `belongs` may follow a Sign branch result",
                ));
            };
            if !matches!(expected, ExpressionType::SignContext) {
                return Err(err(
                    line.no,
                    "`belongs` is only valid in a case branch whose result type is Sign",
                ));
            }
            belongs.push(category);
            index += 1;
        }
        branches.push(CaseBranch {
            condition,
            result,
            belongs,
            name: branch_name,
            source: SourceLocation::line(branch.no),
        });
    }
    Ok((
        TypedCase {
            selection,
            expected,
            scrutinee,
            name: case_name,
            branches,
            source: SourceLocation::line(head.no),
        },
        index,
    ))
}

fn parse_constraint(source: &str, line: usize) -> Result<BinaryConstraint, ParseError> {
    let Some(open) = source.find('(') else {
        return Err(err(line, "constraint must be `predicate(left, right)`"));
    };
    let Some(arguments) = source[open + 1..].strip_suffix(')') else {
        return Err(err(line, "constraint must end with `)`"));
    };
    let predicate = match source[..open].trim() {
        "equal" | "unify" => ConstraintPredicate::Equal,
        "before" => ConstraintPredicate::Before,
        "adjacent" => ConstraintPredicate::Adjacent,
        other => return Err(err(line, format!("unknown constraint predicate {other:?}"))),
    };
    let arguments = split_arguments(arguments, line)?;
    if arguments.len() != 2 || arguments.iter().any(|argument| argument.is_empty()) {
        return Err(err(line, "binary constraint requires exactly two operands"));
    }
    Ok(BinaryConstraint {
        predicate,
        left: arguments[0].to_owned(),
        right: arguments[1].to_owned(),
        source: SourceLocation::line(line),
    })
}

/// trait 引用?回傳 (name, block):`None` = 整個 trait(裸 `Name` 或 `Name[]`)、
/// `Some(n)` = `Name[n]`(0 起算)。
fn trait_use(l: &str) -> Option<(String, Option<u32>)> {
    match l.find('[') {
        // 裸 `Name` = 整個 trait
        None => ident_ok(l).then(|| (l.to_owned(), None)),
        Some(open) => {
            let name = &l[..open];
            if !ident_ok(name) {
                return None;
            }
            let inside = l[open + 1..].strip_suffix(']')?.trim();
            if inside.is_empty() {
                Some((name.to_owned(), None)) // `Name[]` = 整個 trait
            } else {
                Some((name.to_owned(), Some(inside.parse().ok()?)))
            }
        }
    }
}

fn ident_ok(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(char::is_whitespace)
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// `belongs Name`。
fn belongs_target(l: &str, line: usize) -> Result<Option<String>, ParseError> {
    let Some(rest) = l.strip_prefix("belongs ") else {
        return Ok(None);
    };
    let name = rest.trim();
    if !ident_ok(name) {
        return Err(err(line, "`belongs` expects a single trait name"));
    }
    Ok(Some(name.to_owned()))
}

/// `name [Filler]` (+ optional `?`) → Slot(I21)。
fn parse_slot(l: &str, line: usize) -> Result<Slot, ParseError> {
    let Some(open) = l.find('[') else {
        return Err(err(line, "slot must be `name [Filler]`"));
    };
    let name = l[..open].trim();
    let after = l[open + 1..].trim_end();
    let optional = after.ends_with("]?");
    let Some(inner) = after.strip_suffix("]?").or_else(|| after.strip_suffix(']')) else {
        return Err(err(line, "slot filler must be `[Trait]` (optional `?`)"));
    };
    let filler = inner.trim();
    if !ident_ok(name) || (filler != "*" && !ident_ok(filler)) {
        return Err(err(
            line,
            "slot name and filler must be identifiers, or use `[*]`",
        ));
    }
    Ok(Slot {
        name: name.to_owned(),
        constraint: if filler == "*" {
            SlotConstraint::AnySign
        } else {
            SlotConstraint::Category(filler.to_owned())
        },
        optional,
    })
}

/// A `syn: slot_features:` entry. The target is a construction slot while a
/// slot-valued RHS remains a deliberately narrow frozen `syn` read.
fn parse_slot_feature_binding(
    line: &str,
    source_line: usize,
) -> Result<SlotFeatureBinding, ParseError> {
    let Some((target, value)) = line.split_once('=') else {
        return Err(err(
            source_line,
            "slot feature entry must be `TARGET.FEATURE = VALUE`",
        ));
    };
    let target = target.trim();
    let mut target_parts = target.split('.');
    let (Some(target_slot), Some(feature), None) = (
        target_parts.next(),
        target_parts.next(),
        target_parts.next(),
    ) else {
        return Err(err(
            source_line,
            "slot feature target must be `TARGET_SLOT.FEATURE`",
        ));
    };
    if !ident_ok(target_slot) || !ident_ok(feature) {
        return Err(err(
            source_line,
            "slot feature target slot and feature must be identifiers",
        ));
    }

    let value = value.trim();
    if !ident_ok(value) {
        let mut parts = value.split('.');
        let (Some("$slot"), Some(slot), Some("syn"), Some(feature), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(err(
                source_line,
                "slot feature value must be an enum literal or `$slot.SOURCE.syn.FEATURE`",
            ));
        };
        if !ident_ok(slot) || !ident_ok(feature) {
            return Err(err(
                source_line,
                "slot feature source slot and feature must be identifiers",
            ));
        }
    }

    Ok(SlotFeatureBinding {
        slot: target_slot.to_owned(),
        feature: feature.to_owned(),
        value: value.to_owned(),
        source: SourceLocation::line(source_line),
    })
}

/// 平坦的 construction slot mapping：`map <slot> <operation> [argument]`。
fn parse_slot_map(l: &str, line: usize) -> Result<Option<SlotMapOp>, ParseError> {
    let mut words = l.split_whitespace();
    if words.next() != Some("map") {
        return Ok(None);
    }
    let Some(slot) = words.next() else {
        return Err(err(line, "slot mapping must name a slot"));
    };
    let Some(operation) = words.next() else {
        return Err(err(line, "slot mapping must name an operation"));
    };
    if !ident_ok(slot) {
        return Err(err(line, "slot mapping slot must be a single identifier"));
    }
    let argument = words.next();
    if words.next().is_some() {
        return Err(err(line, "slot mapping has too many arguments"));
    }
    let operation = match (operation, argument) {
        ("preserve", None) => SlotMapOp::Preserve {
            slot: slot.to_owned(),
        },
        ("rename", Some(to)) if ident_ok(to) => SlotMapOp::Rename {
            slot: slot.to_owned(),
            to: to.to_owned(),
        },
        ("autofill", Some(filler)) if ident_ok(filler) => SlotMapOp::AutoFill {
            slot: slot.to_owned(),
            filler: filler.to_owned(),
        },
        ("internalize", None) => SlotMapOp::Internalize {
            slot: slot.to_owned(),
        },
        ("optional", Some("true")) => SlotMapOp::Optional {
            slot: slot.to_owned(),
            optional: true,
        },
        ("optional", Some("false")) => SlotMapOp::Optional {
            slot: slot.to_owned(),
            optional: false,
        },
        _ => {
            return Err(err(
                line,
                "slot mapping must be `map SLOT preserve|internalize`, `map SLOT rename|autofill NAME`, or `map SLOT optional true|false`",
            ));
        }
    };
    Ok(Some(operation))
}

/// 規則行 → Rule(尾綴 `@stage`;body 原文;維度依所在區塊 I25/P44)。
/// Lexurgy 式命名前綴(P46 取徑 A,限 phon):`name: <rule>`。name 可含連字號;
/// 空 body、含空白的 head、保留字(Scan/stage/Then/Else/realization/case/when/
/// propagate)不視為名。回傳 (name, 規則本文)。
fn lexurgy_name_prefix(text: &str) -> Option<(String, &str)> {
    let (head, rest) = text.split_once(':')?;
    let name = head.trim();
    let rest = rest.trim();
    if name.is_empty()
        || rest.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        || matches!(
            name,
            "Scan"
                | "stage"
                | "Then"
                | "Else"
                | "then"
                | "else"
                | "realization"
                | "case"
                | "when"
                | "propagate"
                | "Propagate"
        )
    {
        return None;
    }
    Some((name.to_owned(), rest))
}

/// phon block 頭(P46 S2):`<name>:`(冒號後無內容);後接縮排 body 即結構化 block。
/// phon block 頭:`name:` 或 **`name propagate:`**(P46 S4,對映引擎 header
/// modifier)。回傳 (name, rule-level propagate)。
fn phon_block_header(text: &str) -> Option<(String, bool)> {
    let head = text.strip_suffix(':')?.trim();
    // `name propagate` → 整條 rule 迭代到 fixpoint。
    let (name, propagate) = match head
        .strip_suffix("propagate")
        .or_else(|| head.strip_suffix("Propagate"))
    {
        // 尾綴必須是**獨立的字**(前面有空白),否則 `xpropagate` 會被誤判。
        Some(rest) if rest.ends_with(char::is_whitespace) => (rest.trim(), true),
        _ => (head, false),
    };
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        || matches!(
            name,
            "Scan"
                | "stage"
                | "Then"
                | "Else"
                | "then"
                | "else"
                | "realization"
                | "case"
                | "when"
                | "propagate"
                | "Propagate"
        )
    {
        return None;
    }
    Some((name.to_owned(), propagate))
}

/// `Then:`/`Else:` 邊界,可帶 **`propagate`** 修飾(P46 S4,對映引擎 boundary
/// modifier → `PhonBlock::Propagate`)。回傳 (is_then, propagate, inline 語句)。
fn phon_block_boundary(text: &str) -> Option<(bool, bool, Option<String>)> {
    for (kw, is_then) in [
        ("Then", true),
        ("then", true),
        ("Else", false),
        ("else", false),
    ] {
        let Some(rest) = text.strip_prefix(kw) else {
            continue;
        };
        // `Then:` / `Then propagate:` —— propagate 前必須有空白。
        let (rest, propagate) = match rest.strip_prefix(char::is_whitespace) {
            Some(after) => {
                let after = after.trim_start();
                match after
                    .strip_prefix("propagate")
                    .or_else(|| after.strip_prefix("Propagate"))
                {
                    Some(tail) => (tail, true),
                    None => (rest, false),
                }
            }
            None => (rest, false),
        };
        if let Some(after) = rest.trim_start().strip_prefix(':') {
            let inline = after.trim();
            return Some((
                is_then,
                propagate,
                (!inline.is_empty()).then(|| inline.to_owned()),
            ));
        }
    }
    None
}

/// 把 `name:` 下的縮排 body 解析成結構化 `PhonBlock`(對映引擎 `RuleBlock`)。
/// leading 語句 = 第一個 Leaf;`Then:`/`Else:`(inline 或縮排子 block)開後續 block;
/// 同層不得混 Then/Else。
fn parse_phon_block(lines: &[Line]) -> Result<PhonBlock, ParseError> {
    let mut first: Vec<String> = Vec::new();
    let mut subs: Vec<PhonBlock> = Vec::new();
    let mut is_then: Option<bool> = None;
    let mut i = 0usize;
    while i < lines.len() {
        let ln = &lines[i];
        if ln.text.is_empty() {
            i += 1;
            continue;
        }
        let text = ln.text.trim();
        if let Some((then, propagate, inline)) = phon_block_boundary(text) {
            if is_then.is_some_and(|k| k != then) {
                return Err(err(
                    ln.no,
                    "cannot mix `Then:` and `Else:` at one block level",
                ));
            }
            is_then = Some(then);
            let sub = if let Some(stmt) = inline {
                PhonBlock::Leaf(vec![stmt])
            } else {
                let start = i + 1;
                let mut j = start;
                while j < lines.len() && (lines[j].text.is_empty() || lines[j].indent > ln.indent) {
                    j += 1;
                }
                let sub = parse_phon_block(&lines[start..j])?;
                i = j - 1;
                sub
            };
            // P46 S4:`Then propagate:` 只修飾**它引入的那個 element**。
            subs.push(if propagate {
                PhonBlock::Propagate(Box::new(sub))
            } else {
                sub
            });
        } else if is_then.is_some() {
            return Err(err(
                ln.no,
                "statement after a `Then:`/`Else:` boundary must sit inside its block",
            ));
        } else {
            first.push(text.to_owned());
        }
        i += 1;
    }
    Ok(match is_then {
        None => PhonBlock::Leaf(first),
        Some(then) => {
            let mut blocks = vec![PhonBlock::Leaf(first)];
            blocks.extend(subs);
            if then {
                PhonBlock::Then(blocks)
            } else {
                PhonBlock::Else(blocks)
            }
        }
    })
}

/// 拆 `… @name <label>` 後綴(或整行 `@name <label>`):回傳 (剩餘, Option<label>)。
fn split_name_suffix(text: &str) -> (&str, Option<&str>) {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("@name ") {
        return ("", Some(rest.trim()));
    }
    if let Some((head, name)) = text.rsplit_once(" @name ") {
        return (head.trim(), Some(name.trim()));
    }
    (text, None)
}

/// 標籤須為單一識別字。
fn validate_label(label: Option<&str>, line: usize) -> Result<Option<String>, ParseError> {
    match label {
        Some(name) if ident_ok(name) => Ok(Some(name.to_owned())),
        Some(_) => Err(err(line, "label must be a single identifier")),
        None => Ok(None),
    }
}

fn parse_rule(lang: &mut Language, l: &str, line: usize, dim: Dim) -> Result<Rule, ParseError> {
    // `@stage` is the outermost suffix (parsed first); `@name <label>` is inner.
    let (rest, stage) = match l.rsplit_once(" @stage ") {
        Some((b, s)) => {
            let stage = match s.trim() {
                "stem" => Stage::Stem,
                "word" => Stage::Word,
                "phrase" => Stage::Phrase,
                other => return Err(err(line, format!("unknown stage {other:?}"))),
            };
            (b.trim(), stage)
        }
        None => (l, Stage::Word),
    };
    let (body, name) = match rest.rsplit_once(" @name ") {
        Some((b, label)) => {
            let label = label.trim();
            if !ident_ok(label) {
                return Err(err(line, "rule label must be a single identifier"));
            }
            (b.trim(), Some(label.to_owned()))
        }
        None => (rest, None),
    };
    let mut rule = lang.rule_dim(body, stage, dim);
    rule.name = name;
    rule.source = SourceLocation::line(line);
    Ok(rule)
}

/// 維度區塊上下文。
#[derive(Clone, Copy, PartialEq)]
enum DimKw {
    Syn,
    Phon,
    Sem,
    Prag,
}
impl DimKw {
    fn prefix(self) -> &'static str {
        match self {
            DimKw::Syn => "syn",
            DimKw::Phon => "phon",
            DimKw::Sem => "sem",
            DimKw::Prag => "prag",
        }
    }
    fn to_dim(self) -> Dim {
        match self {
            DimKw::Syn => Dim::Syn,
            DimKw::Phon => Dim::Phon,
            DimKw::Sem => Dim::Sem,
            DimKw::Prag => Dim::Prag,
        }
    }
    fn parse(s: &str) -> Option<DimKw> {
        match s {
            "syn:" => Some(DimKw::Syn),
            "phon:" => Some(DimKw::Phon),
            "sem:" => Some(DimKw::Sem),
            "prag:" => Some(DimKw::Prag),
            _ => None,
        }
    }
}

/// 解析一個容器 body(縮排行序列)→ Vec<Block>(`==` 切 block,統一 body I22)。
fn parse_body(lang: &mut Language, body: &[Line]) -> Result<Vec<Block>, ParseError> {
    let mut blocks = vec![Block::default()];
    if body.is_empty() {
        return Ok(blocks);
    }
    let base = body[0].indent; // level-1 縮排
    let mut cur_dim: Option<DimKw> = None;
    let mut in_slots = false;
    let mut slots_indent = 0usize;
    let mut in_slot_features = false;
    let mut slot_features_indent = 0usize;
    let mut in_feature = false;
    let mut feature_indent = 0usize;
    let mut in_roles = false;
    let mut roles_indent = 0usize;
    // §10.3:sem 的 senses / 衍生邊子區塊。
    let mut in_senses = false;
    let mut senses_indent = 0usize;
    let mut in_edges = false;
    let mut edges_indent = 0usize;
    let mut in_realization = false;
    let mut realization_indent = 0usize;
    let mut in_constraints = false;
    let mut constraints_indent = 0usize;

    let mut index = 0usize;
    while index < body.len() {
        let ln = &body[index];
        index += 1;
        let (no, ind, text) = (ln.no, ln.indent, ln.text.as_str());
        if in_slots && ind <= slots_indent {
            in_slots = false;
        }
        if in_slot_features && ind <= slot_features_indent {
            in_slot_features = false;
        }
        if in_feature && ind <= feature_indent {
            in_feature = false;
        }
        if in_roles && ind <= roles_indent {
            in_roles = false;
        }
        if in_senses && ind <= senses_indent {
            in_senses = false;
        }
        if in_edges && ind <= edges_indent {
            in_edges = false;
        }
        if in_realization && ind <= realization_indent {
            in_realization = false;
        }
        if in_constraints && ind <= constraints_indent {
            in_constraints = false;
        }
        if ind == base {
            // level-1:重設維度上下文
            cur_dim = None;
            in_slots = false;
            in_slot_features = false;
            in_feature = false;
            in_roles = false;
            in_senses = false;
            in_edges = false;
            in_realization = false;
            in_constraints = false;
            if text == "==" {
                blocks.push(Block::default());
            } else if text == "constraints:" {
                in_constraints = true;
                constraints_indent = ind;
            } else if is_context_head(text) {
                let (case, next) =
                    parse_typed_case(lang, body, index - 1, ExpressionType::SignContext)?;
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::SignExpression(SignExpression {
                        source: case.source,
                        expression: Expression::Case(Box::new(case)),
                    }));
                index = next;
            } else if let Some(t) = belongs_target(text, no)? {
                blocks.last_mut().unwrap().items.push(SignItem::Belongs(t));
            } else if let Some(dim) = DimKw::parse(text) {
                cur_dim = Some(dim);
            } else if let Some((name, block)) = trait_use(text) {
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::TraitUse { name, block });
            } else if !text.contains("=>")
                && text
                    .split_once('=')
                    .is_some_and(|(f, _)| ident_ok(f.trim()))
            {
                // 頂層非維度 Def(如 `entrenchment = 0.5`)
                let (path, value) = text.split_once('=').unwrap();
                blocks.last_mut().unwrap().items.push(SignItem::Def(Def {
                    path: path.trim().to_owned(),
                    value: value.trim().to_owned(),
                }));
            } else {
                return Err(err(no, format!("unexpected line {text:?} (expected belongs / Name[n] / <dim>: / field = value / ==)")));
            }
            continue;
        }
        // ind > base:維度區塊內容
        let Some(dim) = cur_dim else {
            if in_constraints {
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::Constraint(parse_constraint(text, no)?));
                continue;
            }
            return Err(err(
                no,
                format!("indented line {text:?} outside a dimension block"),
            ));
        };
        if !in_slots
            && !in_slot_features
            && !in_feature
            && !in_roles
            && !in_senses
            && !in_edges
            && !in_realization
            && is_context_head(text)
        {
            if dim.to_dim() == Dim::Phon {
                return Err(err(
                    no,
                    "phon dimension branching belongs under `realization:`",
                ));
            }
            let (case, next) = parse_typed_case(lang, body, index - 1, dim_context(dim.to_dim()))?;
            blocks
                .last_mut()
                .unwrap()
                .items
                .push(SignItem::SignExpression(SignExpression {
                    source: case.source,
                    expression: Expression::Case(Box::new(case)),
                }));
            index = next;
            continue;
        }
        if in_slots {
            let slot = parse_slot(text, no)?;
            blocks.last_mut().unwrap().items.push(SignItem::Slot(slot));
            continue;
        }
        if in_slot_features {
            let binding = parse_slot_feature_binding(text, no)?;
            blocks
                .last_mut()
                .unwrap()
                .items
                .push(SignItem::SlotFeatureBinding(binding));
            continue;
        }
        if in_feature {
            if let Some(rest) = text.strip_prefix("else ") {
                let Some(SignItem::FeatureRule(rule)) = blocks.last_mut().unwrap().items.last_mut()
                else {
                    return Err(err(no, "`else` without a preceding feature rule"));
                };
                if !rule.then_chain.is_empty() {
                    return Err(err(
                        no,
                        "cannot mix `then` and `else` in one flat rule (use nesting)",
                    ));
                }
                rule.else_chain.push(rest.trim().to_owned());
                rule.branch_sources.push(SourceLocation::line(no));
                continue;
            }
            if let Some(rest) = text.strip_prefix("then ") {
                let Some(SignItem::FeatureRule(rule)) = blocks.last_mut().unwrap().items.last_mut()
                else {
                    return Err(err(no, "`then` without a preceding feature rule"));
                };
                if !rule.else_chain.is_empty() {
                    return Err(err(
                        no,
                        "cannot mix `then` and `else` in one flat rule (use nesting)",
                    ));
                }
                rule.then_chain.push(rest.trim().to_owned());
                rule.branch_sources.push(SourceLocation::line(no));
                continue;
            }
            if let Some(name) = text.strip_suffix("=>").map(str::trim) {
                if !ident_ok(name) {
                    return Err(err(no, "feature expression target must be an identifier"));
                }
                let Some(next_line) = body.get(index) else {
                    return Err(err(no, "feature expression requires a typed case"));
                };
                if next_line.indent <= ind
                    || !(next_line.text == "case:"
                        || (next_line.text.starts_with("case ") && next_line.text.ends_with(':')))
                {
                    return Err(err(
                        no,
                        "feature expression requires an indented typed case",
                    ));
                }
                let expected = ExpressionType::Feature {
                    dim: dim.to_dim(),
                    name: name.to_owned(),
                };
                let (case, next) = parse_typed_case(lang, body, index, expected)?;
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::FeatureExpression(FeatureExpression {
                        dim: dim.to_dim(),
                        name: name.to_owned(),
                        source: SourceLocation::line(no),
                        expression: Expression::Case(Box::new(case)),
                    }));
                index = next;
                continue;
            }
            if text.contains("=>") {
                let rule = parse_rule(lang, text, no, dim.to_dim())?;
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::FeatureRule(rule));
                continue;
            }
            let Some((name, value)) = text.split_once('=') else {
                return Err(err(
                    no,
                    "feature entry must be `NAME = enum(...)`, `NAME = VALUE`, or `NAME => EXPR`",
                ));
            };
            let name = name.trim();
            if !ident_ok(name) {
                return Err(err(no, "feature name must be a single identifier"));
            }
            let value = value.trim();
            if let Some(domain) = value
                .strip_prefix("enum(")
                .and_then(|v| v.strip_suffix(')'))
            {
                let values = domain
                    .split(',')
                    .map(str::trim)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if values.is_empty() || values.iter().any(|value| !ident_ok(value)) {
                    return Err(err(
                        no,
                        "feature enum must contain one or more identifier values",
                    ));
                }
                let mut unique = std::collections::BTreeSet::new();
                if values.iter().any(|value| !unique.insert(value.clone())) {
                    return Err(err(no, "feature enum values must be unique"));
                }
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::FeatureDecl(FeatureDecl {
                        dim: dim.to_dim(),
                        name: name.to_owned(),
                        values,
                        source: SourceLocation::line(no),
                    }));
            } else {
                if !ident_ok(value) {
                    return Err(err(no, "feature value must be a single enum identifier"));
                }
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::FeatureValue(FeatureValue {
                        dim: dim.to_dim(),
                        name: name.to_owned(),
                        value: value.to_owned(),
                        source: SourceLocation::line(no),
                    }));
            }
            continue;
        }
        if in_senses {
            // `name = gloss`
            let Some((name, gloss)) = text.split_once('=') else {
                return Err(err(no, "a sense line is `name = gloss`"));
            };
            let (name, gloss) = (name.trim(), gloss.trim());
            if !ident_ok(name) {
                return Err(err(no, "sense name must be an identifier"));
            }
            if gloss.is_empty() {
                return Err(err(no, "sense gloss must not be empty"));
            }
            blocks
                .last_mut()
                .unwrap()
                .items
                .push(SignItem::Sense(crate::Sense {
                    name: name.to_owned(),
                    gloss: gloss.to_owned(),
                    source: SourceLocation::line(no),
                }));
            continue;
        }
        if in_edges {
            // `<to> from <from> <kind> [opaque]`
            let mut parts = text.split_whitespace();
            let (Some(to), Some(keyword), Some(from), Some(kind)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return Err(err(
                    no,
                    "a sense edge is `<sense> from <sense> <kind> [opaque]`",
                ));
            };
            if keyword != "from" {
                return Err(err(no, "a sense edge uses the `from` keyword"));
            }
            let Some(kind) = crate::DerivationKind::parse(kind) else {
                return Err(err(
                    no,
                    "sense edge kind must be metaphor|metonymy|narrow|broaden",
                ));
            };
            let transparency = match parts.next() {
                None => crate::SenseTransparency::Transparent,
                Some(word) => match crate::SenseTransparency::parse(word) {
                    Some(value) => value,
                    None => return Err(err(no, "sense edge modifier must be `opaque`")),
                },
            };
            if parts.next().is_some() {
                return Err(err(no, "trailing text after a sense edge"));
            }
            blocks
                .last_mut()
                .unwrap()
                .items
                .push(SignItem::SenseEdge(crate::SenseEdge {
                    to: to.to_owned(),
                    from: from.to_owned(),
                    kind,
                    transparency,
                    source: SourceLocation::line(no),
                }));
            continue;
        }
        if in_roles {
            if let Some((name, value)) = text.split_once('=') {
                let name = name.trim();
                let value = value.trim();
                if value.is_empty() {
                    if !ident_ok(name) {
                        return Err(err(no, "role expression target must be an identifier"));
                    }
                    let Some(next_line) = body.get(index) else {
                        return Err(err(no, "role expression requires a typed case"));
                    };
                    if next_line.indent <= ind
                        || !(next_line.text == "case:"
                            || (next_line.text.starts_with("case ")
                                && next_line.text.ends_with(':')))
                    {
                        return Err(err(no, "role expression requires an indented typed case"));
                    }
                    let (case, next) = parse_typed_case(
                        lang,
                        body,
                        index,
                        ExpressionType::Role {
                            name: name.to_owned(),
                        },
                    )?;
                    blocks
                        .last_mut()
                        .unwrap()
                        .items
                        .push(SignItem::RoleExpression(RoleExpression {
                            name: name.to_owned(),
                            source: SourceLocation::line(no),
                            expression: Expression::Case(Box::new(case)),
                        }));
                    index = next;
                    continue;
                }
                let Some(slot) = value.strip_prefix('{').and_then(|v| v.strip_suffix('}')) else {
                    return Err(err(no, "role binding must be `NAME = {slot}`"));
                };
                if !ident_ok(name) || !ident_ok(slot.trim()) {
                    return Err(err(no, "role and slot names must be identifiers"));
                }
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::RoleBinding(RoleBinding {
                        name: name.to_owned(),
                        slot: slot.trim().to_owned(),
                        source: SourceLocation::line(no),
                    }));
            } else {
                let optional = text.ends_with('?');
                let declaration = text.strip_suffix('?').unwrap_or(text).trim();
                let Some((name, constraint)) = declaration.split_once('[') else {
                    return Err(err(no, "role declaration must be `NAME [Trait]?`"));
                };
                let Some(constraint) = constraint.strip_suffix(']') else {
                    return Err(err(
                        no,
                        "role declaration must close its `[Trait]` constraint",
                    ));
                };
                let constraint = constraint.trim();
                if !ident_ok(name.trim()) || (constraint != "*" && !ident_ok(constraint)) {
                    return Err(err(
                        no,
                        "role name and constraint must be identifiers, or use `[*]`",
                    ));
                }
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::RoleDecl(RoleDecl {
                        name: name.trim().to_owned(),
                        constraint: if constraint == "*" {
                            SlotConstraint::AnySign
                        } else {
                            SlotConstraint::Category(constraint.to_owned())
                        },
                        optional,
                        source: SourceLocation::line(no),
                    }));
            }
            continue;
        }
        if in_realization {
            if text == "case:" || (text.starts_with("case ") && text.ends_with(':')) {
                let (case, next) =
                    parse_typed_case(lang, body, index - 1, ExpressionType::PhonContext)?;
                let item = blocks.last_mut().unwrap().items.last_mut();
                let Some(SignItem::Realization(realization)) = item else {
                    return Err(err(no, "internal realization block state is invalid"));
                };
                if realization.expression.is_some() {
                    return Err(err(no, "realization may contain one typed case"));
                }
                realization.expression = Some(case);
                index = next;
                continue;
            }
            return Err(err(
                no,
                "realization must contain a `case:` selecting phon templates by guard",
            ));
        }
        if text == "slots:" {
            if dim != DimKw::Syn {
                return Err(err(no, "`slots:` only under `syn:`"));
            }
            in_slots = true;
            slots_indent = ind;
            continue;
        }
        if text == "slot_features:" {
            if dim != DimKw::Syn {
                return Err(err(no, "`slot_features:` only under `syn:`"));
            }
            in_slot_features = true;
            slot_features_indent = ind;
            continue;
        }
        if text == "feature:" {
            // P71-C:`prag:` 亦支援 `feature:`。P71 R2 要求自造欄位一律先宣告值域,
            // 而 prag 原本沒有這個出口,R2 在該維無處可去。`phon:` 仍不支援——
            // 其 Def 是 UR/模板,規則屬 DSL 音變語言,不走 typed feature 這條。
            if !matches!(dim, DimKw::Syn | DimKw::Sem | DimKw::Prag) {
                return Err(err(
                    no,
                    "`feature:` is only valid under `syn:`, `sem:` or `prag:`",
                ));
            }
            in_feature = true;
            feature_indent = ind;
            continue;
        }
        if text == "roles:" {
            if dim != DimKw::Sem {
                return Err(err(no, "`roles:` is only valid under `sem:`"));
            }
            in_roles = true;
            roles_indent = ind;
            continue;
        }
        if text == "senses:" {
            if dim != DimKw::Sem {
                return Err(err(no, "`senses:` is only valid under `sem:`"));
            }
            in_senses = true;
            senses_indent = ind;
            continue;
        }
        if text == "edges:" {
            if dim != DimKw::Sem {
                return Err(err(no, "`edges:` is only valid under `sem:`"));
            }
            in_edges = true;
            edges_indent = ind;
            continue;
        }
        if text == "realization:" {
            if dim != DimKw::Phon {
                return Err(err(no, "`realization:` is only valid under `phon:`"));
            }
            blocks
                .last_mut()
                .unwrap()
                .items
                .push(SignItem::Realization(Realization::default()));
            in_realization = true;
            realization_indent = ind;
            continue;
        }
        if let Some(mapping) = parse_slot_map(text, no)? {
            if dim != DimKw::Syn {
                return Err(err(no, "slot mapping is only valid under `syn:`"));
            }
            blocks
                .last_mut()
                .unwrap()
                .items
                .push(SignItem::SlotMap(mapping));
            continue;
        }
        if let Some(rest) = text.strip_prefix("else ") {
            let Some(SignItem::Rule(r) | SignItem::FeatureRule(r)) =
                blocks.last_mut().unwrap().items.last_mut()
            else {
                return Err(err(no, "`else` without a preceding rule"));
            };
            if !r.then_chain.is_empty() {
                return Err(err(
                    no,
                    "cannot mix `then` and `else` in one flat rule (use nesting)",
                ));
            }
            r.else_chain.push(rest.trim().to_owned());
            r.branch_sources.push(SourceLocation::line(no));
            continue;
        }
        if let Some(rest) = text.strip_prefix("then ") {
            let Some(SignItem::Rule(r) | SignItem::FeatureRule(r)) =
                blocks.last_mut().unwrap().items.last_mut()
            else {
                return Err(err(no, "`then` without a preceding rule"));
            };
            if !r.else_chain.is_empty() {
                return Err(err(
                    no,
                    "cannot mix `then` and `else` in one flat rule (use nesting)",
                ));
            }
            r.then_chain.push(rest.trim().to_owned());
            r.branch_sources.push(SourceLocation::line(no));
            continue;
        }
        // phon 維:`/…/` = UR/模板 Def;其餘 = phon 規則
        if dim == DimKw::Phon && text.starts_with('/') && text.ends_with('/') && text.len() >= 2 {
            blocks.last_mut().unwrap().items.push(SignItem::Def(Def {
                path: "phon".to_owned(),
                value: text.to_owned(),
            }));
            continue;
        }
        // `path = value`(Path + `=`,非 `=>`)→ Def(path 加維度前綴);
        // 否則視為該維規則(phon 的 dsl 動詞、`A => B`、syn/sem/prag 規則於 12d)。
        let definition = if text.contains("=>") {
            None
        } else {
            text.split_once('=')
        };
        if let Some((field, _)) = definition {
            if dim == DimKw::Phon {
                // phon has no consumable `field = value` metadata: only a `/…/`
                // UR template, a rule, or `realization:`.
                return Err(err(
                    no,
                    "`phon:` takes a `/…/` UR template, a rule, or `realization:` — not a `field = value` definition",
                ));
            }
            let field = field.trim();
            let path = format!("{}.{}", dim.prefix(), field);
            parse_path(&path)
                .map_err(|error| err(no, format!("invalid definition path {field:?}: {error}")))?;
            let (field, value) = text.split_once('=').unwrap();
            blocks.last_mut().unwrap().items.push(SignItem::Def(Def {
                path: format!("{}.{}", dim.prefix(), field.trim()),
                value: value.trim().to_owned(),
            }));
        } else if dim == DimKw::Phon
            && phon_block_header(text).is_some()
            && index < body.len()
            && body[index].indent > ind
        {
            // phon: bare `name:` header + indented body = structured Lexurgy block
            // (P46 S2).
            let (name, propagate) = phon_block_header(text).expect("checked above");
            let start = index;
            while index < body.len() && (body[index].text.is_empty() || body[index].indent > ind) {
                index += 1;
            }
            let block = parse_phon_block(&body[start..index])?;
            let mut r = lang.rule_dim(String::new(), Stage::Word, Dim::Phon);
            r.name = Some(name);
            r.propagate = propagate;
            r.phon_block = Some(block);
            r.source = SourceLocation::line(no);
            blocks.last_mut().unwrap().items.push(SignItem::Rule(r));
        } else {
            // phon: accept a Lexurgy-style `name:` prefix (P46 取徑 A, inline).
            let (prefix_name, rule_text) = match (dim, lexurgy_name_prefix(text)) {
                (DimKw::Phon, Some((name, rest))) => (Some(name), rest),
                _ => (None, text),
            };
            let mut r = parse_rule(lang, rule_text, no, dim.to_dim())?;
            r.name = prefix_name.or(r.name);
            blocks.last_mut().unwrap().items.push(SignItem::Rule(r));
        }
    }
    Ok(blocks)
}

/// 剝除 `/* … */` 區塊註解(可跨行、行內、整行;非巢狀,首個 `*/` 結束;未閉合視為
/// 至檔尾)。**保留換行**以維持行號一致(錯誤定位不漂移);註解內容以空白取代,
/// 使整行註解成空白行(被 parser 略過)、行尾/行內註解不影響縮排與 token。
/// 註解於 canonical 不保留(IR dump 慣例)。
///
/// **公開供 `.chg` 重用**:三種格式(`.qy`/`.lang`/`.chg`)共用同一套區塊註解
/// (擁有者 2026-07-12 定案;`#` 在 `.qy` 已被詞界 D19 佔用,故不可當註解)。
pub fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_comment = false;
    while let Some(c) = chars.next() {
        if in_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_comment = false;
            } else if c == '\n' {
                out.push('\n'); // 保留行號
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_comment = true;
        } else {
            out.push(c);
        }
    }
    out
}

pub fn parse(src: &str) -> Result<Language, ParseError> {
    let mut lang = Language::new();
    let src = strip_comments(src);
    let lines: Vec<Line> = src
        .lines()
        .enumerate()
        .map(|(i, raw)| Line {
            no: i + 1,
            indent: indent_of(raw),
            text: raw.trim().to_owned(),
        })
        .collect();

    let mut i = 0usize;
    let mut seen_language = false;

    while i < lines.len() {
        let ln = &lines[i];
        if ln.text.is_empty() {
            i += 1;
            continue;
        }
        // 舊 v2 schema 標頭:**顯式拒絕**,不是靜默忽略,也不是放它掉進 dsl 域。
        //
        // v1 於 2026-07-24 硬移除後,這行對解析、canonical dump 與 identity digest
        // 皆零影響——留著「認得但無意義」的語法會讓作者以為它有作用。
        //
        // 但也不能只把分支刪掉:`.lang` 把首個 language 構造前的內容 verbatim 交給
        // dsl 域(裁決1),所以那行會被送進 tshiatun 的 lexer,得到
        // `parse error at line 1: … expected: {Arrow, Lt, RepeatPlus, …}`
        // ——拿著舊檔的人完全看不出問題在哪。同 I17-d:顯式拒絕優於難懂的下游錯誤。
        if ln.text == "schema conlang.lang/v2" {
            return Err(err(
                ln.no,
                "`schema conlang.lang/v2` is retired; delete this line \
                 (it never affected parsing, canonical dump, or identity digest)",
            ));
        }
        if ln.text.starts_with("prosody =") {
            return Err(err(
                ln.no,
                "`prosody = …` is retired; use DSL `Prosody LEVEL < …` in the declaration preamble",
            ));
        }
        // dsl 域 verbatim(首個 language 頭之前)
        if !seen_language && !is_language_head(&ln.text) {
            lang.dsl_decls.push(ln.text.clone());
            i += 1;
            continue;
        }
        seen_language = true;

        if ln.text == "distribution:" {
            let base = ln.indent;
            i += 1;
            while i < lines.len() && (lines[i].text.is_empty() || lines[i].indent > base) {
                let l2 = &lines[i];
                i += 1;
                if l2.text.is_empty() {
                    continue;
                }
                let Some((k, v)) = l2.text.split_once('=') else {
                    return Err(err(l2.no, "distribution entry must be `key = value`"));
                };
                lang.distribution
                    .push((k.trim().to_owned(), v.trim().to_owned()));
            }
        } else if let Some((kw, name)) = container_head(&ln.text) {
            let header_indent = ln.indent;
            let name = name.to_owned();
            i += 1;
            // 蒐集 body(縮排 > header 的非空行)
            let mut body: Vec<Line> = Vec::new();
            while i < lines.len() {
                if lines[i].text.is_empty() {
                    i += 1;
                    continue;
                }
                if lines[i].indent <= header_indent {
                    break;
                }
                body.push(Line {
                    no: lines[i].no,
                    indent: lines[i].indent,
                    text: lines[i].text.clone(),
                });
                i += 1;
            }
            let blocks = parse_body(&mut lang, &body)?;
            match kw {
                "sign" => {
                    let items: Vec<SignItem> = blocks.into_iter().flat_map(|b| b.items).collect();
                    lang.add_sign(name, items);
                }
                _ => lang.add_trait(TraitDef {
                    name,
                    global: kw == "global trait",
                    blocks,
                }),
            }
        } else {
            return Err(err(ln.no, format!("unexpected line {:?}", ln.text)));
        }
    }
    Ok(lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsl_region_is_verbatim_by_file_position() {
        let l = parse("Feature voice(+voice, -voice)\n\ndistribution:\n    /k/ = 0.15\n").unwrap();
        assert_eq!(l.dsl_decls, vec!["Feature voice(+voice, -voice)"]);
        assert_eq!(l.distribution, vec![("/k/".to_owned(), "0.15".to_owned())]);
    }

    #[test]
    fn rejects_retired_language_prosody_syntax() {
        let error = parse("prosody = μ σ\n").unwrap_err();
        assert!(error.msg.contains("retired"), "{error}");
    }

    #[test]
    fn errors_are_located() {
        let e = parse("sign s:\n    ??bad\n").unwrap_err();
        assert_eq!(e.line, 2);
    }

    #[test]
    fn parses_syn_slot_feature_bindings_with_source_locations() {
        let language = parse(
            r#"sign Clause:
    syn:
        slots:
            target [Nominal]
            source [Nominal]
        slot_features:
            target.case = nominative
            target.number = $slot.source.syn.number
"#,
        )
        .expect("parse slot feature bindings");
        let clause = language.sign_named("Clause").expect("Clause sign");
        let bindings = clause
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::SlotFeatureBinding(binding) => Some(binding),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].slot, "target");
        assert_eq!(bindings[0].feature, "case");
        assert_eq!(bindings[0].value, "nominative");
        assert_eq!(bindings[0].source.line, 7);
        assert_eq!(bindings[1].slot, "target");
        assert_eq!(bindings[1].feature, "number");
        assert_eq!(bindings[1].value, "$slot.source.syn.number");
        assert_eq!(bindings[1].source.line, 8);
    }

    #[test]
    fn rejects_malformed_or_non_syn_slot_feature_bindings() {
        let outside_syn = parse(
            r#"sign Bad:
    sem:
        slot_features:
            target.case = nominative
"#,
        )
        .unwrap_err();
        assert_eq!(outside_syn.line, 3);
        assert!(outside_syn.msg.contains("only under `syn:`"));

        let invalid_read = parse(
            r#"sign Bad:
    syn:
        slot_features:
            target.case = $slot.source.sem.number
"#,
        )
        .unwrap_err();
        assert_eq!(invalid_read.line, 4);
        assert!(invalid_read.msg.contains("enum literal"));

        let invalid_target = parse(
            r#"sign Bad:
    syn:
        slot_features:
            target.case.extra = nominative
"#,
        )
        .unwrap_err();
        assert_eq!(invalid_target.line, 4);
        assert!(invalid_target.msg.contains("TARGET_SLOT.FEATURE"));
    }
}
