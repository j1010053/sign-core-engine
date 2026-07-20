//! 四維 ontology registry + `belongs` 閉包(步驟 12a;修補07 P38/P40)。
//!
//! 四棵**彼此獨立**的分類樹(phon/syn/sem/prag,P38 不共享同一棵):由
//! **ontology trait**(`dim = Some(_)`)構成節點,`belongs`(P40)= 同 dim 樹上的
//! 父邊。registry 自一組 [`Language`] 建成——**最小本體為額外引用的 stdlib
//! `.lang`**(I20:本體是資料層,非 Rust 硬編碼;見 [`std_ontology`]),使用者的
//! `.lang` 併入其後、可擴充自定範疇(掛回本體某節點,docs/07 §9)。
//!
//! `belongs` 三義(P40):成員 + 展開來源 + 保留標記。本模組管**成員 + 標記**
//! (閉包查詢);**展開**(繼承 Def 的有效值)由 [`crate::projection`] 於讀取時
//! 解析(P39:Defs 單一源,不物理複製)。診斷分級回報、不 panic(承 B9/P26)。

use std::collections::{BTreeMap, BTreeSet};

use crate::{Dim, Item, Language, SignDef, SignItem, TraitDef};

/// ontology 建構期診斷(全為 error 級;分級資料,不 panic)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntologyDiag {
    /// `belongs` 目標不是任一維的 ontology trait。
    UnknownTrait { referrer: String, target: String },
    /// ontology trait 的 `belongs` 跨維(P38:閉包只在同 dim 內走)。
    CrossDimBelongs {
        trait_name: String,
        dim: Dim,
        target: String,
        target_dim: Dim,
    },
    /// 同 dim 樹的 `belongs` 邊成環。
    Cycle { dim: Dim, path: Vec<String> },
    /// 同 dim 內重複定義的 ontology trait 名。
    DuplicateTrait { dim: Dim, name: String },
}

/// 一個 ontology 節點(某維分類樹上的範疇)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntoNode {
    pub dim: Dim,
    /// 直接父邊(`belongs` 目標,保序去重)。
    pub parents: Vec<String>,
    /// 節點自帶的 dim-scoped Defs(繼承給後代;projection 讀取時解析,P39)。
    pub defs: Vec<(String, String)>,
}

/// 四維 ontology registry。名稱空間**依 dim 分開**(P38);同名不同維合法。
#[derive(Debug, Clone, Default)]
pub struct OntologyRegistry {
    trees: BTreeMap<Dim, BTreeMap<String, OntoNode>>,
}

/// 抽出一個路徑的維度歸屬(前綴 = dim 關鍵詞)。
/// `syn.provides`/`sem.gloss`/`prag.register` 或裸 `phon`(phon 維 UR)。
fn dim_of_path(path: &str) -> Option<Dim> {
    if let Some((head, _)) = path.split_once('.') {
        return Dim::parse(head);
    }
    Dim::parse(path) // 裸維名(如 `phon = /go/`)
}

fn trait_belongs(t: &TraitDef) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for b in &t.blocks {
        for it in &b.items {
            if let Item::Belongs(name) = it {
                if seen.insert(name.clone()) {
                    out.push(name.clone());
                }
            }
        }
    }
    out
}

fn trait_dim_defs(t: &TraitDef, dim: Dim) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for b in &t.blocks {
        for it in &b.items {
            if let Item::Def(d) = it {
                if dim_of_path(&d.path) == Some(dim) {
                    out.push((d.path.clone(), d.value.clone()));
                }
            }
        }
    }
    out
}

impl OntologyRegistry {
    /// 自一組 Language 建 registry(順序 = 覆蓋層:stdlib 在前,使用者在後)。
    /// 回傳 registry + 建構診斷(errors);診斷非空時 registry 仍盡量可用。
    pub fn build(langs: &[&Language]) -> (OntologyRegistry, Vec<OntologyDiag>) {
        let mut reg = OntologyRegistry::default();
        let mut diags = Vec::new();

        // ── 收集節點(同 dim 重名 = DuplicateTrait)──
        for lang in langs {
            for t in &lang.traits {
                let Some(dim) = t.dim else { continue };
                let tree = reg.trees.entry(dim).or_default();
                if tree.contains_key(&t.name) {
                    diags.push(OntologyDiag::DuplicateTrait {
                        dim,
                        name: t.name.clone(),
                    });
                    continue;
                }
                tree.insert(
                    t.name.clone(),
                    OntoNode {
                        dim,
                        parents: trait_belongs(t),
                        defs: trait_dim_defs(t, dim),
                    },
                );
            }
        }

        // ── 驗證 belongs 邊(未知 / 跨維)──
        // name → 出現於哪些維(供跨維偵測)。
        let mut name_dims: BTreeMap<String, Vec<Dim>> = BTreeMap::new();
        for (dim, tree) in &reg.trees {
            for name in tree.keys() {
                name_dims.entry(name.clone()).or_default().push(*dim);
            }
        }
        for dim in Dim::all() {
            let Some(tree) = reg.trees.get(&dim) else {
                continue;
            };
            for (name, node) in tree {
                for parent in &node.parents {
                    if tree.contains_key(parent) {
                        continue; // 同維存在 ✓
                    }
                    match name_dims.get(parent) {
                        Some(dims) => diags.push(OntologyDiag::CrossDimBelongs {
                            trait_name: name.clone(),
                            dim,
                            target: parent.clone(),
                            target_dim: dims[0],
                        }),
                        None => diags.push(OntologyDiag::UnknownTrait {
                            referrer: name.clone(),
                            target: parent.clone(),
                        }),
                    }
                }
            }
        }

        // ── sign 的 belongs 目標:須為某維 ontology 節點(否則靜默丟分類)──
        for lang in langs {
            for s in &lang.signs {
                for it in &s.items {
                    if let SignItem::Belongs(target) = it {
                        if !name_dims.contains_key(target) {
                            diags.push(OntologyDiag::UnknownTrait {
                                referrer: s.name.clone(),
                                target: target.clone(),
                            });
                        }
                    }
                }
            }
        }

        // ── 循環偵測(每維 DFS)──
        for dim in Dim::all() {
            if let Some(tree) = reg.trees.get(&dim) {
                detect_cycles(dim, tree, &mut diags);
            }
        }

        (reg, diags)
    }

