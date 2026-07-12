//! verbs:具名動詞第一批(M0 步驟 3)——insert / dock / fill / merge。
//!
//! **全部組合六原語,不得另闢狀態**(M0 §1.1 原則 2):
//! - `insert_floating_near` = 環境匹配 + 原語 insert(浮游)
//! - `dock` = **條件 associate**(候選挑選一律經 `strategy::resolve`,D28)
//! - `fill` = **逐 Ø 的 insert+associate**(每個 Ø 錨點一個獨立自體段,D22)
//! - `merge_adjacent_equal` = **delete+associate**(OCP 合併,以前狀態一次判斷)
//!
//! 每個動詞實作執行語意 §1 的步驟 1–2:在**凍結** `Word` 上 Parallel Match、
//! 產出 `Vec<Action>`;步驟 3–5 由呼叫端交 `lifecycle::run`。皆為 parallel 語意
//! (執行語意 §2);無適用目標 = 空 Action 清單(B8 noop)。
//!
//! 環境選擇子在本步驟為最小內容定址(8.1 所需的 `onset&[特徵]`);
//! 一般 selector 由 DSL AST 驅動(步驟 4+),屆時本模組函數改吃 AST 參數。

use crate::lifecycle::{Action, EngineError};
use crate::primitives;
use crate::repr::feature::FeatBits;
use crate::repr::intern::{SymId, ValId};
use crate::repr::inventory::Inventory;
use crate::repr::melody::MelodyTier;
use crate::repr::prosody::AnchorRef;
use crate::repr::word::Word;
use crate::strategy::{self, Strategy};

// ── 唯讀導航(沿支配/聯結的條件讀取,D1 純度:只讀不寫)──

fn tier_of<'w>(w: &'w Word, tier: SymId) -> Result<&'w MelodyTier, EngineError> {
    w.tier(tier).ok_or(EngineError::TierNotFound(tier))
}

/// 音節的 onset 音段索引(I8:onset = 音節內不入任何莫拉的前緣音段)。
fn onset_segs(w: &Word, syl_idx: usize) -> Vec<u32> {
    let syl = &w.prosody.syllables[syl_idx];
    (syl.lo..syl.hi)
        .take_while(|&i| !w.prosody.moras.iter().any(|m| m.contains_idx(i)))
        .collect()
}

/// 音節內第一個莫拉的錨點索引。
fn first_mora_in(w: &Word, syl_idx: usize) -> Option<u32> {
    let syl = &w.prosody.syllables[syl_idx];
    w.prosody
        .moras
        .iter()
        .position(|m| !m.is_empty_node() && m.within(syl))
        .map(|i| i as u32)
}

/// 該 tier 錨定層上的 Ø 錨點(無任何承載者)。
fn empty_anchors(w: &Word, tier: &MelodyTier) -> Vec<u32> {
    let n = w.anchor_count(tier.anchor) as u32;
    (0..n)
        .filter(|&i| tier.bearers_of(AnchorRef::new(tier.anchor, i)).is_empty())
        .collect()
}

/// 浮游者的**原位投影**(I11):左鄰最近已聯結者的最大錨點 +1;
/// 無則右鄰最近已聯結者的最小錨點 −1(飽和);全浮游時 seq 索引即投影。
fn projected_anchor(tier: &MelodyTier, i: usize) -> u32 {
    for j in (0..i).rev() {
        if let Some(a) = tier.seq[j].links.iter().map(|l| l.index).max() {
            return a + 1;
        }
    }
    for j in (i + 1)..tier.seq.len() {
        if let Some(a) = tier.seq[j].links.iter().map(|l| l.index).min() {
            return a.saturating_sub(1);
        }
    }
    i as u32
}

/// 維持 seq 依錨點次序的插入位置:插在第一個「最小錨點 > anchor」的已聯結者之前;
/// 無此者則追加尾端(浮游者位置中立)。避免 fill/insert 產生跨越邊(NCC)。
fn insert_pos(tier: &MelodyTier, anchor: u32) -> Option<usize> {
    tier.seq.iter().position(|a| {
        a.links
            .iter()
            .map(|l| l.index)
            .min()
            .is_some_and(|m| m > anchor)
    })
}

// ── 動詞 ──

