//! One member-composition rule for lexical binding, compilation and source references.
use super::*;

#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub reference: TypeRef,
    pub kind: Kind,
    pub exported: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MemberScopes {
    pub(super) catalog: BTreeMap<(Kind, String), Template>,
}

impl MemberScopes {
    pub(crate) fn new(standard: &Language, language: &Language) -> Result<Self, String> {
        Ok(Self {
            catalog: catalog(standard, language)?,
        })
    }

    pub(crate) fn with_language(&self, language: &Language) -> Result<Self, String> {
        let mut result = self.clone();
        result.catalog.extend(catalog(&Language::new(), language)?);
        Ok(result)
    }

    /// Alias paths point at existing declarations, never synthetic source nodes.
    pub(crate) fn source_aliases(
        &self,
        language: &Language,
    ) -> Result<Vec<(Kind, String, String)>, String> {
        self.source_aliases_with(language, true)
    }

    /// Frozen type-position behavior for reference_semantics 3 sidecars.
    pub(crate) fn source_aliases_v3(
        &self,
        language: &Language,
    ) -> Result<Vec<(Kind, String, String)>, String> {
        self.source_aliases_with(language, false)
    }

    fn source_aliases_with(
        &self,
        language: &Language,
        instance_types: bool,
    ) -> Result<Vec<(Kind, String, String)>, String> {
        let mut result = Vec::new();
        for template in self.catalog.values() {
            let mut owner = TypeRef::parse(&template.levels.last().unwrap().path)?;
            for level in &template.levels {
                owner.segments[level.path.split('.').count() - 1].arguments = level
                    .parameters
                    .iter()
                    .map(|p| TypeRef::parse(&p.name).map(Into::into))
                    .collect::<Result<_, _>>()?;
            }
            for (name, member) in self.members(&owner)? {
                let alias = format!("{}.{name}", owner.path());
                if alias != member.reference.path() {
                    result.push((member.kind, alias, member.reference.path()));
                }
            }
        }
        // Follow paths actually written in the document as well. Enumerating
        // every possible descendant alias would be infinite for recursive scopes.
        let qualified = qualify_members_with(language, &|owner, _| {
            Ok(self.members(owner)?.into_keys().collect())
        })?;
        for template in catalog(&Language::new(), &qualified)?.values() {
            let mut owner = TypeRef::parse(&template.levels.last().unwrap().path)?;
            for level in &template.levels {
                owner.segments[level.path.split('.').count() - 1].arguments = level
                    .parameters
                    .iter()
                    .map(|p| TypeRef::parse(&p.name).map(Into::into))
                    .collect::<Result<_, _>>()?;
            }
            let mut bind = |name: &mut String, kind| {
                self.collect_source_path(
                    &TypeRef::parse(name)?,
                    kind,
                    &owner,
                    &mut result,
                    instance_types,
                )
            };
            match template.definition.clone() {
                Definition::Trait(mut t) => {
                    for parameter in &mut t.type_params {
                        if let Some(bound) = &mut parameter.bound {
                            bind(bound, Kind::Trait.into())?;
                        }
                    }
                    for block in &mut t.blocks {
                        crate::package_scope::visit_items(&mut block.items, &mut bind)?;
                    }
                }
                Definition::Sign(mut s) => {
                    crate::package_scope::visit_items(&mut s.items, &mut bind)?
                }
                Definition::Paradigm(mut p) => {
                    crate::package_scope::visit_case(&mut p.selector, &mut bind)?
                }
            }
        }
        result.sort();
        result.dedup();
        Ok(result)
    }

