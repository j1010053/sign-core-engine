//! 步驟 16 ①② —— **演化圖節點 + memoize replay**(docs/06 §1、§5)。
//!
//! 演化圖 = 有向圖,拓撲**預設為樹(單親)**,允許多親(融合)。節點**只在四種情形
//! 結晶**(分叉/接觸/融合/使用者釘選),其餘語言狀態不持久化——
//! **狀態永遠是 ChangeSet 的函數**(docs/06 §5,單一資訊源)。
//!
//! ## 為什麼要 memoize
//!
//! 即時 replay 下,看第 N 代語言得從 root 一路重跑所有祖先的 changeset;而演化會讓
//! 文件變大,**跑最多次的深節點也正好每筆最貴**。實測 @1000 signs 約 26 ms/編輯,
//! 一棵中等的樹就是分鐘級。docs/06 §5 早已把 memoize 列為【M+】並指明
//! 「與 lazy reparse 同構,實作思路復用」——本模組即照 `ChangeSession` 的
//! `dirty` + `Option<cached>` + 計數器樣板實作。
//!
//! ## 節點的 changeset 存**原文**不存解析結果
//!
//! `.chg` 的 prelude 釘著 `base_source`/`base_identities` 三道 digest,而 base 是
//! **parent 的求值結果**——所以必須在 replay 當下、拿到 parent 文件後才能 resolve。
//! 存原文也讓「改一個節點的 changeset」成為單純的字串替換。

use crate::{ChangeInterpreter, LanguageDocument, ReplayError, UnresolvedChangeSet};
use conlang_language::LibrarySpec;
use std::collections::{BTreeMap, BTreeSet};

/// 節點識別(使用者可讀且穩定;非 P26 的序列 id——那是 Language 內部節點用的)。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub String);