/// `insert <值> floating near mora / onset&[test] _`(8.1 tonogenesis 的子項)。
/// 對每個「onset 含匹配音段」的音節,在其第一個莫拉附近 insert 浮游 `val`。
/// 匹配 = 自然類超集測試(`seg.feats ⊇ test`)。VerbClass::Query(讀韻律結構)。
pub fn insert_floating_near(
    w: &Word,
    tier: SymId,
    val: ValId,
    onset_test: FeatBits,
) -> Result<Vec<Action>, EngineError> {
    let t = tier_of(w, tier)?;
    let mut out = Vec::new();
    for syl_idx in 0..w.prosody.syllables.len() {
        let onset_hit = onset_segs(w, syl_idx)
            .iter()
            .any(|&i| w.skeleton[i as usize].feats.contains(onset_test));
        if !onset_hit {
            continue;
        }
        let Some(mora) = first_mora_in(w, syl_idx) else {
            continue; // 無莫拉可停靠:B8 noop
        };
        out.push(primitives::insert(
            tier,
            insert_pos(t, mora),
            val,
            Default::default(),
        ));
    }
    Ok(out)
}

/// `dock <tier>&floating strategy …` = 條件 associate。
/// 一次媒合所有浮游者(parallel):每個浮游者以**原位投影**(I11)為參考,
/// 經 `strategy::resolve` 自 Ø 錨點中挑一個;多者同拍選中同一錨點 → 全部接上(D27)。
/// VerbClass::Query。
pub fn dock(w: &Word, tier: SymId, s: Strategy) -> Result<Vec<Action>, EngineError> {
    let t = tier_of(w, tier)?;
    let cands = empty_anchors(w, t);
    let mut out = Vec::new();
    for i in t.floating_indices() {
        let reference = projected_anchor(t, i);
        if let Some(c) = strategy::resolve(&cands, reference, s) {
            out.push(primitives::associate(tier, i, AnchorRef::new(t.anchor, c)));
        }
    }
    Ok(out)
}

/// `fill <tier> Ø => <值> [within pword]` = 逐 Ø 的 insert+associate(D22)。
/// 對每個 Ø 錨點建立**獨立**自體段(生成即掛),不延展。
/// M0 的 Word 即單一 pword,`within pword` 為全詞。VerbClass::Query。
pub fn fill(w: &Word, tier: SymId, val: ValId) -> Result<Vec<Action>, EngineError> {
    let t = tier_of(w, tier)?;
    Ok(empty_anchors(w, t)
        .into_iter()
        .map(|c| {
            let links = std::iter::once(AnchorRef::new(t.anchor, c)).collect();
            primitives::insert(tier, insert_pos(t, c), val, links)
        })
        .collect())
}

/// 音段 rewrite 規則的環境項(`/ pre _ post`;皆可省略)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegPat {
    /// 特徵矩陣(自然類超集測試)。
    Feats(FeatBits),
    /// 詞界 `#`。
    Boundary,
}

/// 音段 rewrite 規則的環境(骨架相鄰一格;語法貼合 Lexurgy 的 `/ C _ D`)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SegEnv {
    pub pre: Option<SegPat>,
    pub post: Option<SegPat>,
}

impl SegEnv {
    fn matches(&self, w: &Word, idx: usize) -> bool {
        let side = |pat: Option<SegPat>, neighbor: Option<&crate::repr::word::Seg>| match pat {
            None => true,
            Some(SegPat::Boundary) => neighbor.is_none(),
            Some(SegPat::Feats(f)) => neighbor.is_some_and(|s| s.feats.contains(f)),
        };
        side(self.pre, idx.checked_sub(1).and_then(|i| w.skeleton.get(i)))
            && side(self.post, w.skeleton.get(idx + 1))
    }
}

/// 音段是否為 onset(I8:落在某音節內、不入任何莫拉的前緣音段)。
fn is_onset(w: &Word, idx: u32) -> bool {
    (0..w.prosody.syllables.len()).any(|s| onset_segs(w, s).contains(&idx))
}

/// 音段層 rewrite 規則:`[match] (&onset) => [subs] / pre _ post`(I12;語法貼合 Lexurgy)。
/// 對每個匹配音段:逐特徵欄位 `set_field` 改寫 → Inventory 反查新符號(無對應 = error)
/// → `SegRewrite`。整段替換、長度不變;非六原語,不供動詞組合。
/// `subs` = (欄位遮罩, 新值位) 序列,由呼叫端(DSL lowering)自輸出矩陣求得。
/// VerbClass::Query(onset 述語讀韻律結構)。
pub fn rewrite(
    w: &Word,
    inv: &Inventory,
    match_feats: FeatBits,
    require_onset: bool,
    subs: &[(FeatBits, FeatBits)],
    env: SegEnv,
) -> Result<Vec<Action>, EngineError> {
    let mut out = Vec::new();
    for (idx, seg) in w.skeleton.iter().enumerate() {
        if !seg.feats.contains(match_feats) {
            continue;
        }
        if require_onset && !is_onset(w, idx as u32) {
            continue;
        }
        if !env.matches(w, idx) {
            continue;
        }
        let mut feats = seg.feats;
        for &(mask, value) in subs {
            feats = feats.set_field(mask, value);
        }
        let sym = inv
            .sym_for(feats)
            .ok_or(EngineError::NoSymbolForBundle { idx })?;
        out.push(Action::SegRewrite { idx, sym, feats });
    }
    Ok(out)
}

