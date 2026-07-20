//! Language parser(步驟 9):`.lang` 原文 → [`Language`]。
//!
//! 行導向遞降(格式為行/括號結構,I6 的 chumsky 決策屬 DSL 規則語言,不及於此)。
//! - **dsl 域區**(裁決 docs/13 §4-1):首個 language 構造之前的非空行 = dsl 域
//!   verbatim(feature/symbol/class,Lexurgy 形),不解析。
//! - language 構造:`prosody =`、`distribution {`、`global trait X {`、`trait X {`、
//!   `sign X {`;trait 內 `==` 切 Block(P27);sign 內 `Name[n]` = trait 引用。
//! - `path = value` = Definition(路徑經 `path::parse_path` 驗證);
//!   含 `=>` 之行 = Rule(尾綴 `@stage`,預設 word);後續 `else …` 行掛入
//!   該規則的 else 鏈(P22)。
//! - id 依文件序決定性再生(I15-b/P26)。
//!
//! round-trip:對 canonical 輸入,`parse(src).dump() == src`(P21)。

use crate::path::parse_path;
use crate::{Block, Def, Dim, Item, Language, SignItem, Stage, TraitDef};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parse error at line {line}: {msg}")]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

fn err(line: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        msg: msg.into(),
    }
}

fn is_language_head(l: &str) -> bool {
    l.starts_with("prosody =")
        || l.starts_with("distribution {")
        || l.starts_with("global trait ")
        || l.starts_with("trait ")
        || l.starts_with("sign ")
        || dim_trait_head(l).is_some()
}

/// `<dim> trait Name {` 頭?回傳 dim(P38 ontology trait 標記)。
fn dim_trait_head(l: &str) -> Option<Dim> {
    let (kw, rest) = l.split_once(" trait ")?;
    let _ = rest;
    Dim::parse(kw)
}

/// 容器頭:`<kw> Name {` → Name。
fn container_name<'a>(l: &'a str, kw: &str, line: usize) -> Result<&'a str, ParseError> {
    let rest = l[kw.len()..].trim();
    let Some(name) = rest.strip_suffix('{').map(str::trim) else {
        return Err(err(line, format!("expected `{kw} Name {{`")));
    };
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Err(err(line, "container name must be a single identifier"));
    }
    Ok(name)
}

/// 一行 → Def 或 Rule 頭(不含 else 續行)。
///
/// 分類(I17-a):先剝尾綴 `@stage X`(省略 = word);含 `=>` = Rule;
/// 否則含 `=` = Definition(路徑驗證,錯誤有行號);**兩者皆無 = Rule,
/// body 為原文 dsl 域動詞語句**(insert/dock/fill/merge/spread/dominate/Scan …
/// ——phon 規則屬 dsl 域,language 不解析其內部,修補05 §1.5)。
fn parse_item(lang: &mut Language, l: &str, line: usize) -> Result<Item, ParseError> {
    let (body, stage, had_stage) = match l.rsplit_once(" @stage ") {
        Some((b, s)) => {
            let stage = match s.trim() {
                "stem" => Stage::Stem,
                "word" => Stage::Word,
                "phrase" => Stage::Phrase,
                other => return Err(err(line, format!("unknown stage {other:?}"))),
            };
            (b.trim(), stage, true)
        }
        None => (l, Stage::Word, false),
    };
    if body.contains("=>") {
        Ok(Item::Rule(lang.rule(body, stage)))
    } else if let Some((path, value)) = body.split_once('=') {
        if had_stage {
            return Err(err(line, "`@stage` is not allowed on a Definition"));
        }
        let path = path.trim();
        parse_path(path).map_err(|e| err(line, e.to_string()))?;
        Ok(Item::Def(Def {
            path: path.to_owned(),
            value: value.trim().to_owned(),
        }))
    } else {
        Ok(Item::Rule(lang.rule(body, stage)))
    }
}

/// `belongs Name`(P40)?回傳目標 trait 名(單一識別字)。
/// 攔在 `parse_item` 之前:belongs 無 `=>`/`=`,否則會被誤判為 dsl 動詞規則(I17-a)。
fn belongs_target(l: &str, line: usize) -> Result<Option<String>, ParseError> {
    let Some(rest) = l.strip_prefix("belongs ") else {
        return Ok(None);
    };
    let name = rest.trim();
    if name.is_empty()
        || name.contains(char::is_whitespace)
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(err(line, "`belongs` expects a single trait name"));
    }
    Ok(Some(name.to_owned()))
}

/// `Name[n]` trait 引用?
fn trait_use(l: &str) -> Option<(String, u32)> {
    let open = l.find('[')?;
    let name = &l[..open];
    let rest = l[open + 1..].strip_suffix(']')?;
    let block: u32 = rest.parse().ok()?;
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
    .then(|| (name.to_owned(), block))
}

