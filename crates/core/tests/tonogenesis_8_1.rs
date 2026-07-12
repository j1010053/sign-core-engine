//! M0 步驟 3 出口:《語法規格 v0.3》§12 範例 8.1(tonogenesis)以**規則序列**推導,
//! 每 commit 一快照(insta;執行語意 §4「每條規則一個 commit = 一個天然快照點」)。
//!
//! 規則檔(docs/02 §12):
//! ```text
//! Melody tone {H, M, L} anchor mora
//! tonogenesis:
//!     insert H floating near mora / onset&[-voice] _
//!     insert L floating near mora / onset&[+voice] _
//! dock-tone:    dock tone&floating strategy nearest
//! devoicing:    [+voice]&onset => [-voice]
//! fill-default: fill tone Ø => M within pword
//! ocp-cleanup:  merge adjacent-equal
//! ```
//! tonogenesis 的兩個平行子項共享同一次 Parallel Match(B5):兩者的 Action 合為一個
//! commit。devoicing 是音段層規則,非六原語(I9)——本測試以直接骨架操作模擬,
//! 音段層規則機制隨 DSL(步驟 4+)引入。
//!
//! 四詞驗證三分 H/M/Ø 與對立轉移:*pa→pá(H)、*ba→pà(L,對立入調)、
//! *baba→pàpà(相鄰 L 合併=延展)、*a→á(fill 補 M)。

use conlang_core::lifecycle::{run, Outcome, VerbClass};
use conlang_core::repr::{notation, Env, FeatBits, Level, MelodyTier, Seg, Span, SymId, Word};
use conlang_core::strategy::Strategy;
use conlang_core::verbs::{self, InsertProbe};

struct Fixture {
    env: Env,
    tone: SymId,
    voiced: FeatBits,
    voiceless: FeatBits,
    p: SymId,
    b: SymId,
    a: SymId,
}

impl Fixture {
    fn new() -> Fixture {
        let mut env = Env::new();
        let tone = env.syms.intern("tone");
        let p = env.syms.intern("p");
        let b = env.syms.intern("b");
        let a = env.syms.intern("a");
        let vplus = env.feats.bits("voice", "+").unwrap_or_else(|| {
            let bit = env.feats.register("voice", "+").unwrap();
            let mut fb = FeatBits::EMPTY;
            fb.insert(bit);
            fb
        });
        let vminus = {
            let bit = env.feats.register("voice", "-").unwrap();
            let mut fb = FeatBits::EMPTY;
            fb.insert(bit);
            fb
        };
        Fixture {
            env,
            tone,
            voiced: vplus,
            voiceless: vminus,
            p,
            b,
            a,
        }
    }

    /// CV 音節序列(onset 音素 × 音節數);onset 不入莫拉(I8)。
    fn word(&mut self, syllables: &[SymId]) -> Word {
        let mut w = Word::new();
        for (k, &onset) in syllables.iter().enumerate() {
            let base = (k * 2) as u32;
            let feats = if onset == self.b {
                self.voiced
            } else {
                self.voiceless
            };
            w.skeleton.push(Seg::new(onset, feats));
            w.skeleton.push(Seg::new(self.a, FeatBits::EMPTY));
            w.prosody.syllables.push(Span::new(base, base + 2));
            w.prosody.moras.push(Span::new(base + 1, base + 2));
        }
        self.push_tone_tier(&mut w);
        w
    }

    /// 純元音詞(單音節、無 onset)。
    fn vowel_word(&mut self) -> Word {
        let mut w = Word::new();
        w.skeleton.push(Seg::new(self.a, FeatBits::EMPTY));
        w.prosody.syllables.push(Span::new(0, 1));
        w.prosody.moras.push(Span::new(0, 1));
        self.push_tone_tier(&mut w);
        w
    }

    fn push_tone_tier(&mut self, w: &mut Word) {
        let h = self.env.vals.intern("H");
        let m = self.env.vals.intern("M");
        let l = self.env.vals.intern("L");
        w.melodies
            .push(MelodyTier::new(self.tone, Level::Mora, vec![h, m, l]));
    }

    fn render(&self, w: &Word) -> String {
        format!(
            "{}  |  {}",
            notation::render_skeleton(w, &self.env.syms),
            notation::render_tier(w.tier(self.tone).unwrap(), &self.env.vals)
        )
    }
}

