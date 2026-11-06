// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Static field metadata used by configuration owners and editor analysis.
//! Runtime deserialization and validation remain authoritative.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schema {
    Object(&'static [Field]),
    Map(&'static Schema),
    Sequence(&'static Schema),
    Union(&'static [Schema]),
    String,
    Integer,
    Number,
    Boolean,
    Enum(&'static [&'static str]),
    Any,
}

impl Schema {
    pub fn type_name(self) -> String {
        match self {
            Self::Object(_) => "object".into(),
            Self::Map(_) => "map".into(),
            Self::Sequence(item) => format!("{}[]", item.type_name()),
            Self::Union(variants) => variants
                .iter()
                .map(|variant| variant.type_name())
                .collect::<Vec<_>>()
                .join(" | "),
            Self::String => "string".into(),
            Self::Integer => "integer".into(),
            Self::Number => "number".into(),
            Self::Boolean => "boolean".into(),
            Self::Enum(_) => "enum".into(),
            Self::Any => "any".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRole {
    Property,
    Namespace,
    Modifier,
    EnumMember,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueConstraint {
    Regex,
    GitTemplate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub schema: Schema,
    pub required: bool,
    pub nullable: bool,
    pub alias_of: Option<&'static str>,
    pub default_value: Option<&'static str>,
    pub description_key: &'static str,
    pub snippet: Option<&'static str>,
    pub role: FieldRole,
    pub value_role: Option<FieldRole>,
    pub constraints: &'static [ValueConstraint],
    /// Stable catalog identity. Missing leaf entries can use a containing entry.
    pub catalog_target: Option<&'static str>,
}

impl Field {
    pub const fn new(name: &'static str, schema: Schema, description_key: &'static str) -> Self {
        Self {
            name,
            schema,
            required: false,
            nullable: true,
            alias_of: None,
            default_value: None,
            description_key,
            snippet: None,
            role: FieldRole::Property,
            value_role: None,
            constraints: &[],
            catalog_target: None,
        }
    }

    pub const fn required(mut self) -> Self {
        self.required = true;
        self.nullable = false;
        self
    }

    pub const fn non_nullable(mut self) -> Self {
        self.nullable = false;
        self
    }

    pub const fn alias_of(mut self, canonical: &'static str) -> Self {
        self.alias_of = Some(canonical);
        self
    }

    pub const fn default_value(mut self, value: &'static str) -> Self {
        self.default_value = Some(value);
        self
    }

    pub const fn snippet(mut self, snippet: &'static str) -> Self {
        self.snippet = Some(snippet);
        self
    }

    pub const fn role(mut self, role: FieldRole) -> Self {
        self.role = role;
        self
    }

    pub const fn value_role(mut self, role: FieldRole) -> Self {
        self.value_role = Some(role);
        self
    }

    pub const fn constraints(mut self, constraints: &'static [ValueConstraint]) -> Self {
        self.constraints = constraints;
        self
    }

    pub const fn catalog(mut self, target: &'static str) -> Self {
        self.catalog_target = Some(target);
        self
    }
}