/// `merge adjacent-equal` = delete+associate(OCP 合併)。
/// 以前狀態一次判斷(parallel):seq 上相鄰同值的極大連段,保留首者、
/// 其餘刪除並把其聯結併給首者(冪等:重複邊由 commit 去重)。VerbClass::Melodic。
pub fn merge_adjacent_equal(w: &Word, tier: SymId) -> Result<Vec<Action>, EngineError> {
    let t = tier_of(w, tier)?;
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.seq.len() {
        let mut j = i + 1;
        while j < t.seq.len() && t.seq[j].val == t.seq[i].val {
            for &link in &t.seq[j].links {
                out.push(primitives::associate(tier, i, link));
            }
            out.push(primitives::delete(tier, j));
            j += 1;
        }
        i = j;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::commit;
    use crate::repr::melody::Autoseg;
    use crate::repr::prosody::{Level, Span};
    use crate::repr::word::Seg;

    const TONE: SymId = SymId(100);
    const H: ValId = ValId(0);
    const M: ValId = ValId(2);

    /// CV.CV 詞(音段 0,2=onset、1,3=核心;μ0=[1,2) μ1=[3,4))+ tone tier。
    fn cvcv(onset_feats: [FeatBits; 2]) -> Word {
        let mut w = Word::new();
        for i in 0..4u32 {
            let feats = if i % 2 == 0 {
                onset_feats[(i / 2) as usize]
            } else {
                FeatBits::EMPTY
            };
            w.skeleton.push(Seg::new(SymId(i), feats));
        }
        w.prosody.syllables.push(Span::new(0, 2));
        w.prosody.syllables.push(Span::new(2, 4));
        w.prosody.moras.push(Span::new(1, 2));
        w.prosody.moras.push(Span::new(3, 4));
        w.melodies.push(MelodyTier::new(
            TONE,
            Level::Mora,
            vec![ValId(0), ValId(1), ValId(2)],
        ));
        w
    }

    fn voiced() -> FeatBits {
        FeatBits(1) // 測試約定:bit0 = (voice,+)
    }

    #[test]
    fn insert_matches_onset_class_per_syllable() {
        // 兩音節 onset 皆 [+voice] → 兩個浮游;皆不匹配 → noop(B8)
        let w = cvcv([voiced(), voiced()]);
        let acts = insert_floating_near(&w, TONE, H, voiced()).unwrap();
        assert_eq!(acts.len(), 2);
        let w2 = commit(&w, &acts).unwrap();
        assert_eq!(w2.tier(TONE).unwrap().floating_indices().len(), 2);

        let none = insert_floating_near(&w, TONE, H, FeatBits(2)).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn dock_uses_projection_i11_and_colands_d27() {
        // 兩浮游、兩 Ø 錨點:投影 0→μ0、1→μ1,各自著陸
        let mut w = cvcv([voiced(), voiced()]);
        w.tier_mut(TONE).unwrap().seq.push(Autoseg::floating(H));
        w.tier_mut(TONE).unwrap().seq.push(Autoseg::floating(M));
        let acts = dock(&w, TONE, Strategy::NEAREST).unwrap();
        let w2 = commit(&w, &acts).unwrap();
        let t = w2.tier(TONE).unwrap();
        assert!(t.seq[0].links.contains(&AnchorRef::new(Level::Mora, 0)));
        assert!(t.seq[1].links.contains(&AnchorRef::new(Level::Mora, 1)));

        // 單一 Ø 錨點、兩浮游 → 共同著陸(D27 全部接上)
        let mut w3 = cvcv([voiced(), voiced()]);
        w3.prosody.moras.pop();
        w3.prosody.syllables[1].hi = 3; // 縮到無第二莫拉的形(僅測著陸)
        w3.skeleton.pop();
        w3.tier_mut(TONE).unwrap().seq.push(Autoseg::floating(H));
        w3.tier_mut(TONE).unwrap().seq.push(Autoseg::floating(M));
        let acts3 = dock(&w3, TONE, Strategy::NEAREST).unwrap();
        let w4 = commit(&w3, &acts3).unwrap();
        let m0 = AnchorRef::new(Level::Mora, 0);
        assert_eq!(w4.tier(TONE).unwrap().bearers_of(m0).len(), 2); // 輪廓
    }

    #[test]
    fn fill_creates_independent_autosegs_d22() {
        let w = cvcv([FeatBits::EMPTY, FeatBits::EMPTY]);
        let acts = fill(&w, TONE, M).unwrap();
        assert_eq!(acts.len(), 2); // 每 Ø 一個
        let w2 = commit(&w, &acts).unwrap();
        let t = w2.tier(TONE).unwrap();
        assert_eq!(t.seq.len(), 2);
        assert!(t.seq.iter().all(|a| a.val == M && a.links.len() == 1)); // 獨立,非延展
    }

    #[test]
    fn rewrite_devoices_onsets_via_inventory_i12() {
        // b=[+voice](bit0)、p=[-voice](bit1);devoicing: [+voice]&onset => [-voice]
        let vplus = FeatBits(0b01);
        let vminus = FeatBits(0b10);
        let mask = FeatBits(0b11); // voice 欄位遮罩
        let mut inv = Inventory::new();
        inv.register(SymId(0), vplus); // 骨架偶數位 = onset b
        let p_sym = SymId(77);
        inv.register(p_sym, vminus);

        let w = cvcv([vplus, vplus]);
        let acts = rewrite(&w, &inv, vplus, true, &[(mask, vminus)], SegEnv::default()).unwrap();
        assert_eq!(acts.len(), 2); // 兩個濁 onset 皆改寫
        let w2 = commit(&w, &acts).unwrap();
        assert_eq!(w2.skeleton[0].sym, p_sym);
        assert_eq!(w2.skeleton[0].feats, vminus);
        assert_eq!(w2.skeleton[2].sym, p_sym);
        // 核心元音(非 onset)不動
        assert_eq!(w2.skeleton[1].sym, SymId(1));
        // 旋律/韻律完全不受影響(長度不變,I12)
        assert_eq!(w2.prosody, w.prosody);

        // 無對應符號 → error(Inventory 缺 [-voice] 時)
        let empty_inv = Inventory::new();
        let err = rewrite(&w, &empty_inv, vplus, true, &[(mask, vminus)], SegEnv::default());
        assert!(matches!(
            err.unwrap_err(),
            EngineError::NoSymbolForBundle { .. }
        ));
    }

    #[test]
    fn rewrite_env_boundary_and_neighbor() {
        // 只在詞首:/ # _
        let vplus = FeatBits(0b01);
        let vminus = FeatBits(0b10);
        let mask = FeatBits(0b11);
        let mut inv = Inventory::new();
        inv.register(SymId(50), vminus);
        let w = cvcv([vplus, vplus]);
        let env = SegEnv {
            pre: Some(SegPat::Boundary),
            post: None,
        };
        let acts = rewrite(&w, &inv, vplus, false, &[(mask, vminus)], env).unwrap();
        assert_eq!(acts.len(), 1); // 僅 idx 0(詞首);idx 2 前有音段
        assert!(matches!(acts[0], Action::SegRewrite { idx: 0, .. }));
    }

    #[test]
    fn merge_folds_adjacent_runs_into_spread() {
        // L~μ0 L~μ1 → L~μ0~μ1(delete+associate)
        let mut w = cvcv([FeatBits::EMPTY, FeatBits::EMPTY]);
        {
            let t = w.tier_mut(TONE).unwrap();
            t.seq.push(Autoseg::linked(
                ValId(1),
                vec![AnchorRef::new(Level::Mora, 0)],
            ));
            t.seq.push(Autoseg::linked(
                ValId(1),
                vec![AnchorRef::new(Level::Mora, 1)],
            ));
        }
        let acts = merge_adjacent_equal(&w, TONE).unwrap();
        let w2 = commit(&w, &acts).unwrap();
        let t = w2.tier(TONE).unwrap();
        assert_eq!(t.seq.len(), 1);
        assert!(t.seq[0].is_spread());
        // 不同值不合併
        let mut w3 = cvcv([FeatBits::EMPTY, FeatBits::EMPTY]);
        w3.tier_mut(TONE).unwrap().seq.push(Autoseg::floating(H));
        w3.tier_mut(TONE).unwrap().seq.push(Autoseg::floating(M));
        assert!(merge_adjacent_equal(&w3, TONE).unwrap().is_empty());
    }
}
