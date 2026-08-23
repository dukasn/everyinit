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

use crate::FileLocation;

/// A span in a source file.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, std::hash::Hash,
)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub length: usize,
}

impl Default for Span {
    fn default() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
            length: 1,
        }
    }
}

impl Span {
    pub fn to_file_location(&self, path: impl Into<String>) -> FileLocation {
        FileLocation::Path {
            path: path.into(),
            line: Some(self.line),
            column: Some(self.column),
        }
    }

    pub fn to_byte_span(&self, path: impl Into<String>) -> FileLocation {
        FileLocation::ByteSpan {
            path: path.into(),
            offset: self.offset,
            length: self.length,
            label: None,
        }
    }
}

/// A wrapper that pairs a value with its span in the source file.
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, serde::Serialize)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn to_file_location(&self, path: impl Into<String>) -> FileLocation {
        self.span.to_file_location(path)
    }

    pub fn to_byte_span(&self, path: impl Into<String>) -> FileLocation {
        self.span.to_byte_span(path)
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub const SPANNED_MAGIC_STRUCT: &str = "$__evin_yaml_private_Spanned";
pub const SPANNED_VALUE_FIELD: &str = "$__value";
pub const SPANNED_SPAN_FIELD: &str = "$__span";

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Spanned<T> {
    fn deserialize<D>(deserializer: D) -> Result<Spanned<T>, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct SpannedVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for SpannedVisitor<T> {
            type Value = Spanned<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a spanned value")
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Spanned<T>, V::Error>
            where
                V: serde::de::MapAccess<'de>,
            {
                let mut span: Option<Span> = None;
                let mut value: Option<T> = None;

                while let Some(key) = visitor.next_key::<String>()? {
                    if key == SPANNED_SPAN_FIELD {
                        span = Some(visitor.next_value()?);
                    } else if key == SPANNED_VALUE_FIELD {
                        value = Some(visitor.next_value()?);
                    } else {
                        let _ = visitor.next_value::<serde::de::IgnoredAny>()?;
                    }
                }

                let span =
                    span.ok_or_else(|| serde::de::Error::missing_field(SPANNED_SPAN_FIELD))?;
                let value =
                    value.ok_or_else(|| serde::de::Error::missing_field(SPANNED_VALUE_FIELD))?;

                Ok(Spanned::new(value, span))
            }
        }

        deserializer.deserialize_struct(
            SPANNED_MAGIC_STRUCT,
            &[SPANNED_VALUE_FIELD, SPANNED_SPAN_FIELD],
            SpannedVisitor(std::marker::PhantomData),
        )
    }
}
