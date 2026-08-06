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
use conlang_changeset::evolution::{EvolutionGraph, NodeId};
use conlang_changeset::rewrite::{AtomicRewrite, DonorScope, RuleHome, ServiceContext};
use conlang_command::{lower, LanguageCommand};
use conlang_generate::{
    build, ranked, DistributionGenerator, GenerationError, Generator, Need, NeedOrigin,
    Strategies,
};
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument, StoreError};
use conlang_query::{
    dialect_groups, project_phoneme_freq, ExploratoryHeuristicV1, GroupingOverride, LexiconFilter,
    SortKey, TreeEdgeCut, ViewConfig,
};
use conlang_stats::{parse_prior_table, EffectiveDistribution, WeightTable};
use std::fmt::Write as _;

pub const USAGE: &str = "\
conlang <command> [options]

  init    <project> --from FILE.lang [--name N] [--namespace NS]
  open    <project>                       開啟專案,列出概況
  lexicon <project> [--node ID] [--category NAME] [--sort name|form|gloss]
  state   <project> [--node ID] [--set-time T] [--set-region R]
  groups  <project> [--threshold F]
  evolve  <project> [--node ID] --rule \"t => k\" [--home TRAIT] [--label L]
  propose <project> --name N --gloss G --weights FILE
                    [--category C] [--template CVC] [--count K] [--adopt I]
  stats   <project> [--node ID] [--weights FILE]
  annotate <project> [--node ID] [--path P] [--set TEXT]