/// 跑一條規則並把 commit 後狀態寫入 transcript;斷言無 error 級問題。
fn step(fx: &Fixture, transcript: &mut String, name: &str, out: Outcome) -> Word {
    assert!(
        !conlang_core::lifecycle::has_error(&out.issues),
        "rule {name}: {:?}",
        out.issues
    );
    transcript.push_str(&format!("  {:<13} {}\n", name, fx.render(&out.word)));
    out.word
}

/// 8.1 全規則序列;回傳最終詞。
fn derive_8_1(fx: &mut Fixture, transcript: &mut String, label: &str, w: Word) -> Word {
    let h = fx.env.vals.intern("H");
    let m = fx.env.vals.intern("M");
    let l = fx.env.vals.intern("L");
    transcript.push_str(&format!("{label}\n  {:<13} {}\n", "input", fx.render(&w)));

    // tonogenesis:兩個平行子項共享同一次 Parallel Match(B5)→ 合為一個 commit
    let mut acts = verbs::insert_floating_near(&w, fx.tone, h, fx.voiceless, InsertProbe::Onset).unwrap();
    acts.extend(verbs::insert_floating_near(&w, fx.tone, l, fx.voiced, InsertProbe::Onset).unwrap());
    let w = step(
        fx,
        transcript,
        "tonogenesis",
        run(&w, &acts, VerbClass::Query).unwrap(),
    );

    // dock-tone
    let acts = verbs::dock(&w, fx.tone, Strategy::NEAREST).unwrap();
    let w = step(
        fx,
        transcript,
        "dock-tone",
        run(&w, &acts, VerbClass::Query).unwrap(),
    );

    // devoicing(音段層規則,非六原語;I9):[+voice]&onset => [-voice],b > p
    let mut w = w;
    for seg in w.skeleton.iter_mut() {
        if seg.feats.contains(fx.voiced) {
            *seg = Seg::new(fx.p, fx.voiceless);
        }
    }
    transcript.push_str(&format!("  {:<13} {}\n", "devoicing", fx.render(&w)));

    // fill-default
    let acts = verbs::fill(&w, fx.tone, m).unwrap();
    let w = step(
        fx,
        transcript,
        "fill-default",
        run(&w, &acts, VerbClass::Query).unwrap(),
    );

    // ocp-cleanup
    let acts = verbs::merge_adjacent_equal(&w, fx.tone).unwrap();
    let w = step(
        fx,
        transcript,
        "ocp-cleanup",
        run(&w, &acts, VerbClass::Melodic).unwrap(),
    );
    transcript.push('\n');
    w
}

#[test]
fn tonogenesis_8_1_rule_sequence() {
    let mut fx = Fixture::new();
    let mut transcript = String::new();

    // *pa → pá(H):清 onset 生 H
    let (p, b) = (fx.p, fx.b);
    let w = fx.word(&[p]);
    let w_pa = derive_8_1(&mut fx, &mut transcript, "*pa", w);

    // *ba → pà(L):濁 onset 生 L;devoicing 後對立轉入聲調
    let w = fx.word(&[b]);
    let w_ba = derive_8_1(&mut fx, &mut transcript, "*ba", w);

    // *baba → pàpà:兩個 L 各自著陸(I11 投影),OCP 合併為延展
    let w = fx.word(&[b, b]);
    let w_baba = derive_8_1(&mut fx, &mut transcript, "*baba", w);

    // *a → á(M):無 onset 無調,fill 補預設 M(三分 H/M/Ø 的 Ø→M)
    let w = fx.vowel_word();
    let w_a = derive_8_1(&mut fx, &mut transcript, "*a", w);

    insta::assert_snapshot!("tonogenesis_8_1", transcript);

    // 關鍵語意錨點(快照之外的硬斷言):
    // 對立轉移:*pa 與 *ba 的骨架已同為 "p a",分辨只剩聲調 H vs L
    assert_eq!(
        notation::render_skeleton(&w_pa, &fx.env.syms),
        notation::render_skeleton(&w_ba, &fx.env.syms)
    );
    let h = fx.env.vals.intern("H");
    let l = fx.env.vals.intern("L");
    let m = fx.env.vals.intern("M");
    assert_eq!(w_pa.tier(fx.tone).unwrap().seq[0].val, h);
    assert_eq!(w_ba.tier(fx.tone).unwrap().seq[0].val, l);
    // *baba:相鄰 L 已合併為單一延展自體段(D22 對照:fill 才是逐 Ø 獨立)
    let t = w_baba.tier(fx.tone).unwrap();
    assert_eq!(t.seq.len(), 1);
    assert!(t.seq[0].is_spread());
    // *a:fill 補 M
    assert_eq!(w_a.tier(fx.tone).unwrap().seq[0].val, m);
}
