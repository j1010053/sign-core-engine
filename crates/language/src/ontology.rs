//! Ontology registry + `belongs` 閉包(步驟 12a;修補07 P38 **v0.2:單一分類樹**)。
//!
//! **一棵維度中立的分類樹**(owner 裁決 2026-07-20):`trait` 是分類節點,`belongs`
//! (P40)構成單一繼承網絡(CxG 單一 constructicon)。**無 `syn trait` 維度標記**——
//! syn/phon/sem/prag 是 sign/trait 身上的**內容面向**(Def 區塊),由 [`crate::projection`]
//! 按維度讀取;分類本身不分維。registry 自一組 [`Language`] 建成;**最小本體為額外
//! 引用的 stdlib `.lang`**(I20;見 [`std_ontology`])。
//!
//! `belongs` 三義(P40):成員 + 展開來源 + 保留標記。本模組管**成員 + 標記**(閉包);
//! **展開**(繼承 Def 的有效值)由 projection 於讀取時解析(P39 Defs 單一源,不複製)。
//! 診斷分級回報、不 panic(承 B9/P26)。

use std::collections::{BTreeMap, BTreeSet};

use crate::{Language, SignDef, SignItem, TraitDef};

/// ontology 建構期診斷(全為 error 級;分級資料,不 panic)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntologyDiag {
    /// `belongs` 目標不是任何已宣告 trait。
    UnknownTrait { referrer: String, target: String },
    /// `belongs` 邊成環。
    Cycle { path: Vec<String> },
    /// 重複定義的 trait 名。
    DuplicateTrait { name: String },
}

/// 一個分類節點。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntoNode {
    /// 直接父邊(`belongs` 目標,保序去重)。
    pub parents: Vec<String>,
    /// 節點自帶 Defs(全維度,含 dim 前綴路徑;繼承給後代,projection 按維讀)。
    pub defs: Vec<(String, String)>,
}

/// 單一分類樹 registry(P38 v0.2)。
#[derive(Debug, Clone, Default)]
pub struct OntologyRegistry {
    tree: BTreeMap<String, OntoNode>,
}

fn trait_belongs(t: &TraitDef) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for b in &t.blocks {
        for it in &b.items {
            if let SignItem::Belongs(name) = it {
                if seen.insert(name.clone()) {
                    out.push(name.clone());
                }
            }
        }
    }
    out
}

fn trait_defs(t: &TraitDef) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for b in &t.blocks {
        for it in &b.items {
            if let SignItem::Def(d) = it {
                out.push((d.path.clone(), d.value.clone()));
            }
        }
    }
    out
}

impl OntologyRegistry {
    /// 自一組 Language 建 registry(順序 = 覆蓋層:stdlib 在前,使用者在後)。
    pub fn build(langs: &[&Language]) -> (OntologyRegistry, Vec<OntologyDiag>) {
        let mut reg = OntologyRegistry::default();
        let mut diags = Vec::new();

        for lang in langs {
            for t in &lang.traits {
                if reg.tree.contains_key(&t.name) {
                    diags.push(OntologyDiag::DuplicateTrait {
                        name: t.name.clone(),
                    });
                    continue;
                }
                reg.tree.insert(
                    t.name.clone(),
                    OntoNode {
                        parents: trait_belongs(t),
                        defs: trait_defs(t),
                    },
                );
            }
        }

        // belongs 目標須存在
        for (name, node) in &reg.tree {
            for parent in &node.parents {
                if !reg.tree.contains_key(parent) {
                    diags.push(OntologyDiag::UnknownTrait {
                        referrer: name.clone(),
                        target: parent.clone(),
                    });
                }
            }
        }
        // sign 的 belongs 目標須存在(不靜默丟分類)
        for lang in langs {
            for s in &lang.signs {
                for it in &s.items {
                    if let SignItem::Belongs(target) = it {
                        if !reg.tree.contains_key(target) {
                            diags.push(OntologyDiag::UnknownTrait {
                                referrer: s.name.clone(),
                                target: target.clone(),
                            });
                        }
                    }
                }
            }
        }
        detect_cycles(&reg.tree, &mut diags);
        (reg, diags)
    }

