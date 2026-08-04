//! 資訊流 D 的 **Query 層**(鳥瞰步驟 21;`邏輯分層架構_v0.1` §3.2)。
//!
//! # 這個 crate 的定義性約束:純函數
//!
//! `邏輯分層` §3.2 把 Query 定成「計算視圖、回答查詢;接收 View Config +
//! Override **作為參數**(**無狀態純函數**)」。故本 crate:
//!
//! - 受 CLAUDE.md §4 可攜性約束——**無 `std::fs`、無執行緒、無時鐘、無環境亂數**、
//!   無 `panic!`/`unwrap()`(測試除外);wasm 必須綠。
//! - 有狀態的東西(工作階段歷史、快取、專案讀寫)一律住 `conlang-app`。
//!
//! **為什麼是獨立 crate 而不是 module**(D-a2):§4 的可攜性是 **crate 粒度的
//! CI 閘門**(`cargo build -p <crate> --target wasm32-unknown-unknown`)。一個
//! 同時需要 fs 的 crate,裡面的「純 module」無從驗證——整個 crate 對 wasm 就
//! 建不起來。純度在本專案是靠 crate 邊界檢查的,不是靠約定。
//!
//! # 這一層幾乎不含新語言學
//!
//! 它**組裝**既有能力:`changeset::diff`、`stats::project_phoneme_freq`、
//! `SignDef::project`、`SemNode::of_sign`。真正需要新設計的互通度與方言分群
//! 留待後續(流 D 框架 §4)。
//!
//! # 派生視圖永不回寫資料層
//!
//! `演化圖本體論` §19 的鐵律。本 crate 全部函數取 `&` 並回傳新值,
//! 不持有可變狀態,也不產生任何 `PrimitiveEdit`。

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod derivation;
pub mod lexicon;

pub use derivation::{derivation_family, DerivationDag, DerivationNode, SenseLink};
pub use lexicon::{lexicon, Lexicon, LexiconEntry, LexiconFilter, SortKey, ViewConfig};

// 既有能力原樣轉出,使 Query 的門面是一處而非四處。**不重新實作**。
pub use conlang_changeset::diff::{diff_vector, DiffVector};
pub use conlang_stats::{project_phoneme_freq, WeightTable};
