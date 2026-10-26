//! Structural type references. Source fields retain their public string layout;
//! all parsing, substitution and name resolution use this tree.
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeRef {
    pub segments: Vec<TypeSegment>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeSegment {
    pub name: String,
    pub arguments: Vec<TypeArgument>,
}

/// The label binds a formal parameter; only the value is a type reference.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeArgument {
    pub name: Option<String>,
    pub value: TypeRef,
}

impl From<TypeRef> for TypeArgument {
    fn from(value: TypeRef) -> Self {
        Self { name: None, value }
    }
}

impl fmt::Display for TypeArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "{name}=")?;
        }
        write!(f, "{}", self.value)
    }
}

impl TypeSegment {
    /// One binding rule for compilation, member composition and runtime lookup.
    /// Return values in declaration order, independent of source argument order.
    pub(crate) fn ordered_arguments(
        &self,
        parameters: &[crate::TraitTypeParam],
    ) -> Result<Vec<TypeRef>, String> {
        let mut values = vec![None; parameters.len()];
        let mut named = false;
        let mut positional = 0;
        for argument in &self.arguments {
            let index = if let Some(name) = &argument.name {
                named = true;
                parameters
                    .iter()
                    .position(|p| p.name == *name)
                    .ok_or_else(|| {
                        format!(
                            "TYPE_ARGUMENT_UNKNOWN_NAME: {} has no parameter `{name}`",
                            self.name
                        )
                    })?
            } else {
                if named {
                    return Err(format!(
                        "TYPE_ARGUMENT_ORDER: {} has a positional argument after a named argument",
                        self.name
                    ));
                }
                let index = positional;
                positional += 1;
                if index >= parameters.len() {
                    return Err(format!(
                        "TYPE_PARAM_ARITY_MISMATCH: {} expects {} argument(s), got {}",
                        self.name,
                        parameters.len(),
                        self.arguments.len()
                    ));
                }
                index
            };
            if values[index].replace(argument.value.clone()).is_some() {
                return Err(format!(
                    "TYPE_ARGUMENT_DUPLICATE: {} parameter `{}` is supplied more than once",
                    self.name, parameters[index].name
                ));
            }
        }
        let missing = parameters
            .iter()
            .zip(&values)
            .filter(|(_, value)| value.is_none())
            .map(|(p, _)| p.name.as_str())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "TYPE_PARAM_ARITY_MISMATCH: {} expects {} argument(s), got {}; missing {}",
                self.name,
                parameters.len(),
                self.arguments.len(),
                missing.join(", ")
            ));
        }
        Ok(values.into_iter().flatten().collect())
    }
}

impl TypeRef {
    pub fn parse(source: &str) -> Result<Self, String> {
        fn ws(s: &str, p: &mut usize) {
            while s
                .get(*p..)
                .and_then(|v| v.chars().next())
                .is_some_and(char::is_whitespace)
            {
                *p += s[*p..].chars().next().unwrap().len_utf8();
            }
        }
        fn parse(s: &str, p: &mut usize, depth: usize) -> Result<TypeRef, String> {
            if depth > 64 {
                return Err("type reference nesting exceeds 64 levels".into());
            }
            let mut segments = Vec::new();
            loop {
                ws(s, p);
                let start = *p;
                while let Some(c) = s.get(*p..).and_then(|v| v.chars().next()) {
                    if c.is_alphanumeric() || c == '_' || c == '-' {
                        *p += c.len_utf8();
                    } else {
                        break;
                    }
                }
                if start == *p {
                    return Err(format!("expected type name in `{s}`"));
                }
                let name = s[start..*p].to_owned();
                ws(s, p);
                let mut arguments = Vec::new();
                if s.as_bytes().get(*p) == Some(&b'<') {
                    *p += 1;
                    loop {
                        let value = parse(s, p, depth + 1)?;
                        let argument = if s.as_bytes().get(*p) == Some(&b'=') {
                            if value.segments.len() != 1 || !value.segments[0].arguments.is_empty()
                            {
                                return Err(format!(
                                    "expected a parameter name before `=` in `{s}`"
                                ));
                            }
                            *p += 1;
                            TypeArgument {
                                name: Some(value.segments[0].name.clone()),
                                value: parse(s, p, depth + 1)?,
                            }
                        } else {
                            value.into()
                        };
                        arguments.push(argument);
                        ws(s, p);
                        match s.as_bytes().get(*p) {
                            Some(b',') => *p += 1,
                            Some(b'>') => {
                                *p += 1;
                                break;
                            }
                            _ => {
                                return Err(format!("unclosed or invalid type arguments in `{s}`"))
                            }
                        }
                    }
                }
                segments.push(TypeSegment { name, arguments });
                ws(s, p);
                if s.as_bytes().get(*p) != Some(&b'.') {
                    break;
                }
                *p += 1;
            }
            Ok(TypeRef { segments })
        }
        let mut p = 0;
        let result = parse(source, &mut p, 0)?;
        if p != source.len() {
            return Err(format!("unexpected text in type reference `{source}`"));
        }
        Ok(result)
    }

    pub fn path(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }

    pub fn substitute(&self, bindings: &BTreeMap<String, TypeRef>) -> TypeRef {
        let mut result = self.clone();
        for segment in &mut result.segments {
            segment.arguments = segment
                .arguments
                .iter()
                .map(|a| TypeArgument {
                    name: a.name.clone(),
                    value: a.value.substitute(bindings),
                })
                .collect();
        }
        if let Some(first) = result.segments.first() {
            if first.arguments.is_empty() {
                if let Some(bound) = bindings.get(&first.name) {
                    let mut segments = bound.segments.clone();
                    segments.extend(result.segments.into_iter().skip(1));
                    result.segments = segments;
                }
            }
        }
        result
    }

    /// The legacy TraitMount stores final-segment arguments separately.
    pub fn split_mount(mut self) -> (String, Vec<String>) {
        let args = std::mem::take(&mut self.segments.last_mut().unwrap().arguments);
        (
            self.to_string(),
            args.iter().map(ToString::to_string).collect(),
        )
    }

    pub fn mount(name: &str, args: &[String]) -> Result<Self, String> {
        let mut result = Self::parse(name)?;
        if !args.is_empty() {
            let last = result.segments.last_mut().unwrap();
            if !last.arguments.is_empty() {
                return Err("duplicate type argument list".into());
            }
            return Self::parse(&format!("{name}<{}>", args.join(", ")));
        }
        Ok(result)
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, segment) in self.segments.iter().enumerate() {
            if i != 0 {
                write!(f, ".")?;
            }
            write!(f, "{}", segment.name)?;
            if !segment.arguments.is_empty() {
                write!(f, "<")?;
                for (j, argument) in segment.arguments.iter().enumerate() {
                    if j != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{argument}")?;
                }
                write!(f, ">")?;
            }
        }
        Ok(())
    }
}

pub(crate) fn comma_parts(source: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut start = 0;
    let mut parts = Vec::new();
    for (i, c) in source.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(source[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(source[start..].trim());
    parts
}
