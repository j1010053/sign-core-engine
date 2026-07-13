//! chumsky parser(I6):行導向——lexer 已依行分組,每行以小型 parser 解析為
//! [`Line`],再以分組後處理把語句掛回所屬規則。錯誤以行號回報。
//!
//! 規則頭與語句同行合法(`dock-tone: dock …`);`stage:` 為保留語句頭,
//! 不會被誤認為規則名(P3/I14)。

use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Tok;

/// 一行解析結果。
#[derive(Debug, Clone, PartialEq)]
enum Line {
    Decl(Decl),
    RuleHeader(String, Option<Stmt>),
    ScanHeader(ScanHead),
    SpelloutHeader,
    SpelloutEntry(SpelloutEntry),
    Stmt(Stmt),
}

/// Spell-out 區塊內的一行。
#[derive(Debug, Clone, PartialEq)]
enum SpelloutEntry {
    Order(Vec<String>),
    Empty(String, String),
    Floating(String, String),
    Contour(String, Vec<String>, String),
}

/// 解析錯誤:行號(1 起算)+ 訊息。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parse error at line {line}: {msg}")]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

// ── 小工具 ──

fn ident() -> impl Parser<Tok, String, Error = Simple<Tok>> + Clone {
    select! { Tok::Ident(s) => s }
}

fn kw(s: &'static str) -> impl Parser<Tok, (), Error = Simple<Tok>> + Clone {
    select! { Tok::Ident(x) if x == s => () }
}

/// 特徵原子名:`+voice` / `-voice` / `labial`(號誌併入值名字串,lowering 拆解)。
fn atom() -> impl Parser<Tok, String, Error = Simple<Tok>> + Clone {
    just(Tok::Plus)
        .to("+")
        .or(just(Tok::Minus).to("-"))
        .or_not()
        .then(ident())
        .map(|(sign, name)| format!("{}{}", sign.unwrap_or(""), name))
}

/// `[atom atom …]`
fn matrix() -> impl Parser<Tok, Vec<String>, Error = Simple<Tok>> + Clone {
    atom()
        .repeated()
        .delimited_by(just(Tok::LBrack), just(Tok::RBrack))
}

fn element() -> impl Parser<Tok, Element, Error = Simple<Tok>> + Clone {
    choice((
        matrix().map(Element::Matrix),
        just(Tok::Hash).to(Element::Boundary),
        just(Tok::Dot).to(Element::SylBoundary),
        just(Tok::Star).to(Element::Star),
        just(Tok::At).ignore_then(ident()).map(Element::ClassRef),
        ident()
            .delimited_by(just(Tok::Lt), just(Tok::Gt))
            .map(Element::LevelRef),
        ident().map(|s| match s.as_str() {
            "Ø" => Element::Empty,
            "floating" => Element::Floating,
            _ => Element::Named(s),
        }),
    ))
}

fn selector() -> impl Parser<Tok, Selector, Error = Simple<Tok>> + Clone {
    element()
        .separated_by(just(Tok::Amp))
        .at_least(1)
        .map(Selector)
}

/// `/ pre? _ post?`
fn rule_env() -> impl Parser<Tok, RuleEnv, Error = Simple<Tok>> + Clone {
    just(Tok::Slash)
        .ignore_then(selector().or_not())
        .then_ignore(just(Tok::Underscore))
        .then(selector().or_not())
        .map(|(pre, post)| RuleEnv { pre, post })
}

// ── 語句 ──

