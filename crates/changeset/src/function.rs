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
//! | 純序列 | 無條件依序全跑 | Recipe |
//! | `case:` | 第一個 Matched | 確定性分支 |
//! | `when:` | **所有 Matched 依序執行**(同 `.lang` 的 `CaseSelection::Accumulate`) | 有條件的全跑 |
//! | `choose:` | **列舉所有 Matched,一個都不執行** | Goal 的候選列舉 |

use crate::rewrite::DonorScope;
use crate::{
    apply_statement_structural, apply_statement_structural_with_packages, call, parse_call_head,
    split_named_argument, PrimitiveEdit, ReplayError, StatementRecord,
};
use conlang_language::{
    compile_document, compile_document_with_packages, sample_weighted_index, table_type,
    LanguageDocument, LibraryCatalog, LibraryExportKind, LibraryPackage, LibrarySpec, PackageLayer,
    ResolvedPackages, SignItem,
};
use std::collections::{BTreeMap, BTreeSet};

pub const FUNCTIONS_SCHEMA_V1: &str = "conlang.functions/v1";

#[derive(Clone, Copy)]
enum RuntimePackages<'a> {
    Legacy(&'a LibrarySpec),
    Resolved(&'a ResolvedPackages),
}

impl RuntimePackages<'_> {
    fn compile(
        self,
        document: &LanguageDocument,
    ) -> Result<conlang_language::CompiledSystem, ReplayError> {
        match self {
            Self::Legacy(spec) => Ok(compile_document(document, spec)?),
            Self::Resolved(packages) => Ok(compile_document_with_packages(document, packages)?),
        }
    }

    /// P52 路徑庫,取自**這次求值實際選中的那組套件**。
    ///
    /// 與 `WeightDb` 由呼叫端傳入不同:權重是**選擇層**的輸入(選擇不屬於
    /// 引擎層,P12/P70),路徑庫則是 recipe 求值途中要查的東西,和 function
    /// 表同源同一組套件。從這裡取,`.chg` 的求值就不會出現「函數載入用一組
    /// 套件、路徑表用另一組」的裂縫。
    fn path_db(self) -> Result<PathDb, ReplayError> {
        match self {
            Self::Legacy(spec) => {
                let catalog = LibraryCatalog::embedded()
                    .map_err(|error| ReplayError::Parse(error.to_string()))?;
                load_path_db(&catalog, spec)
            }
            Self::Resolved(packages) => load_path_db_from_resolved(packages),
        }
    }

    fn apply_statement(
        self,
        document: &LanguageDocument,
        edits: &[PrimitiveEdit],
    ) -> Result<(LanguageDocument, StatementRecord), ReplayError> {
        match self {
            Self::Legacy(spec) => apply_statement_structural(document, 0, edits, spec),
            Self::Resolved(packages) => {
                apply_statement_structural_with_packages(document, 0, edits, packages)
            }
        }
    }
}

/// rebase 對一個 function runtime 失敗的歸類(P57 的四桶去掉 `Clean`)。
///
/// 桶的定義來自 P57 本身:**衝突** = 這筆編輯在新 base 上套不上去;**環境變動** =
/// 檔案宣告的外部依賴與實際提供的對不上;**輸入錯** = 使用者寫的東西本身壞了。
/// 每個變體歸哪一桶寫在 [`FunctionError::class`],不散落到 `evolution.rs`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionErrorClass {
    Conflict,
    Environment,
    Broken,
}

/// Step 17 函數／Goal **runtime** 的分型錯誤(P57 鐵律:分類靠型別變體,絕不靠訊息字串)。
///
/// ## 為什麼非分型不可
///
/// `.chg` 的語句可以呼叫 function(`UnresolvedChangeSet::resolve` 的語句迴圈),
/// 而 rebase 依 `ReplayError` 的**變體**分類。這些失敗原本全部是 `ReplayError::Parse`,
/// 於是 `RebaseOutcome::classify` 一律判成 `Broken`「changeset 本身壞了」。
/// 具體反例:`.chg` 呼叫一個帶 `/ $sign.syn.category == VERB` guard 的 Recipe,
/// rebase 到「該 sign 已被 reanalyze 成 AUX」的新 base——這是**最典型的衝突**,
/// 卻會被告知去修自己的檔案,而且句號一併丟失。
///
/// ## 邊界:只收 runtime,不收載入期
///
/// 定義文件的解析錯、weight DB 的格式錯留在 `ReplayError::Parse`——它們在任何語句
/// 開始前就發生,永遠到不了 rebase,而且「檔案格式壞掉」確實就是 `Broken` 的本意。
/// 這裡只收**求值中**才可能發生的那些。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FunctionError {
    // ── 語言狀態面:新 base 上套不上去 ⇒ Conflict ──
    #[error("FUNCTION_GUARD_UNSATISFIED: function {function:?} guard {guard:?} does not hold")]
    GuardUnsatisfied { function: String, guard: String },
    #[error("FUNCTION_CASE_NO_BRANCH: function {function:?} has no matching branch and no else")]
    CaseNoBranch { function: String },
    #[error(
        "FUNCTION_GUARD_UNKNOWN_SIGN: {function:?} guard {guard:?} names unknown sign {sign:?}"
    )]
    GuardUnknownSign {
        function: String,
        guard: String,
        sign: String,
    },
    #[error("FUNCTION_GUARD_ERROR: {function:?} guard {guard:?}: {message}")]
    GuardEvaluation {
        function: String,
        guard: String,
        message: String,
    },
    #[error("FUNCTION_ARGUMENT_UNKNOWN_SIGN: {function:?} parameter {parameter:?} names unknown sign {sign:?}")]
    ConstraintUnknownSign {
        function: String,
        parameter: String,
        sign: String,
    },
    #[error("FUNCTION_ARGUMENT_CONSTRAINT: {function:?} parameter {parameter:?} requires [{constraint}], but sign {sign:?} does not belong to it")]
    ConstraintUnsatisfied {
        function: String,
        parameter: String,
        constraint: String,
        sign: String,
    },

    // ── 載入表面:套件／權重表換版就會變 ⇒ Environment ──
    #[error("FUNCTION_UNKNOWN: unknown function {name:?}")]
    Unknown { name: String },
    #[error("FUNCTION_NOT_VISIBLE: {name:?} is defined but not exported from its package")]
    NotVisible { name: String },
    #[error("FUNCTION_AMBIGUOUS: ambiguous function {name:?} at equal priority; qualify it as `package::{name}`")]
    Ambiguous { name: String },
    #[error("WEIGHT_DB_MISSING: Goal {goal:?} candidate Recipe {recipe:?} has no weight")]
    WeightMissing { goal: String, recipe: String },
    #[error("GOAL_SAMPLING: Goal {goal:?} cannot be sampled: {message}")]
    Sampling { goal: String, message: String },
    /// P52:候選已經成立(`path(...)` guard 過了),卻查不到 δ。這代表路徑表
    /// 在 guard 與取值之間變了,或兩者查了不同的表——**環境**問題,不是誰寫錯。
    /// 欄名是 `source_concept` 而不是 `source`:`thiserror` 把叫 `source` 的
    /// 欄位當成錯誤鏈的上游來源,而這裡的 source 是路徑表的來源概念欄。
    #[error("PATH_DB_MISSING: no grammaticalization path {source_concept:?} -> {target:?}")]
    PathMissing {
        source_concept: String,
        target: String,
    },

    // ── 呼叫端寫錯 ⇒ Broken ──
    #[error(
        "FUNCTION_CANDIDATES_REQUIRE_SELECTION: {function:?} yields candidates and cannot execute here"
    )]
    CandidatesRequireSelection { function: String },
    #[error("FUNCTION_CANDIDATE_LAYER: {function:?} yields candidate {candidate:?}, which is another candidate function")]
    CandidateLayer { function: String, candidate: String },
    #[error("FUNCTION_GUARD_NO_SUBJECT: {function:?} guard {guard:?} reads no bound parameter")]
    GuardNoSubject { function: String, guard: String },
    #[error("FUNCTION_GUARD_SUBJECT_NOT_A_SIGN: {function:?} guard {guard:?} needs sign(\"…\"), got {value:?}")]
    GuardSubjectNotASign {
        function: String,
        guard: String,
        value: String,
    },
    #[error("FUNCTION_ARGUMENT_NOT_A_SIGN: {function:?} parameter {parameter:?} [{constraint}] expects sign(\"…\"), got {value:?}")]
    ConstraintNotASign {
        function: String,
        parameter: String,
        constraint: String,
        value: String,
    },
    #[error("FUNCTION_ARGUMENT: function {function:?} {message}")]
    Argument { function: String, message: String },
    #[error("PATH_BUILTIN_ARITY: {function:?} {call:?} expects path(<sign>, <sense>, <target>)")]
    PathBuiltinArity { function: String, call: String },
    #[error("PATH_BUILTIN_SUBJECT: {function:?} {call:?} needs sign(\"…\"), got {value:?}")]
    PathBuiltinSubject {
        function: String,
        call: String,
        value: String,
    },
    #[error("PATH_BUILTIN_UNKNOWN_SIGN: {function:?} {call:?} references unknown sign {sign:?}")]
    PathBuiltinUnknownSign {
        function: String,
        call: String,
        sign: String,
    },
    #[error("PATH_BUILTIN_NO_SENSE: {function:?} {call:?}: sign {sign:?} has no sense {sense:?}")]
    PathBuiltinNoSense {
        function: String,
        call: String,
        sign: String,
        sense: String,
    },
    #[error("FUNCTION_UNEXPECTED_BLOCK: function {name:?} does not accept an indented block")]
    UnexpectedBlock { name: String },
}

