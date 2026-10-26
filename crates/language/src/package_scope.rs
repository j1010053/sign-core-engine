//! Package-local binding, export visibility and dot-qualified references.
//! Lowering works on detached ASTs and never changes source identities.
use std::collections::{BTreeMap, BTreeSet};

use crate::library::{LibraryExportKind as Kind, LibraryPackage, PackageId, PackageSpec};
use crate::{
    CaseCondition, Expression, Language, SignApplication, SignArgumentValue, SignItem,
    SlotConstraint, SlotMapOp, TypedCase,
};

const INTERNAL: &str = "__package_";

/// A type use accepts a trait category or a Sign's nominal instance type.
/// Content mounts and calls retain their distinct declaration capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefKind {
    Declaration(Kind),
    Type,
}

impl From<Kind> for RefKind {
    fn from(kind: Kind) -> Self {
        Self::Declaration(kind)
    }
}

impl RefKind {
    pub(crate) fn accepts(self, actual: Kind) -> bool {
        match self {
            Self::Type => matches!(actual, Kind::Trait | Kind::Sign),
            Self::Declaration(kind) => compatible(actual, kind),
        }
    }
}

#[derive(Debug, Clone)]
struct Symbol {
    package: PackageId,
    name: String,
    key: String,
    kind: Kind,
    exported: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PackageScope {
    symbols: Vec<Symbol>,
    member_scopes: crate::specialization::MemberScopes,
    aliases: BTreeMap<String, PackageId>,
    package_aliases: BTreeMap<PackageId, BTreeMap<String, PackageId>>,
    dependencies: BTreeMap<PackageId, BTreeSet<PackageId>>,
}

pub(crate) fn declarations(language: &Language) -> Vec<(Kind, &str, bool)> {
    language
        .traits
        .iter()
        .map(|v| (Kind::Trait, v.name.as_str(), v.exported))
        .chain(
            language
                .signs
                .iter()
                .map(|v| (Kind::Sign, v.name.as_str(), v.exported)),
        )
        .chain(
            language
                .paradigms
                .iter()
                .map(|v| (Kind::Paradigm, v.name.as_str(), v.exported)),
        )
        .collect()
}

fn compatible(actual: Kind, requested: Kind) -> bool {
    actual == requested || (requested == Kind::Sign && actual == Kind::Paradigm)
}

impl PackageScope {
    pub(crate) fn new(
        packages: &[LibraryPackage],
        languages: &[Language],
        spec: &PackageSpec,
    ) -> Result<Self, String> {
        let mut scope = Self::default();
        let mut counts = BTreeMap::new();
        let mut qualifiers = BTreeMap::<String, Vec<PackageId>>::new();
        for (package, language) in packages.iter().zip(languages) {
            qualifiers
                .entry(package.id.name.clone())
                .or_default()
                .push(package.id.clone());
            let mut local = BTreeSet::new();
            for (kind, name, exported) in declarations(language) {
                if !crate::parser::symbol_ok(name)
                    || name.contains('.')
                    || name.starts_with(INTERNAL)
                {
                    return Err(format!("PACKAGE_DECLARATION_INVALID: {} declares {name:?}; package declarations use bare names", package.id));
                }
                let group = if kind == Kind::Paradigm {
                    Kind::Sign
                } else {
                    kind
                };
                if !local.insert((group, name)) {
                    return Err(format!(
                        "PACKAGE_SYMBOL_DUPLICATE: {} declares {name:?} twice",
                        package.id
                    ));
                }
                if exported {
                    // Type positions share a trait/Sign namespace. Distinct
                    // packages must not collapse their qualified names merely
                    // because the declarations have different kinds.
                    *counts.entry(name.to_owned()).or_insert(0usize) += 1;
                }
                scope.symbols.push(Symbol {
                    package: package.id.clone(),
                    name: name.to_owned(),
                    key: name.to_owned(),
                    kind,
                    exported,
                });
            }
            scope.dependencies.insert(
                package.id.clone(),
                package.requires.iter().map(|r| r.id.clone()).collect(),
            );
        }
        // Retain legacy unique public spellings. Private and ambiguous symbols
        // receive collision-free internal keys; node IDs remain package-owned.
        for symbol in &mut scope.symbols {
            if !symbol.exported || counts.get(&symbol.name).copied().unwrap_or(0) != 1 {
                let encoded = symbol
                    .package
                    .to_string()
                    .bytes()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                symbol.key = format!("{INTERNAL}{encoded}.{}", symbol.name);
            }
        }
        for (name, ids) in qualifiers {
            if ids.len() == 1 {
                scope.aliases.insert(name, ids[0].clone());
            }
        }
        scope.aliases.extend(spec.aliases.clone());
        loop {
            let old = scope.dependencies.clone();
            for dependencies in scope.dependencies.values_mut() {
                let direct = dependencies.clone();
                for id in direct {
                    if let Some(transitive) = old.get(&id) {
                        dependencies.extend(transitive.iter().cloned());
                    }
                }
            }
            if old == scope.dependencies {
                break;
            }
        }
        for package in packages {
            // Caller-selected, unrelated roots cannot change a package's own
            // default qualifiers. Only this package and its dependencies count.
            let mut candidates = BTreeMap::<String, Vec<PackageId>>::new();
            for visible in packages
                .iter()
                .filter(|p| p.id == package.id || scope.dependencies[&package.id].contains(&p.id))
            {
                candidates
                    .entry(visible.id.name.clone())
                    .or_default()
                    .push(visible.id.clone());
            }
            let mut aliases: BTreeMap<_, _> = candidates
                .into_iter()
                .filter_map(|(name, ids)| (ids.len() == 1).then(|| (name, ids[0].clone())))
                .collect();
            for (alias, target) in &package.aliases {
                if target != &package.id && !scope.dependencies[&package.id].contains(target) {
                    return Err(format!(
                        "PACKAGE_ALIAS_TARGET_UNSELECTED: {} alias {alias:?} is not a dependency",
                        package.id
                    ));
                }
                aliases.insert(alias.clone(), target.clone());
            }
            scope.package_aliases.insert(package.id.clone(), aliases);
        }
        let mut headers = Language::new();
        for (package, language) in packages.iter().zip(languages) {
            let mut header = crate::specialization::qualify_members(language)?;
            scope.lower_headers(&mut header, Some(&package.id), &BTreeSet::new())?;
            scope.rename_roots(&mut header, &package.id)?;
            headers.append_library(header);
        }
        scope.member_scopes = crate::specialization::MemberScopes::new(&Language::new(), &headers)?;
        Ok(scope)
    }