    pub fn has(&self, name: &str) -> bool {
        self.tree.contains_key(name)
    }

    pub fn node(&self, name: &str) -> Option<&OntoNode> {
        self.tree.get(name)
    }

    /// 範疇節點的 **belongs 閉包**:self → 父 → 祖 … → 根(nearest-first、
    /// pre-order DFS、首見去重、循環安全)。未知 → 空。
    pub fn closure(&self, name: &str) -> Vec<String> {
        if !self.tree.contains_key(name) {
            return Vec::new();
        }
        fn visit(
            tree: &BTreeMap<String, OntoNode>,
            name: &str,
            out: &mut Vec<String>,
            seen: &mut BTreeSet<String>,
        ) {
            if !seen.insert(name.to_owned()) {
                return;
            }
            out.push(name.to_owned());
            if let Some(node) = tree.get(name) {
                for p in &node.parents {
                    if tree.contains_key(p) {
                        visit(tree, p, out, seen);
                    }
                }
            }
        }
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        visit(&self.tree, name, &mut out, &mut seen);
        out
    }

    /// 一個 sign 的分類閉包:各 `belongs` 目標的閉包依序併接、全域去重。
    pub fn sign_categories(&self, sign: &SignDef) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for it in &sign.items {
            if let SignItem::Belongs(target) = it {
                if self.has(target) {
                    for c in self.closure(target) {
                        if seen.insert(c.clone()) {
                            out.push(c);
                        }
                    }
                }
            }
        }
        out
    }

    /// 全部節點名(測試/inspection;決定性排序)。
    pub fn names(&self) -> Vec<&str> {
        self.tree.keys().map(String::as_str).collect()
    }
}

/// DFS 三色循環偵測。
fn detect_cycles(tree: &BTreeMap<String, OntoNode>, diags: &mut Vec<OntologyDiag>) {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: BTreeMap<String, Color> =
        tree.keys().map(|k| (k.clone(), Color::White)).collect();
    let mut reported: BTreeSet<Vec<String>> = BTreeSet::new();

    fn dfs(
        node: &str,
        tree: &BTreeMap<String, OntoNode>,
        color: &mut BTreeMap<String, Color>,
        stack: &mut Vec<String>,
        reported: &mut BTreeSet<Vec<String>>,
        diags: &mut Vec<OntologyDiag>,
    ) {
        color.insert(node.to_owned(), Color::Gray);
        stack.push(node.to_owned());
        if let Some(n) = tree.get(node) {
            for p in &n.parents {
                if !tree.contains_key(p) {
                    continue;
                }
                match color.get(p).copied().unwrap_or(Color::White) {
                    Color::Gray => {
                        if let Some(pos) = stack.iter().position(|x| x == p) {
                            let cycle = stack[pos..].to_vec();
                            if reported.insert(cycle.clone()) {
                                diags.push(OntologyDiag::Cycle { path: cycle });
                            }
                        }
                    }
                    Color::White => dfs(p, tree, color, stack, reported, diags),
                    Color::Black => {}
                }
            }
        }
        stack.pop();
        color.insert(node.to_owned(), Color::Black);
    }

    let names: Vec<String> = tree.keys().cloned().collect();
    for name in names {
        if color.get(&name).copied() == Some(Color::White) {
            let mut stack = Vec::new();
            dfs(&name, tree, &mut color, &mut stack, &mut reported, diags);
        }
    }
}

/// 最小本體:額外引用的 stdlib `.lang` trait(I20;資料層,非 Rust 硬編碼)。
pub fn std_ontology() -> Language {
    Language::parse(include_str!("../std/ontology.lang")).expect("std ontology must parse")
}

/// 便利:以 stdlib 本體 + 使用者 Language 建 registry(額外引用語意)。
pub fn with_std(user: &Language) -> (OntologyRegistry, Vec<OntologyDiag>) {
    let std = std_ontology();
    OntologyRegistry::build(&[&std, user])
}
