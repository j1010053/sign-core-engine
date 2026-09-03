//! conlang-language — 共時語言知識檔(2.0 步驟 8)。
//!
//! **Language = 語言知識的唯一存放處**(P8):Global / Trait / Sign 三容器 +
//! Definition(`=`)與 Rule(`=>`)兩種語句(P9)。本 crate 提供:
//! - 五組 AST 節點(修補05 §10.3):①定義 ②規則(帶 RuleId)③容器 ④Ref ⑤分佈;
//! - **canonical empty root**(P28):`Language::new()` 永遠存在,四原語有處掛靠;
//! - **canonical printer**(P21):IR dump = Language 源文字的 canonical form,
//!   確定性(區段序固定、具名容器按名排序、規則保序、集合排鍵;I15-d)。
//!
//! dsl 域宣告(feature/symbol/class,Lexurgy 形)以**不透明區塊**承載(I15-a,
//! 裁決 docs/13 §4-1):language 不解析,step 11+ 原樣交給 `tshiatun_dsl::compile`。
//! `RuleId`/`SignId` 不入印出格式；local source re-parse 依文件序決定性再生，
//! package source 則由其 `config/package.conf` 的 `rule_namespace` 重綁定
//! (I15-b/P26、修補06 §1.2)。
//! 依賴方向:`language → dsl`(P20);本 crate 對 dsl 的使用僅限公開型別。

//! # Minimal document
//!
//! ```
//! let language = conlang_language::Language::new();
//! assert_eq!(language.dump(), "");
//! ```
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod codegen;
pub mod compile;
pub mod construction;
pub mod diagnostic;
pub mod identity;
pub mod library;
pub mod metadata;
pub mod ontology;
pub mod parser;
pub mod patch;
pub mod path;
pub mod printer;
pub mod projection;
pub mod reference;
pub mod sampling;
pub mod sem;
pub mod semantic_dto;
pub mod stdlib;
pub mod synchronic;
pub mod system;
pub mod word;

pub use construction::{OccurrenceCaseRecord, OccurrenceCaseStatus, OccurrenceRecord};
pub use diagnostic::{Diagnostic, DiagnosticSource, Severity, SourceLocation, ValidationReport};
pub use identity::{
    sha256_hex, AddressSegment, AstNode, EditableField, IdentityAllocatorV2, IdentityError,
    IdentityManifestV2, IdentityNamespace, LanguageDocument, NodeAddress, NodeEntryV1, NodeId,
    NodeKind, NodeRef, RefBindingV1, RefTargetV1, ResolvedTarget, IDENTITY_SCHEMA_V2,
};
pub use library::table_type;
pub use library::{
    LibraryCatalog, LibraryDataSource, LibraryExport, LibraryExportKind, LibraryFunctionSource,
    LibraryId, LibraryKind, LibraryLoadError, LibraryPackage, LibrarySpec, PackageCapabilities,
    PackageFile, PackageId, PackageLayer, PackageRequirement, PackageResolver, PackageSource,
    PackageSources, PackageSpec, ResolvedPackages, SelectedPackage,
};
pub use metadata::{SignLifecycle, SignProvenance};
pub use sampling::{
    sample_weighted_index, WeightedSampleError, WeightedSampleTrace, WEIGHTED_SAMPLER_ALGORITHM,
};
pub use semantic_dto::{
    SemanticDocumentError, SemanticDocumentV1, SemanticNodeV1, SemanticSourceV1, SEMANTIC_SCHEMA_V1,
};
pub use stdlib::{StdExport, StdExportKind, StdLoadError, StdPackage};
pub use system::{
    check_document, check_document_with_packages, check_language, check_language_with_libraries,
    check_language_with_packages, compile_document, compile_document_with_packages, compile_system,
    compile_system_ref, compile_with_libraries, compile_with_libraries_ref,
    compile_with_packages_ref, CandidateSelectionTrace, CandidateSelector, CandidateSet,
    CaseBranchStatus, CaseRecord, CompileSystemError, CompiledSystem, ConstructionCandidate,
    DerivationContext, EvaluatedToken, PhonRealization, RealizedPhonInput,
    SignExpressionEvaluation, SignValue, SystemDerivation, SystemError, COMPILER_SEMANTICS_VERSION,
};
pub use tshiatun_dsl::lower::Stage;

use serde::{Deserialize, Serialize};

// ── 共時四維(修補07 P38 v0.2;單一分類樹、四個內容面向)──

/// 四個內容彼此獨立的共時維度。分類只有一棵維度中立的 ontology；
/// 正交性由 projection、validation、patch、diff 與 rule write-set 保證。
/// 出境時序列化為小寫關鍵詞(`"phon"`…),與 [`Dim::keyword`] 一致
/// ——UI 看到的字面與 `.lang` 的維度區塊同名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dim {
    Phon,
    Syn,
    Sem,
    Prag,
}

impl Dim {
    /// canonical 關鍵詞(dim-marked trait 頭 + typed projection 路徑前綴)。
    pub fn keyword(self) -> &'static str {
        match self {
            Dim::Phon => "phon",
            Dim::Syn => "syn",
            Dim::Sem => "sem",
            Dim::Prag => "prag",
        }
    }
    pub fn parse(s: &str) -> Option<Dim> {
        match s {
            "phon" => Some(Dim::Phon),
            "syn" => Some(Dim::Syn),
            "sem" => Some(Dim::Sem),
            "prag" => Some(Dim::Prag),
            _ => None,
        }
    }
    /// 四個內容面向的固定巡覽序。
    pub fn all() -> [Dim; 4] {
        [Dim::Phon, Dim::Syn, Dim::Sem, Dim::Prag]
    }
}

// ── ④ 引用類(P24:Ref 是屬性值,非圖邊)──

