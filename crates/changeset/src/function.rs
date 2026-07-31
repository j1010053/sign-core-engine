//! P49/P50 ④ —— **`.chg` 定義文件模式**(歷時 function 層的定義)。
//!
//! 定義是**函數,不綁特定 Language**,故沒有 `base_source`/`base_identities`;
//! 以 `package <ns>:` 頭與可 replay 的 `changeset <ns>:` 區分,只收 `function`、
//! 不收 `statement`。定義住套件的 `code/*.chg`(P29 三層目錄)。
//!
//! **Recipe/Goal 不是關鍵字**(P48)——body 的執行語意由既有的 `case`/`when` 承載:
//!
//! | body 形狀 | 語意 | 慣稱 |
//! |---|---|---|
//! | 純序列 | 依序全跑 | Recipe |
//! | `case:` | 第一個 Matched | 確定性分支 |
//! | `when:` | 所有 Matched 依序合併 | Goal 的候選列舉 |

use crate::rewrite::DonorScope;
use crate::{
    apply_statement_structural, call, parse_call_head, split_named_argument, PrimitiveEdit,
    ReplayError,
};
use conlang_language::{
    compile_document, sample_weighted_index, LanguageDocument, LibraryCatalog, LibraryExportKind,
    LibraryKind, LibraryPackage, LibrarySpec, SignItem,
};
use std::collections::{BTreeMap, BTreeSet};

pub const FUNCTIONS_SCHEMA_V1: &str = "conlang.functions/v1";

/// 一次呼叫(body 的一行)。與 `.chg` 執行區的呼叫同構(P47)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCall {
    pub name: String,
    pub positional: Option<String>,
    pub named: Vec<(String, String)>,
}

/// `case:`/`when:` 的一個分支。`guard == None` 代表 `else`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBranch {
    pub guard: Option<String>,
    pub call: FunctionCall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBody {
    /// 純序列 = **依序全跑**(慣稱 Recipe)。
    Sequence(Vec<FunctionCall>),
    /// `case:` = **第一個 Matched 的分支**。
    Case(Vec<FunctionBranch>),
    /// `when:` = **所有 Matched 依序合併**(慣稱 Goal 的候選列舉)。
    When(Vec<FunctionBranch>),
}

/// 形式參數。`constraint` 是 slot 式範疇約束(`verb [Verb]`),取代大部分 guard。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParam {
    pub name: String,
    pub constraint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<FunctionParam>,
    /// header 尾綴的 `/ guard`(參數約束表達不了的條件才用)。
    pub guard: Option<String>,
    pub body: FunctionBody,
}

/// One fully substituted call observed while evaluating an ordinary function.
///
/// `stack` records function ownership; the final `call` is always an atomic
/// rewrite at this stage. Goal candidates are returned separately and never
/// appear as executed trace steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionTraceStep {
    pub stack: Vec<String>,
    pub call: FunctionCall,
}

