//! 韻律域(Prosodic Domain)模組(I14):域定義的**單一存放處**。
//!
//! 內建域 = segment/mora/syllable/foot/pword + φ(phphrase)/ι(iphrase)/U(utterance)
//! (P1 七層;φ/ι/U 於 M0 僅為合法名)。**自定域**經 [`DomainRegistry::register`] 加入
//! (DSL 的 `Prosody` 宣告鏈中未知名自動註冊),以 [`super::prosody::Level::Custom`]
//! 引用、`ProsodyLayers::extra` 承載。
//!
//! 音節內結構概念(onset/coda/核心)屬內建 mora/syllable,不泛化到自定域(I14)。

use super::prosody::Level;

/// 自定域 id(`Level::Custom(id)` 的載荷;內建域不佔 id)。
pub type CustomId = u16;

/// 空節點政策(on-daughter-loss,v0.2 §2.2)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaughterLoss {
    KeepEmpty,
    Delete,
}

/// 一個韻律域的定義。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainDef {
    pub name: String,
    /// notation 用字符。
    pub glyph: String,
    /// 下層(Span 索引所指向的層)。
    pub lower: Level,
    /// 對下層全覆蓋?(音節=true、莫拉=false)
    pub full_coverage: bool,
    /// 允許重疊?(莫拉=true:長元音)
    pub allow_overlap: bool,
    pub on_daughter_loss: DaughterLoss,
}

/// 域註冊表:內建域定義 + 自定域。住 `Env`。
#[derive(Debug, Clone, Default)]
pub struct DomainRegistry {
    custom: Vec<DomainDef>,
}

impl DomainRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 註冊自定域;同名冪等,回傳其 `Level::Custom(id)`。
    pub fn register(&mut self, def: DomainDef) -> Level {
        if let Some(i) = self.custom.iter().position(|d| d.name == def.name) {
            return Level::Custom(i as CustomId);
        }
        self.custom.push(def);
        Level::Custom((self.custom.len() - 1) as CustomId)
    }

    /// 便利:以名稱註冊一個「skeleton 之上、不強制覆蓋、不重疊」的通用自定域。
    pub fn register_named(&mut self, name: &str) -> Level {
        self.register(DomainDef {
            name: name.to_owned(),
            glyph: name.chars().take(1).collect(),
            lower: Level::Segment,
            full_coverage: false,
            allow_overlap: false,
            on_daughter_loss: DaughterLoss::Delete,
        })
    }

    pub fn get(&self, id: CustomId) -> Option<&DomainDef> {
        self.custom.get(id as usize)
    }

    /// 名稱 → Level(內建名優先,再查自定)。
    pub fn by_name(&self, name: &str) -> Option<Level> {
        match name {
            "segment" => Some(Level::Segment),
            "mora" => Some(Level::Mora),
            "syllable" => Some(Level::Syllable),
            "foot" => Some(Level::Foot),
            "pword" => Some(Level::Pword),
            // P1 七層:M0 內為合法名,以自定域承載(未註冊則 None,由呼叫端註冊)
            _ => self
                .custom
                .iter()
                .position(|d| d.name == name)
                .map(|i| Level::Custom(i as CustomId)),
        }
    }

    pub fn glyph(&self, level: Level) -> &str {
        match level {
            Level::Custom(id) => self.get(id).map(|d| d.glyph.as_str()).unwrap_or("?"),
            other => other.glyph(),
        }
    }

    pub fn len_custom(&self) -> usize {
        self.custom.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_custom_domain_idempotent_and_lookup() {
        let mut r = DomainRegistry::new();
        let colon = r.register_named("colon");
        let colon2 = r.register_named("colon");
        assert_eq!(colon, colon2);
        assert!(matches!(colon, Level::Custom(0)));
        assert_eq!(r.by_name("colon"), Some(colon));
        assert_eq!(r.by_name("mora"), Some(Level::Mora));
        assert_eq!(r.by_name("nonesuch"), None);
        assert_eq!(r.glyph(colon), "c");
        assert_eq!(r.glyph(Level::Mora), "μ");
    }
}