fn stmt() -> impl Parser<Tok, Stmt, Error = Simple<Tok>> + Clone {
    let insert = kw("insert")
        .ignore_then(atom())
        .then_ignore(kw("floating"))
        .then(kw("near").ignore_then(ident()).or_not())
        .then(rule_env().or_not())
        .map(|((val, near), env)| Stmt::Insert { val, near, env });

    let dock = kw("dock")
        .ignore_then(selector())
        .then_ignore(kw("strategy"))
        .then(ident())
        .then(ident().or_not())
        .map(|((sel, strategy), tiebreak)| Stmt::Dock {
            sel,
            strategy,
            tiebreak,
        });

    let fill = kw("fill")
        .ignore_then(ident())
        .then_ignore(kw("Ø"))
        .then_ignore(just(Tok::Arrow))
        .then(atom())
        .then(kw("within").ignore_then(ident()).or_not())
        .map(|((tier, val), within)| Stmt::Fill { tier, val, within });

    let merge = kw("merge")
        .then(kw("adjacent-equal"))
        .to(Stmt::MergeAdjacentEqual);

    let spread = kw("spread")
        .ignore_then(atom())
        .then(ident())
        .then(kw("blocked-by").ignore_then(selector()).or_not())
        .then(kw("within").ignore_then(ident()).or_not())
        .then(kw("through").or_not().map(|t| t.is_some()))
        .then(kw("on-conflict").ignore_then(atom()).or_not())
        .map(
            |(((((val, ward), blocked_by), within), through), on_conflict)| Stmt::Spread {
                val,
                ward,
                blocked_by,
                within,
                through,
                on_conflict,
            },
        );

    let shift = kw("shift")
        .ignore_then(select! { Tok::Int(n) => n })
        .then(ident())
        .then(ident())
        .map(|((n, unit), ward)| Stmt::Shift { n, unit, ward });

    let dominate = kw("dominate")
        .ignore_then(selector())
        .then_ignore(just(Tok::ThinArrow))
        .then(selector())
        .then(ident())
        .map(|((sel, target), ward)| Stmt::Dominate { sel, target, ward });

    let stage = kw("stage")
        .then(just(Tok::Colon))
        .ignore_then(ident())
        .map(Stmt::Stage);

    let ordinal = just(Tok::LBrack)
        .ignore_then(
            select! { Tok::Int(n) => OrdinalAst::Nth(n) }
                .or(kw("first").to(OrdinalAst::First)),
        )
        .then_ignore(just(Tok::RBrack));

    let scan_assoc = kw("associate")
        .ignore_then(atom())
        .then_ignore(just(Tok::ThinArrow))
        .then(selector())
        .then(ordinal.or_not())
        .map(|((val, target), ordinal)| Stmt::ScanAssociate {
            val,
            target,
            ordinal,
        });

    let rewrite = selector()
        .then_ignore(just(Tok::Arrow))
        .then(selector())
        .then(rule_env().or_not())
        .map(|((from, to), env)| Stmt::Rewrite { from, to, env });

    choice((stage, insert, dock, fill, merge, spread, shift, dominate, scan_assoc, rewrite))
}

// ── 宣告 ──

fn decl() -> impl Parser<Tok, Decl, Error = Simple<Tok>> + Clone {
    let feature = kw("Feature")
        .ignore_then(ident())
        .then(
            atom()
                .separated_by(just(Tok::Comma))
                .at_least(1)
                .delimited_by(just(Tok::LParen), just(Tok::RParen)),
        )
        .map(|(name, values)| Decl::Feature { name, values });

    let symbol = kw("Symbol")
        .ignore_then(ident())
        .then(matrix().or_not())
        .map(|(name, atoms)| Decl::Symbol {
            name,
            atoms: atoms.unwrap_or_default(),
        });

    let class = kw("Class")
        .ignore_then(ident())
        .then(
            ident()
                .separated_by(just(Tok::Comma))
                .at_least(1)
                .delimited_by(just(Tok::LBrace), just(Tok::RBrace)),
        )
        .map(|(name, members)| Decl::Class { name, members });

    let parse_term = just(Tok::At)
        .ignore_then(ident())
        .then(just(Tok::Question).or_not().map(|q| q.is_some()))
        .map(|(class, optional)| ParseTerm { class, optional });
    let parse_alt = parse_term.separated_by(just(Tok::Colon).then(just(Tok::Colon))).at_least(1);
    let parse_decl = kw("Parse")
        .ignore_then(ident())
        .then_ignore(just(Tok::Colon))
        .then(parse_alt.separated_by(just(Tok::Pipe)).at_least(1))
        .map(|(level, alts)| Decl::Parse { level, alts });

    let prosody = kw("Prosody")
        .ignore_then(ident().separated_by(just(Tok::Lt)).at_least(1))
        .map(|chain| Decl::Prosody { chain });

    let melody = kw("Melody")
        .ignore_then(ident())
        .then(
            atom()
                .separated_by(just(Tok::Comma))
                .at_least(1)
                .delimited_by(just(Tok::LBrace), just(Tok::RBrace)),
        )
        .then_ignore(kw("anchor"))
        .then(ident())
        .map(|((name, values), anchor)| Decl::Melody {
            name,
            values,
            anchor,
        });

    choice((feature, symbol, class, parse_decl, prosody, melody))
}