    /// 某維是否有此範疇節點。
    pub fn has(&self, dim: Dim, name: &str) -> bool {
        self.trees.get(&dim).is_some_and(|t| t.contains_key(name))
    }

    pub fn node(&self, dim: Dim, name: &str) -> Option<&OntoNode> {
        self.trees.get(&dim)?.get(name)
    }

    /// 一個範疇節點的 **belongs 閉包**:自身 + 所有祖先,**nearest-first**
    /// (self → 直接父 → 祖父 … → 根;pre-order DFS、parents 保序、首見去重、
    /// 循環安全)。近祖在前使 projection 繼承時「近勝遠」。未知節點 → 空
    /// (建構期已診斷)。
    pub fn closure(&self, dim: Dim, name: &str) -> Vec<String> {
        let Some(tree) = self.trees.get(&dim) else {
            return Vec::new();
        };
        if !tree.contains_key(name) {
            return Vec::new();
        }
        fn visit(
            tree: &BTreeMap<String, OntoNode>,
            name: &str,
            out: &mut Vec<String>,
            seen: &mut BTreeSet<String>,
        ) {
            if !seen.insert(name.to_owned()) {
                return; // 已收(保較近一次)或成環:止步
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
        visit(tree, name, &mut out, &mut seen);
        out
    }

    /// 一個 **sign** 在某維的分類閉包:sign 的 `belongs`(可跨維,取本維者)
    /// 各自展開閉包後**依序併接、全域去重**(sign 級,P43 逐單元判定的前提)。
    pub fn sign_categories(&self, sign: &SignDef, dim: Dim) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for it in &sign.items {
            if let SignItem::Belongs(target) = it {
                if self.has(dim, target) {
                    for c in self.closure(dim, target) {
                        if seen.insert(c.clone()) {
                            out.push(c);
                        }
                    }
                }
            }
        }
        out
    }

    /// registry 內某維全部節點名(測試/inspection;決定性排序)。
    pub fn names(&self, dim: Dim) -> Vec<&str> {
        self.trees
            .get(&dim)
            .map(|t| t.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }
}

/// 每維 DFS 三色循環偵測;命中即記一條 `Cycle`(路徑供定位)。
fn detect_cycles(dim: Dim, tree: &BTreeMap<String, OntoNode>, diags: &mut Vec<OntologyDiag>) {
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
        dim: Dim,
        reported: &mut BTreeSet<Vec<String>>,
        diags: &mut Vec<OntologyDiag>,
    ) {
        color.insert(node.to_owned(), Color::Gray);
        stack.push(node.to_owned());
        if let Some(n) = tree.get(node) {
            for p in &n.parents {
                if !tree.contains_key(p) {
                    continue; // 未知父(另有診斷)
                }
                match color.get(p).copied().unwrap_or(Color::White) {
                    Color::Gray => {
                        // 回邊 → 環;截取 stack 自 p 起
                        if let Some(pos) = stack.iter().position(|x| x == p) {
                            let cycle = stack[pos..].to_vec();
                            if reported.insert(cycle.clone()) {
                                diags.push(OntologyDiag::Cycle { dim, path: cycle });
                            }
                        }
                    }
                    Color::White => dfs(p, tree, color, stack, dim, reported, diags),
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
            dfs(&name, tree, &mut color, &mut stack, dim, &mut reported, diags);
        }
    }
}

/// 最小本體:額外引用的 stdlib `.lang` trait(I20;資料層,非 Rust 硬編碼)。
/// 內容住 `std/ontology.lang`(隨 crate 發布,round-trip 測試把關)。
pub fn std_ontology() -> Language {
    Language::parse(include_str!("../std/ontology.lang")).expect("std ontology must parse")
}

/// 便利:以 stdlib 本體 + 使用者 Language 建 registry(額外引用語意)。
pub fn with_std(user: &Language) -> (OntologyRegistry, Vec<OntologyDiag>) {
    let std = std_ontology();
    OntologyRegistry::build(&[&std, user])
}
