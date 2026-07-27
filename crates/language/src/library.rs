//! Deterministic embedded library catalog shared by standard, plugin, and
//! natural-language packages.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

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

    fn rank(self) -> u8 {
        match self {
            LibraryKind::Std => 0,
            LibraryKind::Natural => 1,
            LibraryKind::Plugin => 2,
        }
    }
}

impl fmt::Display for LibraryKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.keyword())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LibraryId {
    pub kind: LibraryKind,
    pub name: String,
}

impl LibraryId {
    pub fn new(kind: LibraryKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
        }
    }
}

impl fmt::Display for LibraryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.kind, self.name)
    }
}

impl FromStr for LibraryId {
    type Err = LibraryLoadError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((kind, name)) = value.split_once(':') else {
            return Err(LibraryLoadError::InvalidId(value.to_owned()));
        };
        let Some(kind) = LibraryKind::parse(kind) else {
            return Err(LibraryLoadError::InvalidId(value.to_owned()));
        };
        if !valid_identifier(name) {
            return Err(LibraryLoadError::InvalidId(value.to_owned()));
        }
        Ok(Self::new(kind, name))
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
pub struct LibraryPackage {
    pub id: LibraryId,
    /// Compatibility short name used by the former stdlib API.
    pub name: String,
    pub version: String,
    pub rule_namespace: String,
    pub enabled: bool,
    pub priority: i32,
    pub requires: Vec<LibraryId>,
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
    pub exports: Vec<LibraryExport>,
    pub code: &'static str,
    pub data: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibrarySpec {
    pub natural: Option<LibraryId>,
    pub plugins: Vec<LibraryId>,
}

impl LibrarySpec {
    pub fn natural(id: LibraryId) -> Self {
        Self {
            natural: Some(id),
            plugins: Vec::new(),
        }
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
    pub packages: Vec<LibraryId>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LibraryLoadError {
    #[error("invalid library id {0:?}")]
    InvalidId(String),
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
    #[error("expected package kind {expected}, got {actual} for {id}")]
    WrongKind {
        id: LibraryId,
        expected: LibraryKind,
        actual: LibraryKind,
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
            Self::Config { .. } => "LIBRARY_CONFIG_INVALID",
            Self::Exports { .. } => "LIBRARY_EXPORTS_INVALID",
            Self::PackageLanguage { .. } => "LIBRARY_LANGUAGE_INVALID",
            Self::UnsupportedContent { .. } => "LIBRARY_CONTENT_UNSUPPORTED",
            Self::DuplicatePackage(_) => "LIBRARY_PACKAGE_DUPLICATE",
            Self::DuplicateRuleNamespace { .. } => "LIBRARY_RULE_NAMESPACE_DUPLICATE",
            Self::DuplicateStableId { .. } => "LIBRARY_EXPORT_ID_DUPLICATE",
            Self::DuplicateAlias { .. } => "LIBRARY_EXPORT_ALIAS_DUPLICATE",
            Self::MissingAlias { .. } => "LIBRARY_EXPORT_MISSING",
            Self::UnknownPackage(_) => "LIBRARY_PACKAGE_UNKNOWN",
            Self::DisabledPackage(_) => "LIBRARY_PACKAGE_DISABLED",
            Self::WrongKind { .. } => "LIBRARY_KIND_MISMATCH",
            Self::DependencyCycle(_) => "LIBRARY_DEPENDENCY_CYCLE",
            Self::UnknownAlias(_) => "LIBRARY_EXPORT_UNKNOWN",
            Self::DuplicateTrait(_) => "LIBRARY_TRAIT_DUPLICATE",
            Self::DuplicateSign(_) => "LIBRARY_SIGN_DUPLICATE",
        }
    }
}

#[derive(Debug)]
struct EmbeddedPackage {
    config: &'static str,
    exports: &'static str,
    code: &'static str,
    data: &'static str,
}

const EMBEDDED_PACKAGES: &[EmbeddedPackage] = &[
    EmbeddedPackage {
        config: include_str!("../lib/std/core/config/package.conf"),
        exports: include_str!("../lib/std/core/config/exports.tsv"),
        code: include_str!("../lib/std/core/code/ontology.lang"),
        data: include_str!("../lib/std/core/data/categories.tsv"),
    },
    EmbeddedPackage {
        config: include_str!("../lib/std/grambank/config/package.conf"),
        exports: include_str!("../lib/std/grambank/config/exports.tsv"),
        code: include_str!("../lib/std/grambank/code/syntax.lang"),
        data: include_str!("../lib/std/grambank/data/features.tsv"),
    },
    EmbeddedPackage {
        config: include_str!("../lib/std/cxg/config/package.conf"),
        exports: include_str!("../lib/std/cxg/config/exports.tsv"),
        code: concat!(
            include_str!("../lib/std/cxg/code/schema.lang"),
            "\n",
            include_str!("../lib/std/cxg/code/realizations.lang")
        ),
        data: include_str!("../lib/std/cxg/data/realizations.tsv"),
    },
    EmbeddedPackage {
        config: include_str!("../lib/natural/en-standard/config/package.conf"),
        exports: include_str!("../lib/natural/en-standard/config/exports.tsv"),
        code: include_str!("../lib/natural/en-standard/code/grammar.lang"),
        data: include_str!("../lib/natural/en-standard/data/grambank-v1.0.3.tsv"),
    },
];

#[derive(Default)]
struct Manifest {
    kind: Option<LibraryKind>,
    name: Option<String>,
    version: Option<String>,
    rule_namespace: Option<String>,
    enabled: Option<bool>,
    priority: Option<i32>,
    requires: Option<Vec<LibraryId>>,
    code_paths: Option<Vec<String>>,
    data_paths: Option<Vec<String>>,
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

fn parse_manifest(source: &str) -> Result<Manifest, LibraryLoadError> {
    let package_hint = source
        .lines()
        .find_map(|line| line.trim().strip_prefix("name ="))
        .map(str::trim)
        .unwrap_or("<unknown>");
    let mut manifest = Manifest::default();
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
                        .map(|part| part.trim().parse())
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

fn required<T>(value: Option<T>, package: &str, key: &str) -> Result<T, LibraryLoadError> {
    value.ok_or_else(|| config_error(package, 0, format!("missing required key {key:?}")))
}

fn parse_exports(id: &LibraryId, source: &str) -> Result<Vec<LibraryExport>, LibraryLoadError> {
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
    if exports.is_empty() {
        return Err(LibraryLoadError::Exports {
            package: id.to_string(),
            line: 0,
            message: "at least one export is required".to_owned(),
        });
    }
    Ok(exports)
}

fn validate_package_code(package: &LibraryPackage) -> Result<Language, LibraryLoadError> {
    let mut language =
        Language::parse(package.code).map_err(|error| LibraryLoadError::PackageLanguage {
            package: package.id.to_string(),
            line: error.line,
            message: error.msg,
        })?;
    if package.id.kind == LibraryKind::Std {
        if !language.dsl_decls.is_empty()
            || !language.prosody.is_empty()
            || !language.distribution.is_empty()
            || !language.signs.is_empty()
        {
            return Err(LibraryLoadError::UnsupportedContent {
                package: package.id.to_string(),
                message: "std code may contain trait declarations only".to_owned(),
            });
        }
        for trait_def in &language.traits {
            if trait_def.global {
                return Err(LibraryLoadError::UnsupportedContent {
                    package: package.id.to_string(),
                    message: format!("global trait {:?} is not supported in std", trait_def.name),
                });
            }
            for item in trait_def.blocks.iter().flat_map(|block| &block.items) {
                if !matches!(
                    item,
                    SignItem::Belongs(_)
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
                            "std trait {:?} contains unsupported item {item:?}",
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

fn load_embedded(source: &EmbeddedPackage) -> Result<LibraryPackage, LibraryLoadError> {
    let manifest = parse_manifest(source.config)?;
    let name = required(manifest.name, "<unknown>", "name")?;
    let kind = required(manifest.kind, &name, "kind")?;
    let id = LibraryId::new(kind, name);
    let rule_namespace = required(manifest.rule_namespace, &id.to_string(), "rule_namespace")?;
    if rule_namespace != id.to_string() {
        return Err(config_error(
            &id.to_string(),
            0,
            format!("rule_namespace must equal package id {id}"),
        ));
    }
    let code_paths = required(manifest.code_paths, "<unknown>", "code")?;
    let package = LibraryPackage {
        exports: parse_exports(&id, source.exports)?,
        name: id.name.clone(),
        id,
        version: required(manifest.version, "<unknown>", "version")?,
        rule_namespace,
        enabled: required(manifest.enabled, "<unknown>", "enabled")?,
        priority: required(manifest.priority, "<unknown>", "priority")?,
        requires: manifest.requires.unwrap_or_default(),
        code_path: code_paths.join(","),
        code_paths,
        data_path: {
            let paths = required(manifest.data_paths.clone(), "<unknown>", "data")?;
            paths.join(",")
        },
        data_paths: required(manifest.data_paths, "<unknown>", "data")?,
        code: source.code,
        data: source.data,
    };
    let language = validate_package_code(&package)?;
    for export in &package.exports {
        let present = match export.kind {
            LibraryExportKind::Trait => language.trait_named(&export.alias).is_some(),
            LibraryExportKind::Sign => language.sign_named(&export.alias).is_some(),
            // P50 ③ 未完成前,function export 無處可查——**顯式拒絕**,
            // 不默默當成不存在(那會報成 MissingAlias,訊息會誤導)。
            LibraryExportKind::Function => {
                return Err(LibraryLoadError::UnsupportedContent {
                    package: package.id.to_string(),
                    message: "function exports need code/*.chg loading (P50 ③)".to_owned(),
                })
            }
        };
        if !present {
            return Err(LibraryLoadError::MissingAlias {
                package: package.id.clone(),
                kind: export.kind,
                alias: export.alias.clone(),
            });
        }
        // std 套件可 export trait 與 function(P52:std::grammaticalization 的
        // 路徑庫就是 std 的 function),但不 export sign。
        if package.id.kind == LibraryKind::Std
            && !matches!(
                export.kind,
                LibraryExportKind::Trait | LibraryExportKind::Function
            )
        {
            return Err(LibraryLoadError::UnsupportedContent {
                package: package.id.to_string(),
                message: "std packages may export traits and functions only".to_owned(),
            });
        }
    }
    Ok(package)
}

#[derive(Debug, Clone)]
pub struct LibraryCatalog {
    packages: Vec<LibraryPackage>,
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

impl LibraryCatalog {
    pub fn embedded() -> Result<Self, LibraryLoadError> {
        let mut packages = EMBEDDED_PACKAGES
            .iter()
            .map(load_embedded)
            .collect::<Result<Vec<_>, _>>()?;
        packages.sort_by(|left, right| {
            left.id
                .kind
                .rank()
                .cmp(&right.id.kind.rank())
                .then(left.priority.cmp(&right.priority))
                .then(left.id.name.cmp(&right.id.name))
        });
        validate_catalog(&packages)?;
        Ok(Self { packages })
    }

    pub fn packages(&self) -> &[LibraryPackage] {
        &self.packages
    }

    pub fn resolve_export(
        &self,
        kind: LibraryKind,
        alias: &str,
    ) -> Result<LibraryExport, LibraryLoadError> {
        self.packages
            .iter()
            .filter(|package| package.enabled && package.id.kind == kind)
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

    fn sort_same_layer(&self, ids: &mut [LibraryId]) -> Result<(), LibraryLoadError> {
        for id in ids.iter() {
            self.package(id)?;
        }
        ids.sort_by(|left, right| {
            let left_package = self.packages.iter().find(|package| package.id == *left);
            let right_package = self.packages.iter().find(|package| package.id == *right);
            match (left_package, right_package) {
                (Some(left_package), Some(right_package)) => left_package
                    .priority
                    .cmp(&right_package.priority)
                    .then(left_package.id.name.cmp(&right_package.id.name)),
                _ => left.cmp(right),
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
        self.sort_same_layer(&mut dependencies)?;
        for dependency in &dependencies {
            self.visit(dependency, states, stack, ordered)?;
        }
        stack.pop();
        states.insert(id.clone(), 2);
        ordered.push(id.clone());
        Ok(())
    }

    pub fn select(&self, spec: &LibrarySpec) -> Result<LibrarySelection, LibraryLoadError> {
        let mut roots = self
            .packages
            .iter()
            .filter(|package| package.enabled && package.id.kind == LibraryKind::Std)
            .map(|package| package.id.clone())
            .collect::<Vec<_>>();
        self.sort_same_layer(&mut roots)?;
        if let Some(natural) = &spec.natural {
            if natural.kind != LibraryKind::Natural {
                return Err(LibraryLoadError::WrongKind {
                    id: natural.clone(),
                    expected: LibraryKind::Natural,
                    actual: natural.kind,
                });
            }
            roots.push(natural.clone());
        }
        let mut plugins = spec.plugins.clone();
        plugins.sort();
        plugins.dedup();
        for plugin in &plugins {
            if plugin.kind != LibraryKind::Plugin {
                return Err(LibraryLoadError::WrongKind {
                    id: plugin.clone(),
                    expected: LibraryKind::Plugin,
                    actual: plugin.kind,
                });
            }
        }
        self.sort_same_layer(&mut plugins)?;
        roots.extend(plugins);
        let mut states = BTreeMap::new();
        let mut ordered = Vec::new();
        for root in roots {
            self.visit(&root, &mut states, &mut Vec::new(), &mut ordered)?;
        }

        let mut standard = Language::new();
        let mut overlay = Language::new();
        let mut trait_names = BTreeSet::new();
        let mut sign_names = BTreeSet::new();
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
            match package.id.kind {
                LibraryKind::Std => standard.append_library(language),
                LibraryKind::Natural | LibraryKind::Plugin => overlay.append_library(language),
            }
        }
        Ok(LibrarySelection {
            standard,
            overlay,
            packages: ordered,
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
            enabled: true,
            priority,
            requires: Vec::new(),
            code_paths: vec!["code/test.lang".to_owned()],
            code_path: "code/test.lang".to_owned(),
            data_path: "data/test.tsv".to_owned(),
            data_paths: vec!["data/test.tsv".to_owned()],
            exports: Vec::new(),
            code: "",
            data: "",
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
            .push(LibraryId::new(LibraryKind::Std, "missing"));
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
            .push(LibraryId::new(LibraryKind::Std, "cxg"));
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
                .filter(|id| id.kind == LibraryKind::Plugin)
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
            rule_namespace: id.to_string(),
            enabled: true,
            priority: 0,
            requires: Vec::new(),
            code_paths: vec!["code/schema.lang".to_owned()],
            code_path: "code/schema.lang".to_owned(),
            data_path: "data/none.tsv".to_owned(),
            data_paths: vec!["data/none.tsv".to_owned()],
            exports: Vec::new(),
            code: "trait Schema:\n    syn:\n        slots:\n            head [Noun]\n        map head rename nucleus\n",
            data: "",
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
