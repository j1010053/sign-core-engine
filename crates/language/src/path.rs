//! Path 表達式(P22/P25;修補05 §3.5,經 P92 縮減):
//! `Path := Name ( '.' Name )*`
//!
//! 同一套文法兩用:條件裡是測試、primitive 裡是定址(P25)。
//! 三道界(P22):無量詞、無計算、無副作用——本模組只解析,不求值。
//!
//! ## 為什麼只剩點分名段
//!
//! 原文法另有 `[鍵]` 與 `~tier` 兩種段。量測(修補13 §6)顯示:
//!
//! - 全庫 201 個 `$` 引用、以及所有 Def lhs,**零處**用到這兩種段;
//! - 查找端(`project(dim).get(&path)`、`FillerSnapshot::scalar`)是**字串鍵
//!   比對**,不解讀結構——三種段的表達力完全相同,`[k]` 只是鍵名裡的字元;
//! - P71 之後自造欄位一律走 `feature:`(單一識別字),多段路徑的唯一來源是
//!   套件座標前綴(`syn.alignment.…`),那些也不用這兩種段;
//! - Step 13 的欄位定址(`identity.rs`)本來就只收 `Name` 段。
//!
//! 留著等於讓讀文法的人推論出「有索引、有 tier 導覽」這種不存在的能力。
//! `[序數]` 與 `~tier` 在它們真正有語意的地方(`.qy` 的 Scan 序數、旋律層
//! tier)不受影響——那是引擎域的獨立文法。

/// Path 的一節。目前只有具名欄位一種。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSeg {
    /// `.name` 欄位存取(首節 = Anchor 名)。
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path(pub Vec<PathSeg>);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid path {src:?} at byte {at}: {msg}")]
pub struct PathError {
    pub src: String,
    pub at: usize,
    pub msg: String,
}

fn ident_ok(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/')
}

/// 解析 Path(`sign.syn.category`、`syn.alignment.ergative`)。
pub fn parse_path(src: &str) -> Result<Path, PathError> {
    let err = |at: usize, msg: &str| PathError {
        src: src.to_owned(),
        at,
        msg: msg.to_owned(),
    };
    let mut segs = Vec::new();
    let mut pos = 0usize;
    for (index, name) in src.split('.').enumerate() {
        if !ident_ok(name) {
            return Err(err(
                pos,
                if index == 0 {
                    "anchor must be a bare name"
                } else {
                    "expected field name after '.'"
                },
            ));
        }
        segs.push(PathSeg::Name(name.to_owned()));
        pos += name.len() + 1;
    }
    Ok(Path(segs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dotted_name_segments() {
        let p = parse_path("cxn.syn.animate").unwrap();
        assert_eq!(
            p.0,
            vec![
                PathSeg::Name("cxn".into()),
                PathSeg::Name("syn".into()),
                PathSeg::Name("animate".into()),
            ]
        );
    }

    #[test]
    fn rejects_malformed_paths() {
        assert!(parse_path("").is_err());
        assert!(parse_path(".x").is_err());
        assert!(parse_path("a..b").is_err());
        assert!(parse_path("a b").is_err());
    }

    /// P92:`[鍵]` 與 `~tier` 不再是 Path 的一部分。
    #[test]
    fn key_and_tier_segments_are_no_longer_path_syntax() {
        assert!(parse_path("cxn.slot[agent].syn").is_err(), "`[鍵]` 已移除");
        assert!(parse_path("t~tone").is_err(), "`~tier` 已移除");
    }
}