#[derive(Debug, Clone)]
pub struct FunctionExecution {
    pub document: LanguageDocument,
    pub edits: Vec<PrimitiveEdit>,
    pub trace: Vec<FunctionTraceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCandidates {
    pub source: String,
    pub candidates: Vec<FunctionCall>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalSelectionTrace {
    pub algorithm: &'static str,
    pub seed: u64,
    pub source: String,
    pub ordered: Vec<(FunctionCall, f64)>,
    pub selected_index: usize,
    pub selected: FunctionCall,
}

#[derive(Debug, Clone)]
pub struct GoalExecution {
    pub selection: GoalSelectionTrace,
    pub execution: FunctionExecution,
}

#[derive(Debug, Clone)]
struct WeightEntry {
    weight: f64,
    package: String,
    priority: i32,
}

/// Read-only statistical weights for Goal candidate Recipes (P12).
#[derive(Debug, Clone, Default)]
pub struct WeightDb {
    entries: BTreeMap<(String, String), WeightEntry>,
}

impl WeightDb {
    pub fn weight(&self, goal: &str, recipe: &str) -> Result<f64, ReplayError> {
        self.entries
            .get(&(goal.to_owned(), recipe.to_owned()))
            .map(|entry| entry.weight)
            .ok_or_else(|| {
                ReplayError::Parse(format!(
                    "WEIGHT_DB_MISSING: Goal {goal:?} candidate Recipe {recipe:?} has no weight"
                ))
            })
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A generic function either executes a sequence/case or stops at a `when:`
/// candidate boundary. The enum deliberately avoids Recipe/Goal syntax-level
/// types: those names remain std authoring file groupings.
#[derive(Debug, Clone)]
pub enum FunctionEvaluation {
    Executed(Box<FunctionExecution>),
    Candidates(FunctionCandidates),
}

/// 一份定義文件(套件的一個 `code/*.chg`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionPackage {
    pub namespace: String,
    pub schema: String,
    pub functions: Vec<FunctionDef>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// 解析一份定義文件。**只收 `function`**;遇到 `statement` 明確拒絕。
pub fn parse_functions(source: &str) -> Result<FunctionPackage, ReplayError> {
    // 與 `.chg`/`.lang`/`.qy` 共用 `/* … */` 區塊註解。
    let source = conlang_language::parser::strip_comments(source);
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0usize;
    // 略過空行與註解。
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }
    let header = lines
        .get(index)
        .map(|line| line.trim())
        .ok_or_else(|| ReplayError::Parse("empty function package".to_owned()))?;
    let namespace = header
        .strip_prefix("package ")
        .and_then(|rest| rest.strip_suffix(':'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ReplayError::Parse(
                "expected `package <namespace>:` (a definition document has no base)".to_owned(),
            )
        })?
        .to_owned();
    index += 1;

    let mut schema = None;
    let mut functions = Vec::new();
    while index < lines.len() {
        let raw = lines[index];
        let text = raw.trim();
        if text.is_empty() {
            index += 1;
            continue;
        }
        if let Some(value) = text.strip_prefix("schema = ") {
            schema = Some(value.trim().to_owned());
            index += 1;
            continue;
        }
        if text.starts_with("statement ") {
            return Err(ReplayError::Parse(
                "a definition document holds functions only; `statement` belongs in a changeset"
                    .to_owned(),
            ));
        }
        if text.starts_with("function ") {
            let (definition, next) = parse_function(&lines, index)?;
            functions.push(definition);
            index = next;
            continue;
        }
        return Err(ReplayError::Parse(format!(
            "unexpected line in a definition document: {text:?}"
        )));
    }

    let schema = schema.ok_or_else(|| ReplayError::Parse("missing schema".to_owned()))?;
    if schema != FUNCTIONS_SCHEMA_V1 {
        return Err(ReplayError::Parse(format!(
            "unsupported function schema {schema:?}"
        )));
    }
    let package = FunctionPackage {
        namespace,
        schema,
        functions,
    };
    check_duplicates(&package)?;
    check_cycles(&package)?;
    Ok(package)
}

/// `function Name(p [C], q) [/ guard]:`
fn parse_function(lines: &[&str], start: usize) -> Result<(FunctionDef, usize), ReplayError> {
    let header_indent = indent_of(lines[start]);
    let header = lines[start].trim();
    let header = header.strip_suffix(':').ok_or_else(|| {
        ReplayError::Parse(format!("function header must end with `:`: {header}"))
    })?;
    // header 尾綴的 `/ guard`(在括號外)。
    let (signature, guard) = split_guard(header);
    let signature = signature.trim();
    let rest = signature
        .strip_prefix("function ")
        .ok_or_else(|| ReplayError::Parse("expected `function <name>(…)`".to_owned()))?;
    let (name, params) = parse_signature(rest)?;

    let mut index = start + 1;
    let mut body_lines = Vec::new();
    while index < lines.len() {
        let raw = lines[index];
        if raw.trim().is_empty() {
            index += 1;
            continue;
        }
        if indent_of(raw) <= header_indent {
            break;
        }
        body_lines.push(raw);
        index += 1;
    }
    if body_lines.is_empty() {
        return Err(ReplayError::Parse(format!(
            "function {name:?} has an empty body"
        )));
    }
    let body = parse_body(&body_lines, &name)?;
    Ok((
        FunctionDef {
            name,
            params,
            guard,
            body,
        },
        index,
    ))
}

/// 切 header 尾綴的 `/ guard`,忽略括號內的 `/`。
fn split_guard(header: &str) -> (&str, Option<String>) {
    let mut depth = 0usize;
    for (index, character) in header.char_indices() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '/' if depth == 0 => {
                let guard = header[index + 1..].trim();
                return (
                    &header[..index],
                    (!guard.is_empty()).then(|| guard.to_owned()),
                );
            }
            _ => {}
        }
    }
    (header, None)
}

/// `Name(p [C], q)` → (Name, params)
fn parse_signature(source: &str) -> Result<(String, Vec<FunctionParam>), ReplayError> {
    let (name, args) = parse_call_head(source)
        .ok_or_else(|| ReplayError::Parse(format!("invalid function signature: {source:?}")))?;
    let mut params = Vec::new();
    for arg in args {
        if arg.is_empty() {
            continue;
        }
        // `verb [Verb]`:比照 `.lang` 的 slot 寫法。
        let (raw_name, constraint) = match arg.split_once('[') {
            Some((head, tail)) => {
                let constraint = tail.strip_suffix(']').ok_or_else(|| {
                    ReplayError::Parse(format!("unclosed parameter constraint in {arg:?}"))
                })?;
                (head.trim(), Some(constraint.trim().to_owned()))
            }
            None => (arg.trim(), None),
        };
        if raw_name.is_empty()
            || !raw_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ReplayError::Parse(format!(
                "parameter name must be an identifier: {arg:?}"
            )));
        }
        params.push(FunctionParam {
            name: raw_name.to_owned(),
            constraint,
        });
    }
    Ok((name, params))
}

