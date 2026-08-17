//! 步驟 15 層② —— **Atomic Rewrite**(P16 定案 12 項)。
//!
//! 契約(《總鳥瞰》line 157/222):`(rewrite, Language) → Vec<PrimitiveEdit>`,
//! **純展開、不執行**。展開讀 Language 當前狀態,回傳一串四原語;要不要套用、
//! 何時套用由呼叫端決定(`crate::apply_edit`)。
//!
//! **封閉內建集**:這 12 項是內建的,使用者/plugin **不得自行新增**——可自行撰寫的
//! 層級是 Recipe/Goal(步驟 16–17)。本模組只提供 Rust API;`.chg` 的
//! `rewrite <name>(…)` 呼叫面是 15c。
//!
//! 展開鐵律(承 CLAUDE.md §3-2「具名動詞一律由原語組合」):
//! **優先「一次完整 Insert」而非「先插空殼再填」**(沿用 `clone` 既有先例),
//! 如此展開序列中不需要引用「尚未存在的節點」,可純讀當前狀態算出。

use crate::{
    parse_selector, resolve_selector, Anchor, DetachedNode, NodeUpdate, PrimitiveEdit, ReplayError,
};
use std::collections::BTreeMap;

use conlang_language::{
    Def, DerivationKind, LanguageDocument, NodeKind, NodeRef, Sense, SenseEdge, SenseTransparency,
    SignDef, SignItem, SignProvenance, SignRef, SourceLocation,
};

/// `reanalyze{target}` 的重分析對象(P16)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReanalysisTarget {
    Valence,
    Category,
    Slot,
    /// 成分邊界重新切分——**尚未支援**(見 [`RewriteError::Unsupported`])。
    Boundary,
}

impl ReanalysisTarget {
    /// 這一種重分析**還不能做**時的說明;`None` 代表可以做。
    ///
    /// ## 誌誤:三個 target 原本都寫「沒有人認的欄位」
    ///
    /// 早期實作把三者一律降階為 `upsert_def(sign, "syn.<target>", to)`。那條路徑
    /// 走的是 `Def` 驗證裡**唯一不檢查內容**的一支(任意 `<dim>.<field>` 放行),
    /// 於是:欄位名沒人讀、值沒有值域、寫什麼都算數。
    ///
    /// 以 `category` 為例,實測 `reanalyze(sign("go"), target: category, to: aux)`
    /// 之後 `belongs` 仍是 `MotionVerb`、`category_is_a(MotionVerb, Verb)` 仍為
    /// `true`——**語法化最核心的動作是空操作**。而 `std/core/code/ontology.lang`
    /// 開宗明義:「category membership—not a mutable `syn.class`」,可變的範疇欄位
    /// 正是該設計拒絕的東西。
    ///
    /// 現在 `Category` 改為搬動 `belongs`(見 `expand`)。`Valence`/`Slot` 沒有對應
    /// 的既有承載處——valence 該是宣告過的 feature、slot 該動 `SignItem::Slot`
    /// ——在裁定前**顯式拒絕**,不再寫死欄位。
    fn unsupported(self) -> Option<&'static str> {
        match self {
            ReanalysisTarget::Category => None,
            ReanalysisTarget::Valence => Some(
                "reanalyze{target: Valence} needs a declared syn feature; \
                 writing an unchecked `syn.valence` def was a no-op",
            ),
            ReanalysisTarget::Slot => Some(
                "reanalyze{target: Slot} needs to move the sign's Slot items; \
                 writing an unchecked `syn.slot` def was a no-op",
            ),
            ReanalysisTarget::Boundary => {
                Some("reanalyze{target: Boundary} needs constituent re-segmentation")
            }
        }
    }
}

/// `adopt{source}` 的來源(P16)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdoptSource {
    Loan,
    Dialect,
    Ancestor,
}

impl AdoptSource {
    fn provenance(self) -> SignProvenance {
        match self {
            // 借詞;方言/祖語輸入在本體上仍是「非原生」,以 Loan 記錄來源性質,
            // 細分留待 provenance 本體擴充(不自行發明新 variant)。
            AdoptSource::Loan | AdoptSource::Dialect | AdoptSource::Ancestor => {
                SignProvenance::Loan
            }
        }
    }
}