    fn resolve(
        &self,
        name: &str,
        kind: impl Into<RefKind>,
        owner: Option<&PackageId>,
        locals: &BTreeSet<(Kind, String)>,
    ) -> Result<String, String> {
        let kind = kind.into();
        if name.starts_with(INTERNAL) {
            return Err(format!(
                "PACKAGE_PRIVATE_SYMBOL: {name:?} is an internal name"
            ));
        }
        if locals.iter().any(|(k, n)| kind.accepts(*k) && n == name) {
            if locals
                .iter()
                .filter(|(k, n)| kind.accepts(*k) && n == name)
                .count()
                > 1
            {
                return Err(format!("TYPE_REFERENCE_AMBIGUOUS: {name}"));
            }
            return Ok(name.to_owned());
        }
        if let Some(owner) = owner {
            if let Some(symbol) = self
                .symbols
                .iter()
                .find(|s| &s.package == owner && s.name == name && kind.accepts(s.kind))
            {
                if self
                    .symbols
                    .iter()
                    .filter(|s| &s.package == owner && s.name == name && kind.accepts(s.kind))
                    .count()
                    > 1
                {
                    return Err(format!("TYPE_REFERENCE_AMBIGUOUS: {name}"));
                }
                return Ok(symbol.key.clone());
            }
        }
        let (package, symbol_name) = if let Some((alias, symbol)) = name.split_once('.') {
            let aliases = owner
                .and_then(|o| self.package_aliases.get(o))
                .unwrap_or(&self.aliases);
            let package = aliases
                .get(alias)
                .ok_or_else(|| format!("PACKAGE_ALIAS_UNKNOWN: {alias:?} in {name:?}"))?;
            (Some(package), symbol)
        } else {
            (None, name)
        };
        let visible = |s: &&Symbol| {
            owner.is_none_or(|o| {
                &s.package == o
                    || self
                        .dependencies
                        .get(o)
                        .is_some_and(|deps| deps.contains(&s.package))
            })
        };
        let candidates: Vec<_> = self
            .symbols
            .iter()
            .filter(|s| {
                s.name == symbol_name
                    && kind.accepts(s.kind)
                    && package.is_none_or(|p| p == &s.package)
            })
            .filter(visible)
            .collect();
        let public: Vec<_> = candidates
            .iter()
            .filter(|s| s.exported || owner == Some(&s.package))
            .collect();
        match public.as_slice() {
            [symbol] => Ok(symbol.key.clone()),
            [] if !candidates.is_empty() => Err(format!("PACKAGE_PRIVATE_SYMBOL: {name:?} is not exported")),
            [] if package.is_some() => Err(format!("PACKAGE_SYMBOL_UNKNOWN: {name:?} is not an exported {kind:?} in an available dependency")),
            [] if owner.is_some() => Err(format!("PACKAGE_SYMBOL_UNKNOWN: {name:?} is not declared locally or exported by a dependency")),
            [] => Ok(name.to_owned()), // Keep caller unknown-name diagnostics.
            _ => Err(format!("PACKAGE_SYMBOL_AMBIGUOUS: {name:?}; use a package alias followed by `.`")),
        }
    }