fn parse_body(lines: &[&str], owner: &str) -> Result<FunctionBody, ReplayError> {
    let head = lines[0].trim();
    if head == "case:" || head == "when:" {
        let mut branches = Vec::new();
        for raw in &lines[1..] {
            let text = raw.trim();
            if text.is_empty() {
                continue;
            }
            // `else <call>` = 無守衛的兜底分支;其餘為 `<call> / guard`。
            let (text, is_else) = match text.strip_prefix("else ") {
                Some(rest) => (rest.trim(), true),
                None => (text, false),
            };
            let (call_text, guard) = split_guard(text);
            if is_else && guard.is_some() {
                return Err(ReplayError::Parse(format!(
                    "function {owner:?}: an `else` branch cannot carry a guard"
                )));
            }
            branches.push(FunctionBranch {
                guard,
                call: parse_call(call_text.trim(), owner)?,
            });
        }
        if branches.is_empty() {
            return Err(ReplayError::Parse(format!(
                "function {owner:?}: `{head}` has no branches"
            )));
        }
        return Ok(if head == "case:" {
            FunctionBody::Case(branches)
        } else {
            FunctionBody::When(branches)
        });
    }
    let mut calls = Vec::new();
    for raw in lines {
        let text = raw.trim();
        if text.is_empty() {
            continue;
        }
        calls.push(parse_call(text, owner)?);
    }
    Ok(FunctionBody::Sequence(calls))
}

fn parse_call(text: &str, owner: &str) -> Result<FunctionCall, ReplayError> {
    let (name, args) = parse_call_head(text).ok_or_else(|| {
        ReplayError::Parse(format!(
            "function {owner:?}: body line is not a call: {text:?}"
        ))
    })?;
    let mut positional = None;
    let mut named = Vec::new();
    for arg in args {
        match split_named_argument(&arg) {
            Some((key, value)) => named.push((key, value)),
            None if arg.is_empty() => {}
            None if positional.is_none() => positional = Some(arg),
            None => {
                return Err(ReplayError::Parse(format!(
                    "function {owner:?}: {name}(…) takes at most one positional argument"
                )))
            }
        }
    }
    Ok(FunctionCall {
        name,
        positional,
        named,
    })
}

fn check_duplicates(package: &FunctionPackage) -> Result<(), ReplayError> {
    let mut seen = BTreeSet::new();
    for definition in &package.functions {
        if !seen.insert(definition.name.as_str()) {
            return Err(ReplayError::Parse(format!(
                "package {:?} defines {:?} more than once",
                package.namespace, definition.name
            )));
        }
    }
    Ok(())
}

/// **P48:必須偵測循環呼叫**(A→B→A)——這是終止性要求,與分層無關。
fn check_cycles(package: &FunctionPackage) -> Result<(), ReplayError> {
    let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for definition in &package.functions {
        let callees = match &definition.body {
            FunctionBody::Sequence(calls) => calls.iter().map(|c| c.name.as_str()).collect(),
            FunctionBody::Case(branches) | FunctionBody::When(branches) => {
                branches.iter().map(|b| b.call.name.as_str()).collect()
            }
        };
        graph.insert(definition.name.as_str(), callees);
    }
    // 只在**本套件內定義**的名字上走圖;跨套件的環由載入端在合併表上再查一次。
    let mut state: BTreeMap<&str, u8> = BTreeMap::new();
    fn walk<'a>(
        node: &'a str,
        graph: &BTreeMap<&'a str, Vec<&'a str>>,
        state: &mut BTreeMap<&'a str, u8>,
    ) -> Result<(), String> {
        match state.get(node) {
            Some(1) => return Err(node.to_owned()),
            Some(2) => return Ok(()),
            _ => {}
        }
        state.insert(node, 1);
        for next in graph.get(node).into_iter().flatten() {
            if graph.contains_key(next) {
                walk(next, graph, state)?;
            }
        }
        state.insert(node, 2);
        Ok(())
    }
    for name in graph.keys() {
        if let Err(cycle) = walk(name, &graph, &mut state) {
            return Err(ReplayError::Parse(format!(
                "package {:?}: function {cycle:?} takes part in a call cycle",
                package.namespace
            )));
        }
    }
    Ok(())
}

