//! **前端邊界**:Tauri command 會 1:1 呼叫的那組函數。
//!
//! # 為什麼邏輯在這裡而不是在 Tauri crate
//!
//! 與 CLI 同一個作法(`conlang-cli` 的邏輯全在 lib、`main.rs` 只有五行):
//! **能測的部分不要跟不能測的部分綁在一起**。
//!
//! Tauri crate 需要 `webkit2gtk` 之類的系統庫與 Node 建置鏈;把邏輯放進去,
//! 整組就只能在有那套環境的機器上驗證。放在這裡,它跟其餘 860 條測試一起跑。
//!
//! Tauri 那層因此只剩:
//!
//! ```ignore
//! #[tauri::command]
//! fn tree(state: State<Mutex<UiSession>>) -> Result<EvolutionTreeV1, UiError> {
//!     state.lock().unwrap().tree()
//! }
//! ```
//!
//! # 單一 session(裁定 ③)
//!
//! [`UiSession`] 是整個應用**唯一**的狀態,Tauri 以 `State<Mutex<UiSession>>`
//! 持有。它同時握 store(檔案側)與 workspace(記憶體側)——編輯頁要的
//! State/annotation 住檔案系統,詞典住記憶體,兩者都要。
//!
//! # 錯誤形狀沿用既有慣例
//!
//! `UiError { code, message }` 對映 repo 既有的 `Diagnostic { code, message }`
//! ——不發明第二種錯誤表示。`code` 是可比對的字串(`APP_*`/`PERSISTENCE_*`…),
//! 前端據此分辨,而不是解析人類可讀訊息。

use conlang_changeset::diff::diff_vector;
use conlang_changeset::evolution::{EvolutionGraph, NodeId, RebaseOutcome};
use conlang_changeset::rewrite::{AtomicRewrite, DonorScope, RuleHome, ServiceContext};
use conlang_changeset::state::EvolutionState;
use conlang_changeset::{
    apply_edit, apply_edit_with_packages, change_set_prelude, change_set_prelude_with_packages,
    source_node_snapshots, stable_node_selector, Anchor, NodeSnapshot, PrimitiveEdit,
    UnresolvedChangeSet,
};
use conlang_command::{LanguageCommand, ViewCommand};
use conlang_generate::{DistributionGenerator, Generator, Need, NeedOrigin, Proposal, Strategies};
use conlang_language::{
    compile_document_with_packages, sha256_hex, AddressSegment, LanguageDocument, LibrarySpec,
    NodeId as SourceNodeId, NodeKind, NodeRef, ResolvedPackages,
};
use conlang_persistence::{GraphStore, ProjectDocument, ProjectPackages};
use conlang_query::{
    derivation_family, intelligibility, project_phoneme_freq, ExploratoryHeuristicV1,
    GroupingOverride, LexiconFilter, SortKey, TreeEdgeCut, ViewConfig,
};
use conlang_stats::{load_prior_from_packages, EffectiveDistribution, Layer, WeightTable};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::wire::{
    AuthoringCatalogV1, AuthoringChoiceV1, AuthoringFieldV1, AuthoringMoveOptionV1,
    AuthoringMoveOptionsV1, AuthoringNodeV1, AuthoringSignV1, AuthoringTraitV1, CatalogPackageV1,
    DerivationViewV1, EvolutionTreeV1, GroupingViewV1, IntelligibilityViewV1, LexiconViewV1,
    NodeDetailV1, PackageCatalogV1, PendingChangeV1, ProjectSummaryV1, ProposalV1, ProposalsViewV1,
    RebasePreviewV1, SegmentStatV1, SourceReconcileV1, SourceViewV1, StatsViewV1, WeightConfigV1,
    WeightEntryV1, UI_SCHEMA_V1,
};
use crate::{apply_view_command, AppError, Workspace};

/// 過 IPC 的錯誤。**帶可比對的 `code`**,前端不必解析訊息文字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiError {
    pub code: String,
    pub message: String,
}

impl UiError {
    pub fn of(error: impl std::fmt::Display) -> UiError {
        let text = error.to_string();
        // 既有錯誤一律是 `CODE: 詳細` 的形狀(APP_*/PERSISTENCE_*/CHANGESET_*…)
        let (code, message) = match text.split_once(": ") {
            Some((code, rest))
                if code
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') =>
            {
                (code.to_owned(), rest.to_owned())
            }
            _ => ("APP_ERROR".to_owned(), text.clone()),
        };
        UiError { code, message }
    }
}

fn ui_error(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.to_owned(),
        message: message.into(),
    }
}

impl From<AppError> for UiError {
    fn from(error: AppError) -> UiError {
        UiError::of(error)
    }
}

impl From<conlang_persistence::StoreError> for UiError {
    fn from(error: conlang_persistence::StoreError) -> UiError {
        UiError::of(error)
    }
}

/// 前端送來的詞典查詢條件。
///
/// 刻意**不**直接收 `LexiconFilter`——那是 query 的內部型別,讓它出現在入境
/// 契約上等於把內部形狀綁死。這裡只收前端真的會給的三個東西。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexiconQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss_contains: Option<String>,
    /// `"name"` | `"form"` | `"gloss"`。其餘值視為 `"name"`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentWeight {
    pub segment: String,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default = "default_template")]
    pub template: String,
    #[serde(default = "default_proposal_count")]
    pub count: usize,
    #[serde(default)]
    pub seed: u64,
    pub weights: Vec<SegmentWeight>,
}

fn default_template() -> String {
    "CVC".to_owned()
}