    fn rename_roots(&self, result: &mut Language, owner: &PackageId) -> Result<(), String> {
        for value in &mut result.traits {
            value.name = self.resolve(&value.name, Kind::Trait, Some(owner), &BTreeSet::new())?;
        }
        for value in &mut result.signs {
            value.name = self.resolve(&value.name, Kind::Sign, Some(owner), &BTreeSet::new())?;
        }
        for value in &mut result.paradigms {
            value.name =
                self.resolve(&value.name, Kind::Paradigm, Some(owner), &BTreeSet::new())?;
        }
        Ok(())
    }

    /// Headers alone suffice to compose scopes before binding body references.
    fn lower_headers(
        &self,
        language: &mut Language,
        owner: Option<&PackageId>,
        locals: &BTreeSet<(Kind, String)>,
    ) -> Result<(), String> {
        fn mounts(
            scope: &PackageScope,
            items: &mut [SignItem],
            owner: Option<&PackageId>,
            locals: &BTreeSet<(Kind, String)>,
        ) -> Result<(), String> {
            for item in items {
                if let SignItem::TraitMount { name, args, .. } = item {
                    let full = crate::type_ref::TypeRef::mount(name, args)?;
                    let full = crate::type_ref::TypeRef::parse(&scope.resolve_reference(
                        &full.to_string(),
                        Kind::Trait,
                        owner,
                        locals,
                    )?)?;
                    let (n, a) = full.split_mount();
                    *name = n;
                    *args = a;
                }
            }
            Ok(())
        }
        for value in &mut language.traits {
            let parameters = locals
                .iter()
                .cloned()
                .chain(
                    value
                        .type_params
                        .iter()
                        .map(|p| (Kind::Trait, p.name.clone())),
                )
                .collect();
            for block in &mut value.blocks {
                mounts(self, &mut block.items, owner, &parameters)?;
            }
            self.lower_headers(&mut value.body.members, owner, &parameters)?;
        }
        for value in &mut language.signs {
            mounts(self, &mut value.items, owner, locals)?;
            self.lower_headers(&mut value.body.members, owner, locals)?;
        }
        Ok(())
    }

    fn qualify_scopes(
        &self,
        language: &Language,
        owner: Option<&PackageId>,
        locals: &BTreeSet<(Kind, String)>,
    ) -> Result<Language, String> {
        let mut headers = crate::specialization::qualify_members(language)?;
        self.lower_headers(&mut headers, owner, locals)?;
        if let Some(owner) = owner {
            self.rename_roots(&mut headers, owner)?;
        }
        let graph = self.member_scopes.with_language(&headers)?;
        crate::specialization::qualify_members_with(language, &|full, parameters| {
            let locals = locals
                .iter()
                .cloned()
                .chain(parameters.iter().map(|p| (Kind::Trait, p.clone())))
                .collect();
            let normalized = self.resolve_reference(
                &full.to_string(),
                self.owner_kind(&full.path(), owner, &locals)?,
                owner,
                &locals,
            )?;
            Ok(graph
                .members(&crate::type_ref::TypeRef::parse(&normalized)?)?
                .into_keys()
                .collect())
        })
    }