// ── P50 ③:把啟用套件的 `code/*.chg` 載成一張 function 表 ──────────────────

/// 一個解析出來的 function 及其來源套件(供 priority 消歧與錯誤訊息)。
#[derive(Debug, Clone)]
pub struct ResolvedFunction {
    pub package: String,
    pub priority: i32,
    pub definition: FunctionDef,
}

/// 合併後的 function 表。名字解析走 **auto-discovery + priority 四層**(P29/P50);
/// 同名同 priority 時**保留兩者並在查詢時報錯**,強制以 `套件::符號` 消歧。
#[derive(Debug, Clone, Default)]
pub struct FunctionTable {
    entries: BTreeMap<String, Vec<ResolvedFunction>>,
}

impl FunctionTable {
    /// `套件::符號` 全名,或裸名(auto-discovery)。
    pub fn get(&self, name: &str) -> Result<&FunctionDef, ReplayError> {
        if let Some((package, symbol)) = name.split_once("::") {
            return self
                .entries
                .get(symbol)
                .and_then(|found| found.iter().find(|entry| entry.package == package))
                .map(|entry| &entry.definition)
                .ok_or_else(|| ReplayError::Parse(format!("unknown function {name:?}")));
        }
        let found = self
            .entries
            .get(name)
            .ok_or_else(|| ReplayError::Parse(format!("unknown function {name:?}")))?;
        let top = found
            .iter()
            .max_by_key(|entry| entry.priority)
            .expect("non-empty");
        // 同名同 priority → 強制消歧(P29「warn + 強制消歧」的嚴格面:
        // 呼叫時才不得含糊)。
        if found
            .iter()
            .filter(|entry| entry.priority == top.priority)
            .count()
            > 1
        {
            return Err(ReplayError::Parse(format!(
                "ambiguous function {name:?} at equal priority; qualify it as `package::{name}`"
            )));
        }
        Ok(&top.definition)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        match name.split_once("::") {
            Some((package, symbol)) => self
                .entries
                .get(symbol)
                .is_some_and(|found| found.iter().any(|entry| entry.package == package)),
            None => self.entries.contains_key(name),
        }
    }
}

fn bind_parameters(
    definition: &FunctionDef,
    invocation: &FunctionCall,
    document: &LanguageDocument,
    libraries: &LibrarySpec,
) -> Result<BTreeMap<String, String>, ReplayError> {
    let mut bindings = BTreeMap::new();
    if let Some(value) = &invocation.positional {
        let parameter = definition.params.first().ok_or_else(|| {
            ReplayError::Parse(format!(
                "function {:?} does not take a positional argument",
                definition.name
            ))
        })?;
        bindings.insert(parameter.name.clone(), value.clone());
    }
    for (name, value) in &invocation.named {
        if !definition
            .params
            .iter()
            .any(|parameter| &parameter.name == name)
        {
            return Err(ReplayError::Parse(format!(
                "function {:?} has no parameter {name:?}",
                definition.name
            )));
        }
        if bindings.insert(name.clone(), value.clone()).is_some() {
            return Err(ReplayError::Parse(format!(
                "function {:?} receives parameter {name:?} more than once",
                definition.name
            )));
        }
    }
    for parameter in &definition.params {
        let value = bindings.get(&parameter.name).ok_or_else(|| {
            ReplayError::Parse(format!(
                "function {:?} is missing parameter {:?}",
                definition.name, parameter.name
            ))
        })?;
        if let Some(constraint) = &parameter.constraint {
            validate_constraint(
                &definition.name,
                &parameter.name,
                value,
                constraint,
                document,
                libraries,
            )?;
        }
    }
    Ok(bindings)
}