// ── 行 → 檔 ──

fn scan_head() -> impl Parser<Tok, ScanHead, Error = Simple<Tok>> {
    kw("Scan")
        .ignore_then(ident())
        .then_ignore(kw("along"))
        .then(ident())
        .then(kw("within").ignore_then(ident()).or_not())
        .then(kw("from").ignore_then(ident()).or_not())
        .then(kw("over").ignore_then(ident()).or_not())
        .then_ignore(just(Tok::Colon))
        .map(|((((tier, along), within), from), over)| ScanHead {
            tier,
            along,
            within,
            from,
            over,
        })
}

fn spellout_entry() -> impl Parser<Tok, SpelloutEntry, Error = Simple<Tok>> {
    let order = kw("order")
        .ignore_then(ident().separated_by(just(Tok::Comma)).at_least(1))
        .map(SpelloutEntry::Order);
    let empty = kw("empty")
        .ignore_then(ident())
        .then_ignore(just(Tok::Arrow))
        .then(atom())
        .map(|(t, v)| SpelloutEntry::Empty(t, v));
    let floating = kw("floating")
        .ignore_then(ident())
        .then_ignore(just(Tok::Arrow))
        .then(ident())
        .then_ignore(ident().or_not()) // `drop warn` 的尾註容忍
        .map(|(t, p)| SpelloutEntry::Floating(t, p));
    let contour = kw("contour")
        .ignore_then(ident())
        .then_ignore(just(Tok::Colon))
        .then(
            atom()
                .repeated()
                .at_least(1)
                .delimited_by(just(Tok::LBrace), just(Tok::RBrace)),
        )
        .then_ignore(just(Tok::Arrow))
        .then(ident())
        .map(|((t, vals), name)| SpelloutEntry::Contour(t, vals, name));
    choice((order, empty, floating, contour))
}

fn line_parser() -> impl Parser<Tok, Line, Error = Simple<Tok>> {
    let header = ident()
        .try_map(|s, span| {
            if s == "stage" {
                Err(Simple::custom(span, "'stage' is a reserved statement head"))
            } else {
                Ok(s)
            }
        })
        .then_ignore(just(Tok::Colon))
        .then(stmt().or_not())
        .map(|(name, inline)| Line::RuleHeader(name, inline));

    let spellout_header = kw("Spell-out").then(just(Tok::Colon)).to(Line::SpelloutHeader);

    choice((
        decl().map(Line::Decl),
        scan_head().map(Line::ScanHeader),
        spellout_header,
        spellout_entry().map(Line::SpelloutEntry),
        stmt().map(Line::Stmt),
        header,
    ))
    .then_ignore(end())
}