    pub(crate) fn lower_package(
        &self,
        language: &Language,
        owner: &PackageId,
    ) -> Result<Language, String> {
        crate::compile::check_global_trait_selection(language)
            .map_err(|error| error.to_string())?;
        let mut result = self.qualify_scopes(language, Some(owner), &BTreeSet::new())?;
        self.lower_contents(&mut result, Some(owner), &BTreeSet::new())?;
        self.rename_roots(&mut result, owner)?;
        // A private global trait must not become ambient in other packages.
        let private_globals: Vec<_> = result
            .traits
            .iter()
            .filter(|t| t.global && !t.exported)
            .map(|t| t.name.clone())
            .collect();
        for value in &mut result.traits {
            if !value.exported {
                value.global = false;
            }
        }
        for sign in &mut result.signs {
            for name in &private_globals {
                sign.items.insert(
                    0,
                    SignItem::TraitMount {
                        name: name.clone(),
                        kind: crate::TraitMountKind::Whole,
                        args: vec![],
                    },
                );
                sign.items.insert(
                    0,
                    SignItem::TraitMount {
                        name: name.clone(),
                        kind: crate::TraitMountKind::Declaration,
                        args: vec![],
                    },
                );
            }
        }
        Ok(result)
    }

    pub(crate) fn lower_caller(&self, language: &Language) -> Result<Language, String> {
        let locals = declarations(language)
            .into_iter()
            .filter(|(_, name, _)| !name.contains('.'))
            .map(|(k, n, _)| (k, n.to_owned()))
            .collect();
        let mut result = self.qualify_scopes(language, None, &locals)?;
        for (_, name, _) in declarations(language) {
            if name.starts_with(INTERNAL) {
                return Err(format!(
                    "PACKAGE_PRIVATE_SYMBOL: reserved declaration {name:?}"
                ));
            }
        }
        // Qualified declarations are explicit document-local overrides.
        for value in &mut result.traits {
            if value.name.contains('.') {
                value.name = self.resolve(&value.name, Kind::Trait, None, &BTreeSet::new())?;
            }
        }
        for value in &mut result.signs {
            if value.name.contains('.') {
                value.name = self.resolve(&value.name, Kind::Sign, None, &BTreeSet::new())?;
            }
        }
        for value in &mut result.paradigms {
            if value.name.contains('.') {
                value.name = self.resolve(&value.name, Kind::Paradigm, None, &BTreeSet::new())?;
            }
        }
        self.lower_contents(&mut result, None, &locals)?;
        Ok(result)
    }

    pub(crate) fn external_name(
        &self,
        name: &str,
        kind: impl Into<RefKind>,
        caller: &Language,
    ) -> Result<String, String> {
        let locals = declarations(caller)
            .into_iter()
            .filter(|(_, n, _)| !n.contains('.'))
            .map(|(k, n, _)| (k, n.to_owned()))
            .collect();
        self.resolve_reference(name, kind, None, &locals)
    }

    pub(crate) fn external_application(
        &self,
        application: &SignApplication,
        caller: &Language,
    ) -> Result<SignApplication, String> {
        let mut application = application.clone();
        visit_application(&mut application, &mut |name, kind| {
            *name = self.external_name(name, kind, caller)?;
            Ok(())
        })?;
        Ok(application)
    }