fn validate_constraint(
    function: &str,
    parameter: &str,
    value: &str,
    constraint: &str,
    document: &LanguageDocument,
    libraries: &LibrarySpec,
) -> Result<(), ReplayError> {
    let sign = value
        .trim()
        .strip_prefix("sign(")
        .and_then(|rest| rest.strip_suffix(')'))
        .map(str::trim)
        .map(|name| name.trim_matches('"'))
        .ok_or_else(|| {
            ReplayError::Parse(format!(
                "function {function:?} parameter {parameter:?} [{constraint}] expects sign(\"…\"), got {value:?}"
            ))
        })?;
    let definition = document
        .language()
        .signs
        .iter()
        .find(|candidate| candidate.name == sign)
        .ok_or_else(|| {
            ReplayError::Parse(format!(
                "function {function:?} parameter {parameter:?} names unknown sign {sign:?}"
            ))
        })?;
    let system = compile_document(document, libraries)?;
    if !definition.items.iter().any(|item| {
        matches!(
            item,
            SignItem::Belongs(name)
                if name == constraint || system.ontology.category_is_a(name, constraint)
        )
    }) {
        return Err(ReplayError::Parse(format!(
            "function {function:?} parameter {parameter:?} requires [{constraint}], but sign {sign:?} does not belong to it"
        )));
    }
    Ok(())
}

fn substitute(value: &str, bindings: &BTreeMap<String, String>) -> String {
    bindings
        .get(value)
        .cloned()
        .unwrap_or_else(|| value.to_owned())
}

fn bind_call(call: &FunctionCall, bindings: &BTreeMap<String, String>) -> FunctionCall {
    FunctionCall {
        name: call.name.clone(),
        positional: call
            .positional
            .as_deref()
            .map(|value| substitute(value, bindings)),
        named: call
            .named
            .iter()
            .map(|(name, value)| (name.clone(), substitute(value, bindings)))
            .collect(),
    }
}

struct EvaluationState<'a> {
    table: &'a FunctionTable,
    libraries: &'a LibrarySpec,
    document: LanguageDocument,
    edits: Vec<PrimitiveEdit>,
    trace: Vec<FunctionTraceStep>,
    stack: Vec<String>,
}

impl EvaluationState<'_> {
    fn evaluate(
        &mut self,
        invocation: &FunctionCall,
    ) -> Result<Option<FunctionCandidates>, ReplayError> {
        let definition = self.table.get(&invocation.name)?.clone();
        if definition.guard.is_some() {
            return Err(ReplayError::Parse(format!(
                "FUNCTION_GUARD_UNSUPPORTED: function {:?} has a header guard",
                definition.name
            )));
        }
        let bindings = bind_parameters(&definition, invocation, &self.document, self.libraries)?;
        self.stack.push(definition.name.clone());
        let result = match &definition.body {
            FunctionBody::Sequence(calls) => {
                for call in calls {
                    let call = bind_call(call, &bindings);
                    self.evaluate_call(&call)?;
                }
                None
            }
            FunctionBody::Case(branches) => {
                let branch = branches
                    .iter()
                    .find(|branch| branch.guard.is_none())
                    .ok_or_else(|| {
                        ReplayError::Parse(format!(
                            "FUNCTION_GUARD_UNSUPPORTED: function {:?} has no unguarded case branch",
                            definition.name
                        ))
                    })?;
                self.evaluate_call(&bind_call(&branch.call, &bindings))?;
                None
            }
            FunctionBody::When(branches) => {
                if branches.iter().any(|branch| branch.guard.is_some()) {
                    return Err(ReplayError::Parse(format!(
                        "FUNCTION_GUARD_UNSUPPORTED: function {:?} has guarded candidates",
                        definition.name
                    )));
                }
                let candidates = branches
                    .iter()
                    .map(|branch| bind_call(&branch.call, &bindings))
                    .collect::<Vec<_>>();
                for candidate in &candidates {
                    let target = self.table.get(&candidate.name)?;
                    if matches!(target.body, FunctionBody::When(_)) {
                        return Err(ReplayError::Parse(format!(
                            "FUNCTION_CANDIDATE_LAYER: function {:?} yields candidate {:?}, which is another candidate function",
                            definition.name, candidate.name
                        )));
                    }
                }
                Some(FunctionCandidates {
                    source: definition.name.clone(),
                    candidates,
                })
            }
        };
        self.stack.pop();
        Ok(result)
    }

    fn evaluate_call(&mut self, invocation: &FunctionCall) -> Result<(), ReplayError> {
        if self.table.contains(&invocation.name) {
            if self.evaluate(invocation)?.is_some() {
                return Err(ReplayError::Parse(format!(
                    "FUNCTION_CANDIDATES_REQUIRE_SELECTION: {:?} cannot execute inside a sequence",
                    invocation.name
                )));
            }
            return Ok(());
        }
        let call = call::Call {
            name: &invocation.name,
            positional: invocation.positional.as_deref(),
            named: &invocation.named,
            block: None,
        };
        let edits = call::lower(&call, &self.document, &DonorScope::new())?;
        let (document, _) = apply_statement_structural(&self.document, 0, &edits, self.libraries)?;
        self.trace.push(FunctionTraceStep {
            stack: self.stack.clone(),
            call: invocation.clone(),
        });
        self.edits.extend(edits);
        self.document = document;
        Ok(())
    }
}

