//! Language parser(步驟 9 + I22 語法重設計):`.lang` 原文 → [`Language`]。
//!
//! **colon + 縮排**風格(貼合 tshiatun/Lexurgy;取代 `{ }`)。統一 body 語法
//! (I22):`sign`/`trait`/`global trait` 三種容器頭 `Name:` 後縮排 body,body 語法
//! **trait 與 sign 完全相同**——
//! - `belongs X`(分類,維度中立單一樹,P38 v0.2);
//! - `Name[n]`(macro 引用,P5/P27);`==`(block 邊界);
//! - **維度區塊** `syn:` / `phon:` / `sem:` / `prag:`,內縮為該維內容:
//!   - `syn:` 下 `slots:` → slot 行(`name [Filler]` 尾綴 `?` = optional,I21);
//!     `field = value` → Def `syn.field`;
//!   - `phon:` 下 `/…/` → Def `phon`(UR/模板);其餘行 → phon 規則(`=>`/dsl 動詞;
//!     尾綴 `@stage`,`else` 續行掛前一規則,P22);
//!   - `sem:`/`prag:` 下 `field = value` → `sem.field`/`prag.field`。
//! - `prosody = …`、`distribution:`(縮排 `key = value`)為 language 級語句。
//! dsl 域宣告(Feature/Symbol/Class/Melody/Spell-out/Parse…)= 首個 language 頭
//! 之前的 verbatim 行(裁決 1)。id 依文件序決定性再生(I15-b/P26)。
//!
//! round-trip:對 canonical 輸入,`parse(src).dump() == src`(P21)。

use crate::{Block, Def, Language, Rule, SignItem, Slot, Stage, TraitDef};

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

/// 一行:行號、縮排空格數、去空白內容。
struct Line {
    no: usize,
    indent: usize,
    text: String,
}

fn indent_of(raw: &str) -> usize {
    raw.chars().take_while(|c| *c == ' ').count()
}

/// 容器頭 `<kw> Name:` → (kw, name)?
fn container_head(text: &str) -> Option<(&'static str, &str)> {
    for kw in ["global trait", "trait", "sign"] {
        if let Some(rest) = text.strip_prefix(kw) {
            if let Some(name) = rest.strip_suffix(':') {
                let name = name.trim();
                if !name.is_empty() && !name.contains(char::is_whitespace) {
                    return Some((kw, name));
                }
            }
        }
    }
    None
}

fn is_language_head(text: &str) -> bool {
    text.starts_with("prosody =") || text == "distribution:" || container_head(text).is_some()
}