macro_rules! ref_ty {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(pub String);
    };
}
ref_ty!(
    /// 指向 sign(穩定 ID 定址)。
    SignRef
);
ref_ty!(
    /// 指向 sense。
    SenseRef
);
ref_ty!(
    /// 指向規則(fossilize/generalize 的搬移對象)。
    RuleRef
);
ref_ty!(
    /// 指向 trait。
    TraitRef
);
ref_ty!(
    /// 指向概念網絡節點。
    ConceptRef
);

// ── ID(P26:namespace + 純序列配發;不入印出格式,I15-b)──

/// The ownership domain of a [`RuleId`].
///
/// `.lang` by itself is intentionally local: a package loader is the authority
/// that binds package code to the stable namespace declared in its config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuleNamespace {
    #[default]
    Local,
    Document(String),
    Package(String),
}

impl std::fmt::Display for RuleNamespace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleNamespace::Local => formatter.write_str("local"),
            RuleNamespace::Document(namespace) => formatter.write_str(namespace),
            RuleNamespace::Package(namespace) => formatter.write_str(namespace),
        }
    }
}

/// Stable rule identity within its source domain.
///
/// `ordinal` is deterministic source order *inside* one namespace. A local
/// rule and a standard-package rule may both have ordinal `0`, but are never
/// the same identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(pub NodeId);

impl RuleId {
    pub fn local(ordinal: u32) -> RuleId {
        RuleId(NodeId::new(IdentityNamespace::Ephemeral, ordinal.into()))
    }

    pub fn package(namespace: impl Into<String>, ordinal: u32) -> RuleId {
        RuleId(NodeId::new(
            IdentityNamespace::Library(namespace.into()),
            ordinal.into(),
        ))
    }

    pub fn document(namespace: impl Into<String>, ordinal: u64) -> RuleId {
        RuleId(NodeId::new(
            IdentityNamespace::Document(namespace.into()),
            ordinal,
        ))
    }

    pub fn namespace(&self) -> RuleNamespace {
        match &self.0.namespace {
            IdentityNamespace::Ephemeral | IdentityNamespace::Synthetic => RuleNamespace::Local,
            IdentityNamespace::Document(namespace) => RuleNamespace::Document(namespace.clone()),
            IdentityNamespace::Library(namespace) => RuleNamespace::Package(namespace.clone()),
        }
    }

    pub fn ordinal(&self) -> u64 {
        self.0.ordinal
    }

    pub fn node_id(&self) -> &NodeId {
        &self.0
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SignId(pub NodeId);

impl SignId {
    pub fn local(ordinal: u64) -> SignId {
        SignId(NodeId::new(IdentityNamespace::Ephemeral, ordinal))
    }

    pub fn document(namespace: impl Into<String>, ordinal: u64) -> SignId {
        SignId(NodeId::new(
            IdentityNamespace::Document(namespace.into()),
            ordinal,
        ))
    }

    pub fn package(namespace: impl Into<String>, ordinal: u64) -> SignId {
        SignId(NodeId::new(
            IdentityNamespace::Library(namespace.into()),
            ordinal,
        ))
    }

    pub fn synthetic() -> SignId {
        SignId(NodeId::new(IdentityNamespace::Synthetic, u64::MAX))
    }

    pub fn node_id(&self) -> &NodeId {
        &self.0
    }
}

impl std::fmt::Display for SignId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

// ── ①/② 語句 ──

/// Definition(`=`):語言知識,無執行順序(P9;compile 依欄位 Merge Strategy 合併)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    /// 左端路徑(`syn.provides`、`phon`、`entrenchment`…)。
    pub path: String,
    /// 右端值(步驟 8 以 canonical 原文承載;步驟 9 起結構化為 Ref/字面值)。
    pub value: String,
}

/// A closed, dimension-local feature domain declared by `feature:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureDecl {
    pub dim: Dim,
    pub name: String,
    pub values: Vec<String>,
    /// P75:尾綴 `?` = **這條 feature 可以沒有值**。
    ///
    /// 與 `slot NAME [C]?` / `role NAME [C]?` 同形同義——`?` 在本語言裡只有一個
    /// 意思(可以不提供),而且一律住在**宣告**處,讀取處繼承。沒有 `?` 時,讀到
    /// 缺席是執行期 Error 而非靜默 `Unmatched`。
    ///
    /// **canonical 上可省略**:`false` 不印,故未使用此語法的套件其 canonical form
    /// 與 library lock digest 逐位元不變(P75 §3 b)。
    pub optional: bool,
    pub source: SourceLocation,
}

/// A typed feature value. Unlike a generic [`Def`], this is only valid when a
/// matching [`FeatureDecl`] is visible on the effective sign.
///
/// **值域,不是宣告域。**`FeatureDecl.values` 說「哪些值合法」(型別,作者寫的
/// schema);這裡的 `values` 說「這個主體實際是哪幾個」,恆為宣告域的子集。
///
/// * `len == 1` — 已定案,等同舊的單值語意。
/// * `len >= 2` — **未定案值域**:此主體在該維度尚未收斂,候選就是這幾個。
///   來源有二:作者直寫(`number = singular | plural`,如單複同形的 *fish*),
///   或投影時多個掛載 trait 給了不同的值而取聯集。決議留給構式——同一個 sign
///   在不同構式中收斂到不同的值,是語言事實而非錯誤,所以這裡不靠優先序挑一個
///   贏家(挑了就把事實刪掉了)。
///
/// 空集合不合法:「沒有值」由**不寫這個項目**表示,那條路歸 P75 的 `?` 管。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureValue {
    pub dim: Dim,
    pub name: String,
    pub values: Vec<String>,
    pub source: SourceLocation,
}

