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

// Unsafe code is denied crate-wide and allowed only inside the minimal
// Windows `ReplaceFileW` wrapper below. `windows-sys` exposes raw FFI, while
// the public persistence API remains safe.
#![deny(unsafe_code)]
#![deny(missing_debug_implementations)]

use conlang_changeset::evolution::{
    Edge, EvolutionError, EvolutionGraph, Nativization, NodeId, PersistedNode,
};
use conlang_changeset::state::EvolutionState;
use conlang_language::{
    sha256_hex, Language, LanguageDocument, LibraryCatalog, LibraryId, LibraryLoadError,
    LibrarySpec, PackageFile, PackageId, PackageLayer, PackageRequirement, PackageResolver,
    PackageSource, PackageSources, PackageSpec, ResolvedPackages, SelectedPackage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const STORE_FORMAT: &str = "conlang-evolution-store/v1\n";
const SNAPSHOT_SCHEMA: &str = "conlang-snapshot-manifest/v1";
const EDGES_SCHEMA: &str = "conlang-node-edges/v1";
pub const PACKAGES_LOCK_SCHEMA: &str = "conlang-packages-lock/v1";

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
    #[error("PERSISTENCE_PACKAGE_ID_INVALID: {0:?} is not a <namespace>:<name> package id")]
    InvalidPackageId(String),
    #[error("PERSISTENCE_PACKAGE_REQUIREMENT_INVALID: {0:?}")]
    InvalidPackageRequirement(String),
    #[error(
        "PERSISTENCE_PACKAGE_PATH_INVALID: {manifest}: {field} path {path:?} must be relative and traversal-free"
    )]
    InvalidPackagePath {
        manifest: PathBuf,
        field: &'static str,
        path: String,
    },
    #[error("PERSISTENCE_PACKAGE_UTF8: package file {0} is not UTF-8")]
    PackageFileNotUtf8(PathBuf),
    #[error(
        "PERSISTENCE_PACKAGE_LOCK_MISMATCH: {package} field {field} expected {expected:?}, got {actual:?}"
    )]
    PackageLockMismatch {
        package: String,
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("PERSISTENCE_PROJECT_FORMAT: project.toml: {0}")]
    ProjectFormat(String),
    #[error("PERSISTENCE_VIEW_NAME_INVALID: {0:?} must be a single path segment")]
    InvalidViewName(String),
    #[error("PERSISTENCE_PATH_INVALID: annotation path {0:?} is not relative and traversal-free")]
    InvalidAnnotationPath(PathBuf),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Identity(#[from] conlang_language::IdentityError),
    #[error(transparent)]
    Evolution(#[from] EvolutionError),
    #[error(transparent)]
    PackageLoad(#[from] LibraryLoadError),
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

/// 一個視角檔的**資料層**表示(`views/<name>.json`,R4 一套一檔)。
///
/// 本型別刻意只有**資料**,沒有任何應用層語意:它不知道 `GroupingOverride`
/// 是什麼,也不知道 `ViewCommand`。翻譯是 app 的事(裁定 2026-08-04)。
///
/// 欄位對應流 D 框架 §4.2 的三段管線:
/// - `sort` → 呈現設定(不影響入選集合);
/// - `assignments` → **分類指派**(D-f2:不是 merge/split,故不可能衝突);
/// - `labels` → 顯示名(不影響群組身分)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    /// node id → group id。**sparse**:未列者用 strategy 算出的結果。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assignments: BTreeMap<String, String>,
    /// group id → 顯示名。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

/// 專案宣告檔(`project.toml`,R3 的 import 表)。
///
/// # 為什麼需要它
///
/// `GraphStore::load` 要一個 `LibrarySpec`,但此前 store 裡**沒有任何檔案記錄
/// 這個專案需要哪些套件**,呼叫端只能傳 `LibrarySpec::default()`
/// (`natural: None`、`plugins: []`)。用了 `natural:en-standard` 的專案因此
/// 開不起來,而錯誤訊息還說「add it to your import table」——**指向一個不存在
/// 的表**(R13 的措辭當時就預設了 R3)。
///
/// # 為什麼不能從 `.chg` 的 library lock 反推
///
/// 每條主幹邊的 prelude 都有 `library <pkg>@<ver> sha256:`,故有演化的專案
/// 確實 recoverable。但:
///
/// - **只有一個 root 的專案沒有任何 `.chg`** → 零資訊,而那正是最需要它的時候;
/// - lock 是 **replay 產物**(當時用了什麼),import 表是**意圖**(這個專案要
///   什麼)。拿產物反推意圖,就表達不了「想加一個套件但還沒用到」。
///
/// # 為什麼是 TOML
///
/// R3 附則:依「功能與編輯方」分——人編輯的宣告檔用 TOML,機器產生的結果用
/// JSON。本檔是人寫的。
///
/// # `packages.lock.json` 為什麼不在這裡
///
/// **鎖只在套件來自二進位之外時才有工作做。** 目前所有套件都是 `include_str!`
/// 內嵌的,解析結果完全由引擎版本決定,不存在「同一份宣告在不同機器解出不同
/// 版本」。等 R9-a 的注入入口真的被 host 用來讀磁碟套件,再做。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 預設開哪個 `views/<name>.json`(R1)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_view: Option<String>,
    #[serde(default)]
    pub packages: ProjectPackages,
    /// Project-level manual sampling overrides. These are authoring data, not
    /// language snapshots, and therefore never participate in node identity.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub weights: BTreeMap<String, f64>,
}

/// import 表本體。**只宣告直接依賴**;遞移依賴由各 package 的 `requires`
/// 展開,走既有的 `LibraryCatalog::visit()`(R3 ①)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ProjectPackages {
    /// Package-loader v2 roots.  Requirements may be unversioned
    /// (`catalog:case`) or exact (`catalog:case@1.0.0`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<String>,
    /// Project-local qualifier to package-id mappings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aliases: BTreeMap<String, String>,
    /// Legacy package-loader v1 fields.  They remain readable while old
    /// projects migrate, but must never be mixed with v2 intent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub std: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProjectPackages {
    roots: Option<Vec<String>>,
    aliases: Option<BTreeMap<String, String>>,
    std: Option<Vec<String>>,
    natural: Option<String>,
    plugins: Option<Vec<String>>,
}

impl<'de> Deserialize<'de> for ProjectPackages {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawProjectPackages::deserialize(deserializer)?;
        let declares_v2 = raw.roots.is_some() || raw.aliases.is_some();
        let declares_legacy = raw.std.is_some() || raw.natural.is_some() || raw.plugins.is_some();
        if declares_v2 && declares_legacy {
            return Err(serde::de::Error::custom(
                "legacy std/natural/plugins cannot be combined with v2 roots/aliases",
            ));
        }
        Ok(Self {
            roots: raw.roots.unwrap_or_default(),
            aliases: raw.aliases.unwrap_or_default(),
            std: raw.std.unwrap_or_default(),
            natural: raw.natural,
            plugins: raw.plugins.unwrap_or_default(),
        })
    }
}