const fn default_proposal_count() -> usize {
    8
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundChangeInput {
    pub rule: String,
    #[serde(default = "default_rule_home")]
    pub home: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

fn default_rule_home() -> String {
    "Core".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredEditInput {
    pub revision: String,
    #[serde(flatten)]
    pub edit: StructuredEdit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StructuredEdit {
    InsertSign {
        name: String,
        #[serde(default)]
        belongs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phon: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gloss: Option<String>,
    },
    InsertTrait {
        name: String,
        #[serde(default)]
        global: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
    },
    CloneSign {
        source: String,
        name: String,
    },
    InsertBody {
        container: String,
        body: BodyItemInput,
    },
    Delete {
        target: String,
    },
    Update {
        target: String,
        field: String,
        value: String,
    },
    Move {
        target: String,
        placement: MovePlacementInput,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BodyItemInput {
    Belongs {
        trait_name: String,
    },
    TraitUse {
        trait_name: String,
    },
    Slot {
        name: String,
        constraint: String,
        #[serde(default)]
        optional: bool,
    },
    Feature {
        dim: String,
        name: String,
        #[serde(default)]
        enum_values: Vec<String>,
        value: String,
    },
    Sense {
        name: String,
        gloss: String,
    },
    Phon {
        form: String,
    },
    Definition {
        dim: String,
        path: String,
        value: String,
    },
    Rule {
        dim: String,
        body: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        stage: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovePlacementInput {
    pub parent: String,
    pub position: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupingQuery {
    #[serde(default = "default_view_name")]
    pub view: String,
    #[serde(default = "default_grouping_threshold")]
    pub threshold: f64,
}

/// Root package declarations written to `project.toml` after validation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PackageSelectionInput {
    /// Package-loader v2 exact or unversioned roots. `Some` records that the
    /// caller chose the v2 shape even when the list is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roots: Option<Vec<String>>,
    /// Project-local qualifier intent. Phase A persists and validates these
    /// targets but does not yet expose qualified symbols in the DSL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases: Option<BTreeMap<String, String>>,
    /// Legacy v1 selection fields retained for older IPC clients.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub std: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<String>,
    /// Set by deserialization when any legacy key was present, including an
    /// explicitly empty array. External Rust callers may also set this when
    /// they intentionally submit an all-empty legacy shape.
    #[doc(hidden)]
    #[serde(skip)]
    pub legacy_shape: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackageSelectionInput {
    #[serde(default, deserialize_with = "present_optional")]
    roots: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "present_optional")]
    aliases: Option<Option<BTreeMap<String, String>>>,
    #[serde(default, deserialize_with = "present_optional")]
    std: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "present_optional")]
    natural: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_optional")]
    plugins: Option<Option<Vec<String>>>,
}

fn present_optional<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

fn non_null<T, E>(field: &'static str, value: Option<Option<T>>) -> Result<Option<T>, E>
where
    E: serde::de::Error,
{
    match value {
        None => Ok(None),
        Some(Some(value)) => Ok(Some(value)),
        Some(None) => Err(E::custom(format!("{field} cannot be null"))),
    }
}

impl<'de> Deserialize<'de> for PackageSelectionInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawPackageSelectionInput::deserialize(deserializer)?;
        let declares_v2 = raw.roots.is_some() || raw.aliases.is_some();
        let declares_legacy = raw.std.is_some() || raw.natural.is_some() || raw.plugins.is_some();
        if declares_v2 && declares_legacy {
            return Err(serde::de::Error::custom(
                "v2 roots/aliases cannot be combined with legacy std/natural/plugins",
            ));
        }
        // `natural = null` is the legacy representation of no natural root;
        // collection/map fields, in contrast, must be arrays/objects.
        let roots = non_null::<_, D::Error>("roots", raw.roots)?;
        let aliases = non_null::<_, D::Error>("aliases", raw.aliases)?;
        let std = non_null::<_, D::Error>("std", raw.std)?.unwrap_or_default();
        let plugins = non_null::<_, D::Error>("plugins", raw.plugins)?.unwrap_or_default();
        Ok(Self {
            roots,
            aliases,
            std,
            natural: raw.natural.flatten(),
            plugins,
            legacy_shape: declares_legacy,
        })
    }
}

fn default_view_name() -> String {
    "default".to_owned()
}

const fn default_grouping_threshold() -> f64 {
    0.6
}

#[derive(Debug)]
struct CachedProposals {
    query: ProposalQuery,
    need: Need,
    proposals: Vec<Proposal>,
}

impl LexiconQuery {
    fn split(&self) -> (LexiconFilter, ViewConfig) {
        let mut filter = LexiconFilter::all();
        if let Some(category) = &self.category {
            filter = filter.with_category(category.clone());
        }
        if let Some(needle) = &self.gloss_contains {
            filter = filter.with_gloss_containing(needle.clone());
        }
        let view = ViewConfig {
            sort: match self.sort.as_deref() {
                Some("form") => SortKey::UnderlyingForm,
                Some("gloss") => SortKey::Gloss,
                _ => SortKey::Name,
            },
        };
        (filter, view)
    }
}

/// 整個應用的**唯一**狀態(裁定 ③:單一 session)。
fn authoring_revision(document: &LanguageDocument) -> Result<String, UiError> {
    let source = document.source();
    let identities = document
        .manifest_json()
        .map_err(|error| ui_error("APP_AUTHORING_REVISION", error.to_string()))?;
    let mut payload = Vec::with_capacity(source.len() + identities.len() + 1);
    payload.extend_from_slice(source.as_bytes());
    payload.push(b'\n');
    payload.extend_from_slice(identities.as_bytes());
    Ok(sha256_hex(&payload))
}

fn ensure_authoring_revision(document: &LanguageDocument, expected: &str) -> Result<(), UiError> {
    if authoring_revision(document)? != expected {
        return Err(ui_error(
            "APP_AUTHORING_STALE",
            "the structured editor is stale; refresh its targets and try again",
        ));
    }
    Ok(())
}

fn source_kind(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Language => "language",
        NodeKind::DslDeclaration => "dsl_declaration",
        NodeKind::Distribution => "distribution",
        NodeKind::Trait => "trait",
        NodeKind::Sign => "sign",
        NodeKind::Block => "block",
        NodeKind::TraitUse => "trait_use",
        NodeKind::Belongs => "belongs",
        NodeKind::Slot => "slot",
        NodeKind::SlotMap => "slot_map",
        NodeKind::FeatureDeclaration => "feature_declaration",
        NodeKind::FeatureValue => "feature_value",
        NodeKind::SlotFeatureBinding => "slot_feature_binding",
        NodeKind::RoleDeclaration => "role",
        NodeKind::RoleBinding => "role_binding",
        NodeKind::Sense => "sense",
        NodeKind::SenseEdge => "sense_edge",
        NodeKind::Realization => "realization",
        NodeKind::FeatureRule => "feature_rule",
        NodeKind::Definition => "definition",
        NodeKind::Rule => "rule",
        NodeKind::RuleElseBranch => "else",
        NodeKind::RuleThenBranch => "then",
        NodeKind::PhonStatement => "phon_statement",
        NodeKind::PhonBlockNode => "phon_block",
        NodeKind::RealizationBranch => "realization_branch",
        NodeKind::Application => "application",
        NodeKind::Case => "case",
        NodeKind::CaseBranch => "case_branch",
        NodeKind::Constraint => "constraint",
    }
}

fn path_component(snapshot: &NodeSnapshot) -> String {
    match snapshot.kind {
        NodeKind::Language => "Language".to_owned(),
        NodeKind::Sign => snapshot
            .value
            .replace("Sign(name=", "sign ")
            .replace(')', ""),
        NodeKind::Trait => snapshot
            .value
            .split(",global=")
            .next()
            .unwrap_or(&snapshot.value)
            .replace("Trait(name=", "trait ")
            .replace(')', ""),
        _ => format!("{} · {}", source_kind(snapshot.kind), snapshot.value),
    }
}

fn snapshot_path(
    id: &SourceNodeId,
    snapshots: &BTreeMap<SourceNodeId, NodeSnapshot>,
    cache: &mut BTreeMap<SourceNodeId, String>,
) -> String {
    if let Some(path) = cache.get(id) {
        return path.clone();
    }
    let Some(snapshot) = snapshots.get(id) else {
        return id.to_string();
    };
    let component = path_component(snapshot);
    let path = match snapshot.parent.as_ref() {
        None => component,
        Some(parent) => {
            let parent_path = snapshot_path(parent, snapshots, cache);
            if parent_path == "Language" {
                component
            } else {
                format!("{parent_path} › {component}")
            }
        }
    };
    cache.insert(id.clone(), path.clone());
    path
}

fn choice(value: impl Into<String>, label: impl Into<String>) -> AuthoringChoiceV1 {
    AuthoringChoiceV1 {
        value: value.into(),
        label: label.into(),
    }
}

fn field(
    name: &str,
    label: &str,
    control: &str,
    choices: Vec<AuthoringChoiceV1>,
) -> AuthoringFieldV1 {
    AuthoringFieldV1 {
        name: name.to_owned(),
        label: label.to_owned(),
        control: control.to_owned(),
        choices,
    }
}

fn update_fields(kind: NodeKind, categories: &[AuthoringChoiceV1]) -> Vec<AuthoringFieldV1> {
    let boolean = || vec![choice("true", "True"), choice("false", "False")];
    let dimensions = || {
        vec![
            choice("phon", "Phon"),
            choice("syn", "Syn"),
            choice("sem", "Sem"),
            choice("prag", "Prag"),
        ]
    };
    let stages = || {
        vec![
            choice("stem", "Stem"),
            choice("word", "Word"),
            choice("phrase", "Phrase"),
        ]
    };
    match kind {
        NodeKind::Sign => vec![field("name", "Name", "text", Vec::new())],
        NodeKind::Trait => vec![
            field("name", "Name", "text", Vec::new()),
            field("global", "Global", "boolean", boolean()),
        ],
        NodeKind::Definition => vec![
            field("path", "Path", "text", Vec::new()),
            field("value", "Value", "text", Vec::new()),
        ],
        NodeKind::FeatureValue => vec![field("value", "Value", "text", Vec::new())],
        NodeKind::Rule | NodeKind::FeatureRule => vec![
            field("body", "Body", "textarea", Vec::new()),
            field("stage", "Stage", "choice", stages()),
            field("dim", "Dimension", "choice", dimensions()),
            field("propagate", "Propagate", "boolean", boolean()),
        ],
        NodeKind::RuleElseBranch | NodeKind::RuleThenBranch | NodeKind::PhonStatement => {
            vec![field("body", "Body", "textarea", Vec::new())]
        }
        NodeKind::PhonBlockNode => {
            vec![field("propagate", "Propagate", "boolean", boolean())]
        }
        NodeKind::Slot => vec![
            field("name", "Name", "text", Vec::new()),
            field("optional", "Optional", "boolean", boolean()),
        ],
        NodeKind::Belongs => vec![field("target", "Trait", "choice", categories.to_vec())],
        NodeKind::Case => vec![field(
            "selection",
            "Selection",
            "choice",
            vec![choice("case", "First match"), choice("when", "Accumulate")],
        )],
        NodeKind::Sense => vec![field("gloss", "Gloss", "text", Vec::new())],
        NodeKind::SenseEdge => vec![
            field(
                "kind",
                "Kind",
                "choice",
                vec![
                    choice("metaphor", "Metaphor"),
                    choice("metonymy", "Metonymy"),
                    choice("narrow", "Narrow"),
                    choice("broaden", "Broaden"),
                ],
            ),
            field(
                "transparency",
                "Transparency",
                "choice",
                vec![
                    choice("transparent", "Transparent"),
                    choice("opaque", "Opaque"),
                ],
            ),
        ],
        _ => Vec::new(),
    }
}

fn structurally_deletable(snapshot: &NodeSnapshot) -> bool {
    match snapshot.address.0.last() {
        None => false,
        Some(
            AddressSegment::CaseExpression
            | AddressSegment::CaseResult
            | AddressSegment::ApplicationArguments(_),
        ) => false,
        Some(_) => true,
    }
}

fn structurally_movable(snapshot: &NodeSnapshot) -> bool {
    matches!(
        snapshot.address.0.last(),
        Some(
            AddressSegment::DslDeclarations(_)
                | AddressSegment::Distribution(_)
                | AddressSegment::Traits(_)
                | AddressSegment::Signs(_)
                | AddressSegment::Blocks(_)
                | AddressSegment::Items(_)
                | AddressSegment::RuleElse(_)
                | AddressSegment::RuleThen(_)
                | AddressSegment::PhonLeaf(_)
                | AddressSegment::PhonThen(_)
                | AddressSegment::PhonElse(_)
                | AddressSegment::RealizationBranches(_)
                | AddressSegment::CaseBranches(_)
        )
    )
}

fn is_body_item(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::TraitUse
            | NodeKind::Belongs
            | NodeKind::Slot
            | NodeKind::SlotMap
            | NodeKind::FeatureDeclaration
            | NodeKind::FeatureValue
            | NodeKind::SlotFeatureBinding
            | NodeKind::RoleDeclaration
            | NodeKind::RoleBinding
            | NodeKind::Sense
            | NodeKind::SenseEdge
            | NodeKind::Realization
            | NodeKind::FeatureRule
            | NodeKind::Definition
            | NodeKind::Rule
            | NodeKind::Application
            | NodeKind::Case
            | NodeKind::Constraint
    )
}

fn parent_accepts_move(child: NodeKind, parent: NodeKind) -> bool {
    match child {
        NodeKind::DslDeclaration | NodeKind::Distribution | NodeKind::Trait | NodeKind::Sign => {
            parent == NodeKind::Language
        }
        NodeKind::Block => parent == NodeKind::Trait,
        kind if is_body_item(kind) => {
            matches!(
                parent,
                NodeKind::Sign | NodeKind::Block | NodeKind::CaseBranch
            )
        }
        NodeKind::RuleElseBranch => matches!(parent, NodeKind::Rule | NodeKind::FeatureRule),
        NodeKind::RuleThenBranch => matches!(parent, NodeKind::Rule | NodeKind::FeatureRule),
        NodeKind::PhonStatement | NodeKind::PhonBlockNode => matches!(
            parent,
            NodeKind::Rule | NodeKind::FeatureRule | NodeKind::PhonBlockNode
        ),
        NodeKind::CaseBranch => parent == NodeKind::Case,
        NodeKind::RealizationBranch => parent == NodeKind::Realization,
        _ => false,
    }
}

fn build_authoring_catalog(
    document: &LanguageDocument,
    libraries: &LibrarySpec,
    packages: Option<&ResolvedPackages>,
) -> Result<AuthoringCatalogV1, UiError> {
    let revision = authoring_revision(document)?;
    let legacy_packages = if packages.is_none() {
        Some(
            conlang_language::library::embedded_catalog()
                .and_then(|catalog| catalog.resolve_legacy(libraries))
                .map_err(UiError::of)?,
        )
    } else {
        None
    };
    let packages = packages
        .or(legacy_packages.as_ref())
        .expect("legacy packages are resolved above");
    compile_document_with_packages(document, packages).map_err(UiError::of)?;

    let mut traits_by_name = BTreeMap::new();
    for item in packages
        .selection
        .standard
        .traits
        .iter()
        .chain(packages.selection.overlay.traits.iter())
    {
        traits_by_name.insert(
            item.name.clone(),
            AuthoringTraitV1 {
                name: item.name.clone(),
                global: item.global,
                blocks: item.blocks.len(),
                source: "library".to_owned(),
                selector: None,
            },
        );
    }
    for item in &document.language().traits {
        traits_by_name.insert(
            item.name.clone(),
            AuthoringTraitV1 {
                name: item.name.clone(),
                global: item.global,
                blocks: item.blocks.len(),
                source: "local".to_owned(),
                selector: document
                    .ref_for_trait(&item.name)
                    .map(|reference| stable_node_selector(&reference)),
            },
        );
    }
    let traits = traits_by_name.into_values().collect::<Vec<_>>();
    let categories = traits
        .iter()
        .filter(|item| !item.global)
        .map(|item| choice(item.name.clone(), item.name.clone()))
        .collect::<Vec<_>>();

    let snapshot_values = source_node_snapshots(document);
    let snapshots = snapshot_values
        .iter()
        .cloned()
        .map(|snapshot| (snapshot.id.clone(), snapshot))
        .collect::<BTreeMap<_, _>>();
    let selectors = snapshot_values
        .iter()
        .map(|snapshot| {
            (
                snapshot.id.clone(),
                stable_node_selector(&NodeRef::new(snapshot.id.clone(), snapshot.kind)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut path_cache = BTreeMap::new();
    let paths = snapshot_values
        .iter()
        .map(|snapshot| {
            (
                snapshot.id.clone(),
                snapshot_path(&snapshot.id, &snapshots, &mut path_cache),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let nodes = snapshot_values
        .iter()
        .map(|snapshot| AuthoringNodeV1 {
            selector: selectors[&snapshot.id].clone(),
            parent: snapshot
                .parent
                .as_ref()
                .and_then(|parent| selectors.get(parent).cloned()),
            kind: source_kind(snapshot.kind).to_owned(),
            path: paths[&snapshot.id].clone(),
            summary: snapshot.value.clone(),
            deletable: structurally_deletable(snapshot),
            movable: structurally_movable(snapshot),
            fields: update_fields(snapshot.kind, &categories),
        })
        .collect::<Vec<_>>();

    let signs = document
        .language()
        .signs
        .iter()
        .filter_map(|sign| {
            document
                .ref_for_sign(&sign.name)
                .map(|reference| AuthoringSignV1 {
                    name: sign.name.clone(),
                    selector: stable_node_selector(&reference),
                })
        })
        .collect::<Vec<_>>();
    let rule_homes = document
        .language()
        .traits
        .iter()
        .filter(|item| item.global)
        .map(|item| choice(item.name.clone(), item.name.clone()))
        .collect::<Vec<_>>();
    let body_containers = snapshot_values
        .iter()
        .filter(|snapshot| matches!(snapshot.kind, NodeKind::Sign | NodeKind::Block))
        .map(|snapshot| choice(selectors[&snapshot.id].clone(), paths[&snapshot.id].clone()))
        .collect::<Vec<_>>();

    Ok(AuthoringCatalogV1 {
        schema: UI_SCHEMA_V1.to_owned(),
        revision,
        nodes,
        signs,
        traits,
        rule_homes,
        body_containers,
    })
}

fn authored_identifier(value: &str, label: &str) -> Result<String, UiError> {
    let value = value.trim();
    if value.is_empty()
        || value.contains(char::is_whitespace)
        || !value
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(ui_error(
            "APP_AUTHORING_INPUT",
            format!("{label} must be a single identifier"),
        ));
    }
    Ok(value.to_owned())
}

fn authored_line(value: &str, label: &str, allow_empty: bool) -> Result<String, UiError> {
    let value = value.trim();
    if value.contains('\r') || value.contains('\n') || (!allow_empty && value.is_empty()) {
        return Err(ui_error(
            "APP_AUTHORING_INPUT",
            format!("{label} must be a single non-empty line"),
        ));
    }
    Ok(value.to_owned())
}

fn authored_dim(value: &str) -> Result<String, UiError> {
    let value = value.trim();
    if matches!(value, "phon" | "syn" | "sem" | "prag") {
        Ok(value.to_owned())
    } else {
        Err(ui_error(
            "APP_AUTHORING_INPUT",
            "dimension must be phon, syn, sem, or prag",
        ))
    }
}

fn authored_stage(value: &str) -> Result<String, UiError> {
    let value = value.trim();
    if matches!(value, "stem" | "word" | "phrase") {
        Ok(value.to_owned())
    } else {
        Err(ui_error(
            "APP_AUTHORING_INPUT",
            "stage must be stem, word, or phrase",
        ))
    }
}

fn quoted_change_value(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn indent_lines(value: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn resolve_authoring_statement(
    document: &LanguageDocument,
    libraries: &LibrarySpec,
    packages: Option<&ResolvedPackages>,
    _namespace: &str,
    operation: &str,
) -> Result<Vec<PrimitiveEdit>, UiError> {
    // Resolution itself forks its base document. Reusing the real pending
    // namespace against a pending preview would therefore collide with the
    // allocator that the earlier statements already installed. This
    // deterministic scratch namespace exists only while lowering the one UI
    // operation; the returned PrimitiveEdits are staged and replayed under the
    // real pending namespace below.
    let namespace = format!("ui:authoring:{}", authoring_revision(document)?);
    let mut source = match packages {
        Some(packages) => change_set_prelude_with_packages(document, packages, &namespace),
        None => change_set_prelude(document, libraries, &namespace),
    }
    .map_err(UiError::of)?;
    source.push_str("\n    #0:\n");
    source.push_str(&indent_lines(operation, 8));
    source.push('\n');
    let change_set = UnresolvedChangeSet::parse(&source).map_err(UiError::of)?;
    let resolved = match packages {
        Some(packages) => change_set.resolve_packages(document, packages),
        None => change_set.resolve(document, libraries),
    }
    .map_err(UiError::of)?;
    let statement = resolved.statements.into_iter().next().ok_or_else(|| {
        ui_error(
            "APP_AUTHORING_EMPTY",
            "the structured operation produced no statement",
        )
    })?;
    if statement.edits.is_empty() {
        return Err(ui_error(
            "APP_AUTHORING_EMPTY",
            "the structured operation produced no primitive edits",
        ));
    }
    Ok(statement.edits)
}

fn catalog_node<'a>(
    catalog: &'a AuthoringCatalogV1,
    selector: &str,
) -> Result<&'a AuthoringNodeV1, UiError> {
    catalog
        .nodes
        .iter()
        .find(|node| node.selector == selector)
        .ok_or_else(|| {
            ui_error(
                "APP_AUTHORING_TARGET",
                format!("unknown or non-local authoring target {selector:?}"),
            )
        })
}

fn catalog_trait<'a>(
    catalog: &'a AuthoringCatalogV1,
    name: &str,
    allow_global: bool,
) -> Result<&'a AuthoringTraitV1, UiError> {
    catalog
        .traits
        .iter()
        .find(|item| item.name == name && (allow_global || !item.global))
        .ok_or_else(|| {
            ui_error(
                "APP_AUTHORING_TRAIT",
                format!("unknown or invalid trait {name:?}"),
            )
        })
}

fn insert_operation(container: &str, fragment: &str) -> String {
    format!(
        "insert into {container} at end:\n{}",
        indent_lines(fragment, 4)
    )
}

fn structured_operation(
    document: &LanguageDocument,
    catalog: &AuthoringCatalogV1,
    edit: &StructuredEdit,
) -> Result<String, UiError> {
    let root = stable_node_selector(&document.root_ref());
    match edit {
        StructuredEdit::InsertSign {
            name,
            belongs,
            phon,
            gloss,
        } => {
            let name = authored_identifier(name, "sign name")?;
            let mut fragment = format!("sign {name}:");
            let mut seen = BTreeSet::new();
            for belongs in belongs {
                let belongs = authored_identifier(belongs, "belongs target")?;
                catalog_trait(catalog, &belongs, false)?;
                if seen.insert(belongs.clone()) {
                    fragment.push_str(&format!("\n    belongs {belongs}"));
                }
            }
            if let Some(phon) = phon.as_deref().filter(|value| !value.trim().is_empty()) {
                let phon = authored_line(phon, "underlying phon", false)?;
                if phon.contains('/') {
                    return Err(ui_error(
                        "APP_AUTHORING_INPUT",
                        "underlying phon must not contain `/` delimiters",
                    ));
                }
                fragment.push_str(&format!("\n    phon:\n        /{phon}/"));
            }
            if let Some(gloss) = gloss.as_deref().filter(|value| !value.trim().is_empty()) {
                let gloss = authored_line(gloss, "core gloss", false)?;
                fragment.push_str(&format!(
                    "\n    sem:\n        senses:\n            core = {gloss}"
                ));
            }
            Ok(insert_operation(&root, &fragment))
        }
        StructuredEdit::InsertTrait {
            name,
            global,
            parent,
        } => {
            let name = authored_identifier(name, "trait name")?;
            let mut fragment = format!("{}trait {name}:", if *global { "global " } else { "" });
            if let Some(parent) = parent.as_deref().filter(|value| !value.trim().is_empty()) {
                let parent = authored_identifier(parent, "parent trait")?;
                catalog_trait(catalog, &parent, false)?;
                fragment.push_str(&format!("\n    belongs {parent}"));
            }
            Ok(insert_operation(&root, &fragment))
        }
        StructuredEdit::CloneSign { source, name } => {
            if !catalog
                .signs
                .iter()
                .any(|sign| sign.selector == source.as_str())
            {
                return Err(ui_error(
                    "APP_AUTHORING_TARGET",
                    "clone source must be a local sign",
                ));
            }
            let name = authored_identifier(name, "cloned sign name")?;
            Ok(format!("clone {source} as {name}"))
        }
        StructuredEdit::InsertBody { container, body } => {
            if !catalog
                .body_containers
                .iter()
                .any(|choice| choice.value == container.as_str())
            {
                return Err(ui_error(
                    "APP_AUTHORING_TARGET",
                    "body items may be inserted only into a local sign or trait block",
                ));
            }
            let fragment = structured_body_fragment(catalog, body)?;
            Ok(insert_operation(container, &fragment))
        }
        StructuredEdit::Delete { target } => {
            let node = catalog_node(catalog, target)?;
            if !node.deletable {
                return Err(ui_error(
                    "APP_AUTHORING_TARGET",
                    "this node cannot be deleted",
                ));
            }
            Ok(format!("delete {target}"))
        }
        StructuredEdit::Update {
            target,
            field,
            value,
        } => {
            let node = catalog_node(catalog, target)?;
            if !node.fields.iter().any(|candidate| candidate.name == *field) {
                return Err(ui_error(
                    "APP_AUTHORING_FIELD",
                    format!("field {field:?} is not editable on this node"),
                ));
            }
            let value = authored_line(value, "updated value", true)?;
            Ok(format!(
                "update {target}.{field} = {}",
                quoted_change_value(&value)
            ))
        }
        StructuredEdit::Move { target, placement } => {
            let node = catalog_node(catalog, target)?;
            if !node.movable {
                return Err(ui_error(
                    "APP_AUTHORING_TARGET",
                    "this node cannot be moved",
                ));
            }
            catalog_node(catalog, &placement.parent)?;
            let anchor = match placement.position.as_str() {
                "start" | "end" if placement.sibling.is_none() => placement.position.clone(),
                "before" | "after" => {
                    let sibling = placement.sibling.as_deref().ok_or_else(|| {
                        ui_error("APP_AUTHORING_INPUT", "move anchor requires a sibling")
                    })?;
                    catalog_node(catalog, sibling)?;
                    format!("{} {sibling}", placement.position)
                }
                _ => return Err(ui_error("APP_AUTHORING_INPUT", "invalid move placement")),
            };
            Ok(format!("move {target} to {} at {anchor}", placement.parent))
        }
    }
}

fn structured_body_fragment(
    catalog: &AuthoringCatalogV1,
    body: &BodyItemInput,
) -> Result<String, UiError> {
    match body {
        BodyItemInput::Belongs { trait_name } => {
            let name = authored_identifier(trait_name, "belongs target")?;
            catalog_trait(catalog, &name, false)?;
            Ok(format!("belongs {name}"))
        }
        BodyItemInput::TraitUse { trait_name } => {
            let name = authored_identifier(trait_name, "trait use")?;
            let item = catalog_trait(catalog, &name, false)?;
            if item.blocks > 1 {
                Ok((0..item.blocks)
                    .map(|block| format!("{name}[{block}]"))
                    .collect::<Vec<_>>()
                    .join("\n"))
            } else {
                Ok(name)
            }
        }
        BodyItemInput::Slot {
            name,
            constraint,
            optional,
        } => {
            let name = authored_identifier(name, "slot name")?;
            let constraint = constraint.trim();
            let constraint = if constraint == "*" {
                "*".to_owned()
            } else {
                let constraint = authored_identifier(constraint, "slot constraint")?;
                catalog_trait(catalog, &constraint, false)?;
                constraint
            };
            Ok(format!(
                "syn:\n    slots:\n        {name} [{constraint}]{}",
                if *optional { "?" } else { "" }
            ))
        }
        BodyItemInput::Feature {
            dim,
            name,
            enum_values,
            value,
        } => {
            let dim = authored_dim(dim)?;
            if dim == "phon" {
                return Err(ui_error(
                    "APP_AUTHORING_INPUT",
                    "feature declarations are supported only in syn, sem, or prag",
                ));
            }
            let name = authored_identifier(name, "feature name")?;
            let values = enum_values
                .iter()
                .map(|item| authored_identifier(item, "feature enum value"))
                .collect::<Result<Vec<_>, _>>()?;
            let value = value.trim();
            if values.is_empty() && value.is_empty() {
                return Err(ui_error(
                    "APP_AUTHORING_INPUT",
                    "feature needs an enum declaration, an assignment, or both",
                ));
            }
            let mut entries = Vec::new();
            if !values.is_empty() {
                entries.push(format!("{name} = enum({})", values.join(", ")));
            }
            if !value.is_empty() {
                let value = authored_identifier(value, "feature value")?;
                if !values.is_empty() && !values.contains(&value) {
                    return Err(ui_error(
                        "APP_AUTHORING_INPUT",
                        format!("feature value {value:?} is not in the declared enum"),
                    ));
                }
                entries.push(format!("{name} = {value}"));
            }
            Ok(format!(
                "{dim}:\n    feature:\n{}",
                indent_lines(&entries.join("\n"), 8)
            ))
        }
        BodyItemInput::Sense { name, gloss } => {
            let name = authored_identifier(name, "sense name")?;
            let gloss = authored_line(gloss, "sense gloss", false)?;
            Ok(format!("sem:\n    senses:\n        {name} = {gloss}"))
        }
        BodyItemInput::Phon { form } => {
            let form = authored_line(form, "underlying phon", false)?;
            if form.contains('/') {
                return Err(ui_error(
                    "APP_AUTHORING_INPUT",
                    "underlying phon must not contain `/` delimiters",
                ));
            }
            Ok(format!("phon:\n    /{form}/"))
        }
        BodyItemInput::Definition { dim, path, value } => {
            let dim = authored_dim(dim)?;
            if dim == "phon" {
                return Err(ui_error(
                    "APP_AUTHORING_INPUT",
                    "use the dedicated underlying-phon form for phon definitions",
                ));
            }
            let path = authored_line(path, "definition path", false)?;
            let value = authored_line(value, "definition value", false)?;
            Ok(format!("{dim}:\n    {path} = {value}"))
        }
        BodyItemInput::Rule {
            dim,
            body,
            name,
            stage,
        } => {
            let dim = authored_dim(dim)?;
            let body = authored_line(body, "rule body", false)?;
            let stage = authored_stage(stage)?;
            let name = name
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| authored_identifier(value, "rule name"))
                .transpose()?;
            Ok(format!(
                "{dim}:\n    {body}{} @stage {stage}",
                name.map(|name| format!(" @name {name}"))
                    .unwrap_or_default()
            ))
        }
    }
}

fn reference_for_selector(document: &LanguageDocument, selector: &str) -> Option<NodeRef> {
    source_node_snapshots(document)
        .into_iter()
        .find_map(|snapshot| {
            let reference = NodeRef::new(snapshot.id, snapshot.kind);
            (stable_node_selector(&reference) == selector).then_some(reference)
        })
}

fn legal_move_options(
    document: &LanguageDocument,
    libraries: &LibrarySpec,
    packages: Option<&ResolvedPackages>,
    catalog: &AuthoringCatalogV1,
    target: &str,
) -> Result<Vec<AuthoringMoveOptionV1>, UiError> {
    let target_node = catalog_node(catalog, target)?;
    if !target_node.movable {
        return Err(ui_error(
            "APP_AUTHORING_TARGET",
            "this node cannot be moved",
        ));
    }
    let target_ref = reference_for_selector(document, target).ok_or_else(|| {
        ui_error(
            "APP_AUTHORING_TARGET",
            "the move target no longer exists in the preview",
        )
    })?;
    let snapshots = source_node_snapshots(document);
    let labels = catalog
        .nodes
        .iter()
        .map(|node| (node.selector.as_str(), node.path.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut outcomes = BTreeSet::new();
    let mut options = Vec::new();

    for parent in &snapshots {
        if !parent_accepts_move(target_ref.expected, parent.kind) {
            continue;
        }
        let parent_ref = NodeRef::new(parent.id.clone(), parent.kind);
        let parent_selector = stable_node_selector(&parent_ref);
        let mut anchors = vec![("start", None, Anchor::Start), ("end", None, Anchor::End)];
        for sibling in snapshots.iter().filter(|candidate| {
            candidate.parent.as_ref() == Some(&parent.id) && candidate.id != target_ref.id
        }) {
            let sibling_ref = NodeRef::new(sibling.id.clone(), sibling.kind);
            let sibling_selector = stable_node_selector(&sibling_ref);
            anchors.push((
                "before",
                Some(sibling_selector.clone()),
                Anchor::Before(sibling_ref.clone()),
            ));
            anchors.push(("after", Some(sibling_selector), Anchor::After(sibling_ref)));
        }
        for (position, sibling, anchor) in anchors {
            let edit = PrimitiveEdit::Move {
                node: target_ref.clone(),
                new_parent: parent_ref.clone(),
                anchor,
            };
            let outcome = match packages {
                Some(packages) => apply_edit_with_packages(document, edit, packages),
                None => apply_edit(document, edit, libraries),
            };
            let Ok(outcome) = outcome else {
                continue;
            };
            if outcome.record.diff.entries.is_empty() {
                continue;
            }
            let fingerprint = authoring_revision(&outcome.document)?;
            if !outcomes.insert(fingerprint) {
                continue;
            }
            let parent_label = labels
                .get(parent_selector.as_str())
                .copied()
                .unwrap_or(parent_selector.as_str());
            let label = match sibling.as_deref() {
                Some(sibling) => format!(
                    "{} {} / {parent_label}",
                    position,
                    labels.get(sibling).copied().unwrap_or(sibling)
                ),
                None => format!("{position} / {parent_label}"),
            };
            options.push(AuthoringMoveOptionV1 {
                parent: parent_selector.clone(),
                parent_label: parent_label.to_owned(),
                position: position.to_owned(),
                sibling,
                label,
            });
        }
    }
    options.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(options)
}

#[derive(Debug)]
pub struct UiSession {
    store: GraphStore,
    workspace: Workspace,
    graph_dirty: bool,
    last_proposals: Option<CachedProposals>,
}

impl UiSession {
    /// 開啟專案。`fallback` 供沒有 `project.toml` 的舊 store。
    pub fn open(
        path: impl AsRef<Path>,
        fallback: conlang_language::LibrarySpec,
    ) -> Result<UiSession, UiError> {
        let store = GraphStore::open(path.as_ref())?;
        let workspace = Workspace::open(&store, fallback)?;
        Ok(UiSession {
            store,
            workspace,
            graph_dirty: false,
            last_proposals: None,
        })
    }

    /// 從 `.lang` 建立新專案。先完整解析 source，成功後才初始化目錄。
    /// 建一個新專案。
    ///
    /// `source_path` 為 `None` ⇒ **空白專案**。空的 Language 是一等公民
    /// (P28:canonical empty root 永遠存在,四原語有處掛靠),故不必先手寫一份
    /// 佔位 `.lang`——那是把實作細節外洩給使用者。
    pub fn create(
        path: impl AsRef<Path>,
        source_path: Option<impl AsRef<Path>>,
        name: Option<String>,
        namespace: &str,
    ) -> Result<UiSession, UiError> {
        let source = match &source_path {
            Some(from) => std::fs::read_to_string(from.as_ref()).map_err(UiError::of)?,
            None => String::new(),
        };
        let document =
            LanguageDocument::import_new_root(&source, namespace).map_err(UiError::of)?;
        let project_path = path.as_ref();
        if project_path.exists() {
            let mut entries = std::fs::read_dir(project_path).map_err(UiError::of)?;
            if entries.next().transpose().map_err(UiError::of)?.is_some() {
                return Err(ui_error(
                    "APP_PROJECT_NOT_EMPTY",
                    project_path.display().to_string(),
                ));
            }
        }

        let store = GraphStore::init(project_path)?;
        let libraries = LibrarySpec::default();
        let mut graph = EvolutionGraph::new(libraries.clone());
        graph.add_root(document).map_err(AppError::Evolution)?;
        store.save(&graph)?;
        let mut project = ProjectDocument::from_spec(&libraries);
        project.name = name;
        store.write_project(&project)?;
        let workspace = Workspace::open(&store, libraries)?;
        if let Some(packages) = workspace.session().packages() {
            store.write_resolved_packages_lock(packages)?;
        }
        Ok(UiSession {
            store,
            workspace,
            graph_dirty: false,
            last_proposals: None,
        })
    }

    pub fn summary(&self) -> ProjectSummaryV1 {
        let session = self.workspace.session();
        let packages = match session.packages() {
            Some(resolved) => resolved
                .intent
                .roots
                .iter()
                .map(|requirement| requirement.id.to_string())
                .collect(),
            None => {
                let spec = session.libraries();
                let mut packages: Vec<String> = spec.std.iter().map(ToString::to_string).collect();
                if let Some(natural) = &spec.natural {
                    packages.push(natural.to_string());
                }
                packages.extend(spec.plugins.iter().map(ToString::to_string));
                packages
            }
        };
        ProjectSummaryV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            path: self.store.root().display().to_string(),
            name: self
                .workspace
                .project()
                .and_then(|project| project.name.clone()),
            legacy: self.workspace.project().is_none(),
            graph_dirty: self.graph_dirty,
            has_pending: self.workspace.session().pending().is_some(),
            node_count: self.workspace.session().graph().len(),
            active: self
                .workspace
                .session()
                .active()
                .map(|id| id.as_str().to_owned()),
            packages,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.graph_dirty || self.workspace.session().pending().is_some()
    }

    /// List the current offline catalog and package selection.
    pub fn package_catalog(&self) -> Result<PackageCatalogV1, UiError> {
        let catalog = self.store.offline_package_catalog(std::iter::empty())?;
        let session = self.workspace.session();
        let (selected, declared) = match session.packages() {
            Some(resolved) => (
                resolved
                    .selection
                    .packages
                    .iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>(),
                resolved
                    .intent
                    .roots
                    .iter()
                    .map(|requirement| requirement.id.clone())
                    .collect::<std::collections::BTreeSet<_>>(),
            ),
            None => {
                let spec = session.libraries();
                let selected = catalog
                    .select(spec)
                    .map_err(|error| UiError::of(AppError::Library(error)))?
                    .packages
                    .into_iter()
                    .collect();
                let declared = spec
                    .std
                    .iter()
                    .chain(spec.natural.iter())
                    .chain(spec.plugins.iter())
                    .cloned()
                    .collect();
                (selected, declared)
            }
        };
        let packages = catalog
            .packages()
            .iter()
            .map(|package| CatalogPackageV1 {
                id: package.id.to_string(),
                kind: package.id.namespace.clone(),
                version: package.version.clone(),
                source: package.source.keyword().to_owned(),
                enabled: package.enabled,
                declared: declared.contains(&package.id),
                selected: selected.contains(&package.id),
                requires: package.requires.iter().map(ToString::to_string).collect(),
            })
            .collect();
        Ok(PackageCatalogV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            packages,
        })
    }

    /// Validate a new declaration against both the catalog and the complete
    /// graph before updating the individually atomic project and lock files,
    /// then reopen the session. A crash between those two writes is fail-closed:
    /// the next open reports a package-lock mismatch instead of replaying with
    /// an unpinned environment.
    pub fn configure_packages(
        &mut self,
        input: PackageSelectionInput,
    ) -> Result<ProjectSummaryV1, UiError> {
        if self.is_dirty() {
            return Err(ui_error(
                "APP_DIRTY_PROJECT",
                "save or discard project changes before changing package libraries",
            ));
        }

        let mut project = self.workspace.project().cloned().unwrap_or_default();
        let current_v2 = !project.packages.roots.is_empty() || !project.packages.aliases.is_empty();
        let declares_v2 = input.roots.is_some() || input.aliases.is_some();
        let declares_legacy = input.legacy_shape
            || !input.std.is_empty()
            || input.natural.is_some()
            || !input.plugins.is_empty();
        if declares_v2 && declares_legacy {
            return Err(ui_error(
                "APP_PACKAGE_SELECTION_MIXED",
                "v2 roots/aliases cannot be combined with legacy std/natural/plugins",
            ));
        }
        if current_v2 && !declares_v2 {
            return Err(ui_error(
                "APP_PACKAGE_SELECTION_MIGRATION_REQUIRED",
                "a legacy package payload cannot overwrite an existing v2 project",
            ));
        }
        project.packages = if declares_v2 {
            ProjectPackages {
                roots: input.roots.unwrap_or(project.packages.roots),
                aliases: input.aliases.unwrap_or(project.packages.aliases),
                ..ProjectPackages::default()
            }
        } else {
            ProjectPackages {
                std: input.std,
                natural: input.natural,
                plugins: input.plugins,
                ..ProjectPackages::default()
            }
        };
        let intent = project.to_package_spec()?;
        let packages = self.store.resolve_packages(&intent, std::iter::empty())?;
        // This is a read-only validation pass. It catches package-lock and
        // replay incompatibilities before the declaration on disk changes.
        self.store.load_with_packages(packages.clone())?;

        let path = self.store.root().to_path_buf();
        self.store.write_project(&project)?;
        self.store.write_resolved_packages_lock(&packages)?;
        // The just-written project declaration is authoritative. The fallback
        // is observed only if `project.toml` disappears concurrently.
        let next = UiSession::open(&path, LibrarySpec::default())?;
        let summary = next.summary();
        *self = next;
        Ok(summary)
    }

    pub fn weight_config(&self) -> Result<WeightConfigV1, UiError> {
        let manual = self
            .workspace
            .project()
            .map(|project| &project.weights)
            .into_iter()
            .flatten()
            .map(|(segment, weight)| WeightEntryV1 {
                segment: segment.clone(),
                weight: *weight,
                source: "manual".to_owned(),
            })
            .collect();
        let distribution = self.effective_distribution(&[])?;
        let effective = distribution
            .resolve()
            .iter()
            .map(|(segment, weight)| WeightEntryV1 {
                segment: segment.to_owned(),
                weight,
                source: match distribution.provenance(segment) {
                    Some(Layer::Manual) => "manual",
                    Some(Layer::Imported) => "imported",
                    Some(Layer::Prior) | None => "prior",
                }
                .to_owned(),
            })
            .collect();
        Ok(WeightConfigV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            declaration_source: "project.toml:weights".to_owned(),
            manual,
            effective,
        })
    }

    /// Manual weights are project authoring data and save immediately, like
    /// node metadata; they do not mutate an immutable language snapshot.
    pub fn set_weights(&mut self, entries: Vec<SegmentWeight>) -> Result<WeightConfigV1, UiError> {
        let mut weights = std::collections::BTreeMap::new();
        for entry in entries {
            let segment = entry.segment.trim();
            if segment.is_empty() || !entry.weight.is_finite() || entry.weight < 0.0 {
                return Err(ui_error(
                    "APP_INVALID_WEIGHT",
                    "segments must be non-empty and weights finite and non-negative",
                ));
            }
            if weights.insert(segment.to_owned(), entry.weight).is_some() {
                return Err(ui_error(
                    "APP_DUPLICATE_WEIGHT",
                    format!("duplicate segment {segment:?}"),
                ));
            }
        }

        let mut project =
            self.workspace.project().cloned().unwrap_or_else(|| {
                ProjectDocument::from_spec(self.workspace.session().libraries())
            });
        project.weights = weights;
        self.store.write_project(&project)?;
        self.workspace.replace_project(project);
        self.last_proposals = None;
        self.weight_config()
    }

    // ── 演化樹面板 ──────────────────────────────────────────────────────

    pub fn tree(&self) -> EvolutionTreeV1 {
        self.workspace.tree()
    }

    /// 點一個節點 → 切過去並回傳編輯頁要的東西。
    pub fn select_node(&mut self, id: &str) -> Result<NodeDetailV1, UiError> {
        if self.workspace.session().pending().is_some() {
            return Err(ui_error(
                "APP_DIRTY_WORKING_COPY",
                "save or discard the pending .chg before switching nodes",
            ));
        }
        let node = NodeId::parse(id).map_err(|_| UiError {
            code: "APP_UNKNOWN_NODE".to_owned(),
            message: id.to_owned(),
        })?;
        self.workspace.session_mut().open(&node)?;
        self.last_proposals = None;
        self.node_detail()
    }

    // ── 辭典面板 ────────────────────────────────────────────────────────

    pub fn lexicon(&mut self, query: &LexiconQuery) -> Result<LexiconViewV1, UiError> {
        let (filter, view) = query.split();
        Ok(self.workspace.lexicon_view(&filter, &view)?)
    }

    // ── 節點編輯頁(**全部雜湊外**,改它們不動任何 replay 產物)──────────

    pub fn node_detail(&self) -> Result<NodeDetailV1, UiError> {
        Ok(self.workspace.node_detail(&self.store)?)
    }

    /// 改標籤。標籤不進 node-v2 雜湊,故**改名不改身分**。
    pub fn set_label(&mut self, label: Option<String>) -> Result<NodeDetailV1, UiError> {
        let id = self.active()?;
        self.workspace
            .session_mut()
            .graph_mut()
            .set_label(&id, label)
            .map_err(|error| UiError::of(AppError::Evolution(error)))?;
        self.workspace.session().persist(&self.store)?;
        self.node_detail()
    }

    /// 改環境。裁定 (A):State 只在撰寫時被讀,**replay 永不看它**。
    pub fn set_state(&mut self, state: &EvolutionState) -> Result<NodeDetailV1, UiError> {
        let id = self.active()?;
        self.store.write_state(&id, state)?;
        // State 只影響撰寫時提議，不影響 replay/lexicon；只清 proposal cache。
        self.last_proposals = None;
        self.node_detail()
    }

    /// 寫旁註。旁註層**正交於本體**(07 §5c):不參與 replay、不被 diff、
    /// 不約束生成。
    pub fn write_annotation(&mut self, path: &str, content: &str) -> Result<NodeDetailV1, UiError> {
        let id = self.active()?;
        self.store.write_annotation(&id, path, content.as_bytes())?;
        self.node_detail()
    }

    pub fn read_annotation(&self, path: &str) -> Result<String, UiError> {
        let id = self.active()?;
        let bytes = self.store.read_annotation(&id, path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    // ── working copy / 演化編寫 ────────────────────────────────────────

    pub fn begin_edit(&mut self, namespace: &str) -> Result<PendingChangeV1, UiError> {
        self.workspace.session_mut().begin_edit(namespace)?;
        self.pending_change()
    }

    pub fn pending_change(&self) -> Result<PendingChangeV1, UiError> {
        let session = self.workspace.session();
        let source = session
            .pending_source()?
            .ok_or_else(|| ui_error("APP_NO_PENDING_CHANGE", "no pending .chg"))?;
        let statements = session
            .pending()
            .map(|pending| pending.statements.len())
            .unwrap_or(0);
        let diff = session.preview_pending()?;
        Ok(PendingChangeV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            source,
            statements,
            diff: diff.into(),
        })
    }

    pub fn replace_pending_source(&mut self, source: &str) -> Result<PendingChangeV1, UiError> {
        self.workspace
            .session_mut()
            .replace_pending_source(source)?;
        self.pending_change()
    }

    pub fn authoring_catalog(&self) -> Result<AuthoringCatalogV1, UiError> {
        let document = self.workspace.session().preview_document()?;
        let session = self.workspace.session();
        build_authoring_catalog(&document, session.libraries(), session.packages())
    }

    pub fn authoring_move_options(
        &self,
        target: &str,
        revision: &str,
    ) -> Result<AuthoringMoveOptionsV1, UiError> {
        let document = self.workspace.session().preview_document()?;
        ensure_authoring_revision(&document, revision)?;
        let session = self.workspace.session();
        let catalog = build_authoring_catalog(&document, session.libraries(), session.packages())?;
        let placements = legal_move_options(
            &document,
            session.libraries(),
            session.packages(),
            &catalog,
            target,
        )?;
        Ok(AuthoringMoveOptionsV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            revision: catalog.revision,
            target: target.to_owned(),
            placements,
        })
    }

    pub fn stage_structured_edit(
        &mut self,
        input: &StructuredEditInput,
    ) -> Result<PendingChangeV1, UiError> {
        let document = self.workspace.session().preview_document()?;
        ensure_authoring_revision(&document, &input.revision)?;
        let libraries = self.workspace.session().libraries().clone();
        let packages = self.workspace.session().packages();
        let catalog = build_authoring_catalog(&document, &libraries, packages)?;
        if let StructuredEdit::Move { target, placement } = &input.edit {
            let legal = legal_move_options(&document, &libraries, packages, &catalog, target)?;
            if !legal.iter().any(|candidate| {
                candidate.parent == placement.parent
                    && candidate.position == placement.position
                    && candidate.sibling == placement.sibling
            }) {
                return Err(ui_error(
                    "APP_AUTHORING_MOVE_INVALID",
                    "the selected move placement is no longer legal",
                ));
            }
        }
        let operation = structured_operation(&document, &catalog, &input.edit)?;
        let namespace = self
            .workspace
            .session()
            .pending()
            .map(|pending| pending.namespace.clone())
            .unwrap_or_else(|| format!("ui:evolve:{}", self.workspace.session().graph().len()));
        let edits =
            resolve_authoring_statement(&document, &libraries, packages, &namespace, &operation)?;
        self.workspace
            .session_mut()
            .stage_checked(&namespace, edits)?;
        self.pending_change()
    }

    pub fn stage_sound_change(
        &mut self,
        input: &SoundChangeInput,
    ) -> Result<PendingChangeV1, UiError> {
        let rule = input.rule.trim();
        let home = input.home.trim();
        if rule.is_empty() {
            return Err(ui_error("APP_RULE_EMPTY", "sound-change rule is empty"));
        }
        if home.is_empty() {
            return Err(ui_error(
                "APP_RULE_HOME_EMPTY",
                "sound-change rule home is empty",
            ));
        }

        // Lower against the complete pending preview, then append and replay a
        // candidate ChangeSet. This lets a freshly inserted global trait serve
        // as the home of the very next sound change without mutating pending on
        // any failure.
        let document = self.workspace.session().preview_document()?;
        if let Some(revision) = input.revision.as_deref() {
            ensure_authoring_revision(&document, revision)?;
        }
        let session = self.workspace.session();
        let catalog = build_authoring_catalog(&document, session.libraries(), session.packages())?;
        if !catalog.rule_homes.iter().any(|choice| choice.value == home) {
            return Err(ui_error(
                "APP_RULE_HOME_UNKNOWN",
                "sound-change home must be a local global trait",
            ));
        }
        let rewrite = AtomicRewrite::SoundChange {
            home: RuleHome::Global(home.to_owned()),
            body: rule.to_owned(),
        };
        let edits = conlang_command::lower(
            &LanguageCommand::ApplyRewrite(&rewrite),
            &document,
            &Strategies::default(),
            &ServiceContext::offline(),
            &DonorScope::new(),
        )
        .map_err(UiError::of)?;
        let namespace = self
            .workspace
            .session()
            .pending()
            .map(|pending| pending.namespace.clone())
            .unwrap_or_else(|| format!("ui:evolve:{}", self.workspace.session().graph().len()));
        self.workspace
            .session_mut()
            .stage_checked(&namespace, edits)?;
        self.pending_change()
    }

    pub fn discard_last_edit(&mut self) -> Result<PendingChangeV1, UiError> {
        self.workspace.session_mut().discard_last_edit()?;
        self.pending_change()
    }

    pub fn save_working_copy(&self, path: impl AsRef<Path>) -> Result<(), UiError> {
        self.workspace.session().save_working_copy(path.as_ref())?;
        Ok(())
    }

    pub fn load_working_copy(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<PendingChangeV1, UiError> {
        self.workspace
            .session_mut()
            .load_working_copy(path.as_ref())?;
        self.pending_change()
    }

    pub fn commit(&mut self, label: Option<String>) -> Result<NodeDetailV1, UiError> {
        self.workspace.session_mut().commit(label)?;
        self.workspace.drop_caches();
        self.graph_dirty = true;
        self.last_proposals = None;
        self.node_detail()
    }

    /// 明確的 graph persist 邊界；commit 本身只建立記憶體節點。
    pub fn save_project(&mut self) -> Result<ProjectSummaryV1, UiError> {
        self.workspace.session().persist(&self.store)?;
        self.graph_dirty = false;
        Ok(self.summary())
    }

    pub fn undo_navigation(&mut self) -> Result<NodeDetailV1, UiError> {
        self.ensure_no_pending()?;
        self.workspace.session_mut().undo()?;
        self.last_proposals = None;
        self.node_detail()
    }

    pub fn redo_navigation(&mut self) -> Result<NodeDetailV1, UiError> {
        self.ensure_no_pending()?;
        self.workspace.session_mut().redo()?;
        self.last_proposals = None;
        self.node_detail()
    }

    pub fn remove_active_leaf(&mut self) -> Result<EvolutionTreeV1, UiError> {
        self.ensure_no_pending()?;
        let id = self.active()?;
        let has_dependents = self.workspace.session().graph().ids().any(|candidate| {
            self.workspace
                .session()
                .graph()
                .node(candidate)
                .map(|node| node.parents().iter().any(|edge| edge.from == id))
                .unwrap_or(false)
        });
        if has_dependents {
            return Err(ui_error(
                "APP_NODE_HAS_DEPENDENTS",
                "only leaf nodes can be deleted",
            ));
        }
        self.store.remove_node(&id)?;
        self.workspace.session_mut().remove_node(&id)?;
        self.workspace.drop_caches();
        self.last_proposals = None;
        Ok(self.tree())
    }

    // ── generation / statistics ────────────────────────────────────────

    pub fn propose(&mut self, query: &ProposalQuery) -> Result<ProposalsViewV1, UiError> {
        if query.count == 0 || query.count > 512 {
            return Err(ui_error(
                "APP_PROPOSAL_COUNT",
                "proposal count must be between 1 and 512",
            ));
        }
        let need = Need {
            name: query.name.clone(),
            categories: query.categories.clone(),
            gloss: query.gloss.clone(),
            origin: NeedOrigin::Coined,
        };
        let distribution = self.effective_distribution(&query.weights)?.resolve();
        let compiled = self.workspace.compiled()?;
        let generated = DistributionGenerator {
            distribution: &distribution,
            template: &query.template,
            count: query.count,
            seed: query.seed,
        }
        .propose(&need, &compiled)
        .map_err(UiError::of)?;
        let proposals: Vec<Proposal> = conlang_generate::ranked(&generated)
            .into_iter()
            .cloned()
            .collect();
        let view = ProposalsViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node: self.active()?.as_str().to_owned(),
            proposals: proposals.iter().map(proposal_view).collect(),
        };
        self.last_proposals = Some(CachedProposals {
            query: query.clone(),
            need,
            proposals,
        });
        Ok(view)
    }

    /// 採用只 stage 到 pending `.chg`，永不偷偷 commit。
    pub fn adopt_proposal(
        &mut self,
        query: &ProposalQuery,
        index: usize,
    ) -> Result<PendingChangeV1, UiError> {
        let cache = self.last_proposals.as_ref().ok_or_else(|| {
            ui_error(
                "APP_PROPOSALS_STALE",
                "generate proposals again before adopting",
            )
        })?;
        if &cache.query != query {
            return Err(ui_error(
                "APP_PROPOSALS_STALE",
                "proposal inputs changed; generate again",
            ));
        }
        let chosen =
            cache.proposals.get(index).cloned().ok_or_else(|| {
                ui_error("APP_UNKNOWN_PROPOSAL", format!("proposal index {index}"))
            })?;
        let need = cache.need.clone();
        let document = self.workspace.session().snapshot()?.clone();
        let edits = conlang_command::lower(
            &LanguageCommand::AdoptProposal {
                need: &need,
                chosen: &chosen,
            },
            &document,
            &Strategies::default(),
            &ServiceContext::offline(),
            &DonorScope::new(),
        )
        .map_err(UiError::of)?;
        if self.workspace.session().pending().is_none() {
            let namespace = format!("ui:coin:{}", self.workspace.session().graph().len());
            self.workspace.session_mut().begin_edit(&namespace)?;
        }
        self.workspace.session_mut().stage(edits)?;
        self.last_proposals = None;
        self.pending_change()
    }

    pub fn stats(&self, inventory: &[String]) -> Result<StatsViewV1, UiError> {
        let document = self.workspace.session().snapshot()?;
        let refs: Vec<&str> = inventory.iter().map(String::as_str).collect();
        let report = project_phoneme_freq(document.language(), &refs);
        Ok(StatsViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node: self.active()?.as_str().to_owned(),
            segmentation: if inventory.is_empty() {
                "per-character".to_owned()
            } else {
                "longest-match".to_owned()
            },
            sampling_source: false,
            segments: report
                .iter()
                .map(|(segment, count)| SegmentStatV1 {
                    segment: segment.to_owned(),
                    count,
                })
                .collect(),
        })
    }

    // ── analysis / views ───────────────────────────────────────────────

    pub fn grouping(&self, query: &GroupingQuery) -> Result<GroupingViewV1, UiError> {
        if !query.threshold.is_finite() || query.threshold < 0.0 {
            return Err(ui_error(
                "APP_GROUP_THRESHOLD",
                "threshold must be finite and non-negative",
            ));
        }
        let view = self.store.read_view(&query.view)?;
        let override_ = GroupingOverride {
            assignments: view.assignments,
            labels: view.labels,
        };
        Ok(self.workspace.grouping_view(
            &TreeEdgeCut {
                threshold: query.threshold,
            },
            &ExploratoryHeuristicV1::suggested(),
            &override_,
        ))
    }

    pub fn assign_group(
        &self,
        query: &GroupingQuery,
        node: String,
        group: String,
    ) -> Result<GroupingViewV1, UiError> {
        let mut document = self.store.read_view(&query.view)?;
        apply_view_command(
            &mut document,
            &ViewCommand::AssignGroup {
                view: query.view.clone(),
                node,
                group,
            },
        );
        self.store.write_view(&query.view, &document)?;
        self.grouping(query)
    }

    pub fn label_group(
        &self,
        query: &GroupingQuery,
        group: String,
        label: String,
    ) -> Result<GroupingViewV1, UiError> {
        let mut document = self.store.read_view(&query.view)?;
        apply_view_command(
            &mut document,
            &ViewCommand::LabelGroup {
                view: query.view.clone(),
                group,
                label,
            },
        );
        self.store.write_view(&query.view, &document)?;
        self.grouping(query)
    }

    pub fn intelligibility(
        &self,
        source: &str,
        target: &str,
    ) -> Result<IntelligibilityViewV1, UiError> {
        let source_id = parse_node(source)?;
        let target_id = parse_node(target)?;
        let source_document = self
            .workspace
            .session()
            .graph()
            .snapshot(&source_id)
            .map_err(AppError::Evolution)?;
        let target_document = self
            .workspace
            .session()
            .graph()
            .snapshot(&target_id)
            .map_err(AppError::Evolution)?;
        let vector = diff_vector(source_document, target_document);
        Ok(IntelligibilityViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            source: source.to_owned(),
            target: target.to_owned(),
            score: intelligibility(
                source_document,
                target_document,
                &ExploratoryHeuristicV1::suggested(),
            ),
            diff: vector.into(),
        })
    }

    pub fn derivation(&mut self, sign: &str) -> Result<DerivationViewV1, UiError> {
        let node = self.active()?.as_str().to_owned();
        let compiled = self.workspace.compiled()?;
        Ok(DerivationViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node,
            family: derivation_family(&compiled, sign),
        })
    }

    pub fn source(&self) -> Result<SourceViewV1, UiError> {
        let id = self.active()?;
        Ok(SourceViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node: id.as_str().to_owned(),
            source: self.workspace.session().snapshot()?.source(),
        })
    }

    /// 將編輯過的 canonical `.lang` 原子轉成 pending `.chg`。
    ///
    /// reconcile 與 reconstruct 都先在區域值上完成。任何 ambiguity、parse 或
    /// unsupported error 都不會碰 session；只有兩步都成功才建立 working copy。
    pub fn reconcile_source(&mut self, source: &str) -> Result<SourceReconcileV1, UiError> {
        self.ensure_no_pending()?;
        let before = self.workspace.session().snapshot()?.clone();
        let namespace = format!("ui:source:{}", self.workspace.session().graph().len());
        let (after, report) = conlang_changeset::reconcile::reconcile_edited_source(
            &before,
            source,
            &namespace,
            &conlang_changeset::reconcile::ReconcileHints::default(),
        )
        .map_err(UiError::of)?;
        let edits =
            conlang_changeset::reconstruct::reconstruct(&before, &after).map_err(UiError::of)?;
        if edits.is_empty() {
            return Err(ui_error(
                "APP_SOURCE_UNCHANGED",
                "the edited .lang has no semantic changes",
            ));
        }
        let primitive_edits = edits.len();

        self.workspace.session_mut().begin_edit(&namespace)?;
        self.workspace.session_mut().stage(edits)?;
        let pending = self.pending_change()?;
        Ok(SourceReconcileV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            matched: report.matched.len(),
            inserted: report.inserted.len(),
            deleted: report.deleted.len(),
            primitive_edits,
            pending,
        })
    }

    /// 在圖副本上執行 rebase。即使結果是 clean，也不會改 active 或新增正式節點。
    pub fn preview_rebase(&self, node: &str, onto: &str) -> Result<RebasePreviewV1, UiError> {
        self.ensure_no_pending()?;
        let node = parse_node(node)?;
        let onto = parse_node(onto)?;
        let mut preview = self.workspace.session().graph().clone();
        let outcome = preview
            .rebase(&node, &onto)
            .map_err(|error| UiError::of(AppError::Evolution(error)))?;
        Ok(rebase_view(&node, &onto, outcome).0)
    }

    /// 使用者確認 preview 後才在正式圖上重跑同一 rebase。
    /// 衝突結果不會建立節點；clean 結果建立並存的新節點，舊鏈保持不動。
    pub fn apply_rebase(&mut self, node: &str, onto: &str) -> Result<RebasePreviewV1, UiError> {
        self.ensure_no_pending()?;
        let node = parse_node(node)?;
        let onto = parse_node(onto)?;
        let outcome = self
            .workspace
            .session_mut()
            .graph_mut()
            .rebase(&node, &onto)
            .map_err(|error| UiError::of(AppError::Evolution(error)))?;
        let (view, result) = rebase_view(&node, &onto, outcome);
        if let Some(result) = result {
            self.workspace.session_mut().open(&result)?;
            self.workspace.drop_caches();
            self.graph_dirty = true;
            self.last_proposals = None;
        }
        Ok(view)
    }

    fn ensure_no_pending(&self) -> Result<(), UiError> {
        if self.workspace.session().pending().is_some() {
            return Err(ui_error(
                "APP_DIRTY_WORKING_COPY",
                "save or discard the pending .chg first",
            ));
        }
        Ok(())
    }

    fn active(&self) -> Result<NodeId, UiError> {
        self.workspace
            .session()
            .active()
            .cloned()
            .ok_or_else(|| UiError::of(AppError::NoActiveNode))
    }

    fn effective_distribution(
        &self,
        overrides: &[SegmentWeight],
    ) -> Result<EffectiveDistribution, UiError> {
        let session = self.workspace.session();
        let legacy_catalog;
        let legacy_selected;
        let packages = match session.packages() {
            Some(resolved) => resolved.packages().iter().collect::<Vec<_>>(),
            None => {
                legacy_catalog = conlang_language::library::embedded_catalog()
                    .map_err(|error| UiError::of(AppError::Library(error)))?;
                legacy_selected = legacy_catalog
                    .select(session.libraries())
                    .map_err(|error| UiError::of(AppError::Library(error)))?;
                legacy_selected
                    .packages
                    .iter()
                    .filter_map(|id| {
                        legacy_catalog
                            .packages()
                            .iter()
                            .find(|package| &package.id == id)
                    })
                    .collect::<Vec<_>>()
            }
        };
        let prior = load_prior_from_packages(&packages)
            .map_err(|error| ui_error("APP_WEIGHT_PRIOR", error.to_string()))?;
        let mut manual = WeightTable::new();
        if let Some(project) = self.workspace.project() {
            for (segment, weight) in &project.weights {
                manual.set(segment.clone(), *weight);
            }
        }
        for entry in overrides {
            if entry.segment.trim().is_empty() || !entry.weight.is_finite() || entry.weight < 0.0 {
                return Err(ui_error(
                    "APP_INVALID_WEIGHT",
                    "segments must be non-empty and weights finite and non-negative",
                ));
            }
            manual.set(entry.segment.trim(), entry.weight);
        }
        Ok(EffectiveDistribution::from_prior(prior).with_manual(manual))
    }
}

