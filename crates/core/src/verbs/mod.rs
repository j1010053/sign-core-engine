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
    if let Some(o) = tier.seq[i].origin {
        return o; // 原位記憶優先(I11 v2)
    }
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

/// insert-near 的環境探測方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertProbe {
    /// `/ onset&[test] _`:錨點所屬音節的 onset 匹配(僅 mora 錨;8.1)。
    Onset,
    /// `/ [test] _`:錨點自身內容匹配(segment 錨=該音段;mora 錨=核心音段;8.2/8.5)。
    AnchorContent,
}

/// `insert <值> floating near <錨層> / <env> _`:對每個環境匹配的錨點,
/// insert 浮游 `val` 並寫入原位記憶(I11 v2)。VerbClass::Query。
pub fn insert_floating_near(
    w: &Word,
    tier: SymId,
    val: ValId,
    test: FeatBits,
    probe: InsertProbe,
) -> Result<Vec<Action>, EngineError> {
    let t = tier_of(w, tier)?;
    let level = t.anchor;
    let mut out = Vec::new();
    match probe {
        InsertProbe::Onset => {
            for syl_idx in 0..w.prosody.syllables.len() {
                let onset_hit = onset_segs(w, syl_idx)
                    .iter()
                    .any(|&i| w.skeleton[i as usize].feats.contains(test));
                if !onset_hit {
                    continue;
                }
                let Some(mora) = first_mora_in(w, syl_idx) else {
                    continue; // 無莫拉可停靠:B8 noop
                };
                out.push(primitives::insert_floating_at(
                    tier,
                    insert_pos(t, mora),
                    val,
                    mora,
                ));
            }
        }
        InsertProbe::AnchorContent => {
            let n = w.anchor_count(level) as u32;
            for a in 0..n {
                if anchor_matches(w, level, a, test) {
                    out.push(primitives::insert_floating_at(
                        tier,
                        insert_pos(t, a),
                        val,
                        a,
                    ));
                }
            }
        }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegPat {
    /// 特徵矩陣(自然類超集測試)。
    Feats(FeatBits),
    /// 類別(符號集,`@vowel`)。
    Class(Vec<SymId>),
    /// 詞界 `#`。
    Boundary,
    /// 音節界 `.`(相鄰位置跨音節或為詞緣)。
    SylBoundary,
}

/// 音段 rewrite 規則的環境(骨架相鄰一格;語法貼合 Lexurgy 的 `/ C _ D`)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SegEnv {
    pub pre: Option<SegPat>,
    pub post: Option<SegPat>,
}

/// idx_a 與 idx_a+1 之間是否為音節界(不同音節或詞緣)。
fn syl_boundary_after(w: &Word, idx: usize) -> bool {
    let a = idx as u32;
    let b = a + 1;
    if b >= w.skeleton.len() as u32 {
        return true; // 詞緣視同音節界
    }
    let syl_of = |i: u32| w.prosody.syllables.iter().position(|s| s.contains_idx(i));
    syl_of(a) != syl_of(b)
}

impl SegEnv {
    fn side_matches(w: &Word, pat: &SegPat, neighbor: Option<usize>) -> bool {
        match pat {
            SegPat::Boundary => neighbor.is_none(),
            SegPat::Feats(f) => neighbor
                .and_then(|i| w.skeleton.get(i))
                .is_some_and(|s| s.feats.contains(*f)),
            SegPat::Class(syms) => neighbor
                .and_then(|i| w.skeleton.get(i))
                .is_some_and(|s| syms.contains(&s.sym)),
            SegPat::SylBoundary => unreachable!("handled in matches()"),
        }
    }

    fn matches(&self, w: &Word, idx: usize) -> bool {
        let pre_ok = match &self.pre {
            None => true,
            Some(SegPat::SylBoundary) => idx == 0 || syl_boundary_after(w, idx - 1),
            Some(p) => Self::side_matches(w, p, idx.checked_sub(1)),
        };
        let post_ok = match &self.post {
            None => true,
            Some(SegPat::SylBoundary) => syl_boundary_after(w, idx),
            Some(p) => {
                let next = (idx + 1 < w.skeleton.len()).then_some(idx + 1);
                Self::side_matches(w, p, next)
            }
        };
        pre_ok && post_ok
    }
}

/// 音段是否為 onset(I8:落在某音節內、不入任何莫拉的前緣音段)。
fn is_onset(w: &Word, idx: u32) -> bool {
    (0..w.prosody.syllables.len()).any(|s| onset_segs(w, s).contains(&idx))
}

/// 音段是否為 coda:音節內、非 onset、且不屬核心(= 該音節第一個非空莫拉)。
/// 注意 coda 可自帶莫拉(weight-by-position),不能以「不入莫拉」判定。
fn is_coda(w: &Word, idx: u32) -> bool {
    let Some(syl) = w.prosody.syllables.iter().find(|s| s.contains_idx(idx)) else {
        return false;
    };
    if is_onset(w, idx) {
        return false;
    }
    let nucleus = w
        .prosody
        .moras
        .iter()
        .filter(|m| !m.is_empty_node() && m.within(syl))
        .min_by_key(|m| m.lo);
    nucleus.is_some_and(|n| !n.contains_idx(idx))
}

/// 音段規則的匹配側(交集語意:全部條件須同時成立)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SegMatch {
    /// 特徵矩陣(空 = 不設限)。
    pub feats: FeatBits,
    /// 類別成員(`@vowel`;None = 不設限)。
    pub class: Option<Vec<SymId>>,
    /// 位置述語(onset / coda;None = 不設限)。
    pub pos: Option<SegPos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegPos {
    Onset,
    Coda,
}

impl SegMatch {
    fn matches(&self, w: &Word, idx: usize) -> bool {
        let seg = &w.skeleton[idx];
        seg.feats.contains(self.feats)
            && self.class.as_ref().is_none_or(|c| c.contains(&seg.sym))
            && match self.pos {
                None => true,
                Some(SegPos::Onset) => is_onset(w, idx as u32),
                Some(SegPos::Coda) => is_coda(w, idx as u32),
            }
    }
}

/// 音段規則的輸出側。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegOut {
    /// `=> [矩陣]`:逐特徵欄位 set_field 改寫((遮罩, 新值位) 序列)。
    Subs(Vec<(FeatBits, FeatBits)>),
    /// `=> *`:刪除(I13 連鎖)。
    Delete,
}

