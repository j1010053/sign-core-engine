//! conlang-core — autosegmental 音變引擎核心。
//!
//! - M0 步驟 1:`repr` 表徵模組(《M0 實作參照》§3、§5)。
//! - M0 步驟 2:`lifecycle`(規則生命週期,執行語意 §1)+ `primitives`(六原語)。
//! - M0 步驟 3:`verbs` 第一批(insert/dock/fill/merge,全組合原語)+ `strategy`(D28)。
//!
//! # 可移植性規範(《M0 實作參照》§1.2,CI 掛 wasm32-unknown-unknown)
//! - 禁多執行緒、禁 `std::fs`、禁 `std::time`、禁環境隨機源
//! - 禁 `panic!` / `println!`:錯誤走 `Result<_, ReprError>`,診斷是回傳資料
//! - `debug_assert!` 僅限內部不變量

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod lifecycle;
pub mod locality;
pub mod primitives;
pub mod repr;
pub mod strategy;
pub mod verbs;
