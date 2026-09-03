//! Deterministic embedded library catalog shared by standard, plugin, and
//! natural-language packages.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use crate::{Language, RuleNamespace, SignItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LibraryKind {
    Std,
    Natural,
    Plugin,
}

impl LibraryKind {
    pub fn keyword(self) -> &'static str {
        match self {
            LibraryKind::Std => "std",
            LibraryKind::Natural => "natural",
            LibraryKind::Plugin => "plugin",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "std" => Some(Self::Std),
            "natural" => Some(Self::Natural),
            "plugin" => Some(Self::Plugin),
            _ => None,
        }
    }
}

impl From<LibraryKind> for String {
    fn from(value: LibraryKind) -> Self {
        value.keyword().to_owned()
    }
}

impl fmt::Display for LibraryKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.keyword())
    }
}

/// Stable package identity.  Unlike the legacy [`LibraryKind`], namespaces are
/// open: `catalog:*`, `dataset:*`, `theory:*`, and project-defined namespaces
/// are ordinary package IDs rather than new engine enum variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageId {
    pub namespace: String,
    pub name: String,
}

impl PackageId {
    /// Construct an ID from either an open namespace string or a legacy
    /// [`LibraryKind`].
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    pub fn legacy_kind(&self) -> Option<LibraryKind> {
        LibraryKind::parse(&self.namespace)
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.namespace, self.name)
    }
}

impl FromStr for PackageId {
    type Err = LibraryLoadError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((namespace, name)) = value.split_once(':') else {
            return Err(LibraryLoadError::InvalidId(value.to_owned()));
        };
        if !valid_identifier(namespace) || !valid_identifier(name) {
            return Err(LibraryLoadError::InvalidId(value.to_owned()));
        }
        Ok(Self::new(namespace, name))
    }
}

/// Migration name retained for callers moving to [`PackageId`].  This keeps
/// the type name, not Rust struct-field source compatibility: v2 IDs expose
/// `namespace` instead of the former closed-enum `kind` field.
pub type LibraryId = PackageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PackageLayer {
    /// Reference models and annotation schemes.  They contribute reusable
    /// traits but no concrete language inventory.
    Reference,
    /// Concrete language, project, or theory overlays.
    Overlay,
    /// Data/function-only packages that do not contribute `.lang` nodes.
    Data,
}

impl PackageLayer {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Overlay => "overlay",
            Self::Data => "data",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "reference" => Some(Self::Reference),
            "overlay" => Some(Self::Overlay),
            "data" => Some(Self::Data),
            _ => None,
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Reference => 0,
            Self::Data => 1,
            Self::Overlay => 2,
        }
    }
}

impl fmt::Display for PackageLayer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.keyword())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackageCapabilities {
    pub traits: bool,
    pub signs: bool,
    pub functions: bool,
    pub data: bool,
}

impl PackageCapabilities {
    fn parse(values: &[String], package: &str) -> Result<Self, LibraryLoadError> {
        let mut capabilities = Self::default();
        for value in values {
            let target = match value.as_str() {
                "traits" => &mut capabilities.traits,
                "signs" => &mut capabilities.signs,
                "functions" => &mut capabilities.functions,
                "data" => &mut capabilities.data,
                _ => {
                    return Err(config_error(
                        package,
                        0,
                        format!("unknown capability {value:?}"),
                    ))
                }
            };
            if std::mem::replace(target, true) {
                return Err(config_error(
                    package,
                    0,
                    format!("duplicate capability {value:?}"),
                ));
            }
        }
        Ok(capabilities)
    }

    fn canonical(self) -> String {
        let mut values = Vec::new();
        if self.traits {
            values.push("traits");
        }
        if self.signs {
            values.push("signs");
        }
        if self.functions {
            values.push("functions");
        }
        if self.data {
            values.push("data");
        }
        values.join(",")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageRequirement {
    pub id: PackageId,
    /// v2 currently supports an exact version pin.  `None` is the legacy
    /// unversioned dependency form and resolves to the only available version.
    pub version: Option<String>,
}

impl PackageRequirement {
    pub fn new(id: PackageId) -> Self {
        Self { id, version: None }
    }

    pub fn exact(id: PackageId, version: impl Into<String>) -> Self {
        Self {
            id,
            version: Some(version.into()),
        }
    }
}

impl From<PackageId> for PackageRequirement {
    fn from(value: PackageId) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for PackageRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.version {
            Some(version) => write!(formatter, "{}@{version}", self.id),
            None => self.id.fmt(formatter),
        }
    }
}

impl FromStr for PackageRequirement {
    type Err = LibraryLoadError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (id, version) = match value.rsplit_once('@') {
            Some((id, version)) if valid_version(version) => (id, Some(version.to_owned())),
            Some(_) => return Err(LibraryLoadError::InvalidRequirement(value.to_owned())),
            None => (value, None),
        };
        Ok(Self {
            id: id.parse()?,
            version,
        })
    }
}

/// v2 project intent.  The default is deliberately empty: shipped packages
/// are available to the resolver but never become roots implicitly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageSpec {
    pub roots: Vec<PackageRequirement>,
    pub aliases: BTreeMap<String, PackageId>,
}

impl PackageSpec {
    pub fn with_root(mut self, requirement: impl Into<PackageRequirement>) -> Self {
        self.roots.push(requirement.into());
        self
    }

    pub fn with_alias(
        mut self,
        alias: impl Into<String>,
        package: PackageId,
    ) -> Result<Self, LibraryLoadError> {
        let alias = alias.into();
        if !valid_identifier(&alias) {
            return Err(LibraryLoadError::InvalidPackageAlias(alias));
        }
        self.aliases.insert(alias, package);
        Ok(self)
    }