/// 規則居所階梯的一級(P14 Life Cycle 軸)。`fossilize` 往下、`generalize` 往上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleHome {
    /// `global trait` —— 最上層,預設自動引用。
    Global(String),
    Trait(String),
    Sign(String),
}

impl RuleHome {
    /// 規則的**容器**節點:trait 的規則住在它的 block(`==` 分隔的那一層),
    /// 不是 trait 節點本身;sign 的規則直接掛在 sign 底下。
    fn selector(&self) -> String {
        match self {
            RuleHome::Global(name) | RuleHome::Trait(name) => format!("trait({name:?}).block[0]"),
            RuleHome::Sign(name) => format!("sign({name:?})"),
        }
    }
    /// 階梯高度:數字越小越「上層」(generalize 提高、fossilize 降低)。
    fn rank(&self) -> u8 {
        match self {
            RuleHome::Global(_) => 0,
            RuleHome::Trait(_) => 1,
            RuleHome::Sign(_) => 2,
        }
    }
}

/// **封閉內建集**(P16 §3.1 定案 12 列)。使用者/plugin 不得新增項目。
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum AtomicRewrite {
    // ── form ──
    /// 音變:把一條 phon 規則加進某個居所。
    SoundChange { home: RuleHome, body: String },

    // ── sem ──
    /// 語意漂移:改某義項的內容。**新值由呼叫端給**——量化屬 SemanticBackend,
    /// 本層不自行計算(P16「量化交 SemanticBackend」)。
    Drift {
        sign: String,
        sense: String,
        gloss: String,
    },
    /// 衍生新義項:加義項 + 加衍生邊(隱喻/換喻/窄化/寬化)。
    DeriveSense {
        sign: String,
        from: String,
        name: String,
        gloss: String,
        kind: DerivationKind,
    },
    /// 衍生邊固化:語源關係不再透明。
    LexicalizeSense { sign: String, edge: usize },

    // ── syn ──
    /// 重新分析:改 valence / category / slot。
    Reanalyze {
        sign: String,
        target: ReanalysisTarget,
        /// 多 belongs 時指定要換掉的那一條;單 belongs 時可省略。
        from: Option<String>,
        to: String,
    },

    // ── usage ──
    /// 固著度 +δ。
    Entrench { sign: String, delta: f64 },
    /// 固著度 −δ。
    Attrit { sign: String, delta: f64 },
    /// token 跨閾值固化為 type(**`lexicalize` 專指此義**,P16 三義消歧)。
    Lexicalize { sign: SignDef },

    // ── 結構 ──
    /// 生。
    Create { sign: SignDef },
    /// 滅(selector 定址,可為 sign 或 sign 內的節點)。
    Delete { selector: String },
    /// 分裂:A → A + A′,指名義項搬到新 sign,`origin` 指回來源。
    Split {
        sign: String,
        new_name: String,
        senses: Vec<String>,
    },
    /// 範疇塌縮(syncretism):A,B → A。B 的義項/邊搬到 A,然後刪 B。
    Merge { into: String, from: String },
    /// 線性融合:A+B → C(`au = à + le`)。建新 sign,不刪來源。
    Fuse {
        left: String,
        right: String,
        name: String,
        gloss: String,
    },

    // ── 接觸 ──
    /// 借入:把 donor 的某個 sign 複製進來。
    ///
    /// **v0.3 前是 `sign: SignDef`**——呼叫端得先自己去 donor 那邊把整個 sign 取出來
    /// 遞進來,「怎麼挑」整個掉在引擎外面(不被記錄、不被測試、不能 replay)。現在改為
    /// **指名**:`donor` 是 prelude 宣告的別名,`sign` 是它裡面的名字,選取在展開時發生。
    ///
    /// 借來的 sign **拿到新 id**(展開成一個 `Insert` 原語)——§6.8:借詞是「造了一個
    /// 新詞」,不是 donor 那個詞跑過來繼續演化。
    Adopt {
        donor: String,
        sign: String,
        source: AdoptSource,
    },

    // ── 居所(P14 Life Cycle 軸)──
    /// 規則往下層搬(global → trait → sign)。
    Fossilize { rule: String, to: RuleHome },
    /// 規則往上層搬(sign → trait → global)。
    Generalize { rule: String, to: RuleHome },
}

