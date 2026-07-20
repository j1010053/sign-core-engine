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
//! `RuleId`/`SignId` 不入印出格式,re-parse 依文件序決定性再生(I15-b/P26)。
//! 依賴方向:`language → dsl`(P20);本 crate 對 dsl 的使用僅限公開型別。

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod codegen;
pub mod compile;
pub mod construction;
pub mod ontology;
pub mod parser;
pub mod projection;
pub mod sem;
pub mod word;
pub mod path;
pub mod printer;

pub use tshiatun_dsl::lower::Stage;

// ── 共時四維(修補07 P38;四棵獨立 ontology)──

/// 四個彼此獨立的共時維度。ontology trait 以此標記歸屬哪棵分類樹;
/// **不共享同一棵樹**(P38)——`belongs` 閉包只在同 dim 內走。
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
    /// 四維迭代(registry 建四棵樹用)。
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

// ── ID(P26:純序列配發;不入印出格式,I15-b)──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SignId(pub u32);

// ── ①/② 語句 ──

/// Definition(`=`):語言知識,無執行順序(P9;compile 依欄位 Merge Strategy 合併)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    /// 左端路徑(`syn.provides`、`phon`、`entrenchment`…)。
    pub path: String,
    /// 右端值(步驟 8 以 canonical 原文承載;步驟 9 起結構化為 Ref/字面值)。
    pub value: String,
}

/// Rule(`=>`):狀態轉換,同 stage 內依書寫順序(P18)。
/// 步驟 8 以 raw body 承載(I15-c);env/action/else 結構化屬步驟 9。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// 穩定 ID(fossilize/generalize 的 move 對象;P25 定址靠它)。
    pub id: RuleId,
    /// 主分支原文(`a => ə / _#`),不含 `@stage` 與 else。
    pub body: String,
    pub stage: Stage,
    /// `else` 鏈(P22):disjunctive 單趟,第一匹配勝出;分支共享本規則 stage。
    /// 各分支為原文(`ɐ / _[+cons]`、無條件 `e`);結構化隨步驟 10。
    pub else_chain: Vec<String>,
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
    TraitUse { name: String, block: Option<u32> },
    /// `belongs Transitive`(P40):sign 掛入某 ontology 節點;閉包由 registry 走。
    Belongs(String),
    /// `slot NAME [Filler]`(可尾綴 `?` = optional;P41 valence=slots,I21)。
    /// 帶 ≥1 slot 的 sign = construction(P42);filler 是 syn ontology 範疇約束。
    Slot(Slot),
    Def(Def),
    Rule(Rule),
}

/// 一個 argument slot(P41:valence 由 slots 構成,非數字欄位)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub name: String,
    /// filler 必須具備的 syn ontology 範疇(其 `belongs` 閉包須含此名,P40)。
    pub filler: String,
    /// `?` 標記 = 非必填(I21;預設必填)。
    pub optional: bool,
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
    next_rule: u32,
    next_sign: u32,
}

impl Language {
    /// canonical empty Language(P28)。
    pub fn new() -> Language {
        Language::default()
    }

    /// 決定性 RuleId 配發(P26:純序列)。
    pub fn fresh_rule_id(&mut self) -> RuleId {
        let id = RuleId(self.next_rule);
        self.next_rule += 1;
        id
    }

    /// 決定性 SignId 配發(P26)。
    pub fn fresh_sign_id(&mut self) -> SignId {
        let id = SignId(self.next_sign);
        self.next_sign += 1;
        id
    }

    /// 建規則(id 自動配發)。
    pub fn rule(&mut self, body: impl Into<String>, stage: Stage) -> Rule {
        Rule {
            id: self.fresh_rule_id(),
            body: body.into(),
            stage,
            else_chain: Vec::new(),
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
            id,
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
        assert_eq!((s1, r2), (s1b, r2b));
        assert_eq!(s1, SignId(0));
        assert_eq!(r2, RuleId(1));
    }
}