impl FeatureValue {
    /// 已定案時回傳那個唯一的值;未定案(`len >= 2`)回 `None`。
    ///
    /// 下游凡是「非得有一個值才能繼續」的地方都該走這裡,而不是 `values[0]`
    /// ——取 `[0]` 會把未定案默默當成已定案,正是這個型別要擋掉的事。
    pub fn decided(&self) -> Option<&str> {
        match self.values.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    /// 未定案 = 候選多於一個。
    pub fn is_undecided(&self) -> bool {
        self.values.len() > 1
    }
}

/// A construction-local binding for a `syn` feature on one of its slots.
///
/// Source form is `TARGET_SLOT.FEATURE = VALUE` where `VALUE` is either an
/// enum literal or `$slot.SOURCE_SLOT.syn.FEATURE`. The parser records this
/// separately from ordinary feature values because its target is a filler
/// occurrence, not the construction token itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFeatureBinding {
    pub slot: String,
    pub feature: String,
    /// Canonical source spelling: an enum literal or `$slot.SOURCE.syn.FEATURE`.
    pub value: String,
    pub source: SourceLocation,
}

/// A semantic role contract. Frame identity remains ontology membership; the
/// role records only its typed structural argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleDecl {
    pub name: String,
    /// Typed filler authorization, **與 slot 同型**(`[*]` = `AnySign`)。
    ///
    /// P71-S:此處曾是裸 `String`,而「任何有意義的節點」只能寫成 `[Semantic]`
    /// ——靠引擎無條件把 `Semantic` 塞進每個 sign 的型別來成立。那是引擎硬寫
    /// std 詞彙;`[*]` 才是該表達的東西。
    pub constraint: SlotConstraint,
    pub optional: bool,
    pub source: SourceLocation,
}

/// A role-to-construction-slot mapping. V1 deliberately accepts only a slot
/// reference, keeping fillers immutable and avoiding arbitrary graph writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleBinding {
    pub name: String,
    pub slot: String,
    pub source: SourceLocation,
}

/// 義項(sense)——**sem 維的一級節點**(《修補05》§10.3「sign 內:… sem(senses +
/// 衍生邊)」;docs/07 §5)。多義 = 多個 `Sense`,**各有身分**(可定址、可被四原語
/// 編輯),取代先前用自創欄位名(`sense2 = …`)假裝義項的土法。
/// Atomic Rewrite `derive_sense` / `drift` 的作用對象(P16)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sense {
    pub name: String,
    /// 義項的語意內容(純量;複雜語意模型日後以新欄位擴充,不破壞此 API)。
    pub gloss: String,
    pub source: SourceLocation,
}

/// 衍生邊的種類(P16 `derive_sense{kind: metaphor|metonymy|narrow|broaden}`)。
///
/// **目前無消費者**(已知延後,非缺陷):全庫沒有任何語意分支讀取本值——
/// parser/printer/DTO/diff/Primitive Edit 都只是原樣搬運,四個 variant 之間
/// 的差異不影響任何行為。語意效果待《測試案例集總索引》實例 7「語用隱喻固化」
/// (現況 ⚪ 未開始)落地,屆時 kind 與 [`SenseTransparency`] 一併成為
/// 語意漂移引擎(B)的輸入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DerivationKind {
    Metaphor,
    Metonymy,
    Narrow,
    Broaden,
}

impl DerivationKind {
    pub fn keyword(self) -> &'static str {
        match self {
            DerivationKind::Metaphor => "metaphor",
            DerivationKind::Metonymy => "metonymy",
            DerivationKind::Narrow => "narrow",
            DerivationKind::Broaden => "broaden",
        }
    }
    pub fn parse(value: &str) -> Option<DerivationKind> {
        match value {
            "metaphor" => Some(DerivationKind::Metaphor),
            "metonymy" => Some(DerivationKind::Metonymy),
            "narrow" => Some(DerivationKind::Narrow),
            "broaden" => Some(DerivationKind::Broaden),
            _ => None,
        }
    }
}

/// 衍生邊是否仍透明。`Opaque` = 已 `lexicalize_sense`(語源關係固化、不再透明)。
///
/// **目前無消費者**(已知延後,非缺陷):`Transparent`/`Opaque` 之間全庫零語意
/// 分支。唯二觸及本值的地方都不是消費者——[`crate::printer`] 省略預設值
/// `Transparent`(純序列化),`lexicalize_sense` 寫入 `Opaque`(純寫入)。
/// 亦即目前 `lexicalize_sense` 的效果僅止於「被記錄下來」。
///
/// 語意效果待《測試案例集總索引》實例 7「語用隱喻固化」(現況 ⚪ 未開始)落地。
/// 該索引 §「透明度一個欄位」列明本欄位為**四案共測、高優先**的共用欄位:
/// 折磨 6(火車)的 component transparency、實例 7(隱喻固化)、實例 1(複合)、
/// 實例 5(緊密度)——屆時四案共用同一欄位,不另開新欄位。
///
/// 註:15a 造出了表面語法(`sem: edges:` 的 `opaque` 尾綴)卻尚無消費者,
/// 這一點繞過了《共時lang語法與資料貼合度》「不先造無消費者語法」的原則;
/// 保留現狀是為了讓實例 7 落地時四案有共同著力點。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum SenseTransparency {
    #[default]
    Transparent,
    Opaque,
}

impl SenseTransparency {
    pub fn keyword(self) -> &'static str {
        match self {
            SenseTransparency::Transparent => "transparent",
            SenseTransparency::Opaque => "opaque",
        }
    }
    pub fn parse(value: &str) -> Option<SenseTransparency> {
        match value {
            "transparent" => Some(SenseTransparency::Transparent),
            "opaque" => Some(SenseTransparency::Opaque),
            _ => None,
        }
    }
}

/// 義項間的**衍生邊**:`to` 由 `from` 經 `kind` 衍生而來。
/// `transparency` 由 Atomic Rewrite `lexicalize_sense` 翻成 `Opaque`(P16)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenseEdge {
    pub to: String,
    pub from: String,
    pub kind: DerivationKind,
    pub transparency: SenseTransparency,
    pub source: SourceLocation,
}