/// 音段層規則:`<match> => <out> / pre _ post`(I12/I13;語法貼合 Lexurgy)。
/// 改寫:新特徵束由 Inventory 反查符號(無對應 = error)→ `SegRewrite`;
/// 刪除:→ `SegDelete`(commit 跑 I13 跨層連鎖)。VerbClass::Query(位置述語讀韻律)。
pub fn rewrite(
    w: &Word,
    inv: &Inventory,
    m: &SegMatch,
    out: &SegOut,
    env: &SegEnv,
) -> Result<Vec<Action>, EngineError> {
    let mut actions = Vec::new();
    for idx in 0..w.skeleton.len() {
        if !m.matches(w, idx) || !env.matches(w, idx) {
            continue;
        }
        match out {
            SegOut::Delete => actions.push(Action::SegDelete { idx }),
            SegOut::Subs(subs) => {
                let mut feats = w.skeleton[idx].feats;
                for &(mask, value) in subs {
                    feats = feats.set_field(mask, value);
                }
                let sym = inv
                    .sym_for(feats)
                    .ok_or(EngineError::NoSymbolForBundle { idx })?;
                actions.push(Action::SegRewrite { idx, sym, feats });
            }
        }
    }
    Ok(actions)
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

// ── 步驟 5:第二批動詞 ──

/// 擴散/位移方向(D10:一律顯式)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ward {
    Leftward,
    Rightward,
    Bidirectional,
}

/// `within` 疆界(M0:pword = 全詞;stem = 型態括號,§4)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Pword,
    Stem,
}