/// **P53:外部服務接點**(形狀先定,實作留空)。
///
/// P30 定 SemanticBackend **必後端**(`drift` 的新語意值本該由它算,目前由呼叫端傳);
/// P33 要求「允許語句中途暫停、commit 仍整句原子」;P34 定「首次執行呼叫服務並記入
/// History,**replay 一律讀 History 不重呼叫**」。
///
/// 現階段是**只讀 History、無 live 呼叫**的空殼:`lookup` 永遠回 `None`,故所有
/// rewrite 的行為與此前完全相同。先開好參數是為了避免日後一次改 12 個簽名與所有
/// 既有呼叫端(含已寫好的 recipe)。
#[derive(Debug, Clone, Default)]
pub struct ServiceContext {
    /// 首次執行時記錄下來的外部服務結果(P34 History 側表)。
    /// 鍵 = 語句序號 + 呼叫序;replay 時只讀這裡,不重新呼叫服務。
    recorded: Vec<(String, String)>,
}

impl ServiceContext {
    /// 不接任何服務的空殼——現階段所有呼叫端用這個。
    pub fn offline() -> ServiceContext {
        ServiceContext::default()
    }

    /// 由 History 側表重建(P34 replay 路徑)。
    pub fn from_history(recorded: Vec<(String, String)>) -> ServiceContext {
        ServiceContext { recorded }
    }

    /// 查一筆已記錄的服務結果。**目前無 live 呼叫**:查不到就是 `None`,
    /// 由呼叫端決定要報錯還是走預設值(如 `drift` 目前取呼叫端給的 gloss)。
    pub fn lookup(&self, key: &str) -> Option<&str> {
        self.recorded
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    #[error("REWRITE_ADDRESS: {0}")]
    Address(#[from] ReplayError),
    /// 展開目標存在但型別不對(例如把 `fossilize` 指到非規則節點)。
    #[error("REWRITE_TARGET: {0}")]
    Target(String),
    /// 這一項的某個參數組合尚未支援——**顯式拒絕,不默默近似**。
    #[error("REWRITE_UNSUPPORTED: {0}")]
    Unsupported(String),
    /// 條目引用了 prelude 沒宣告的 donor 別名(P63 §7.3 的第一道硬錯)。
    #[error("REWRITE_UNDECLARED_DONOR: {0} is not declared in the prelude")]
    UndeclaredDonor(String),
    /// donor 裡沒有這個 sign。**硬錯而非略過**——指名借入卻借不到,是作者的錯誤,
    /// 不是可以無聲跳過的情形。
    #[error("REWRITE_DONOR_SIGN_NOT_FOUND: {donor} has no sign {sign:?}")]
    DonorSignNotFound { donor: String, sign: String },
}

/// 展開時**讀得到哪些別的語言**(P62/P63)。
///
/// 鍵是**檔案內的別名**——`.chg` 的條目按別名引用(`fr.sign("eau")`),而別名到
/// node-id 的對應住在 prelude 的 `donor` 行。範圍在建構時就決定
/// (parents ∪ 宣告),故這裡取不到的東西就是範圍外的。
#[derive(Debug, Default)]
pub struct DonorScope<'a> {
    by_alias: BTreeMap<&'a str, &'a LanguageDocument>,
}

impl<'a> DonorScope<'a> {
    pub fn new() -> DonorScope<'a> {
        DonorScope {
            by_alias: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, alias: &'a str, document: &'a LanguageDocument) {
        self.by_alias.insert(alias, document);
    }

    pub fn get(&self, alias: &str) -> Option<&'a LanguageDocument> {
        self.by_alias.get(alias).copied()
    }
}