    fn collect_source_path(
        &self,
        reference: &TypeRef,
        kind: RefKind,
        owner: &TypeRef,
        result: &mut Vec<(Kind, String, String)>,
        instance_types: bool,
    ) -> Result<(), String> {
        let kind = if !instance_types && kind == RefKind::Type {
            Kind::Trait.into()
        } else {
            kind
        };
        for (index, segment) in reference.segments.iter().enumerate() {
            for argument in &segment.arguments {
                self.collect_source_path(
                    &argument.value,
                    RefKind::Type,
                    owner,
                    result,
                    instance_types,
                )?;
            }
            let prefix = TypeRef {
                segments: reference.segments[..=index].to_vec(),
            };
            let (target, alias) = self.resolve(&prefix, owner)?;
            if alias {
                let actual = if index + 1 == reference.segments.len() {
                    self.catalog.values().find(|t| {
                        kind.accepts(t.kind()) && t.levels.last().unwrap().path == target.path()
                    })
                } else {
                    self.owner(&target.path())
                };
                if let Some(actual) = actual {
                    result.push((actual.kind(), prefix.path(), target.path()));
                }
            }
        }
        Ok(())
    }

    fn owner(&self, path: &str) -> Option<&Template> {
        self.catalog
            .get(&(Kind::Trait, path.into()))
            .or_else(|| self.catalog.get(&(Kind::Sign, path.into())))
    }

    /// Normalize spelling before comparing owners, composing members or looking
    /// up an existing instance. Argument labels never enter instance identity.
    pub(crate) fn normalize_arguments(&self, reference: &TypeRef) -> Result<TypeRef, String> {
        let mut result = reference.clone();
        let mut path = String::new();
        for segment in &mut result.segments {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(&segment.name);
            for argument in &mut segment.arguments {
                argument.value = self.normalize_arguments(&argument.value)?;
            }
            // Bare names can be captured formal parameters during lexical binding.
            // Completeness is checked when a concrete template is instantiated.
            if segment.arguments.is_empty() {
                continue;
            }
            if let Some(template) = self
                .owner(&path)
                .or_else(|| self.catalog.get(&(Kind::Paradigm, path.clone())))
            {
                segment.arguments = segment
                    .ordered_arguments(&template.levels.last().unwrap().parameters)?
                    .into_iter()
                    .map(Into::into)
                    .collect();
            }
        }
        Ok(result)
    }

    fn bindings(
        template: &Template,
        reference: &TypeRef,
    ) -> Result<BTreeMap<String, TypeRef>, String> {
        let mut bindings = BTreeMap::new();
        for level in &template.levels {
            let segment = &reference.segments[level.path.split('.').count() - 1];
            for (parameter, argument) in level
                .parameters
                .iter()
                .zip(segment.ordered_arguments(&level.parameters)?)
            {
                bindings.insert(parameter.name.clone(), argument);
            }
        }
        Ok(bindings)
    }

    pub(crate) fn members(&self, owner: &TypeRef) -> Result<BTreeMap<String, Member>, String> {
        if !self.catalog.values().any(|t| t.levels.len() > 1) {
            return Ok(BTreeMap::new());
        }
        self.collect(owner, &mut Vec::new())
    }

