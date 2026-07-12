//! strategy:候選挑選的統一解析器(D28)。
//!
//! 「選哪個」(selector)與「多候選怎麼挑」(strategy)分家(D17)。所有動詞的
//! 候選挑選一律呼叫本模組的 [`resolve`]——**動詞不得各自實作挑選邏輯**(D28;
//! 執行語意 §2)。輸入:候選錨點索引集 + 參考位置 + 策略;輸出:零或一個選擇。
//!
//! 自定義 strategy(以名稱註冊進同一模組)為 D28 預留的擴充點,隨 DSL(步驟 4+)
//! 的名稱解析一起引入;M0 步驟 3 僅內建三種。

/// 挑選方式(語法規格 §4)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pick {
    /// 距參考位置最近(D17;dock 的預設)。
    #[default]
    Nearest,
    Leftmost,
    Rightmost,
}

/// 等距 tie-break(預設 prefer-left,D17)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TieBreak {
    #[default]
    PreferLeft,
    PreferRight,
}

/// 動詞 strategy 槽的值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Strategy {
    pub pick: Pick,
    pub tiebreak: TieBreak,
}

impl Strategy {
    pub const NEAREST: Strategy = Strategy {
        pick: Pick::Nearest,
        tiebreak: TieBreak::PreferLeft,
    };
}

/// 統一解析器(D28):自 `candidates`(同一層的錨點索引,任意序)中挑零或一個。
/// `reference` 是呼叫端定義的參考位置(如 dock 的原位投影,I11);
/// `Leftmost`/`Rightmost` 不使用 reference。
pub fn resolve(candidates: &[u32], reference: u32, s: Strategy) -> Option<u32> {
    match s.pick {
        Pick::Leftmost => candidates.iter().copied().min(),
        Pick::Rightmost => candidates.iter().copied().max(),
        Pick::Nearest => candidates.iter().copied().min_by_key(|&c| {
            let dist = c.abs_diff(reference);
            // 等距時以 tie-break 決定:prefer-left 取索引小者、prefer-right 取大者。
            // (dist, 次序鍵) 字典序;prefer-right 用反向索引使大者勝。
            let tie = match s.tiebreak {
                TieBreak::PreferLeft => c,
                TieBreak::PreferRight => u32::MAX - c,
            };
            (dist, tie)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_with_tiebreak_d17() {
        // 參考 2,候選 {0,1,3}:1 與 3 等距(1),prefer-left 取 1
        assert_eq!(resolve(&[0, 1, 3], 2, Strategy::NEAREST), Some(1));
        // prefer-right 取 3
        let pr = Strategy {
            pick: Pick::Nearest,
            tiebreak: TieBreak::PreferRight,
        };
        assert_eq!(resolve(&[0, 1, 3], 2, pr), Some(3));
        // 無候選 → None(B8:呼叫端 noop)
        assert_eq!(resolve(&[], 2, Strategy::NEAREST), None);
    }

    #[test]
    fn leftmost_rightmost_ignore_reference() {
        let lm = Strategy {
            pick: Pick::Leftmost,
            tiebreak: TieBreak::PreferLeft,
        };
        let rm = Strategy {
            pick: Pick::Rightmost,
            tiebreak: TieBreak::PreferLeft,
        };
        assert_eq!(resolve(&[2, 0, 5], 99, lm), Some(0));
        assert_eq!(resolve(&[2, 0, 5], 99, rm), Some(5));
    }
}
