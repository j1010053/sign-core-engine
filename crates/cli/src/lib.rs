//! `conlang` CLI —— 步驟 22 的前置。
//!
//! # 為什麼 UI 之前先做 CLI
//!
//! CLAUDE.md §0.2 是硬規則:**「不存在『做完但沒有測試綠燈』的階段」**。
//! 而 UI 沒有天然的綠燈。CLI 的整合測試是真綠燈,不必為了 UI 發明一套勉強的
//! 出口——而 UI 之後呼叫的是**同一組** `conlang-command`,不是重寫。
//!
//! 它同時是整條路的第一次端到端檢查:專案開啟 → 編譯 → 查詢 → 命令 → 提交 →
//! 落盤。M0 當初也是這個順序(CLI 先跑通,UI 後面才來)。
//!
//! # 為什麼邏輯全在 lib
//!
//! [`run`] 取 argv 與一個輸出緩衝,不碰 stdout、不呼叫 `exit`。測試因此直接
//! 呼叫它即可,不必開子程序;`main.rs` 只是把結果印出去的五行。
//!
//! # 為什麼不用 clap
//!
//! 子命令只有五個、旗標形狀單純。實作原則 4「功能完整下精簡」——
//! 等旗標長到手寫會出錯時再換,那時測試已經在了。

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use conlang_app::{AppError, Workspace};
use conlang_changeset::evolution::NodeId;
use conlang_changeset::rewrite::{AtomicRewrite, DonorScope, RuleHome, ServiceContext};
use conlang_command::{lower, LanguageCommand};
use conlang_generate::Strategies;
use conlang_language::LibrarySpec;
use conlang_persistence::{GraphStore, StoreError};
use conlang_query::{
    dialect_groups, ExploratoryHeuristicV1, GroupingOverride, LexiconFilter, SortKey, TreeEdgeCut,
    ViewConfig,
};
use std::fmt::Write as _;

pub const USAGE: &str = "\
conlang <command> [options]

  open    <project>                       開啟專案,列出概況
  lexicon <project> [--node ID] [--category NAME] [--sort name|form|gloss]
  state   <project> [--node ID] [--set-time T] [--set-region R]
  groups  <project> [--threshold F]
  evolve  <project> [--node ID] --rule \"t => k\" [--home TRAIT] [--label L]

`--node` 省略時取第一個 root(依 id 序,故決定性)。
";

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("CLI_USAGE: {0}\n\n{USAGE}")]
    Usage(String),
    #[error("CLI_UNKNOWN_NODE: {0}")]
    UnknownNode(String),
    /// 兩個大的錯誤型別**裝箱**:`AppError` 逾 128 bytes,直接內嵌會讓每個
    /// `Result<_, CliError>` 都背著那份大小(clippy `result_large_err`)。
    #[error(transparent)]
    App(Box<AppError>),
    #[error(transparent)]
    Store(Box<StoreError>),
}

impl From<AppError> for CliError {
    fn from(error: AppError) -> CliError {
        CliError::App(Box::new(error))
    }
}

impl From<StoreError> for CliError {
    fn from(error: StoreError) -> CliError {
        CliError::Store(Box::new(error))
    }
}

/// 執行一個命令,輸出寫進 `out`。
pub fn run(args: &[String], out: &mut String) -> Result<(), CliError> {
    let Some(command) = args.first() else {
        return Err(CliError::Usage("缺少子命令".to_owned()));
    };
    let rest = &args[1..];
    match command.as_str() {
        "open" => open(rest, out),
        "lexicon" => lexicon(rest, out),
        "state" => state(rest, out),
        "groups" => groups(rest, out),
        "evolve" => evolve(rest, out),
        "help" | "--help" | "-h" => {
            out.push_str(USAGE);
            Ok(())
        }
        other => Err(CliError::Usage(format!("不認得的子命令 {other:?}"))),
    }
}

// ── 旗標解析(手寫;見模組文件)────────────────────────────────────────────

#[derive(Debug)]
struct Options {
    project: String,
    flags: Vec<(String, String)>,
}

fn parse(args: &[String]) -> Result<Options, CliError> {
    let Some(project) = args.first() else {
        return Err(CliError::Usage("缺少專案路徑".to_owned()));
    };
    let mut flags = Vec::new();
    let mut index = 1;
    while index < args.len() {
        let key = &args[index];
        if !key.starts_with("--") {
            return Err(CliError::Usage(format!("預期旗標,得到 {key:?}")));
        }
        let Some(value) = args.get(index + 1) else {
            return Err(CliError::Usage(format!("旗標 {key} 缺少值")));
        };
        flags.push((key.trim_start_matches("--").to_owned(), value.clone()));
        index += 2;
    }
    Ok(Options {
        project: project.clone(),
        flags,
    })
}