/// 展開一個 Atomic Rewrite 為四原語序列。**不執行**。
pub fn expand(
    rewrite: &AtomicRewrite,
    document: &LanguageDocument,
    services: &ServiceContext,
    donors: &DonorScope<'_>,
) -> Result<Vec<PrimitiveEdit>, RewriteError> {
    let _ = services; // P53:接點已開,SemanticBackend 等接上時在此消費。
    match rewrite {
        AtomicRewrite::SoundChange { home, body } => {
            let parent = node(document, &home.selector())?;
            Ok(vec![insert_item(
                parent,
                SignItem::Rule(phon_rule(document, body)),
            )])
        }

        AtomicRewrite::Drift { sign, sense, gloss } => {
            let target = node(document, &format!("sign({sign:?}).sense[{sense:?}]"))?;
            Ok(vec![PrimitiveEdit::Update {
                node: target,
                change: NodeUpdate::SenseGloss(gloss.clone()),
            }])
        }

        AtomicRewrite::DeriveSense {
            sign,
            from,
            name,
            gloss,
            kind,
        } => {
            // 來源義項必須存在,否則衍生邊會指向幽靈節點。
            node(document, &format!("sign({sign:?}).sense[{from:?}]"))?;
            let parent = node(document, &format!("sign({sign:?})"))?;
            Ok(vec![
                insert_item(
                    parent.clone(),
                    SignItem::Sense(Sense {
                        name: name.clone(),
                        gloss: gloss.clone(),
                        source: SourceLocation::unknown(),
                    }),
                ),
                insert_item(
                    parent,
                    SignItem::SenseEdge(SenseEdge {
                        to: name.clone(),
                        from: from.clone(),
                        kind: *kind,
                        transparency: SenseTransparency::Transparent,
                        source: SourceLocation::unknown(),
                    }),
                ),
            ])
        }

        // **寫入後目前無消費者**(已知延後,非缺陷):翻成 `Opaque` 之後,全庫沒有
        // 任何語意分支因此改變行為(見 `SenseTransparency` 的 doc comment)。
        // 語意效果待《測試案例集總索引》實例 7「語用隱喻固化」落地,屆時與折磨 6
        // (火車)的 component transparency 共用同一欄位。
        AtomicRewrite::LexicalizeSense { sign, edge } => {
            let target = node(document, &format!("sign({sign:?}).edge[{edge}]"))?;
            Ok(vec![PrimitiveEdit::Update {
                node: target,
                change: NodeUpdate::SenseEdgeTransparency(SenseTransparency::Opaque),
            }])
        }

        AtomicRewrite::Reanalyze {
            sign,
            target,
            from,
            to,
        } => {
            if let Some(reason) = target.unsupported() {
                return Err(RewriteError::Unsupported(reason.to_owned()));
            }
            reanalyze_category(document, sign, from.as_deref(), to)
        }

        AtomicRewrite::Entrench { sign, delta } => entrenchment_edit(document, sign, *delta),
        AtomicRewrite::Attrit { sign, delta } => entrenchment_edit(document, sign, -*delta),

        // token → type,以及生/借入:都是「一次完整 Insert」。
        AtomicRewrite::Lexicalize { sign } | AtomicRewrite::Create { sign } => {
            Ok(vec![insert_sign(document, sign.clone())?])
        }
        AtomicRewrite::Adopt {
            donor,
            sign,
            source,
        } => {
            let language = donors
                .get(donor)
                .ok_or_else(|| RewriteError::UndeclaredDonor(donor.clone()))?;
            let borrowed = language
                .language()
                .signs
                .iter()
                .find(|candidate| &candidate.name == sign)
                .ok_or_else(|| RewriteError::DonorSignNotFound {
                    donor: donor.clone(),
                    sign: sign.clone(),
                })?;
            Ok(vec![insert_sign(
                document,
                borrowed.clone().with_provenance(source.provenance()),
            )?])
        }

        AtomicRewrite::Delete { selector } => Ok(vec![PrimitiveEdit::Delete {
            node: node(document, selector)?,
        }]),

        AtomicRewrite::Split {
            sign,
            new_name,
            senses,
        } => expand_split(document, sign, new_name, senses),

        AtomicRewrite::Merge { into, from } => expand_merge(document, into, from),

        AtomicRewrite::Fuse {
            left,
            right,
            name,
            gloss,
        } => {
            // 兩個來源都必須存在(融合的成分)。
            node(document, &format!("sign({left:?})"))?;
            node(document, &format!("sign({right:?})"))?;
            // P54:記錄**兩個成分**(《修補05》§4.3 的「component 引用」)。
            // `origin` 是單一來源、`components` 是線性組合的各成分——au = à + le
            // 兩者都必須留下,否則 `fuse(a,b)` 與 `fuse(a,c)` 會產出相同結果。
            let fused = SignDef {
                id: conlang_language::SignId::synthetic(),
                name: name.clone(),
                items: vec![SignItem::Sense(Sense {
                    name: "core".to_owned(),
                    gloss: gloss.clone(),
                    source: SourceLocation::unknown(),
                })],
            }
            .with_provenance(SignProvenance::Derived)
            .with_origin(SignRef(left.clone()))
            .with_components(&[SignRef(left.clone()), SignRef(right.clone())]);
            Ok(vec![insert_sign(document, fused)?])
        }

        AtomicRewrite::Fossilize { rule, to } => expand_move_rule(document, rule, to, true),
        AtomicRewrite::Generalize { rule, to } => expand_move_rule(document, rule, to, false),
    }
}