    pub(crate) fn public_spelling(&self, key: &str, kind: impl Into<RefKind>) -> Option<String> {
        let kind = kind.into();
        let mut reference = crate::type_ref::TypeRef::parse(key).ok()?;
        for segment in &mut reference.segments {
            for arg in &mut segment.arguments {
                arg.value = crate::type_ref::TypeRef::parse(
                    &self.public_spelling(&arg.value.to_string(), RefKind::Type)?,
                )
                .ok()?;
            }
        }
        let path = reference.path();
        if let Some(symbol) = self
            .symbols
            .iter()
            .filter(|s| path == s.key || path.starts_with(&format!("{}.", s.key)))
            .filter(|s| {
                if path == s.key {
                    kind.accepts(s.kind)
                } else {
                    matches!(s.kind, Kind::Trait | Kind::Sign)
                }
            })
            .max_by_key(|s| s.key.len())
        {
            if !symbol.exported {
                return None;
            }
            if symbol.key.starts_with(INTERNAL) {
                let alias = self.aliases.iter().find(|(_, p)| **p == symbol.package)?.0;
                let count = symbol.key.split('.').count();
                let args = reference.segments[count - 1].arguments.clone();
                let mut prefix =
                    crate::type_ref::TypeRef::parse(&format!("{alias}.{}", symbol.name)).ok()?;
                prefix.segments.last_mut()?.arguments = args;
                prefix
                    .segments
                    .extend(reference.segments.into_iter().skip(count));
                reference = prefix;
            }
        }
        Some(reference.to_string())
    }

    fn owner_kind(
        &self,
        root: &str,
        owner: Option<&PackageId>,
        locals: &BTreeSet<(Kind, String)>,
    ) -> Result<Kind, String> {
        let local = locals
            .iter()
            .filter(|(_, name)| name == root)
            .map(|(kind, _)| *kind)
            .collect::<Vec<_>>();
        if local.len() == 1 {
            return Ok(local[0]);
        }
        let mut kinds = BTreeSet::new();
        for kind in [Kind::Trait, Kind::Sign] {
            if let Ok(key) = self.resolve(root, kind, owner, locals) {
                if self.symbols.iter().any(|s| s.key == key && s.kind == kind) {
                    kinds.insert(kind);
                }
            }
        }
        if kinds.len() > 1 {
            return Err(format!("PACKAGE_SCOPE_AMBIGUOUS: {root}"));
        }
        Ok(kinds.into_iter().next().unwrap_or(Kind::Trait))
    }

    fn resolve_reference(
        &self,
        name: &str,
        kind: impl Into<RefKind>,
        owner: Option<&PackageId>,
        locals: &BTreeSet<(Kind, String)>,
    ) -> Result<String, String> {
        use crate::type_ref::{TypeRef, TypeSegment};
        let kind = kind.into();
        let mut reference = TypeRef::parse(name)?;
        for segment in &mut reference.segments {
            for arg in &mut segment.arguments {
                arg.value = TypeRef::parse(&self.resolve_reference(
                    &arg.value.to_string(),
                    RefKind::Type,
                    owner,
                    locals,
                )?)?;
            }
        }
        let first = &reference.segments[0].name;
        let aliases = owner
            .and_then(|p| self.package_aliases.get(p))
            .unwrap_or(&self.aliases);
        let count = if !locals
            .iter()
            .any(|(k, n)| n == first && (reference.segments.len() > 1 || kind.accepts(*k)))
            && aliases.contains_key(first)
            && reference.segments.len() > 1
        {
            2
        } else {
            1
        };
        let root = reference.segments[..count]
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(".");
        // Both Sign and trait declarations own member scopes.
        let root_kind = if reference.segments.len() > count {
            self.owner_kind(&root, owner, locals)?.into()
        } else {
            kind
        };
        let resolved = self.resolve(&root, root_kind, owner, locals)?;
        let args = reference.segments[count - 1].arguments.clone();
        if count == 2 && !reference.segments[0].arguments.is_empty() {
            return Err("package aliases cannot have type arguments".into());
        }
        let mut prefix: Vec<TypeSegment> = resolved
            .split('.')
            .map(|name| TypeSegment {
                name: name.into(),
                arguments: vec![],
            })
            .collect();
        prefix.last_mut().unwrap().arguments = args;
        prefix.extend(reference.segments.into_iter().skip(count));
        Ok(TypeRef { segments: prefix }.to_string())
    }

