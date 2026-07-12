//! logos lexer(I6):入口先 NFC 正規化(ã 一碼 vs 兩碼的變音符坑,統一於此)。
//!
//! 詞法極簡:識別字(含連字號,涵蓋 `dock-tone`/`adjacent-equal`/`prefer-left`)+
//! 標點。關鍵字不設專用 token——在 parser 依字串判定,避免與規則名/IPA 符號撞名
//! (strategy 名是開放集 D28,本就須以識別字承載)。
//!
//! 註解:`;` 至行尾(**暫定**:規格未定註解語法;`#` 已被詞界佔用 D19,
//! `//` 已被 Lexurgy 沿用欄佔用)。詞界 `#` 為獨立 token。

use logos::Logos;
use unicode_normalization::UnicodeNormalization;

#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r]+")]
pub enum Tok {
    #[token("\n")]
    Newline,

    #[regex(r";[^\n]*", logos::skip)]
    Comment,

    #[token(":")]
    Colon,
    #[token("&")]
    Amp,
    #[token("=>")]
    #[token("→")]
    Arrow,
    #[token("/")]
    Slash,
    #[token("_")]
    Underscore,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBrack,
    #[token("]")]
    RBrack,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(",")]
    Comma,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("#")]
    Hash,
    #[token("*")]
    Star,

    /// 識別字:字母開頭,可含字母/變音/數字/連字號(IPA 友善)。
    /// `Ø` 亦落入此類,由 parser 判為零特徵記號。
    #[regex(r"[\p{L}][\p{L}\p{M}\p{N}\-]*", |lex| lex.slice().to_owned())]
    Ident(String),
}

/// 詞法錯誤:位置 + 該行內容片段。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("lex error at byte {at}: unexpected character {snippet:?}")]
pub struct LexError {
    pub at: usize,
    pub snippet: String,
}

/// 整檔 token 化(先 NFC),回傳**依行分組**的 token 序列(空行濾除)。
/// 行導向讓 parser 逐行小步解析,錯誤定位天然以行為界。
pub fn lex_lines(src: &str) -> Result<Vec<Vec<Tok>>, LexError> {
    let src: String = src.nfc().collect();
    let mut lines: Vec<Vec<Tok>> = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut lexer = Tok::lexer(&src);
    while let Some(t) = lexer.next() {
        match t {
            Ok(Tok::Newline) => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
            }
            Ok(tok) => cur.push(tok),
            Err(()) => {
                return Err(LexError {
                    at: lexer.span().start,
                    snippet: lexer.slice().to_owned(),
                })
            }
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_rule_line_with_hyphen_names_and_matrix() {
        let lines = lex_lines("dock-tone: dock tone&floating strategy nearest\n").unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0][0], Tok::Ident("dock-tone".into()));
        assert_eq!(lines[0][1], Tok::Colon);
        let lines = lex_lines("[+voice]&onset => [-voice] ; devoice\n").unwrap();
        assert_eq!(
            lines[0],
            vec![
                Tok::LBrack,
                Tok::Plus,
                Tok::Ident("voice".into()),
                Tok::RBrack,
                Tok::Amp,
                Tok::Ident("onset".into()),
                Tok::Arrow,
                Tok::LBrack,
                Tok::Minus,
                Tok::Ident("voice".into()),
                Tok::RBrack,
            ]
        );
    }

    #[test]
    fn nfc_normalizes_at_entry() {
        // "ã" 兩碼(a + U+0303)與一碼應 lex 出同一識別字
        let two = lex_lines("a\u{0303}\n").unwrap();
        let one = lex_lines("\u{00e3}\n").unwrap();
        assert_eq!(two, one);
    }
}