// ── 展開輔助 ──────────────────────────────────────────────────────────────

/// 解析一個 selector 字串為穩定節點(複用 `.chg` 既有定址,不另造一套)。
fn node(document: &LanguageDocument, selector: &str) -> Result<NodeRef, RewriteError> {
    let parsed = parse_selector(selector)?;
    Ok(resolve_selector(&parsed, document)?)
}

fn insert_item(parent: NodeRef, item: SignItem) -> PrimitiveEdit {
    PrimitiveEdit::Insert {
        parent,
        anchor: Anchor::End,
        subtree: DetachedNode::Item(item),
    }
}

fn insert_sign(
    document: &LanguageDocument,
    mut sign: SignDef,
) -> Result<PrimitiveEdit, RewriteError> {
    // 插入的子樹必須已是 **canonical 順序**:identity 依插入時的順序配址,而
    // `finish_edit` 會拿 canonical dump 重新比對形狀,順序不符會 ShapeMismatch。
    // (`with_provenance`/`with_origin` 是 push 到尾端的,故此處統一重排。)
    sign.items.sort_by_key(crate::item_group);
    Ok(PrimitiveEdit::Insert {
        parent: document.root_ref(),
        anchor: Anchor::End,
        subtree: DetachedNode::Sign(sign),
    })
}

/// 一條 phon 規則(規則本體為原文,交給引擎解析)。
fn phon_rule(document: &LanguageDocument, body: &str) -> conlang_language::Rule {
    let mut rule = document.language().clone().rule_dim(
        body.to_owned(),
        conlang_language::Stage::Word,
        conlang_language::Dim::Phon,
    );
    rule.source = SourceLocation::unknown();
    rule
}

/// `reanalyze{target: Category}` —— **搬動 `belongs`**。
///
/// 範疇在本系統是**本體樹的成員關係**(`std/core/code/ontology.lang`:
/// 「category membership—not a mutable `syn.class`」),而 `category_is_a` 與參數
/// 約束 `[Verb]` 讀的也是它。所以「重新分析成助動詞」就是把 `belongs` 換掉;
/// 換完之後 `[Verb]` 約束會**真的**不再成立,這正是語法化該有的可觀測後果。
///
/// `from` 指定要換掉的那一條 `belongs`。單 belongs 時可省略(自動推斷);
/// 多 belongs 時必須給(不猜,猜錯會靜默給出錯的範疇)。
/// 一個都沒有時拒絕——沒有範疇可搬。
fn reanalyze_category(
    document: &LanguageDocument,
    sign: &str,
    from: Option<&str>,
    to: &str,
) -> Result<Vec<PrimitiveEdit>, RewriteError> {
    let definition = document
        .language()
        .signs
        .iter()
        .find(|candidate| candidate.name == sign)
        .ok_or_else(|| RewriteError::Target(format!("unknown sign {sign:?}")))?;
    let belongs: Vec<&str> = definition
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::TraitMount { name: name, kind: conlang_language::TraitMountKind::Declaration } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if belongs.is_empty() {
        return Err(RewriteError::Unsupported(format!(
            "reanalyze{{target: Category}} needs at least one `belongs` on sign {sign:?}, found 0"
        )));
    }
    let current = match from {
        Some(f) => {
            if !belongs.contains(&f) {
                return Err(RewriteError::Target(format!(
                    "sign {sign:?} does not `belongs {f}` (has: {})",
                    belongs.join(", ")
                )));
            }
            f.to_owned()
        }
        None => {
            if belongs.len() != 1 {
                return Err(RewriteError::Unsupported(format!(
                    "reanalyze{{target: Category}} on sign {sign:?} has {} `belongs` — \
                     specify `from:` to pick which one to replace (has: {})",
                    belongs.len(),
                    belongs.join(", ")
                )));
            }
            belongs[0].to_owned()
        }
    };
    let target = node(document, &format!("sign({sign:?}).belongs[{current:?}]"))?;
    Ok(vec![PrimitiveEdit::Update {
        node: target,
        change: NodeUpdate::Belongs(to.to_owned()),
    }])
}

