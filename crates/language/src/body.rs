//! Shared content and lexical ownership for Sign definitions and their templates.
use crate::{AddressSegment, Block, Language, SignDef, SignItem, TraitDef};
use std::ops::{Deref, DerefMut};

/// Both declaration kinds own exactly one body and one lexical member scope.
/// Only the arrangement of content differs: a Sign has one item sequence;
/// a trait can divide that same content into indexed macro blocks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopedBody<C> {
    pub content: C,
    pub members: Language,
    /// Compile-only provenance: content already present at explicit expansion sites.
    pub(crate) materialized_traits: std::collections::BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraitBlocks {
    pub blocks: Vec<Block>,
}

impl<C> Deref for ScopedBody<C> {
    type Target = C;
    fn deref(&self) -> &C {
        &self.content
    }
}
impl<C> DerefMut for ScopedBody<C> {
    fn deref_mut(&mut self) -> &mut C {
        &mut self.content
    }
}
impl From<Vec<SignItem>> for ScopedBody<Block> {
    fn from(items: Vec<SignItem>) -> Self {
        Self {
            content: Block { items },
            members: Language::new(),
            materialized_traits: Default::default(),
        }
    }
}
impl From<(Vec<Block>, Language)> for ScopedBody<TraitBlocks> {
    fn from((blocks, members): (Vec<Block>, Language)) -> Self {
        Self {
            content: TraitBlocks { blocks },
            members,
            materialized_traits: Default::default(),
        }
    }
}

impl Deref for SignDef {
    type Target = ScopedBody<Block>;
    fn deref(&self) -> &Self::Target {
        &self.body
    }
}
impl DerefMut for SignDef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.body
    }
}
impl Deref for TraitDef {
    type Target = ScopedBody<TraitBlocks>;
    fn deref(&self) -> &Self::Target {
        &self.body
    }
}
impl DerefMut for TraitDef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.body
    }
}

impl Language {
    /// All lexical containers. Consumers must not assume only traits own scopes.
    pub fn member_scopes(&self) -> impl Iterator<Item = (AddressSegment, &str, &Language)> {
        self.traits
            .iter()
            .enumerate()
            .map(|(i, t)| (AddressSegment::Traits(i), t.name.as_str(), &t.body.members))
            .chain(
                self.signs
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (AddressSegment::Signs(i), s.name.as_str(), &s.body.members)),
            )
    }

    pub fn member_scopes_mut(&mut self) -> impl Iterator<Item = (AddressSegment, &mut Language)> {
        self.traits
            .iter_mut()
            .enumerate()
            .map(|(i, t)| (AddressSegment::Traits(i), &mut t.body.members))
            .chain(
                self.signs
                    .iter_mut()
                    .enumerate()
                    .map(|(i, s)| (AddressSegment::Signs(i), &mut s.body.members)),
            )
    }

    pub fn members_at(&self, owner: &AddressSegment) -> Option<&Language> {
        match owner {
            AddressSegment::Traits(i) => Some(&self.traits.get(*i)?.body.members),
            AddressSegment::Signs(i) => Some(&self.signs.get(*i)?.body.members),
            _ => None,
        }
    }

    pub fn members_at_mut(&mut self, owner: &AddressSegment) -> Option<&mut Language> {
        match owner {
            AddressSegment::Traits(i) => Some(&mut self.traits.get_mut(*i)?.body.members),
            AddressSegment::Signs(i) => Some(&mut self.signs.get_mut(*i)?.body.members),
            _ => None,
        }
    }

    pub(crate) fn has_members(&self) -> bool {
        self.member_scopes().any(|(_, _, scope)| {
            !scope.traits.is_empty() || !scope.signs.is_empty() || !scope.paradigms.is_empty()
        })
    }
}
