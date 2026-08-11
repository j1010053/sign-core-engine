//! 把應用層的 [`ViewCommand`] 翻成資料層的 [`ViewDocument`] 修改。
//!
//! # 為什麼翻譯住這裡
//!
//! 擁有者裁定(2026-08-04):**`conlang-persistence` 不得認得 `ViewCommand`**。
//! 它擁有「檔案裡放什麼、怎麼讀寫」(`ViewDocument` + `read_view`/`write_view`),
//! 但不該隨 UI 的意圖集合一起長——那是 §2.2「app 不得自訂第二套格式」的鏡像:
//! **格式層也不該把應用層的語意吃進去。**
//!
//! 故 app 做這一層翻譯:意圖 → 資料。加一個新的 `ViewCommand` 只動這裡。

use conlang_command::ViewCommand;
use conlang_persistence::ViewDocument;

/// 套用一個視角意圖。回傳它動的是哪一個視角檔。
///
/// **不寫檔**——寫入由呼叫端在攢完一批之後做一次
/// (同 §3.3 的「多步操作合併成一次」)。
pub fn apply_view_command(document: &mut ViewDocument, command: &ViewCommand) -> String {
    match command {
        ViewCommand::SetViewConfig { view, sort } => {
            document.sort = Some(sort.clone());
            view.clone()
        }
        ViewCommand::AssignGroup { view, node, group } => {
            document.assignments.insert(node.clone(), group.clone());
            view.clone()
        }
        ViewCommand::LabelGroup { view, group, label } => {
            document.labels.insert(group.clone(), label.clone());
            view.clone()
        }
    }
}
