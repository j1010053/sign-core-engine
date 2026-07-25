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
pub use library::{
    LibraryCatalog, LibraryExport, LibraryExportKind, LibraryId, LibraryKind, LibraryLoadError,
    LibraryPackage, LibrarySpec,
};
pub use metadata::{SignLifecycle, SignProvenance};
pub use semantic_dto::{
    SemanticDocumentError, SemanticDocumentV1, SemanticNodeV1, SemanticSourceV1, SEMANTIC_SCHEMA_V1,
};
pub use stdlib::{StdExport, StdExportKind, StdLoadError, StdPackage};
pub use system::{
    check_document, check_language, check_language_with_libraries, compile_document,
    compile_system, compile_system_ref, compile_with_libraries, compile_with_libraries_ref,
    CandidateSelectionTrace, CandidateSelector, CandidateSet, CaseBranchStatus, CaseRecord,
    CompileSystemError, CompiledSystem, ConstructionCandidate, DerivationContext, PhonRealization,
    RealizedPhonInput, SignExpressionEvaluation, SignValue, SystemDerivation, SystemError,
};
pub use tshiatun_dsl::lower::Stage;

// ── 共時四維(修補07 P38 v0.2;單一分類樹、四個內容面向)──

/// 四個內容彼此獨立的共時維度。分類只有一棵維度中立的 ontology；
/// 正交性由 projection、validation、patch、diff 與 rule write-set 保證。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    pub source: SourceLocation,
}

/// A typed feature value. Unlike a generic [`Def`], this is only valid when a
/// matching [`FeatureDecl`] is visible on the effective sign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureValue {
    pub dim: Dim,
    pub name: String,
    pub value: String,
    pub source: SourceLocation,
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
    pub constraint: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealizationBranch {
    /// Complete `/.../` phon template.
    pub template: String,
    /// Read-only guard over `$self` and frozen `$slot`; `None` is `else`.
    pub guard: Option<String>,
    pub source: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Realization {
    pub branches: Vec<RealizationBranch>,
    /// V2 context-typed expression.  Legacy flat branches remain available so
    /// a V1 document can be parsed and dumped byte-for-byte compatibly.
    pub expression: Option<TypedCase>,
}

/// 舊 v2 schema 標頭。**v1 已淘汰、v2 為唯一模型**(2026-07-24 硬移除):FP 層永遠
/// 可用,不再需要標頭選版。為 back-compat,parser 仍**接受並忽略**此行(printer 不再
/// 輸出);它不影響解析、canonical dump 或 identity digest。
pub const LEGACY_V2_HEADER: &str = "schema conlang.lang/v2";

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// 穩定 ID(fossilize/generalize 的 move 對象;P25 定址靠它)。
    pub id: RuleId,
    /// 可選人類可讀標籤(P 系列取徑 B):`@name <label>` 後綴宣告,供 keyed 定址
    /// `rule["label"]`。`None` = 匿名(仍可用序數/穩定 id 定址)。
    pub name: Option<String>,
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
/// - 一般 trait = 分類節點(`belongs` 建單一繼承樹)+ 可帶 dimension 內容(繼承給
///   後代,projection 解析)。`Name[n]` block-indexed macro(P5/P27)仍支援。
///   **無 `syn trait` 維度標記**(維度是內容面向,非分類樹)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitDef {
    pub name: String,
    pub global: bool,
    pub blocks: Vec<Block>,
}

/// sign 內項目:trait 引用位置有語意(P5),故與 Def/Rule 同列保序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignItem {
    /// trait macro 引用(P5/P27)。`block`:**0 起算**——
    /// - `Some(n)` = `Name[n]`,只引用第 n 個 block(indexed 須覆蓋全部 block,P5);
    /// - `None` = **整個 trait**(裸 `Name` 或 `Name[]`,全 block 依序 inline)。
    TraitUse {
        name: String,
        block: Option<u32>,
    },
    /// `belongs Transitive`(P40):sign 掛入某 ontology 節點;閉包由 registry 走。
    Belongs(String),
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

// ── 根(P28 canonical empty)──

/// Language 根節點:**永遠存在**(等同 MLIR `builtin.module`),
/// `Language::new()` = canonical empty Language,四原語(步驟 13)有處掛靠。
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Language {
    /// dsl 域宣告區(Lexurgy 形,不透明 verbatim 行;I15-a)。
    pub dsl_decls: Vec<String>,
    /// ① `prosody = μ σ Ft ω φ ι U`(七層鏈;空 = 未宣告)。
    pub prosody: Vec<String>,
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
            .field("prosody", &self.prosody)
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
        self.prosody.append(&mut other.prosody);
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
