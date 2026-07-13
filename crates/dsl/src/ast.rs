//! 型別化 AST(parser 產物;名稱尚未解析——lowering 才把字串換成 id/bits)。
//!
//! 宣告貼合 Lexurgy 形(`Feature`/`Symbol`/`Class`;M0 §4 沿用欄);
//! `Melody` 為本 DSL 擴充。規則 = 具名 + 語句序列(語法規格 §5–§6、§11)。

/// 頂層宣告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decl {
    /// `Feature voice(+voice, -voice)` / `Feature place(labial, alveolar)`
    Feature { name: String, values: Vec<String> },
    /// `Symbol p [-voice]`;矩陣可省(無特徵符號)。
    Symbol { name: String, atoms: Vec<String> },
    /// `Class vowel {a, e}`
    Class { name: String, members: Vec<String> },
    /// `Prosody mora < syllable < foot < pword`(鏈中未知名 → 註冊自定域,I14)
    Prosody { chain: Vec<String> },
    /// `Parse mora: @vowel | @vowel :: @cons`(WBP)/ `Parse syllable: @cons? :: @vowel :: @cons?`
    Parse {
        level: String,
        /// 擇一(`|`)→ 各為 `::` 分節的 term 序列。
        alts: Vec<Vec<ParseTerm>>,
    },
    /// `Melody tone {H, M, L} anchor mora`
    Melody {
        name: String,
        values: Vec<String>,
        anchor: String,
    },
}

/// Parse pattern 的一個 term(`@class` + 可選 `?`;D24 有限 pattern language 子集)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTerm {
    pub class: String,
    pub optional: bool,
}

/// selector 的 element(D15:一切皆 element,`&` 交集)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    /// `[+voice -nasal]`:原子以值名列出(`+voice`、`labial`)。
    Matrix(Vec<String>),
    /// 識別字 element:類名/層名/tier 名/旋律值/文字音段——lowering 依宣告消歧。
    Named(String),
    /// `@vowel`(類別引用)
    ClassRef(String),
    /// `<mora>`(韻律層引用)
    LevelRef(String),
    /// `floating`
    Floating,
    /// `Ø`(零特徵)
    Empty,
    /// `#`(詞界)
    Boundary,
    /// `.`(音節界;僅環境內)
    SylBoundary,
    /// `*`(刪除;僅改寫輸出側)
    Star,
}

/// selector = element (`&` element)*(序數尾槽步驟 6 才需要)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector(pub Vec<Element>);

/// 規則環境 `/ pre _ post`(pre/post 各為可省 selector)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEnv {
    pub pre: Option<Selector>,
    pub post: Option<Selector>,
}

/// 規則內語句。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `stage: stem|word|phrase`(P3/I14;預設 word)
    Stage(String),
    /// `insert <值> floating [near <層>] [/ env]`
    Insert {
        val: String,
        near: Option<String>,
        env: Option<RuleEnv>,
    },
    /// `dock <sel> strategy <名> [prefer-left|prefer-right]`
    Dock {
        sel: Selector,
        strategy: String,
        tiebreak: Option<String>,
    },
    /// `fill <tier> Ø => <值> [within <疆界>]`
    Fill {
        tier: String,
        val: String,
        within: Option<String>,
    },
    /// `merge adjacent-equal`
    MergeAdjacentEqual,
    /// `spread <值> <方向> [blocked-by <sel>] [within <單位>] [through] [on-conflict <值|stop>]`
    Spread {
        val: String,
        ward: String,
        blocked_by: Option<Selector>,
        within: Option<String>,
        through: bool,
        on_conflict: Option<String>,
    },
    /// `shift <n> <軌道單位> <方向>`
    Shift {
        n: u32,
        unit: String,
        ward: String,
    },
    /// `dominate <sel> -> <sel> <方向>`(結構修復)
    Dominate {
        sel: Selector,
        target: Selector,
        ward: String,
    },
    /// Scan 塊內:`associate <值> -> <目標>[序數]`(D16)
    ScanAssociate {
        val: String,
        target: Selector,
        ordinal: Option<OrdinalAst>,
    },
    /// 音段 rewrite:`<sel> => <sel|*> [/ env]`(貼合 Lexurgy);Scan 塊內同形 = 值改寫
    Rewrite {
        from: Selector,
        to: Selector,
        env: Option<RuleEnv>,
    },
}

/// Scan 塊頭(D3 三道鎖)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHead {
    pub tier: String,
    pub along: String,
    pub within: Option<String>,
    pub from: Option<String>,
    pub over: Option<String>,
}

/// 序數尾槽(D16;僅 Scan 內)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdinalAst {
    Nth(u32),
    First,
}

/// 具名規則(`scan` 有值 = Scan 塊;塊內每條語句各為一條規則)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAst {
    pub name: String,
    pub scan: Option<ScanHead>,
    pub stmts: Vec<Stmt>,
}

/// Spell-out 宣告區塊(C10;語意權威=執行語意 §6)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpelloutAst {
    /// `order tone, nasal`
    pub order: Vec<String>,
    /// `empty tone => M`(值 "bare" = 不帶)
    pub empty: Vec<(String, String)>,
    /// `floating tone => drop|error`
    pub floating: Option<String>,
    /// `contour tone:{H L} => falling`
    pub contour: Vec<(String, Vec<String>, String)>,
}

/// 整個規則檔。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileAst {
    pub decls: Vec<Decl>,
    pub rules: Vec<RuleAst>,
    pub spellout: Option<SpelloutAst>,
}
