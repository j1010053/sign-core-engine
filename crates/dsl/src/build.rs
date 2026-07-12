//! 造詞器:表層字串 → `Word`(臨時韻律域,P1)。
//!
//! **暫定 CV 音節化**(M0 步驟 4 的 CLI 端到端所需):以宣告的 `Class vowel`
//! 判別核心;每個元音起一音節、成一莫拉(onset 不入莫拉,I8);元音間的輔音串
//! 全歸下一音節 onset(最大 onset);詞尾輔音併入末音節為 coda(不成莫拉)。
//! 正式的 `Parse` 宣告(語法規格 §5,pattern language)於步驟 5+ 取代本函數。
//!
//! 符號化:貪婪最長匹配宣告過的 `Symbol`(NFC 已於 lexer 入口統一,此處再保險)。

use conlang_core::repr::prosody::Span;
use conlang_core::repr::word::{Seg, Word};
use unicode_normalization::UnicodeNormalization;

use crate::lower::Program;
use crate::DslError;

/// 表層字串 → Word(含 Program 宣告的所有 melody tier,空 seq)。
pub fn build_word(p: &Program, text: &str) -> Result<Word, DslError> {
    let text: String = text.trim().nfc().collect();
    // 貪婪最長符號匹配
    let mut syms = Vec::new();
    let mut rest = text.as_str();
    'outer: while !rest.is_empty() {
        // 由長到短嘗試已宣告符號(數量小,線性掃描即可)
        let mut best: Option<(&str, usize)> = None;
        for cand in all_symbol_names(p) {
            if rest.starts_with(cand) && best.map_or(true, |(_, l)| cand.len() > l) {
                best = Some((cand, cand.len()));
            }
        }
        match best {
            Some((name, len)) => {
                syms.push(name.to_owned());
                rest = &rest[len..];
                continue 'outer;
            }
            None => {
                return Err(DslError::UnknownSegment {
                    word: text.clone(),
                    at: text.len() - rest.len(),
                })
            }
        }
    }

    let vowels: Vec<&str> = p
        .classes
        .get("vowel")
        .map(|ids| {
            ids.iter()
                .filter_map(|&s| p.env.syms.resolve(s))
                .collect()
        })
        .unwrap_or_default();

    let mut w = Word::new();
    let mut syl_start: u32 = 0;
    let mut last_nucleus: Option<u32> = None;
    for (i, name) in syms.iter().enumerate() {
        let i = i as u32;
        let sym = resolve_symbol(p, name)?;
        let feats = p.env.inv.feats_of(sym).unwrap_or_default();
        w.skeleton.push(Seg::new(sym, feats));
        if vowels.contains(&name.as_str()) {
            if let Some(pn) = last_nucleus {
                // 最大 onset:核心間輔音串全歸下一音節 → 界在前核心之後
                w.prosody.syllables.push(Span::new(syl_start, pn + 1));
                syl_start = pn + 1;
            }
            w.prosody.moras.push(Span::new(i, i + 1));
            last_nucleus = Some(i);
        }
    }
    let n = w.skeleton.len() as u32;
    if n > 0 {
        w.prosody.syllables.push(Span::new(syl_start, n)); // 末音節(含詞尾 coda)
    }
    if last_nucleus.is_none() && n > 0 {
        return Err(DslError::NoNucleus { word: text });
    }
    for t in &p.tiers {
        w.melodies.push(t.clone());
    }
    Ok(w)
}

fn all_symbol_names<'a>(p: &'a Program) -> impl Iterator<Item = &'a str> {
    // Inventory 未存名稱;由 interner 反查(宣告過的 Symbol 必已 intern)
    (0..p.env.syms.len() as u32).filter_map(|i| {
        let sym = conlang_core::repr::intern::SymId(i);
        p.env.inv.feats_of(sym)?; // 只取音素庫內的符號
        p.env.syms.resolve(sym)
    })
}

fn resolve_symbol(
    p: &Program,
    name: &str,
) -> Result<conlang_core::repr::intern::SymId, DslError> {
    (0..p.env.syms.len() as u32)
        .map(conlang_core::repr::intern::SymId)
        .find(|&s| p.env.syms.resolve(s) == Some(name))
        .ok_or_else(|| DslError::UnknownSegment {
            word: name.to_owned(),
            at: 0,
        })
}