    pub fn from_legacy(spec: &LibrarySpec) -> Self {
        let mut roots = spec
            .std
            .iter()
            .cloned()
            .map(PackageRequirement::from)
            .collect::<Vec<_>>();
        roots.extend(spec.natural.iter().cloned().map(PackageRequirement::from));
        roots.extend(spec.plugins.iter().cloned().map(PackageRequirement::from));
        Self {
            roots,
            aliases: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LibraryExportKind {
    Trait,
    Sign,
    /// 歷時 function 層(Recipe/Goal;P48–P50)。住套件 `code/*.chg`。
    Function,
}

impl LibraryExportKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "trait" => Some(Self::Trait),
            "sign" => Some(Self::Sign),
            "function" => Some(Self::Function),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExport {
    /// Compatibility short name; `package_id` carries the fully qualified ID.
    pub package: String,
    pub package_id: LibraryId,
    pub stable_id: String,
    pub kind: LibraryExportKind,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFunctionSource {
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryDataSource {
    pub path: String,
    pub source: String,
    /// 這份表的**表型穩定 ID**,由 `config/tables.tsv` 宣告(P29:跨套件契約
    /// 是穩定 ID,不是套件內部檔案路徑)。`None` = 未宣告表型;沒有任何解析器
    /// 會看到它,它只進 library lock。
    ///
    /// 表型決定**誰來讀**這份表,不決定欄位怎麼解讀——欄位仍由認得該表型的
    /// 消費者定義。引擎自帶的兩個表型在 [`table_type`] 模組。
    pub table_type: Option<String>,
}

/// 引擎自帶的表型穩定 ID。
///
/// 這兩個 schema 定義在引擎 crate(`changeset` / `stats`)裡而不在任何套件裡,
/// 故命名空間是 `engine:` 而非某個 package id。套件要自帶表型時用自己的
/// 命名空間(例:`plugin:tonepack:ToneTable`),不必改引擎。
pub mod table_type {
    /// Step 17 Weight DB(`goal<TAB>recipe<TAB>weight`)。
    pub const WEIGHT_TABLE: &str = "engine:WeightTable";
    /// 模組 E 的 E1 音段先驗(`segment<TAB>weight`)。
    pub const SEGMENT_PRIOR: &str = "engine:SegmentPrior";
    /// P52 語法化路徑庫(`source<TAB>target<TAB>delta`):來源概念、目標語意、
    /// 預設 δ。機制住 `code/*.chg` 的參數化 function,路徑本身住這張表——
    /// **加一條路徑 = 加一行 data**,不改 `.chg`、不改引擎。
    pub const GRAMMATICALIZATION_PATH_TABLE: &str = "engine:GrammaticalizationPathTable";
}

/// 表型 ID 格式:兩段以上、以 `:` 分隔的識別字。
///
/// 與 export stable id 同形(`std:grammaticalization:VerbToTense`),但**不強制**
/// 前綴等於宣告者的 package id——套件本來就該能宣告「我這份表是**別人**定義的
/// 那個表型」,那正是 P29 要的跨套件契約。
fn valid_table_type(value: &str) -> bool {
    let mut segments = value.split(':');
    let first = segments.next().is_some_and(valid_identifier);
    let rest: Vec<&str> = segments.collect();
    first && !rest.is_empty() && rest.iter().copied().all(valid_identifier)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPackage {
    pub id: PackageId,
    /// Compatibility short name used by the former stdlib API.
    pub name: String,
    pub version: String,
    /// Manifest schema used to load this package.  Schema 1 is the historical
    /// `package.conf`; schema 2 is `package.toml`.
    pub manifest_schema: u32,
    pub layer: PackageLayer,
    pub capabilities: PackageCapabilities,
    pub source: PackageSource,
    pub rule_namespace: String,
    pub enabled: bool,
    pub priority: i32,
    pub requires: Vec<PackageRequirement>,
    /// Ordered source files compiled as one package. Declaration and rule
    /// order follow this list, so package manifests must keep it stable.
    pub code_paths: Vec<String>,
    /// Compatibility rendering of `code_paths` for callers that previously
    /// consumed the single `code` manifest field.
    pub code_path: String,
    /// Compatibility rendering of `data_paths` (comma-joined), mirroring `code_path`.
    pub data_path: String,
    /// Ordered data files of this package (P50 ①).
    pub data_paths: Vec<String>,
    /// Ordered data documents paired with their manifest paths.
    pub data_sources: Vec<LibraryDataSource>,
    /// Ordered `code/*.chg` files (P50 ③). Empty when the package ships no
    /// diachronic functions.
    pub function_paths: Vec<String>,
    /// Ordered function definition documents paired with their manifest paths.
    ///
    /// `functions` remains a compatibility fallback for callers that still
    /// provide one synthetic document.
    pub function_sources: Vec<LibraryFunctionSource>,
    pub exports: Vec<LibraryExport>,
    /// R9-a:這些**曾是 `&'static str`**,型別層面把 package 鎖死在編譯期常數上
    /// ——沒有任何執行期來源進得來,`plugin` kind 因此不可達、E1 先驗庫
    /// (PHOIBLE/Grambank 全集)也無處可去。改為 owned 之後,隨引擎發布的那組
    /// 仍走 `include_str!`(它們是**預設**),但 package 不再**必須**是常數。
    pub code: String,
    /// 歷時 function 原始碼(`.chg`),**verbatim**;`language` 不解析(P20)。
    pub functions: String,
    pub data: String,
}

impl LibraryPackage {
    /// 本套件中宣告為 `table_type` 這個表型的 data 檔,依 manifest 序。
    ///
    /// 這是消費者(Weight DB、E1 先驗、日後的路徑庫)取表的**唯一**入口:
    /// 認表型,不認檔案路徑(P29)。套件把表放在 `data/` 底下哪個路徑、叫什麼
    /// 名字,都不是契約。
    pub fn tables<'a>(
        &'a self,
        table_type: &'a str,
    ) -> impl Iterator<Item = &'a LibraryDataSource> {
        self.data_sources
            .iter()
            .filter(move |source| source.table_type.as_deref() == Some(table_type))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySpec {
    /// 要載入的 std package(R12)。
    ///
    /// 此前 `select()` 憑 `kind == Std` **自動**把它們當作解析起點——沒有任何人
    /// 宣告,`natural`/`plugin` 卻要點名。裁定 S 之下三種 kind 一視同仁,全部
    /// 由此宣告;[`LibrarySpec::default`] 仍填入隨引擎發布的那一組,故行為不變。
    ///
    /// 差別在於**特權從程式邏輯降級為一份可覆寫的預設值**:現在才可能
    /// 「不載入 `std:grambank`」或「用自己的 core 取代 `std:core`」。
    /// 對映 C++:`libstdc++` 隨編譯器發布、預設連結,但它只是個 library。
    pub std: Vec<LibraryId>,
    pub natural: Option<LibraryId>,
    pub plugins: Vec<LibraryId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageSource {
    Embedded,
    Vendored(String),
    Installed(String),
    Injected(String),
}

impl Default for PackageSource {
    fn default() -> Self {
        Self::Injected("host".to_owned())
    }
}

impl PackageSource {
    pub fn keyword(&self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Vendored(_) => "vendored",
            Self::Installed(_) => "installed",
            Self::Injected(_) => "injected",
        }
    }
}

/// 隨引擎發布的預設 std 組合。**是預設值,不是特權**——可被 spec 覆寫或清空。
pub fn default_std_packages() -> Vec<LibraryId> {
    ["core", "cxg", "grambank", "grammaticalization"]
        .into_iter()
        .map(|name| LibraryId::new(LibraryKind::Std, name))
        .collect()
}

impl Default for LibrarySpec {
    fn default() -> Self {
        Self {
            std: default_std_packages(),
            natural: None,
            plugins: Vec::new(),
        }
    }
}

impl LibrarySpec {
    pub fn natural(id: LibraryId) -> Self {
        Self {
            natural: Some(id),
            ..Self::default()
        }
    }

    /// 完全不載入任何 std——`belongs Noun` 之類會得到未知範疇診斷(附 R13 指路)。
    pub fn without_std(mut self) -> Self {
        self.std.clear();
        self
    }

    pub fn with_plugin(mut self, id: LibraryId) -> Self {
        self.plugins.push(id);
        self
    }
}

#[derive(Debug, Clone)]
pub struct LibrarySelection {
    pub standard: Language,
    pub overlay: Language,
    pub packages: Vec<PackageId>,
    /// Exact package facts used by compile, cache, lock, and replay.  Callers
    /// must not reconstruct these from roots using a different catalog.
    pub resolved: Vec<SelectedPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedPackage {
    pub id: PackageId,
    pub version: String,
    pub digest: String,
    pub source: PackageSource,
    pub layer: PackageLayer,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LibraryLoadError {
    #[error("invalid library id {0:?}")]
    InvalidId(String),
    #[error("invalid package requirement {0:?}")]
    InvalidRequirement(String),
    #[error("invalid package alias {0:?}")]
    InvalidPackageAlias(String),
    #[error("library package {package:?} config line {line}: {message}")]
    Config {
        package: String,
        line: usize,
        message: String,
    },
    #[error("library package {package:?} exports line {line}: {message}")]
    Exports {
        package: String,
        line: usize,
        message: String,
    },
    #[error("library package {package:?} tables line {line}: {message}")]
    Tables {
        package: String,
        line: usize,
        message: String,
    },
    #[error("library package {package:?} code line {line}: {message}")]
    PackageLanguage {
        package: String,
        line: usize,
        message: String,
    },
    #[error("library package {package:?} contains unsupported content: {message}")]
    UnsupportedContent { package: String, message: String },
    #[error("duplicate library package id {0}")]
    DuplicatePackage(LibraryId),
    #[error("duplicate rule namespace {namespace:?} in {first} and {second}")]
    DuplicateRuleNamespace {
        namespace: String,
        first: LibraryId,
        second: LibraryId,
    },
    #[error("duplicate export stable id {stable_id:?} in {first} and {second}")]
    DuplicateStableId {
        stable_id: String,
        first: LibraryId,
        second: LibraryId,
    },
    #[error("duplicate export alias {alias:?} in {first} and {second}")]
    DuplicateAlias {
        alias: String,
        first: LibraryId,
        second: LibraryId,
    },
    #[error("package {package} exports missing {kind:?} {alias:?}")]
    MissingAlias {
        package: LibraryId,
        kind: LibraryExportKind,
        alias: String,
    },
    #[error("unknown library package {0}")]
    UnknownPackage(LibraryId),
    #[error("library package {0} is disabled")]
    DisabledPackage(LibraryId),
    #[error("package {package} requires version {expected:?}, but resolver selected {actual:?}")]
    VersionMismatch {
        package: PackageId,
        expected: String,
        actual: String,
    },
    #[error("package root {package} is declared more than once with incompatible versions")]
    ConflictingRequirements { package: PackageId },
    #[error("package alias {alias:?} targets unselected package {package}")]
    AliasTargetNotSelected { alias: String, package: PackageId },
    #[error("expected package namespace {expected}, got {actual} for {id}")]
    WrongKind {
        id: LibraryId,
        expected: LibraryKind,
        actual: String,
    },
    #[error("library dependency cycle: {0}")]
    DependencyCycle(String),
    #[error("unknown standard export alias {0:?}")]
    UnknownAlias(String),
    #[error("combined library defines duplicate trait {0:?}")]
    DuplicateTrait(String),
    #[error("combined library defines duplicate sign {0:?}")]
    DuplicateSign(String),
}

impl LibraryLoadError {
    /// Stable machine-readable code for catalog, manifest, and dependency errors.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidId(_) => "LIBRARY_INVALID_ID",
            Self::InvalidRequirement(_) => "PACKAGE_REQUIREMENT_INVALID",
            Self::InvalidPackageAlias(_) => "PACKAGE_ALIAS_INVALID",
            Self::Config { .. } => "LIBRARY_CONFIG_INVALID",
            Self::Exports { .. } => "LIBRARY_EXPORTS_INVALID",
            Self::Tables { .. } => "LIBRARY_TABLES_INVALID",
            Self::PackageLanguage { .. } => "LIBRARY_LANGUAGE_INVALID",
            Self::UnsupportedContent { .. } => "LIBRARY_CONTENT_UNSUPPORTED",
            Self::DuplicatePackage(_) => "LIBRARY_PACKAGE_DUPLICATE",
            Self::DuplicateRuleNamespace { .. } => "LIBRARY_RULE_NAMESPACE_DUPLICATE",
            Self::DuplicateStableId { .. } => "LIBRARY_EXPORT_ID_DUPLICATE",
            Self::DuplicateAlias { .. } => "LIBRARY_EXPORT_ALIAS_DUPLICATE",
            Self::MissingAlias { .. } => "LIBRARY_EXPORT_MISSING",
            Self::UnknownPackage(_) => "LIBRARY_PACKAGE_UNKNOWN",
            Self::DisabledPackage(_) => "LIBRARY_PACKAGE_DISABLED",
            Self::VersionMismatch { .. } => "PACKAGE_VERSION_MISMATCH",
            Self::ConflictingRequirements { .. } => "PACKAGE_REQUIREMENT_CONFLICT",
            Self::AliasTargetNotSelected { .. } => "PACKAGE_ALIAS_TARGET_UNSELECTED",
            Self::WrongKind { .. } => "LIBRARY_KIND_MISMATCH",
            Self::DependencyCycle(_) => "LIBRARY_DEPENDENCY_CYCLE",
            Self::UnknownAlias(_) => "LIBRARY_EXPORT_UNKNOWN",
            Self::DuplicateTrait(_) => "LIBRARY_TRAIT_DUPLICATE",
            Self::DuplicateSign(_) => "LIBRARY_SIGN_DUPLICATE",
        }
    }
}

/// 一個 package 的原始檔內容,**對映磁碟上的 `config/` + `code/` + `data/` 佈局**。
///
/// R9-a:host(`persistence` / 未來的 `app`)讀檔後以此注入;`language` 仍不碰
/// `std::fs`(§4、wasm 綠)。wasm 前端無 fs,由 app shell 供給,走同一介面。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageSources {
    /// Legacy `config/package.conf` or v2 `package.toml` contents.
    pub config: String,
    /// `config/exports.tsv`
    pub exports: String,
    /// `config/tables.tsv`(選填;空字串 = 沒有具型別的 data 表)
    pub tables: String,
    /// 依 manifest 序合併的 `code/*.lang`
    pub code: String,
    /// `code/*.chg`(P50 ③,verbatim 不解析)
    pub functions: Vec<PackageFile>,
    /// 依 manifest 序合併的 `data/*`
    pub data: String,
    /// 逐檔的 `data/*`,配上 manifest 路徑
    pub data_files: Vec<PackageFile>,
    /// Host-provided provenance. Moving identical bytes between sources does
    /// not alter the semantic package digest.
    pub source: PackageSource,
}

/// 一個帶 manifest 路徑的來源檔。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFile {
    pub path: String,
    pub source: String,
}

#[derive(Debug)]
struct EmbeddedPackage {
    config: &'static str,
    exports: &'static str,
    /// `config/tables.tsv`;沒有具型別的表時為空字串。
    tables: &'static str,
    code: &'static str,
    /// P50 ③:套件 `code/*.chg` 的歷時 function 原始碼,**verbatim 承載不解析**。
    /// `language` 不得解析 `.chg`(P20 依賴方向 `changeset → language`),
    /// 比照 dsl 域宣告以不透明區塊承載的既有作法(I15-a);由 `changeset` 端解析。
    functions: &'static [EmbeddedFunctionSource],
    data: &'static str,
    data_sources: &'static [EmbeddedDataSource],
}

#[derive(Debug)]
struct EmbeddedFunctionSource {
    path: &'static str,
    source: &'static str,
}

#[derive(Debug)]
struct EmbeddedDataSource {
    path: &'static str,
    source: &'static str,
}

const GRAMMATICALIZATION_FUNCTIONS: &[EmbeddedFunctionSource] = &[
    EmbeddedFunctionSource {
        path: "code/recipes.chg",
        source: include_str!("../lib/std/grammaticalization/code/recipes.chg"),
    },
    EmbeddedFunctionSource {
        path: "code/goals.chg",
        source: include_str!("../lib/std/grammaticalization/code/goals.chg"),
    },
];

const CORE_DATA: &[EmbeddedDataSource] = &[EmbeddedDataSource {
    path: "data/categories.tsv",
    source: include_str!("../lib/std/core/data/categories.tsv"),
}];
const GRAMBANK_DATA: &[EmbeddedDataSource] = &[EmbeddedDataSource {
    path: "data/features.tsv",
    source: include_str!("../lib/std/grambank/data/features.tsv"),
}];
const CXG_DATA: &[EmbeddedDataSource] = &[EmbeddedDataSource {
    path: "data/realizations.tsv",
    source: include_str!("../lib/std/cxg/data/realizations.tsv"),
}];
const GRAMMATICALIZATION_DATA: &[EmbeddedDataSource] = &[
    EmbeddedDataSource {
        path: "data/paths.tsv",
        source: include_str!("../lib/std/grammaticalization/data/paths.tsv"),
    },
    EmbeddedDataSource {
        path: "data/weights.tsv",
        source: include_str!("../lib/std/grammaticalization/data/weights.tsv"),
    },
];
const EN_STANDARD_DATA: &[EmbeddedDataSource] = &[EmbeddedDataSource {
    path: "data/grambank-v1.0.3.tsv",
    source: include_str!("../lib/natural/en-standard/data/grambank-v1.0.3.tsv"),
}];

const EMBEDDED_PACKAGES: &[EmbeddedPackage] = &[
    EmbeddedPackage {
        config: include_str!("../lib/std/core/config/package.conf"),
        exports: include_str!("../lib/std/core/config/exports.tsv"),
        tables: "",
        code: include_str!("../lib/std/core/code/ontology.lang"),
        functions: &[],
        data: include_str!("../lib/std/core/data/categories.tsv"),
        data_sources: CORE_DATA,
    },
    EmbeddedPackage {
        config: include_str!("../lib/std/grambank/config/package.conf"),
        exports: include_str!("../lib/std/grambank/config/exports.tsv"),
        tables: "",
        code: include_str!("../lib/std/grambank/code/syntax.lang"),
        functions: &[],
        data: include_str!("../lib/std/grambank/data/features.tsv"),
        data_sources: GRAMBANK_DATA,
    },
    EmbeddedPackage {
        config: include_str!("../lib/std/cxg/config/package.conf"),
        exports: include_str!("../lib/std/cxg/config/exports.tsv"),
        tables: "",
        code: concat!(
            include_str!("../lib/std/cxg/code/schema.lang"),
            "\n",
            include_str!("../lib/std/cxg/code/realizations.lang")
        ),
        functions: &[],
        data: include_str!("../lib/std/cxg/data/realizations.tsv"),
        data_sources: CXG_DATA,
    },
    EmbeddedPackage {
        config: include_str!("../lib/std/grammaticalization/config/package.conf"),
        exports: include_str!("../lib/std/grammaticalization/config/exports.tsv"),
        tables: include_str!("../lib/std/grammaticalization/config/tables.tsv"),
        code: "",
        functions: GRAMMATICALIZATION_FUNCTIONS,
        data: concat!(
            include_str!("../lib/std/grammaticalization/data/paths.tsv"),
            "\n",
            include_str!("../lib/std/grammaticalization/data/weights.tsv")
        ),
        data_sources: GRAMMATICALIZATION_DATA,
    },
    EmbeddedPackage {
        config: include_str!("../lib/natural/en-standard/config/package.conf"),
        exports: include_str!("../lib/natural/en-standard/config/exports.tsv"),
        tables: "",
        code: include_str!("../lib/natural/en-standard/code/grammar.lang"),
        functions: &[],
        data: include_str!("../lib/natural/en-standard/data/grambank-v1.0.3.tsv"),
        data_sources: EN_STANDARD_DATA,
    },
];

#[derive(Default)]
struct Manifest {
    schema: u32,
    id: Option<PackageId>,
    kind: Option<LibraryKind>,
    name: Option<String>,
    version: Option<String>,
    layer: Option<PackageLayer>,
    capabilities: Option<PackageCapabilities>,
    rule_namespace: Option<String>,
    enabled: Option<bool>,
    priority: Option<i32>,
    requires: Option<Vec<PackageRequirement>>,
    code_paths: Option<Vec<String>>,
    data_paths: Option<Vec<String>>,
    function_paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlManifestV2 {
    schema: u32,
    id: String,
    version: String,
    layer: String,
    capabilities: Vec<String>,
    #[serde(default = "default_exports_path")]
    exports: String,
    #[serde(default)]
    rule_namespace: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    code: Vec<String>,
    #[serde(default)]
    functions: Vec<String>,
    #[serde(default)]
    data: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_exports_path() -> String {
    "config/exports.tsv".to_owned()
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
}

fn config_error(package: &str, line: usize, message: impl Into<String>) -> LibraryLoadError {
    LibraryLoadError::Config {
        package: package.to_owned(),
        line,
        message: message.into(),
    }
}

fn unquote(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if matches!(
            (bytes[0], bytes[value.len() - 1]),
            (b'"', b'"') | (b'\'', b'\'')
        ) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn set_once<T>(
    field: &mut Option<T>,
    value: T,
    package: &str,
    line: usize,
    key: &str,
) -> Result<(), LibraryLoadError> {
    if field.replace(value).is_some() {
        return Err(config_error(
            package,
            line,
            format!("duplicate key {key:?}"),
        ));
    }
    Ok(())
}

fn parse_legacy_manifest(source: &str) -> Result<Manifest, LibraryLoadError> {
    let package_hint = source
        .lines()
        .find_map(|line| line.trim().strip_prefix("name ="))
        .map(str::trim)
        .unwrap_or("<unknown>");
    let mut manifest = Manifest {
        schema: 1,
        ..Manifest::default()
    };
    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            return Err(config_error(
                package_hint,
                line_number,
                "expected `key = value`",
            ));
        };
        let key = key.trim();
        let value = unquote(raw_value);
        match key {
            "kind" => {
                let kind = LibraryKind::parse(value).ok_or_else(|| {
                    config_error(
                        package_hint,
                        line_number,
                        "kind must be std, natural, or plugin",
                    )
                })?;
                set_once(&mut manifest.kind, kind, package_hint, line_number, key)?;
            }
            "name" => {
                if !valid_identifier(value) {
                    return Err(config_error(
                        package_hint,
                        line_number,
                        "invalid package name",
                    ));
                }
                set_once(
                    &mut manifest.name,
                    value.to_owned(),
                    package_hint,
                    line_number,
                    key,
                )?;
            }
            "version" => set_once(
                &mut manifest.version,
                value.to_owned(),
                package_hint,
                line_number,
                key,
            )?,
            "rule_namespace" => set_once(
                &mut manifest.rule_namespace,
                value.to_owned(),
                package_hint,
                line_number,
                key,
            )?,
            "enabled" => {
                let enabled = match value {
                    "true" => true,
                    "false" => false,
                    _ => {
                        return Err(config_error(
                            package_hint,
                            line_number,
                            "enabled must be true or false",
                        ))
                    }
                };
                set_once(
                    &mut manifest.enabled,
                    enabled,
                    package_hint,
                    line_number,
                    key,
                )?;
            }
            "priority" => {
                let priority = value.parse::<i32>().map_err(|_| {
                    config_error(
                        package_hint,
                        line_number,
                        "priority must be a 32-bit integer",
                    )
                })?;
                set_once(
                    &mut manifest.priority,
                    priority,
                    package_hint,
                    line_number,
                    key,
                )?;
            }
            "requires" => {
                let dependencies = if value.is_empty() {
                    Vec::new()
                } else {
                    value
                        .split(',')
                        .map(|part| {
                            part.trim()
                                .parse::<PackageId>()
                                .map(PackageRequirement::from)
                        })
                        .collect::<Result<Vec<_>, _>>()?
                };
                set_once(
                    &mut manifest.requires,
                    dependencies,
                    package_hint,
                    line_number,
                    key,
                )?;
            }
            "code" => {
                let paths = value
                    .split(',')
                    .map(|path| path.trim().replace('\\', "/"))
                    .collect::<Vec<_>>();
                if paths.is_empty() || paths.iter().any(String::is_empty) {
                    return Err(config_error(
                        package_hint,
                        line_number,
                        "code must contain one or more comma-separated paths",
                    ));
                }
                set_once(
                    &mut manifest.code_paths,
                    paths,
                    package_hint,
                    line_number,
                    key,
                )?;
            }
            "functions" => {
                // P50 ③:選填;列 `code/` 底下的 `.chg` 檔。
                let paths = value
                    .split(',')
                    .map(|path| path.trim().replace('\\', "/"))
                    .collect::<Vec<_>>();
                if paths.iter().any(String::is_empty) {
                    return Err(config_error(
                        package_hint,
                        line_number,
                        "functions must contain one or more comma-separated paths",
                    ));
                }
                set_once(
                    &mut manifest.function_paths,
                    paths,
                    package_hint,
                    line_number,
                    key,
                )?;
            }
            "data" => {
                // P50 ①:data 與 code 同樣接受逗號分隔的多路徑(先驗快照、
                // Weight DB、分佈表可並存);內容仍串接為單一字串,故既有的
                // library lock digest 自動涵蓋全部檔案(可重現性不破)。
                let paths = value
                    .split(',')
                    .map(|path| path.trim().replace('\\', "/"))
                    .collect::<Vec<_>>();
                if paths.is_empty() || paths.iter().any(String::is_empty) {
                    return Err(config_error(
                        package_hint,
                        line_number,
                        "data must contain one or more comma-separated paths",
                    ));
                }
                set_once(
                    &mut manifest.data_paths,
                    paths,
                    package_hint,
                    line_number,
                    key,
                )?;
            }
            _ => {
                return Err(config_error(
                    package_hint,
                    line_number,
                    format!("unknown key {key:?}"),
                ))
            }
        }
    }
    Ok(manifest)
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && !value.chars().any(|character| {
            character.is_whitespace() || character.is_control() || character == '@'
        })
}

fn validate_manifest_paths(
    package: &str,
    key: &str,
    paths: Vec<String>,
) -> Result<Vec<String>, LibraryLoadError> {
    let mut seen = BTreeSet::new();
    let mut output = Vec::with_capacity(paths.len());
    for raw in paths {
        let path = raw.trim().replace('\\', "/");
        let segments = path.split('/').collect::<Vec<_>>();
        if path.is_empty()
            || path.starts_with('/')
            || path.contains(':')
            || segments
                .iter()
                .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
        {
            return Err(config_error(
                package,
                0,
                format!("{key} contains unsafe relative path {raw:?}"),
            ));
        }
        if !seen.insert(path.clone()) {
            return Err(config_error(
                package,
                0,
                format!("{key} contains duplicate path {path:?}"),
            ));
        }
        output.push(path);
    }
    Ok(output)
}

fn parse_toml_manifest(source: &str) -> Result<Manifest, LibraryLoadError> {
    let raw = toml::from_str::<TomlManifestV2>(source)
        .map_err(|error| config_error("<unknown>", 0, error.to_string()))?;
    if raw.schema != 2 {
        return Err(config_error(
            &raw.id,
            0,
            format!("unsupported package manifest schema {}", raw.schema),
        ));
    }
    if !valid_version(&raw.version) {
        return Err(config_error(
            &raw.id,
            0,
            "version must be a non-empty exact version without whitespace",
        ));
    }
    let id = raw.id.parse::<PackageId>()?;
    let layer = PackageLayer::parse(&raw.layer)
        .ok_or_else(|| config_error(&raw.id, 0, "layer must be reference, overlay, or data"))?;
    let capabilities = PackageCapabilities::parse(&raw.capabilities, &raw.id)?;
    let _exports_path = validate_manifest_paths(&raw.id, "exports", vec![raw.exports])?;
    let requires = raw
        .requires
        .iter()
        .map(|requirement| requirement.parse())
        .collect::<Result<Vec<_>, _>>()?;
    let code_paths = validate_manifest_paths(&raw.id, "code", raw.code)?;
    let function_paths = validate_manifest_paths(&raw.id, "functions", raw.functions)?;
    let data_paths = validate_manifest_paths(&raw.id, "data", raw.data)?;
    if code_paths.is_empty() && function_paths.is_empty() && data_paths.is_empty() {
        return Err(config_error(
            &raw.id,
            0,
            "a package must declare code, functions, or data",
        ));
    }
    Ok(Manifest {
        schema: 2,
        id: Some(id.clone()),
        kind: None,
        name: None,
        version: Some(raw.version),
        layer: Some(layer),
        capabilities: Some(capabilities),
        rule_namespace: Some(raw.rule_namespace.unwrap_or_else(|| id.to_string())),
        enabled: Some(raw.enabled),
        priority: Some(raw.priority),
        requires: Some(requires),
        code_paths: Some(code_paths),
        data_paths: Some(data_paths),
        function_paths: Some(function_paths),
    })
}

fn parse_manifest(source: &str) -> Result<Manifest, LibraryLoadError> {
    let is_v2 = source.lines().any(|raw| {
        let key = raw.trim().split_once('=').map(|(key, _)| key.trim());
        matches!(key, Some("schema" | "id" | "layer" | "capabilities"))
    });
    if is_v2 {
        parse_toml_manifest(source)
    } else {
        parse_legacy_manifest(source)
    }
}

fn required<T>(value: Option<T>, package: &str, key: &str) -> Result<T, LibraryLoadError> {
    value.ok_or_else(|| config_error(package, 0, format!("missing required key {key:?}")))
}

fn parse_exports(
    id: &LibraryId,
    source: &str,
    allow_empty: bool,
) -> Result<Vec<LibraryExport>, LibraryLoadError> {
    if source.trim().is_empty() && allow_empty {
        return Ok(Vec::new());
    }
    let mut lines = source.lines().enumerate().filter_map(|(index, raw)| {
        let line = raw.trim().trim_start_matches('\u{feff}');
        (!line.is_empty() && !line.starts_with('#')).then_some((index + 1, line))
    });
    let Some((header_line, header)) = lines.next() else {
        return Err(LibraryLoadError::Exports {
            package: id.to_string(),
            line: 0,
            message: "missing header".to_owned(),
        });
    };
    if header.split('\t').map(str::trim).collect::<Vec<_>>() != ["stable_id", "kind", "alias"] {
        return Err(LibraryLoadError::Exports {
            package: id.to_string(),
            line: header_line,
            message: "header must be `stable_id<TAB>kind<TAB>alias`".to_owned(),
        });
    }
    let mut exports = Vec::new();
    for (line, value) in lines {
        let fields = value.split('\t').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 3 || fields.iter().any(|field| field.is_empty()) {
            return Err(LibraryLoadError::Exports {
                package: id.to_string(),
                line,
                message: "expected three non-empty tab-separated fields".to_owned(),
            });
        }
        let Some(kind) = LibraryExportKind::parse(fields[1]) else {
            return Err(LibraryLoadError::Exports {
                package: id.to_string(),
                line,
                message: "export kind must be trait or sign".to_owned(),
            });
        };
        if !fields[0].starts_with(&format!("{id}:")) || fields[0].contains(char::is_whitespace) {
            return Err(LibraryLoadError::Exports {
                package: id.to_string(),
                line,
                message: format!("stable id must begin with `{id}:`"),
            });
        }
        if !valid_identifier(fields[2]) {
            return Err(LibraryLoadError::Exports {
                package: id.to_string(),
                line,
                message: "alias must be a single identifier".to_owned(),
            });
        }
        exports.push(LibraryExport {
            package: id.name.clone(),
            package_id: id.clone(),
            stable_id: fields[0].to_owned(),
            kind,
            alias: fields[2].to_owned(),
        });
    }
    if exports.is_empty() && !allow_empty {
        return Err(LibraryLoadError::Exports {
            package: id.to_string(),
            line: 0,
            message: "at least one export is required".to_owned(),
        });
    }
    Ok(exports)
}

/// 解析 `config/tables.tsv`:把套件的 data 檔綁到表型穩定 ID。
///
/// P29 說跨套件契約是穩定 ID,不是套件內部檔案路徑。在這個檔案出現以前,
/// 引擎只能靠檔名尾綴(`ends_with("/weights.tsv")`)認表——套件因此既不能
/// 把已知表型的表放在自己選的路徑下,也不能宣告新表型。
///
/// 空白/缺檔 = 該套件沒有任何具型別的表(合法;所有 data 檔都只進 lock)。
fn parse_tables(
    id: &LibraryId,
    source: &str,
    data_paths: &[String],
) -> Result<BTreeMap<String, String>, LibraryLoadError> {
    let mut tables = BTreeMap::new();
    if source.trim().is_empty() {
        return Ok(tables);
    }
    let mut lines = source.lines().enumerate().filter_map(|(index, raw)| {
        let line = raw.trim().trim_start_matches('\u{feff}');
        (!line.is_empty() && !line.starts_with('#')).then_some((index + 1, line))
    });
    let error = |line: usize, message: &str| LibraryLoadError::Tables {
        package: id.to_string(),
        line,
        message: message.to_owned(),
    };
    let Some((header_line, header)) = lines.next() else {
        return Ok(tables);
    };
    if header.split('\t').map(str::trim).collect::<Vec<_>>() != ["path", "type"] {
        return Err(error(header_line, "header must be `path<TAB>type`"));
    }
    for (line, value) in lines {
        let fields = value.split('\t').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 2 || fields.iter().any(|field| field.is_empty()) {
            return Err(error(line, "expected two non-empty tab-separated fields"));
        }
        let path = fields[0].replace('\\', "/");
        if !data_paths.contains(&path) {
            return Err(error(
                line,
                &format!("path {path:?} is not declared in the manifest `data` list"),
            ));
        }
        if !valid_table_type(fields[1]) {
            return Err(error(
                line,
                &format!(
                    "table type {:?} must be two or more `:`-separated identifiers",
                    fields[1]
                ),
            ));
        }
        if tables.insert(path.clone(), fields[1].to_owned()).is_some() {
            return Err(error(line, &format!("duplicate entry for path {path:?}")));
        }
    }
    Ok(tables)
}

fn validate_package_code(package: &LibraryPackage) -> Result<Language, LibraryLoadError> {
    let mut language =
        Language::parse(&package.code).map_err(|error| LibraryLoadError::PackageLanguage {
            package: package.id.to_string(),
            line: error.line,
            message: error.msg,
        })?;
    if !package.capabilities.traits && !language.traits.is_empty() {
        return Err(LibraryLoadError::UnsupportedContent {
            package: package.id.to_string(),
            message: "manifest does not grant the traits capability".to_owned(),
        });
    }
    if !package.capabilities.signs && !language.signs.is_empty() {
        return Err(LibraryLoadError::UnsupportedContent {
            package: package.id.to_string(),
            message: "manifest does not grant the signs capability".to_owned(),
        });
    }
    if package.layer == PackageLayer::Data
        && (!language.dsl_decls.is_empty()
            || !language.distribution.is_empty()
            || !language.traits.is_empty()
            || !language.signs.is_empty())
    {
        return Err(LibraryLoadError::UnsupportedContent {
            package: package.id.to_string(),
            message: "data-layer packages may not contain language declarations".to_owned(),
        });
    }
    if package.layer == PackageLayer::Reference {
        if !language.dsl_decls.is_empty()
            || !language.distribution.is_empty()
            || !language.signs.is_empty()
        {
            return Err(LibraryLoadError::UnsupportedContent {
                package: package.id.to_string(),
                message: "reference code may contain trait declarations only".to_owned(),
            });
        }
        for trait_def in &language.traits {
            if trait_def.global {
                return Err(LibraryLoadError::UnsupportedContent {
                    package: package.id.to_string(),
                    message: format!(
                        "global trait {:?} is not supported in reference packages",
                        trait_def.name
                    ),
                });
            }
            for item in trait_def.blocks.iter().flat_map(|block| &block.items) {
                if !matches!(
                    item,
                    SignItem::TraitMount { name: _, kind: crate::TraitMountKind::Declaration, .. }
                        // `belongs X` 載入並確定 trait,`X[n]` 是具體的展開點
                        // ——**兩個語法形式指向同一份掛載**,`TraitUse` 不可能
                        // 獨立出現。白名單允許 `Belongs` 卻禁止 `TraitUse`,
                        // 等於允許宣告、禁止那個宣告強制要求的展開。
                        | SignItem::TraitMount { kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_), .. }
                        // `pass` 是塊形狀的標記,比任何內容都少
                        | SignItem::Pass
                        | SignItem::Def(_)
                        | SignItem::Slot(_)
                        | SignItem::FeatureDecl(_)
                        | SignItem::FeatureValue(_)
                        | SignItem::SlotFeatureBinding(_)
                        | SignItem::RoleDecl(_)
                        | SignItem::RoleBinding(_)
                        | SignItem::Realization(_)
                        | SignItem::Constraint(_)
                        | SignItem::FeatureRule(_)
                        | SignItem::Rule(_)
                ) {
                    return Err(LibraryLoadError::UnsupportedContent {
                        package: package.id.to_string(),
                        message: format!(
                            "reference trait {:?} contains unsupported item {item:?}",
                            trait_def.name
                        ),
                    });
                }
            }
        }
    }
    language.bind_rule_namespace(RuleNamespace::Package(package.rule_namespace.clone()));
    Ok(language)
}

/// 由 owned 來源建 package。與 [`load_embedded`] **共用同一套解析與驗證**
/// ——內嵌與注入不得有兩套規則,否則「外部 package 為什麼過不了」會變成
/// 需要對照兩份實作才答得出的問題。
fn load_sources(sources: &PackageSources) -> Result<LibraryPackage, LibraryLoadError> {
    let embedded = OwnedPackageView {
        config: &sources.config,
        exports: &sources.exports,
        tables: &sources.tables,
        code: &sources.code,
        functions: &sources.functions,
        data: &sources.data,
        data_files: &sources.data_files,
        source: &sources.source,
    };
    load_package(embedded)
}

/// 借用視圖,讓內嵌(`&'static str`)與注入(`String`)走同一條路。
struct OwnedPackageView<'a> {
    config: &'a str,
    exports: &'a str,
    tables: &'a str,
    code: &'a str,
    functions: &'a [PackageFile],
    data: &'a str,
    data_files: &'a [PackageFile],
    source: &'a PackageSource,
}

fn load_embedded(source: &EmbeddedPackage) -> Result<LibraryPackage, LibraryLoadError> {
    let functions = source
        .functions
        .iter()
        .map(|f| PackageFile {
            path: f.path.to_owned(),
            source: f.source.to_owned(),
        })
        .collect::<Vec<_>>();
    let data_files = source
        .data_sources
        .iter()
        .map(|d| PackageFile {
            path: d.path.to_owned(),
            source: d.source.to_owned(),
        })
        .collect::<Vec<_>>();
    load_package(OwnedPackageView {
        config: source.config,
        exports: source.exports,
        tables: source.tables,
        code: source.code,
        functions: &functions,
        data: source.data,
        data_files: &data_files,
        source: &PackageSource::Embedded,
    })
}

fn load_package(source: OwnedPackageView<'_>) -> Result<LibraryPackage, LibraryLoadError> {
    let manifest = parse_manifest(source.config)?;
    let schema = manifest.schema;
    let legacy_kind = manifest.kind;
    let id = match manifest.id {
        Some(id) => id,
        None => {
            let name = required(manifest.name, "<unknown>", "name")?;
            let kind = required(legacy_kind, &name, "kind")?;
            LibraryId::new(kind, name)
        }
    };
    let layer = manifest.layer.unwrap_or(match legacy_kind {
        Some(LibraryKind::Std) => PackageLayer::Reference,
        Some(LibraryKind::Natural | LibraryKind::Plugin) | None => PackageLayer::Overlay,
    });
    let rule_namespace = required(manifest.rule_namespace, &id.to_string(), "rule_namespace")?;
    if rule_namespace != id.to_string() {
        return Err(config_error(
            &id.to_string(),
            0,
            format!("rule_namespace must equal package id {id}"),
        ));
    }
    let code_paths = manifest.code_paths.unwrap_or_default();
    let data_paths = match manifest.data_paths {
        Some(paths) => paths,
        None if schema == 1 => {
            return Err(config_error(
                &id.to_string(),
                0,
                "missing required key \"data\"",
            ))
        }
        None => Vec::new(),
    };
    let embedded_data_paths = source
        .data_files
        .iter()
        .map(|data| data.path.as_str())
        .collect::<Vec<_>>();
    if data_paths.iter().map(String::as_str).collect::<Vec<_>>() != embedded_data_paths {
        return Err(config_error(
            &id.to_string(),
            0,
            format!(
                "data manifest paths {data_paths:?} do not match provided sources {embedded_data_paths:?}"
            ),
        ));
    }
    let function_paths = manifest.function_paths.unwrap_or_default();
    let embedded_function_paths = source
        .functions
        .iter()
        .map(|function| function.path.as_str())
        .collect::<Vec<_>>();
    if function_paths
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != embedded_function_paths
    {
        return Err(config_error(
            &id.to_string(),
            0,
            format!(
                "functions manifest paths {function_paths:?} do not match provided sources {embedded_function_paths:?}"
            ),
        ));
    }
    if code_paths.is_empty() && function_paths.is_empty() && data_paths.is_empty() {
        return Err(config_error(
            &id.to_string(),
            0,
            "a package must declare code, functions, or data",
        ));
    }
    let allow_empty_exports = schema >= 2
        && manifest.capabilities.is_some_and(|capabilities| {
            !capabilities.traits && !capabilities.signs && !capabilities.functions
        });
    let exports = parse_exports(&id, source.exports, allow_empty_exports)?;
    let tables = parse_tables(&id, source.tables, &data_paths)?;
    let capabilities = manifest.capabilities.unwrap_or(PackageCapabilities {
        traits: exports
            .iter()
            .any(|export| export.kind == LibraryExportKind::Trait),
        signs: exports
            .iter()
            .any(|export| export.kind == LibraryExportKind::Sign),
        functions: !function_paths.is_empty()
            || exports
                .iter()
                .any(|export| export.kind == LibraryExportKind::Function),
        data: !data_paths.is_empty(),
    });
    let package = LibraryPackage {
        exports,
        name: id.name.clone(),
        id,
        version: required(manifest.version, "<unknown>", "version")?,
        manifest_schema: schema,
        layer,
        capabilities,
        source: source.source.clone(),
        rule_namespace,
        enabled: required(manifest.enabled, "<unknown>", "enabled")?,
        priority: required(manifest.priority, "<unknown>", "priority")?,
        requires: manifest.requires.unwrap_or_default(),
        code_path: code_paths.join(","),
        code_paths,
        data_path: data_paths.join(","),
        data_paths,
        data_sources: source
            .data_files
            .iter()
            .map(|data| LibraryDataSource {
                table_type: tables.get(&data.path).cloned(),
                path: data.path.to_owned(),
                source: data.source.to_owned(),
            })
            .collect(),
        function_paths,
        function_sources: source
            .functions
            .iter()
            .map(|function| LibraryFunctionSource {
                path: function.path.to_owned(),
                source: function.source.to_owned(),
            })
            .collect(),
        code: source.code.to_owned(),
        functions: String::new(),
        data: source.data.to_owned(),
    };
    if !package.capabilities.functions && !package.function_paths.is_empty() {
        return Err(LibraryLoadError::UnsupportedContent {
            package: package.id.to_string(),
            message: "manifest does not grant the functions capability".to_owned(),
        });
    }
    if !package.capabilities.data && !package.data_paths.is_empty() {
        return Err(LibraryLoadError::UnsupportedContent {
            package: package.id.to_string(),
            message: "manifest does not grant the data capability".to_owned(),
        });
    }
    let language = validate_package_code(&package)?;
    for export in &package.exports {
        let present = match export.kind {
            LibraryExportKind::Trait => language.trait_named(&export.alias).is_some(),
            LibraryExportKind::Sign => language.sign_named(&export.alias).is_some(),
            // function export 的存在性**由 changeset 端查驗**——`language` 不得
            // 解析 `.chg`(P20 依賴方向)。這裡只確認套件真的有帶 function 原始碼;
            // 名字對不對由 `changeset::function` 載入 function 表時報錯。
            LibraryExportKind::Function => {
                !package.function_sources.is_empty() || !package.functions.trim().is_empty()
            }
        };
        if !present {
            return Err(LibraryLoadError::MissingAlias {
                package: package.id.clone(),
                kind: export.kind,
                alias: export.alias.clone(),
            });
        }
        let granted = match export.kind {
            LibraryExportKind::Trait => package.capabilities.traits,
            LibraryExportKind::Sign => package.capabilities.signs,
            LibraryExportKind::Function => package.capabilities.functions,
        };
        if !granted {
            return Err(LibraryLoadError::UnsupportedContent {
                package: package.id.to_string(),
                message: format!(
                    "export {:?} is not granted by package capabilities",
                    export.kind
                ),
            });
        }
    }
    Ok(package)
}

fn lock_normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Canonical semantic content used by project locks and ChangeSet replay.
/// Schema-1 packages retain the historical byte layout exactly; schema 2 adds
/// the behavior-bearing manifest fields that did not exist in v1.
pub fn package_lock_content(package: &LibraryPackage) -> String {
    let mut content = String::new();
    if package.manifest_schema >= 2 {
        content.push_str(&format!(
            "manifest-schema {}\nlayer {}\ncapabilities {}\n",
            package.manifest_schema,
            package.layer,
            package.capabilities.canonical()
        ));
    }
    content.push_str(&format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n",
        package.id,
        package.version,
        package.rule_namespace,
        package.priority,
        package.code_path,
        package.data_path
    ));
    for dependency in &package.requires {
        content.push_str(&format!("requires {dependency}\n"));
    }
    for export in &package.exports {
        content.push_str(&format!(
            "export {} {:?} {}\n",
            export.stable_id, export.kind, export.alias
        ));
    }
    // 表型決定哪個解析器讀這份表,是承載行為的宣告,故必須進 digest。只印
    // 有宣告表型的來源:未宣告的套件因此逐位元維持舊 lock 內容,digest 不動。
    for source in &package.data_sources {
        if let Some(table_type) = &source.table_type {
            content.push_str(&format!("table {} {}\n", source.path, table_type));
        }
    }
    content.push_str(&lock_normalized(&package.code));
    content.push('\n');
    if package.function_sources.is_empty() {
        content.push_str(&lock_normalized(&package.functions));
    } else {
        for source in &package.function_sources {
            content.push_str("\nfunction-source ");
            content.push_str(&source.path);
            content.push('\n');
            content.push_str(&lock_normalized(&source.source));
        }
    }
    content.push('\n');
    if package.data_sources.len() <= 1 {
        content.push_str(&lock_normalized(&package.data));
    } else {
        for source in &package.data_sources {
            content.push_str("\ndata-source ");
            content.push_str(&source.path);
            content.push('\n');
            content.push_str(&lock_normalized(&source.source));
        }
    }
    content
}

pub fn package_digest(package: &LibraryPackage) -> String {
    crate::sha256_hex(package_lock_content(package).as_bytes())
}

#[derive(Debug, Clone)]
pub struct LibraryCatalog {
    packages: Vec<LibraryPackage>,
}

/// Immutable result of one resolver pass.  Compile, check, lock generation,
/// and replay consume this same value; none of them rediscover packages.
#[derive(Debug, Clone)]
pub struct ResolvedPackages {
    pub intent: PackageSpec,
    pub selection: LibrarySelection,
    packages: Vec<LibraryPackage>,
    available_exports: BTreeMap<String, PackageId>,
}

impl ResolvedPackages {
    pub fn packages(&self) -> &[LibraryPackage] {
        &self.packages
    }

    pub fn package(&self, id: &PackageId) -> Option<&LibraryPackage> {
        self.packages.iter().find(|package| &package.id == id)
    }

    pub fn available_exports(&self) -> &BTreeMap<String, PackageId> {
        &self.available_exports
    }

    pub fn lock_digest(&self) -> String {
        let mut packages = self.selection.resolved.clone();
        packages.sort_by(|left, right| left.id.cmp(&right.id));
        let content = packages
            .iter()
            .map(|package| {
                format!(
                    "{}@{} sha256:{}\n",
                    package.id, package.version, package.digest
                )
            })
            .collect::<String>();
        crate::sha256_hex(content.as_bytes())
    }
}

pub trait PackageResolver {
    fn resolve(&self, spec: &PackageSpec) -> Result<ResolvedPackages, LibraryLoadError>;
}

fn validate_catalog(packages: &[LibraryPackage]) -> Result<(), LibraryLoadError> {
    let mut ids = BTreeSet::new();
    let mut namespaces = BTreeMap::<String, LibraryId>::new();
    let mut stable_ids = BTreeMap::<String, LibraryId>::new();
    let mut aliases = BTreeMap::<String, LibraryId>::new();
    for package in packages {
        if !ids.insert(package.id.clone()) {
            return Err(LibraryLoadError::DuplicatePackage(package.id.clone()));
        }
        if let Some(first) = namespaces.insert(package.rule_namespace.clone(), package.id.clone()) {
            return Err(LibraryLoadError::DuplicateRuleNamespace {
                namespace: package.rule_namespace.clone(),
                first,
                second: package.id.clone(),
            });
        }
        for export in &package.exports {
            if let Some(first) = stable_ids.insert(export.stable_id.clone(), package.id.clone()) {
                return Err(LibraryLoadError::DuplicateStableId {
                    stable_id: export.stable_id.clone(),
                    first,
                    second: package.id.clone(),
                });
            }
            if let Some(first) = aliases.insert(export.alias.clone(), package.id.clone()) {
                return Err(LibraryLoadError::DuplicateAlias {
                    alias: export.alias.clone(),
                    first,
                    second: package.id.clone(),
                });
            }
        }
    }
    Ok(())
}

fn sort_catalog(packages: &mut [LibraryPackage]) {
    packages.sort_by(|left, right| {
        left.layer
            .rank()
            .cmp(&right.layer.rank())
            .then(left.priority.cmp(&right.priority))
            .then(left.id.namespace.cmp(&right.id.namespace))
            .then(left.id.name.cmp(&right.id.name))
    });
}

impl LibraryCatalog {
    pub fn embedded() -> Result<Self, LibraryLoadError> {
        let mut packages = EMBEDDED_PACKAGES
            .iter()
            .map(load_embedded)
            .collect::<Result<Vec<_>, _>>()?;
        sort_catalog(&mut packages);
        validate_catalog(&packages)?;
        Ok(Self { packages })
    }

    /// 在隨引擎發布的那組之外,**再注入** host 提供的 package(R9-a)。
    ///
    /// 此前 `embedded()` 是唯一入口,吃編譯期常數陣列——`plugin` kind 因此
    /// **在任何執行路徑上都不可達**,E1 先驗庫(PHOIBLE/Grambank 全集)也無處可去
    /// (它們差好幾個數量級,不可能 `include_str!`)。
    ///
    /// 注入者仍受同一套把關:kind/dependency/priority/exports/rule namespace
    /// 由 `validate_catalog` 檢查,内容雜湊由 `.chg` 的第三道 digest 釘住
    /// ——**內嵌對可重現性從來沒有貢獻**,那是 lock 的職責。
    pub fn with_packages(
        extra: impl IntoIterator<Item = PackageSources>,
    ) -> Result<Self, LibraryLoadError> {
        let mut packages = EMBEDDED_PACKAGES
            .iter()
            .map(load_embedded)
            .collect::<Result<Vec<_>, _>>()?;
        for sources in extra {
            packages.push(load_sources(&sources)?);
        }
        sort_catalog(&mut packages);
        validate_catalog(&packages)?;
        Ok(Self { packages })
    }

    /// Build an offline catalog in host search order: project-vendored,
    /// user-installed, then shipped embedded fallback.  Higher tiers replace
    /// lower-tier candidates with the same package ID.
    pub fn with_source_precedence(
        vendored: impl IntoIterator<Item = PackageSources>,
        installed: impl IntoIterator<Item = PackageSources>,
    ) -> Result<Self, LibraryLoadError> {
        fn load_tier(
            sources: impl IntoIterator<Item = PackageSources>,
        ) -> Result<Vec<LibraryPackage>, LibraryLoadError> {
            let mut packages = Vec::new();
            let mut ids = BTreeSet::new();
            for source in sources {
                let package = load_sources(&source)?;
                if !ids.insert(package.id.clone()) {
                    return Err(LibraryLoadError::DuplicatePackage(package.id));
                }
                packages.push(package);
            }
            Ok(packages)
        }

        let vendored = load_tier(vendored)?;
        let installed = load_tier(installed)?;
        let embedded = EMBEDDED_PACKAGES
            .iter()
            .map(load_embedded)
            .collect::<Result<Vec<_>, _>>()?;
        let mut chosen = BTreeMap::<PackageId, LibraryPackage>::new();
        for package in embedded.into_iter().chain(installed).chain(vendored) {
            chosen.insert(package.id.clone(), package);
        }
        let mut packages = chosen.into_values().collect::<Vec<_>>();
        sort_catalog(&mut packages);
        validate_catalog(&packages)?;
        Ok(Self { packages })
    }

    pub fn packages(&self) -> &[LibraryPackage] {
        &self.packages
    }

    pub fn resolve_legacy(&self, spec: &LibrarySpec) -> Result<ResolvedPackages, LibraryLoadError> {
        self.validate_legacy_spec(spec)?;
        self.resolve(&PackageSpec::from_legacy(spec))
    }

    fn validate_legacy_spec(&self, spec: &LibrarySpec) -> Result<(), LibraryLoadError> {
        for id in &spec.std {
            if id.namespace != LibraryKind::Std.keyword() {
                return Err(LibraryLoadError::WrongKind {
                    id: id.clone(),
                    expected: LibraryKind::Std,
                    actual: id.namespace.clone(),
                });
            }
        }
        if let Some(natural) = &spec.natural {
            if natural.namespace != LibraryKind::Natural.keyword() {
                return Err(LibraryLoadError::WrongKind {
                    id: natural.clone(),
                    expected: LibraryKind::Natural,
                    actual: natural.namespace.clone(),
                });
            }
        }
        for plugin in &spec.plugins {
            if plugin.namespace != LibraryKind::Plugin.keyword() {
                return Err(LibraryLoadError::WrongKind {
                    id: plugin.clone(),
                    expected: LibraryKind::Plugin,
                    actual: plugin.namespace.clone(),
                });
            }
        }
        Ok(())
    }

    /// 名字 → 匯出它的 package(**全 catalog**,不限已選取者)。
    ///
    /// 供 R13 的指路訊息使用:只在名字查無時查詢,故命中必然是尚未宣告的套件。
    /// 同名由多個 package 匯出時取排序後第一個(catalog 已是決定性排序)。
    pub fn export_index(&self) -> std::collections::BTreeMap<String, LibraryId> {
        let mut index = std::collections::BTreeMap::new();
        for package in &self.packages {
            for export in &package.exports {
                index
                    .entry(export.alias.clone())
                    .or_insert_with(|| package.id.clone());
            }
        }
        index
    }

    pub fn resolve_export(
        &self,
        kind: LibraryKind,
        alias: &str,
    ) -> Result<LibraryExport, LibraryLoadError> {
        self.packages
            .iter()
            .filter(|package| package.enabled && package.id.legacy_kind() == Some(kind))
            .flat_map(|package| package.exports.iter())
            .find(|export| export.alias == alias)
            .cloned()
            .ok_or_else(|| LibraryLoadError::UnknownAlias(alias.to_owned()))
    }

    fn package(&self, id: &LibraryId) -> Result<&LibraryPackage, LibraryLoadError> {
        self.packages
            .iter()
            .find(|package| &package.id == id)
            .ok_or_else(|| LibraryLoadError::UnknownPackage(id.clone()))
    }

    fn package_for_requirement(
        &self,
        requirement: &PackageRequirement,
    ) -> Result<&LibraryPackage, LibraryLoadError> {
        let package = self.package(&requirement.id)?;
        if let Some(expected) = &requirement.version {
            if !valid_version(expected) {
                return Err(LibraryLoadError::InvalidRequirement(
                    requirement.to_string(),
                ));
            }
            if &package.version != expected {
                return Err(LibraryLoadError::VersionMismatch {
                    package: requirement.id.clone(),
                    expected: expected.clone(),
                    actual: package.version.clone(),
                });
            }
        }
        Ok(package)
    }

    fn sort_requirements(
        &self,
        requirements: &mut [PackageRequirement],
    ) -> Result<(), LibraryLoadError> {
        for requirement in requirements.iter() {
            self.package_for_requirement(requirement)?;
        }
        requirements.sort_by(|left, right| {
            let left_package = self.packages.iter().find(|package| package.id == left.id);
            let right_package = self.packages.iter().find(|package| package.id == right.id);
            match (left_package, right_package) {
                (Some(left_package), Some(right_package)) => left_package
                    .layer
                    .rank()
                    .cmp(&right_package.layer.rank())
                    .then(left_package.priority.cmp(&right_package.priority))
                    .then(left.id.cmp(&right.id)),
                _ => left.id.cmp(&right.id),
            }
        });
        Ok(())
    }

    fn visit(
        &self,
        id: &LibraryId,
        states: &mut BTreeMap<LibraryId, u8>,
        stack: &mut Vec<LibraryId>,
        ordered: &mut Vec<LibraryId>,
    ) -> Result<(), LibraryLoadError> {
        match states.get(id).copied() {
            Some(2) => return Ok(()),
            Some(1) => {
                stack.push(id.clone());
                return Err(LibraryLoadError::DependencyCycle(
                    stack
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" -> "),
                ));
            }
            _ => {}
        }
        let package = self.package(id)?;
        if !package.enabled {
            return Err(LibraryLoadError::DisabledPackage(id.clone()));
        }
        states.insert(id.clone(), 1);
        stack.push(id.clone());
        let mut dependencies = package.requires.clone();
        self.sort_requirements(&mut dependencies)?;
        for dependency in &dependencies {
            self.package_for_requirement(dependency)?;
            self.visit(&dependency.id, states, stack, ordered)?;
        }
        stack.pop();
        states.insert(id.clone(), 2);
        ordered.push(id.clone());
        Ok(())
    }

    pub fn select(&self, spec: &LibrarySpec) -> Result<LibrarySelection, LibraryLoadError> {
        self.validate_legacy_spec(spec)?;
        self.select_packages(&PackageSpec::from_legacy(spec))
    }

    pub fn select_packages(
        &self,
        spec: &PackageSpec,
    ) -> Result<LibrarySelection, LibraryLoadError> {
        let mut root_versions = BTreeMap::<PackageId, Option<String>>::new();
        for root in &spec.roots {
            match root_versions.get_mut(&root.id) {
                None => {
                    root_versions.insert(root.id.clone(), root.version.clone());
                }
                Some(existing) => match (&*existing, &root.version) {
                    (Some(left), Some(right)) if left != right => {
                        return Err(LibraryLoadError::ConflictingRequirements {
                            package: root.id.clone(),
                        })
                    }
                    (None, Some(version)) => *existing = Some(version.clone()),
                    _ => {}
                },
            }
        }
        let mut roots = root_versions
            .into_iter()
            .map(|(id, version)| PackageRequirement { id, version })
            .collect::<Vec<_>>();
        self.sort_requirements(&mut roots)?;
        let mut states = BTreeMap::new();
        let mut ordered = Vec::new();
        for root in roots {
            self.package_for_requirement(&root)?;
            self.visit(&root.id, &mut states, &mut Vec::new(), &mut ordered)?;
        }
        for (alias, package) in &spec.aliases {
            if !valid_identifier(alias) {
                return Err(LibraryLoadError::InvalidPackageAlias(alias.clone()));
            }
            if !ordered.contains(package) {
                return Err(LibraryLoadError::AliasTargetNotSelected {
                    alias: alias.clone(),
                    package: package.clone(),
                });
            }
        }

        let mut standard = Language::new();
        let mut overlay = Language::new();
        let mut trait_names = BTreeSet::new();
        let mut sign_names = BTreeSet::new();
        let mut resolved = Vec::new();
        for id in &ordered {
            let package = self.package(id)?;
            let language = validate_package_code(package)?;
            for trait_def in &language.traits {
                if !trait_names.insert(trait_def.name.clone()) {
                    return Err(LibraryLoadError::DuplicateTrait(trait_def.name.clone()));
                }
            }
            for sign in &language.signs {
                if !sign_names.insert(sign.name.clone()) {
                    return Err(LibraryLoadError::DuplicateSign(sign.name.clone()));
                }
            }
            match package.layer {
                PackageLayer::Reference => standard.append_library(language),
                PackageLayer::Overlay => overlay.append_library(language),
                PackageLayer::Data => {}
            }
            resolved.push(SelectedPackage {
                id: package.id.clone(),
                version: package.version.clone(),
                digest: package_digest(package),
                source: package.source.clone(),
                layer: package.layer,
            });
        }
        Ok(LibrarySelection {
            standard,
            overlay,
            packages: ordered,
            resolved,
        })
    }
}

impl PackageResolver for LibraryCatalog {
    fn resolve(&self, spec: &PackageSpec) -> Result<ResolvedPackages, LibraryLoadError> {
        let selection = self.select_packages(spec)?;
        let packages = selection
            .packages
            .iter()
            .map(|id| self.package(id).cloned())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ResolvedPackages {
            intent: spec.clone(),
            selection,
            packages,
            available_exports: self.export_index(),
        })
    }
}