fn proposal_view(proposal: &Proposal) -> ProposalV1 {
    ProposalV1 {
        phon: proposal.phon.clone(),
        score: proposal.score,
        rationale: proposal.rationale.clone(),
    }
}

fn parse_node(id: &str) -> Result<NodeId, UiError> {
    NodeId::parse(id).map_err(|_| ui_error("APP_UNKNOWN_NODE", id))
}

fn rebase_view(
    node: &NodeId,
    onto: &NodeId,
    outcome: RebaseOutcome,
) -> (RebasePreviewV1, Option<NodeId>) {
    let (status, statement, message, result) = match outcome {
        RebaseOutcome::Clean(id) => ("clean", None, None, Some(id)),
        RebaseOutcome::Conflict { statement, error } => {
            ("conflict", statement, Some(error.to_string()), None)
        }
        RebaseOutcome::Environment(error) => ("environment", None, Some(error.to_string()), None),
        RebaseOutcome::Broken(error) => ("broken", None, Some(error.to_string()), None),
    };
    let result_text = result.as_ref().map(|id| id.as_str().to_owned());
    (
        RebasePreviewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node: node.as_str().to_owned(),
            onto: onto.as_str().to_owned(),
            status: status.to_owned(),
            statement,
            message,
            result: result_text,
        },
        result,
    )
}

