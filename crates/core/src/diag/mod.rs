//! diag:訊息分級資料型別(B9;執行語意 §7)。
//!
//! 診斷是**回傳資料**,不是列印(可移植性規範 §1.2)。四級行為:
//! error = 停止該詞演化;warn = 記錄續跑;info = 追蹤視圖顯示;trace = 逐步除錯。
//! 良構檢查(`InvariantIssue`)經 [`from_issues`] 映射入級;B8(動詞無目標 noop)
//! 與 lazy 重剖觸發由 executor 以 info 級記錄。

use crate::repr::invariant::{InvariantIssue, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagLevel {
    Trace,
    Info,
    Warn,
    Error,
}

/// 一則診斷:級別 + 訊息(結構化來源保留於 `issue`,追蹤視圖可再投影)。
#[derive(Debug, Clone, PartialEq)]
pub struct Diag {
    pub level: DiagLevel,
    pub message: String,
    /// 來源良構問題(若由 Validation 產生);規則層事件(noop/reparse)為 None。
    pub issue: Option<InvariantIssue>,
}

impl Diag {
    pub fn info(message: impl Into<String>) -> Diag {
        Diag {
            level: DiagLevel::Info,
            message: message.into(),
            issue: None,
        }
    }
    pub fn trace(message: impl Into<String>) -> Diag {
        Diag {
            level: DiagLevel::Trace,
            message: message.into(),
            issue: None,
        }
    }
}

/// Validation 產物 → 分級診斷(嚴重度對映執行語意 §7)。
pub fn from_issues(issues: &[InvariantIssue]) -> Vec<Diag> {
    issues
        .iter()
        .map(|i| Diag {
            level: match i.severity() {
                Severity::Error => DiagLevel::Error,
                Severity::Warn => DiagLevel::Warn,
                Severity::Info => DiagLevel::Info,
            },
            message: format!("{i:?}"),
            issue: Some(i.clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::prosody::Level;

    #[test]
    fn issue_levels_map_to_diag_levels_b9() {
        let issues = vec![
            InvariantIssue::EmptyProsodyNode {
                level: Level::Mora,
                index: 1,
            }, // info
            InvariantIssue::SyllableCoverageGap { at: 2 }, // warn
            InvariantIssue::SyllableOverlap { a: 0, b: 1 }, // error
        ];
        let d = from_issues(&issues);
        assert_eq!(
            d.iter().map(|x| x.level).collect::<Vec<_>>(),
            vec![DiagLevel::Info, DiagLevel::Warn, DiagLevel::Error]
        );
        assert!(d[0].issue.is_some());
        assert!(DiagLevel::Error > DiagLevel::Warn); // 排序可用於過濾門檻
    }
}
