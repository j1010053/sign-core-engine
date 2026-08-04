//! conlang-stats — 模組 E:**先驗、有效分佈、統計投影**(鳥瞰步驟 19)。
//!
//! 權威:`統計先驗與抽樣引擎_v0.1` §1–§4 + **§6 增修 A**(擁有者 2026-08-04)。
//!
//! # 有效分佈是**三層**,不是四層
//!
//! ```text
//! 手動逐項覆寫    ← 使用者個別調(「/k/ = 0.15」)
//!   > 導入 provider ← 整份注入(§3)
//!   > E1 先驗       ← 基底
//! ```
//!
//! §2 原訂四層,第三層是「從本節點 sign 集合即時數出的統計投影」。
//! §6.1 把它**移出抽樣棧**:投影照做,但接 Query API 當**唯讀報表**
//! ([`project_phoneme_freq`]),抽樣器不看。
//!
//! **代價**:分佈不再隨語言演化自動更新。**可逆**:投影原居第三層,
//! 日後要加回是在 provider 與 E1 之間插一層,對介面純增量。
//!
//! # 鍵 = IPA 字串(§6.2)
//!
//! 三個來源共用一組鍵。**不用 `SymId`**——那是 tshiatun 引擎側的 intern 結果,
//! 拿它當跨語言先驗的鍵會把 E 綁進引擎。
//!
//! # 職責邊界(§4)
//!
//! E **只管加權**。硬約束(音位配列)由過濾器負責,且過濾器不住這裡
//! ——見 `conlang_generate::PhonotacticFilter`。兩職責分立:
//! E 不碰約束、驗證器不碰頻率。
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use conlang_language::{Language, LibraryPackage, SignItem};
use std::collections::BTreeMap;

/// **IPA 字串 → 權重**。三個來源共用同一組鍵(§6.2)。
///
/// 保 `BTreeMap` 而非 `HashMap`:抽樣必須決定性(P26),而抽樣器吃的是
/// **有序** `Vec<f64>`,位置即身分——鍵序不定就等於抽樣結果不定。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeightTable {
    entries: BTreeMap<String, f64>,
}

impl WeightTable {
    pub fn new() -> WeightTable {
        WeightTable::default()
    }

    pub fn set(&mut self, key: impl Into<String>, weight: f64) -> &mut Self {
        self.entries.insert(key.into(), weight);
        self
    }

    pub fn get(&self, key: &str) -> Option<f64> {
        self.entries.get(key).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 鍵序固定(`BTreeMap`),故可直接餵抽樣器。
    pub fn iter(&self) -> impl Iterator<Item = (&str, f64)> {
        self.entries.iter().map(|(key, weight)| (key.as_str(), *weight))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// 拆成 `sample_weighted_index` 要的兩份平行資料(鍵序固定,故對得起來)。
    pub fn to_sampler_input(&self) -> (Vec<&str>, Vec<f64>) {
        self.iter().unzip()
    }
}

impl FromIterator<(String, f64)> for WeightTable {
    fn from_iter<T: IntoIterator<Item = (String, f64)>>(iter: T) -> Self {
        WeightTable {
            entries: iter.into_iter().collect(),
        }
    }
}

/// 某一項的權重來自哪一層——**可審計**。
///
/// 沒有這個,使用者調了 `/k/` 卻看到別的數字時無從追查是被哪層蓋掉。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// 手動逐項覆寫(最高)。
    Manual,
    /// 導入的 provider 分佈。
    Imported,
    /// E1 先驗(基底)。
    Prior,
}

/// 三層疊加的有效分佈(§6.1)。**高優先者逐項覆寫低優先**,
/// 未覆蓋的項落到下一層。
#[derive(Debug, Clone, Default)]
pub struct EffectiveDistribution {
    manual: WeightTable,
    imported: WeightTable,
    prior: WeightTable,
}

impl EffectiveDistribution {
    /// 以 E1 先驗為基底起手。
    pub fn from_prior(prior: WeightTable) -> Self {
        Self {
            prior,
            ..Self::default()
        }
    }

