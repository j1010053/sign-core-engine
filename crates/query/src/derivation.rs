//! 衍生家族:**兩張圖的接合**,不是一次遍歷。
//!
//! 這是流 D 框架 §1.1 記下的那個坑。零件都在,但定址空間不同:
//!
//! | 圖 | 承載 | 範圍 | 端點型別 |
//! |---|---|---|---|
//! | **sign 世系** | `SignDef::origin()` | **跨 sign** | `SignRef`(sign 名) |
//! | **義項衍生** | `SignItem::SenseEdge { to, from }` | **單一 sign 內部** | 義項名 |
//!
//! `SemNode::of_sign` 只走 `reg.effective_sign(sign).items`,所以 `SenseEdge`
//! 永遠不會跨出一個 sign。把兩者寫成一次遍歷,會得到「同 sign 內看得到、
//! 跨 sign 斷掉」這種只有部分綠的假象。
//!
//! 故本模組明確分兩層:[`DerivationNode`] 是跨 sign 的世系節點,
//! 其 `senses` 才掛該 sign 內部的義項邊。

use conlang_language::sem::SemNode;
use conlang_language::{CompiledSystem, DerivationKind, SenseTransparency, SignDef};
use std::collections::BTreeSet;

/// 一條**義項內部**的衍生邊(同一個 sign 之內)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenseLink {
    pub to: String,
    pub from: String,
    pub kind: DerivationKind,
    /// `Opaque` 表示已語法化/詞彙化——由 `lexicalize_sense` 翻設(P16)。
    pub transparency: SenseTransparency,
}

/// 世系上的一個 sign。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationNode {
    pub name: String,
    /// 這個 sign 衍生自誰(`origin`)。root 為 `None`。
    pub origin: Option<String>,
    pub underlying_form: Option<String>,
    pub gloss: Option<String>,
    /// **這個 sign 內部**的義項衍生邊。
    pub senses: Vec<SenseLink>,
}

/// 一個 sign 的衍生家族。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationDag {
    /// 查詢的起點。
    pub root: String,
    /// 家族成員,依 sign 名排序(決定性)。含起點自身。
    pub nodes: Vec<DerivationNode>,
    /// 被 `origin` 指到、但這個 Language 裡查無此 sign 的名字。
    ///
    /// **不靜默丟棄**:跨語言節點的借詞來源、或已被刪掉的祖先,都會落在這裡。
    /// 吞掉它們會讓斷掉的世系看起來像完整的。
    pub dangling_origins: Vec<String>,
}

fn node_of(sign: &SignDef, system: &CompiledSystem) -> DerivationNode {
    let semantics = SemNode::of_sign(sign, &system.ontology);
    DerivationNode {
        name: sign.name.clone(),
        origin: sign.origin().map(|reference| reference.0),
        underlying_form: sign.underlying_form().map(str::to_owned),
        gloss: semantics.field("gloss").map(str::to_owned),
        senses: semantics
            .edges
            .iter()
            .map(|edge| SenseLink {
                to: edge.to.clone(),
                from: edge.from.clone(),
                kind: edge.kind,
                transparency: edge.transparency,
            })
            .collect(),
    }
}

/// 一個 sign 的衍生家族:沿 `origin` **雙向**走到底(祖先 + 後代)。
///
/// 只走祖先會漏掉兄弟——`bake` 與 `baker` 都源自 `bake` 的話,從 `baker` 查
/// 家族卻看不到其他派生詞,那不是「家族」。
///
/// 起點不存在時回空家族(`nodes` 為空),不是錯誤:UI 端「查一個還沒建的詞」
/// 是正常操作。
pub fn derivation_family(system: &CompiledSystem, sign_name: &str) -> DerivationDag {
    let signs = &system.effective_language().signs;
    let find = |name: &str| signs.iter().find(|sign| sign.name == name);

    let mut members: BTreeSet<String> = BTreeSet::new();
    let mut dangling: BTreeSet<String> = BTreeSet::new();

    if find(sign_name).is_some() {
        members.insert(sign_name.to_owned());
        // 往上:沿 origin 鏈。`members.insert` 的回傳值兼作環偵測——**第二道防線**,
        // 第一道是編譯期的 `META_ORIGIN_CYCLE`(故正常路徑上不可達)。
        let mut cursor = sign_name.to_owned();
        while let Some(parent) = find(&cursor).and_then(SignDef::origin).map(|r| r.0) {
            if find(&parent).is_none() {
                // 限定名(`pkg::sign`)不受本地存在檢查,家族斷在這裡
                dangling.insert(parent);
                break;
            }
            if !members.insert(parent.clone()) {
                break; // 已收過 ⇒ 成環
            }
            cursor = parent;
        }

        // 往下:反覆掃,收 origin 落在已知成員裡的 sign,直到不再長大。
        loop {
            let grown: Vec<String> = signs
                .iter()
                .filter(|sign| !members.contains(&sign.name))
                .filter(|sign| {
                    sign.origin()
                        .map(|r| members.contains(&r.0))
                        .unwrap_or(false)
                })
                .map(|sign| sign.name.clone())
                .collect();
            if grown.is_empty() {
                break;
            }
            members.extend(grown);
        }
    }

    // 家族成員自己的 origin 若查無,也要現形
    for name in &members {
        if let Some(sign) = find(name) {
            if let Some(parent) = sign.origin() {
                if find(&parent.0).is_none() {
                    dangling.insert(parent.0);
                }
            }
        }
    }

    DerivationDag {
        root: sign_name.to_owned(),
        nodes: members
            .iter()
            .filter_map(|name| find(name))
            .map(|sign| node_of(sign, system))
            .collect(),
        dangling_origins: dangling.into_iter().collect(),
    }
}