/// upsert 一個 Def:已存在就 Update,否則 Insert。
fn upsert_def(
    document: &LanguageDocument,
    sign: &str,
    path: &str,
    value: &str,
) -> Result<PrimitiveEdit, RewriteError> {
    if let Ok(existing) = node(document, &format!("sign({sign:?}).def[{path}]")) {
        return Ok(PrimitiveEdit::Update {
            node: existing,
            change: NodeUpdate::DefinitionValue(value.to_owned()),
        });
    }
    let parent = node(document, &format!("sign({sign:?})"))?;
    Ok(insert_item(
        parent,
        SignItem::Def(Def {
            path: path.to_owned(),
            value: value.to_owned(),
        }),
    ))
}

/// 固著度 ±δ:讀當前值、夾在 [0, ∞),再 upsert。
fn entrenchment_edit(
    document: &LanguageDocument,
    sign: &str,
    delta: f64,
) -> Result<Vec<PrimitiveEdit>, RewriteError> {
    let definition = document
        .language()
        .signs
        .iter()
        .find(|candidate| candidate.name == sign)
        .ok_or_else(|| RewriteError::Target(format!("unknown sign {sign:?}")))?;
    let current = definition.entrenchment().unwrap_or(0.0);
    let next = (current + delta).max(0.0);
    Ok(vec![upsert_def(
        document,
        sign,
        "entrenchment",
        &next.to_string(),
    )?])
}

/// `split`:新 sign 帶走指名義項(+ 兩端都在搬移集合內的衍生邊),來源刪掉它們。
///
/// **與《修補05》§4.3 草案的刻意分歧**(語意等價):該表寫
/// `insert + move + update(refs)`;此處是「insert(完整新 sign,含義項與 origin)
/// 後接 delete(來源義項)」。理由是「一次完整 Insert」鐵律——避免展開序列引用
/// 尚未存在的節點。淨效果相同(義項搬到新 sign、來源失去它、origin 指回),
/// 且仍封閉於四原語(§4.3 的用意是證明封閉性,非規定確切序列)。
fn expand_split(
    document: &LanguageDocument,
    sign: &str,
    new_name: &str,
    senses: &[String],
) -> Result<Vec<PrimitiveEdit>, RewriteError> {
    if senses.is_empty() {
        return Err(RewriteError::Target(
            "split needs at least one sense to move".to_owned(),
        ));
    }
    let source = document
        .language()
        .signs
        .iter()
        .find(|candidate| candidate.name == sign)
        .ok_or_else(|| RewriteError::Target(format!("unknown sign {sign:?}")))?;

    let moving: Vec<&Sense> = source
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::Sense(value) if senses.contains(&value.name) => Some(value),
            _ => None,
        })
        .collect();
    if moving.len() != senses.len() {
        return Err(RewriteError::Target(format!(
            "sign {sign:?} does not declare every sense in {senses:?}"
        )));
    }

    // 兩端都在搬移集合 → 一起搬;只有一端在 → 邊會懸空,**顯式拒絕**。
    let mut moving_edges = Vec::new();
    for (index, edge) in source
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::SenseEdge(edge) => Some(edge),
            _ => None,
        })
        .enumerate()
    {
        let to_moves = senses.contains(&edge.to);
        let from_moves = senses.contains(&edge.from);
        match (to_moves, from_moves) {
            (true, true) => moving_edges.push((index, edge)),
            (false, false) => {}
            _ => {
                return Err(RewriteError::Unsupported(format!(
                    "split would strand the derivation edge {:?} from {:?}",
                    edge.to, edge.from
                )))
            }
        }
    }

    // 一次完整 Insert:新 sign 直接帶著義項/邊與 origin。
    let mut items: Vec<SignItem> = moving
        .iter()
        .map(|sense| SignItem::Sense((*sense).clone()))
        .collect();
    items.extend(
        moving_edges
            .iter()
            .map(|(_, edge)| SignItem::SenseEdge((*edge).clone())),
    );
    let derived = SignDef {
        id: conlang_language::SignId::synthetic(),
        name: new_name.to_owned(),
        items,
    }
    .with_provenance(SignProvenance::Derived)
    .with_origin(SignRef(sign.to_owned()));

    let mut edits = vec![insert_sign(document, derived)?];
    // 先刪邊再刪義項:否則中途狀態會有指向已刪義項的邊(參照完整性)。
    for (index, _) in &moving_edges {
        edits.push(PrimitiveEdit::Delete {
            node: node(document, &format!("sign({sign:?}).edge[{index}]"))?,
        });
    }
    for sense in senses {
        edits.push(PrimitiveEdit::Delete {
            node: node(document, &format!("sign({sign:?}).sense[{sense:?}]"))?,
        });
    }
    Ok(edits)
}