    /// 整份注入一個 provider 的分佈(§3)。
    ///
    /// **導入 = 複製 snapshot 進覆寫層**,非外部活連結——可重現性與離線性不破(§3)。
    pub fn with_imported(mut self, provider: &dyn DistributionProvider) -> Self {
        self.imported = provider.distribution();
        self
    }

    /// 逐項手動覆寫(最高優先)。
    pub fn with_manual(mut self, manual: WeightTable) -> Self {
        self.manual = manual;
        self
    }

    /// 某鍵的有效權重。
    pub fn weight(&self, key: &str) -> Option<f64> {
        self.manual
            .get(key)
            .or_else(|| self.imported.get(key))
            .or_else(|| self.prior.get(key))
    }

    /// 某鍵的權重**來自哪一層**。
    pub fn provenance(&self, key: &str) -> Option<Layer> {
        if self.manual.get(key).is_some() {
            Some(Layer::Manual)
        } else if self.imported.get(key).is_some() {
            Some(Layer::Imported)
        } else if self.prior.get(key).is_some() {
            Some(Layer::Prior)
        } else {
            None
        }
    }

    /// 疊完的結果。鍵 = 三層鍵的聯集(`BTreeMap` 故有序)。
    pub fn resolve(&self) -> WeightTable {
        let mut keys: Vec<&str> = Vec::new();
        keys.extend(self.prior.keys());
        keys.extend(self.imported.keys());
        keys.extend(self.manual.keys());
        keys.sort_unstable();
        keys.dedup();
        keys.into_iter()
            .filter_map(|key| self.weight(key).map(|weight| (key.to_owned(), weight)))
            .collect()
    }
}

/// 整份分佈的來源(§3)。
///
/// **導入只影響抽樣權重,不碰 phonotactics 硬約束**(§3 界線):
/// 「接西班牙語分佈」= 頻率像西語,音節合法性仍由本語言的 phonotactics 決定。
pub trait DistributionProvider: std::fmt::Debug {
    fn distribution(&self) -> WeightTable;
}

/// 逐項手動(§3;最高優先層通常直接用 [`EffectiveDistribution::with_manual`],
/// 此型別供「把手動表也當 provider 傳遞」的場合)。
#[derive(Debug, Clone)]
pub struct Manual(pub WeightTable);

/// E1 的某個類型學傾向(§3【M】)。
#[derive(Debug, Clone)]
pub struct TypologicalPrior(pub WeightTable);

/// 借另一個 conlang 節點的分佈——方言/姊妹語參考(§3【M】)。
#[derive(Debug, Clone)]
pub struct OtherNode(pub WeightTable);

macro_rules! provider {
    ($name:ident) => {
        impl DistributionProvider for $name {
            fn distribution(&self) -> WeightTable {
                self.0.clone()
            }
        }
    };
}
provider!(Manual);
provider!(TypologicalPrior);
provider!(OtherNode);

#[derive(Debug, thiserror::Error)]
pub enum PriorError {
    #[error("E1 prior {path}: line {line}: {message}")]
    Parse {
        path: String,
        line: usize,
        message: String,
    },
}

/// E1 起步庫的檔名尾綴。任何 package 的 `data/` 底下叫這個的都會被讀入。
pub const PRIOR_FILE_SUFFIX: &str = "/segments.tsv";

/// 自套件 `data/*/segments.tsv` 載入 E1 先驗(§6.5)。
///
/// 格式 `segment<TAB>weight`,首行為表頭。與步驟 17 的
/// `weight_db_from_packages` 同形——先驗是 **data**(裁定 W),
/// 不進 `.lang`/`.chg`,且由 R9-a 之後可以是外部注入的 package。
///
/// 同一個 segment 出現在多個 package 時**後載入者勝**;呼叫端以 catalog 的
/// 決定性排序決定順序。
pub fn load_prior_from_packages(packages: &[&LibraryPackage]) -> Result<WeightTable, PriorError> {
    let mut table = WeightTable::new();
    for package in packages {
        for source in package
            .data_sources
            .iter()
            .filter(|source| source.path.ends_with(PRIOR_FILE_SUFFIX))
        {
            parse_prior(&mut table, &source.path, &source.source)?;
        }
    }
    Ok(table)
}

fn parse_prior(table: &mut WeightTable, path: &str, source: &str) -> Result<(), PriorError> {
    let error = |line: usize, message: &str| PriorError::Parse {
        path: path.to_owned(),
        line,
        message: message.to_owned(),
    };
    let mut lines = source.lines().enumerate();
    let Some((_, header)) = lines.find(|(_, line)| !line.trim().is_empty()) else {
        return Err(error(0, "prior table is empty"));
    };
    let columns: Vec<&str> = header.split('\t').map(str::trim).collect();
    if columns != ["segment", "weight"] {
        return Err(error(1, "header must be `segment<TAB>weight`"));
    }
    for (index, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let number = index + 1;
        let Some((segment, weight)) = line.split_once('\t') else {
            return Err(error(number, "row must be `segment<TAB>weight`"));
        };
        let weight: f64 = weight
            .trim()
            .parse()
            .map_err(|_| error(number, "weight must be a number"))?;
        if !weight.is_finite() || weight < 0.0 {
            return Err(error(number, "weight must be finite and non-negative"));
        }
        table.set(segment.trim(), weight);
    }
    Ok(())
}

// ── 統計投影:**報表,不是抽樣依據**(§6.1)──────────────────────────────

/// 數本語言 UR(`phon` Def)裡各音素出現次數。
///
/// **這是報表**——接 `query::phoneme_stats`(邏輯分層 §3.2),供使用者參考
/// 「這個語言目前實際長什麼樣」。**抽樣器不看它**(§6.1)。
///
/// # 切分依 `inventory`,**最長匹配**
///
/// 多字元 IPA 音段(塞擦音 `t͡ʃ`、帶附加符號者)必須整段算,否則 `t͡ʃa` 會被
/// 拆成三個「音素」。清單由**呼叫端提供**:`Language` 的 dsl 域宣告是
/// **不透明 verbatim 行**(I15-a / 裁決 1),`language` 刻意不解析,`stats`
/// 更不該越界去 parse `Symbol` 行。與 `PhonotacticFilter`、
/// `DistributionProvider` 同一個注入模式。
///
/// 最有用的清單是**有效分佈的鍵集**——那樣報表與抽樣用同一組鍵,
/// 「我的語言實際用多少 / 先驗說該用多少」才對得起來。
///
/// # 兩個刻意的行為
///
/// - `inventory` 為空 → 退回**逐字元**切分(沒有宣告清單時的報表仍可用);
/// - 匹配不到任何音素的字元 → **以該字元自成一鍵計入**,不是丟掉。
///   這樣「本語言用了不在清單裡的音段」會在報表上現形,那是撰寫錯誤的訊號。
///
/// 口徑其餘部分取最直白的一種:數 UR、不以 `entrenchment` 加權、type 非 token。
/// 因為沒有東西依賴它,日後要別的口徑就多一個函數,不會有既有行為被改壞。
pub fn project_phoneme_freq(language: &Language, inventory: &[&str]) -> WeightTable {
    // 最長匹配:長的排前面,先試長的。
    let mut ordered: Vec<&str> = inventory.iter().copied().filter(|s| !s.is_empty()).collect();
    ordered.sort_by_key(|segment| std::cmp::Reverse(segment.chars().count()));

    let mut table = WeightTable::new();
    for sign in &language.signs {
        for item in &sign.items {
            let SignItem::Def(def) = item else { continue };
            if def.path != "phon" {
                continue;
            }
            let form = def.value.trim().trim_matches('/');
            let mut rest = form;
            while !rest.is_empty() {
                let matched = ordered.iter().find(|segment| rest.starts_with(**segment));
                let key = match matched {
                    Some(segment) => *segment,
                    // 匹配不到 → 取一個字元自成一鍵(現形,不吞掉)
                    None => {
                        let first = rest.chars().next().expect("rest is not empty");
                        &rest[..first.len_utf8()]
                    }
                };
                rest = &rest[key.len()..];
                if key.chars().all(char::is_whitespace) {
                    continue;
                }
                let next = table.get(key).unwrap_or(0.0) + 1.0;
                table.set(key, next);
            }
        }
    }
    table
}