/// Launcher 可以在沒有專案時存在；Tauri 只管理這一個 slot。
#[derive(Debug, Default)]
pub struct ProjectSlot {
    session: Option<UiSession>,
}

impl ProjectSlot {
    pub fn summary(&self) -> Option<ProjectSummaryV1> {
        self.session.as_ref().map(UiSession::summary)
    }

    pub fn open(
        &mut self,
        path: impl AsRef<Path>,
        discard_dirty: bool,
    ) -> Result<ProjectSummaryV1, UiError> {
        self.ensure_replace_allowed(discard_dirty)?;
        let next = UiSession::open(path, LibrarySpec::default())?;
        let summary = next.summary();
        self.session = Some(next);
        Ok(summary)
    }

    pub fn create(
        &mut self,
        path: impl AsRef<Path>,
        source_path: Option<impl AsRef<Path>>,
        name: Option<String>,
        namespace: &str,
        discard_dirty: bool,
    ) -> Result<ProjectSummaryV1, UiError> {
        self.ensure_replace_allowed(discard_dirty)?;
        let next = UiSession::create(path, source_path, name, namespace)?;
        let summary = next.summary();
        self.session = Some(next);
        Ok(summary)
    }

    pub fn close(&mut self, discard_dirty: bool) -> Result<(), UiError> {
        self.ensure_replace_allowed(discard_dirty)?;
        self.session = None;
        Ok(())
    }

    pub fn session(&self) -> Result<&UiSession, UiError> {
        self.session
            .as_ref()
            .ok_or_else(|| ui_error("APP_NO_PROJECT", "no project is open"))
    }

    pub fn session_mut(&mut self) -> Result<&mut UiSession, UiError> {
        self.session
            .as_mut()
            .ok_or_else(|| ui_error("APP_NO_PROJECT", "no project is open"))
    }

    fn ensure_replace_allowed(&self, discard_dirty: bool) -> Result<(), UiError> {
        if !discard_dirty && self.session.as_ref().is_some_and(UiSession::is_dirty) {
            return Err(ui_error(
                "APP_DIRTY_PROJECT",
                "save or discard the current project before replacing it",
            ));
        }
        Ok(())
    }
}