    fn lower_contents(
        &self,
        language: &mut Language,
        owner: Option<&PackageId>,
        locals: &BTreeSet<(Kind, String)>,
    ) -> Result<(), String> {
        for value in &mut language.traits {
            let parameters: BTreeSet<_> = value
                .type_params
                .iter()
                .map(|p| (Kind::Trait, p.name.clone()))
                .chain(locals.iter().cloned())
                .collect();
            let mut bind = |name: &mut String, kind| {
                *name = self.resolve_reference(name, kind, owner, &parameters)?;
                Ok(())
            };
            for p in &mut value.type_params {
                if let Some(bound) = &mut p.bound {
                    bind(bound, Kind::Trait.into())?;
                }
            }
            for block in &mut value.blocks {
                visit_items(&mut block.items, &mut bind)?;
            }
            self.lower_contents(&mut value.members, owner, &parameters)?;
        }
        let mut bind = |name: &mut String, kind| {
            *name = self.resolve_reference(name, kind, owner, locals)?;
            Ok(())
        };
        for value in &mut language.signs {
            visit_items(&mut value.items, &mut bind)?;
            self.lower_contents(&mut value.body.members, owner, locals)?;
        }
        for value in &mut language.paradigms {
            visit_case(&mut value.selector, &mut bind)?;
        }
        Ok(())
    }
}

type Binder<'a> = dyn FnMut(&mut String, RefKind) -> Result<(), String> + 'a;
fn visit_constraint(constraint: &mut SlotConstraint, bind: &mut Binder<'_>) -> Result<(), String> {
    if let SlotConstraint::Category(name) = constraint {
        bind(name, RefKind::Type)?;
    }
    Ok(())
}

pub(crate) fn visit_application(
    value: &mut SignApplication,
    bind: &mut Binder<'_>,
) -> Result<(), String> {
    bind(&mut value.callee, Kind::Sign.into())?;
    for arg in &mut value.arguments {
        match &mut arg.value {
            SignArgumentValue::Application(application) => visit_application(application, bind)?,
            SignArgumentValue::SelfSign | SignArgumentValue::Slot(_) => {}
        }
    }
    Ok(())
}

pub(crate) fn visit_case(case: &mut TypedCase, bind: &mut Binder<'_>) -> Result<(), String> {
    let category_scrutinee = case
        .scrutinee
        .as_deref()
        .and_then(|v| v.split_once('.'))
        .is_some_and(|(_, projection)| projection == "phon");
    for branch in &mut case.branches {
        match &mut branch.condition {
            CaseCondition::Guard(guard) => visit_guard(guard, bind)?,
            CaseCondition::Equals(category) if category_scrutinee => bind(category, RefKind::Type)?,
            CaseCondition::Equals(_)
            | CaseCondition::Bundle(_)
            | CaseCondition::Base
            | CaseCondition::Else => {}
        }
        for name in &mut branch.belongs {
            bind(name, Kind::Trait.into())?;
        }
        visit_expression(&mut branch.result, bind)?;
    }
    Ok(())
}

fn visit_expression(expression: &mut Expression, bind: &mut Binder<'_>) -> Result<(), String> {
    match expression {
        Expression::SignApplication(app) | Expression::PhonInterpolation(app) => {
            visit_application(app, bind)?
        }
        Expression::Projection { value, .. } => visit_expression(value, bind)?,
        Expression::Case(case) => visit_case(case, bind)?,
        Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
            visit_items(items, bind)?
        }
        Expression::PhonTemplate(template) => visit_template(template, bind)?,
        Expression::EnumValue(_)
        | Expression::SelfSign
        | Expression::Slot(_)
        | Expression::ImplicitBase => {}
    }
    Ok(())
}

// Only category comparisons contain type names. Feature paths, enum values,
// phonological classes and literal strings are not identifier substitutions.
fn visit_guard(text: &mut String, bind: &mut Binder<'_>) -> Result<(), String> {
    let original = text.clone();
    let mut edits = Vec::new();
    for (start, _) in original.match_indices('[') {
        let prefix = original[..start].trim_end();
        if !prefix.is_empty() && !prefix.ends_with("&&") && !prefix.ends_with("==") {
            continue;
        }
        if let Some(offset) = original[start..].find(']') {
            let end = start + offset;
            let mut name = original[start + 1..end].trim().to_owned();
            if crate::type_ref::TypeRef::parse(&name).is_ok() {
                bind(&mut name, RefKind::Type)?;
                edits.push((start + 1, end, name));
            }
        }
    }
    for (start, end, name) in edits.into_iter().rev() {
        text.replace_range(start..end, &name);
    }
    Ok(())
}