/// 解析整檔(lexer 已行分組)。
pub fn parse_lines(lines: &[Vec<Tok>]) -> Result<FileAst, ParseError> {
    let mut file = FileAst::default();
    let p = line_parser();
    for (i, toks) in lines.iter().enumerate() {
        let lineno = i + 1; // 空行已濾除,此為邏輯行號
        let parsed = p.parse(toks.clone()).map_err(|errs| ParseError {
            line: lineno,
            msg: errs
                .first()
                .map(|e| format!("{e:?}"))
                .unwrap_or_else(|| "unknown".into()),
        })?;
        match parsed {
            Line::Decl(d) => {
                if !file.rules.is_empty() {
                    return Err(ParseError {
                        line: lineno,
                        msg: "declarations must precede rules".into(),
                    });
                }
                file.decls.push(d);
            }
            Line::RuleHeader(name, inline) => {
                let mut stmts = Vec::new();
                if let Some(s) = inline {
                    stmts.push(s);
                }
                file.rules.push(RuleAst {
                    name,
                    scan: None,
                    stmts,
                });
            }
            Line::SpelloutHeader => {
                file.spellout = Some(SpelloutAst::default());
            }
            Line::SpelloutEntry(e) => match file.spellout.as_mut() {
                None => {
                    return Err(ParseError {
                        line: lineno,
                        msg: "spell-out entry outside `Spell-out:` block".into(),
                    })
                }
                Some(sp) => match e {
                    SpelloutEntry::Order(v) => sp.order = v,
                    SpelloutEntry::Empty(t, v) => sp.empty.push((t, v)),
                    SpelloutEntry::Floating(_t, p) => sp.floating = Some(p),
                    SpelloutEntry::Contour(t, vals, name) => sp.contour.push((t, vals, name)),
                },
            },
            Line::ScanHeader(head) => {
                let name = format!("Scan({} along {})", head.tier, head.along);
                file.rules.push(RuleAst {
                    name,
                    scan: Some(head),
                    stmts: Vec::new(),
                });
            }
            Line::Stmt(s) => match file.rules.last_mut() {
                Some(r) => r.stmts.push(s),
                None => {
                    return Err(ParseError {
                        line: lineno,
                        msg: "statement outside of any rule".into(),
                    })
                }
            },
        }
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex_lines;

    fn parse(src: &str) -> FileAst {
        parse_lines(&lex_lines(src).unwrap()).unwrap()
    }

    #[test]
    fn parses_declarations() {
        let f = parse("Feature voice(+voice, -voice)\nSymbol p [-voice]\nSymbol a\nClass vowel {a}\nMelody tone {H, M, L} anchor mora\n");
        assert_eq!(f.decls.len(), 5);
        assert_eq!(
            f.decls[0],
            Decl::Feature {
                name: "voice".into(),
                values: vec!["+voice".into(), "-voice".into()]
            }
        );
        assert_eq!(
            f.decls[4],
            Decl::Melody {
                name: "tone".into(),
                values: vec!["H".into(), "M".into(), "L".into()],
                anchor: "mora".into()
            }
        );
    }

    #[test]
    fn parses_rule_with_inline_and_following_stmts() {
        let f = parse(
            "tonogenesis:\n    insert H floating near mora / onset&[-voice] _\n    insert L floating near mora / onset&[+voice] _\ndock-tone: dock tone&floating strategy nearest\n",
        );
        assert_eq!(f.rules.len(), 2);
        assert_eq!(f.rules[0].stmts.len(), 2);
        assert!(matches!(f.rules[0].stmts[0], Stmt::Insert { .. }));
        assert!(matches!(f.rules[1].stmts[0], Stmt::Dock { .. }));
    }

    #[test]
    fn parses_rewrite_and_stage_marker_p3_i14() {
        let f = parse("devoicing:\n    stage: word\n    [+voice]&onset => [-voice]\n");
        assert_eq!(f.rules[0].stmts[0], Stmt::Stage("word".into()));
        match &f.rules[0].stmts[1] {
            Stmt::Rewrite { from, to, env } => {
                assert_eq!(
                    from.0,
                    vec![
                        Element::Matrix(vec!["+voice".into()]),
                        Element::Named("onset".into())
                    ]
                );
                assert_eq!(to.0, vec![Element::Matrix(vec!["-voice".into()])]);
                assert!(env.is_none());
            }
            other => panic!("expected rewrite, got {other:?}"),
        }
    }

    #[test]
    fn parses_env_with_boundary() {
        let f = parse("final: [+voice] => [-voice] / _ #\n");
        match &f.rules[0].stmts[0] {
            Stmt::Rewrite { env: Some(e), .. } => {
                assert!(e.pre.is_none());
                assert_eq!(e.post.as_ref().unwrap().0, vec![Element::Boundary]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn statement_outside_rule_is_error() {
        let lines = lex_lines("merge adjacent-equal\n").unwrap();
        assert!(parse_lines(&lines).is_err());
    }
}
