//! 整合測試:《語法規格 v0.3》範例 8.1 / 8.4 的**表徵狀態序列**,以六原語 + 生命週期驅動。
//!
//! M0 步驟 1 曾以手動欄位操作驗證「repr 足以承載每個狀態」;本檔為**步驟 2 出口**——
//! 旋律/韻律的狀態轉換全部改為 `primitives::*` 建構 Action、`lifecycle::run` 套用
//! (執行語意 §1 步驟 3–5)。音段層(骨架)改動不屬六原語(I9):8.1 的 devoicing、
//! 8.4 的 coda 脫落及其結構 fallout(跨層連鎖重編留步驟 5,I10)仍為直接操作,逐處註明。

use conlang_core::lifecycle::{run, validate, VerbClass};
use conlang_core::primitives::{associate, dominate, insert_floating};
use conlang_core::repr::{
    notation, AnchorRef, Env, FeatBits, InvariantIssue, Level, MelodyTier, Seg, Severity, Span,
    Word,
};

/// 8.1 tonogenesis:*ba → insert 浮游 (L)@0 → dock(associate)→ L~μ0 → 音段清化 b>p。
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
    assert!(validate(&w).is_empty());
    let s0 = w.snapshot();

    // 狀態 1:tonogenesis 讀到 onset [+voice](唯讀跨層條件,D1)→ 旋律層 insert 浮游 L
    assert!(w.skeleton[0].feats.contains(voiced));
    let out = run(&w, &[insert_floating(tone, None, vl)], VerbClass::Melodic).unwrap();
    let w = out.word;
    assert!(out.issues.is_empty());
    assert!(!out.reparse_pending);
    assert_eq!(
        notation::render_tier(w.tier(tone).unwrap(), &env.vals),
        "(L)@0"
    );

    // 狀態 2:dock(= 條件 associate)→ L~μ0
    let out = run(
        &w,
        &[associate(tone, 0, AnchorRef::new(Level::Mora, 0))],
        VerbClass::Melodic,
    )
    .unwrap();
    let w = out.word;
    assert!(out.issues.is_empty());
    assert_eq!(
        notation::render_tier(w.tier(tone).unwrap(), &env.vals),
        "L~μ0"
    );

    // 狀態 3:devoicing b>p——音段層規則,非六原語(I9);旋律層完全不動 = 對立已轉入聲調
    let mut w = w;
    w.skeleton[0] = Seg::new(p, FeatBits::EMPTY);
    assert_eq!(notation::render_skeleton(&w, &env.syms), "p a");
    assert_eq!(
        notation::render_tier(w.tier(tone).unwrap(), &env.vals),
        "L~μ0"
    );
    assert!(validate(&w).is_empty());

    // 快照獨立(I1):狀態 0 不受後續影響
    assert_eq!(notation::render_skeleton(&s0, &env.syms), "b a");
    assert_eq!(s0.tier(tone).unwrap().seq.len(), 0);
}

/// 8.4 補償性延長:ak(重音節)→ coda 脫落留空莫拉(info)→ dominate 修復 → 長元音。
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
    assert!(validate(&w).is_empty());

    // 狀態 1:coda 脫落——音段層刪除非六原語(I9);其結構 fallout(音節縮、莫拉成空節點、
    // stale 標記)屬跨層連鎖重編,步驟 2 手動模擬、步驟 5 由 commit/lazy-reparse 接手(I10)。
    w.skeleton.pop();
    w.prosody.syllables[0].hi = 1; // 音節同步縮(模擬 commit 重編)
    w.prosody.moras[1] = Span::new(1, 1); // 空莫拉(mora keep-empty)
    w.stale.mark(Level::Mora);
    let issues = validate(&w);
    let empties: Vec<&InvariantIssue> = issues
        .iter()
        .filter(|i| matches!(i, InvariantIssue::EmptyProsodyNode { .. }))
        .collect();
    assert_eq!(empties.len(), 1);
    assert_eq!(empties[0].severity(), Severity::Info); // 暫態病理,非 error
    assert_eq!(notation::render_prosody(&w), "σ0[0,1) μ0[0,1) μ1[1,1)∅");

    // 狀態 2:dominate <mora>&empty -> @vowel leftward(repair 原語)
    assert!(w.stale.is_stale(Level::Mora)); // repair 看 stale 舊結構(A3)
    let out = run(&w, &[dominate(Level::Mora, 1, 0)], VerbClass::Repair).unwrap();
    let w = out.word;
    assert!(!out.reparse_pending, "repair 不觸發 implicit reparse(A3)");
    assert!(w.stale.is_stale(Level::Mora), "repair 不清 stale(A3)");
    // 兩莫拉共享同一音段 = 長元音(I8:莫拉重疊合法);空節點已被修復
    assert!(
        out.issues.is_empty(),
        "long vowel state must be clean: {:?}",
        out.issues
    );
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