/// `Default` 已移除:沒有「空 realization」這個狀態(見 `expression` 的說明)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Realization {
    /// Context-typed realization: a `PhonContext` `case:` selecting a full phon
    /// template by guard (shared typed-case machinery; the former flat V1
    /// `RealizationBranch` list was removed with the v1 path).
    ///
    /// **非 `Option`(2026-08-18)**:parser 只在 `case:` 到位時才建這個 item,
    /// 所以「空 realization」在型別上不存在。此前它是 `Option`,而那個 `None`
    /// 是 `NodeUpdate::Realization` 唯一承載的語意(None↔Some 切換);
    /// 收掉 `Option` 之後那個 update 隨之不可達。
    pub expression: TypedCase,
}

impl Expression {
    /// P93:取一個 phon case 分支的**深層模板**。
    ///
    /// 分支有兩種表示:`PhonTemplate`(結構化 phon 表達式,如 interpolation 那類
    /// 的鄰居)與 phon `DimFragment`(模板 + 若干規則)。四個消費端都要問同一個
    /// 問題「這一支的模板是什麼」,故收成一個存取器,免得各自展開 fragment 而
    /// 漏掉其中一處(實作 P93 時就漏過三處)。
    ///
    /// `None` = 這一支沒有自己的模板:純規則分支的 base 是 sign 的深層形。
    pub fn phon_base_template(&self) -> Option<&str> {
        match self {
            Expression::PhonTemplate(template) => Some(template),
            Expression::DimFragment {
                dim: Dim::Phon,
                items,
            } => items.iter().rev().find_map(|item| match item {
                SignItem::Def(def) if def.path == "phon" => Some(def.value.as_str()),
                _ => None,
            }),
            _ => None,
        }
    }

    /// P93:這一支帶的實現規則。非 phon fragment 一律空。
    pub fn phon_branch_rules(&self) -> &[SignItem] {
        match self {
            Expression::DimFragment {
                dim: Dim::Phon,
                items,
            } => items,
            _ => &[],
        }
    }
}