impl ProjectDocument {
    /// 由既有的 `LibrarySpec` 產生一份宣告(存檔用)。
    pub fn from_spec(spec: &LibrarySpec) -> ProjectDocument {
        ProjectDocument {
            packages: ProjectPackages {
                std: spec.std.iter().map(ToString::to_string).collect(),
                natural: spec.natural.as_ref().map(ToString::to_string),
                plugins: spec.plugins.iter().map(ToString::to_string).collect(),
                ..ProjectPackages::default()
            },
            ..ProjectDocument::default()
        }
    }

    /// Create a project declaration from package-loader v2 intent.
    pub fn from_package_spec(spec: &PackageSpec) -> ProjectDocument {
        ProjectDocument {
            packages: ProjectPackages {
                roots: spec.roots.iter().map(ToString::to_string).collect(),
                aliases: spec
                    .aliases
                    .iter()
                    .map(|(alias, package)| (alias.clone(), package.to_string()))
                    .collect(),
                ..ProjectPackages::default()
            },
            ..ProjectDocument::default()
        }
    }

    /// 翻成 `LibrarySpec`。
    ///
    /// **`std` 為空表示「不載入任何 std」,不是「用預設」。** 兩者必須分得開,
    /// 否則 R12 剛做出來的「不載入 `std:grambank`」就表達不了——而那正是
    /// 裁定 S 的重點:特權是一份**可覆寫**的預設值。
    /// 「用預設」由**檔案不存在**表達(見 [`GraphStore::read_project`])。
    pub fn to_spec(&self) -> Result<LibrarySpec, StoreError> {
        self.packages.validate_format()?;
        if self.packages.has_v2() {
            return Err(StoreError::ProjectFormat(
                "v2 package roots/aliases cannot be represented as a legacy LibrarySpec".to_owned(),
            ));
        }
        let parse = |text: &str| {
            text.parse::<LibraryId>()
                .map_err(|_| StoreError::InvalidPackageId(text.to_owned()))
        };
        Ok(LibrarySpec {
            std: self
                .packages
                .std
                .iter()
                .map(|id| parse(id))
                .collect::<Result<_, _>>()?,
            natural: self.packages.natural.as_deref().map(parse).transpose()?,
            plugins: self
                .packages
                .plugins
                .iter()
                .map(|id| parse(id))
                .collect::<Result<_, _>>()?,
        })
    }

