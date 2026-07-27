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

use crate::{parse_call_head, split_named_argument, ReplayError};
use conlang_language::{
    LibraryCatalog, LibraryExportKind, LibraryKind, LibraryPackage, LibrarySpec,
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
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0usize;
    // 略過空行與註解。
    while index < lines.len()
        && (lines[index].trim().is_empty() || lines[index].trim_start().starts_with('#'))
    {
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
        if text.is_empty() || text.starts_with('#') {
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

/// 載入核心:直接吃一組套件。與 `load_functions` 分開是為了可用合成套件測到
/// **export 過濾**與**export/定義不一致**這兩條路徑(catalog 只能由嵌入建構)。
pub fn functions_from_packages(packages: &[&LibraryPackage]) -> Result<FunctionTable, ReplayError> {
    let mut table = FunctionTable::default();
    for package in packages.iter().copied() {
        if package.functions.trim().is_empty() {
            continue;
        }
        let parsed = parse_functions(package.functions)?;
        // export 表是唯一穩定契約(P29):只有列在 exports 的 function 對外可見。
        let exported: BTreeSet<&str> = package
            .exports
            .iter()
            .filter(|export| export.kind == LibraryExportKind::Function)
            .map(|export| export.alias.as_str())
            .collect();
        for alias in &exported {
            if !parsed
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
        for definition in parsed.functions {
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
