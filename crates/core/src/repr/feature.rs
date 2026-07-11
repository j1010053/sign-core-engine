//! 特徵系統:每個「特徵=值」原子占 bitset 一位。
//!
//! - 二元特徵:`(voice, +)` 與 `(voice, -)` 是兩個原子(容許缺值 = 兩位皆 0,對應 Lexurgy 的 `*`)。
//! - 多值特徵:`(place, labial)`、`(place, alveolar)` … 各一原子(one-hot)。
//! - 私有特徵:僅登記 `(nasal, +)`;不鼻化 = 位元 0(對齊 D12 的 Ø 哲學)。
//! - `[αplace]` 特徵變數:以 `mask_of("place")` 取遮罩、`extract` 抽出該特徵目前的值位。
//!
//! MVP 上限 64 原子(u64);超出回傳 `FeatureSpaceExhausted`(未來換 u128/Vec<u64>)。

use std::collections::HashMap;

use super::ReprError;

/// 單一特徵原子的位元索引。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatBit(pub u8);

/// 特徵集合(bitset)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FeatBits(pub u64);

impl FeatBits {
    pub const EMPTY: FeatBits = FeatBits(0);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// self ⊇ other(自然類匹配:`[labial stop]` 對音段 = 超集測試)。
    pub fn contains(self, other: FeatBits) -> bool {
        self.0 & other.0 == other.0
    }
    pub fn has(self, bit: FeatBit) -> bool {
        self.0 & (1u64 << bit.0) != 0
    }
    pub fn union(self, other: FeatBits) -> FeatBits {
        FeatBits(self.0 | other.0)
    }
    pub fn intersect(self, other: FeatBits) -> FeatBits {
        FeatBits(self.0 & other.0)
    }
    pub fn minus(self, other: FeatBits) -> FeatBits {
        FeatBits(self.0 & !other.0)
    }
    pub fn insert(&mut self, bit: FeatBit) {
        self.0 |= 1u64 << bit.0;
    }
    pub fn remove(&mut self, bit: FeatBit) {
        self.0 &= !(1u64 << bit.0);
    }
    /// 以 `mask` 界定的特徵欄位整體改值:清掉舊值位、放入新值位。
    /// 同化(`[nasal] => [$place]`)的位元運算基礎。
    pub fn set_field(self, mask: FeatBits, value: FeatBits) -> FeatBits {
        debug_assert!(mask.contains(value), "value bits must lie within field mask");
        self.minus(mask).union(value.intersect(mask))
    }
}

/// 特徵註冊表:原子 ↔ 位元的單一存放處。
#[derive(Debug, Default, Clone)]
pub struct FeatureRegistry {
    atoms: Vec<(String, String)>,
    by_atom: HashMap<(String, String), u8>,
    feature_mask: HashMap<String, u64>,
}

impl FeatureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 註冊(或取回)一個「特徵=值」原子。
    pub fn register(&mut self, feature: &str, value: &str) -> Result<FeatBit, ReprError> {
        let key = (feature.to_owned(), value.to_owned());
        if let Some(&b) = self.by_atom.get(&key) {
            return Ok(FeatBit(b));
        }
        let idx = self.atoms.len();
        if idx >= 64 {
            return Err(ReprError::FeatureSpaceExhausted);
        }
        self.atoms.push(key.clone());
        self.by_atom.insert(key, idx as u8);
        *self.feature_mask.entry(feature.to_owned()).or_insert(0) |= 1u64 << idx;
        Ok(FeatBit(idx as u8))
    }

    pub fn bit(&self, feature: &str, value: &str) -> Option<FeatBit> {
        self.by_atom
            .get(&(feature.to_owned(), value.to_owned()))
            .map(|&b| FeatBit(b))
    }

    /// 單一原子的 bitset(便於組合矩陣)。
    pub fn bits(&self, feature: &str, value: &str) -> Option<FeatBits> {
        self.bit(feature, value).map(|b| {
            let mut fb = FeatBits::EMPTY;
            fb.insert(b);
            fb
        })
    }

    /// 某特徵所有值位的遮罩(`[αF]` 用)。
    pub fn mask_of(&self, feature: &str) -> Option<FeatBits> {
        self.feature_mask.get(feature).map(|&m| FeatBits(m))
    }

    /// 從 `feats` 抽出某特徵目前的值位(可能為空 = 缺值)。
    pub fn extract(&self, feats: FeatBits, feature: &str) -> Option<FeatBits> {
        self.mask_of(feature).map(|m| feats.intersect(m))
    }

    /// 反查位元對應的(特徵, 值),供診斷/notation 顯示。
    pub fn atom(&self, bit: FeatBit) -> Option<(&str, &str)> {
        self.atoms
            .get(bit.0 as usize)
            .map(|(f, v)| (f.as_str(), v.as_str()))
    }

    pub fn len(&self) -> usize {
        self.atoms.len()
    }
    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_match_natural_class() {
        let mut r = FeatureRegistry::new();
        let lab = r.bits_or("place", "labial");
        let stop = r.bits_or("manner", "stop");
        let vcd = r.bits_or("voice", "+");

        // p = [labial stop];b = [labial stop +voice]
        let p = lab.union(stop);
        let b = p.union(vcd);
        let class_labial_stop = lab.union(stop);
        assert!(p.contains(class_labial_stop));
        assert!(b.contains(class_labial_stop));
        assert!(!lab.contains(class_labial_stop));
    }

    #[test]
    fn field_mask_and_assimilation() {
        let mut r = FeatureRegistry::new();
        let lab = r.bits_or("place", "labial");
        let alv = r.bits_or("place", "alveolar");
        let nas = r.bits_or("nasal", "+");
        let place_mask = r.mask_of("place").unwrap();

        // n = [alveolar +nasal];後接 p = [labial] → 同化:n 的 place 欄位改 labial
        let n = alv.union(nas);
        let assimilated = n.set_field(place_mask, lab);
        assert!(assimilated.contains(lab));
        assert!(!assimilated.contains(alv));
        assert!(assimilated.contains(nas)); // 非 place 位不受影響
        assert_eq!(r.extract(assimilated, "place").unwrap(), lab);
    }

    #[test]
    fn exhaustion_at_64() {
        let mut r = FeatureRegistry::new();
        for i in 0..64 {
            r.register("f", &i.to_string()).unwrap();
        }
        assert_eq!(
            r.register("f", "overflow"),
            Err(crate::repr::ReprError::FeatureSpaceExhausted)
        );
    }

    impl FeatureRegistry {
        /// 測試便利:register + bits,失敗即測試失敗。
        fn bits_or(&mut self, f: &str, v: &str) -> FeatBits {
            let bit = self.register(f, v).expect("under 64 atoms in tests");
            let mut fb = FeatBits::EMPTY;
            fb.insert(bit);
            fb
        }
    }
}