/// 雙向同拍衝突的處置(D11)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnConflict {
    /// 錨點保持 Ø,前沿停止(8.5 用)。
    Stop,
    /// 以指定值建新自體段掛上。
    Value(ValId),
}

/// 錨點(level, idx)的內容是否匹配特徵測試(Segment=該音段;Mora/Syl=區間內任一音段)。
fn anchor_matches(w: &Word, level: crate::repr::prosody::Level, idx: u32, test: FeatBits) -> bool {
    use crate::repr::prosody::Level;
    match level {
        Level::Segment => w
            .skeleton
            .get(idx as usize)
            .is_some_and(|s| s.feats.contains(test)),
        _ => w
            .prosody
            .level(level)
            .and_then(|v| v.get(idx as usize))
            .is_some_and(|span| {
                (span.lo..span.hi).any(|i| w.skeleton[i as usize].feats.contains(test))
            }),
    }
}

/// 錨點的骨架範圍(疆界判定用)。
fn anchor_seg_range(w: &Word, level: crate::repr::prosody::Level, idx: u32) -> (u32, u32) {
    use crate::repr::prosody::Level;
    match level {
        Level::Segment => (idx, idx + 1),
        _ => w
            .prosody
            .level(level)
            .and_then(|v| v.get(idx as usize))
            .map(|s| (s.lo, s.hi))
            .unwrap_or((0, 0)),
    }
}

/// 錨點是否落在疆界內。
fn in_domain(w: &Word, level: crate::repr::prosody::Level, idx: u32, dom: &(u32, u32)) -> bool {
    let (lo, hi) = anchor_seg_range(w, level, idx);
    dom.0 <= lo && hi <= dom.1
}

