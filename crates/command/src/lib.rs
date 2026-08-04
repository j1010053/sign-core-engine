//! 資訊流 D 的 **Command 層**(鳥瞰步驟 21;流 D 框架 §3)。
//!
//! # 這一層的全部內容是三件事
//!
//! **① Command 分三類,型別上就分開。**
//! 原案把 `set_view_config` 與 `adopt_proposal` 並列為 `command::*`,再說
//! 「每個 command 都留下可 replay 的 `.chg`」——**自相矛盾**,前者根本不產生
//! `PrimitiveEdit`。分成三個型別之後,「視圖編輯不進 replay」不再是註解裡的
//! 約定,而是**編譯期事實**:`ViewCommand` 上沒有降階函數。
//!
//! | 類 | 去向 | replay | digest |
//! |---|---|---|---|
//! | [`LanguageCommand`] | `PrimitiveEdit` → `.chg` → 節點 | ✅ | ✅ |
//! | [`ViewCommand`] | `views/<name>.json` | ❌ | ❌ |
//! | [`ProjectDataCommand`] | `data/`、`project.toml` | ❌ | ❌ |
//!
//! **② 每個 `LanguageCommand` 都降階為四原語(C1)。**
//! [`lower`] 是唯一入口,底下一律走既有的 `generate::build` 或
//! `rewrite::expand`。繞過四原語直接改 `Language` 會讓改動不可 replay、
//! 進不了演化圖、三道 digest 失去意義。
//!
//! **③ Command ≠ ChangeSet。**
//! 這三層早已實作,只是原案把它們當成同一個:
//!
//! ```text
//! LanguageCommand  →  Vec<PrimitiveEdit>          ← lower()
//!                  →  一個 ResolvedStatement       ← stage(),交易單位(P26)
//! 數個 Statement   →  一份 ResolvedChangeSet       ← 提交邊界 = 一條演化邊
//! ```
//!
//! 故 UI 得以:預覽後再提交、多步操作合併成一個歷史節點、單一 command 失敗
//! 不污染 pending buffer。
//!
//! # 沒有 `CommandResult { diagnostics, preview }`
//!
//! 外部審查建議過。但 `build`/`expand` 的失敗走 `Result`,**沒有**「成功但有
//! 診斷」的產生者;而「預覽」就是「拿到 edits 但還沒 stage」——已經有了。
//! 依《共時lang語法與資料貼合度》「不先造無消費者語法」,兩者都不造。
//!
//! # pending buffer 不需要新格式
//!
//! 未提交的編輯 = 一個基底節點 + 一疊 statements,而那就是 `.chg` 的定義。
//! 故 [`stage`] 直接往 `ResolvedChangeSet::statements` 追加,不另立 `working/`
//! 格式——那會變成兩種東西說同一件事(實作原則 3),且提交時還要轉換。
//!
//! # 可攜性
//!
//! 本 crate 受 CLAUDE.md §4 約束(無 fs / 執行緒 / 時鐘 / 環境亂數),wasm 綠。
//! 實際寫檔的是 `conlang-app`。

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use conlang_changeset::rewrite::{expand, AtomicRewrite, DonorScope, RewriteError, ServiceContext};
use conlang_changeset::{PrimitiveEdit, ResolvedChangeSet, ResolvedStatement};
use conlang_generate::{build, BuildError, Need, Proposal, Strategies};
use conlang_language::LanguageDocument;

/// 會改變 Language 的意圖。**只有這一類降階為四原語。**
#[derive(Debug, Clone)]
pub enum LanguageCommand<'a> {
    /// 採用一個造詞候選(步驟 18 的 `Need → Proposal → Builder`)。
    ///
    /// 候選**怎麼選**不在這裡——`ranked()`(手動)與 `sample_proposal()`(自動)
    /// 是選擇層的事(P70:列舉與選擇分離)。這個 command 只接受「已經選定的那個」。
    AdoptProposal {
        need: &'a Need,
        chosen: &'a Proposal,
    },
    /// 套用一個 Atomic Rewrite(音變、漂移、重新分析、借入……)。
    ///
    /// 12 項是**封閉內建集**(P16);使用者可寫的是 Recipe/Goal 層,不得新增
    /// rewrite。故這裡收整個 `AtomicRewrite` 而非為每一項開一個 variant——
    /// 後者只是把同一份封閉集抄第二遍,兩邊會失同步。
    ApplyRewrite(&'a AtomicRewrite),
}