impl FunctionError {
    /// 這個失敗對 rebase 而言屬於哪一桶。**窮盡 match**:日後新增變體時編譯器會
    /// 強迫在這裡表態,不會有變體悄悄落進預設桶。
    pub fn class(&self) -> FunctionErrorClass {
        match self {
            // guard／參數約束讀的是 Language 當前狀態,換了 base 答案就會變——
            // 這正是「編輯套不上新 base」,不是使用者寫錯。
            FunctionError::GuardUnsatisfied { .. }
            | FunctionError::CaseNoBranch { .. }
            | FunctionError::GuardUnknownSign { .. }
            | FunctionError::GuardEvaluation { .. }
            | FunctionError::ConstraintUnknownSign { .. }
            | FunctionError::ConstraintUnsatisfied { .. } => FunctionErrorClass::Conflict,
            // 名字解不開／權重查不到:function 與權重都住套件,換版就沒了。
            // 與 `LibraryLockMismatch` 同類——不該要求人去改 changeset。
            FunctionError::Unknown { .. }
            | FunctionError::NotVisible { .. }
            | FunctionError::Ambiguous { .. }
            | FunctionError::WeightMissing { .. }
            | FunctionError::PathMissing { .. }
            | FunctionError::Sampling { .. } => FunctionErrorClass::Environment,
            // 呼叫端自己寫的東西壞了:引數對不上簽名、guard 讀不到主體、
            // 把候選函數直接當語句用。這些換幾個 base 都一樣錯。
            FunctionError::CandidatesRequireSelection { .. }
            | FunctionError::CandidateLayer { .. }
            | FunctionError::GuardNoSubject { .. }
            | FunctionError::GuardSubjectNotASign { .. }
            | FunctionError::ConstraintNotASign { .. }
            | FunctionError::Argument { .. }
            | FunctionError::PathBuiltinArity { .. }
            | FunctionError::PathBuiltinSubject { .. }
            | FunctionError::PathBuiltinUnknownSign { .. }
            | FunctionError::PathBuiltinNoSense { .. }
            | FunctionError::UnexpectedBlock { .. } => FunctionErrorClass::Broken,
        }
    }
}

/// 一次呼叫(body 的一行)。與 `.chg` 執行區的呼叫同構(P47)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCall {
    pub name: String,
    pub positional: Option<String>,
    pub named: Vec<(String, String)>,
}

/// 一個分支的成立條件。三種,對齊 `.lang` 的 `CaseCondition`。
///
/// ## `Else` 與 `Always` 為什麼要分開
///
/// 先前兩者都記成 `guard: None` 並且**都恆成立**,於是 `else` 只是裝飾——parser 分得
/// 出 `else X` 與 `X`,語意卻塌成同一個。在 `case:`(取第一個)下兩種讀法結果相同,
/// 在 `when:`/`choose:`(全取)下**不同**:`.lang` 的 `else` 是 `!any_matched`
/// (`system.rs` 的 Accumulate 分支),只在前面都不成立時才生效。
///
/// `Always` 是 `.lang` 沒有的第三種,理由是**結構差異**而非語意分歧:`.lang` 的
/// `when:` 區塊住在更大的 sign body 裡,無條件的項目寫在區塊外就好;function 的
/// body **整個就是一個區塊**(`parse_body` 一旦看到 `when:` 就把其餘全部當分支),
/// 沒有「區塊外」可以放。裸呼叫因此是「這個區塊裡的無條件成員」的唯一寫法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchCondition {
    /// `<call> / <guard>` —— 依 guard 求值。
    Guard(String),
    /// `else <call>` —— **只在前面的分支都不成立時**生效(`.lang` 的
    /// `CaseCondition::Else`)。`case:` 下因為取第一個,走到它時必然沒人成立,
    /// 故兩種選擇模式的行為在該處自然一致。
    Else,
    /// 裸 `<call>` —— 恆成立,且**計入 `any_matched`**(它成立了,後面的 `else`
    /// 就不該再補位)。
    Always,
}

/// `case:`/`when:`/`choose:` 的一個分支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBranch {
    pub condition: BranchCondition,
    pub call: FunctionCall,
}

