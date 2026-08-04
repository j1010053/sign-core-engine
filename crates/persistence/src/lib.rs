//! Host persistence for evolution graphs (P60/P64).
//!
//! The semantic crates remain filesystem-free. This crate owns the host
//! boundary:
//!
//! - canonical top-level language fragments and changesets live in a shared,
//!   content-addressed `objects/` directory (P60);
//! - every evolution node has `nodes/<id>/{manifest,edges,annotation/,config}`
//!   (P64);
//! - snapshot, edges and nativization are hash-in; annotation, config and
//!   labels are hash-out;
//! - loading always revalidates object hashes, exact source/identity pairing,
//!   node-v2 ids and replay/fsck before returning an [`EvolutionGraph`].

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use conlang_changeset::state::EvolutionState;
use conlang_changeset::evolution::{
    Edge, EvolutionError, EvolutionGraph, Nativization, NodeId, PersistedNode,
};
use conlang_language::{sha256_hex, Language, LanguageDocument, LibrarySpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const STORE_FORMAT: &str = "conlang-evolution-store/v1\n";
const SNAPSHOT_SCHEMA: &str = "conlang-snapshot-manifest/v1";
const EDGES_SCHEMA: &str = "conlang-node-edges/v1";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("PERSISTENCE_IO: {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("PERSISTENCE_FORMAT: {0}")]
    Format(String),
    #[error("PERSISTENCE_OBJECT_CORRUPT: expected {expected}, got {actual}")]
    ObjectCorrupt { expected: String, actual: String },
    #[error("PERSISTENCE_NODE_IMMUTABLE: {node} already stores different {field}")]
    ImmutableNode { node: String, field: &'static str },
    /// store 裡有這個節點,但傳進 `save` 的圖沒有它。
    ///
    /// `save` 是 append-only,**不替呼叫端刪東西**——若它會,一個只持有部分圖的
    /// 呼叫端就能不可逆地清空 store。但也不能默默忽略:那會讓「從圖裡移除節點
    /// → `save`」看起來成功,而下一次 `load` 又把它讀回來。故硬擋,並指向
    /// [`GraphStore::remove_node`]。
    #[error(
        "PERSISTENCE_STALE_NODE: {node} exists in the store but not in the graph; \
         call remove_node to delete it explicitly"
    )]
    StaleNode { node: String },
    /// 節點被 store 裡別的節點引用為 parent,不得移除(同 `EvolutionGraph` 側)。
    #[error("PERSISTENCE_NODE_HAS_DEPENDENTS: {node} is a parent of {dependent}")]
    NodeHasDependents { node: String, dependent: String },
    #[error("PERSISTENCE_PATH_INVALID: annotation path {0:?} is not relative and traversal-free")]
    InvalidAnnotationPath(PathBuf),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Identity(#[from] conlang_language::IdentityError),
    #[error(transparent)]
    Evolution(#[from] EvolutionError),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NodeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Hash-external host preferences only. The engine never reads these
    /// values while replaying or reconstructing a snapshot.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub preferences: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct GraphStore {
    root: PathBuf,
}

impl GraphStore {
    /// Initialize a store or reopen an existing store with the same schema.
    pub fn init(path: impl AsRef<Path>) -> Result<GraphStore, StoreError> {
        let root = path.as_ref().to_path_buf();
        create_dir_all(&root)?;
        let store = GraphStore { root };
        create_dir_all(&store.objects_dir())?;
        create_dir_all(&store.nodes_dir())?;
        let format = store.root.join("format");
        if format.exists() {
            store.validate_format()?;
        } else {
            write_new_file(&format, STORE_FORMAT.as_bytes())?;
        }
        Ok(store)
    }

    /// Open an initialized store without creating missing structure.
    pub fn open(path: impl AsRef<Path>) -> Result<GraphStore, StoreError> {
        let store = GraphStore {
            root: path.as_ref().to_path_buf(),
        };
        store.validate_format()?;
        require_directory(&store.objects_dir())?;
        require_directory(&store.nodes_dir())?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 已存節點的目錄清單(排序後,跳過 `.` 開頭的暫存目錄)。
    ///
    /// `save` 的過期檢查與 `load` 共用同一份列舉——兩邊若各寫一套,
    /// 「store 裡有什麼」就有兩個答案。
    fn stored_node_dirs(&self) -> Result<Vec<PathBuf>, StoreError> {
        let mut entries = fs::read_dir(self.nodes_dir()).map_err(|source| StoreError::Io {
            path: self.nodes_dir(),
            source,
        })?;
        let mut directories = Vec::new();
        while let Some(entry) = entries
            .next()
            .transpose()
            .map_err(|source| StoreError::Io {
                path: self.nodes_dir(),
                source,
            })?
        {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let file_type = entry.file_type().map_err(|source| StoreError::Io {
                path: entry.path(),
                source,
            })?;
            if !file_type.is_dir() {
                return Err(StoreError::Format(format!(
                    "unexpected non-directory under nodes/: {:?}",
                    name
                )));
            }
            directories.push(entry.path());
        }
        directories.sort();
        Ok(directories)
    }

    /// 已存節點的 id 清單。
    fn stored_node_ids(&self) -> Result<Vec<String>, StoreError> {
        self.stored_node_dirs()?
            .into_iter()
            .map(|directory| {
                directory
                    .file_name()
                    .and_then(OsStr::to_str)
                    .map(str::to_owned)
                    .ok_or_else(|| StoreError::Format("node directory is not UTF-8".to_owned()))
            })
            .collect()
    }

    /// Append every graph node to the content-addressed store.
    ///
    /// Existing immutable node files must match byte-for-byte. Config is the
    /// only file updated; arbitrary existing preferences and annotations are
    /// preserved while the graph's current label is synchronized.
    ///
    /// # append-only,但**不靜默**
    ///
    /// 本方法只新增,不刪除。若 store 裡有節點而傳進來的圖沒有,回
    /// [`StoreError::StaleNode`] 而非默默略過——後者會讓「從圖裡移除節點 →
    /// `save`」看起來成功,而 `load`(以 `nodes/` 的目錄內容為準)又把它讀回來。
    /// 要真的刪,呼叫 [`remove_node`](Self::remove_node)。
    ///
    /// 檢查在寫入**之前**做,故被擋下時 store 未被動過。
    pub fn save(&self, graph: &EvolutionGraph) -> Result<(), StoreError> {
        graph.verify_all()?;
        let known: std::collections::BTreeSet<&str> = graph.ids().map(NodeId::as_str).collect();
        for stored in self.stored_node_ids()? {
            if !known.contains(stored.as_str()) {
                return Err(StoreError::StaleNode { node: stored });
            }
        }
        for id in graph.ids() {
            let node = graph
                .node(id)
                .ok_or_else(|| EvolutionError::UnknownNode(id.clone()))?;
            let manifest = self.persist_snapshot(node.snapshot(), node.nativization())?;
            let edges = self.persist_edges(node.parents())?;
            let manifest_bytes = json_bytes(&manifest)?;
            let edges_bytes = json_bytes(&edges)?;
            let node_dir = self.node_dir(id);
            if node_dir.exists() {
                ensure_same(&node_dir.join("manifest"), &manifest_bytes, id, "manifest")?;
                ensure_same(&node_dir.join("edges"), &edges_bytes, id, "edges")?;
                let mut config = self.read_config(id)?;
                config.label = node.label().map(str::to_owned);
                atomic_write(&node_dir.join("config"), &json_bytes(&config)?)?;
                continue;
            }

            let temporary = self.nodes_dir().join(format!(".{}.tmp", id.as_str()));
            if temporary.exists() {
                remove_dir_all(&temporary)?;
            }
            create_dir_all(&temporary)?;
            write_new_file(&temporary.join("manifest"), &manifest_bytes)?;
            write_new_file(&temporary.join("edges"), &edges_bytes)?;
            write_new_file(
                &temporary.join("config"),
                &json_bytes(&NodeConfig {
                    label: node.label().map(str::to_owned),
                    preferences: BTreeMap::new(),
                })?,
            )?;
            create_dir_all(&temporary.join("annotation"))?;
            fs::rename(&temporary, &node_dir).map_err(|source| StoreError::Io {
                path: node_dir,
                source,
            })?;
        }
        Ok(())
    }

    /// Load and fsck every node currently stored in `nodes/`.
    ///
    /// `libraries` is deliberately injected by the host. Library locks inside
    /// `.chg` remain the authority for replay compatibility; no state-changing
    /// dependency is smuggled through hash-external P64 config.
    pub fn load(&self, libraries: LibrarySpec) -> Result<EvolutionGraph, StoreError> {
        let directories = self.stored_node_dirs()?;

        let mut records = Vec::with_capacity(directories.len());
        for directory in directories {
            let stored_id = directory
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or_else(|| StoreError::Format("node directory is not UTF-8".to_owned()))?;
            let id = NodeId::parse(stored_id)?;
            let manifest: SnapshotManifest = read_json(&directory.join("manifest"))?;
            if manifest.schema != SNAPSHOT_SCHEMA {
                return Err(StoreError::Format(format!(
                    "{} has snapshot schema {:?}",
                    id, manifest.schema
                )));
            }
            let edges: EdgeManifest = read_json(&directory.join("edges"))?;
            if edges.schema != EDGES_SCHEMA {
                return Err(StoreError::Format(format!(
                    "{} has edge schema {:?}",
                    id, edges.schema
                )));
            }
            let config: NodeConfig = read_json(&directory.join("config"))?;
            let snapshot = self.materialize_snapshot(&manifest)?;
            let parents = self.materialize_edges(&edges)?;
            records.push(PersistedNode {
                id,
                parents,
                snapshot,
                nativization: manifest.nativization.into(),
                label: config.label,
            });
        }
        Ok(EvolutionGraph::restore(libraries, records)?)
    }

    /// 從 store 刪掉一個節點的整個資料夾。**只有葉節點可刪**。
    ///
    /// 這是唯一的破壞性操作,故刻意做成顯式呼叫而非 `save` 的副作用
    /// (見 [`StoreError::StaleNode`])。
    ///
    /// - 若 store 裡還有節點以它為 parent(含引用邊),回
    ///   [`StoreError::NodeHasDependents`]——子節點的 id 由 parents 的 id 算出,
    ///   父節點消失後 `load` 會直接拒收(`PersistedParentMissing`);
    /// - `manifest`/`edges`/`config`/`state`/`annotation/` 一併刪除;
    /// - **`objects/` 不動**:它是內容定址且跨節點共用,刪掉會破壞別的節點。
    ///   孤兒 object 是無害的空間佔用,回收另計。
    ///
    /// 呼叫端通常要與 `EvolutionGraph::remove_node` 成對使用,否則下一次
    /// `save` 會把它寫回去。
    pub fn remove_node(&self, id: &NodeId) -> Result<(), StoreError> {
        let node_dir = self.node_dir(id);
        if !node_dir.exists() {
            return Err(StoreError::Format(format!("unknown node {id}")));
        }
        for directory in self.stored_node_dirs()? {
            if directory == node_dir {
                continue;
            }
            let edges: EdgeManifest = read_json(&directory.join("edges"))?;
            if edges.edges.iter().any(|edge| edge.from == id.as_str()) {
                let dependent = directory
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or("<non-utf8>")
                    .to_owned();
                return Err(StoreError::NodeHasDependents {
                    node: id.as_str().to_owned(),
                    dependent,
                });
            }
        }
        remove_dir_all(&node_dir)
    }

    /// 讀節點的 State(外部環境)。**雜湊外**,不存在時回預設空值。
    ///
    /// 裁定 (A):State 只在撰寫時被讀,**replay 不看它**——故它與
    /// `manifest`/`edges` 分檔、不進 node-v2 雜湊,可自由編輯而不影響
    /// 任何既有節點的重放產物。
    pub fn read_state(&self, id: &NodeId) -> Result<EvolutionState, StoreError> {
        let path = self.node_dir(id).join("state");
        if !path.exists() {
            return Ok(EvolutionState::default());
        }
        read_json(&path)
    }

    pub fn write_state(&self, id: &NodeId, state: &EvolutionState) -> Result<(), StoreError> {
        atomic_write(&self.node_dir(id).join("state"), &json_bytes(state)?)
    }

    pub fn read_config(&self, id: &NodeId) -> Result<NodeConfig, StoreError> {
        read_json(&self.node_dir(id).join("config"))
    }

    /// Replace hash-external node config. This never edits manifest or edges.
    pub fn write_config(&self, id: &NodeId, config: &NodeConfig) -> Result<(), StoreError> {
        require_node_dir(&self.node_dir(id))?;
        atomic_write(&self.node_dir(id).join("config"), &json_bytes(config)?)
    }

    /// Store an annotation under `annotation/`, rejecting absolute paths,
    /// `..`, prefixes and empty paths.
    pub fn write_annotation(
        &self,
        id: &NodeId,
        relative: impl AsRef<Path>,
        content: &[u8],
    ) -> Result<(), StoreError> {
        let relative = checked_relative(relative.as_ref())?;
        let root = self.node_dir(id).join("annotation");
        require_directory(&root)?;
        let target = annotation_target(&root, relative, true)?;
        atomic_write(&target, content)
    }

    pub fn read_annotation(
        &self,
        id: &NodeId,
        relative: impl AsRef<Path>,
    ) -> Result<Vec<u8>, StoreError> {
        let relative = checked_relative(relative.as_ref())?;
        let root = self.node_dir(id).join("annotation");
        require_directory(&root)?;
        read_bytes(&annotation_target(&root, relative, false)?)
    }

    pub fn list_annotations(&self, id: &NodeId) -> Result<Vec<PathBuf>, StoreError> {
        let root = self.node_dir(id).join("annotation");
        require_directory(&root)?;
        let mut files = Vec::new();
        collect_files(&root, &root, &mut files)?;
        files.sort();
        Ok(files)
    }

    fn validate_format(&self) -> Result<(), StoreError> {
        let path = self.root.join("format");
        let actual = read_bytes(&path)?;
        if actual != STORE_FORMAT.as_bytes() {
            return Err(StoreError::Format(format!(
                "unsupported store format in {}",
                path.display()
            )));
        }
        Ok(())
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn nodes_dir(&self) -> PathBuf {
        self.root.join("nodes")
    }

    fn node_dir(&self, id: &NodeId) -> PathBuf {
        self.nodes_dir().join(id.as_str())
    }

    fn persist_snapshot(
        &self,
        document: &LanguageDocument,
        nativization: Nativization,
    ) -> Result<SnapshotManifest, StoreError> {
        let language = document.language();
        let mut globals = Language::new();
        globals.dsl_decls.clone_from(&language.dsl_decls);
        globals.distribution.clone_from(&language.distribution);
        let globals = self.put_object(globals.dump().as_bytes())?;

        let mut canonical_traits = language.traits.iter().collect::<Vec<_>>();
        canonical_traits.sort_by(|left, right| {
            (!left.global, left.name.as_str()).cmp(&(!right.global, right.name.as_str()))
        });
        let mut traits = Vec::with_capacity(canonical_traits.len());
        for trait_def in canonical_traits {
            let mut fragment = Language::new();
            fragment.traits.push(trait_def.clone());
            traits.push(NamedObject {
                name: trait_def.name.clone(),
                object: self.put_object(fragment.dump().as_bytes())?,
            });
        }

        let mut signs = Vec::with_capacity(language.signs.len());
        for sign in &language.signs {
            let mut fragment = Language::new();
            fragment.signs.push(sign.clone());
            signs.push(NamedObject {
                name: sign.name.clone(),
                object: self.put_object(fragment.dump().as_bytes())?,
            });
        }
        signs.sort_by(|left, right| left.name.cmp(&right.name));

        let identities_source = document.manifest_json()?;
        let identities = self.put_object(identities_source.as_bytes())?;
        Ok(SnapshotManifest {
            schema: SNAPSHOT_SCHEMA.to_owned(),
            source_sha256: sha256_hex(document.source().as_bytes()),
            identity_sha256: sha256_hex(identities_source.as_bytes()),
            globals,
            traits,
            signs,
            identities,
            nativization: nativization.into(),
        })
    }

    fn persist_edges(&self, edges: &[Edge]) -> Result<EdgeManifest, StoreError> {
        let mut stored = Vec::with_capacity(edges.len());
        for edge in edges {
            stored.push(StoredEdge {
                from: edge.from.as_str().to_owned(),
                changeset: edge
                    .changeset
                    .as_ref()
                    .map(|source| self.put_object(source.as_bytes()))
                    .transpose()?,
            });
        }
        Ok(EdgeManifest {
            schema: EDGES_SCHEMA.to_owned(),
            edges: stored,
        })
    }

    fn materialize_snapshot(
        &self,
        manifest: &SnapshotManifest,
    ) -> Result<LanguageDocument, StoreError> {
        let globals = String::from_utf8(self.get_object(&manifest.globals)?)
            .map_err(|error| StoreError::Format(error.to_string()))?;
        let mut sections = Vec::new();
        if !globals.is_empty() {
            sections.push(globals);
        }
        for item in &manifest.traits {
            let fragment = String::from_utf8(self.get_object(&item.object)?)
                .map_err(|error| StoreError::Format(error.to_string()))?;
            validate_named_fragment(&fragment, &item.name, NamedKind::Trait)?;
            sections.push(fragment);
        }
        for item in &manifest.signs {
            let fragment = String::from_utf8(self.get_object(&item.object)?)
                .map_err(|error| StoreError::Format(error.to_string()))?;
            validate_named_fragment(&fragment, &item.name, NamedKind::Sign)?;
            sections.push(fragment);
        }
        let source = sections.join("\n");
        let canonical = Language::parse(&source)
            .map_err(|error| StoreError::Format(error.to_string()))?
            .dump();
        if canonical != source {
            return Err(StoreError::Format(
                "snapshot object order is not canonical".to_owned(),
            ));
        }
        let actual_source = sha256_hex(source.as_bytes());
        if actual_source != manifest.source_sha256 {
            return Err(StoreError::ObjectCorrupt {
                expected: manifest.source_sha256.clone(),
                actual: actual_source,
            });
        }
        let identities = String::from_utf8(self.get_object(&manifest.identities)?)
            .map_err(|error| StoreError::Format(error.to_string()))?;
        let actual_identity = sha256_hex(identities.as_bytes());
        if actual_identity != manifest.identity_sha256 {
            return Err(StoreError::ObjectCorrupt {
                expected: manifest.identity_sha256.clone(),
                actual: actual_identity,
            });
        }
        Ok(LanguageDocument::open(&source, &identities)?)
    }

    fn materialize_edges(&self, manifest: &EdgeManifest) -> Result<Vec<Edge>, StoreError> {
        let mut edges = Vec::with_capacity(manifest.edges.len());
        for edge in &manifest.edges {
            let from = NodeId::parse(edge.from.clone())?;
            let changeset = edge
                .changeset
                .as_ref()
                .map(|object| {
                    String::from_utf8(self.get_object(object)?)
                        .map_err(|error| StoreError::Format(error.to_string()))
                })
                .transpose()?;
            edges.push(Edge { from, changeset });
        }
        Ok(edges)
    }

    fn put_object(&self, content: &[u8]) -> Result<String, StoreError> {
        let hash = sha256_hex(content);
        let target = self.objects_dir().join(&hash);
        if target.exists() {
            let existing = read_bytes(&target)?;
            let actual = sha256_hex(&existing);
            if actual != hash || existing != content {
                return Err(StoreError::ObjectCorrupt {
                    expected: hash,
                    actual,
                });
            }
            return Ok(hash);
        }
        let temporary = self.objects_dir().join(format!(".{hash}.tmp"));
        if temporary.exists() {
            remove_file(&temporary)?;
        }
        write_new_file(&temporary, content)?;
        match fs::rename(&temporary, &target) {
            Ok(()) => Ok(hash),
            Err(_) if target.exists() => {
                remove_file(&temporary)?;
                let existing = read_bytes(&target)?;
                let actual = sha256_hex(&existing);
                if actual == hash && existing == content {
                    Ok(hash)
                } else {
                    Err(StoreError::ObjectCorrupt {
                        expected: hash,
                        actual,
                    })
                }
            }
            Err(source) => Err(StoreError::Io {
                path: target,
                source,
            }),
        }
    }

    fn get_object(&self, hash: &str) -> Result<Vec<u8>, StoreError> {
        validate_hash(hash)?;
        let content = read_bytes(&self.objects_dir().join(hash))?;
        let actual = sha256_hex(&content);
        if actual != hash {
            return Err(StoreError::ObjectCorrupt {
                expected: hash.to_owned(),
                actual,
            });
        }
        Ok(content)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotManifest {
    schema: String,
    source_sha256: String,
    identity_sha256: String,
    globals: String,
    traits: Vec<NamedObject>,
    signs: Vec<NamedObject>,
    identities: String,
    nativization: StoredNativization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NamedObject {
    name: String,
    object: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredNativization {
    None,
    Pidgin,
    Creole { generation: u32 },
}

impl From<Nativization> for StoredNativization {
    fn from(value: Nativization) -> StoredNativization {
        match value {
            Nativization::None => StoredNativization::None,
            Nativization::Pidgin => StoredNativization::Pidgin,
            Nativization::Creole { generation } => StoredNativization::Creole { generation },
        }
    }
}

impl From<StoredNativization> for Nativization {
    fn from(value: StoredNativization) -> Nativization {
        match value {
            StoredNativization::None => Nativization::None,
            StoredNativization::Pidgin => Nativization::Pidgin,
            StoredNativization::Creole { generation } => Nativization::Creole { generation },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EdgeManifest {
    schema: String,
    edges: Vec<StoredEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredEdge {
    from: String,
    changeset: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum NamedKind {
    Trait,
    Sign,
}

fn validate_named_fragment(
    source: &str,
    expected: &str,
    kind: NamedKind,
) -> Result<(), StoreError> {
    let parsed = Language::parse(source).map_err(|error| StoreError::Format(error.to_string()))?;
    let actual = match kind {
        NamedKind::Trait if parsed.traits.len() == 1 && parsed.signs.is_empty() => {
            Some(parsed.traits[0].name.as_str())
        }
        NamedKind::Sign if parsed.signs.len() == 1 && parsed.traits.is_empty() => {
            Some(parsed.signs[0].name.as_str())
        }
        _ => None,
    };
    if actual != Some(expected) {
        return Err(StoreError::Format(format!(
            "{kind:?} object is not the declared {expected:?}"
        )));
    }
    Ok(())
}

fn validate_hash(hash: &str) -> Result<(), StoreError> {
    if hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(StoreError::Format(format!("invalid object hash {hash:?}")))
    }
}

fn checked_relative(path: &Path) -> Result<&Path, StoreError> {
    let mut any = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => any = true,
            _ => return Err(StoreError::InvalidAnnotationPath(path.to_path_buf())),
        }
    }
    if !any {
        return Err(StoreError::InvalidAnnotationPath(path.to_path_buf()));
    }
    Ok(path)
}

fn annotation_target(
    root: &Path,
    relative: &Path,
    create_parents: bool,
) -> Result<PathBuf, StoreError> {
    let root_metadata = fs::symlink_metadata(root).map_err(|source| StoreError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    if root_metadata.file_type().is_symlink() {
        return Err(StoreError::Format(format!(
            "annotation root is a symlink: {}",
            root.display()
        )));
    }
    let components = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value),
            _ => Err(StoreError::InvalidAnnotationPath(relative.to_path_buf())),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut current = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let is_target = index + 1 == components.len();
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::Format(format!(
                    "annotation path crosses symlink {}",
                    current.display()
                )));
            }
            Ok(metadata) if !is_target && !metadata.is_dir() => {
                return Err(StoreError::Format(format!(
                    "annotation parent {} is not a directory",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !is_target && create_parents {
                    fs::create_dir(&current).map_err(|source| StoreError::Io {
                        path: current.clone(),
                        source,
                    })?;
                }
            }
            Err(source) => {
                return Err(StoreError::Io {
                    path: current,
                    source,
                });
            }
        }
    }
    Ok(current)
}

fn json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, StoreError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, StoreError> {
    Ok(serde_json::from_slice(&read_bytes(path)?)?)
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, StoreError> {
    let mut file = File::open(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)
        .map_err(|source| StoreError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(content)
}

fn write_new_file(path: &Path, content: &[u8]) -> Result<(), StoreError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| StoreError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(content).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), StoreError> {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| StoreError::Format("target filename is not UTF-8".to_owned()))?;
    let temporary = path.with_file_name(format!(".{name}.tmp"));
    if temporary.exists() {
        remove_file(&temporary)?;
    }
    write_new_file(&temporary, content)?;
    if path.exists() {
        remove_file(path)?;
    }
    fs::rename(&temporary, path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn ensure_same(
    path: &Path,
    expected: &[u8],
    id: &NodeId,
    field: &'static str,
) -> Result<(), StoreError> {
    if read_bytes(path)? == expected {
        Ok(())
    } else {
        Err(StoreError::ImmutableNode {
            node: id.as_str().to_owned(),
            field,
        })
    }
}

fn create_dir_all(path: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn require_directory(path: &Path) -> Result<(), StoreError> {
    let metadata = fs::metadata(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(StoreError::Format(format!(
            "{} is not a directory",
            path.display()
        )))
    }
}

fn require_node_dir(path: &Path) -> Result<(), StoreError> {
    require_directory(path)
}

fn remove_file(path: &Path) -> Result<(), StoreError> {
    fs::remove_file(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn remove_dir_all(path: &Path) -> Result<(), StoreError> {
    fs::remove_dir_all(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn collect_files(root: &Path, current: &Path, output: &mut Vec<PathBuf>) -> Result<(), StoreError> {
    let mut entries = fs::read_dir(current).map_err(|source| StoreError::Io {
        path: current.to_path_buf(),
        source,
    })?;
    while let Some(entry) = entries
        .next()
        .transpose()
        .map_err(|source| StoreError::Io {
            path: current.to_path_buf(),
            source,
        })?
    {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_symlink() {
            return Err(StoreError::Format(format!(
                "annotation tree contains symlink {}",
                path.display()
            )));
        }
        if file_type.is_dir() {
            collect_files(root, &path, output)?;
        } else if file_type.is_file() {
            output.push(
                path.strip_prefix(root)
                    .map_err(|error| StoreError::Format(error.to_string()))?
                    .to_path_buf(),
            );
        } else {
            return Err(StoreError::Format(format!(
                "annotation tree contains unsupported entry {}",
                path.display()
            )));
        }
    }
    Ok(())
}