    /// Translate either a v2 declaration or a legacy declaration into the
    /// open-namespace package intent consumed by the v2 resolver.
    pub fn to_package_spec(&self) -> Result<PackageSpec, StoreError> {
        self.packages.validate_format()?;
        if !self.packages.has_v2() {
            return Ok(PackageSpec::from_legacy(&self.to_spec()?));
        }

        let roots = self
            .packages
            .roots
            .iter()
            .map(|requirement| {
                requirement
                    .parse::<PackageRequirement>()
                    .map_err(|_| StoreError::InvalidPackageRequirement(requirement.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut spec = PackageSpec {
            roots,
            aliases: BTreeMap::new(),
        };
        for (alias, package) in &self.packages.aliases {
            let id = package
                .parse::<PackageId>()
                .map_err(|_| StoreError::InvalidPackageId(package.clone()))?;
            spec = spec
                .with_alias(alias.clone(), id)
                .map_err(|error| StoreError::ProjectFormat(error.to_string()))?;
        }
        Ok(spec)
    }
}

impl ProjectPackages {
    fn has_v2(&self) -> bool {
        !self.roots.is_empty() || !self.aliases.is_empty()
    }

    fn has_legacy(&self) -> bool {
        !self.std.is_empty() || self.natural.is_some() || !self.plugins.is_empty()
    }

    fn validate_format(&self) -> Result<(), StoreError> {
        if self.has_v2() && self.has_legacy() {
            return Err(StoreError::ProjectFormat(
                "legacy std/natural/plugins cannot be combined with v2 roots/aliases".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Typed contents of `packages.lock.json`.
///
/// Unlike [`PackageSpec`], this is not dependency intent: every entry is an
/// exact resolver result and therefore records version, semantic digest,
/// provenance, and compilation layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagesLock {
    pub schema: String,
    pub packages: Vec<LockedPackage>,
}

impl Default for PackagesLock {
    fn default() -> Self {
        Self {
            schema: PACKAGES_LOCK_SCHEMA.to_owned(),
            packages: Vec::new(),
        }
    }
}

impl PackagesLock {
    pub fn from_resolved(resolved: &ResolvedPackages) -> Self {
        let mut lock = Self {
            packages: resolved
                .selection
                .resolved
                .iter()
                .cloned()
                .map(LockedPackage::from)
                .collect(),
            ..Self::default()
        };
        lock.sort_canonical();
        lock
    }

    /// Prove that a fresh offline resolution is byte-for-byte compatible with
    /// the persisted exact records.  Project open must call this before using
    /// a lock-backed resolution for compile or replay.
    pub fn verify_resolved(&self, resolved: &ResolvedPackages) -> Result<(), StoreError> {
        self.validate()?;
        let mut expected = self.clone();
        expected.sort_canonical();
        let actual = Self::from_resolved(resolved);
        let expected_ids = expected
            .packages
            .iter()
            .map(|package| package.id.to_string())
            .collect::<Vec<_>>();
        let actual_ids = actual
            .packages
            .iter()
            .map(|package| package.id.to_string())
            .collect::<Vec<_>>();
        if expected_ids != actual_ids {
            return Err(StoreError::PackageLockMismatch {
                package: "<package-set>".to_owned(),
                field: "packages",
                expected: expected_ids.join(","),
                actual: actual_ids.join(","),
            });
        }
        for (expected, actual) in expected.packages.iter().zip(&actual.packages) {
            if expected.version != actual.version {
                return Err(lock_mismatch(
                    expected,
                    "version",
                    expected.version.clone(),
                    actual.version.clone(),
                ));
            }
            if expected.digest != actual.digest {
                return Err(lock_mismatch(
                    expected,
                    "digest",
                    expected.digest.clone(),
                    actual.digest.clone(),
                ));
            }
            if expected.source != actual.source {
                return Err(lock_mismatch(
                    expected,
                    "source",
                    source_lock_text(&expected.source),
                    source_lock_text(&actual.source),
                ));
            }
            if expected.layer != actual.layer {
                return Err(lock_mismatch(
                    expected,
                    "layer",
                    expected.layer.keyword().to_owned(),
                    actual.layer.keyword().to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn sort_canonical(&mut self) {
        self.packages.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then(left.version.cmp(&right.version))
                .then(left.digest.cmp(&right.digest))
                .then(left.layer.cmp(&right.layer))
                .then(source_sort_key(&left.source).cmp(&source_sort_key(&right.source)))
        });
    }

    fn validate(&self) -> Result<(), StoreError> {
        if self.schema != PACKAGES_LOCK_SCHEMA {
            return Err(StoreError::Format(format!(
                "packages.lock.json has unsupported schema {:?}",
                self.schema
            )));
        }
        let mut ids = BTreeSet::new();
        for package in &self.packages {
            if package.version.trim().is_empty() {
                return Err(StoreError::Format(format!(
                    "packages.lock.json package {} has an empty version",
                    package.id
                )));
            }
            if package.digest.len() != 64
                || !package
                    .digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(StoreError::Format(format!(
                    "packages.lock.json package {} has an invalid sha256 digest",
                    package.id
                )));
            }
            if !ids.insert(package.id.clone()) {
                return Err(StoreError::Format(format!(
                    "packages.lock.json contains duplicate package {}",
                    package.id
                )));
            }
        }
        Ok(())
    }
}

fn lock_mismatch(
    package: &LockedPackage,
    field: &'static str,
    expected: String,
    actual: String,
) -> StoreError {
    StoreError::PackageLockMismatch {
        package: package.id.to_string(),
        field,
        expected,
        actual,
    }
}

impl From<&ResolvedPackages> for PackagesLock {
    fn from(value: &ResolvedPackages) -> Self {
        Self::from_resolved(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedPackage {
    pub id: PackageId,
    pub version: String,
    pub digest: String,
    pub source: PackageSource,
    pub layer: PackageLayer,
}

impl From<SelectedPackage> for LockedPackage {
    fn from(value: SelectedPackage) -> Self {
        Self {
            id: value.id,
            version: value.version,
            digest: value.digest,
            source: value.source,
            layer: value.layer,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackagesLock {
    schema: String,
    packages: Vec<RawLockedPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLockedPackage {
    id: String,
    version: String,
    digest: String,
    source: RawPackageSource,
    layer: String,
}

/// Filesystem-routing subset of a v2 `package.toml`.  The complete, original
/// TOML document is still forwarded to `conlang-language` for semantic
/// validation; this host view only decides which in-store files to read.
#[derive(Debug, Deserialize)]
struct VendoredManifest {
    schema: u32,
    #[serde(default = "default_exports_path")]
    exports: String,
    #[serde(default = "default_tables_path")]
    tables: String,
    #[serde(default)]
    code: Vec<String>,
    #[serde(default)]
    functions: Vec<String>,
    #[serde(default)]
    data: Vec<String>,
}

fn default_exports_path() -> String {
    "config/exports.tsv".to_owned()
}

/// `config/tables.tsv` 綁 data 檔到表型穩定 ID。缺檔合法(= 沒有具型別的表)。
fn default_tables_path() -> String {
    "config/tables.tsv".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "location", rename_all = "snake_case")]
enum RawPackageSource {
    Embedded,
    Vendored(String),
    Installed(String),
    Injected(String),
}

impl From<&PackageSource> for RawPackageSource {
    fn from(value: &PackageSource) -> Self {
        match value {
            PackageSource::Embedded => Self::Embedded,
            PackageSource::Vendored(location) => Self::Vendored(location.clone()),
            PackageSource::Installed(location) => Self::Installed(location.clone()),
            PackageSource::Injected(location) => Self::Injected(location.clone()),
        }
    }
}

impl From<RawPackageSource> for PackageSource {
    fn from(value: RawPackageSource) -> Self {
        match value {
            RawPackageSource::Embedded => Self::Embedded,
            RawPackageSource::Vendored(location) => Self::Vendored(location),
            RawPackageSource::Installed(location) => Self::Installed(location),
            RawPackageSource::Injected(location) => Self::Injected(location),
        }
    }
}

fn source_sort_key(source: &PackageSource) -> (&'static str, &str) {
    match source {
        PackageSource::Embedded => ("embedded", ""),
        PackageSource::Vendored(location) => ("vendored", location),
        PackageSource::Installed(location) => ("installed", location),
        PackageSource::Injected(location) => ("injected", location),
    }
}

fn source_lock_text(source: &PackageSource) -> String {
    let (kind, location) = source_sort_key(source);
    if location.is_empty() {
        kind.to_owned()
    } else {
        format!("{kind}:{location}")
    }
}

fn package_layer_from_keyword(value: &str) -> Result<PackageLayer, StoreError> {
    match value {
        "reference" => Ok(PackageLayer::Reference),
        "overlay" => Ok(PackageLayer::Overlay),
        "data" => Ok(PackageLayer::Data),
        _ => Err(StoreError::Format(format!(
            "packages.lock.json contains unknown layer {value:?}"
        ))),
    }
}

impl From<&PackagesLock> for RawPackagesLock {
    fn from(value: &PackagesLock) -> Self {
        Self {
            schema: value.schema.clone(),
            packages: value
                .packages
                .iter()
                .map(|package| RawLockedPackage {
                    id: package.id.to_string(),
                    version: package.version.clone(),
                    digest: package.digest.clone(),
                    source: RawPackageSource::from(&package.source),
                    layer: package.layer.keyword().to_owned(),
                })
                .collect(),
        }
    }
}

impl TryFrom<RawPackagesLock> for PackagesLock {
    type Error = StoreError;

    fn try_from(value: RawPackagesLock) -> Result<Self, Self::Error> {
        let mut lock = Self {
            schema: value.schema,
            packages: value
                .packages
                .into_iter()
                .map(|package| {
                    Ok(LockedPackage {
                        id: package
                            .id
                            .parse::<PackageId>()
                            .map_err(|_| StoreError::InvalidPackageId(package.id.clone()))?,
                        version: package.version,
                        digest: package.digest,
                        source: package.source.into(),
                        layer: package_layer_from_keyword(&package.layer)?,
                    })
                })
                .collect::<Result<Vec<_>, StoreError>>()?,
        };
        lock.validate()?;
        lock.sort_canonical();
        Ok(lock)
    }
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
        create_dir_all(&store.packages_dir())?;
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
        Ok(EvolutionGraph::restore(libraries, self.load_records()?)?)
    }

    /// Load through package-loader v2, preserving the already-resolved exact
    /// package context for replay instead of rediscovering dependencies.
    pub fn load_with_packages(
        &self,
        packages: ResolvedPackages,
    ) -> Result<EvolutionGraph, StoreError> {
        Ok(EvolutionGraph::restore_with_packages(
            packages,
            self.load_records()?,
        )?)
    }

    fn load_records(&self) -> Result<Vec<PersistedNode>, StoreError> {
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
        Ok(records)
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

    // ── views/(R4:一套一檔)────────────────────────────────────────────
    //
    // **這是資料層 API,不是 UI 的。** 本 crate 刻意**不認得** `ViewCommand`
    // ——那是應用層的意圖型別;persistence 只擁有「檔案裡放什麼、怎麼讀寫」。
    // 由 app 負責把意圖翻成對 [`ViewDocument`] 的修改再交回來寫。
    //
    // 若反過來讓 persistence 直接吃 command,格式層就會隨 UI 的意圖集合一起長,
    // 而那正是 §2.2「app 不得自行定義第二套格式」想避免的鏡像錯誤。

    // ── project.toml(R3 的 import 表)────────────────────────────────

    /// 讀專案宣告。**不存在時回 `None`,不是錯誤。**
    ///
    /// `None` 與「空的宣告」是兩件事:前者表示「這個目錄沒有專案宣告,
    /// 沿用呼叫端的預設」,後者表示「明確宣告不載入任何套件」。
    /// 分不開的話,升級後既有的 store 目錄就會突然變成「什麼都不載入」。
    pub fn read_project(&self) -> Result<Option<ProjectDocument>, StoreError> {
        let path = self.project_path();
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?;
        let project: ProjectDocument =
            toml::from_str(&text).map_err(|error| StoreError::ProjectFormat(error.to_string()))?;
        project.packages.validate_format()?;
        Ok(Some(project))
    }

    pub fn write_project(&self, project: &ProjectDocument) -> Result<(), StoreError> {
        project.packages.validate_format()?;
        let text = toml::to_string_pretty(project)
            .map_err(|error| StoreError::ProjectFormat(error.to_string()))?;
        atomic_write(&self.project_path(), text.as_bytes())
    }

    /// 這個專案要載入的套件組合。
    ///
    /// 沒有 `project.toml` 時回 `fallback`——**既有的 store 目錄照樣打得開**,
    /// 不製造遷移斷點。
    pub fn library_spec_or(&self, fallback: LibrarySpec) -> Result<LibrarySpec, StoreError> {
        match self.read_project()? {
            Some(project) => project.to_spec(),
            None => Ok(fallback),
        }
    }

    /// Read package-loader v2 project intent, using `fallback` only when the
    /// project has no `project.toml` at all.
    pub fn package_spec_or(&self, fallback: PackageSpec) -> Result<PackageSpec, StoreError> {
        match self.read_project()? {
            Some(project) => project.to_package_spec(),
            None => Ok(fallback),
        }
    }

    fn project_path(&self) -> PathBuf {
        self.root.join("project.toml")
    }

    /// Read the exact dependency resolution used by this project. Absence is
    /// distinct from an empty lock and lets callers perform first resolution.
    pub fn read_packages_lock(&self) -> Result<Option<PackagesLock>, StoreError> {
        let path = self.packages_lock_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw: RawPackagesLock = read_json(&path)?;
        Ok(Some(PackagesLock::try_from(raw)?))
    }

    /// Atomically replace the lock with a canonical, package-id-sorted
    /// document. Readers observe either the complete previous file or the
    /// complete replacement, never a remove-then-create gap.
    pub fn write_packages_lock(&self, lock: &PackagesLock) -> Result<(), StoreError> {
        let mut canonical = lock.clone();
        canonical.validate()?;
        canonical.sort_canonical();
        let raw = RawPackagesLock::from(&canonical);
        atomic_write(&self.packages_lock_path(), &json_bytes(&raw)?)
    }

    /// Persist the same immutable resolver result consumed by compile/check.
    pub fn write_resolved_packages_lock(
        &self,
        resolved: &ResolvedPackages,
    ) -> Result<PackagesLock, StoreError> {
        let lock = PackagesLock::from_resolved(resolved);
        self.write_packages_lock(&lock)?;
        Ok(lock)
    }

    /// Load every vendored v2 package below `<store>/packages` without network
    /// access. Manifest order is preserved within each package; package order
    /// is canonical by project-relative manifest path.
    pub fn read_vendored_packages(&self) -> Result<Vec<PackageSources>, StoreError> {
        let packages_dir = self.packages_dir();
        if !packages_dir.exists() {
            return Ok(Vec::new());
        }
        let mut manifests = Vec::new();
        discover_package_manifests(&packages_dir, &mut manifests)?;
        manifests.sort();

        manifests
            .into_iter()
            .map(|manifest_path| {
                let package_root = manifest_path.parent().ok_or_else(|| {
                    StoreError::Format(format!(
                        "package manifest has no parent: {}",
                        manifest_path.display()
                    ))
                })?;
                let config = read_utf8_file(&manifest_path)?;
                let routing: VendoredManifest = toml::from_str(&config).map_err(|error| {
                    StoreError::Format(format!(
                        "invalid vendored package manifest {}: {error}",
                        manifest_path.display()
                    ))
                })?;
                if routing.schema != 2 {
                    return Err(StoreError::Format(format!(
                        "vendored package manifest {} must use schema 2",
                        manifest_path.display()
                    )));
                }

                let exports = read_optional_manifest_file(
                    package_root,
                    &manifest_path,
                    "exports",
                    &routing.exports,
                )?
                .map(|file| file.source)
                .unwrap_or_default();
                let tables = read_optional_manifest_file(
                    package_root,
                    &manifest_path,
                    "tables",
                    &routing.tables,
                )?
                .map(|file| file.source)
                .unwrap_or_default();
                let code_files =
                    read_manifest_files(package_root, &manifest_path, "code", &routing.code)?;
                let functions = read_manifest_files(
                    package_root,
                    &manifest_path,
                    "functions",
                    &routing.functions,
                )?;
                let data_files =
                    read_manifest_files(package_root, &manifest_path, "data", &routing.data)?;
                let relative_root = package_root.strip_prefix(&self.root).map_err(|_| {
                    StoreError::Format(format!(
                        "vendored package escaped store root: {}",
                        package_root.display()
                    ))
                })?;
                let source = path_to_slash_string(relative_root).ok_or_else(|| {
                    StoreError::Format(format!(
                        "vendored package path is not UTF-8: {}",
                        relative_root.display()
                    ))
                })?;

                Ok(PackageSources {
                    config,
                    exports,
                    tables,
                    code: join_package_files(&code_files),
                    functions,
                    data: join_package_files(&data_files),
                    data_files,
                    source: PackageSource::Vendored(source),
                })
            })
            .collect()
    }

    /// Compose the offline resolver catalog in deterministic precedence order:
    /// project-vendored, caller-provided installed cache, shipped embedded.
    pub fn offline_package_catalog(
        &self,
        installed: impl IntoIterator<Item = PackageSources>,
    ) -> Result<LibraryCatalog, StoreError> {
        Ok(LibraryCatalog::with_source_precedence(
            self.read_vendored_packages()?,
            installed,
        )?)
    }

    /// Resolve explicit v2 intent through the host's offline source chain.
    pub fn resolve_packages(
        &self,
        spec: &PackageSpec,
        installed: impl IntoIterator<Item = PackageSources>,
    ) -> Result<ResolvedPackages, StoreError> {
        Ok(self.offline_package_catalog(installed)?.resolve(spec)?)
    }

    /// Resolve package intent stored in one already-read project document.
    pub fn resolve_project_packages(
        &self,
        project: &ProjectDocument,
        installed: impl IntoIterator<Item = PackageSources>,
    ) -> Result<ResolvedPackages, StoreError> {
        let resolved = self.resolve_packages(&project.to_package_spec()?, installed)?;
        if let Some(lock) = self.read_packages_lock()? {
            lock.verify_resolved(&resolved)?;
        }
        Ok(resolved)
    }

    fn packages_lock_path(&self) -> PathBuf {
        self.root.join("packages.lock.json")
    }

    fn packages_dir(&self) -> PathBuf {
        self.root.join("packages")
    }

    /// 一個視角檔的內容。`views/<name>.json`。
    ///
    /// 專案根 == store 根(R1),故它與 `objects/`、`nodes/` 同層。
    pub fn read_view(&self, name: &str) -> Result<ViewDocument, StoreError> {
        let path = self.view_path(name)?;
        if !path.exists() {
            return Ok(ViewDocument::default());
        }
        read_json(&path)
    }

    pub fn write_view(&self, name: &str, view: &ViewDocument) -> Result<(), StoreError> {
        let path = self.view_path(name)?;
        create_dir_all(&self.views_dir())?;
        atomic_write(&path, &json_bytes(view)?)
    }

    /// 已存在的視角名,排序後。
    pub fn list_views(&self) -> Result<Vec<String>, StoreError> {
        let dir = self.views_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        let entries = fs::read_dir(&dir).map_err(|source| StoreError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| StoreError::Io {
                path: dir.clone(),
                source,
            })?;
            let file = entry.file_name();
            let Some(name) = file.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            names.push(name.to_owned());
        }
        names.sort();
        Ok(names)
    }

    pub fn remove_view(&self, name: &str) -> Result<(), StoreError> {
        let path = self.view_path(name)?;
        if path.exists() {
            remove_file(&path)?;
        }
        Ok(())
    }

    fn views_dir(&self) -> PathBuf {
        self.root.join("views")
    }

    /// 視角名必須是單一路徑段——`../` 之類會逃出專案根。
    fn view_path(&self, name: &str) -> Result<PathBuf, StoreError> {
        let candidate = Path::new(name);
        let mut parts = candidate.components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(_)), None) if !name.is_empty() => {}
            _ => return Err(StoreError::InvalidViewName(name.to_owned())),
        }
        Ok(self.views_dir().join(format!("{name}.json")))
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

fn discover_package_manifests(
    directory: &Path,
    manifests: &mut Vec<PathBuf>,
) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(directory).map_err(|source| StoreError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StoreError::Format(format!(
            "vendored package directory is not a real directory: {}",
            directory.display()
        )));
    }

    let entries = fs::read_dir(directory).map_err(|source| StoreError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| StoreError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| StoreError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(StoreError::Format(format!(
                "vendored packages may not contain symlinks: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            discover_package_manifests(&path, manifests)?;
        } else if metadata.is_file() && entry.file_name() == OsStr::new("package.toml") {
            manifests.push(path);
        }
    }
    Ok(())
}

fn normalize_manifest_path(
    manifest: &Path,
    field: &'static str,
    raw: &str,
) -> Result<(String, PathBuf), StoreError> {
    let normalized = raw.trim().replace('\\', "/");
    let segments = normalized.split('/').collect::<Vec<_>>();
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return Err(StoreError::InvalidPackagePath {
            manifest: manifest.to_path_buf(),
            field,
            path: raw.to_owned(),
        });
    }
    let mut relative = PathBuf::new();
    for segment in segments {
        relative.push(segment);
    }
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StoreError::InvalidPackagePath {
            manifest: manifest.to_path_buf(),
            field,
            path: raw.to_owned(),
        });
    }
    Ok((normalized, relative))
}

fn read_manifest_files(
    package_root: &Path,
    manifest: &Path,
    field: &'static str,
    paths: &[String],
) -> Result<Vec<PackageFile>, StoreError> {
    paths
        .iter()
        .map(|path| read_manifest_file(package_root, manifest, field, path))
        .collect()
}

fn read_optional_manifest_file(
    package_root: &Path,
    manifest: &Path,
    field: &'static str,
    raw: &str,
) -> Result<Option<PackageFile>, StoreError> {
    let (normalized, relative) = normalize_manifest_path(manifest, field, raw)?;
    let root_metadata = fs::symlink_metadata(package_root).map_err(|source| StoreError::Io {
        path: package_root.to_path_buf(),
        source,
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(StoreError::Format(format!(
            "vendored package root is not a real directory: {}",
            package_root.display()
        )));
    }

    let components = relative.components().collect::<Vec<_>>();
    let mut current = package_root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err(StoreError::InvalidPackagePath {
                manifest: manifest.to_path_buf(),
                field,
                path: raw.to_owned(),
            });
        };
        current.push(segment);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(StoreError::Io {
                    path: current,
                    source,
                })
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(StoreError::InvalidPackagePath {
                manifest: manifest.to_path_buf(),
                field,
                path: raw.to_owned(),
            });
        }
        let is_target = index + 1 == components.len();
        if (!is_target && !metadata.is_dir()) || (is_target && !metadata.is_file()) {
            return Err(StoreError::Format(format!(
                "vendored package path has the wrong file type: {}",
                current.display()
            )));
        }
    }

    Ok(Some(PackageFile {
        path: normalized,
        source: read_utf8_file(&current)?,
    }))
}

fn read_manifest_file(
    package_root: &Path,
    manifest: &Path,
    field: &'static str,
    raw: &str,
) -> Result<PackageFile, StoreError> {
    let (normalized, relative) = normalize_manifest_path(manifest, field, raw)?;
    let root_metadata = fs::symlink_metadata(package_root).map_err(|source| StoreError::Io {
        path: package_root.to_path_buf(),
        source,
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(StoreError::Format(format!(
            "vendored package root is not a real directory: {}",
            package_root.display()
        )));
    }

    let components = relative.components().collect::<Vec<_>>();
    let mut current = package_root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err(StoreError::InvalidPackagePath {
                manifest: manifest.to_path_buf(),
                field,
                path: raw.to_owned(),
            });
        };
        current.push(segment);
        let metadata = fs::symlink_metadata(&current).map_err(|source| StoreError::Io {
            path: current.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(StoreError::InvalidPackagePath {
                manifest: manifest.to_path_buf(),
                field,
                path: raw.to_owned(),
            });
        }
        let is_target = index + 1 == components.len();
        if (!is_target && !metadata.is_dir()) || (is_target && !metadata.is_file()) {
            return Err(StoreError::Format(format!(
                "vendored package path has the wrong file type: {}",
                current.display()
            )));
        }
    }

    Ok(PackageFile {
        path: normalized,
        source: read_utf8_file(&current)?,
    })
}

fn read_utf8_file(path: &Path) -> Result<String, StoreError> {
    String::from_utf8(read_bytes(path)?).map_err(|_| StoreError::PackageFileNotUtf8(path.into()))
}

fn join_package_files(files: &[PackageFile]) -> String {
    files
        .iter()
        .map(|file| file.source.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn path_to_slash_string(path: &Path) -> Option<String> {
    let mut segments = Vec::new();
    for component in path.components() {
        let Component::Normal(segment) = component else {
            return None;
        };
        segments.push(segment.to_str()?);
    }
    Some(segments.join("/"))
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
    replace_file(&temporary, path)
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, path: &Path) -> Result<(), StoreError> {
    fs::rename(temporary, path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(windows)]
fn replace_file(temporary: &Path, path: &Path) -> Result<(), StoreError> {
    if path.exists() {
        windows_atomic_replace::replace_existing(path, temporary)
    } else {
        fs::rename(temporary, path).map_err(|source| StoreError::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_atomic_replace {
    use super::{Path, StoreError};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    pub(super) fn replace_existing(path: &Path, temporary: &Path) -> Result<(), StoreError> {
        let replaced = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let replacement = temporary
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        // SAFETY: both paths are owned, NUL-terminated UTF-16 buffers that
        // remain alive for the call. Backup/exclusion/preserved pointers are
        // null as allowed by ReplaceFileW. No Rust references cross the FFI.
        let replaced = unsafe {
            ReplaceFileW(
                replaced.as_ptr(),
                replacement.as_ptr(),
                ptr::null(),
                0,
                ptr::null(),
                ptr::null(),
            )
        };
        if replaced == 0 {
            Err(StoreError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::last_os_error(),
            })
        } else {
            Ok(())
        }
    }
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