/// The type expected at an expression site.  `Feature` carries its declared
/// enum domain separately during type checking; it is not an untyped string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpressionType {
    /// A case in Sign position may yield a complete Sign expression or a
    /// fragment which is merged into the Sign currently being built.
    SignContext,
    /// A phon case yields a pure phon fragment (currently a complete template
    /// or a full-Sign phon projection).  Trait expansion is deliberately not
    /// available in this context.
    PhonContext,
    /// A fragment confined to the syntactic/form dimension.
    SynContext,
    /// A fragment confined to the semantic/meaning dimension.
    SemContext,
    /// A fragment confined to the pragmatic/function dimension.
    PragContext,
    Feature {
        dim: Dim,
        name: String,
    },
    Role {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignArgumentValue {
    SelfSign,
    Slot(String),
    Application(Box<SignApplication>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignArgument {
    /// `None` is the one-parameter positional shorthand.
    pub name: Option<String>,
    pub value: SignArgumentValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignApplication {
    pub callee: String,
    pub arguments: Vec<SignArgument>,
    pub source: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignProjection {
    Phon,
    Syn,
    Sem,
    Prag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    SignApplication(SignApplication),
    /// An anonymous, typed Sign context.  Its items use the same closed
    /// vocabulary and compile-time trait expansion path as a normal Sign
    /// body, but the fragment has no independent Sign identity.
    SignFragment(Vec<SignItem>),
    /// An anonymous fragment confined to one non-phon dimension.  Phon uses
    /// its existing pure-template representation instead of Sign items.
    DimFragment {
        dim: Dim,
        items: Vec<SignItem>,
    },
    /// A complete Sign application projected into a pure phon template:
    /// `/{callee(...).phon.ret}/`.
    PhonInterpolation(SignApplication),
    Projection {
        value: Box<Expression>,
        dimension: SignProjection,
    },
    PhonTemplate(String),
    EnumValue(String),
    SelfSign,
    Slot(String),
    Case(Box<TypedCase>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseCondition {
    /// `case EXPR:` followed by `== VALUE:`.
    Equals(String),
    /// Guard-form `case:` branch.  The existing guard evaluator supplies the
    /// Matched / Unmatched / Error trichotomy.
    Guard(String),
    Else,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseSelection {
    /// `case:` selects the first Matched branch.
    FirstMatch,
    /// `when:` merges every Matched anonymous fragment in source order.
    Accumulate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseBranch {
    pub condition: CaseCondition,
    pub result: Expression,
    /// Only meaningful for `case<SignContext>`; type checking rejects it elsewhere.
    pub belongs: Vec<String>,
    /// 可選標籤(P 系列取徑 B):`@name <label>` → keyed 定址 `branch["label"]`。
    pub name: Option<String>,
    pub source: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedCase {
    pub selection: CaseSelection,
    pub expected: ExpressionType,
    pub scrutinee: Option<String>,
    /// 可選標籤(P 系列取徑 B):`@name <label>` → keyed 定址 `case["label"]`。
    pub name: Option<String>,
    pub branches: Vec<CaseBranch>,
    pub source: SourceLocation,
}

/// V2 Sign-body expression.  Its enclosing Sign is the implicit/default
/// `$self`, so a case without a matching branch is an identity expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignExpression {
    pub expression: Expression,
    pub source: SourceLocation,
}

/// A V2 typed expression assigned to a declared enum feature. The expression
/// is evaluated in the dimension's normal Syn -> Sem -> Prag order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureExpression {
    pub dim: Dim,
    pub name: String,
    pub expression: Expression,
    pub source: SourceLocation,
}

/// A V2 expression selecting the Sign that fills a semantic role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleExpression {
    pub name: String,
    pub expression: Expression,
    pub source: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintPredicate {
    Equal,
    Before,
    Adjacent,
}

impl ConstraintPredicate {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::Before => "before",
            Self::Adjacent => "adjacent",
        }
    }
}

/// A closed binary constraint.  Operands retain their canonical expression
/// spelling until lowering resolves them to typed feature/form references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryConstraint {
    pub predicate: ConstraintPredicate,
    pub left: String,
    pub right: String,
    pub source: SourceLocation,
}

/// Rule(`=>`):狀態轉換,同 stage 內依書寫順序(P18)。
/// 步驟 8 以 raw body 承載(I15-c);env/action/else 結構化屬步驟 9。
/// 結構化 Lexurgy phon block(P46 取徑 A),1:1 對映引擎 `tshiatun_dsl` 的 `RuleBlock`:
/// `Leaf` 多語句同時套用;`Then` 逐 block commit 接力(Sequential);`Else` 第一個 match 的
/// block 整組勝出、其餘不跑(FirstMatching);`Propagate` 迭代到 fixpoint。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhonBlock {
    Leaf(Vec<String>),
    Then(Vec<PhonBlock>),
    Else(Vec<PhonBlock>),
    Propagate(Box<PhonBlock>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// 穩定 ID(fossilize/generalize 的 move 對象;P25 定址靠它)。
    pub id: RuleId,
    /// 可選人類可讀標籤(P 系列取徑 B):`@name <label>` 後綴宣告,供 keyed 定址
    /// `rule["label"]`。`None` = 匿名(仍可用序數/穩定 id 定址)。
    pub name: Option<String>,
    /// P46 取徑 A(限 phon):結構化 Lexurgy block(`Then:`/`Else:` 巢狀,對映引擎
    /// `RuleBlock`)。`Some` 時 codegen/printer 以此為準,`body`/`else_chain`/`then_chain`
    /// 空置;`None` = 沿用扁平 body + 鏈(向後相容,syn/sem/prag 的 P43 Else 亦走此路)。
    pub phon_block: Option<PhonBlock>,
    /// P46 S4(限 phon,對映引擎 header modifier `name propagate:`):整條 rule
    /// **迭代到 fixpoint**。與 block **邊界** propagate(`Then propagate:` →
    /// `PhonBlock::Propagate`)正交——後者只重複它修飾的那個 block element。
    pub propagate: bool,
    /// 主分支原文(`a => ə / _#`),不含 `@stage` 與 else。
    pub body: String,
    pub stage: Stage,
    /// 規則所屬**維度**(I25/P44):由所在維度區塊決定(phon:/syn:/sem:/prag:)。
    /// phon 規則求值於 Word(dsl);syn/sem/prag 規則求值於 Sign projection(12d)。
    pub dim: Dim,
    /// `else` 鏈(P22/P43,Lexurgy `Else:`):**第一匹配 fallback**——各分支從同一輸入
    /// 依序試,第一個匹配勝出、其餘不跑(identity=匹配)。各分支為原文,共享本規則 stage。
    pub else_chain: Vec<String>,
    /// `then` 鏈(I26,Lexurgy `Then:`):**順序組合**——前分支 match/apply/commit 後,
    /// 下一分支讀更新後狀態;**全分支依序皆跑**(非條件分支)。與 `else_chain` **互斥**
    /// (平坦層不得混用;混用 = 定位錯誤,巢狀括號屬後續)。
    pub then_chain: Vec<String>,
    /// 主分支在原 `.lang` 的實體行號；Rust API 建立時為 unknown。
    pub source: SourceLocation,
    /// 與 `else_chain` 或 `then_chain` 一一對應的來源行號。
    pub branch_sources: Vec<SourceLocation>,
}

/// Trait 的 block(P27 選項 A:`==` 是 Block 節點邊界,非分隔 token)。
/// **統一 body 語法(I22)**:trait body 與 sign body 同型別(`SignItem`)——
/// belongs / slots / dimension Defs / rules 皆可,trait 只多 `==` 分 block(P27)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block {
    pub items: Vec<SignItem>,
}

// ── ③ 容器類 ──

/// Trait:**維度中立的分類節點 / macro 模板**(修補07 P38 v0.2:單一分類樹)。
/// - `global = true` = phon-rule macro,預設自動引用(P6),codegen 收入 phon 側;
/// - `marker = true` = **純分類節點**,以 `marker trait` 宣告:承諾**永不帶內容**,
///   由驗證強制。它讓「這個 trait 只是個標記」成為**契約**而不是當下的事實
///   ——差別在於改變它必須改宣告行(看得見),而不是往 body 裡塞一行(看不見);
/// - 一般 trait = 分類節點(`belongs` 建單一繼承樹)+ 可帶 dimension 內容(繼承給
///   後代,projection 解析)。`Name[n]` block-indexed macro(P5/P27)仍支援。
///   **無 `syn trait` 維度標記**(維度是內容面向,非分類樹)。
/// P76:trait 型別參數。`trait Agreement<C: Nominal, T>:` 中的 `C: Nominal` 和 `T`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitTypeParam {
    pub name: String,
    /// 上界:實例化時填入的範疇必須是此範疇的子範疇。`None` = 無約束(`[*]`)。
    pub bound: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitDef {
    pub name: String,
    pub global: bool,
    /// **純分類節點**(`marker trait`):承諾永不帶內容。見型別說明。
    pub marker: bool,
    /// P76:型別參數列表。空 = 非泛型(今天的行為)。
    pub type_params: Vec<TraitTypeParam>,
    pub blocks: Vec<Block>,
}

impl SignItem {
    /// 抹平 `SourceLocation` 的複本,供**內容**比較用(diff 分量、3-way merge)。
    ///
    /// 行號是來源註記,不是內容。多數項目型別(`FeatureDecl`/`FeatureValue`/
    /// `Sense`/`SenseEdge`/`RoleDecl`/…)帶 `SourceLocation` 且參與 `PartialEq`,
    /// 於是**在別處插入或刪除一行**就會位移其後所有項目的位置,使內容未變的
    /// sign 被判成改過——diff 會多算一個維度分量,3-way merge 會無中生有一個
    /// Content 衝突。
    ///
    /// 這個坑長期被掩蓋,因為 [`Def`] 是少數**不帶位置**的項目型別,而舊 fixture
    /// 的自造欄位幾乎都寫成 `Def`;P71 把它們遷入 `feature:` 後才浮現。
    pub fn without_source_location(&self) -> SignItem {
        let mut item = self.clone();
        let blank = SourceLocation::unknown();
        match &mut item {
            SignItem::FeatureDecl(value) => value.source = blank,
            SignItem::FeatureValue(value) => value.source = blank,
            SignItem::FeatureExpression(value) => value.source = blank,
            SignItem::SlotFeatureBinding(value) => value.source = blank,
            SignItem::RoleDecl(value) => value.source = blank,
            SignItem::RoleBinding(value) => value.source = blank,
            SignItem::RoleExpression(value) => value.source = blank,
            SignItem::Sense(value) => value.source = blank,
            SignItem::SenseEdge(value) => value.source = blank,
            SignItem::Rule(rule) | SignItem::FeatureRule(rule) => {
                rule.source = blank;
                rule.branch_sources = Vec::new();
            }
            _ => {}
        }
        item
    }
}

/// 一次 trait 掛載的三種語法形式(見 [`SignItem::TraitMount`])。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraitMountKind {
    /// `belongs X` —— **載入並確定 trait**:分類邊的來源,身分的唯一權威。
    /// 它本身**不展開內容**。
    Declaration,
    /// 裸 `X` —— 在此處展開**全部**塊(繞過 P5 的逐塊完整性)。
    Whole,
    /// `X[n]` —— 在此處展開第 n 塊(0 起算;indexed 須覆蓋全部塊,P5)。
    Block(u32),
}

impl TraitMountKind {
    /// 展開點的塊索引。**宣告沒有索引**(它不展開),裸 `X` 也沒有(它是全塊)。
    pub fn block(&self) -> Option<u32> {
        match self {
            TraitMountKind::Block(index) => Some(*index),
            TraitMountKind::Declaration | TraitMountKind::Whole => None,
        }
    }

    /// 這是不是「載入並確定 trait」的那一份。
    pub fn is_declaration(&self) -> bool {
        matches!(self, TraitMountKind::Declaration)
    }

    /// `.chg` 舊契約的 `block: Option<u32>` → 展開點。**不會產生宣告**
    /// ——宣告走 `NodeKind::Belongs`,不經這條路。
    pub fn from_block(block: Option<u32>) -> TraitMountKind {
        match block {
            Some(index) => TraitMountKind::Block(index),
            None => TraitMountKind::Whole,
        }
    }
}

/// sign 內項目:trait 引用位置有語意(P5),故與 Def/Rule 同列保序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignItem {
    /// `pass`:**這一塊是故意空的**。
    ///
    /// 空塊本身一直是合法的,問題是它**啞**——看不出是刻意留白還是寫到一半。
    /// `pass` 讓作者說出來,而沒有 `pass` 的空塊會發診斷(警告,不是錯誤)。
    ///
    /// 做成語法而不是註解,是因為 canonical printer 由 AST 印出,**註解過不了
    /// 一次 `dump()`**——而 `.chg` 的 replay、reconstruct、工作副本存檔全走
    /// canonical 形式。要活過往返的意圖就必須是語法。
    ///
    /// 與其他項目**互斥**:`pass` 和內容同時出現是錯誤(驗證擋)。
    Pass,
    /// **掛載一個 trait**:`belongs X` 與 `X[n]` 是同一件事的兩半,故是同一個項目。
    ///
    /// # 為什麼合成一個
    ///
    /// `X[n]` **不可能獨立出現**——它是展開點,指的是哪一個掛載由 `belongs`
    /// 決定;而有 `belongs` 就必須把展開點寫出來(否則內容不會進來)。兩者拆成
    /// 兩種項目時,trait 名(以及日後 P76 的實參)得在每一處重複帶一份,而
    /// 「哪一份是權威」沒有型別上的答案——`.chg` 因此可以只改展開點的目標、
    /// 留下指向舊 trait 的 `belongs`,產生一個不連貫的文件。
    ///
    /// 合成一個之後,**身分只有一個來源**([`crate::TraitMountKind::Declaration`]),
    /// 展開點只說「在這裡展開哪一塊」。
    TraitMount {
        name: String,
        kind: TraitMountKind,
        /// P76:型別實參。只有 `Declaration` 可帶實參(參數寫在 `belongs` 上);
        /// `Whole`/`Block` 時為空,展開時回查同容器的 Declaration 取實參。
        args: Vec<String>,
    },
    /// `slot NAME [Filler]`(可尾綴 `?` = optional;P41 valence=slots,I21)。
    /// 帶 ≥1 slot 的 sign = construction(P42);filler 是 syn ontology 範疇約束。
    Slot(Slot),
    /// 構式 slot 的外部介面轉換；source 語法為 `syn:` 內平坦的
    /// `map <slot> <operation> ...`，不是 token attestation。
    SlotMap(SlotMapOp),
    FeatureDecl(FeatureDecl),
    FeatureValue(FeatureValue),
    FeatureExpression(FeatureExpression),
    SlotFeatureBinding(SlotFeatureBinding),
    RoleDecl(RoleDecl),
    RoleBinding(RoleBinding),
    RoleExpression(RoleExpression),
    /// `sem:` 下 `senses:` 的一個義項(§10.3)。
    Sense(Sense),
    /// `sem:` 下 `edges:` 的一條衍生邊(§10.3)。
    SenseEdge(SenseEdge),
    Realization(Realization),
    SignExpression(SignExpression),
    Constraint(BinaryConstraint),
    /// A rule written inside `feature:`; its LHS must name a declared enum.
    FeatureRule(Rule),
    Def(Def),
    Rule(Rule),
}

/// 一個 argument slot(P41:valence 由 slots 構成,非數字欄位)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub name: String,
    /// Typed filler authorization. `AnySign` is spelled `[*]` in `.lang`.
    pub constraint: SlotConstraint,
    /// `?` 標記 = 非必填(I21;預設必填)。
    pub optional: bool,
}

/// Function-level view of the legacy slot syntax.  Keeping this as a distinct
/// public type makes the FP contract explicit without changing V1 source or
/// the mature construction machinery which still stores [`Slot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignParameter {
    pub name: String,
    pub constraint: SlotConstraint,
    pub optional: bool,
}

