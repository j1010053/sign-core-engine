//! 整合測試:以 pub API 手動建構《語法規格 v0.3》範例 8.1 / 8.4 的**表徵狀態序列**。
//! M0 步驟 1 只驗證「repr 足以承載每個狀態」;規則引擎(步驟 2+)之後把手動步驟換成原語呼叫。

use conlang_core::repr::{
    check_word, notation, Autoseg, AnchorRef, Env, FeatBits, InvariantIssue, Level, MelodyTier,
    Seg, Severity, Span, Word,
};

/// 8.1 tonogenesis:*ba → (L)@0 浮游 → dock → L~μ0 → 音段清化 b>p → 表徵 pà 之前狀態。
#[test]
fn tonogenesis_state_sequence() {
    let mut env = Env::new();
    let b = env.syms.intern("b");
    let p = env.syms.intern("p");
    let a = env.syms.intern("a");
    let tone = env.syms.intern("tone");
    let vh = env.vals.intern("H");
    let vm = env.vals.intern("M");
    let vl = env.vals.intern("L");
    let voiced = {
        let bit = env.feats.register("voice", "+").unwrap();
        let mut fb = FeatBits::EMPTY;
        fb.insert(bit);
        fb
    };

    // 狀態 0:*ba,無調
    let mut w = Word::new();
    w.skeleton.push(Seg::new(b, voiced));
    w.skeleton.push(Seg::new(a, FeatBits::EMPTY));
    w.prosody.syllables.push(Span::new(0, 2));
    w.prosody.moras.push(Span::new(1, 2)); // onset 不入莫拉(I8)
    w.melodies
        .push(MelodyTier::new(tone, Level::Mora, vec![vh, vm, vl]));
    assert!(check_word(&w).is_empty());
    let s0 = w.snapshot();

    // 狀態 1:tonogenesis 讀到 onset [+voice] → 旋律層 insert 浮游 L
    assert!(w.skeleton[0].feats.contains(voiced)); // 唯讀跨層條件
    w.tier_mut(tone).unwrap().seq.push(Autoseg::floating(vl));
    assert_eq!(
        notation::render_tier(w.tier(tone).unwrap(), &env.vals),
        "(L)@0"
    );
    assert!(check_word(&w).is_empty());

    // 狀態 2:dock floating → L~μ0
    w.tier_mut(tone).unwrap().seq[0]
        .links
        .push(AnchorRef::new(Level::Mora, 0));
    assert_eq!(
        notation::render_tier(w.tier(tone).unwrap(), &env.vals),
        "L~μ0"
    );
    assert!(check_word(&w).is_empty());

    // 狀態 3:devoicing b>p(音段層;旋律層完全不動 = 對立已轉入聲調)
    w.skeleton[0] = Seg::new(p, FeatBits::EMPTY);
    assert_eq!(notation::render_skeleton(&w, &env.syms), "p a");
    assert_eq!(
        notation::render_tier(w.tier(tone).unwrap(), &env.vals),
        "L~μ0"
    );
    assert!(check_word(&w).is_empty());

    // 快照獨立:狀態 0 不受後續影響
    assert_eq!(notation::render_skeleton(&s0, &env.syms), "b a");
    assert_eq!(s0.tier(tone).unwrap().seq.len(), 0);
}

/// 8.4 補償性延長:ak(重音節)→ coda 脫落留空莫拉(info)→ dominate 修復 → 長元音(兩莫拉共享 a)。
#[test]
fn compensatory_lengthening_state_sequence() {
    let mut env = Env::new();
    let a = env.syms.intern("a");
    let k = env.syms.intern("k");

    // 狀態 0:a k;μ0=核心、μ1=coda(weight-by-position);σ=[0,2)
    let mut w = Word::new();
    w.skeleton.push(Seg::new(a, FeatBits::EMPTY));
    w.skeleton.push(Seg::new(k, FeatBits::EMPTY));
    w.prosody.syllables.push(Span::new(0, 2));
    w.prosody.moras.push(Span::new(0, 1));
    w.prosody.moras.push(Span::new(1, 2));
    assert!(check_word(&w).is_empty());

    // 狀態 1:coda 脫落(音段刪除;mora keep-empty → μ1 成空節點;lazy:結構標 stale)
    w.skeleton.pop();
    w.prosody.syllables[0].hi = 1; // 音節同步縮(模擬 commit 重編)
    w.prosody.moras[1] = Span::new(1, 1); // 空莫拉
    w.stale.mark(Level::Mora);
    let issues = check_word(&w);
    let empties: Vec<&InvariantIssue> = issues
        .iter()
        .filter(|i| matches!(i, InvariantIssue::EmptyProsodyNode { .. }))
        .collect();
    assert_eq!(empties.len(), 1);
    assert_eq!(empties[0].severity(), Severity::Info); // 暫態病理,非 error
    assert_eq!(notation::render_prosody(&w), "σ0[0,1) μ0[0,1) μ1[1,1)∅");

    // 狀態 2:dominate <mora>&empty -> @vowel leftward(repair,不觸發 reparse)
    assert!(w.stale.is_stale(Level::Mora)); // repair 看 stale 結構(A3)
    w.prosody.moras[1] = Span::new(0, 1); // 空莫拉向下重掛到 a
    // 兩莫拉共享同一音段 = 長元音(I8:莫拉重疊合法)
    let issues2 = check_word(&w);
    assert!(issues2.is_empty(), "long vowel state must be clean: {:?}", issues2);
    assert_eq!(notation::render_prosody(&w), "σ0[0,1) μ0[0,1) μ1[0,1)");
    // a 現有兩個莫拉 → spell-out 時渲染為 aː(步驟 6 的事)
    let bearing: usize = w
        .prosody
        .moras
        .iter()
        .filter(|m| m.contains_idx(0))
        .count();
    assert_eq!(bearing, 2);
}