    fn collect(
        &self,
        owner: &TypeRef,
        active: &mut Vec<String>,
    ) -> Result<BTreeMap<String, Member>, String> {
        let owner = &self.normalize_arguments(owner)?;
        let Some(template) = self.owner(&owner.path()) else {
            return Ok(BTreeMap::new());
        };
        let spelling = owner.to_string();
        if active.contains(&spelling) || active.len() >= 64 {
            return Err(failure("TEMPLATE_MEMBER_CYCLE", active.join(" -> ")));
        }
        active.push(spelling.clone());
        let mut result = BTreeMap::new();
        let mut insert = |name: String, member: Member| -> Result<(), String> {
            if result.insert(name.clone(), member).is_some() {
                return Err(failure(
                    "TEMPLATE_MEMBER_CONFLICT",
                    format!("{owner} has multiple declarations of {name}"),
                ));
            }
            Ok(())
        };
        for child in self.catalog.values().filter(|t| {
            t.levels.len() == template.levels.len() + 1
                && t.levels[t.levels.len() - 2].path == owner.path()
        }) {
            let level = child.levels.last().unwrap();
            let name = level.path.rsplit('.').next().unwrap().to_owned();
            let mut reference = owner.clone();
            reference.segments.push(TypeSegment {
                name: name.clone(),
                arguments: vec![],
            });
            insert(
                name,
                Member {
                    reference,
                    kind: child.kind(),
                    exported: level.exported,
                },
            )?;
        }
        let bindings = Self::bindings(template, owner)?;
        let mut items: Vec<SignItem> = match &template.definition {
            Definition::Trait(t) => t
                .blocks
                .iter()
                .flat_map(|b| b.items.iter().cloned())
                .collect(),
            Definition::Sign(s) => s.items.clone(),
            Definition::Paradigm(_) => vec![],
        };
        bind_mount_arguments(&mut items, self)?;
        let mut mounted = BTreeSet::new();
        for item in &items {
            if let SignItem::TraitMount { name, args, .. } = item {
                let base = TypeRef::mount(name, args)?.substitute(&bindings);
                let base = self.resolve_inner(&base, owner, active)?.0;
                // `belongs T` plus T's individual blocks names one member family.
                if mounted.insert(base.clone()) {
                    for (name, member) in self.collect(&base, active)? {
                        insert(name, member)?;
                    }
                }
            }
        }
        active.pop();
        Ok(result)
    }

    /// Follow receiver member aliases without copying their source declarations.
    /// Every edge checks its receiver's visibility before reaching the source member.
    pub(crate) fn resolve(
        &self,
        reference: &TypeRef,
        caller: &TypeRef,
    ) -> Result<(TypeRef, bool), String> {
        self.resolve_inner(reference, caller, &mut Vec::new())
    }

    fn resolve_inner(
        &self,
        reference: &TypeRef,
        caller: &TypeRef,
        active: &mut Vec<String>,
    ) -> Result<(TypeRef, bool), String> {
        let mut resolved = self.normalize_arguments(reference)?;
        let mut alias = false;
        for end in 1..reference.segments.len() {
            let owner = TypeRef {
                segments: resolved.segments[..end].to_vec(),
            };
            let Some(_) = self.owner(&owner.path()) else {
                continue;
            };
            let name = resolved.segments[end].name.clone();
            // Direct declarations are handled by the compiler's ordinary visibility checks.
            let direct = format!("{}.{}", owner.path(), name);
            if self.catalog.keys().any(|(_, path)| *path == direct) {
                continue;
            }
            if let Some(member) = self.collect(&owner, active)?.get(&name) {
                if !member.exported && !inside(caller, &owner) {
                    return Err(failure(
                        "NESTED_PRIVATE_MEMBER",
                        format!("{caller} cannot reference {reference}"),
                    ));
                }
                let mut target = member.reference.clone();
                target.segments.last_mut().unwrap().arguments =
                    resolved.segments[end].arguments.clone();
                target
                    .segments
                    .extend(resolved.segments.iter().skip(end + 1).cloned());
                resolved = target;
                alias = true;
                // Aliases may change the number of path segments; restart from their source.
                let (target, _) = self.resolve_inner(&resolved, &owner, active)?;
                return Ok((target, alias));
            }
        }
        Ok((resolved, alias))
    }

    pub(crate) fn grants(&self, caller: &TypeRef, member_owner: &TypeRef) -> Result<bool, String> {
        let caller = &self.normalize_arguments(caller)?;
        let member_owner = &self.normalize_arguments(member_owner)?;
        for end in (1..=caller.segments.len()).rev() {
            let scope = TypeRef {
                segments: caller.segments[..end].to_vec(),
            };
            if inside(&scope, member_owner) {
                return Ok(true);
            }
            if self.members(&scope)?.values().any(|m| {
                let mut parent = m.reference.clone();
                parent.segments.pop();
                parent == *member_owner
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn inside(caller: &TypeRef, scope: &TypeRef) -> bool {
    caller.segments.starts_with(&scope.segments)
}