/// Evaluate one loaded function without live services, weighting, sampling, or
/// persistence. A sequence/case returns a committed temporary document and its
/// primitive trace; `when:` stops at an ordered candidate list.
pub fn evaluate_function_offline(
    table: &FunctionTable,
    invocation: &FunctionCall,
    document: &LanguageDocument,
    libraries: &LibrarySpec,
) -> Result<FunctionEvaluation, ReplayError> {
    let mut state = EvaluationState {
        table,
        libraries,
        document: document.clone(),
        edits: Vec::new(),
        trace: Vec::new(),
        stack: Vec::new(),
    };
    match state.evaluate(invocation)? {
        Some(candidates) => Ok(FunctionEvaluation::Candidates(candidates)),
        None => Ok(FunctionEvaluation::Executed(Box::new(FunctionExecution {
            document: state.document,
            edits: state.edits,
            trace: state.trace,
        }))),
    }
}

pub fn select_goal_candidate(
    candidates: &FunctionCandidates,
    weights: &WeightDb,
    seed: u64,
) -> Result<GoalSelectionTrace, ReplayError> {
    let ordered = candidates
        .candidates
        .iter()
        .map(|candidate| {
            Ok((
                candidate.clone(),
                weights.weight(&candidates.source, &candidate.name)?,
            ))
        })
        .collect::<Result<Vec<_>, ReplayError>>()?;
    let distribution = ordered
        .iter()
        .map(|(_, weight)| *weight)
        .collect::<Vec<_>>();
    let sample = sample_weighted_index(&distribution, seed).map_err(|error| {
        ReplayError::Parse(format!(
            "GOAL_SAMPLING: Goal {:?} cannot be sampled: {error}",
            candidates.source
        ))
    })?;
    let selected = ordered
        .get(sample.selected_index)
        .expect("sampler returns an in-range index")
        .0
        .clone();
    Ok(GoalSelectionTrace {
        algorithm: sample.algorithm,
        seed,
        source: candidates.source.clone(),
        ordered,
        selected_index: sample.selected_index,
        selected,
    })
}

/// Authoring-time Goal path. The selected concrete Recipe call, rather than
/// the random operation, is what callers persist into `.chg` for replay.
pub fn evaluate_goal_offline(
    table: &FunctionTable,
    invocation: &FunctionCall,
    document: &LanguageDocument,
    libraries: &LibrarySpec,
    weights: &WeightDb,
    seed: u64,
) -> Result<GoalExecution, ReplayError> {
    let FunctionEvaluation::Candidates(candidates) =
        evaluate_function_offline(table, invocation, document, libraries)?
    else {
        return Err(ReplayError::Parse(format!(
            "FUNCTION_NOT_GOAL: {:?} executes directly instead of yielding candidates",
            invocation.name
        )));
    };
    let selection = select_goal_candidate(&candidates, weights, seed)?;
    let FunctionEvaluation::Executed(execution) =
        evaluate_function_offline(table, &selection.selected, document, libraries)?
    else {
        return Err(ReplayError::Parse(format!(
            "FUNCTION_CANDIDATE_LAYER: Goal {:?} selected another candidate function",
            invocation.name
        )));
    };
    Ok(GoalExecution {
        selection,
        execution: *execution,
    })
}

/// priority 四層(P29):未啟用不參與 < std < 已啟用 plugin < 專案本地。
fn package_priority(package: &LibraryPackage) -> i32 {
    let base = match package.id.kind {
        LibraryKind::Std => 0,
        LibraryKind::Natural | LibraryKind::Plugin => 1_000,
    };
    base + package.priority
}

