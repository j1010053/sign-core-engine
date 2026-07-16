//! Path 表達式(P22/P25;修補05 §3.4–3.5):
//! `Path := Anchor ( '.' Name | '[' Key ']' | '~' TierRef )*`
//!
//! 同一套文法兩用:條件裡是測試、primitive 裡是定址(P25)。
//! 三道界(P22):無量詞、無計算、無副作用——本模組只解析,不求值。

/// Path 的一節。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSeg {
    /// `.name` 欄位存取(首節 = Anchor 名)。
    Name(String),
    /// `[key]` slot/序數存取(與 `TR[1]`、`<syl>[2]` 同記法)。
    Key(String),
    /// `~tier` 聯結存取(非線性;tier 相對)。
    Tier(String),
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

/// 解析 Path(`sign.syn.category`、`cxn.slot[agent].syn.animate`、`_~H` 的 `~` 節)。
pub fn parse_path(src: &str) -> Result<Path, PathError> {
    let err = |at: usize, msg: &str| PathError {
        src: src.to_owned(),
        at,
        msg: msg.to_owned(),
    };
    let mut segs = Vec::new();
    let mut rest = src;
    let mut pos = 0usize;
    // Anchor(首節):裸名
    let head_end = rest
        .find(['.', '[', '~'])
        .unwrap_or(rest.len());
    let head = &rest[..head_end];
    if !ident_ok(head) {
        return Err(err(0, "anchor must be a bare name"));
    }
    segs.push(PathSeg::Name(head.to_owned()));
    pos += head_end;
    rest = &rest[head_end..];

    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('.') {
            let end = r.find(['.', '[', '~']).unwrap_or(r.len());
            let name = &r[..end];
            if !ident_ok(name) {
                return Err(err(pos + 1, "expected field name after '.'"));
            }
            segs.push(PathSeg::Name(name.to_owned()));
            pos += 1 + end;
            rest = &r[end..];
        } else if let Some(r) = rest.strip_prefix('[') {
            let Some(close) = r.find(']') else {
                return Err(err(pos, "unclosed '['"));
            };
            let key = &r[..close];
            if !ident_ok(key) {
                return Err(err(pos + 1, "bad key in '[...]'"));
            }
            segs.push(PathSeg::Key(key.to_owned()));
            pos += 1 + close + 1;
            rest = &r[close + 1..];
        } else if let Some(r) = rest.strip_prefix('~') {
            let end = r.find(['.', '[', '~']).unwrap_or(r.len());
            let tier = &r[..end];
            if !ident_ok(tier) {
                return Err(err(pos + 1, "expected tier name after '~'"));
            }
            segs.push(PathSeg::Tier(tier.to_owned()));
            pos += 1 + end;
            rest = &r[end..];
        } else {
            return Err(err(pos, "unexpected character in path"));
        }
    }
    Ok(Path(segs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_field_key_tier_segments() {
        let p = parse_path("cxn.slot[agent].syn.animate").unwrap();
        assert_eq!(
            p.0,
            vec![
                PathSeg::Name("cxn".into()),
                PathSeg::Name("slot".into()),
                PathSeg::Key("agent".into()),
                PathSeg::Name("syn".into()),
                PathSeg::Name("animate".into()),
            ]
        );
        let p = parse_path("t~tone").unwrap();
        assert_eq!(
            p.0,
            vec![PathSeg::Name("t".into()), PathSeg::Tier("tone".into())]
        );
    }

    #[test]
    fn rejects_malformed_paths() {
        assert!(parse_path("").is_err());
        assert!(parse_path(".x").is_err());
        assert!(parse_path("a[unclosed").is_err());
        assert!(parse_path("a..b").is_err());
    }
}
