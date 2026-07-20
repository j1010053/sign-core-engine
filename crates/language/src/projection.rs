//! 四維 typed projection(步驟 12a;修補07 P39)。
//!
//! `sign.project(dim, &registry)` = 對 **`Defs` 的型別化解讀**——不另存一份易失
//! 同步的副本(P39:`Defs` 恆為單一資訊源)。一個維度視圖含:
//! - `categories`:sign 在該維的 `belongs` 閉包(ontology 分類,registry 提供);
//! - `defs`:**有效** dim-scoped Defs = 繼承(閉包祖先自帶 Def,ancestors-first)
//!   ⊕ 本地(本地覆蓋祖先,P6 後者勝 / 本地勝);同 path 後者勝。
//!
//! 修改不暴露可變 projection,走 typed patch `Sign × Patch → Sign'`(P39,保留
//! 原 Sign);patch 的資料欄位/介面於 12e 補全,行為留 M2 後。

use std::collections::BTreeMap;

use crate::ontology::OntologyRegistry;
use crate::{Dim, SignDef, SignItem};

/// 一個維度的型別化視圖(唯讀;對 `Defs` + registry 的解讀)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimProjection {
    pub dim: Dim,
    /// belongs 閉包(ontology 範疇;self-first、ancestors-first、去重)。
    pub categories: Vec<String>,
    /// 有效 dim-scoped Defs(繼承 ⊕ 本地;本地覆蓋,同 path 後者勝)。保插入序。
    pub defs: Vec<(String, String)>,
}

fn path_dim(path: &str) -> Option<Dim> {
    let head = path.split_once('.').map(|(h, _)| h).unwrap_or(path);
    Dim::parse(head)
}

/// sign 本地某維 Defs(路徑前綴 = dim;保插入序)。
fn local_dim_defs(sign: &SignDef, dim: Dim) -> Vec<(String, String)> {
    sign.items
        .iter()
        .filter_map(|it| match it {
            SignItem::Def(d) if path_dim(&d.path) == Some(dim) => {
                Some((d.path.clone(), d.value.clone()))
            }
            _ => None,
        })
        .collect()
}

/// 依「後者勝」把 (path,value) 序列壓成有效集(保最後一次出現的位置)。
fn last_wins(pairs: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut seen = BTreeMap::new();
    let mut keep = vec![false; pairs.len()];
    for (i, (p, _)) in pairs.iter().enumerate().rev() {
        if seen.insert(p.clone(), ()).is_none() {
            keep[i] = true;
        }
    }
    pairs
        .into_iter()
        .enumerate()
        .filter_map(|(i, kv)| keep[i].then_some(kv))
        .collect()
}

impl SignDef {
    /// 某維的 typed projection(對 `Defs` 的解讀 + registry 繼承)。
    /// **分類閉包維度中立**(P38 v0.2 單一樹);`defs` 才依維度過濾。
    pub fn project(&self, dim: Dim, reg: &OntologyRegistry) -> DimProjection {
        let categories = reg.sign_categories(self); // 維度中立
        // 繼承:閉包 self-first;範疇 Def 是「範疇預設」,越遠祖越先(越易被覆蓋)
        // → 反轉閉包令根祖在前、本地最後 → last_wins 讓本地與近祖勝出(P6)。
        let mut all: Vec<(String, String)> = Vec::new();
        for cat in categories.iter().rev() {
            if let Some(node) = reg.node(cat) {
                all.extend(
                    node.defs
                        .iter()
                        .filter(|(p, _)| path_dim(p) == Some(dim)) // 只取本維 Def
                        .cloned(),
                );
            }
        }
        all.extend(local_dim_defs(self, dim)); // 本地在最後 → 覆蓋
        DimProjection {
            dim,
            categories,
            defs: last_wins(all),
        }
    }

    /// 四維一次投影(便利)。
    pub fn project_all(&self, reg: &OntologyRegistry) -> [DimProjection; 4] {
        let [a, b, c, d] = Dim::all();
        [
            self.project(a, reg),
            self.project(b, reg),
            self.project(c, reg),
            self.project(d, reg),
        ]
    }
}

impl DimProjection {
    /// 有效 Def 查值(便利)。
    pub fn get(&self, path: &str) -> Option<&str> {
        self.defs
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, v)| v.as_str())
    }
    /// 是否屬某範疇(belongs 閉包成員)。
    pub fn is_a(&self, category: &str) -> bool {
        self.categories.iter().any(|c| c == category)
    }
}
