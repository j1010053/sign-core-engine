//! 音素庫(Inventory):`SymId ↔ FeatBits` 的單一存放處(I12)。
//!
//! 音段規則以特徵運算改寫(Lexurgy 式:符號=具名特徵束);改寫後的特徵束
//! 反查符號——無對應 = error(`EngineError::NoSymbolForBundle`,由呼叫端回報)。
//! 同一特徵束多符號時取先宣告者(宣告序 = 優先序)。

use super::feature::FeatBits;
use super::intern::SymId;

#[derive(Debug, Default, Clone)]
pub struct Inventory {
    /// (符號, 特徵束),依宣告序。
    entries: Vec<(SymId, FeatBits)>,
}

impl Inventory {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登記符號的特徵束(重複登記同符號 = 覆寫,後者為準)。
    pub fn register(&mut self, sym: SymId, feats: FeatBits) {
        if let Some(e) = self.entries.iter_mut().find(|(s, _)| *s == sym) {
            e.1 = feats;
        } else {
            self.entries.push((sym, feats));
        }
    }

    /// 特徵束 → 符號(精確匹配;先宣告者優先)。
    pub fn sym_for(&self, feats: FeatBits) -> Option<SymId> {
        self.entries
            .iter()
            .find(|(_, f)| *f == feats)
            .map(|(s, _)| *s)
    }

    /// 符號 → 特徵束。
    pub fn feats_of(&self, sym: SymId) -> Option<FeatBits> {
        self.entries
            .iter()
            .find(|(s, _)| *s == sym)
            .map(|(_, f)| *f)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_declaration_order_priority() {
        let mut inv = Inventory::new();
        inv.register(SymId(0), FeatBits(0b01)); // p = [-voice]
        inv.register(SymId(1), FeatBits(0b10)); // b = [+voice]
        inv.register(SymId(9), FeatBits(0b01)); // 同束後宣告者不奪先
        assert_eq!(inv.sym_for(FeatBits(0b01)), Some(SymId(0)));
        assert_eq!(inv.sym_for(FeatBits(0b10)), Some(SymId(1)));
        assert_eq!(inv.sym_for(FeatBits(0b11)), None);
        assert_eq!(inv.feats_of(SymId(1)), Some(FeatBits(0b10)));
        // 覆寫
        inv.register(SymId(1), FeatBits(0b11));
        assert_eq!(inv.feats_of(SymId(1)), Some(FeatBits(0b11)));
    }
}