/// body 形狀就是執行語意(P48「不新增 layer 標記」)——寫的人不宣告自己是哪一層,
/// 故不可能宣告與行為不符。
///
/// ## 誌誤:`when:` 曾被借去表達候選列舉
///
/// `when:` 在 `.lang` 是 `CaseSelection::Accumulate`——**所有成立的分支都生效**
/// (`system.rs` 把每個成立分支的 fragment `merged_items.extend` 進結果)。
/// 早期 function 層借用同一個關鍵字來表達「列舉候選、一個都不執行」,兩者
/// **「取全部」相同、「然後呢」相反**:一個全都生效,一個全都不生效。
///
/// 《修補10》§2 的論證只證明了「取全部而非取第一個」,沒有證明「取全部之後不執行」
/// ——把 *which* 與 *then what* 混為一談。副作用是「**有條件的全跑**」整格空掉:
/// 純序列不能帶 guard、`case:` 只取一個、`when:` 被徵用,`.lang` 的 `when` 本義在
/// function 層反而寫不出來。
///
/// 現改回:`when:` 回歸 accumulate,候選列舉獨立為 `choose:`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBody {
    /// 純序列 = **無條件依序全跑**(慣稱 Recipe)。
    Sequence(Vec<FunctionCall>),
    /// `case:` = **第一個 Matched 的分支**。
    Case(Vec<FunctionBranch>),
    /// `when:` = **所有 Matched 依序執行**(`.lang` 的 `CaseSelection::Accumulate`)。
    /// 一個都不成立時**無操作、不報錯**——比照 `.lang` 的 `return Ok(current)`。
    When(Vec<FunctionBranch>),
    /// `choose:` = **列舉所有 Matched 的候選,一個都不執行**,交給呼叫端選
    /// (慣稱 Goal)。選擇者可以是使用者,也可以是抽樣器(P12:Weight DB 是
    /// **自動模式**的權重)——引擎不預設是哪一種。
    Choose(Vec<FunctionBranch>),
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
                FunctionError::WeightMissing {
                    goal: goal.to_owned(),
                    recipe: recipe.to_owned(),
                }
                .into()
            })
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone)]
struct PathEntry {
    delta: f64,
    package: String,
    priority: i32,
}

/// P52 語法化路徑庫:**來源概念 → 目標語意** 加預設 δ。
///
/// 「GO→未來、WANT→未來、COME→未來…」機制完全相同,只有兩端的概念不同,
/// 所以它們**不是 30–50 個 function**,而是一個參數化 function 加 30–50 行 data。
/// 官方要加一條路徑,加一行 `source<TAB>target<TAB>delta` 即可——不碰 `.chg`、
/// 不碰引擎。
///
/// 這張表**只在候選比對時被讀**(goal 的 `path(...)` guard 與
/// `path_delta(...)` 值),讀出來的 δ 隨即固化成候選呼叫裡的字面數字。
/// 因此寫進 `.chg` 的是數字不是查表指令,replay 不重讀這張表(P26 逐位元
/// 可重現),而表本身仍受 library lock 的第三道 digest 保護。
#[derive(Debug, Clone, Default)]
pub struct PathDb {
    entries: BTreeMap<(String, String), PathEntry>,
}

impl PathDb {
    /// 這條路徑的預設 δ。
    pub fn delta(&self, source: &str, target: &str) -> Result<f64, ReplayError> {
        self.entries
            .get(&(source.to_owned(), target.to_owned()))
            .map(|entry| entry.delta)
            .ok_or_else(|| {
                FunctionError::PathMissing {
                    source_concept: source.to_owned(),
                    target: target.to_owned(),
                }
                .into()
            })
    }

    /// 表上有沒有這條路徑。goal 的 `path(...)` guard 用它——**沒有不是錯**,
    /// 那只是「這個詞不是這個目標的已知來源」,候選不成立而已(P70 零候選合法)。
    pub fn contains(&self, source: &str, target: &str) -> bool {
        self.entries
            .contains_key(&(source.to_owned(), target.to_owned()))
    }