impl From<&Slot> for SignParameter {
    fn from(slot: &Slot) -> Self {
        Self {
            name: slot.name.clone(),
            constraint: slot.constraint.clone(),
            optional: slot.optional,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SlotConstraint {
    Category(String),
    AnySign,
}

impl SlotConstraint {
    pub fn category(&self) -> Option<&str> {
        match self {
            SlotConstraint::Category(category) => Some(category),
            SlotConstraint::AnySign => None,
        }
    }

    /// filler 的範疇集合是否獲授權。`[*]` 一律通過;具名約束委派
    /// [`crate::ontology::OntologyRegistry::categories_satisfy`]——**slot 與 role
    /// 共用同一判定**,不再各寫一套。
    pub fn is_satisfied_by(
        &self,
        categories: &[String],
        registry: &crate::ontology::OntologyRegistry,
    ) -> bool {
        match self {
            SlotConstraint::AnySign => true,
            SlotConstraint::Category(required) => registry.categories_satisfy(categories, required),
        }
    }

    pub fn display_name(&self) -> &str {
        self.category().unwrap_or("*")
    }
}

/// Construction slot 的共時外部介面轉換(P42)。這是 `.lang` 與 Rust API
/// 共用的單一資料型別；操作在消耗任何 filler 前整批驗證、原子套用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotMapOp {
    Preserve { slot: String },
    Rename { slot: String, to: String },
    AutoFill { slot: String, filler: String },
    Internalize { slot: String },
    Optional { slot: String, optional: bool },
}

/// Sign:真正的語言單位(phon=UR / sem / syn / prag 稀疏,以 Def 承載)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignDef {
    pub id: SignId,
    pub name: String,
    pub items: Vec<SignItem>,
}

impl SignDef {
    /// 內容相等:**忽略 `SourceLocation`**。見 [`SignItem::without_source_location`]。
    pub fn content_eq(&self, other: &SignDef) -> bool {
        self.name == other.name && items_content_eq(&self.items, &other.items)
    }

