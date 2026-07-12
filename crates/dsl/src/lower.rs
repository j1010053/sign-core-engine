//! lowering:AST(字串)→ `Program`(id/bits,executor 可直接執行)。
//!
//! 名稱消歧靠宣告:特徵值名 → 原子位、符號 → Inventory、tier 名/旋律值 →
//! interned id。M0 步驟 4 只降低 8.1 所需的語句形;超出者回報
//! `LowerError::Unsupported`(明確指出,不靜默容忍)。

use std::collections::HashMap;

use conlang_core::repr::feature::FeatBits;
use conlang_core::repr::intern::{SymId, ValId};
use conlang_core::repr::melody::MelodyTier;
use conlang_core::repr::prosody::Level;
use conlang_core::repr::Env;
use conlang_core::strategy::{Pick, Strategy, TieBreak};
use conlang_core::verbs::{Domain, InsertProbe, OnConflict, SegEnv, SegMatch, SegOut, SegPat, SegPos, Ward};

use crate::ast::*;

/// 規則的層級錨定 stage(P3/I14;M0 內僅標記,無行為)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    Stem,
    #[default]
    Word,
    Phrase,
}

/// 降低後語句(executor 的直接輸入)。
#[derive(Debug, Clone, PartialEq)]
pub enum LoweredStmt {
    InsertFloatingNear {
        tier: SymId,
        val: ValId,
        test: FeatBits,
        probe: InsertProbe,
    },
    Dock {
        tier: SymId,
        strategy: Strategy,
    },
    Fill {
        tier: SymId,
        val: ValId,
    },
    MergeAdjacentEqual {
        tier: SymId,
    },
    Spread {
        tier: SymId,
        val: ValId,
        ward: Ward,
        blocked_by: Option<FeatBits>,
        within: Domain,
        through: bool,
        on_conflict: OnConflict,
    },
    Shift {
        tier: SymId,
        n: u32,
        ward: Ward,
    },
    DominateEmpty {
        level: Level,
        class: Vec<SymId>,
        ward: Ward,
    },
    Rewrite {
        m: SegMatch,
        out: SegOut,
        env: SegEnv,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredRule {
    pub name: String,
    pub stage: Stage,
    pub stmts: Vec<LoweredStmt>,
}

/// 可執行程式:環境(interner/特徵/音素庫)+ tier 模板 + 規則序列。
#[derive(Debug, Clone)]
pub struct Program {
    pub env: Env,
    /// 每個 Melody 宣告一個模板;造詞時 clone 進 `Word::melodies`。
    pub tiers: Vec<MelodyTier>,
    pub rules: Vec<LoweredRule>,
    /// 類別(如 `vowel`,暫定音節化會用)。
    pub classes: HashMap<String, Vec<SymId>>,
}

impl Program {
    /// 依名稱找 tier 的 SymId(測試/CLI 便利)。
    pub fn tier_named(&self, name: &str) -> Option<SymId> {
        self.tiers
            .iter()
            .map(|t| t.name)
            .find(|&t| self.env.syms.resolve(t) == Some(name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LowerError {
    #[error("unknown name {0:?} (not a declared feature value / symbol / tier / class)")]
    UnknownName(String),
    #[error("melody value {0:?} not found in any declared tier alphabet")]
    UnknownMelodyValue(String),
    #[error("rule {rule:?}: {what} is not supported in M0 step 4")]
    Unsupported { rule: String, what: String },
    #[error("feature space exhausted while declaring {0:?}")]
    FeatureSpace(String),
    #[error("invalid stage {0:?} (expected stem|word|phrase)")]
    BadStage(String),
    #[error("invalid anchor level {0:?}")]
    BadAnchor(String),
}

fn level_of(name: &str) -> Option<Level> {
    match name {
        "segment" => Some(Level::Segment),
        "mora" => Some(Level::Mora),
        "syllable" => Some(Level::Syllable),
        "foot" => Some(Level::Foot),
        "pword" => Some(Level::Pword),
        _ => None,
    }
}

fn ward_of(name: &str) -> Option<Ward> {
    match name {
        "leftward" => Some(Ward::Leftward),
        "rightward" => Some(Ward::Rightward),
        "bidirectional" => Some(Ward::Bidirectional),
        _ => None,
    }
}

/// 值名拆解:`+voice` → ("voice","+");`labial` 需查它屬哪個特徵。
struct FeatNames {
    /// 值名(宣告原文,如 "+voice"、"labial")→ 單原子位集
    atoms: HashMap<String, FeatBits>,
    /// 值名 → 所屬特徵的欄位遮罩
    masks: HashMap<String, FeatBits>,
}

pub fn lower(file: &FileAst) -> Result<Program, LowerError> {
    let mut env = Env::new();
    let mut names = FeatNames {
        atoms: HashMap::new(),
        masks: HashMap::new(),
    };
    let mut tiers: Vec<MelodyTier> = Vec::new();
    let mut classes: HashMap<String, Vec<SymId>> = HashMap::new();

    // ── 宣告 ──
    for d in &file.decls {
        match d {
            Decl::Feature { name, values } => {
                for v in values {
                    let (feat, val) = match v.strip_prefix(['+', '-']) {
                        // `+voice`/`-voice`:特徵名 = 去號誌,值 = 號誌
                        Some(base) => (base.to_owned(), v[..1].to_owned()),
                        None => (name.clone(), v.clone()),
                    };
                    let bit = env
                        .feats
                        .register(&feat, &val)
                        .map_err(|_| LowerError::FeatureSpace(v.clone()))?;
                    let mut fb = FeatBits::EMPTY;
                    fb.insert(bit);
                    names.atoms.insert(v.clone(), fb);
                }
                // 欄位遮罩註冊完值後才完整,回填
                for v in values {
                    let feat = v
                        .strip_prefix(['+', '-'])
                        .map(str::to_owned)
                        .unwrap_or_else(|| name.clone());
                    let mask = env.feats.mask_of(&feat).unwrap_or(FeatBits::EMPTY);
                    names.masks.insert(v.clone(), mask);
                }
            }
            Decl::Symbol { name, atoms } => {
                let sym = env.syms.intern(name);
                let mut fb = FeatBits::EMPTY;
                for a in atoms {
                    fb = fb.union(
                        *names
                            .atoms
                            .get(a)
                            .ok_or_else(|| LowerError::UnknownName(a.clone()))?,
                    );
                }
                env.inv.register(sym, fb);
            }
            Decl::Class { name, members } => {
                let ids = members.iter().map(|m| env.syms.intern(m)).collect();
                classes.insert(name.clone(), ids);
            }
            Decl::Prosody { chain } => {
                for name in chain {
                    if env.domains.by_name(name).is_none() {
                        env.domains.register_named(name); // 自定域(I14)
                    }
                }
            }
            Decl::Melody {
                name,
                values,
                anchor,
            } => {
                let tname = env.syms.intern(name);
                let vals: Vec<ValId> = values.iter().map(|v| env.vals.intern(v)).collect();
                let lvl = level_of(anchor)
                    .or_else(|| env.domains.by_name(anchor))
                    .ok_or_else(|| LowerError::BadAnchor(anchor.clone()))?;
                tiers.push(MelodyTier::new(tname, lvl, vals));
            }
        }
    }

    // 旋律值 → (tier, val) 反查
    let find_val = |env: &Env, tiers: &[MelodyTier], name: &str| -> Option<(SymId, ValId)> {
        let vid = tiers.iter().find_map(|t| {
            t.alphabet.iter().copied().find(|v| {
                env.vals.resolve(*v) == Some(name)
            })
        })?;
        let tier = tiers
            .iter()
            .find(|t| t.alphabet.contains(&vid))
            .map(|t| t.name)?;
        Some((tier, vid))
    };
    let find_tier = |env: &Env, tiers: &[MelodyTier], name: &str| -> Option<SymId> {
        tiers
            .iter()
            .map(|t| t.name)
            .find(|&t| env.syms.resolve(t) == Some(name))
    };

    // 矩陣 → (合併位集, [(mask, value)] 逐原子)
    let matrix_bits = |names: &FeatNames, atoms: &[String]| -> Result<(FeatBits, Vec<(FeatBits, FeatBits)>), LowerError> {
        let mut all = FeatBits::EMPTY;
        let mut subs = Vec::new();
        for a in atoms {
            let bits = *names
                .atoms
                .get(a)
                .ok_or_else(|| LowerError::UnknownName(a.clone()))?;
            let mask = *names.masks.get(a).unwrap_or(&bits);
            all = all.union(bits);
            subs.push((mask, bits));
        }
        Ok((all, subs))
    };

    // ── 規則 ──
    let mut rules = Vec::new();
    for r in &file.rules {
        let unsupported = |what: &str| LowerError::Unsupported {
            rule: r.name.clone(),
            what: what.to_owned(),
        };
        let mut stage = Stage::default();
        let mut stmts = Vec::new();
        for s in &r.stmts {
            match s {
                Stmt::Stage(l) => {
                    stage = match l.as_str() {
                        "stem" => Stage::Stem,
                        "word" => Stage::Word,
                        "phrase" => Stage::Phrase,
                        other => return Err(LowerError::BadStage(other.to_owned())),
                    };
                }
                Stmt::Insert { val, near, env: e } => {
                    let (tier, vid) = find_val(&env, &tiers, val)
                        .ok_or_else(|| LowerError::UnknownMelodyValue(val.clone()))?;
                    if let Some(n) = near {
                        let anchor = tiers.iter().find(|t| t.name == tier).map(|t| t.anchor);
                        if level_of(n) != anchor {
                            return Err(unsupported("`near` level differing from tier anchor"));
                        }
                    }
                    // 環境:`/ onset&[矩陣] _`(Onset 探測)或 `/ [矩陣] _`(錨點內容探測)
                    let mut test = FeatBits::EMPTY;
                    let mut probe = InsertProbe::AnchorContent;
                    match e {
                        None => {}
                        Some(RuleEnv { pre: Some(pre), post: None }) => {
                            for el in &pre.0 {
                                match el {
                                    Element::Named(n) if n == "onset" => {
                                        probe = InsertProbe::Onset
                                    }
                                    Element::Matrix(atoms) => {
                                        test = test.union(matrix_bits(&names, atoms)?.0)
                                    }
                                    other => {
                                        return Err(unsupported(&format!(
                                            "insert env element {other:?}"
                                        )))
                                    }
                                }
                            }
                        }
                        Some(_) => return Err(unsupported("insert env shape")),
                    }
                    stmts.push(LoweredStmt::InsertFloatingNear {
                        tier,
                        val: vid,
                        test,
                        probe,
                    });
                }
                Stmt::Dock {
                    sel,
                    strategy,
                    tiebreak,
                } => {
                    // sel 必須形如 `tier&floating`
                    let mut tier = None;
                    for el in &sel.0 {
                        match el {
                            Element::Named(n) => {
                                tier = find_tier(&env, &tiers, n);
                            }
                            Element::Floating => {}
                            other => return Err(unsupported(&format!("dock selector {other:?}"))),
                        }
                    }
                    let tier = tier.ok_or_else(|| unsupported("dock without tier name"))?;
                    let pick = match strategy.as_str() {
                        "nearest" => Pick::Nearest,
                        "leftmost" => Pick::Leftmost,
                        "rightmost" => Pick::Rightmost,
                        other => return Err(unsupported(&format!("strategy {other:?}"))), // 自定義註冊:步驟 5+
                    };
                    let tb = match tiebreak.as_deref() {
                        None | Some("prefer-left") => TieBreak::PreferLeft,
                        Some("prefer-right") => TieBreak::PreferRight,
                        Some(other) => return Err(unsupported(&format!("tiebreak {other:?}"))),
                    };
                    stmts.push(LoweredStmt::Dock {
                        tier,
                        strategy: Strategy { pick, tiebreak: tb },
                    });
                }
                Stmt::Fill { tier, val, within } => {
                    if let Some(wb) = within {
                        if wb != "pword" {
                            return Err(unsupported("fill within other than pword"));
                        }
                    }
                    let t = find_tier(&env, &tiers, tier)
                        .ok_or_else(|| LowerError::UnknownName(tier.clone()))?;
                    let (vt, vid) = find_val(&env, &tiers, val)
                        .ok_or_else(|| LowerError::UnknownMelodyValue(val.clone()))?;
                    if vt != t {
                        return Err(unsupported("fill value not in tier alphabet"));
                    }
                    stmts.push(LoweredStmt::Fill { tier: t, val: vid });
                }
                Stmt::MergeAdjacentEqual => {
                    // M0:單 tier 檔案;多 tier 時需受詞消歧(D26),暫取唯一 tier
                    let t = tiers
                        .first()
                        .map(|t| t.name)
                        .ok_or_else(|| unsupported("merge without any melody tier"))?;
                    if tiers.len() > 1 {
                        return Err(unsupported("merge with multiple tiers (needs selector)"));
                    }
                    stmts.push(LoweredStmt::MergeAdjacentEqual { tier: t });
                }
                Stmt::Spread {
                    val,
                    ward,
                    blocked_by,
                    within,
                    through,
                    on_conflict,
                } => {
                    let (tier, vid) = find_val(&env, &tiers, val)
                        .ok_or_else(|| LowerError::UnknownMelodyValue(val.clone()))?;
                    let ward = ward_of(ward).ok_or_else(|| unsupported("spread ward"))?;
                    let blocked = match blocked_by {
                        None => None,
                        Some(sel) => match sel.0.as_slice() {
                            [Element::Matrix(atoms)] => Some(matrix_bits(&names, atoms)?.0),
                            other => {
                                return Err(unsupported(&format!("blocked-by {other:?}")))
                            }
                        },
                    };
                    let within = match within.as_deref() {
                        None | Some("pword") => Domain::Pword,
                        Some("stem") => Domain::Stem,
                        Some(other) => return Err(unsupported(&format!("within {other:?}"))),
                    };
                    let on_conflict = match on_conflict.as_deref() {
                        None | Some("stop") => OnConflict::Stop,
                        Some(v) => {
                            let (vt, vv) = find_val(&env, &tiers, v)
                                .ok_or_else(|| LowerError::UnknownMelodyValue(v.to_owned()))?;
                            if vt != tier {
                                return Err(unsupported("on-conflict value not in tier"));
                            }
                            OnConflict::Value(vv)
                        }
                    };
                    stmts.push(LoweredStmt::Spread {
                        tier,
                        val: vid,
                        ward,
                        blocked_by: blocked,
                        within,
                        through: *through,
                        on_conflict,
                    });
                }
                Stmt::Shift { n, unit, ward } => {
                    let t = tiers.first().ok_or_else(|| unsupported("shift without tier"))?;
                    if tiers.len() > 1 {
                        return Err(unsupported("shift with multiple tiers (needs selector)"));
                    }
                    if level_of(unit) != Some(t.anchor) {
                        return Err(unsupported("shift unit differing from tier anchor"));
                    }
                    let ward = match ward_of(ward) {
                        Some(w @ (Ward::Leftward | Ward::Rightward)) => w,
                        _ => return Err(unsupported("shift ward (leftward|rightward only)")),
                    };
                    stmts.push(LoweredStmt::Shift {
                        tier: t.name,
                        n: *n,
                        ward,
                    });
                }
                Stmt::Dominate { sel, target, ward } => {
                    // sel 形:`<level>&empty`
                    let mut lvl = None;
                    let mut saw_empty = false;
                    for el in &sel.0 {
                        match el {
                            Element::LevelRef(l) => lvl = level_of(l),
                            Element::Named(n) if n == "empty" => saw_empty = true,
                            other => {
                                return Err(unsupported(&format!("dominate selector {other:?}")))
                            }
                        }
                    }
                    let level = lvl.ok_or_else(|| unsupported("dominate without <level>"))?;
                    if !saw_empty {
                        return Err(unsupported("dominate without &empty (M0 repair form)"));
                    }
                    // target 形:`@class`
                    let class = match target.0.as_slice() {
                        [Element::ClassRef(c)] => classes
                            .get(c)
                            .cloned()
                            .ok_or_else(|| LowerError::UnknownName(c.clone()))?,
                        other => return Err(unsupported(&format!("dominate target {other:?}"))),
                    };
                    let ward = match ward_of(ward) {
                        Some(w @ (Ward::Leftward | Ward::Rightward)) => w,
                        _ => return Err(unsupported("dominate ward (leftward|rightward only)")),
                    };
                    stmts.push(LoweredStmt::DominateEmpty { level, class, ward });
                }
                Stmt::Rewrite { from, to, env: e } => {
                    let mut m = SegMatch::default();
                    for el in &from.0 {
                        match el {
                            Element::Matrix(atoms) => {
                                m.feats = m.feats.union(matrix_bits(&names, atoms)?.0)
                            }
                            Element::Named(n) if n == "onset" => m.pos = Some(SegPos::Onset),
                            Element::Named(n) if n == "coda" => m.pos = Some(SegPos::Coda),
                            Element::ClassRef(c) => {
                                // 宣告類別優先;未宣告的 @onset/@coda 為內建位置述語
                                if let Some(members) = classes.get(c) {
                                    m.class = Some(members.clone());
                                } else if c == "onset" {
                                    m.pos = Some(SegPos::Onset);
                                } else if c == "coda" {
                                    m.pos = Some(SegPos::Coda);
                                } else {
                                    return Err(LowerError::UnknownName(c.clone()));
                                }
                            }
                            other => {
                                return Err(unsupported(&format!("rewrite match {other:?}")))
                            }
                        }
                    }
                    let out = match to.0.as_slice() {
                        [Element::Star] => SegOut::Delete,
                        [Element::Matrix(atoms)] => SegOut::Subs(matrix_bits(&names, atoms)?.1),
                        other => return Err(unsupported(&format!("rewrite output {other:?}"))),
                    };
                    let seg_pat = |sel: &Selector| -> Result<SegPat, LowerError> {
                        match sel.0.as_slice() {
                            [Element::Boundary] => Ok(SegPat::Boundary),
                            [Element::SylBoundary] => Ok(SegPat::SylBoundary),
                            [Element::Matrix(atoms)] => {
                                Ok(SegPat::Feats(matrix_bits(&names, atoms)?.0))
                            }
                            [Element::ClassRef(c)] => Ok(SegPat::Class(
                                classes
                                    .get(c)
                                    .cloned()
                                    .ok_or_else(|| LowerError::UnknownName(c.clone()))?,
                            )),
                            other => Err(unsupported(&format!("rewrite env {other:?}"))),
                        }
                    };
                    let env_lowered = match e {
                        None => SegEnv::default(),
                        Some(RuleEnv { pre, post }) => SegEnv {
                            pre: pre.as_ref().map(&seg_pat).transpose()?,
                            post: post.as_ref().map(&seg_pat).transpose()?,
                        },
                    };
                    stmts.push(LoweredStmt::Rewrite {
                        m,
                        out,
                        env: env_lowered,
                    });
                }
            }
        }
        rules.push(LoweredRule {
            name: r.name.clone(),
            stage,
            stmts,
        });
    }

    Ok(Program {
        env,
        tiers,
        rules,
        classes,
    })
}