    /// 通往 `target` 的所有來源概念,鍵序固定。診斷訊息用。
    pub fn sources_for(&self, target: &str) -> Vec<&str> {
        self.entries
            .keys()
            .filter(|(_, entry_target)| entry_target == target)
            .map(|(source, _)| source.as_str())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
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
    if head == "case:" || head == "when:" || head == "choose:" {
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
            let condition = match (is_else, guard) {
                (true, _) => BranchCondition::Else,
                (false, Some(guard)) => BranchCondition::Guard(guard),
                (false, None) => BranchCondition::Always,
            };
            branches.push(FunctionBranch {
                condition,
                call: parse_call(call_text.trim(), owner)?,
            });
        }
        if branches.is_empty() {
            return Err(ReplayError::Parse(format!(
                "function {owner:?}: `{head}` has no branches"
            )));
        }
        return Ok(match head {
            "case:" => FunctionBody::Case(branches),
            "when:" => FunctionBody::When(branches),
            _ => FunctionBody::Choose(branches),
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
            FunctionBody::Case(branches)
            | FunctionBody::When(branches)
            | FunctionBody::Choose(branches) => {
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
    /// 是否列在套件的 export 表。**未 export 者仍進表**,但只有同套件內的呼叫看得到
    /// ——那就是「套件私有 function」。先前的作法是直接丟掉,導致套件內的呼叫圖
    /// 根本不存在(一個 Recipe 無法把步驟拆成同套件的小函式)。
    pub exported: bool,
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
    /// 外部(`.chg` 或宿主)看得到的解析:**只有 export 表上的**。
    ///
    /// export 表是唯一穩定契約(P29),故跨套件與外部呼叫都走這條。
    pub fn get(&self, name: &str) -> Result<&FunctionDef, ReplayError> {
        self.resolve(name, None).map(|entry| &entry.definition)
    }

    /// 帶**呼叫端套件**的解析:看得到 export 表上的,**外加該套件自己的私有 function**。
    ///
    /// 私有性是這樣達成的——不是「不進表」,而是「別的套件查不到」。前者會讓套件內
    /// 的呼叫圖整個不存在。
    pub fn resolve(
        &self,
        name: &str,
        from_package: Option<&str>,
    ) -> Result<&ResolvedFunction, ReplayError> {
        let visible = |entry: &&ResolvedFunction| {
            entry.exported || from_package.is_some_and(|package| package == entry.package)
        };
        if let Some((package, symbol)) = name.split_once("::") {
            return self
                .entries
                .get(symbol)
                .and_then(|found| {
                    found
                        .iter()
                        .filter(visible)
                        .find(|entry| entry.package == package)
                })
                .ok_or_else(|| {
                    FunctionError::Unknown {
                        name: name.to_owned(),
                    }
                    .into()
                });
        }
        let found: Vec<&ResolvedFunction> = self
            .entries
            .get(name)
            .map(|found| found.iter().filter(visible).collect())
            .unwrap_or_default();
        // **同套件的私有定義優先於外來的同名 export**:呼叫端寫的是自己套件裡的名字,
        // 被別的套件的同名 export 搶走會是最難查的一種錯。
        if let Some(package) = from_package {
            if let Some(own) = found.iter().find(|entry| entry.package == package) {
                return Ok(own);
            }
        }
        let top = *found
            .iter()
            .max_by_key(|entry| entry.priority)
            .ok_or_else(|| FunctionError::Unknown {
                name: name.to_owned(),
            })?;
        // 同名同 priority → 強制消歧(P29「warn + 強制消歧」的嚴格面:
        // 呼叫時才不得含糊)。
        if found
            .iter()
            .filter(|entry| entry.priority == top.priority)
            .count()
            > 1
        {
            return Err(FunctionError::Ambiguous {
                name: name.to_owned(),
            }
            .into());
        }
        Ok(top)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 表裡**有沒有這個名字**(不論可見性)。用來把「看不到」與「根本不是 function」
    /// 分開報 —— 兩者的訊息差很多,混在一起會把使用者導向錯的方向。
    pub(crate) fn defines(&self, name: &str) -> bool {
        let symbol = name.split_once("::").map_or(name, |(_, symbol)| symbol);
        self.entries.contains_key(symbol)
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
    libraries: RuntimePackages<'_>,
) -> Result<BTreeMap<String, String>, ReplayError> {
    let mut bindings = BTreeMap::new();
    if let Some(value) = &invocation.positional {
        let parameter = definition
            .params
            .first()
            .ok_or_else(|| FunctionError::Argument {
                function: definition.name.clone(),
                message: "does not take a positional argument".to_owned(),
            })?;
        bindings.insert(parameter.name.clone(), value.clone());
    }
    for (name, value) in &invocation.named {
        if !definition
            .params
            .iter()
            .any(|parameter| &parameter.name == name)
        {
            return Err(FunctionError::Argument {
                function: definition.name.clone(),
                message: format!("has no parameter {name:?}"),
            }
            .into());
        }
        if bindings.insert(name.clone(), value.clone()).is_some() {
            return Err(FunctionError::Argument {
                function: definition.name.clone(),
                message: format!("receives parameter {name:?} more than once"),
            }
            .into());
        }
    }
    for parameter in &definition.params {
        let value = bindings
            .get(&parameter.name)
            .ok_or_else(|| FunctionError::Argument {
                function: definition.name.clone(),
                message: format!("is missing parameter {:?}", parameter.name),
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
    libraries: RuntimePackages<'_>,
) -> Result<(), ReplayError> {
    let sign = sign_argument(value).ok_or_else(|| FunctionError::ConstraintNotASign {
        function: function.to_owned(),
        parameter: parameter.to_owned(),
        constraint: constraint.to_owned(),
        value: value.to_owned(),
    })?;
    let definition = document
        .language()
        .signs
        .iter()
        .find(|candidate| candidate.name == sign)
        .ok_or_else(|| FunctionError::ConstraintUnknownSign {
            function: function.to_owned(),
            parameter: parameter.to_owned(),
            sign: sign.to_owned(),
        })?;
    let system = libraries.compile(document)?;
    if !definition.items.iter().any(|item| {
        matches!(
            item,
            SignItem::TraitMount { name: name, kind: conlang_language::TraitMountKind::Declaration, .. }
                if name == constraint || system.ontology.category_is_a(name, constraint)
        )
    }) {
        return Err(FunctionError::ConstraintUnsatisfied {
            function: function.to_owned(),
            parameter: parameter.to_owned(),
            constraint: constraint.to_owned(),
            sign: sign.to_owned(),
        }
        .into());
    }
    Ok(())
}

/// `sign("x")` → `x`。參數約束與 guard 都要認這個形狀,**共用一個解析**
/// ——兩份會走鐘,而走鐘的後果是「約束擋得住的、guard 擋不住」。
fn sign_argument(value: &str) -> Option<&str> {
    value
        .trim()
        .strip_prefix("sign(")
        .and_then(|rest| rest.strip_suffix(')'))
        .map(str::trim)
        .map(|name| name.trim_matches('"'))
}

/// guard 裡出現在**路徑頭**的已綁定參數名(`verb.syn.category` 的 `verb`)。
///
/// 以識別字為單位掃描而非用 `contains`:參數叫 `verb` 而值裡剛好有 `verb.` 時,
/// 字串比對會誤判成「這個 guard 讀的是該參數」。
/// `<param> == [Trait]` —— **範疇成員形式**的主體。
///
/// 對一個 function guard 求值(P49 `function Name(…) / guard:`;
/// 分支的 `<call> / guard` 同一套)。
///
/// ## 一套 guard 語言,不是兩套
///
/// 形狀沿用 `.lang` 規則的 `/ guard`(修補10 §3 的例:`/ verb.syn.category == verb`),
/// 差別只在**主體是參數名而非 `$self`**。故求值方式是:把參數名代換成 `$self`,
/// 再交給 `.lang` 既有的求值器(`synchronic::guard_matches_sign`)。
///
/// 自己在這裡另寫一個述詞求值器會製造第二套語意——而那個走鐘是**無聲的**:
/// 同一句 guard 共時求值是一個意思、歷時求值是另一個意思。
///
/// ## 在**實際 base** 上求值
///
/// 承修補10 §11.2:「guard 到 invoke 時才在實際 base 上求值,定義檔即完全
/// base-independent」。故這裡吃的是當下的 `document`,不是定義時的任何東西。
///
/// ## 讀的是 effective sign
///
/// 與參數約束一致——約束走 `belongs` 閉包(`category_is_a`),guard 也該看得到繼承
/// 下來的內容,否則「約束過得了、guard 過不了」會變成兩套可見範圍。
/// `name(a, b, c)` → `["a", "b", "c"]`。不是這個內建就回 `None`。
///
/// 刻意只認**平坦的一層**:內建不是運算式語言的開端,它是兩個查表動作。
/// 巢狀呼叫會在這裡被當成一個引數,`resolve_path_arguments` 隨即因為主體
/// 不是 `sign("…")` 而報錯——不會靜默算出別的東西。
fn parse_path_builtin<'a>(value: &'a str, name: &str) -> Option<Vec<&'a str>> {
    let rest = value.trim().strip_prefix(name)?.trim_start();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner.split(',').map(str::trim).collect())
}

/// 一個 sign 的某個義項現在的 gloss = 這條語法化路徑的**來源概念**。
///
/// 讀的是 effective sign(含 trait 繼承),與 guard 求值同一份口徑。
fn sense_gloss(
    owner: &str,
    call: &str,
    sign_name: &str,
    sense: &str,
    document: &LanguageDocument,
    libraries: RuntimePackages<'_>,
) -> Result<String, ReplayError> {
    let sign = document
        .language()
        .signs
        .iter()
        .find(|candidate| candidate.name == sign_name)
        .ok_or_else(|| FunctionError::PathBuiltinUnknownSign {
            function: owner.to_owned(),
            call: call.to_owned(),
            sign: sign_name.to_owned(),
        })?;
    let system = libraries.compile(document)?;
    let effective = system.ontology.effective_sign(sign);
    effective
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::Sense(value) if value.name == sense => Some(value.gloss.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            FunctionError::PathBuiltinNoSense {
                function: owner.to_owned(),
                call: call.to_owned(),
                sign: sign_name.to_owned(),
                sense: sense.to_owned(),
            }
            .into()
        })
}

/// 內建的共同前半:把 `f(<sign>, <sense>, <target>)` 的三個引數解出來,
/// 並把主體換成它的來源概念。回傳 `(source, target)`。
fn path_builtin_key(
    owner: &str,
    call: &str,
    args: &[&str],
    bindings: &BTreeMap<String, String>,
    document: &LanguageDocument,
    libraries: RuntimePackages<'_>,
) -> Result<(String, String), ReplayError> {
    if args.len() != 3 || args.iter().any(|argument| argument.is_empty()) {
        return Err(FunctionError::PathBuiltinArity {
            function: owner.to_owned(),
            call: call.to_owned(),
        }
        .into());
    }
    let subject = substitute(args[0], bindings);
    let sign_name = sign_argument(&subject).ok_or_else(|| FunctionError::PathBuiltinSubject {
        function: owner.to_owned(),
        call: call.to_owned(),
        value: subject.clone(),
    })?;
    let sense = substitute(args[1], bindings);
    let target = substitute(args[2], bindings);
    let source = sense_gloss(owner, call, sign_name, &sense, document, libraries)?;
    Ok((source, target))
}

fn guard_holds(
    owner: &str,
    guard: &str,
    bindings: &BTreeMap<String, String>,
    document: &LanguageDocument,
    libraries: RuntimePackages<'_>,
    paths: &PathDb,
) -> Result<bool, ReplayError> {
    // ── P52 內建:`path(<sign>, <sense>, <target>)` ────────────────────
    //
    // 「這個詞是不是那個目標語意的**已知來源**」。答案住 `data/paths.tsv`,
    // 所以官方加一條路徑(GO→未來、WANT→未來…)是加一行 data,而不是在
    // 這裡多寫一個 `choose:` 分支、更不是多寫一個 function。
    //
    // **不在表上不是錯**,候選不成立而已——對一個沒有 GO 類動詞的語言跑
    // 「動詞→未來」就該得到零候選(P70:零候選是合法結果)。
    if let Some(args) = parse_path_builtin(guard, "path") {
        let (source, target) =
            path_builtin_key(owner, guard, &args, bindings, document, libraries)?;
        return Ok(paths.contains(&source, &target));
    }
    // P81:主體以 `$<參數名>` 顯式書寫,由**環境**解析,不再掃描識別字再
    // 文字代換成 `$self`。跨參數的 guard 因此可表達——舊路徑只有一個隱含
    // 主體,故必須以 `FUNCTION_GUARD_MULTI_SUBJECT` 拒絕。
    let roles = conlang_language::synchronic::guard_binding_roles(guard);
    if roles.subjects.is_empty() && roles.scalars.is_empty() {
        return Err(FunctionError::GuardNoSubject {
            function: owner.to_owned(),
            guard: guard.to_owned(),
        }
        .into());
    }

    let unbound = || -> ReplayError {
        FunctionError::GuardNoSubject {
            function: owner.to_owned(),
            guard: guard.to_owned(),
        }
        .into()
    };

    let system = libraries.compile(document)?;
    let mut effective = BTreeMap::new();
    for name in &roles.subjects {
        let value = bindings.get(name).ok_or_else(unbound)?;
        // 主體必須綁到 sign——`$x.syn.f` 的 `x` 是「哪個 sign」。
        let sign_name =
            sign_argument(value).ok_or_else(|| FunctionError::GuardSubjectNotASign {
                function: owner.to_owned(),
                guard: guard.to_owned(),
                value: value.to_owned(),
            })?;
        let sign = document
            .language()
            .signs
            .iter()
            .find(|candidate| candidate.name == sign_name)
            .ok_or_else(|| FunctionError::GuardUnknownSign {
                function: owner.to_owned(),
                guard: guard.to_owned(),
                sign: sign_name.to_owned(),
            })?;
        effective.insert(name.clone(), system.ontology.effective_sign(sign));
    }
    let env = effective
        .iter()
        .map(|(name, sign)| (name.clone(), sign))
        .collect();
    // 純量參數:右端的 `$y`,綁的是字面值而非 sign。
    let mut scalars = BTreeMap::new();
    for name in &roles.scalars {
        scalars.insert(
            name.clone(),
            bindings.get(name).ok_or_else(unbound)?.clone(),
        );
    }

    conlang_language::synchronic::guard_matches_bindings(guard, &env, &scalars, &system.ontology)
        .map_err(|error| {
            FunctionError::GuardEvaluation {
                function: owner.to_owned(),
                guard: guard.to_owned(),
                message: error,
            }
            .into()
        })
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
    libraries: RuntimePackages<'a>,
    /// P52 路徑庫。**唯讀**,且只在候選比對階段被查(`path` guard 與
    /// `path_delta` 值),不在執行階段。
    paths: PathDb,
    document: LanguageDocument,
    edits: Vec<PrimitiveEdit>,
    trace: Vec<FunctionTraceStep>,
    stack: Vec<String>,
    /// 當下正在求值的 function 屬於哪個套件——決定它看不看得到私有的同套件 function。
    package: Option<String>,
}

impl EvaluationState<'_> {
    /// 綁定參數,並就地解掉 P52 的 `path_delta(...)` 值。
    ///
    /// ## 為什麼在這裡解、解成字面數字
    ///
    /// 這條路徑的 δ 必須在**來源概念還在**的時候查。recipe 的第一步
    /// `drift(verb, sense: core, gloss: tense)` 會把 core 的 gloss 從 `GO`
    /// 改寫成 `FUTURE`;P51 的接力展開讓後續每一步都讀得到前一步的結果,
    /// 所以 recipe 走到 `entrench` 那行時,表上的來源鍵**已經不存在了**。
    /// 在候選比對階段(凍結文件、drift 之前)解掉,問題不會發生。
    ///
    /// 解出來的是字面數字,於是應用層寫進 `.chg` 的候選呼叫是
    /// `VerbToTense(sign("go"), …, delta: 0.3)`——replay 不重查這張表
    /// (P26 逐位元可重現),表本身仍由 library lock 的第三道 digest 看著。
    fn bound_call(
        &self,
        owner: &str,
        call: &FunctionCall,
        bindings: &BTreeMap<String, String>,
    ) -> Result<FunctionCall, ReplayError> {
        let mut bound = bind_call(call, bindings);
        for value in bound
            .positional
            .iter_mut()
            .chain(bound.named.iter_mut().map(|(_, value)| value))
        {
            let Some(args) = parse_path_builtin(value, "path_delta") else {
                continue;
            };
            let (source, target) = path_builtin_key(
                owner,
                value,
                &args,
                bindings,
                &self.document,
                self.libraries,
            )?;
            *value = self.paths.delta(&source, &target)?.to_string();
        }
        Ok(bound)
    }

    fn evaluate(
        &mut self,
        invocation: &FunctionCall,
    ) -> Result<Option<FunctionCandidates>, ReplayError> {
        let resolved = self
            .table
            .resolve(&invocation.name, self.package.as_deref())?
            .clone();
        let definition = resolved.definition;
        let bindings = bind_parameters(&definition, invocation, &self.document, self.libraries)?;
        // header guard **不成立即錯**,不是靜默不做事。
        //
        // 理由是與姊妹機制一致:修補10 §3 明說「參數約束**取代大部分 guard**」,兩者
        // 是同一個用途的兩種寫法;而參數約束不符時是 `Err`。若 guard 改成「不符就當
        // 沒呼叫過」,同一個條件寫成約束會擋、寫成 guard 會靜默略過。
        if let Some(guard) = &definition.guard {
            if !guard_holds(
                &definition.name,
                guard,
                &bindings,
                &self.document,
                self.libraries,
                &self.paths,
            )? {
                return Err(FunctionError::GuardUnsatisfied {
                    function: definition.name.clone(),
                    guard: guard.clone(),
                }
                .into());
            }
        }
        self.stack.push(definition.name.clone());
        let outer_package = self.package.replace(resolved.package.clone());
        let result = match &definition.body {
            FunctionBody::Sequence(calls) => {
                for call in calls {
                    let call = self.bound_call(&definition.name, call, &bindings)?;
                    self.evaluate_call(&call)?;
                }
                None
            }
            // `case:` = **第一個成立的分支**(first-match)。`guard == None` 是 `else`,
            // 恆成立,故它一旦出現就終結搜尋——與 `.lang` 的 `case` 同語意。
            FunctionBody::Case(branches) => {
                let mut selected = None;
                for branch in branches {
                    // 取第一個成立者,故走到 `else` 時必然前面都沒成立
                    // ——`!any_matched` 恆為真,與 `.lang` 的 `Else => Ok(true)` 一致。
                    let matched = match &branch.condition {
                        BranchCondition::Else | BranchCondition::Always => true,
                        BranchCondition::Guard(guard) => guard_holds(
                            &definition.name,
                            guard,
                            &bindings,
                            &self.document,
                            self.libraries,
                            &self.paths,
                        )?,
                    };
                    if matched {
                        selected = Some(branch);
                        break;
                    }
                }
                // 全部不成立**即錯**,不是安靜跳過:`case:` 的契約是「選一個」,
                // 選不出來就是作者沒寫兜底,該讓他知道。
                let branch = selected.ok_or_else(|| FunctionError::CaseNoBranch {
                    function: definition.name.clone(),
                })?;
                let call = self.bound_call(&definition.name, &branch.call, &bindings)?;
                self.evaluate_call(&call)?;
                None
            }
            // `when:` = **所有成立的分支依序執行**(`.lang` 的 `CaseSelection::Accumulate`)。
            // 與 `case:` 的差別只在「取第一個」vs「全取」,兩者都**執行**。
            //
            // 一個都不成立時**無操作、不報錯**——比照 `.lang` 的 `!any_matched =>
            // return Ok(current)`。這與 `case:` 的 `CaseNoBranch` 刻意不同:`case:` 的
            // 契約是「選一個」,選不出來是作者漏了兜底;`when:` 的契約是「成立的都做」,
            // 一個都不成立就是「這次沒事要做」,那是合法結果。
            FunctionBody::When(branches) => {
                // **兩階段**:先在凍結的文件上把所有 guard 求完,再依序執行命中的分支
                // (規格《case、when 與 Context Fragment V2》§`when:` 第 2 條)。
                let hits = self.match_branches(&definition.name, branches, &bindings)?;
                for (branch, hit) in branches.iter().zip(hits) {
                    if hit {
                        let call = self.bound_call(&definition.name, &branch.call, &bindings)?;
                        self.evaluate_call(&call)?;
                    }
                }
                None
            }
            // `choose:` = **列舉所有成立的候選,一個都不執行**。guard 不成立的候選
            // 被排除,這就是 Goal 的候選篩選。
            FunctionBody::Choose(branches) => {
                let hits = self.match_branches(&definition.name, branches, &bindings)?;
                let candidates = branches
                    .iter()
                    .zip(hits)
                    .filter(|(_, hit)| *hit)
                    .map(|(branch, _)| self.bound_call(&definition.name, &branch.call, &bindings))
                    .collect::<Result<Vec<_>, ReplayError>>()?;
                for candidate in &candidates {
                    let target = &self
                        .table
                        .resolve(&candidate.name, Some(&resolved.package))?
                        .definition;
                    if matches!(target.body, FunctionBody::Choose(_)) {
                        return Err(FunctionError::CandidateLayer {
                            function: definition.name.clone(),
                            candidate: candidate.name.clone(),
                        }
                        .into());
                    }
                }
                Some(FunctionCandidates {
                    source: definition.name.clone(),
                    candidates,
                })
            }
        };
        self.stack.pop();
        self.package = outer_package;
        Ok(result)
    }

    /// `when:`/`choose:` 的**比對階段**:一次算完哪些分支成立,回傳與 `branches`
    /// 等長的命中表。
    ///
    /// ## 為什麼比對必須與執行分開(frozen matching)
    ///
    /// 《`case`、`when` 與 Context Fragment(V2)》§`when:` 第 2 條:
    ///
    /// > 所有非 `else` guard 都**只讀同一份 snapshot**;先前命中的 fragment 對後續
    /// > guard 不可見。
    ///
    /// 本函數取 `&self`,`self.document` 在整個迴圈中不會變,**凍結由型別保證**。
    /// 先前的寫法是邊比對邊執行,於是後面的 guard 讀得到前面分支的結果——實測:
    /// 「是 verb 就 reanalyze 成 aux」+「是 aux 就 entrench」會**兩條都跑**,
    /// entrenchment 0.2 → 0.7,而規格要的是 0.2。
    ///
    /// 這與 P51「Recipe 逐步接力展開」不衝突:接力是**執行**階段的性質(第 n 步讀
    /// 第 n−1 步的結果),比對是另一個階段。分開之後兩者都成立,並順帶得到規格
    /// §`when:` 第 3 條的原子性——guard 出錯時還沒有任何分支被執行。
    ///
    /// `else` 的判定與 `.lang` 的迴圈同構(`system.rs` 的 `Else => !any_matched`,
    /// 在迴圈內累計),故位置相依。實務上 `else` 一律寫在最後,兩種讀法在該處一致。
    ///
    /// `case:` 不走這裡:它取第一個成立者,走到 `else` 時 `any_matched` 必然為 false。
    fn match_branches(
        &self,
        owner: &str,
        branches: &[FunctionBranch],
        bindings: &BTreeMap<String, String>,
    ) -> Result<Vec<bool>, ReplayError> {
        let mut hits = Vec::with_capacity(branches.len());
        let mut any_matched = false;
        for branch in branches {
            let hit = match &branch.condition {
                BranchCondition::Always => true,
                BranchCondition::Else => !any_matched,
                BranchCondition::Guard(guard) => guard_holds(
                    owner,
                    guard,
                    bindings,
                    &self.document,
                    self.libraries,
                    &self.paths,
                )?,
            };
            any_matched |= hit;
            hits.push(hit);
        }
        Ok(hits)
    }

    fn evaluate_call(&mut self, invocation: &FunctionCall) -> Result<(), ReplayError> {
        if let Ok(entry) = self
            .table
            .resolve(&invocation.name, self.package.as_deref())
        {
            // 可見 → 當成 function 呼叫。
            //
            // **靜態形狀先看,不先求值**:`when:` 出現在序列裡是**寫錯**,與語言狀態
            // 無關。先求值的話,body 裡任何一個因狀態而起的錯(guard 不成立、參數
            // 約束不符)會先冒出來,把真正的問題蓋掉——而那些錯分類為 `Conflict`,
            // 於是 rebase 會叫人去解一個「換什麼 base 都解不掉」的衝突。
            if matches!(entry.definition.body, FunctionBody::Choose(_)) {
                return Err(FunctionError::CandidatesRequireSelection {
                    function: invocation.name.clone(),
                }
                .into());
            }
            if self.evaluate(invocation)?.is_some() {
                unreachable!("`choose:` 已在上面攔下");
            }
            return Ok(());
        }
        // 名字存在但這裡看不到 ⇒ 它是別的套件的私有 function。若讓它掉到下面的
        // rewrite 分派,錯誤會變成「unknown rewrite:12 個原子改寫是封閉內建集」
        // ——那把使用者導向完全錯的方向。
        if self.table.defines(&invocation.name) {
            return Err(FunctionError::NotVisible {
                name: invocation.name.clone(),
            }
            .into());
        }
        let call = call::Call {
            name: &invocation.name,
            positional: invocation.positional.as_deref(),
            named: &invocation.named,
            block: None,
        };
        let edits = call::lower(&call, &self.document, &DonorScope::new())?;
        let (document, _) = self.libraries.apply_statement(&self.document, &edits)?;
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
/// primitive trace; `choose:` stops at an ordered candidate list.
pub fn evaluate_function_offline(
    table: &FunctionTable,
    invocation: &FunctionCall,
    document: &LanguageDocument,
    libraries: &LibrarySpec,
) -> Result<FunctionEvaluation, ReplayError> {
    evaluate_function_runtime(
        table,
        invocation,
        document,
        RuntimePackages::Legacy(libraries),
    )
}

pub fn evaluate_function_with_packages(
    table: &FunctionTable,
    invocation: &FunctionCall,
    document: &LanguageDocument,
    packages: &ResolvedPackages,
) -> Result<FunctionEvaluation, ReplayError> {
    evaluate_function_runtime(
        table,
        invocation,
        document,
        RuntimePackages::Resolved(packages),
    )
}

fn evaluate_function_runtime(
    table: &FunctionTable,
    invocation: &FunctionCall,
    document: &LanguageDocument,
    libraries: RuntimePackages<'_>,
) -> Result<FunctionEvaluation, ReplayError> {
    let mut state = EvaluationState {
        table,
        libraries,
        paths: libraries.path_db()?,
        document: document.clone(),
        edits: Vec::new(),
        trace: Vec::new(),
        stack: Vec::new(),
        package: None,
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

/// 從候選清單抽一個。**選擇不屬於引擎層**——P12 把 Goal 的型別定到
/// `Vec<Recipe 候選>` 為止,抽樣器是消費那份清單的下游;本函數是應用層(候選面板 /
/// 批次迴圈)呼叫的工具,不是 `.chg` 的語法。產物是**具名的 Recipe 呼叫**,由呼叫端
/// 寫進 `.chg`,故 `.chg` 本文永遠不含隨機操作(P26 逐位元可重現)。
///
/// ## 零候選回 `Ok(None)`,不是錯誤
///
/// `choose:` 的所有 guard 都不成立時,候選清單是空的——那代表**這個語言目前沒有任何
/// 適用的演化路徑**(例如對一個沒有動詞的語言跑「動詞語法化」)。那是語言狀態的
/// 事實,不是失敗。
///
/// 先前它會落到 `sample_weighted_index` 的 `Empty`,被包成 `FunctionError::Sampling`
/// ——而 `Sampling` 分類為 `Environment`「套件/權重表換版了」,方向完全錯。改成
/// `Option` 而不是換一個錯誤變體,是因為呼叫端**必須**分辨這兩種結局;回 `Ok(None)`
/// 讓編譯器強迫它表態,回錯誤則可以被一句 `?` 靜默轉手。
///
/// 擋掉空清單之後,`Sampling` 只承載 `InvalidWeight`/`AllZero` 兩種**權重資料**問題,
/// `Environment` 才名副其實。
pub fn select_goal_candidate(
    candidates: &FunctionCandidates,
    weights: &WeightDb,
    seed: u64,
) -> Result<Option<GoalSelectionTrace>, ReplayError> {
    if candidates.candidates.is_empty() {
        return Ok(None);
    }
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
        ReplayError::from(FunctionError::Sampling {
            goal: candidates.source.clone(),
            message: error.to_string(),
        })
    })?;
    let selected = ordered
        .get(sample.selected_index)
        .expect("sampler returns an in-range index")
        .0
        .clone();
    Ok(Some(GoalSelectionTrace {
        algorithm: sample.algorithm,
        seed,
        source: candidates.source.clone(),
        ordered,
        selected_index: sample.selected_index,
        selected,
    }))
}

/// priority 四層(P29):未啟用不參與 < std < 已啟用 plugin < 專案本地。
fn package_priority(package: &LibraryPackage) -> i32 {
    let base = match package.layer {
        PackageLayer::Reference | PackageLayer::Data => 0,
        PackageLayer::Overlay => 1_000,
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

pub fn load_functions_from_resolved(
    packages: &ResolvedPackages,
) -> Result<FunctionTable, ReplayError> {
    functions_from_packages(&packages.packages().iter().collect::<Vec<_>>())
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

pub fn load_weight_db_from_resolved(packages: &ResolvedPackages) -> Result<WeightDb, ReplayError> {
    weight_db_from_packages(&packages.packages().iter().collect::<Vec<_>>())
}

pub fn load_path_db(catalog: &LibraryCatalog, spec: &LibrarySpec) -> Result<PathDb, ReplayError> {
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
    path_db_from_packages(&chosen)
}

pub fn load_path_db_from_resolved(packages: &ResolvedPackages) -> Result<PathDb, ReplayError> {
    path_db_from_packages(&packages.packages().iter().collect::<Vec<_>>())
}

/// P52 路徑庫載入。與 [`weight_db_from_packages`] 同形:認表型不認路徑(P29),
/// 同一條路徑被多個套件宣告時走 priority 四層,同優先級撞號**強制消歧**。
pub fn path_db_from_packages(packages: &[&LibraryPackage]) -> Result<PathDb, ReplayError> {
    let mut database = PathDb::default();
    for package in packages.iter().copied() {
        let priority = package_priority(package);
        for source in package.tables(table_type::GRAMMATICALIZATION_PATH_TABLE) {
            parse_path_source(
                &mut database,
                package,
                priority,
                &source.path,
                &source.source,
            )?;
        }
    }
    Ok(database)
}

fn parse_path_source(
    database: &mut PathDb,
    package: &LibraryPackage,
    priority: i32,
    path: &str,
    source: &str,
) -> Result<(), ReplayError> {
    let mut lines = source.lines().enumerate();
    let Some((_, header)) = lines.find(|(_, line)| !line.trim().is_empty()) else {
        return Err(ReplayError::Parse(format!(
            "PATH_DB_SCHEMA: {}:{path} is empty",
            package.id
        )));
    };
    if header.trim_end() != "source\ttarget\tdelta" {
        return Err(ReplayError::Parse(format!(
            "PATH_DB_SCHEMA: {}:{path} expects header source\\ttarget\\tdelta",
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
                "PATH_DB_SCHEMA: {}:{path}:{} expects source, target, delta",
                package.id,
                line_index + 1
            )));
        }
        let delta = columns[2].parse::<f64>().map_err(|_| {
            ReplayError::Parse(format!(
                "PATH_DB_DELTA: {}:{path}:{} has invalid delta {:?}",
                package.id,
                line_index + 1,
                columns[2]
            ))
        })?;
        if !delta.is_finite() || delta < 0.0 {
            return Err(ReplayError::Parse(format!(
                "PATH_DB_DELTA: {}:{path}:{} delta must be finite and non-negative, got {delta}",
                package.id,
                line_index + 1
            )));
        }
        let key = (columns[0].to_owned(), columns[1].to_owned());
        if !local.insert(key.clone()) {
            return Err(ReplayError::Parse(format!(
                "PATH_DB_DUPLICATE: {}:{path}:{} repeats path {:?} -> {:?}",
                package.id,
                line_index + 1,
                key.0,
                key.1
            )));
        }
        if let Some(existing) = database.entries.get(&key) {
            if existing.priority == priority {
                return Err(ReplayError::Parse(format!(
                    "PATH_DB_AMBIGUOUS: path {:?} -> {:?} is defined by {:?} and {} at equal priority",
                    key.0, key.1, existing.package, package.id
                )));
            }
            if existing.priority > priority {
                continue;
            }
        }
        database.entries.insert(
            key,
            PathEntry {
                delta,
                package: package.id.to_string(),
                priority,
            },
        );
    }
    Ok(())
}

pub fn weight_db_from_packages(packages: &[&LibraryPackage]) -> Result<WeightDb, ReplayError> {
    let mut database = WeightDb::default();
    for package in packages.iter().copied() {
        let priority = package_priority(package);
        // 表型宣告(`config/tables.tsv`)是唯一入口。此處**曾經**是
        // `path.ends_with("/weights.tsv")`——那讓套件內部檔名變成跨套件契約,
        // 正是 P29 禁止的;套件既不能換路徑,也無從宣告新表型。
        for source in package.tables(table_type::WEIGHT_TABLE) {
            parse_weight_source(
                &mut database,
                package,
                priority,
                &source.path,
                &source.source,
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
            let parsed = parse_functions(&package.functions)?;
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
                let parsed = parse_functions(&source.source).map_err(|error| {
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
            let is_exported = exported.contains(definition.name.as_str());
            table
                .entries
                .entry(definition.name.clone())
                .or_default()
                .push(ResolvedFunction {
                    package: package.id.to_string(),
                    priority: package_priority(package),
                    exported: is_exported,
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct FunctionKey<'a> {
        package: &'a str,
        symbol: &'a str,
    }

    impl std::fmt::Display for FunctionKey<'_> {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "{}::{}", self.package, self.symbol)
        }
    }

    fn walk_graph<'a>(
        node: FunctionKey<'a>,
        graph: &BTreeMap<FunctionKey<'a>, Vec<FunctionKey<'a>>>,
        state: &mut BTreeMap<FunctionKey<'a>, u8>,
        stack: &mut Vec<FunctionKey<'a>>,
    ) -> Result<(), Vec<FunctionKey<'a>>> {
        match state.get(&node) {
            Some(1) => {
                let start = stack
                    .iter()
                    .position(|candidate| *candidate == node)
                    .unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(node);
                return Err(cycle);
            }
            Some(2) => return Ok(()),
            _ => {}
        }
        state.insert(node, 1);
        stack.push(node);
        for &next in graph.get(&node).into_iter().flatten() {
            if graph.contains_key(&next) {
                walk_graph(next, graph, state, stack)?;
            }
        }
        stack.pop();
        state.insert(node, 2);
        Ok(())
    }

    // 節點 identity 必須包含套件。只用裸名會把兩個套件各自的 private `Helper`
    // 合成同一節點，憑空製造不存在的回邊。
    let mut graph: BTreeMap<FunctionKey<'_>, Vec<FunctionKey<'_>>> = BTreeMap::new();
    for found in table.entries.values() {
        for entry in found {
            let owner = FunctionKey {
                package: entry.package.as_str(),
                symbol: entry.definition.name.as_str(),
            };
            let calls: Vec<&FunctionCall> = match &entry.definition.body {
                FunctionBody::Sequence(calls) => calls.iter().collect(),
                FunctionBody::Case(branches)
                | FunctionBody::When(branches)
                | FunctionBody::Choose(branches) => {
                    branches.iter().map(|branch| &branch.call).collect()
                }
            };
            let callees = calls
                .into_iter()
                // 與執行期共用同一套 export/private/priority/qualified-name 解析語意。
                // 無法解析或有歧義的呼叫本來就不能執行，因此不構成可執行的循環邊；
                // 它們仍在真正 invoke 時沿用既有的 Unknown/NotVisible/Ambiguous 錯誤。
                .filter_map(|call| table.resolve(&call.name, Some(entry.package.as_str())).ok())
                .map(|resolved| FunctionKey {
                    package: resolved.package.as_str(),
                    symbol: resolved.definition.name.as_str(),
                })
                .collect();
            graph.insert(owner, callees);
        }
    }
    let mut state: BTreeMap<FunctionKey<'_>, u8> = BTreeMap::new();
    let mut stack = Vec::new();
    for &name in graph.keys() {
        if let Err(cycle) = walk_graph(name, &graph, &mut state, &mut stack) {
            let path = cycle
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(ReplayError::Parse(format!(
                "function call cycle across packages: {path}"
            )));
        }
    }
    Ok(())
}