/// 只改視圖詮釋的意圖。**不產生 `PrimitiveEdit`,不進 replay。**
///
/// 去向 `views/<name>.json`(R4 一套一檔)。`演化圖本體論` §19 的鐵律:
/// 互通度、方言界線、顯示分類皆為派生視圖,**永不回寫資料層**。
///
/// 本型別**刻意沒有** `lower()`——那是這條界線的型別層落實。
#[derive(Debug, Clone, PartialEq)]
pub enum ViewCommand {
    /// 換排序/呈現設定。
    SetViewConfig { view: String, sort: String },
    /// 方言分群的**分類指派**(D-f2:不是 merge/split)。
    ///
    /// 指派是函數不是關係,故不可能互相矛盾,結果由建構保證唯一。
    AssignGroup { view: String, node: String, group: String },
    /// 群組**顯示名**。純展示,不影響群組身分。
    LabelGroup { view: String, group: String, label: String },
}

/// 只改專案級資料的意圖。**不產生 `PrimitiveEdit`,不進 replay。**
///
/// 去向 `data/`(R2 專案級覆寫)與 `project.toml`(R3 import 表)。
/// 同樣**刻意沒有** `lower()`。
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectDataCommand {
    /// 手動覆寫某個音段的抽樣權重(統計先驗 §2 的最高層)。
    SetWeight { table: String, key: String, weight: f64 },
    /// 專案的直接依賴宣告(R3;R15 已裁定其合法性)。
    DeclarePackage { id: String, version: String },
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error(transparent)]
    Rewrite(#[from] RewriteError),
}

/// 把一個 `LanguageCommand` 降階為四原語。**唯一的降階入口。**
///
/// `services` / `donors` 是**注入**的(P53 的 `ServiceContext` 接點、
/// `DonorScope` 的借入來源),`strategies` 同理(步驟 18 的三個 Strategy
/// 委派不內建)。本函數不替呼叫端造預設值——那會讓「用了哪套策略」變成
/// 藏在引擎裡的事。
pub fn lower(
    command: &LanguageCommand<'_>,
    document: &LanguageDocument,
    strategies: &Strategies,
    services: &ServiceContext,
    donors: &DonorScope<'_>,
) -> Result<Vec<PrimitiveEdit>, CommandError> {
    match command {
        LanguageCommand::AdoptProposal { need, chosen } => {
            Ok(build(need, chosen, document, strategies)?)
        }
        LanguageCommand::ApplyRewrite(rewrite) => {
            Ok(expand(rewrite, document, services, donors)?)
        }
    }
}

/// 把一批原語**堆進 pending buffer**,成為一個 statement(交易單位,P26)。
///
/// 回傳該 statement 的序號。序號 = 既有 statements 的數量,故決定性。
///
/// 這裡不做提交:提交邊界是整份 `ResolvedChangeSet` 交給
/// `ChangeInterpreter` 跑、再 `EvolutionGraph::commit` 落成節點。
/// 多個 command 可以先堆在一起,合併成一個歷史節點——那正是
/// 「設計期加 50 個詞不該變成 50 個演化節點」的作法(流 D 框架 §5.2)。
pub fn stage(pending: &mut ResolvedChangeSet, edits: Vec<PrimitiveEdit>) -> u64 {
    let ordinal = pending.statements.len() as u64;
    pending.statements.push(ResolvedStatement { ordinal, edits });
    ordinal
}

/// 丟棄最後一個 statement(未提交編輯的 undo;流 D 框架 §5.5 的 (A) 線)。
///
/// 回傳被丟掉的那個。空的時候回 `None`,不是錯誤。
///
/// (A) 專案編輯的 undo **就是編輯這份 `.chg`**,不需要 app 另外維護 undo
/// stack——那是一份文件的編輯歷史,與 `views/` 那條線同型。
pub fn discard_last(pending: &mut ResolvedChangeSet) -> Option<ResolvedStatement> {
    pending.statements.pop()
}