/// `spread <值> <方向> [blocked-by …] [within …] [through] [on-conflict …]`
/// = 迭代 associate(iterative,執行語意 §2):自每個已聯結的 `val` 源沿方向逐格展開,
/// 以規則內部中間狀態判斷下一格(不外洩,commit 一次);只填 Ø,遇非 Ø 停(D8,
/// `through` 才穿越)、遇 blocker 停、出疆界停;bidirectional 先到先佔,
/// 同拍衝突 → 錨點保持 Ø + on-conflict(D11)。VerbClass::Query。
#[allow(clippy::too_many_arguments)]
pub fn spread(
    w: &Word,
    tier: SymId,
    val: ValId,
    ward: Ward,
    blocked_by: Option<FeatBits>,
    within: Domain,
    through: bool,
    on_conflict: OnConflict,
) -> Result<Vec<Action>, EngineError> {
    let t = tier_of(w, tier)?;
    let level = t.anchor;
    let n = w.anchor_count(level) as u32;
    // 內部佔用表(iterative 中間狀態;初始 = 前狀態的承載)
    let mut occupied: Vec<bool> = (0..n)
        .map(|i| !t.bearers_of(AnchorRef::new(level, i)).is_empty())
        .collect();

    // 前沿:(seq_idx, 當前位置, 步進方向, 疆界)
    struct Front {
        seq: usize,
        cur: u32,
        dir: i64,
        dom: (u32, u32),
    }
    let mut fronts: Vec<Front> = Vec::new();
    for (si, a) in t.seq.iter().enumerate() {
        if a.val != val || a.is_floating() {
            continue;
        }
        let min = a.links.iter().map(|l| l.index).min().unwrap_or(0);
        let max = a.links.iter().map(|l| l.index).max().unwrap_or(0);
        let dom = match within {
            Domain::Pword => (0, w.skeleton.len() as u32),
            Domain::Stem => {
                let (slo, shi) = anchor_seg_range(w, level, min);
                w.morph
                    .iter()
                    .find(|b| {
                        b.unit == crate::repr::word::MorphUnit::Stem && b.lo <= slo && shi <= b.hi
                    })
                    .map(|b| (b.lo, b.hi))
                    // 缺括號:M0 以全詞續跑(相應 warn 屬 diag,步驟 6+)
                    .unwrap_or((0, w.skeleton.len() as u32))
            }
        };
        match ward {
            Ward::Rightward => fronts.push(Front { seq: si, cur: max, dir: 1, dom }),
            Ward::Leftward => fronts.push(Front { seq: si, cur: min, dir: -1, dom }),
            Ward::Bidirectional => {
                fronts.push(Front { seq: si, cur: max, dir: 1, dom });
                fronts.push(Front { seq: si, cur: min, dir: -1, dom });
            }
        }
    }

    let mut actions = Vec::new();
    let mut active: Vec<bool> = fronts.iter().map(|_| true).collect();
    while active.iter().any(|&a| a) {
        // 同拍:所有活前沿各提一個 claim
        let mut claims: Vec<(usize, u32)> = Vec::new();
        for (fi, f) in fronts.iter_mut().enumerate() {
            if !active[fi] {
                continue;
            }
            loop {
                let next_i = f.cur as i64 + f.dir;
                if next_i < 0 || next_i >= n as i64 {
                    active[fi] = false;
                    break;
                }
                let next = next_i as u32;
                if !in_domain(w, level, next, &f.dom) {
                    active[fi] = false;
                    break;
                }
                if let Some(b) = blocked_by {
                    if anchor_matches(w, level, next, b) {
                        active[fi] = false;
                        break;
                    }
                }
                if occupied[next as usize] {
                    if through {
                        f.cur = next; // 穿越:跨過佔用格續走(交叉由 NCC 於 validation 回報,D7)
                        continue;
                    }
                    active[fi] = false;
                    break;
                }
                claims.push((fi, next));
                break;
            }
        }
        // 同拍衝突偵測(D11):同一目標多前沿 → 錨點保持 Ø + on-conflict
        let mut resolved: Vec<(usize, u32)> = Vec::new();
        while let Some((fi, target)) = claims.pop() {
            let mut conflicted = false;
            claims.retain(|&(fj, t2)| {
                if t2 == target {
                    active[fj] = false;
                    conflicted = true;
                    false
                } else {
                    true
                }
            });
            if conflicted {
                active[fi] = false;
                if let OnConflict::Value(v) = on_conflict {
                    let links = std::iter::once(AnchorRef::new(level, target)).collect();
                    actions.push(primitives::insert(tier, None, v, links));
                    occupied[target as usize] = true;
                }
            } else {
                resolved.push((fi, target));
            }
        }
        // 套用本拍 claims(先到先佔)
        for (fi, target) in resolved.into_iter().rev() {
            actions.push(primitives::associate(
                tier,
                fronts[fi].seq,
                AnchorRef::new(level, target),
            ));
            occupied[target as usize] = true;
            fronts[fi].cur = target;
        }
    }
    Ok(actions)
}

/// `shift <n> <軌道單位> <方向>` = 每條邊同步平移(delink+associate,邊集在前狀態
/// 一次定案)。平移出界的邊只 delink → 自體段可致浮游(D14 stability)。VerbClass::Query。
pub fn shift(w: &Word, tier: SymId, n: u32, ward: Ward) -> Result<Vec<Action>, EngineError> {
    let t = tier_of(w, tier)?;
    let level = t.anchor;
    let count = w.anchor_count(level) as i64;
    let delta = match ward {
        Ward::Rightward => n as i64,
        Ward::Leftward => -(n as i64),
        Ward::Bidirectional => return Err(EngineError::TierNotFound(tier)), // 語法層已擋;防衛
    };
    let mut actions = Vec::new();
    for (si, a) in t.seq.iter().enumerate() {
        for l in a.links.iter() {
            let old = AnchorRef::new(level, l.index);
            actions.push(primitives::delink(tier, si, old));
            let ni = l.index as i64 + delta;
            if (0..count).contains(&ni) {
                actions.push(primitives::associate(
                    tier,
                    si,
                    AnchorRef::new(level, ni as u32),
                ));
            }
        }
    }
    Ok(actions)
}