`--node` 省略時取第一個 root(依 id 序,故決定性)。
";

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("CLI_USAGE: {0}\n\n{USAGE}")]
    Usage(String),
    #[error("CLI_UNKNOWN_NODE: {0}")]
    UnknownNode(String),
    #[error("CLI_WEIGHTS: {path}: {message}")]
    Weights { path: String, message: String },
    #[error(transparent)]
    Generation(#[from] GenerationError),
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
        "init" => init(rest, out),
        "open" => open(rest, out),
        "lexicon" => lexicon(rest, out),
        "state" => state(rest, out),
        "groups" => groups(rest, out),
        "evolve" => evolve(rest, out),
        "propose" => propose(rest, out),
        "stats" => stats(rest, out),
        "annotate" => annotate(rest, out),
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

/// 從一份 `.lang` 建一個新專案。
///
/// 沒有這個,CLI 就只能開別人做好的專案——教學也無從從零開始。
/// 寫出的 `project.toml` 記的是**這次實際用來載入的套件組合**,故建完立刻
/// 開得起來(不會存出一份自己開不起來的宣告)。
fn init(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let Some(source_path) = options.get("from") else {
        return Err(CliError::Usage("init 需要 --from <FILE.lang>".to_owned()));
    };
    let source = std::fs::read_to_string(source_path).map_err(|error| CliError::Usage(format!(
        "讀不到 {source_path}:{error}"
    )))?;
    let namespace = options.get("namespace").unwrap_or("root");
    let document = LanguageDocument::import_new_root(&source, namespace)
        .map_err(|error| CliError::Usage(format!("{source_path} 解析失敗:{error}")))?;

    let store = GraphStore::init(&options.project)?;
    let libraries = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(libraries.clone());
    let root = graph.add_root(document).map_err(AppError::Evolution)?;
    store.save(&graph)?;

    let mut project = ProjectDocument::from_spec(&libraries);
    project.name = options.get("name").map(str::to_owned);
    store.write_project(&project)?;

    let _ = writeln!(out, "created: {}", options.project);
    let _ = writeln!(out, "root: {}", root.as_str());
    Ok(())
}

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

// ── 候選詞 / 統計 / 旁註 ─────────────────────────────────────────────────

/// 讀一份 `segment<TAB>weight` 表。**解析器與 package `data/` 共用**,
/// 不在這裡重寫格式知識。
fn weights(path: &str) -> Result<WeightTable, CliError> {
    let source = std::fs::read_to_string(path).map_err(|error| CliError::Weights {
        path: path.to_owned(),
        message: error.to_string(),
    })?;
    parse_prior_table(path, &source).map_err(|error| CliError::Weights {
        path: path.to_owned(),
        message: error.to_string(),
    })
}

/// 候選詞:**手動模式**(P12/步驟 18)——引擎只排序,選擇權交出去。
///
/// # 為什麼一定要 `--weights`
///
/// 分佈只有三層(手動 > 導入 provider > E1 先驗),而 **E1 目前沒有實際資料**
/// (步驟 19 記明:PHOIBLE/Grambank 子集需離線匯入)。故沒有手動表就沒有分佈。
///
/// **不能拿 `stats` 的投影來頂**——`統計先驗與抽樣引擎` §6.1 已把統計投影
/// **移出抽樣棧**,它只是報表。這裡寧可報錯,也不偷偷把報表當先驗用。
fn propose(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let (Some(name), Some(gloss)) = (options.get("name"), options.get("gloss")) else {
        return Err(CliError::Usage("propose 需要 --name 與 --gloss".to_owned()));
    };
    let Some(path) = options.get("weights") else {
        return Err(CliError::Usage(
            "propose 需要 --weights:三層分佈的手動層。E1 先驗目前無實際資料,\
             而統計投影(見 `stats`)依 §6.1 不得充當抽樣來源"
                .to_owned(),
        ));
    };
    let table = weights(path)?;
    let store = GraphStore::open(&options.project)?;
    let mut workspace = workspace(&options)?;
    let compiled = workspace.compiled()?;

    let need = Need {
        name: name.to_owned(),
        categories: options
            .get("category")
            .map(|c| vec![c.to_owned()])
            .unwrap_or_default(),
        gloss: Some(gloss.to_owned()),
        origin: NeedOrigin::Coined,
    };
    let distribution = EffectiveDistribution::default()
        .with_manual(table)
        .resolve();
    let proposals = DistributionGenerator {
        distribution: &distribution,
        template: options.get("template").unwrap_or("CVC"),
        count: options
            .get("count")
            .and_then(|c| c.parse().ok())
            .unwrap_or(8),
        seed: 0,
    }
    .propose(&need, &compiled)?;

    let ordered = ranked(&proposals);
    let _ = writeln!(out, "{} candidates for {name:?}", ordered.len());
    for (index, proposal) in ordered.iter().enumerate() {
        let _ = writeln!(out, "  [{index}] {} score={:.3}", proposal.phon, proposal.score);
    }

    let Some(pick) = options.get("adopt") else {
        return Ok(());
    };
    let index: usize = pick
        .parse()
        .map_err(|_| CliError::Usage(format!("--adopt 要一個序號,得到 {pick:?}")))?;
    let Some(chosen) = ordered.get(index) else {
        return Err(CliError::Usage(format!("沒有第 {index} 個候選")));
    };

    // 採用 = 走 Builder 降階四原語(C1),與其他命令同一條路
    let document = workspace.session().snapshot()?.clone();
    let edits = build(&need, chosen, &document, &Strategies::default())
        .map_err(|error| CliError::Usage(format!("造詞失敗:{error}")))?;
    let namespace = format!("cli:coin:{}", workspace.session().graph().len());
    workspace.session_mut().begin_edit(&namespace)?;
    workspace.session_mut().stage(edits)?;
    let id = workspace.session_mut().commit(Some(namespace))?;
    workspace.session().persist(&store)?;

    let _ = writeln!(out, "adopted [{index}] {} -> {}", chosen.phon, id.as_str());
    Ok(())
}

/// 統計投影:**唯讀報表**,不是抽樣來源(§6.1)。
///
/// 切分依**給定的音素清單**最長匹配(§6.6)。清單由 `--weights` 的鍵提供;
/// 沒給就退回逐字元,而那會把塞擦音之類的多字元音段拆開——故輸出會標明。
fn stats(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let workspace = workspace(&options)?;
    let document = workspace.session().snapshot()?;

    let table = match options.get("weights") {
        Some(path) => Some(weights(path)?),
        None => None,
    };
    let inventory: Vec<&str> = table.as_ref().map(|t| t.keys().collect()).unwrap_or_default();
    let report = project_phoneme_freq(document.language(), &inventory);

    let _ = writeln!(
        out,
        "segmentation: {}",
        if inventory.is_empty() {
            "per-character(未給 --weights;多字元音段會被拆開)"
        } else {
            "longest-match against --weights keys"
        }
    );
    let _ = writeln!(out, "note: 報表,非抽樣來源(§6.1)");
    let total: f64 = report.iter().map(|(_, count)| count).sum();
    let _ = writeln!(out, "{} distinct / {total} total", report.len());
    for (segment, count) in report.iter() {
        let _ = writeln!(out, "  {segment:<8} {count}");
    }
    Ok(())
}

/// 旁註層:**正交於本體**(07 §5c)——不參與 replay、不被 diff、不約束生成。
fn annotate(args: &[String], out: &mut String) -> Result<(), CliError> {
    let options = parse(args)?;
    let store = GraphStore::open(&options.project)?;
    let workspace = workspace(&options)?;
    let Some(id) = workspace.session().active().cloned() else {
        return Err(CliError::Usage("這個專案沒有任何節點".to_owned()));
    };

    match (options.get("path"), options.get("set")) {
        (Some(path), Some(text)) => {
            store.write_annotation(&id, path, text.as_bytes())?;
            let _ = writeln!(out, "wrote {path} ({} bytes)", text.len());
        }
        (Some(path), None) => {
            let content = store.read_annotation(&id, path)?;
            let _ = writeln!(out, "{}", String::from_utf8_lossy(&content));
        }
        (None, _) => {
            let files = store.list_annotations(&id)?;
            let _ = writeln!(out, "node: {}", id.as_str());
            let _ = writeln!(out, "annotations: {}", files.len());
            for file in files {
                let _ = writeln!(out, "  {}", file.display());
            }
        }
    }
    Ok(())
}
