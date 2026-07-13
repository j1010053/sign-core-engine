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

    // 核心類:Parse 宣告優先(D24 子集),否則退回 `Class vowel` 暫定行為
    let nucleus_ids: Vec<conlang_core::repr::intern::SymId> = p
        .parse_cfg
        .as_ref()
        .filter(|c| !c.nucleus.is_empty())
        .map(|c| c.nucleus.clone())
        .or_else(|| p.classes.get("vowel").cloned())
        .unwrap_or_default();

    let mut w = Word::new();
    let mut seg_ids = Vec::new();
    for name in &syms {
        let sym = resolve_symbol(p, name)?;
        let feats = p.env.inv.feats_of(sym).unwrap_or_default();
        seg_ids.push(sym);
        w.skeleton.push(Seg::new(sym, feats));
    }
    let n = w.skeleton.len() as u32;
    let nuclei: Vec<u32> = (0..n)
        .filter(|&i| nucleus_ids.contains(&seg_ids[i as usize]))
        .collect();
    if nuclei.is_empty() && n > 0 {
        return Err(DslError::NoNucleus { word: text });
    }

    // 音節界:核心間輔音串——有 Parse onset 類(`@X?` = 至多一)時僅末一個
    // (且屬該類)歸下一音節 onset,其餘為前音節 coda;無宣告 = 全歸下一音節(舊行為)。
    let onset_take = |gap_last: u32| -> u32 {
        match p.parse_cfg.as_ref().and_then(|c| c.onset.as_ref()) {
            None => u32::MAX, // 全部
            Some(cls) => {
                if cls.contains(&seg_ids[gap_last as usize]) {
                    1
                } else {
                    0
                }
            }
        }
    };
    let mut syl_start: u32 = 0;
    for pair in nuclei.windows(2) {
        let (prev, next) = (pair[0], pair[1]);
        let gap_len = next - prev - 1;
        let take = if gap_len == 0 { 0 } else { onset_take(next - 1).min(gap_len) };
        let boundary = next - take;
        w.prosody.syllables.push(Span::new(syl_start, boundary));
        syl_start = boundary;
    }
    if n > 0 {
        w.prosody.syllables.push(Span::new(syl_start, n));
    }

    // 莫拉:核心各一;WBP(Parse mora 第二擇一)時,coda 類音段各自帶莫拉
    let wbp = p.parse_cfg.as_ref().and_then(|c| c.wbp_coda.as_ref());
    for i in 0..n {
        if nuclei.contains(&i) {
            w.prosody.moras.push(Span::new(i, i + 1));
        } else if let Some(cls) = wbp {
            // coda 判定:屬 WBP 類、且在其音節內位於某核心之後
            let in_syl_after_nucleus = w
                .prosody
                .syllables
                .iter()
                .any(|s| s.contains_idx(i) && nuclei.iter().any(|&nu| s.contains_idx(nu) && nu < i));
            if cls.contains(&seg_ids[i as usize]) && in_syl_after_nucleus {
                w.prosody.moras.push(Span::new(i, i + 1));
            }
        }
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