fn visit_template(text: &mut String, bind: &mut Binder<'_>) -> Result<(), String> {
    let parts = crate::reference::scan_interpolations(text);
    for part in parts.into_iter().rev() {
        if let Some(source) = part.inner.strip_suffix(".phon.ret") {
            if let Ok(mut application) = crate::parser::parse_sign_application_source(source) {
                visit_application(&mut application, bind)?;
                let replacement = format!(
                    "{{{}.phon.ret}}",
                    crate::printer::format_sign_application(&application)
                );
                text.replace_range(part.start..part.end, &replacement);
            }
        }
    }
    Ok(())
}

fn visit_rule(rule: &mut crate::Rule, bind: &mut Binder<'_>) -> Result<(), String> {
    // Sound-change classes and literals are not package categories.
    if rule.dim == crate::Dim::Phon {
        return Ok(());
    }
    for text in std::iter::once(&mut rule.body)
        .chain(rule.else_chain.iter_mut())
        .chain(rule.then_chain.iter_mut())
    {
        if let Some((body, guard)) = text.split_once(" / ") {
            let mut guard = guard.to_owned();
            visit_guard(&mut guard, bind)?;
            *text = format!("{body} / {guard}");
        }
    }
    Ok(())
}

pub(crate) fn visit_items(items: &mut [SignItem], bind: &mut Binder<'_>) -> Result<(), String> {
    for item in items {
        match item {
            SignItem::TraitMount { name, args, .. } => {
                let mut full = crate::type_ref::TypeRef::mount(name, args)?.to_string();
                bind(&mut full, Kind::Trait.into())?;
                (*name, *args) = crate::type_ref::TypeRef::parse(&full)?.split_mount();
            }
            SignItem::Slot(slot) => visit_constraint(&mut slot.constraint, bind)?,
            SignItem::RoleDecl(role) => visit_constraint(&mut role.constraint, bind)?,
            SignItem::SlotMap(SlotMapOp::AutoFill { filler, .. }) => {
                bind(filler, Kind::Sign.into())?
            }
            SignItem::SlotMap(SlotMapOp::AutoFillCall { filler, .. }) => {
                visit_application(filler, bind)?
            }
            SignItem::FeatureExpression(v) => visit_expression(&mut v.expression, bind)?,
            SignItem::RoleExpression(v) => visit_expression(&mut v.expression, bind)?,
            SignItem::SignExpression(v) => visit_expression(&mut v.expression, bind)?,
            SignItem::Realization(v) => visit_case(&mut v.expression, bind)?,
            SignItem::Rule(rule) | SignItem::FeatureRule(rule) => visit_rule(rule, bind)?,
            SignItem::Def(def) if def.path == "phon" => visit_template(&mut def.value, bind)?,
            SignItem::Def(def) if matches!(def.path.as_str(), "origin" | "components") => {
                let references = if def.path == "origin" {
                    crate::metadata::parse_origin(&def.value).map(|r| vec![r])
                } else {
                    crate::metadata::parse_components(&def.value)
                };
                if let Some(mut references) = references {
                    for reference in &mut references {
                        // Historical cross-language origins keep their existing `::` spelling.
                        if crate::type_ref::TypeRef::parse(&reference.0).is_ok() {
                            bind(&mut reference.0, Kind::Sign.into())?;
                        }
                    }
                    def.value = references
                        .iter()
                        .map(|r| format!("sign({})", r.0))
                        .collect::<Vec<_>>()
                        .join(", ");
                }
            }
            SignItem::SlotMap(
                SlotMapOp::Preserve { .. }
                | SlotMapOp::Rename { .. }
                | SlotMapOp::Internalize { .. }
                | SlotMapOp::Optional { .. },
            )
            | SignItem::Pass
            | SignItem::FeatureDecl(_)
            | SignItem::FeatureValue(_)
            | SignItem::SlotFeatureBinding(_)
            | SignItem::RoleBinding(_)
            | SignItem::Sense(_)
            | SignItem::SenseEdge(_)
            | SignItem::Constraint(_)
            | SignItem::Def(_) => {}
        }
    }
    Ok(())
}
