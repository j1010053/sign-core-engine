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
    /// `Melody tone {H, M, L} anchor mora`
    Melody {
        name: String,
        values: Vec<String>,
        anchor: String,
    },
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
    /// `level: stem|word|phrase`(P3;預設 word)
    Level(String),
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
    /// 音段 rewrite:`<sel> => <sel|*> [/ env]`(貼合 Lexurgy)
    Rewrite {
        from: Selector,
        to: Selector,
        env: Option<RuleEnv>,
    },
}

/// 具名規則。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAst {
    pub name: String,
    pub stmts: Vec<Stmt>,
}

/// 整個規則檔。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileAst {
    pub decls: Vec<Decl>,
    pub rules: Vec<RuleAst>,
}
