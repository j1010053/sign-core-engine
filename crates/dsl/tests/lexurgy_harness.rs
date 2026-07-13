//! Lexurgy 黑盒 harness 骨架(M0 步驟 7;完整匯入轉換器 = M2)。
//!
//! 現階段:(1) .wli 詞表讀取器;(2) .lsc 規則行分類器——把社群規則檔逐行分為
//! 「M0 子集可匯入 / 白名單機制(corpus/whitelist.md)」,對社群檔案跑分類統計
//! 作為結構確認。比對執行結果留 M2(匯入器 + Else:/defer 映射)。
//! 哨兵規則(I5):僅讀其資料格式,不引用實作。

use std::path::Path;

const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/lexurgy/cli/test");

/// .wli 讀取:一行一詞;空行與註解略過(格式參照 Lexurgy CLI 文件)。
fn read_wli(p: &Path) -> Vec<String> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("//"))
        .map(str::to_owned)
        .collect()
}

/// M0 子集分類:一行 .lsc 屬於哪一類。
fn classify(line: &str) -> &'static str {
    let l = line.trim();
    if l.is_empty() || l.starts_with('#') {
        return "blank/comment";
    }
    if l.starts_with("Feature") || l.starts_with("Symbol") || l.starts_with("Class") {
        return "decl-importable"; // 宣告子集(多值 Feature 需 M2 展開)
    }
    if l.starts_with("Diacritic") {
        return "whitelist-diacritic"; // 浮游變音符 → tier 提升(M2;白名單)
    }
    if l.starts_with("Syllables") || l.contains("::") {
        return "whitelist-syllables"; // 音節標注 → Span/Parse(M2;白名單)
    }
    if l.starts_with("Else") || l.starts_with("Then") || l.contains("defer") {
        return "whitelist-else-defer"; // docs/02 §13 開放項
    }
    if l.ends_with(':') {
        return "rule-header";
    }
    if l.contains("=>") {
        return "rewrite-candidate"; // 純符號/矩陣改寫:M0 子集候選
    }
    "other"
}

#[test]
fn harness_skeleton_reads_community_corpus() {
    let dir = Path::new(CORPUS);
    if !dir.exists() {
        eprintln!("corpus submodule not checked out; skipping");
        return;
    }
    // .wli 讀取器:社群詞表非空
    let words = read_wli(&dir.join("proto-syllabian.wli"));
    assert!(!words.is_empty(), "proto-syllabian.wli should list words");

    // 規則分類統計(muipidan + kharulian + nitherwe + syllabian)
    let mut stats: std::collections::BTreeMap<&str, usize> = Default::default();
    for f in ["muipidan.lsc", "kharulian.lsc", "nitherwe.lsc", "syllabian.lsc"] {
        let src = std::fs::read_to_string(dir.join(f)).unwrap_or_default();
        for line in src.lines() {
            *stats.entry(classify(line)).or_default() += 1;
        }
    }
    let mut out = String::new();
    for (k, v) in &stats {
        out.push_str(&format!("{k:<24} {v}\n"));
    }
    insta::assert_snapshot!("lexurgy_corpus_classification", out);
    // 骨架斷言:語料確實含 M0 子集候選與白名單機制兩類
    assert!(stats.get("rewrite-candidate").copied().unwrap_or(0) > 0);
    assert!(stats.get("whitelist-diacritic").copied().unwrap_or(0) > 0);
}
