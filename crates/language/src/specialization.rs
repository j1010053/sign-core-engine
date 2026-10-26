//! Lexical binding and concrete type instantiation, before ontology construction.
mod member_scope;
pub(crate) use member_scope::MemberScopes;

use crate::package_scope::RefKind;
use crate::type_ref::{TypeRef, TypeSegment};
use crate::{Language, LibraryExportKind as Kind, SignItem, TraitDef, TraitTypeParam};
use std::collections::{BTreeMap, BTreeSet};

fn failure(code: &str, message: impl std::fmt::Display) -> String {
    format!("{code}: {message}")
}

/// A captured argument has already passed access checks at its application site.
/// This grants use of that exact type, never access to its private members.
fn captured_argument(caller: &TypeRef, reference: &TypeRef) -> bool {
    caller
        .segments
        .iter()
        .flat_map(|segment| &segment.arguments)
        .any(|argument| {
            argument.value == *reference || captured_argument(&argument.value, reference)
        })
}

/// Both scope composition and content expansion use the same implicit mount binding.
fn bind_mount_arguments(items: &mut [SignItem], scopes: &MemberScopes) -> Result<(), String> {
    let declarations = items
        .iter()
        .filter_map(|item| match item {
            SignItem::TraitMount { name, kind, args } if kind.is_declaration() => Some(
                TypeRef::mount(name, args)
                    .and_then(|r| scopes.normalize_arguments(&r))
                    .map(TypeRef::split_mount),
            ),
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;
    for item in items {
        if let SignItem::TraitMount { name, kind, args } = item {
            if !kind.is_declaration() && args.is_empty() {
                let normalized = scopes
                    .normalize_arguments(&TypeRef::parse(name)?)?
                    .to_string();
                let candidates = declarations
                    .iter()
                    .filter(|(n, _)| *n == normalized)
                    .map(|(_, a)| a)
                    .collect::<BTreeSet<_>>();
                if candidates.len() > 1 {
                    return Err(failure("TYPE_ARGUMENT_AMBIGUOUS", name));
                }
                if let Some(candidate) = candidates.first() {
                    *args = (*candidate).clone();
                }
            }
        }
    }
    Ok(())
}

/// Qualify lexical member references without granting lexical access at call sites.
/// Parameter arguments remain structural; feature names and phon literals are untouched.
pub(crate) fn qualify_members(source: &Language) -> Result<Language, String> {
    qualify_members_with(source, &|_, _| Ok(vec![]))
}

type InheritedNames<'a> = dyn Fn(&TypeRef, &BTreeSet<String>) -> Result<Vec<String>, String> + 'a;
pub(crate) fn qualify_members_with(
    source: &Language,
    inherited_names: &InheritedNames<'_>,
) -> Result<Language, String> {
    fn bind(name: &mut String, scope: &BTreeMap<String, TypeRef>) -> Result<(), String> {
        fn walk(value: &mut TypeRef, scope: &BTreeMap<String, TypeRef>) -> Result<(), String> {
            for segment in &mut value.segments {
                for arg in &mut segment.arguments {
                    walk(&mut arg.value, scope)?;
                }
            }
            if let Some(prefix) = scope.get(&value.segments[0].name) {
                let mut prefix = prefix.clone();
                prefix.segments.last_mut().unwrap().arguments = value.segments[0].arguments.clone();
                prefix
                    .segments
                    .extend(value.segments.iter().skip(1).cloned());
                *value = prefix;
            }
            Ok(())
        }
        let mut value = TypeRef::parse(name)?;
        walk(&mut value, scope)?;
        *name = value.to_string();
        Ok(())
    }
    fn child_path(parent: Option<&TypeRef>, name: &str) -> Result<TypeRef, String> {
        if let Some(parent) = parent {
            let mut full = parent.clone();
            full.segments.push(TypeSegment {
                name: name.into(),
                arguments: vec![],
            });
            Ok(full)
        } else {
            TypeRef::parse(name)
        }
    }
    fn member_scope(
        members: &Language,
        full: &TypeRef,
        inherited: &BTreeMap<String, TypeRef>,
    ) -> BTreeMap<String, TypeRef> {
        let mut scope = inherited.clone();
        for (_, name, _) in crate::package_scope::declarations(members) {
            let mut path = full.clone();
            path.segments.push(TypeSegment {
                name: name.into(),
                arguments: vec![],
            });
            scope.insert(name.into(), path);
        }
        scope
    }
    fn descend(
        language: &mut Language,
        parent: Option<&TypeRef>,
        inherited: &BTreeMap<String, TypeRef>,
        captured: &BTreeSet<String>,
        inherited_names: &InheritedNames<'_>,
    ) -> Result<(), String> {
        for name in language
            .traits
            .iter()
            .map(|v| &v.name)
            .chain(language.signs.iter().map(|v| &v.name))
            .chain(language.paradigms.iter().map(|v| &v.name))
        {
            if name.starts_with("__type_instance_") || name.starts_with("__formal_") {
                return Err(failure("TYPE_INTERNAL_NAME", name));
            }
        }
        if parent.is_some() && (!language.dsl_decls.is_empty() || !language.distribution.is_empty())
        {
            return Err(failure(
                "NESTED_DECLARATION_CONTENT",
                "nested scopes contain declarations only",
            ));
        }
        let mut scope = inherited.clone();
        if let Some(parent) = parent {
            for name in language
                .traits
                .iter()
                .map(|v| &v.name)
                .chain(language.signs.iter().map(|v| &v.name))
                .chain(language.paradigms.iter().map(|v| &v.name))
            {
                if !crate::parser::symbol_ok(name) || name.contains('.') || captured.contains(name)
                {
                    return Err(failure(
                        "NESTED_DECLARATION_NAME",
                        format!("invalid or shadowed member {name:?}"),
                    ));
                }
                let mut full = parent.clone();
                full.segments.push(TypeSegment {
                    name: name.clone(),
                    arguments: vec![],
                });
                scope.insert(name.clone(), full);
            }
        }
        for value in &mut language.traits {
            let mut parameters = captured.clone();
            let own_parameters = value
                .type_params
                .iter()
                .map(|p| p.name.clone())
                .collect::<BTreeSet<_>>();
            for parameter in &mut value.type_params {
                if let Some(bound) = &parameter.bound {
                    fn forward(
                        reference: &TypeRef,
                        own: &BTreeSet<String>,
                        visible: &BTreeSet<String>,
                    ) -> bool {
                        (own.contains(&reference.segments[0].name)
                            && !visible.contains(&reference.segments[0].name))
                            || reference
                                .segments
                                .iter()
                                .flat_map(|s| &s.arguments)
                                .any(|a| forward(&a.value, own, visible))
                    }
                    if forward(&TypeRef::parse(bound)?, &own_parameters, &parameters) {
                        return Err(failure(
                            "TYPE_PARAM_FORWARD_BOUND",
                            format!(
                                "{}: bounds can reference only earlier parameters",
                                value.name
                            ),
                        ));
                    }
                }
                if !parameters.insert(parameter.name.clone()) {
                    return Err(failure(
                        "TYPE_PARAM_DUPLICATE_NAME",
                        format!(
                            "{} shadows or repeats parameter {}",
                            value.name, parameter.name
                        ),
                    ));
                }
                if scope.contains_key(&parameter.name) {
                    return Err(failure("TYPE_PARAM_SHADOWING", &parameter.name));
                }
                if let Some(bound) = &mut parameter.bound {
                    bind(bound, &scope)?;
                }
            }
            if value.global && (!value.type_params.is_empty() || parent.is_some()) {
                return Err(failure("TYPE_PARAM_ON_GLOBAL_TRAIT", &value.name));
            }
            let mut full = if let Some(parent) = parent {
                let mut full = parent.clone();
                full.segments.push(TypeSegment {
                    name: value.name.clone(),
                    arguments: vec![],
                });
                full
            } else {
                TypeRef::parse(&value.name)?
            };
            full.segments.last_mut().unwrap().arguments = value
                .type_params
                .iter()
                .map(|p| TypeRef::parse(&p.name).map(Into::into))
                .collect::<Result<_, _>>()?;
            let mut body_scope = member_scope(&value.members, &full, &scope);
            for name in inherited_names(&full, &parameters)? {
                let mut path = full.clone();
                path.segments.push(TypeSegment {
                    name: name.clone(),
                    arguments: vec![],
                });
                body_scope.insert(name, path);
            }
            for block in &mut value.blocks {
                crate::package_scope::visit_items(&mut block.items, &mut |name, _| {
                    bind(name, &body_scope)
                })?;
            }
            descend(
                &mut value.body.members,
                Some(&full),
                &body_scope,
                &parameters,
                inherited_names,
            )?;
        }
        for value in &mut language.signs {
            let full = child_path(parent, &value.name)?;
            let mut body_scope = member_scope(&value.members, &full, &scope);
            for name in inherited_names(&full, captured)? {
                let mut path = full.clone();
                path.segments.push(TypeSegment {
                    name: name.clone(),
                    arguments: vec![],
                });
                body_scope.insert(name, path);
            }
            crate::package_scope::visit_items(&mut value.items, &mut |name, _| {
                bind(name, &body_scope)
            })?;
            descend(
                &mut value.body.members,
                Some(&full),
                &body_scope,
                captured,
                inherited_names,
            )?;
        }
        for value in &mut language.paradigms {
            crate::package_scope::visit_case(&mut value.selector, &mut |name, _| {
                bind(name, &scope)
            })?;
        }
        Ok(())
    }
    let mut source = source.clone();
    descend(
        &mut source,
        None,
        &BTreeMap::new(),
        &BTreeSet::new(),
        inherited_names,
    )?;
    Ok(source)
}

#[derive(Clone, Debug)]
enum Definition {
    Trait(TraitDef),
    Sign(crate::SignDef),
    Paradigm(crate::ParadigmDef),
}

#[derive(Clone, Debug)]
struct Level {
    path: String,
    parameters: Vec<TraitTypeParam>,
    exported: bool,
    kind: Kind,
}

#[derive(Clone, Debug)]
struct Template {
    levels: Vec<Level>,
    definition: Definition,
    standard: bool,
}
impl Template {
    fn kind(&self) -> Kind {
        match self.definition {
            Definition::Trait(_) => Kind::Trait,
            Definition::Sign(_) => Kind::Sign,
            Definition::Paradigm(_) => Kind::Paradigm,
        }
    }
    fn generic(&self) -> bool {
        self.levels.iter().any(|l| !l.parameters.is_empty())
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct InstanceIndex {
    scopes: MemberScopes,
    names: BTreeMap<(Kind, String), String>,
    spellings: BTreeMap<(Kind, String), String>,
    private: BTreeSet<(Kind, String)>,
    /// Runtime instances retain the source identity without allocating source nodes.
    pub origins: BTreeMap<crate::SignId, crate::SignId>,
}
impl InstanceIndex {
    pub(crate) fn external(&self, name: &str, kind: impl Into<RefKind>) -> Result<String, String> {
        let kind = kind.into();
        if name.starts_with("__type_instance_") || name.starts_with("__formal_") {
            return Err(failure(
                "TYPE_INTERNAL_NAME",
                "compiler instance keys are not source names",
            ));
        }
        let mut reference = TypeRef::parse(name)?;
        // Arguments are references too: validate their visibility even when the
        // enclosing specialization was already materialized by an inner scope.
        for segment in &mut reference.segments {
            for argument in &mut segment.arguments {
                let key = self.external(&argument.value.to_string(), RefKind::Type)?;
                if let Some(spelling) = self
                    .spellings
                    .get(&(Kind::Trait, key.clone()))
                    .or_else(|| self.spellings.get(&(Kind::Sign, key)))
                {
                    argument.value = TypeRef::parse(spelling)?;
                }
            }
        }
        let canonical = self.scopes.normalize_arguments(&reference)?.to_string();
        let kinds = [Kind::Trait, Kind::Sign, Kind::Paradigm]
            .into_iter()
            .filter(|k| kind.accepts(*k))
            .collect::<Vec<_>>();
        if kinds
            .iter()
            .filter(|k| self.names.contains_key(&(**k, canonical.clone())))
            .count()
            > 1
        {
            return Err(failure("TYPE_REFERENCE_AMBIGUOUS", canonical));
        }
        for k in kinds {
            if self.private.contains(&(k, canonical.clone())) {
                return Err(failure("NESTED_PRIVATE_MEMBER", canonical));
            }
            if let Some(key) = self.names.get(&(k, canonical.clone())) {
                return Ok(key.clone());
            }
        }
        let (target, alias) = self
            .scopes
            .resolve(&TypeRef::parse(&canonical)?, &TypeRef::parse("__external")?)?;
        if alias {
            return self.external(&target.to_string(), kind);
        }
        Ok(canonical)
    }
    pub(crate) fn spelling(&self, key: &str, kind: Kind) -> Option<String> {
        let spelling = self
            .spellings
            .get(&(kind, key.to_owned()))
            .cloned()
            .unwrap_or_else(|| key.into());
        (!self.private.contains(&(kind, spelling.clone()))).then_some(spelling)
    }
}

pub(crate) struct Prepared {
    pub standard: Language,
    pub language: Language,
    pub abstract_validation: Option<(Language, Language)>,
    pub index: InstanceIndex,
}

fn hex(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn catalog(
    standard: &Language,
    language: &Language,
) -> Result<BTreeMap<(Kind, String), Template>, String> {
    fn collect(
        l: &Language,
        parents: &[Level],
        standard: bool,
        out: &mut BTreeMap<(Kind, String), Template>,
    ) -> Result<(), String> {
        for (name, exported, parameters, definition) in l
            .traits
            .iter()
            .map(|t| {
                (
                    &t.name,
                    t.exported,
                    t.type_params.clone(),
                    Definition::Trait(t.clone()),
                )
            })
            .chain(
                l.signs
                    .iter()
                    .map(|s| (&s.name, s.exported, vec![], Definition::Sign(s.clone()))),
            )
            .chain(
                l.paradigms
                    .iter()
                    .map(|p| (&p.name, p.exported, vec![], Definition::Paradigm(p.clone()))),
            )
        {
            let path = parents
                .last()
                .map(|p| format!("{}.{name}", p.path))
                .unwrap_or_else(|| name.clone());
            let mut levels = parents.to_vec();
            levels.push(Level {
                path: path.clone(),
                parameters,
                exported,
                kind: match &definition {
                    Definition::Trait(_) => Kind::Trait,
                    Definition::Sign(_) => Kind::Sign,
                    Definition::Paradigm(_) => Kind::Paradigm,
                },
            });
            let template = Template {
                levels: levels.clone(),
                definition: definition.clone(),
                standard,
            };
            let kind = template.kind();
            if out.insert((kind, path.clone()), template).is_some() {
                return Err(failure(
                    if !parents.is_empty() {
                        "TEMPLATE_MEMBER_CONFLICT"
                    } else if kind == Kind::Trait {
                        "ONTOLOGY_DUPLICATE_TRAIT"
                    } else {
                        "SIGN_DUPLICATE"
                    },
                    path,
                ));
            }
            match definition {
                Definition::Trait(t) => collect(&t.members, &levels, standard, out)?,
                Definition::Sign(s) => collect(&s.members, &levels, standard, out)?,
                Definition::Paradigm(_) => {}
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    collect(standard, &[], true, &mut result)?;
    collect(language, &[], false, &mut result)?;
    Ok(result)
}

struct Lowerer<'a> {
    catalog: &'a BTreeMap<(Kind, String), Template>,
    scopes: MemberScopes,
    standard: Language,
    language: Language,
    index: InstanceIndex,
    active: Vec<String>,
    formal: BTreeSet<String>,
    bounds: Vec<(String, String, String)>,
}

impl<'a> Lowerer<'a> {
    fn new(
        catalog: &'a BTreeMap<(Kind, String), Template>,
        std: &Language,
        source: &Language,
    ) -> Self {
        let empty = |l: &Language| {
            let mut result = l.clone();
            result.traits.clear();
            result.signs.clear();
            result.paradigms.clear();
            result
        };
        Self {
            catalog,
            scopes: MemberScopes {
                catalog: catalog.clone(),
            },
            standard: empty(std),
            language: empty(source),
            index: InstanceIndex {
                scopes: MemberScopes {
                    catalog: catalog.clone(),
                },
                ..Default::default()
            },
            active: vec![],
            formal: BTreeSet::new(),
            bounds: vec![],
        }
    }
    fn template(&self, path: &str, kind: Kind) -> Option<&Template> {
        self.catalog.get(&(kind, path.into())).or_else(|| {
            (kind == Kind::Sign)
                .then(|| self.catalog.get(&(Kind::Paradigm, path.into())))
                .flatten()
        })
    }
    fn reference(
        &mut self,
        reference: &TypeRef,
        kind: impl Into<RefKind>,
        owner: &str,
    ) -> Result<String, String> {
        let requested = kind.into();
        let caller = TypeRef::parse(owner)?;
        let (resolved, alias) = self.scopes.resolve(reference, &caller)?;
        let kind = match requested {
            RefKind::Declaration(kind) => kind,
            RefKind::Type => {
                let trait_type = self.catalog.contains_key(&(Kind::Trait, resolved.path()));
                let sign_type = self.catalog.contains_key(&(Kind::Sign, resolved.path()));
                if trait_type && sign_type {
                    return Err(failure("TYPE_REFERENCE_AMBIGUOUS", reference));
                }
                if sign_type {
                    Kind::Sign
                } else {
                    Kind::Trait
                }
            }
        };
        if alias {
            let spelling = reference.to_string();
            let key = self.reference(&resolved, kind, owner)?;
            let actual_kind = self
                .template(&resolved.path(), kind)
                .map(Template::kind)
                .unwrap_or(kind);
            self.index
                .names
                .insert((actual_kind, spelling.clone()), key.clone());
            if self
                .scopes
                .resolve(reference, &TypeRef::parse("__external")?)
                .is_err()
            {
                self.index.private.insert((actual_kind, spelling));
            }
            return Ok(key);
        }
        let reference = &resolved;
        let path = reference.path();
        if (path.starts_with("__type_instance_") || path.starts_with("__formal_"))
            && !self.formal.contains(&path)
        {
            return Err(failure("TYPE_INTERNAL_NAME", reference));
        }
        let Some(template) = self.template(&path, kind).cloned() else {
            if kind == Kind::Trait && self.catalog.contains_key(&(Kind::Sign, path.clone())) {
                return Err(failure(
                    "TYPE_REFERENCE_KIND",
                    format!("{reference} is a Sign instance type, not a trait template"),
                ));
            }
            if reference.segments.iter().any(|s| !s.arguments.is_empty()) {
                return Err(failure("TYPE_REFERENCE_UNKNOWN", reference));
            }
            // Preserve established unknown-slot/role/callee diagnostics downstream.
            return Ok(reference.to_string());
        };
        let mut bindings = BTreeMap::new();
        let mut canonical = reference.clone();
        let mut pending_bounds = Vec::new();
        for (level_index, level) in template.levels.iter().enumerate() {
            let segment_index = level.path.split('.').count() - 1;
            let segment = &reference.segments[segment_index];
            if level_index > 0 && !level.exported {
                let parent = TypeRef {
                    segments: reference.segments[..segment_index].to_vec(),
                };
                if !self.scopes.grants(&caller, &parent)? && !captured_argument(&caller, reference)
                {
                    return Err(failure(
                        "NESTED_PRIVATE_MEMBER",
                        format!("{owner} cannot reference {reference}"),
                    ));
                }
            }
            for (parameter, argument) in level
                .parameters
                .iter()
                .zip(segment.ordered_arguments(&level.parameters)?)
            {
                let argument_key = self.reference(&argument, RefKind::Type, owner)?;
                let (argument_target, _) = self.scopes.resolve(&argument, &caller)?;
                if self
                    .template(&argument_target.path(), Kind::Trait)
                    .is_none()
                    && !self
                        .catalog
                        .contains_key(&(Kind::Sign, argument_target.path()))
                    && !self.formal.contains(&argument_key)
                {
                    return Err(failure("TYPE_ARGUMENT_UNKNOWN", argument));
                }
                if let Some(bound) = &parameter.bound {
                    let bound = TypeRef::parse(bound)?.substitute(&bindings);
                    let bound_key = self.reference(&bound, Kind::Trait, &level.path)?;
                    if self.template(&bound.path(), Kind::Trait).is_none()
                        && !self.formal.contains(&bound_key)
                    {
                        return Err(failure("TYPE_PARAM_UNKNOWN_BOUND", bound));
                    }
                    pending_bounds.push((
                        argument_key.clone(),
                        bound_key,
                        format!("{reference}: {}: {bound}", parameter.name),
                    ));
                }
                let argument = self
                    .index
                    .spellings
                    .get(&(Kind::Trait, argument_key.clone()))
                    .or_else(|| self.index.spellings.get(&(Kind::Sign, argument_key)))
                    .map(|spelling| TypeRef::parse(spelling))
                    .transpose()?
                    .unwrap_or(argument_target);
                bindings.insert(parameter.name.clone(), argument);
            }
            canonical.segments[segment_index].arguments = level
                .parameters
                .iter()
                .map(|p| bindings[&p.name].clone().into())
                .collect();
        }
        // A package qualifier is not a parameterized owner.
        let root_end = template.levels[0].path.split('.').count() - 1;
        if canonical.segments[..root_end]
            .iter()
            .any(|s| !s.arguments.is_empty())
        {
            return Err(failure("TYPE_PARAM_ARITY_MISMATCH", reference));
        }
        let spelling = canonical.to_string();
        let kind = template.kind();
        if let Some(key) = self.index.names.get(&(kind, spelling.clone())).cloned() {
            self.index
                .names
                .insert((kind, reference.to_string()), key.clone());
            return Ok(key);
        }
        let earlier = self
            .active
            .iter()
            .filter(|active| TypeRef::parse(active).is_ok_and(|r| r.path() == path))
            .collect::<Vec<_>>();
        if earlier.len() >= 2
            && earlier
                .last()
                .is_some_and(|previous| spelling.len() > previous.len())
        {
            return Err(failure(
                "TYPE_INSTANTIATION_CYCLE",
                format!(
                    "growing specialization: {} -> {spelling}",
                    self.active.join(" -> ")
                ),
            ));
        }
        if self.active.len() >= 64 || self.index.names.len() >= 16384 {
            return Err(failure(
                "TYPE_INSTANTIATION_CYCLE",
                format!("non-finite or excessively deep specialization at {spelling}"),
            ));
        }
        let specialized = template.generic() || template.levels.len() > 1;
        let key = if specialized {
            format!("__type_instance_{}", hex(&spelling))
        } else {
            spelling.clone()
        };
        self.index
            .names
            .insert((kind, spelling.clone()), key.clone());
        self.index
            .names
            .insert((kind, reference.to_string()), key.clone());
        self.index
            .spellings
            .insert((kind, key.clone()), spelling.clone());
        if template.levels.iter().skip(1).any(|l| !l.exported) {
            self.index.private.insert((kind, spelling.clone()));
        }
        self.bounds.extend(pending_bounds);
        self.active.push(spelling.clone());
        let owner_path = &template.levels.last().unwrap().path;
        let owner = &canonical.to_string();
        if template.levels.len() > 1 {
            let mut parent = canonical.clone();
            parent.segments.pop();
            self.reference(
                &parent,
                template.levels[template.levels.len() - 2].kind,
                owner,
            )?;
        }
        let mut definition = template.definition.clone();
        match &mut definition {
            Definition::Trait(t) => {
                t.name = key.clone();
                t.type_params.clear();
                t.members = Language::new();
                for block in &mut t.blocks {
                    self.items(&mut block.items, &bindings, owner)?;
                }
            }
            Definition::Sign(s) => {
                s.name = key.clone();
                s.body.members = Language::new();
                self.items(&mut s.items, &bindings, owner)?;
                if specialized {
                    let source = s.id.clone();
                    s.id = crate::SignId(crate::NodeId::new(
                        crate::IdentityNamespace::Instantiation {
                            source: Box::new(source.0.clone()),
                            arguments: spelling.clone(),
                        },
                        0,
                    ));
                    self.index.origins.insert(s.id.clone(), source);
                }
            }
            Definition::Paradigm(p) => {
                p.name = key.clone();
                crate::package_scope::visit_case(&mut p.selector, &mut |name, kind| {
                    *name =
                        self.reference(&TypeRef::parse(name)?.substitute(&bindings), kind, owner)?;
                    Ok(())
                })?;
                if specialized {
                    p.id = crate::ParadigmId(crate::NodeId::new(
                        crate::IdentityNamespace::Instantiation {
                            source: Box::new(p.id.0.clone()),
                            arguments: spelling.clone(),
                        },
                        0,
                    ));
                }
            }
        }
        let target = if template.standard {
            &mut self.standard
        } else {
            &mut self.language
        };
        match definition {
            Definition::Trait(t) => target.traits.push(t),
            Definition::Sign(s) => target.signs.push(s),
            Definition::Paradigm(p) => target.paradigms.push(p),
        }
        // Binding an owner activates its captured, non-generic member family.
        let children = self
            .catalog
            .values()
            .filter(|t| {
                t.levels.len() == template.levels.len() + 1
                    && t.levels[t.levels.len() - 2].path == *owner_path
                    && t.levels.last().unwrap().parameters.is_empty()
            })
            .cloned()
            .collect::<Vec<_>>();
        for child in children {
            let mut child_ref = canonical.clone();
            child_ref.segments.push(TypeSegment {
                name: child
                    .levels
                    .last()
                    .unwrap()
                    .path
                    .rsplit('.')
                    .next()
                    .unwrap()
                    .into(),
                arguments: vec![],
            });
            self.reference(&child_ref, child.kind(), owner)?;
        }
        for (name, member) in self.scopes.members(&canonical)? {
            let Some(child) = self.template(&member.reference.path(), member.kind) else {
                continue;
            };
            if !child.levels.last().unwrap().parameters.is_empty() {
                continue;
            }
            let mut alias = canonical.clone();
            alias.segments.push(TypeSegment {
                name,
                arguments: vec![],
            });
            if alias != member.reference {
                self.reference(&alias, member.kind, owner)?;
            }
        }
        self.active.pop();
        Ok(key)
    }
    fn items(
        &mut self,
        items: &mut [SignItem],
        bindings: &BTreeMap<String, TypeRef>,
        owner: &str,
    ) -> Result<(), String> {
        bind_mount_arguments(items, &self.scopes)?;
        crate::package_scope::visit_items(items, &mut |name, kind| {
            *name = self.reference(&TypeRef::parse(name)?.substitute(bindings), kind, owner)?;
            Ok(())
        })
    }
    fn roots(&mut self) -> Result<(), String> {
        for template in self
            .catalog
            .values()
            .filter(|t| t.levels.len() == 1 && !t.generic())
            .cloned()
            .collect::<Vec<_>>()
        {
            let path = &template.levels[0].path;
            self.reference(&TypeRef::parse(path)?, template.kind(), path)?;
        }
        Ok(())
    }
    fn abstract_roots(&mut self) -> Result<(), String> {
        for template in self
            .catalog
            .values()
            .filter(|t| t.generic())
            .cloned()
            .collect::<Vec<_>>()
        {
            let mut reference = TypeRef::parse(&template.levels.last().unwrap().path)?;
            let mut bindings = BTreeMap::new();
            for level in &template.levels {
                let segment_index = level.path.split('.').count() - 1;
                for parameter in &level.parameters {
                    let name = format!(
                        "__formal_{}",
                        hex(&format!("{}:{}", level.path, parameter.name))
                    );
                    if self.formal.insert(name.clone()) {
                        let mut blocks = vec![crate::Block::default()];
                        if let Some(bound) = &parameter.bound {
                            let bound = TypeRef::parse(bound)?.substitute(&bindings);
                            let bound_key = self.reference(&bound, Kind::Trait, &level.path)?;
                            if self.template(&bound.path(), Kind::Trait).is_none()
                                && !self.formal.contains(&bound_key)
                            {
                                return Err(failure("TYPE_PARAM_UNKNOWN_BOUND", bound));
                            }
                            blocks[0].items.push(SignItem::TraitMount {
                                name: bound_key,
                                kind: crate::TraitMountKind::Declaration,
                                args: vec![],
                            });
                        }
                        // A bounded formal is a classification node, not a marker (it has a parent).
                        self.language.traits.push(TraitDef {
                            exported: false,
                            name: name.clone(),
                            global: false,
                            marker: false,
                            type_params: vec![],
                            body: (blocks, Language::new()).into(),
                        });
                    }
                    let argument = TypeRef::parse(&name)?;
                    bindings.insert(parameter.name.clone(), argument.clone());
                    reference.segments[segment_index]
                        .arguments
                        .push(argument.into());
                }
            }
            // Validation executes within the declaration's own lexical scope.
            self.reference(&reference, template.kind(), &reference.to_string())?;
        }
        Ok(())
    }
    fn check_bounds(&self) -> Result<(), String> {
        let (registry, _) =
            crate::ontology::OntologyRegistry::build(&[&self.standard, &self.language]);
        for (argument, bound, context) in &self.bounds {
            if !registry.type_is_within(argument, bound) {
                return Err(failure("TYPE_PARAM_BOUND_VIOLATION", context));
            }
        }
        Ok(())
    }
}

pub(crate) fn prepare(standard: &Language, language: &Language) -> Result<Prepared, String> {
    let scopes = MemberScopes::new(standard, language)?;
    let bind =
        |owner: &TypeRef, _: &BTreeSet<String>| Ok(scopes.members(owner)?.into_keys().collect());
    let standard = qualify_members_with(standard, &bind)?;
    let language = qualify_members_with(language, &bind)?;
    let (standard, language) = (&standard, &language);
    let catalog = catalog(standard, language)?;
    let mut ground = Lowerer::new(&catalog, standard, language);
    ground.roots()?;
    ground.check_bounds()?;
    let abstract_validation = if catalog.values().any(Template::generic) {
        let mut abstract_types = Lowerer::new(&catalog, standard, language);
        abstract_types.roots()?;
        abstract_types.abstract_roots()?;
        abstract_types.check_bounds()?;
        Some((abstract_types.standard, abstract_types.language))
    } else {
        None
    };
    Ok(Prepared {
        standard: ground.standard,
        language: ground.language,
        index: ground.index,
        abstract_validation,
    })
}

pub(crate) fn diagnostic(message: String) -> crate::Diagnostic {
    let code = match message.split_once(": ").map(|(code, _)| code) {
        Some("TEMPLATE_MEMBER_CONFLICT") => "TEMPLATE_MEMBER_CONFLICT",
        Some("TEMPLATE_MEMBER_CYCLE") => "TEMPLATE_MEMBER_CYCLE",
        Some("NESTED_DECLARATION_NAME") => "NESTED_DECLARATION_NAME",
        Some("NESTED_PRIVATE_MEMBER") => "NESTED_PRIVATE_MEMBER",
        Some("TYPE_PARAM_DUPLICATE_NAME") => "TYPE_PARAM_DUPLICATE_NAME",
        Some("TYPE_PARAM_SHADOWING") => "TYPE_PARAM_SHADOWING",
        Some("TYPE_PARAM_FORWARD_BOUND") => "TYPE_PARAM_FORWARD_BOUND",
        Some("TYPE_INTERNAL_NAME") => "TYPE_INTERNAL_NAME",
        Some("NESTED_DECLARATION_CONTENT") => "NESTED_DECLARATION_CONTENT",
        Some("TYPE_PARAM_ON_GLOBAL_TRAIT") => "TYPE_PARAM_ON_GLOBAL_TRAIT",
        Some("TYPE_PARAM_ARITY_MISMATCH") => "TYPE_PARAM_ARITY_MISMATCH",
        Some("TYPE_PARAM_UNKNOWN_BOUND") => "TYPE_PARAM_UNKNOWN_BOUND",
        Some("TYPE_PARAM_BOUND_VIOLATION") => "TYPE_PARAM_BOUND_VIOLATION",
        Some("TYPE_REFERENCE_UNKNOWN") => "TYPE_REFERENCE_UNKNOWN",
        Some("TYPE_ARGUMENT_UNKNOWN") => "TYPE_ARGUMENT_UNKNOWN",
        Some("TYPE_ARGUMENT_UNKNOWN_NAME") => "TYPE_ARGUMENT_UNKNOWN_NAME",
        Some("TYPE_ARGUMENT_DUPLICATE") => "TYPE_ARGUMENT_DUPLICATE",
        Some("TYPE_ARGUMENT_ORDER") => "TYPE_ARGUMENT_ORDER",
        Some("TYPE_ARGUMENT_AMBIGUOUS") => "TYPE_ARGUMENT_AMBIGUOUS",
        Some("TYPE_REFERENCE_AMBIGUOUS") => "TYPE_REFERENCE_AMBIGUOUS",
        Some("TYPE_REFERENCE_KIND") => "TYPE_REFERENCE_KIND",
        Some("TYPE_INSTANTIATION_CYCLE") => "TYPE_INSTANTIATION_CYCLE",
        Some("ONTOLOGY_DUPLICATE_TRAIT") => "ONTOLOGY_DUPLICATE_TRAIT",
        Some("SIGN_DUPLICATE") => "SIGN_DUPLICATE",
        _ => "TYPE_REFERENCE_INVALID",
    };
    crate::Diagnostic::new(crate::Severity::Error, code, message)
}

pub(crate) fn needs_lowering(language: &Language) -> bool {
    language.has_members() || language.traits.iter().any(|t| !t.type_params.is_empty())
}

/// Rename a declaration through structural references, including captured
/// member paths and type arguments. Parameters are lexical binders, not uses
/// of same-spelled global declarations.
pub(crate) fn rename_references(
    language: &mut Language,
    address: &crate::NodeAddress,
    new_name: &str,
) -> Result<(), String> {
    let Some(old) = crate::identity::declaration_name(language, address) else {
        return Ok(());
    };
    let (scope, relative) = language
        .scope_at_address(&address.0)
        .ok_or("invalid source scope")?;
    let (local_name, target_kind) = match relative {
        [crate::AddressSegment::Traits(i)] => (&scope.traits[*i].name, Kind::Trait),
        [crate::AddressSegment::Signs(i)] => (&scope.signs[*i].name, Kind::Sign),
        [crate::AddressSegment::Paradigms(i)] => (&scope.paradigms[*i].name, Kind::Sign),
        _ => return Ok(()),
    };
    let new = format!("{}{new_name}", &old[..old.len() - local_name.len()]);
    let old = TypeRef::parse(&old)?;
    let new = TypeRef::parse(&new)?;
    fn rewrite(
        value: &mut TypeRef,
        kind: RefKind,
        target: Kind,
        old: &TypeRef,
        new: &TypeRef,
        parameters: &BTreeSet<String>,
    ) {
        for segment in &mut value.segments {
            for argument in &mut segment.arguments {
                rewrite(
                    &mut argument.value,
                    RefKind::Type,
                    target,
                    old,
                    new,
                    parameters,
                );
            }
        }
        if parameters.contains(&value.segments[0].name) {
            return;
        }
        if value.segments.len() < old.segments.len() {
            return;
        }
        if !kind.accepts(target) && !(value.segments.len() > old.segments.len()) {
            return;
        }
        if !value
            .segments
            .iter()
            .zip(&old.segments)
            .all(|(a, b)| a.name == b.name)
        {
            return;
        }
        let mut prefix = new.clone();
        if old.segments.len() == new.segments.len() {
            for (to, from) in prefix.segments.iter_mut().zip(&value.segments) {
                to.arguments = from.arguments.clone();
            }
        } else {
            prefix.segments.last_mut().unwrap().arguments =
                value.segments[old.segments.len() - 1].arguments.clone();
        }
        prefix
            .segments
            .extend(value.segments.iter().skip(old.segments.len()).cloned());
        *value = prefix;
    }
    fn walk(
        language: &mut Language,
        parameters: &BTreeSet<String>,
        target: Kind,
        old: &TypeRef,
        new: &TypeRef,
    ) -> Result<(), String> {
        let bind = |name: &mut String, kind, parameters: &BTreeSet<String>| {
            let mut reference = TypeRef::parse(name)?;
            rewrite(&mut reference, kind, target, old, new, parameters);
            *name = reference.to_string();
            Ok(())
        };
        for value in &mut language.traits {
            let mut parameters = parameters.clone();
            parameters.extend(value.type_params.iter().map(|p| p.name.clone()));
            for parameter in &mut value.type_params {
                if let Some(bound) = &mut parameter.bound {
                    bind(bound, Kind::Trait.into(), &parameters)?;
                }
            }
            for block in &mut value.blocks {
                crate::package_scope::visit_items(&mut block.items, &mut |name, kind| {
                    bind(name, kind, &parameters)
                })?;
            }
            walk(&mut value.members, &parameters, target, old, new)?;
        }
        for value in &mut language.signs {
            crate::package_scope::visit_items(&mut value.items, &mut |name, kind| {
                bind(name, kind, parameters)
            })?;
            walk(&mut value.body.members, parameters, target, old, new)?;
        }
        for value in &mut language.paradigms {
            crate::package_scope::visit_case(&mut value.selector, &mut |name, kind| {
                bind(name, kind, parameters)
            })?;
        }
        Ok(())
    }
    let lexical = qualify_members(language)?;
    let scopes = MemberScopes::new(&Language::new(), &lexical)?;
    let mut result = qualify_members_with(language, &|owner, _| {
        Ok(scopes.members(owner)?.into_keys().collect())
    })?;
    for (kind, alias, source) in scopes.source_aliases(language)? {
        if source == old.path()
            && (kind == target_kind || (kind == Kind::Paradigm && target_kind == Kind::Sign))
        {
            let alias = TypeRef::parse(&alias)?;
            let mut renamed = alias.clone();
            renamed.segments.last_mut().unwrap().name = new_name.into();
            walk(&mut result, &BTreeSet::new(), target_kind, &alias, &renamed)?;
        }
    }
    walk(&mut result, &BTreeSet::new(), target_kind, &old, &new)?;
    *language = result;
    Ok(())
}