impl std::fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// 演化圖的一個節點(docs/06 §1)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageNode {
    /// 預設長度 1;**多親是融合(克里奧爾),非特例結構**。空 = 直接掛在 root。
    pub parents: Vec<NodeId>,
    /// 相對 parent(們)的 `.chg` **原文**(見模組說明)。
    pub changeset: String,
    /// 使用者釘選/命名(結晶四情形之一)。
    pub pin: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum EvolutionError {
    #[error("EVOLUTION_UNKNOWN_NODE: {0}")]
    UnknownNode(NodeId),
    /// 引用圖必須無環(docs/06 §2 v0.1.1 無環約束)。
    #[error("EVOLUTION_CYCLE: {0} takes part in a parent cycle")]
    Cycle(NodeId),
    #[error("EVOLUTION_DUPLICATE: {0} already exists")]
    Duplicate(NodeId),
    #[error(transparent)]
    Replay(#[from] ReplayError),
}

/// 演化圖 + **memoize 的 replay**。
///
/// 快取語意(docs/06 §5【M+】):快取節點求值結果;**parent 變動時沿依賴邊標 stale**。
/// `replay_count` 用來佐證快取真的生效(比照 `ChangeSession::compile_count`)——
/// 沒有它,「有沒有快取」在測試裡看不出來。
#[derive(Debug)]
pub struct EvolutionGraph {
    root: LanguageDocument,
    libraries: LibrarySpec,
    nodes: BTreeMap<NodeId, LanguageNode>,
    cache: BTreeMap<NodeId, LanguageDocument>,
    replay_count: u64,
}

impl EvolutionGraph {
    /// root 是圖的起點語言;它自己沒有 changeset。
    pub fn new(root: LanguageDocument, libraries: LibrarySpec) -> EvolutionGraph {
        EvolutionGraph {
            root,
            libraries,
            nodes: BTreeMap::new(),
            cache: BTreeMap::new(),
            replay_count: 0,
        }
    }

    pub fn root(&self) -> &LanguageDocument {
        &self.root
    }

    pub fn node(&self, id: &NodeId) -> Option<&LanguageNode> {
        self.nodes.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &NodeId> {
        self.nodes.keys()
    }

    /// **實際跑過幾次 replay**(不含快取命中)。測試用來證明 memoize 生效。
    pub fn replay_count(&self) -> u64 {
        self.replay_count
    }

    pub fn is_cached(&self, id: &NodeId) -> bool {
        self.cache.contains_key(id)
    }

    /// 加一個節點。parent 必須已存在,且不得成環。
    pub fn insert(&mut self, id: NodeId, node: LanguageNode) -> Result<(), EvolutionError> {
        if self.nodes.contains_key(&id) {
            return Err(EvolutionError::Duplicate(id));
        }
        for parent in &node.parents {
            if !self.nodes.contains_key(parent) {
                return Err(EvolutionError::UnknownNode(parent.clone()));
            }
        }
        self.nodes.insert(id.clone(), node);
        // 新節點本身不可能製造環(parent 都是既有節點),但仍做一次總檢查,
        // 讓不變式由**一處**保證,而不是靠推理。
        self.check_acyclic()?;
        Ok(())
    }

    /// 改一個節點的 changeset。**該節點與其所有後代**的快取失效(祖先不受影響)。
    pub fn set_changeset(
        &mut self,
        id: &NodeId,
        changeset: impl Into<String>,
    ) -> Result<(), EvolutionError> {
        let node = self
            .nodes
            .get_mut(id)
            .ok_or_else(|| EvolutionError::UnknownNode(id.clone()))?;
        node.changeset = changeset.into();
        self.invalidate(id);
        Ok(())
    }

    /// 標記 `id` 與其**所有後代**為 stale(docs/06 §5「沿依賴邊標 stale」)。
    /// 依賴圖是局部的(§3.4「只引用直接入邊」),故 stale 傳播不失控。
    fn invalidate(&mut self, id: &NodeId) {
        let mut stale = BTreeSet::new();
        let mut frontier = vec![id.clone()];
        while let Some(current) = frontier.pop() {
            if !stale.insert(current.clone()) {
                continue;
            }
            for (candidate, node) in &self.nodes {
                if node.parents.contains(&current) && !stale.contains(candidate) {
                    frontier.push(candidate.clone());
                }
            }
        }
        self.cache.retain(|key, _| !stale.contains(key));
    }

    /// 求一個節點的語言狀態。**memoize**:命中快取就不重跑。
    ///
    /// 多親的 MVP 語意(docs/06 §5 v0.1.1):以 **`parents[0]` 為 replay 主幹**,
    /// 其餘 parent 僅供 ChangeSet 條目引用取材;完整融合 replay 屬【M+】。
    pub fn resolve(&mut self, id: &NodeId) -> Result<LanguageDocument, EvolutionError> {
        // 快取命中不需要特別的 early return:`ancestor_chain` 遇到已快取的節點就
        // 停止上溯,迴圈又對已快取者 `continue`,故命中時一次 replay 都不會跑。
        // (曾有一個 early return,但它在行為上完全冗餘、也無法被 `replay_count`
        // 觀測到——刪掉比留一個測不到的分支好。)
        if !self.nodes.contains_key(id) {
            return Err(EvolutionError::UnknownNode(id.clone()));
        }
        // 先把**祖先鏈**由淺到深排出來,再逐一求值——用迴圈而非遞迴,避免深樹爆棧。
        let chain = self.ancestor_chain(id)?;
        for current in chain {
            if self.cache.contains_key(&current) {
                continue;
            }
            let node = self.nodes.get(&current).expect("chain holds known nodes");
            let base = match node.parents.first() {
                Some(parent) => self
                    .cache
                    .get(parent)
                    .cloned()
                    .expect("ancestors are resolved before their children"),
                None => self.root.clone(),
            };
            let document = self.replay_one(&base, &node.changeset.clone())?;
            self.replay_count += 1;
            self.cache.insert(current, document);
        }
        Ok(self.cache.get(id).expect("just resolved").clone())
    }

    /// 由 root 到 `id` 的主幹鏈(不含已快取者以外的分支),淺 → 深。
    fn ancestor_chain(&self, id: &NodeId) -> Result<Vec<NodeId>, EvolutionError> {
        let mut chain = Vec::new();
        let mut current = Some(id.clone());
        let mut seen = BTreeSet::new();
        while let Some(node_id) = current {
            if !seen.insert(node_id.clone()) {
                return Err(EvolutionError::Cycle(node_id));
            }
            let node = self
                .nodes
                .get(&node_id)
                .ok_or_else(|| EvolutionError::UnknownNode(node_id.clone()))?;
            chain.push(node_id.clone());
            // 已快取的祖先就是求值的起點,不必再往上走。
            // **這是走訪成本的優化,不是正確性機制**——迴圈本來就會 `continue`
            // 掉已快取者,拿掉這個 break 結果一樣、replay 次數也一樣,只是深鏈
            // 每次都要白走 O(depth)。故 mutation testing 觀測不到它(誠實標記)。
            if self.cache.contains_key(&node_id) {
                break;
            }
            current = node.parents.first().cloned();
        }
        chain.reverse();
        Ok(chain)
    }

    fn replay_one(
        &self,
        base: &LanguageDocument,
        changeset: &str,
    ) -> Result<LanguageDocument, EvolutionError> {
        let parsed = UnresolvedChangeSet::parse(changeset)?;
        let namespace = parsed.namespace.clone();
        let resolved = parsed.resolve(base, &self.libraries)?;
        let interpreter = ChangeInterpreter::new(base.clone(), self.libraries.clone(), namespace)?;
        Ok(interpreter.run(&resolved)?.document)
    }

    /// 無環約束(docs/06 §2 v0.1.1)。
    fn check_acyclic(&self) -> Result<(), EvolutionError> {
        let mut state: BTreeMap<&NodeId, u8> = BTreeMap::new();
        for id in self.nodes.keys() {
            if let Err(cycle) = self.walk(id, &mut state) {
                return Err(EvolutionError::Cycle(cycle));
            }
        }
        Ok(())
    }

    fn walk<'a>(
        &'a self,
        id: &'a NodeId,
        state: &mut BTreeMap<&'a NodeId, u8>,
    ) -> Result<(), NodeId> {
        match state.get(id) {
            Some(1) => return Err(id.clone()),
            Some(2) => return Ok(()),
            _ => {}
        }
        state.insert(id, 1);
        if let Some(node) = self.nodes.get(id) {
            for parent in &node.parents {
                if let Some((key, _)) = self.nodes.get_key_value(parent) {
                    self.walk(key, state)?;
                }
            }
        }
        state.insert(id, 2);
        Ok(())
    }
}
