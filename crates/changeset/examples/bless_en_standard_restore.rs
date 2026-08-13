//! **顯式** golden 重生工具:`tutorials/en-standard-reconstruction/restore.chg`。
//!
//! ```sh
//! cargo run -p conlang-changeset --example bless_en_standard_restore
//! ```
//!
//! # 為什麼是獨立工具,不是「測試自己寫檔」
//!
//! 環境變數會漏——CI、某人的 shell、一次忘了 unset。而一旦漏了,測試就用
//! **改寫期望值**的方式變綠:那是最壞的一種假綠燈,因為它看起來完全正常。
//!
//! 顯式工具不會被誤觸:要重生就得有人打這一行,而那一行會出現在 shell 歷史與
//! commit 訊息裡。
//!
//! # 什麼時候需要跑它
//!
//! **只有在 std 套件內容真的變了的時候。** `restore.chg` 的 prelude 鎖住四個
//! stdlib package 的內容 digest,而 `grammar.lang` 確實依賴它們
//! (`belongs Adposition` 來自 `std:core`、`belongs AttributionConstruction`
//! 來自 `std:cxg`)。套件一變,這份 `.chg` 就**真的不該再被 replay**——
//! 那個鎖是名實相符的,不是雜訊。
//!
//! # 不得手改 digest
//!
//! 2026-08-07 出過一次:有人把兩行 sha256 從引擎算得出來的值改成算不出來的值,
//! 於是那份教學材料變成 resolve 不過的死檔。digest 是**衍生值**,只能由這個
//! 工具重算。

use conlang_changeset::reconstruct::reconstruct;
use conlang_changeset::{change_set_prelude, ResolvedStatement, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};
use std::path::PathBuf;

const BASE: &str = include_str!("../../../tutorials/en-standard-reconstruction/base.lang");
const TARGET: &str = include_str!("../../language/lib/natural/en-standard/code/grammar.lang");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tutorials/en-standard-reconstruction/restore.chg");

    let base = LanguageDocument::import_new_root(BASE, "evo:en-standard")?;
    let target = LanguageDocument::import_new_root(TARGET, "evo:en-standard")?;
    let edits = reconstruct(&base, &target)?;

    let libraries = LibrarySpec::default();
    let prelude = change_set_prelude(&base, &libraries, "evo:en-standard-restore")?;
    let mut resolved = UnresolvedChangeSet::parse(&prelude)?.resolve(&base, &libraries)?;
    resolved.statements = vec![ResolvedStatement { ordinal: 0, edits }];
    let regenerated = resolved.dump().expect("dump");

    let previous = std::fs::read_to_string(&path).unwrap_or_default();
    if previous == regenerated {
        println!("unchanged: {}", path.display());
        return Ok(());
    }

    // 逐行報出改了什麼——重生一份 golden 時,人該看得見自己接受了什麼
    for (before, after) in diff_lines(&previous, &regenerated) {
        println!("- {before}");
        println!("+ {after}");
    }
    let body_changed = statements_of(&previous) != statements_of(&regenerated);
    println!(
        "statements: {}",
        if body_changed {
            "**changed** —— reconstruct 的產物變了,這不只是套件 digest 漂移,請確認是預期的"
        } else {
            "unchanged(只有 prelude 的衍生值變動)"
        }
    );

    std::fs::write(&path, &regenerated)?;
    println!("wrote: {}", path.display());
    Ok(())
}

/// 只報**內容不同**的那幾行(行數相同時逐行對;不同時整段報)。
fn diff_lines<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
    let (old, new): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    if old.len() != new.len() {
        return vec![("<行數不同,整份重寫>", "<see git diff>")];
    }
    old.into_iter()
        .zip(new)
        .filter(|(a, b)| a != b)
        .collect()
}

/// prelude 之後的部分(第一個 `#` 語句標記起)。
fn statements_of(text: &str) -> &str {
    text.find("\n    #").map(|at| &text[at..]).unwrap_or("")
}