pub fn parse(src: &str) -> Result<Language, ParseError> {
    let mut lang = Language::new();
    let lines: Vec<(usize, &str)> = src
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim_end()))
        .collect();
    let mut i = 0usize;
    let mut seen_language = false;

    while i < lines.len() {
        let (ln, raw) = lines[i];
        let l = raw.trim();
        if l.is_empty() {
            i += 1;
            continue;
        }

        if !seen_language && !is_language_head(l) {
            lang.dsl_decls.push(l.to_owned()); // dsl 域 verbatim(I15-a)
            i += 1;
            continue;
        }
        seen_language = true;

        if let Some(rest) = l.strip_prefix("prosody =") {
            lang.prosody = rest.split_whitespace().map(str::to_owned).collect();
            i += 1;
        } else if l == "distribution {" {
            i += 1;
            while i < lines.len() {
                let (ln2, raw2) = lines[i];
                let l2 = raw2.trim();
                i += 1;
                if l2 == "}" {
                    break;
                }
                if l2.is_empty() {
                    continue;
                }
                let Some((k, v)) = l2.split_once('=') else {
                    return Err(err(ln2, "distribution entry must be `key = value`"));
                };
                lang.distribution
                    .push((k.trim().to_owned(), v.trim().to_owned()));
            }
        } else if l.starts_with("global trait ")
            || l.starts_with("trait ")
            || dim_trait_head(l).is_some()
        {
            let global = l.starts_with("global trait ");
            let dim = dim_trait_head(l);
            let kw = if global {
                "global trait".to_owned()
            } else if let Some(d) = dim {
                format!("{} trait", d.keyword())
            } else {
                "trait".to_owned()
            };
            let name = container_name(l, &kw, ln)?.to_owned();
            let mut blocks = vec![Block::default()];
            i += 1;
            let mut closed = false;
            while i < lines.len() {
                let (ln2, raw2) = lines[i];
                let l2 = raw2.trim();
                i += 1;
                if l2 == "}" {
                    closed = true;
                    break;
                }
                if l2.is_empty() {
                    continue;
                }
                if l2 == "==" {
                    blocks.push(Block::default()); // P27:Block 節點邊界
                    continue;
                }
                if let Some(target) = belongs_target(l2, ln2)? {
                    blocks
                        .last_mut()
                        .expect("nonempty")
                        .items
                        .push(Item::Belongs(target));
                    continue;
                }
                if let Some(rest) = l2.strip_prefix("else ") {
                    // P22:掛入前一條規則的 else 鏈
                    let Some(Item::Rule(r)) =
                        blocks.last_mut().and_then(|b| b.items.last_mut())
                    else {
                        return Err(err(ln2, "`else` without a preceding rule"));
                    };
                    r.else_chain.push(rest.trim().to_owned());
                    continue;
                }
                let item = parse_item(&mut lang, l2, ln2)?;
                blocks.last_mut().expect("nonempty").items.push(item);
            }
            if !closed {
                return Err(err(ln, format!("unclosed `{{` for {kw} {name}")));
            }
            lang.add_trait(TraitDef {
                name,
                global,
                dim,
                blocks,
            });
        } else if l.starts_with("sign ") {
            let name = container_name(l, "sign", ln)?.to_owned();
            let mut items: Vec<SignItem> = Vec::new();
            i += 1;
            let mut closed = false;
            while i < lines.len() {
                let (ln2, raw2) = lines[i];
                let l2 = raw2.trim();
                i += 1;
                if l2 == "}" {
                    closed = true;
                    break;
                }
                if l2.is_empty() {
                    continue;
                }
                if let Some(target) = belongs_target(l2, ln2)? {
                    items.push(SignItem::Belongs(target));
                    continue;
                }
                if let Some((tname, block)) = trait_use(l2) {
                    items.push(SignItem::TraitUse { name: tname, block });
                    continue;
                }
                if let Some(rest) = l2.strip_prefix("else ") {
                    let Some(SignItem::Rule(r)) = items.last_mut() else {
                        return Err(err(ln2, "`else` without a preceding rule"));
                    };
                    r.else_chain.push(rest.trim().to_owned());
                    continue;
                }
                match parse_item(&mut lang, l2, ln2)? {
                    Item::Def(d) => items.push(SignItem::Def(d)),
                    Item::Rule(r) => items.push(SignItem::Rule(r)),
                    Item::Belongs(t) => items.push(SignItem::Belongs(t)),
                }
            }
            if !closed {
                return Err(err(ln, format!("unclosed `{{` for sign {name}")));
            }
            lang.add_sign(name, items);
        } else {
            return Err(err(ln, format!("unexpected line {l:?}")));
        }
    }
    Ok(lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_located() {
        assert!(parse("trait T {\n").unwrap_err().msg.contains("unclosed"));
        let e = parse("trait T {\n    else x\n}\n").unwrap_err();
        assert!(e.msg.contains("else"), "{e}");
        assert_eq!(e.line, 2);
        assert!(parse("sign s {\n    ..bad = 1\n}\n").is_err());
    }

    /// dsl 域區:首個 language 構造前的行原樣保留(裁決 1)。
    #[test]
    fn dsl_region_is_verbatim_by_file_position() {
        let l = parse("Feature voice(+voice, -voice)\n\nprosody = μ σ\n").unwrap();
        assert_eq!(l.dsl_decls, vec!["Feature voice(+voice, -voice)"]);
        assert_eq!(l.prosody, vec!["μ", "σ"]);
    }
}