/// 由目前選中的套件載入 function 表。**auto-discovery,無顯式 import**(P50)。
pub fn load_functions(
    catalog: &LibraryCatalog,
    spec: &LibrarySpec,
) -> Result<FunctionTable, ReplayError> {
    let selection = catalog
        .select(spec)
        .map_err(|error| ReplayError::Parse(error.to_string()))?;
    let chosen: Vec<&LibraryPackage> = selection
        .packages
        .iter()
        .map(|id| {
            catalog
                .packages()
                .iter()
                .find(|candidate| &candidate.id == id)
                .expect("catalog selection returns catalog IDs")
        })
        .collect();
    functions_from_packages(&chosen)
}

pub fn load_weight_db(
    catalog: &LibraryCatalog,
    spec: &LibrarySpec,
) -> Result<WeightDb, ReplayError> {
    let selection = catalog
        .select(spec)
        .map_err(|error| ReplayError::Parse(error.to_string()))?;
    let chosen = selection
        .packages
        .iter()
        .map(|id| {
            catalog
                .packages()
                .iter()
                .find(|package| &package.id == id)
                .expect("catalog selection returns catalog IDs")
        })
        .collect::<Vec<_>>();
    weight_db_from_packages(&chosen)
}

pub fn weight_db_from_packages(packages: &[&LibraryPackage]) -> Result<WeightDb, ReplayError> {
    let mut database = WeightDb::default();
    for package in packages.iter().copied() {
        let priority = package_priority(package);
        for source in package
            .data_sources
            .iter()
            .filter(|source| source.path.ends_with("/weights.tsv"))
        {
            parse_weight_source(
                &mut database,
                package,
                priority,
                &source.path,
                source.source,
            )?;
        }
    }
    Ok(database)
}

fn parse_weight_source(
    database: &mut WeightDb,
    package: &LibraryPackage,
    priority: i32,
    path: &str,
    source: &str,
) -> Result<(), ReplayError> {
    let mut lines = source.lines().enumerate();
    let Some((_, header)) = lines.find(|(_, line)| !line.trim().is_empty()) else {
        return Err(ReplayError::Parse(format!(
            "WEIGHT_DB_SCHEMA: {}:{path} is empty",
            package.id
        )));
    };
    if header.trim_end() != "goal\trecipe\tweight" {
        return Err(ReplayError::Parse(format!(
            "WEIGHT_DB_SCHEMA: {}:{path} expects header goal\\trecipe\\tweight",
            package.id
        )));
    }
    let mut local = BTreeSet::new();
    for (line_index, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 3 || columns[0].is_empty() || columns[1].is_empty() {
            return Err(ReplayError::Parse(format!(
                "WEIGHT_DB_SCHEMA: {}:{path}:{} expects goal, recipe, weight",
                package.id,
                line_index + 1
            )));
        }
        let weight = columns[2].parse::<f64>().map_err(|_| {
            ReplayError::Parse(format!(
                "WEIGHT_DB_WEIGHT: {}:{path}:{} has invalid weight {:?}",
                package.id,
                line_index + 1,
                columns[2]
            ))
        })?;
        if !weight.is_finite() || weight < 0.0 {
            return Err(ReplayError::Parse(format!(
                "WEIGHT_DB_WEIGHT: {}:{path}:{} weight must be finite and non-negative, got {weight}",
                package.id,
                line_index + 1
            )));
        }
        let key = (columns[0].to_owned(), columns[1].to_owned());
        if !local.insert(key.clone()) {
            return Err(ReplayError::Parse(format!(
                "WEIGHT_DB_DUPLICATE: {}:{path}:{} repeats Goal {:?} Recipe {:?}",
                package.id,
                line_index + 1,
                key.0,
                key.1
            )));
        }
        if let Some(existing) = database.entries.get(&key) {
            if existing.priority == priority {
                return Err(ReplayError::Parse(format!(
                    "WEIGHT_DB_AMBIGUOUS: Goal {:?} Recipe {:?} is defined by {:?} and {} at equal priority",
                    key.0, key.1, existing.package, package.id
                )));
            }
            if existing.priority > priority {
                continue;
            }
        }
        database.entries.insert(
            key,
            WeightEntry {
                weight,
                package: package.id.to_string(),
                priority,
            },
        );
    }
    Ok(())
}