    /// **底層形 UR**(P1):`phon:` 區塊的 `/…/`,已去界定斜線與前後空白。
    ///
    /// 表層形永不儲存,由共時規則按需導出——故這裡回的一定是 UR。
    ///
    /// 只看**本地**項目,不走繼承投影:繼承來的 phon 是 construction 的模板
    /// (`ge{stem}t` 這類),與「這個 sign 的底層形」是兩件事。
    ///
    /// 存放處是路徑為 `phon` 的 `Def`(P71 `ENGINE_DEF_PATHS` 之一)。
    /// 這件事**只在這裡知道**——`stats` 的音素投影與 `query` 的詞典視圖都經此,
    /// 免得「UR 住哪」散成三份。
    pub fn underlying_form(&self) -> Option<&str> {
        self.items.iter().find_map(|item| match item {
            SignItem::Def(def) if def.path == "phon" => Some(def.value.trim().trim_matches('/')),
            _ => None,
        })
    }
}

impl TraitDef {
    /// 內容相等:**忽略 `SourceLocation`**。見 [`SignItem::without_source_location`]。
    pub fn content_eq(&self, other: &TraitDef) -> bool {
        self.name == other.name
            && self.global == other.global
            && self.marker == other.marker
            && self.type_params == other.type_params
            && self.blocks.len() == other.blocks.len()
            && self
                .blocks
                .iter()
                .zip(&other.blocks)
                .all(|(a, b)| items_content_eq(&a.items, &b.items))
    }
}

fn items_content_eq(left: &[SignItem], right: &[SignItem]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.without_source_location() == b.without_source_location())
}

// ── 根(P28 canonical empty)──

/// Language 根節點:**永遠存在**(等同 MLIR `builtin.module`),
/// `Language::new()` = canonical empty Language,四原語(步驟 13)有處掛靠。
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Language {
    /// dsl 域宣告區(Lexurgy 形,不透明 verbatim 行;I15-a)。
    pub dsl_decls: Vec<String>,
    /// ⑤ 分佈覆寫(E 的覆寫層,稀疏;鍵→值,印出時按鍵排序)。
    pub distribution: Vec<(String, String)>,
    /// ③ trait 容器(含 global;印出時按名排序,I15-d)。
    pub traits: Vec<TraitDef>,
    /// ③ sign 容器(印出時按名排序)。
    pub signs: Vec<SignDef>,
    rule_namespace: RuleNamespace,
    next_rule: u32,
    next_sign: u32,
}

impl std::fmt::Debug for Language {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = formatter.debug_struct("Language");
        debug
            .field("dsl_decls", &self.dsl_decls)
            .field("distribution", &self.distribution)
            .field("traits", &self.traits)
            .field("signs", &self.signs)
            .field("rule_namespace", &self.rule_namespace)
            .field("next_rule", &self.next_rule)
            .field("next_sign", &self.next_sign)
            .finish()
    }
}

impl Language {
    /// canonical empty Language(P28)。
    pub fn new() -> Language {
        Language::default()
    }