/// `dominate <level>&empty -> @class <方向>`(8.4 repair):每個空節點沿方向找
/// 第一個匹配類別的音段 → `Dominate`。repair 類:不觸發 reparse(A3)。
pub fn dominate_empty(
    w: &Word,
    level: crate::repr::prosody::Level,
    target_class: &[SymId],
    ward: Ward,
) -> Result<Vec<Action>, EngineError> {
    let spans = w
        .prosody
        .level(level)
        .ok_or(EngineError::NotAProsodyLevel(level))?;
    let n = w.skeleton.len() as i64;
    let mut actions = Vec::new();
    for (node, s) in spans.iter().enumerate() {
        if !s.is_empty_node() {
            continue;
        }
        let (start, dir): (i64, i64) = match ward {
            Ward::Leftward => (s.lo as i64 - 1, -1),
            Ward::Rightward => (s.lo as i64, 1),
            Ward::Bidirectional => return Err(EngineError::NotAProsodyLevel(level)), // 語法層已擋
        };
        let mut i = start;
        while (0..n).contains(&i) {
            if target_class.contains(&w.skeleton[i as usize].sym) {
                actions.push(primitives::dominate(level, node, i as u32));
                break;
            }
            i += dir;
        }
    }
    Ok(actions)
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
        let acts = insert_floating_near(&w, TONE, H, voiced(), InsertProbe::Onset).unwrap();
        assert_eq!(acts.len(), 2);
        let w2 = commit(&w, &acts).unwrap();
        assert_eq!(w2.tier(TONE).unwrap().floating_indices().len(), 2);

        let none = insert_floating_near(&w, TONE, H, FeatBits(2), InsertProbe::Onset).unwrap();
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
        let m = SegMatch {
            feats: vplus,
            class: None,
            pos: Some(SegPos::Onset),
        };
        let out = SegOut::Subs(vec![(mask, vminus)]);
        let acts = rewrite(&w, &inv, &m, &out, &SegEnv::default()).unwrap();
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
        let err = rewrite(&w, &empty_inv, &m, &out, &SegEnv::default());
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
        let m = SegMatch {
            feats: vplus,
            class: None,
            pos: None,
        };
        let out = SegOut::Subs(vec![(mask, vminus)]);
        let acts = rewrite(&w, &inv, &m, &out, &env).unwrap();
        assert_eq!(acts.len(), 1); // 僅 idx 0(詞首);idx 2 前有音段
        assert!(matches!(acts[0], Action::SegRewrite { idx: 0, .. }));
    }

    /// 8.2 形:spread rightward 只填 Ø、遇 blocker 停(D8)。anchor = Segment。
    #[test]
    fn spread_rightward_fills_until_blocker_8_2() {
        // m a t a:nasal tier anchor segment;+nasal 掛 seg0;t 帶 [-son](bit1)
        let mut w = Word::new();
        let son_minus = FeatBits(0b10);
        for (i, f) in [FeatBits::EMPTY, FeatBits::EMPTY, son_minus, FeatBits::EMPTY]
            .iter()
            .enumerate()
        {
            w.skeleton.push(Seg::new(SymId(i as u32), *f));
        }
        w.prosody.syllables.push(Span::new(0, 2));
        w.prosody.syllables.push(Span::new(2, 4));
        w.prosody.moras.push(Span::new(1, 2));
        w.prosody.moras.push(Span::new(3, 4));
        let nasal = SymId(200);
        let plus = ValId(9);
        let mut t = MelodyTier::new(nasal, Level::Segment, vec![plus]);
        t.seq.push(Autoseg::linked(
            plus,
            vec![AnchorRef::new(Level::Segment, 0)],
        ));
        w.melodies.push(t);

        let acts = spread(
            &w,
            nasal,
            plus,
            Ward::Rightward,
            Some(son_minus),
            Domain::Pword,
            false,
            OnConflict::Stop,
        )
        .unwrap();
        let w2 = commit(&w, &acts).unwrap();
        let links = &w2.tier(nasal).unwrap().seq[0].links;
        // 掛 seg0(源)+ seg1(a);seg2 是 blocker 停下,seg3 不達
        assert_eq!(links.len(), 2);
        assert!(links.contains(&AnchorRef::new(Level::Segment, 1)));
        assert!(!links.contains(&AnchorRef::new(Level::Segment, 3)));
    }

    /// 8.5 形:bidirectional 同拍衝突 → 錨點保持 Ø(on-conflict stop,D11);within stem。
    #[test]
    fn spread_bidirectional_conflict_keeps_empty_8_5() {
        // 五莫拉,+ATR 源掛 μ0 與 μ4(兩源相向),中點 μ2 同拍衝突
        let mut w = Word::new();
        for i in 0..5u32 {
            w.skeleton.push(Seg::new(SymId(i), FeatBits::EMPTY));
        }
        w.prosody.syllables.push(Span::new(0, 5));
        for i in 0..5u32 {
            w.prosody.moras.push(Span::new(i, i + 1));
        }
        let atr = SymId(300);
        let plus = ValId(7);
        let mut t = MelodyTier::new(atr, Level::Mora, vec![plus]);
        t.seq.push(Autoseg::linked(plus, vec![AnchorRef::new(Level::Mora, 0)]));
        t.seq.push(Autoseg::linked(plus, vec![AnchorRef::new(Level::Mora, 4)]));
        w.melodies.push(t);

        let acts = spread(
            &w,
            atr,
            plus,
            Ward::Bidirectional,
            None,
            Domain::Stem, // 無括號:M0 以全詞續跑
            false,
            OnConflict::Stop,
        )
        .unwrap();
        let w2 = commit(&w, &acts).unwrap();
        let t2 = w2.tier(atr).unwrap();
        let m2 = AnchorRef::new(Level::Mora, 2);
        assert!(t2.bearers_of(m2).is_empty(), "同拍衝突錨點保持 Ø(D11)");
        assert!(t2.seq[0].links.contains(&AnchorRef::new(Level::Mora, 1)));
        assert!(t2.seq[1].links.contains(&AnchorRef::new(Level::Mora, 3)));
    }

    /// 8.3 形:shift 右移一莫拉;末端邊掉出 → 浮游(D14)。
    #[test]
    fn shift_moves_edges_and_floats_at_edge_8_3() {
        let mut w = cvcv([FeatBits::EMPTY, FeatBits::EMPTY]);
        {
            let t = w.tier_mut(TONE).unwrap();
            t.seq.push(Autoseg::linked(H, vec![AnchorRef::new(Level::Mora, 0)]));
            t.seq.push(Autoseg::linked(M, vec![AnchorRef::new(Level::Mora, 1)]));
        }
        let acts = shift(&w, TONE, 1, Ward::Rightward).unwrap();
        let w2 = commit(&w, &acts).unwrap();
        let t = w2.tier(TONE).unwrap();
        assert!(t.seq[0].links.contains(&AnchorRef::new(Level::Mora, 1))); // H → μ1
        assert!(t.seq[1].is_floating()); // M 掉出右緣 → 浮游
    }

    /// 8.4 形:dominate_empty 空莫拉向左找 @vowel 重掛。
    #[test]
    fn dominate_empty_finds_vowel_leftward_8_4() {
        let mut w = Word::new();
        let a_sym = SymId(0);
        w.skeleton.push(Seg::new(a_sym, FeatBits::EMPTY));
        w.prosody.syllables.push(Span::new(0, 1));
        w.prosody.moras.push(Span::new(0, 1));
        w.prosody.moras.push(Span::new(1, 1)); // 空莫拉(coda 脫落後)
        let acts = dominate_empty(&w, Level::Mora, &[a_sym], Ward::Leftward).unwrap();
        assert_eq!(acts.len(), 1);
        let w2 = commit(&w, &acts).unwrap();
        assert_eq!(w2.prosody.moras[1], Span::new(0, 1)); // 長元音
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