/// 載入核心:直接吃一組套件。與 `load_functions` 分開是為了可用合成套件測到
/// **export 過濾**與**export/定義不一致**這兩條路徑(catalog 只能由嵌入建構)。
pub fn functions_from_packages(packages: &[&LibraryPackage]) -> Result<FunctionTable, ReplayError> {
    let mut table = FunctionTable::default();
    for package in packages.iter().copied() {
        if package.function_sources.is_empty() && package.functions.trim().is_empty() {
            continue;
        }
        let mut definitions = Vec::new();
        if package.function_sources.is_empty() {
            let parsed = parse_functions(package.functions)?;
            if parsed.namespace != package.id.to_string() {
                return Err(ReplayError::Parse(format!(
                    "package {} function document declares namespace {:?}",
                    package.id, parsed.namespace
                )));
            }
            definitions.extend(parsed.functions);
        } else {
            if package.function_sources.len() != package.function_paths.len() {
                return Err(ReplayError::Parse(format!(
                    "package {} declares {} function paths but provides {} sources",
                    package.id,
                    package.function_paths.len(),
                    package.function_sources.len()
                )));
            }
            for (expected_path, source) in
                package.function_paths.iter().zip(&package.function_sources)
            {
                if expected_path != &source.path {
                    return Err(ReplayError::Parse(format!(
                        "package {} expected function source {expected_path:?}, got {:?}",
                        package.id, source.path
                    )));
                }
                let parsed = parse_functions(source.source).map_err(|error| {
                    ReplayError::Parse(format!(
                        "package {} function source {:?}: {error}",
                        package.id, source.path
                    ))
                })?;
                if parsed.namespace != package.id.to_string() {
                    return Err(ReplayError::Parse(format!(
                        "package {} function source {:?} declares namespace {:?}",
                        package.id, source.path, parsed.namespace
                    )));
                }
                definitions.extend(parsed.functions);
            }
        }
        let combined = FunctionPackage {
            namespace: package.id.to_string(),
            schema: FUNCTIONS_SCHEMA_V1.to_owned(),
            functions: definitions,
        };
        check_duplicates(&combined)?;
        check_cycles(&combined)?;
        // export 表是唯一穩定契約(P29):只有列在 exports 的 function 對外可見。
        let exported: BTreeSet<&str> = package
            .exports
            .iter()
            .filter(|export| export.kind == LibraryExportKind::Function)
            .map(|export| export.alias.as_str())
            .collect();
        for alias in &exported {
            if !combined
                .functions
                .iter()
                .any(|definition| &definition.name == alias)
            {
                return Err(ReplayError::Parse(format!(
                    "package {} exports function {alias:?} but its code/*.chg does not define it",
                    package.id
                )));
            }
        }
        for definition in combined.functions {
            if !exported.contains(definition.name.as_str()) {
                continue; // 未 export = 套件內部,不進表
            }
            table
                .entries
                .entry(definition.name.clone())
                .or_default()
                .push(ResolvedFunction {
                    package: package.id.to_string(),
                    priority: package_priority(package),
                    definition,
                });
        }
    }
    check_table_cycles(&table)?;
    Ok(table)
}

/// 跨套件的循環偵測(P48):單一套件內的環由 `parse_functions` 擋掉,
/// 合併表可能出現跨套件的環。
fn check_table_cycles(table: &FunctionTable) -> Result<(), ReplayError> {
    let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, found) in &table.entries {
        let mut callees = Vec::new();
        for entry in found {
            match &entry.definition.body {
                FunctionBody::Sequence(calls) => {
                    callees.extend(calls.iter().map(|call| call.name.as_str()))
                }
                FunctionBody::Case(branches) | FunctionBody::When(branches) => {
                    callees.extend(branches.iter().map(|branch| branch.call.name.as_str()))
                }
            }
        }
        graph.insert(name.as_str(), callees);
    }
    let mut state: BTreeMap<&str, u8> = BTreeMap::new();
    for name in graph.keys() {
        if let Err(cycle) = walk_graph(name, &graph, &mut state) {
            return Err(ReplayError::Parse(format!(
                "function {cycle:?} takes part in a call cycle across packages"
            )));
        }
    }
    Ok(())
}

fn walk_graph<'a>(
    node: &'a str,
    graph: &BTreeMap<&'a str, Vec<&'a str>>,
    state: &mut BTreeMap<&'a str, u8>,
) -> Result<(), String> {
    match state.get(node) {
        Some(1) => return Err(node.to_owned()),
        Some(2) => return Ok(()),
        _ => {}
    }
    state.insert(node, 1);
    for next in graph.get(node).into_iter().flatten() {
        if graph.contains_key(next) {
            walk_graph(next, graph, state)?;
        }
    }
    state.insert(node, 2);
    Ok(())
}