    /// 決定性 RuleId 配發(P26:namespace 內純序列)。
    pub fn fresh_rule_id(&mut self) -> RuleId {
        let namespace = match &self.rule_namespace {
            RuleNamespace::Local => IdentityNamespace::Ephemeral,
            RuleNamespace::Document(namespace) => IdentityNamespace::Document(namespace.clone()),
            RuleNamespace::Package(namespace) => IdentityNamespace::Library(namespace.clone()),
        };
        let id = RuleId(NodeId::new(namespace, self.next_rule.into()));
        self.next_rule += 1;
        id
    }

    /// Bind already-parsed source to a package-owned namespace.
    ///
    /// Kept crate-private so raw `.lang` source cannot spoof package ownership;
    /// the std/package loader validates its config before calling this method.
    pub(crate) fn bind_rule_namespace(&mut self, namespace: RuleNamespace) {
        fn bind_items(items: &mut [SignItem], namespace: &RuleNamespace) {
            for item in items {
                if let SignItem::Rule(rule) | SignItem::FeatureRule(rule) = item {
                    rule.id.0.namespace = match namespace {
                        RuleNamespace::Local => IdentityNamespace::Ephemeral,
                        RuleNamespace::Document(namespace) => {
                            IdentityNamespace::Document(namespace.clone())
                        }
                        RuleNamespace::Package(namespace) => {
                            IdentityNamespace::Library(namespace.clone())
                        }
                    };
                }
            }
        }

        for trait_def in &mut self.traits {
            for block in &mut trait_def.blocks {
                bind_items(&mut block.items, &namespace);
            }
        }
        for sign in &mut self.signs {
            bind_items(&mut sign.items, &namespace);
            if let RuleNamespace::Package(package) = &namespace {
                sign.id = SignId::package(package.clone(), sign.id.0.ordinal);
                if !sign
                    .items
                    .iter()
                    .any(|item| matches!(item, SignItem::Def(def) if def.path == "source_package"))
                {
                    sign.items.push(SignItem::Def(Def {
                        path: "source_package".to_owned(),
                        value: package.clone(),
                    }));
                }
            }
        }
        self.rule_namespace = namespace;
    }

    /// Merge parsed package source into an executable language. Package-owned
    /// RuleIds retain their bound namespaces while SignIds are made unique in
    /// the combined runtime view.
    pub(crate) fn append_library(&mut self, mut other: Language) {
        self.dsl_decls.append(&mut other.dsl_decls);
        self.distribution.append(&mut other.distribution);
        self.traits.append(&mut other.traits);
        for sign in other.signs.drain(..) {
            self.signs.push(sign);
        }
    }

    /// 決定性 SignId 配發(P26)。
    pub fn fresh_sign_id(&mut self) -> SignId {
        let id = SignId::local(self.next_sign.into());
        self.next_sign += 1;
        id
    }

    /// 建規則(id 自動配發;預設 phon 維——向後相容既有 phon 規則與測試)。
    pub fn rule(&mut self, body: impl Into<String>, stage: Stage) -> Rule {
        self.rule_dim(body, stage, Dim::Phon)
    }

    /// 建規則並指定維度(I25/P44)。
    pub fn rule_dim(&mut self, body: impl Into<String>, stage: Stage, dim: Dim) -> Rule {
        Rule {
            id: self.fresh_rule_id(),
            name: None,
            phon_block: None,
            propagate: false,
            body: body.into(),
            stage,
            dim,
            else_chain: Vec::new(),
            then_chain: Vec::new(),
            source: SourceLocation::unknown(),
            branch_sources: Vec::new(),
        }
    }

    /// 解析 canonical(或使用者)`.lang` 原文(步驟 9);round-trip:
    /// `Language::parse(src)?.dump()` 對 canonical 輸入恆等(P21)。
    pub fn parse(src: &str) -> Result<Language, parser::ParseError> {
        parser::parse(src)
    }

    /// 建 sign(id 自動配發)並加入容器。
    pub fn add_sign(&mut self, name: impl Into<String>, items: Vec<SignItem>) -> SignId {
        let id = self.fresh_sign_id();
        self.signs.push(SignDef {
            id: id.clone(),
            name: name.into(),
            items,
        });
        id
    }

    pub fn add_trait(&mut self, t: TraitDef) {
        self.traits.push(t);
    }

    /// IR dump = canonical form(P21)。
    pub fn dump(&self) -> String {
        printer::print(self)
    }

    pub fn trait_named(&self, name: &str) -> Option<&TraitDef> {
        self.traits.iter().find(|t| t.name == name)
    }
    pub fn sign_named(&self, name: &str) -> Option<&SignDef> {
        self.signs.iter().find(|s| s.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P28:canonical empty root 存在且印出為空(無內容即無話可說)。
    #[test]
    fn canonical_empty_root_p28() {
        let l = Language::new();
        assert_eq!(l.dump(), "");
        assert_eq!(l, Language::default());
    }

    /// P26/I15-b:同構造序列 → 相同 id;id 純序列。
    #[test]
    fn deterministic_sequential_ids_p26() {
        let mk = || {
            let mut l = Language::new();
            let r1 = l.rule("a => b", Stage::Word);
            let s1 = l.add_sign("go", vec![SignItem::Rule(r1)]);
            let r2 = l.rule("b => c", Stage::Stem);
            (l, s1, r2.id)
        };
        let (l1, s1, r2) = mk();
        let (l2, s1b, r2b) = mk();
        assert_eq!(l1, l2);
        assert_eq!((s1.clone(), r2.clone()), (s1b, r2b));
        assert_eq!(s1, SignId::local(0));
        assert_eq!(r2, RuleId::local(1));
    }

    #[test]
    fn package_rule_namespace_keeps_local_ordinals_distinct() {
        let local = RuleId::local(0);
        let mut language = Language::new();
        language.bind_rule_namespace(RuleNamespace::Package("std:fixture".to_owned()));
        let package = language.rule("x => y", Stage::Word).id;

        assert_eq!(package, RuleId::package("std:fixture", 0));
        assert_ne!(local, package);
        assert_eq!(package.to_string(), "std:fixture:0");
    }
}