impl Options {
    fn get(&self, key: &str) -> Option<&str> {
        self.flags
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

/// 開專案並停在指定節點(省略則是第一個 root)。
fn workspace(options: &Options) -> Result<Workspace, CliError> {
    let store = GraphStore::open(&options.project)?;
    let mut workspace = Workspace::open(&store, LibrarySpec::default())?;
    if let Some(wanted) = options.get("node") {
        let id = NodeId::parse(wanted)
            .map_err(|_| CliError::UnknownNode(wanted.to_owned()))?;
        workspace.session_mut().open(&id)?;
    }
    Ok(workspace)
}

// ── 子命令 ───────────────────────────────────────────────────────────────

fn open(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let workspace = workspace(&options)?;
    let session = workspace.session();

    let name = workspace
        .project()
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| "(未命名)".to_owned());
    let _ = writeln!(out, "project: {name}");
    let _ = writeln!(
        out,
        "declaration: {}",
        if workspace.project().is_some() {
            "project.toml"
        } else {
            "(無;使用預設套件組合)"
        }
    );
    let spec = session.libraries();
    let _ = writeln!(
        out,
        "packages: std={} natural={} plugins={}",
        spec.std.len(),
        spec.natural
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "-".to_owned()),
        spec.plugins.len()
    );
    let _ = writeln!(out, "nodes: {}", session.graph().len());
    let _ = writeln!(
        out,
        "active: {}",
        session
            .active()
            .map(|id| id.as_str().to_owned())
            .unwrap_or_else(|| "-".to_owned())
    );
    Ok(())
}

fn lexicon(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let mut workspace = workspace(&options)?;

    let mut filter = LexiconFilter::all();
    if let Some(category) = options.get("category") {
        filter = filter.with_category(category);
    }
    let view = ViewConfig {
        sort: match options.get("sort") {
            Some("form") => SortKey::UnderlyingForm,
            Some("gloss") => SortKey::Gloss,
            _ => SortKey::Name,
        },
    };

    let lexicon = workspace.lexicon(&filter, &view)?;
    let _ = writeln!(
        out,
        "{} / {} entries",
        lexicon.entries.len(),
        lexicon.total_before_filter
    );
    for entry in &lexicon.entries {
        let _ = writeln!(
            out,
            "  {:<16} {:<12} {}",
            entry.name,
            entry.underlying_form.as_deref().unwrap_or("-"),
            entry.gloss.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

/// 步驟 20 欠的「State 的 UI 顯示」——先在 CLI 還。
fn state(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let store = GraphStore::open(&options.project)?;
    let workspace = workspace(&options)?;
    let Some(id) = workspace.session().active().cloned() else {
        return Err(CliError::Usage("這個專案沒有任何節點".to_owned()));
    };

    let mut state = store.read_state(&id)?;
    let mut changed = false;
    if let Some(time) = options.get("set-time") {
        state.time = Some(time.to_owned());
        changed = true;
    }
    if let Some(region) = options.get("set-region") {
        state.region = Some(region.to_owned());
        changed = true;
    }
    if changed {
        store.write_state(&id, &state)?;
    }

    let _ = writeln!(out, "node: {}", id.as_str());
    let _ = writeln!(out, "time: {}", state.time.as_deref().unwrap_or("-"));
    let _ = writeln!(out, "region: {}", state.region.as_deref().unwrap_or("-"));
    let _ = writeln!(
        out,
        "society: {}",
        if state.society.is_empty() {
            "-".to_owned()
        } else {
            state.society.join(", ")
        }
    );
    let _ = writeln!(out, "contacts: {}", state.contacts.len());
    for contact in &state.contacts {
        let _ = writeln!(
            out,
            "  {} ({:?}) {}",
            contact.counterpart,
            contact.intensity,
            contact.period.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn groups(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let workspace = workspace(&options)?;
    let threshold = options
        .get("threshold")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.6);

    let grouping = dialect_groups(
        workspace.session().graph(),
        &TreeEdgeCut { threshold },
        &ExploratoryHeuristicV1::suggested(),
        &GroupingOverride::default(),
    );
    let _ = writeln!(
        out,
        "measure: {} threshold: {}",
        grouping.measure_id, grouping.threshold
    );
    for group in grouping.groups() {
        let label = grouping
            .labels
            .get(group)
            .map(String::as_str)
            .unwrap_or(group);
        let _ = writeln!(out, "  {label}: {}", grouping.members_of(group).join(", "));
    }
    Ok(())
}

fn evolve(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let Some(rule) = options.get("rule") else {
        return Err(CliError::Usage("evolve 需要 --rule".to_owned()));
    };
    let store = GraphStore::open(&options.project)?;
    let mut workspace = workspace(&options)?;

    let namespace = format!("cli:{}", workspace.session().graph().len());
    workspace.session_mut().begin_edit(&namespace)?;
    let document = workspace.session().snapshot()?.clone();
    let rewrite = AtomicRewrite::SoundChange {
        home: RuleHome::Global(options.get("home").unwrap_or("Core").to_owned()),
        body: rule.to_owned(),
    };
    let edits = lower(
        &LanguageCommand::ApplyRewrite(&rewrite),
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .map_err(|error| CliError::Usage(format!("規則降階失敗:{error}")))?;
    workspace.session_mut().stage(edits)?;

    let id = workspace
        .session_mut()
        .commit(options.get("label").map(str::to_owned))?;
    workspace.session().persist(&store)?;

    let _ = writeln!(out, "committed: {}", id.as_str());
    let _ = writeln!(out, "nodes: {}", workspace.session().graph().len());
    Ok(())
}
