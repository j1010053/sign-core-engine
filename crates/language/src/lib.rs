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
//! 裁決 docs/13 §4-1):language 不解析,step 11+ 原樣交給 `conlang_dsl::compile`。
//! `RuleId`/`SignId` 不入印出格式,re-parse 依文件序決定性再生(I15-b/P26)。
//! 依賴方向:`language → dsl`(P20);本 crate 對 dsl 的使用僅限公開型別。

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod printer;

pub use conlang_dsl::lower::Stage;

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
    /// 規則本體原文(`a => ə / _#`),不含 `@stage`。
    pub body: String,
    pub stage: Stage,
}

/// Block 內項目(P27:Item = Definition | Rule)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Def(Def),
    Rule(Rule),
}

/// Trait 的 block(P27 選項 A:`==` 是 Block 節點邊界,非分隔 token)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block {
    pub items: Vec<Item>,
}

// ── ③ 容器類 ──

/// Trait = sign 內容的 macro 展開模板(P5);`global: true` = 預設自動引用(P6)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitDef {
    pub name: String,
    pub global: bool,
    pub blocks: Vec<Block>,
}

/// sign 內項目:trait 引用位置有語意(P5),故與 Def/Rule 同列保序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignItem {
    /// `VerbCommon[1]`(block 序數 1 起算;全 block 強制顯式,compile 驗證)。
    TraitUse { name: String, block: u32 },
    Def(Def),
    Rule(Rule),
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
        }
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
