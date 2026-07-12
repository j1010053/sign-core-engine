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
use conlang_core::verbs::{SegEnv, SegPat};

use crate::ast::*;

/// 規則層級(P3;M0 內僅標記,無行為)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuleLevel {
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
        onset_test: FeatBits,
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
    Rewrite {
        match_feats: FeatBits,
        require_onset: bool,
        subs: Vec<(FeatBits, FeatBits)>,
        env: SegEnv,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredRule {
    pub name: String,
    pub level: RuleLevel,
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
    #[error("invalid level {0:?} (expected stem|word|phrase)")]
    BadLevel(String),
    #[error("invalid anchor level {0:?}")]
    BadAnchor(String),
}

fn level_of(name: &str) -> Option<Level> {
    match name {
        "mora" => Some(Level::Mora),
        "syllable" => Some(Level::Syllable),
        "foot" => Some(Level::Foot),
        "pword" => Some(Level::Pword),
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
            Decl::Melody {
                name,
                values,
                anchor,
            } => {
                let tname = env.syms.intern(name);
                let vals: Vec<ValId> = values.iter().map(|v| env.vals.intern(v)).collect();
                let lvl = level_of(anchor).ok_or_else(|| LowerError::BadAnchor(anchor.clone()))?;
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
        let mut level = RuleLevel::default();
        let mut stmts = Vec::new();
        for s in &r.stmts {
            match s {
                Stmt::Level(l) => {
                    level = match l.as_str() {
                        "stem" => RuleLevel::Stem,
                        "word" => RuleLevel::Word,
                        "phrase" => RuleLevel::Phrase,
                        other => return Err(LowerError::BadLevel(other.to_owned())),
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
                    // 環境:M0 僅支援 `/ onset&[矩陣] _`
                    let mut test = FeatBits::EMPTY;
                    match e {
                        None => {}
                        Some(RuleEnv { pre: Some(pre), post: None }) => {
                            let mut saw_onset = false;
                            for el in &pre.0 {
                                match el {
                                    Element::Named(n) if n == "onset" => saw_onset = true,
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
                            if !saw_onset {
                                return Err(unsupported("insert env without `onset`"));
                            }
                        }
                        Some(_) => return Err(unsupported("insert env shape")),
                    }
                    stmts.push(LoweredStmt::InsertFloatingNear {
                        tier,
                        val: vid,
                        onset_test: test,
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
                Stmt::Rewrite { from, to, env: e } => {
                    let mut match_feats = FeatBits::EMPTY;
                    let mut require_onset = false;
                    for el in &from.0 {
                        match el {
                            Element::Matrix(atoms) => {
                                match_feats = match_feats.union(matrix_bits(&names, atoms)?.0)
                            }
                            Element::Named(n) if n == "onset" => require_onset = true,
                            other => {
                                return Err(unsupported(&format!("rewrite match {other:?}")))
                            }
                        }
                    }
                    let subs = match to.0.as_slice() {
                        [Element::Matrix(atoms)] => matrix_bits(&names, atoms)?.1,
                        other => return Err(unsupported(&format!("rewrite output {other:?}"))),
                    };
                    let seg_pat = |sel: &Selector| -> Result<SegPat, LowerError> {
                        match sel.0.as_slice() {
                            [Element::Boundary] => Ok(SegPat::Boundary),
                            [Element::Matrix(atoms)] => {
                                Ok(SegPat::Feats(matrix_bits(&names, atoms)?.0))
                            }
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
                        match_feats,
                        require_onset,
                        subs,
                        env: env_lowered,
                    });
                }
            }
        }
        rules.push(LoweredRule {
            name: r.name.clone(),
            level,
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