/// `Name[n]` trait 引用?
fn trait_use(l: &str) -> Option<(String, u32)> {
    let open = l.find('[')?;
    let name = &l[..open];
    let block: u32 = l[open + 1..].strip_suffix(']')?.parse().ok()?;
    (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
        .then(|| (name.to_owned(), block))
}

fn ident_ok(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(char::is_whitespace)
        && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// `belongs Name`。
fn belongs_target(l: &str, line: usize) -> Result<Option<String>, ParseError> {
    let Some(rest) = l.strip_prefix("belongs ") else {
        return Ok(None);
    };
    let name = rest.trim();
    if !ident_ok(name) {
        return Err(err(line, "`belongs` expects a single trait name"));
    }
    Ok(Some(name.to_owned()))
}

/// `name [Filler]` (+ optional `?`) → Slot(I21)。
fn parse_slot(l: &str, line: usize) -> Result<Slot, ParseError> {
    let Some(open) = l.find('[') else {
        return Err(err(line, "slot must be `name [Filler]`"));
    };
    let name = l[..open].trim();
    let after = l[open + 1..].trim_end();
    let optional = after.ends_with("]?");
    let Some(inner) = after.strip_suffix("]?").or_else(|| after.strip_suffix(']')) else {
        return Err(err(line, "slot filler must be `[Trait]` (optional `?`)"));
    };
    let filler = inner.trim();
    if !ident_ok(name) || !ident_ok(filler) {
        return Err(err(line, "slot name and filler must be single identifiers"));
    }
    Ok(Slot {
        name: name.to_owned(),
        filler: filler.to_owned(),
        optional,
    })
}

/// 規則行 → Rule(尾綴 `@stage`;body 原文)。
fn parse_rule(lang: &mut Language, l: &str, line: usize) -> Result<Rule, ParseError> {
    let (body, stage) = match l.rsplit_once(" @stage ") {
        Some((b, s)) => {
            let stage = match s.trim() {
                "stem" => Stage::Stem,
                "word" => Stage::Word,
                "phrase" => Stage::Phrase,
                other => return Err(err(line, format!("unknown stage {other:?}"))),
            };
            (b.trim(), stage)
        }
        None => (l, Stage::Word),
    };
    Ok(lang.rule(body, stage))
}

/// 維度區塊上下文。
#[derive(Clone, Copy, PartialEq)]
enum DimKw {
    Syn,
    Phon,
    Sem,
    Prag,
}
impl DimKw {
    fn prefix(self) -> &'static str {
        match self {
            DimKw::Syn => "syn",
            DimKw::Phon => "phon",
            DimKw::Sem => "sem",
            DimKw::Prag => "prag",
        }
    }
    fn parse(s: &str) -> Option<DimKw> {
        match s {
            "syn:" => Some(DimKw::Syn),
            "phon:" => Some(DimKw::Phon),
            "sem:" => Some(DimKw::Sem),
            "prag:" => Some(DimKw::Prag),
            _ => None,
        }
    }
}

/// 解析一個容器 body(縮排行序列)→ Vec<Block>(`==` 切 block,統一 body I22)。
fn parse_body(lang: &mut Language, body: &[Line]) -> Result<Vec<Block>, ParseError> {
    let mut blocks = vec![Block::default()];
    if body.is_empty() {
        return Ok(blocks);
    }
    let base = body[0].indent; // level-1 縮排
    let mut cur_dim: Option<DimKw> = None;
    let mut in_slots = false;
    let mut slots_indent = 0usize;

    for ln in body {
        let (no, ind, text) = (ln.no, ln.indent, ln.text.as_str());
        if in_slots && ind <= slots_indent {
            in_slots = false;
        }
        if ind == base {
            // level-1:重設維度上下文
            cur_dim = None;
            in_slots = false;
            if text == "==" {
                blocks.push(Block::default());
            } else if let Some(t) = belongs_target(text, no)? {
                blocks.last_mut().unwrap().items.push(SignItem::Belongs(t));
            } else if let Some(dim) = DimKw::parse(text) {
                cur_dim = Some(dim);
            } else if let Some((name, block)) = trait_use(text) {
                blocks
                    .last_mut()
                    .unwrap()
                    .items
                    .push(SignItem::TraitUse { name, block });
            } else if !text.contains("=>")
                && text.split_once('=').is_some_and(|(f, _)| ident_ok(f.trim()))
            {
                // 頂層非維度 Def(如 `entrenchment = 0.5`)
                let (path, value) = text.split_once('=').unwrap();
                blocks.last_mut().unwrap().items.push(SignItem::Def(Def {
                    path: path.trim().to_owned(),
                    value: value.trim().to_owned(),
                }));
            } else {
                return Err(err(no, format!("unexpected line {text:?} (expected belongs / Name[n] / <dim>: / field = value / ==)")));
            }
            continue;
        }
        // ind > base:維度區塊內容
        let Some(dim) = cur_dim else {
            return Err(err(no, format!("indented line {text:?} outside a dimension block")));
        };
        if in_slots {
            let slot = parse_slot(text, no)?;
            blocks.last_mut().unwrap().items.push(SignItem::Slot(slot));
            continue;
        }
        if text == "slots:" {
            if dim != DimKw::Syn {
                return Err(err(no, "`slots:` only under `syn:`"));
            }
            in_slots = true;
            slots_indent = ind;
            continue;
        }
        if let Some(rest) = text.strip_prefix("else ") {
            let Some(SignItem::Rule(r)) = blocks.last_mut().unwrap().items.last_mut() else {
                return Err(err(no, "`else` without a preceding rule"));
            };
            r.else_chain.push(rest.trim().to_owned());
            continue;
        }
        // phon 維:`/…/` = UR/模板 Def;其餘 = phon 規則
        if dim == DimKw::Phon
            && text.starts_with('/')
            && text.ends_with('/')
            && text.len() >= 2
        {
            blocks.last_mut().unwrap().items.push(SignItem::Def(Def {
                path: "phon".to_owned(),
                value: text.to_owned(),
            }));
            continue;
        }
        // `field = value`(單一 ident + `=`,非 `=>`)→ Def(path 加維度前綴);
        // 否則視為該維規則(phon 的 dsl 動詞、`A => B`、syn/sem/prag 規則於 12d)。
        let is_def = !text.contains("=>")
            && text
                .split_once('=')
                .is_some_and(|(f, _)| ident_ok(f.trim()));
        if is_def {
            let (field, value) = text.split_once('=').unwrap();
            blocks.last_mut().unwrap().items.push(SignItem::Def(Def {
                path: format!("{}.{}", dim.prefix(), field.trim()),
                value: value.trim().to_owned(),
            }));
        } else {
            let r = parse_rule(lang, text, no)?;
            blocks.last_mut().unwrap().items.push(SignItem::Rule(r));
        }
    }
    Ok(blocks)
}

pub fn parse(src: &str) -> Result<Language, ParseError> {
    let mut lang = Language::new();
    let lines: Vec<Line> = src
        .lines()
        .enumerate()
        .map(|(i, raw)| Line {
            no: i + 1,
            indent: indent_of(raw),
            text: raw.trim().to_owned(),
        })
        .collect();

    let mut i = 0usize;
    let mut seen_language = false;

    while i < lines.len() {
        let ln = &lines[i];
        if ln.text.is_empty() {
            i += 1;
            continue;
        }
        // dsl 域 verbatim(首個 language 頭之前)
        if !seen_language && !is_language_head(&ln.text) {
            lang.dsl_decls.push(ln.text.clone());
            i += 1;
            continue;
        }
        seen_language = true;

        if let Some(rest) = ln.text.strip_prefix("prosody =") {
            lang.prosody = rest.split_whitespace().map(str::to_owned).collect();
            i += 1;
        } else if ln.text == "distribution:" {
            let base = ln.indent;
            i += 1;
            while i < lines.len() && (lines[i].text.is_empty() || lines[i].indent > base) {
                let l2 = &lines[i];
                i += 1;
                if l2.text.is_empty() {
                    continue;
                }
                let Some((k, v)) = l2.text.split_once('=') else {
                    return Err(err(l2.no, "distribution entry must be `key = value`"));
                };
                lang.distribution
                    .push((k.trim().to_owned(), v.trim().to_owned()));
            }
        } else if let Some((kw, name)) = container_head(&ln.text) {
            let header_indent = ln.indent;
            let name = name.to_owned();
            i += 1;
            // 蒐集 body(縮排 > header 的非空行)
            let mut body: Vec<Line> = Vec::new();
            while i < lines.len() {
                if lines[i].text.is_empty() {
                    i += 1;
                    continue;
                }
                if lines[i].indent <= header_indent {
                    break;
                }
                body.push(Line {
                    no: lines[i].no,
                    indent: lines[i].indent,
                    text: lines[i].text.clone(),
                });
                i += 1;
            }
            let blocks = parse_body(&mut lang, &body)?;
            match kw {
                "sign" => {
                    let items: Vec<SignItem> =
                        blocks.into_iter().flat_map(|b| b.items).collect();
                    lang.add_sign(name, items);
                }
                _ => lang.add_trait(TraitDef {
                    name,
                    global: kw == "global trait",
                    blocks,
                }),
            }
        } else {
            return Err(err(ln.no, format!("unexpected line {:?}", ln.text)));
        }
    }
    Ok(lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsl_region_is_verbatim_by_file_position() {
        let l = parse("Feature voice(+voice, -voice)\n\nprosody = μ σ\n").unwrap();
        assert_eq!(l.dsl_decls, vec!["Feature voice(+voice, -voice)"]);
        assert_eq!(l.prosody, vec!["μ", "σ"]);
    }

    #[test]
    fn errors_are_located() {
        let e = parse("sign s:\n    ??bad\n").unwrap_err();
        assert_eq!(e.line, 2);
    }
}