/// `merge`:B 的義項/邊搬進 A,然後刪 B(範疇塌縮)。
fn expand_merge(
    document: &LanguageDocument,
    into: &str,
    from: &str,
) -> Result<Vec<PrimitiveEdit>, RewriteError> {
    if into == from {
        return Err(RewriteError::Target(
            "merge needs two different signs".to_owned(),
        ));
    }
    let target = node(document, &format!("sign({into:?})"))?;
    let source_ref = node(document, &format!("sign({from:?})"))?;
    let source = document
        .language()
        .signs
        .iter()
        .find(|candidate| candidate.name == from)
        .ok_or_else(|| RewriteError::Target(format!("unknown sign {from:?}")))?;

    let mut edits = Vec::new();
    // 邊先搬(搬完義項才不會有中途懸空),再搬義項。
    for index in 0..source
        .items
        .iter()
        .filter(|item| matches!(item, SignItem::SenseEdge(_)))
        .count()
    {
        edits.push(PrimitiveEdit::Move {
            node: node(document, &format!("sign({from:?}).edge[{index}]"))?,
            new_parent: target.clone(),
            anchor: Anchor::End,
        });
    }
    for sense in source.items.iter().filter_map(|item| match item {
        SignItem::Sense(sense) => Some(&sense.name),
        _ => None,
    }) {
        edits.push(PrimitiveEdit::Move {
            node: node(document, &format!("sign({from:?}).sense[{sense:?}]"))?,
            new_parent: target.clone(),
            anchor: Anchor::End,
        });
    }
    edits.push(PrimitiveEdit::Delete { node: source_ref });
    Ok(edits)
}

/// `fossilize`/`generalize`:規則在居所階梯間**搬移**(P14),用 Move 原語。
fn expand_move_rule(
    document: &LanguageDocument,
    rule: &str,
    to: &RuleHome,
    downward: bool,
) -> Result<Vec<PrimitiveEdit>, RewriteError> {
    let target = node(document, rule)?;
    if !matches!(target.expected, NodeKind::Rule | NodeKind::FeatureRule) {
        return Err(RewriteError::Target(format!(
            "{rule:?} is not a rule ({:?})",
            target.expected
        )));
    }
    let destination = node(document, &to.selector())?;
    let from_home = home_of(document, rule)?;
    // 方向必須與名稱相符:fossilize 往下、generalize 往上(P14 Life Cycle 軸)。
    let (low, high) = (from_home.rank(), to.rank());
    if downward && high <= low {
        return Err(RewriteError::Target(format!(
            "fossilize must move a rule downward, not {from_home:?} → {to:?}"
        )));
    }
    if !downward && high >= low {
        return Err(RewriteError::Target(format!(
            "generalize must move a rule upward, not {from_home:?} → {to:?}"
        )));
    }
    Ok(vec![PrimitiveEdit::Move {
        node: target,
        new_parent: destination,
        anchor: Anchor::End,
    }])
}

/// 規則目前住在哪一級。**必須查 Language**——`global trait X` 與 `trait X` 的
/// selector 都是 `trait("X")`,只看前綴會把 P14 的三級階梯
/// (Global↔Trait↔Sign)塌成兩級,導致 global→trait 的 fossilize 被誤拒。
fn home_of(document: &LanguageDocument, rule: &str) -> Result<RuleHome, RewriteError> {
    if rule.starts_with("sign(") {
        return Ok(RuleHome::Sign(String::new()));
    }
    if let Some(rest) = rule.strip_prefix("trait(") {
        let name = rest
            .split(')')
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches('"');
        let is_global = document
            .language()
            .traits
            .iter()
            .any(|trait_def| trait_def.name == name && trait_def.global);
        return Ok(if is_global {
            RuleHome::Global(name.to_owned())
        } else {
            RuleHome::Trait(name.to_owned())
        });
    }
    Err(RewriteError::Target(format!(
        "cannot tell which home {rule:?} lives in"
    )))
}
