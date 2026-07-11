//! 字串 interning:引擎內部只流通整數 id(《M0 實作參照》§3 interning)。
//! `SymId`(音段/tier 名等符號)與 `ValId`(旋律值)型別隔離,不可混用。

use std::collections::HashMap;

/// 符號 id(音段符號、tier 名、類別名)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymId(pub u32);

/// 旋律值 id(H、L、+nasal …)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValId(pub u32);

#[derive(Debug, Default, Clone)]
struct Interner {
    strings: Vec<String>,
    map: HashMap<String, u32>,
}

impl Interner {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(s.to_owned());
        self.map.insert(s.to_owned(), id);
        id
    }

    fn resolve(&self, id: u32) -> Option<&str> {
        self.strings.get(id as usize).map(String::as_str)
    }

    fn len(&self) -> usize {
        self.strings.len()
    }
}

/// 符號表。
#[derive(Debug, Default, Clone)]
pub struct SymTable(Interner);

impl SymTable {
    pub fn intern(&mut self, s: &str) -> SymId {
        SymId(self.0.intern(s))
    }
    pub fn resolve(&self, id: SymId) -> Option<&str> {
        self.0.resolve(id.0)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
}

/// 旋律值表。
#[derive(Debug, Default, Clone)]
pub struct ValTable(Interner);

impl ValTable {
    pub fn intern(&mut self, s: &str) -> ValId {
        ValId(self.0.intern(s))
    }
    pub fn resolve(&self, id: ValId) -> Option<&str> {
        self.0.resolve(id.0)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_is_idempotent_and_resolves() {
        let mut t = SymTable::default();
        let a = t.intern("tʃ");
        let b = t.intern("tʃ");
        let c = t.intern("a");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(t.resolve(a), Some("tʃ"));
        assert_eq!(t.resolve(c), Some("a"));
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn resolve_unknown_is_none() {
        let t = ValTable::default();
        assert_eq!(t.resolve(ValId(9)), None);
    }
}