pub fn embedded_catalog() -> Result<LibraryCatalog, LibraryLoadError> {
    LibraryCatalog::embedded()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_plugin(name: &str, priority: i32) -> LibraryPackage {
        let id = LibraryId::new(LibraryKind::Plugin, name);
        LibraryPackage {
            name: name.to_owned(),
            rule_namespace: id.to_string(),
            id,
            version: "test".to_owned(),
            manifest_schema: 1,
            layer: PackageLayer::Overlay,
            capabilities: PackageCapabilities::default(),
            source: PackageSource::default(),
            enabled: true,
            priority,
            requires: Vec::new(),
            code_paths: vec!["code/test.lang".to_owned()],
            code_path: "code/test.lang".to_owned(),
            data_path: "data/test.tsv".to_owned(),
            data_paths: vec!["data/test.tsv".to_owned()],
            data_sources: Vec::new(),
            function_paths: Vec::new(),
            function_sources: Vec::new(),
            exports: Vec::new(),
            code: String::new(),
            functions: String::new(),
            data: String::new(),
        }
    }

    /// P50 ①:`data` 與 `code` 同樣接受逗號分隔的多路徑(先驗快照 / Weight DB /
    /// 分佈表可並存)。內容仍串接為單一字串,故既有 library lock digest 自動涵蓋。
    #[test]
    fn a_manifest_accepts_several_data_paths() {
        let manifest = parse_manifest(
            "kind = std\nname = t\nversion = 0.1.0\nrule_namespace = std:t\nenabled = true\npriority = 0\nrequires =\ncode = code/a.lang\ndata = data/paths.tsv, data/weights.tsv\n",
        )
        .expect("multi-data manifest parses");
        assert_eq!(
            manifest.data_paths.as_deref(),
            Some(["data/paths.tsv".to_owned(), "data/weights.tsv".to_owned()].as_slice())
        );
    }

    #[test]
    fn an_empty_data_path_is_rejected() {
        assert!(parse_manifest(
            "kind = std\nname = t\nversion = 0.1.0\nrule_namespace = std:t\nenabled = true\npriority = 0\nrequires =\ncode = code/a.lang\ndata = data/a.tsv,\n",
        )
        .is_err());
    }

    /// P50 ②:`exports.tsv` 認得 `function` 這個 kind(歷時 function 層)。
    #[test]
    fn the_export_table_understands_functions() {
        assert_eq!(
            LibraryExportKind::parse("function"),
            Some(LibraryExportKind::Function)
        );
        assert_eq!(LibraryExportKind::parse("recipe"), None, "非關鍵字(P48)");
    }

    #[test]
    fn catalog_rejects_duplicate_identity_surfaces() {
        let catalog = LibraryCatalog::embedded().unwrap();

        let mut duplicate_package = catalog.packages.clone();
        duplicate_package.push(duplicate_package[0].clone());
        assert!(matches!(
            validate_catalog(&duplicate_package),
            Err(LibraryLoadError::DuplicatePackage(_))
        ));

        let mut namespace = catalog.packages.clone();
        namespace[1].rule_namespace = namespace[0].rule_namespace.clone();
        assert!(matches!(
            validate_catalog(&namespace),
            Err(LibraryLoadError::DuplicateRuleNamespace { .. })
        ));

        let mut stable_id = catalog.packages.clone();
        stable_id[1].exports[0].stable_id = stable_id[0].exports[0].stable_id.clone();
        assert!(matches!(
            validate_catalog(&stable_id),
            Err(LibraryLoadError::DuplicateStableId { .. })
        ));

        let mut alias = catalog.packages.clone();
        alias[1].exports[0].alias = alias[0].exports[0].alias.clone();
        assert!(matches!(
            validate_catalog(&alias),
            Err(LibraryLoadError::DuplicateAlias { .. })
        ));
    }

    #[test]
    fn selection_rejects_unknown_disabled_and_cyclic_dependencies() {
        let catalog = LibraryCatalog::embedded().unwrap();
        let english = LibraryId::new(LibraryKind::Natural, "en-standard");

        let mut unknown = catalog.clone();
        unknown
            .packages
            .iter_mut()
            .find(|package| package.id == english)
            .unwrap()
            .requires
            .push(LibraryId::new(LibraryKind::Std, "missing").into());
        let unknown_error = unknown
            .select(&LibrarySpec::natural(english.clone()))
            .unwrap_err();
        assert!(matches!(
            &unknown_error,
            LibraryLoadError::UnknownPackage(_)
        ));
        assert_eq!(unknown_error.code(), "LIBRARY_PACKAGE_UNKNOWN");

        let mut disabled = catalog.clone();
        disabled
            .packages
            .iter_mut()
            .find(|package| package.id == LibraryId::new(LibraryKind::Std, "core"))
            .unwrap()
            .enabled = false;
        let disabled_error = disabled.select(&LibrarySpec::default()).unwrap_err();
        assert!(matches!(
            &disabled_error,
            LibraryLoadError::DisabledPackage(_)
        ));
        assert_eq!(disabled_error.code(), "LIBRARY_PACKAGE_DISABLED");

        let mut cycle = catalog.clone();
        cycle
            .packages
            .iter_mut()
            .find(|package| package.id == LibraryId::new(LibraryKind::Std, "core"))
            .unwrap()
            .requires
            .push(LibraryId::new(LibraryKind::Std, "cxg").into());
        let cycle_error = cycle.select(&LibrarySpec::default()).unwrap_err();
        assert!(matches!(&cycle_error, LibraryLoadError::DependencyCycle(_)));
        assert_eq!(cycle_error.code(), "LIBRARY_DEPENDENCY_CYCLE");
    }

    #[test]
    fn selected_plugins_use_priority_then_package_name() {
        let mut catalog = LibraryCatalog::embedded().unwrap();
        for package in [
            empty_plugin("late", 10),
            empty_plugin("beta", 5),
            empty_plugin("alpha", 5),
        ] {
            catalog.packages.push(package);
        }
        let spec = LibrarySpec::default()
            .with_plugin(LibraryId::new(LibraryKind::Plugin, "late"))
            .with_plugin(LibraryId::new(LibraryKind::Plugin, "beta"))
            .with_plugin(LibraryId::new(LibraryKind::Plugin, "alpha"));
        let selected = catalog.select(&spec).unwrap();
        assert_eq!(
            selected
                .packages
                .iter()
                .filter(|id| id.legacy_kind() == Some(LibraryKind::Plugin))
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["plugin:alpha", "plugin:beta", "plugin:late"]
        );
    }

    #[test]
    fn std_traits_reject_source_declared_slot_maps() {
        let id = LibraryId::new(LibraryKind::Std, "slotmap-boundary");
        let package = LibraryPackage {
            name: id.name.clone(),
            id: id.clone(),
            version: "test".to_owned(),
            manifest_schema: 1,
            layer: PackageLayer::Reference,
            capabilities: PackageCapabilities {
                traits: true,
                ..PackageCapabilities::default()
            },
            source: PackageSource::default(),
            rule_namespace: id.to_string(),
            enabled: true,
            priority: 0,
            requires: Vec::new(),
            code_paths: vec!["code/schema.lang".to_owned()],
            code_path: "code/schema.lang".to_owned(),
            data_path: "data/none.tsv".to_owned(),
            data_paths: vec!["data/none.tsv".to_owned()],
            data_sources: Vec::new(),
            function_paths: Vec::new(),
            function_sources: Vec::new(),
            exports: Vec::new(),
            code: "trait Schema:\n    syn:\n        slots:\n            head [Noun]\n        map head rename nucleus\n".to_owned(),
            functions: String::new(),
            data: String::new(),
        };

        let error = validate_package_code(&package)
            .expect_err("std package content must not declare a source SlotMap");
        assert!(matches!(
            error,
            LibraryLoadError::UnsupportedContent { package, message }
                if package == "std:slotmap-boundary" && message.contains("SlotMap")
        ));
    }
}
