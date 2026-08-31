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

use crate::{ConfigError, Span, SpannedValue};
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};

pub const SPANNED_MAGIC_STRUCT: &str = "$__evin_yaml_private_Spanned";
pub const SPANNED_VALUE_MAGIC_STRUCT: &str = "$__evin_yaml_private_RawSpannedValue";
pub const SPANNED_VALUE_FIELD: &str = "$__value";
pub const SPANNED_SPAN_FIELD: &str = "$__span";

thread_local! {
    pub static SPANNED_VALUE_EXCHANGE: std::cell::RefCell<Option<SpannedValue>> = const { std::cell::RefCell::new(None) };
    pub static CONFIG_REPORT: std::cell::RefCell<crate::ConfigReport> = const { std::cell::RefCell::new(crate::ConfigReport { overrides: Vec::new(), defaults: Vec::new() }) };
}

pub struct Deserializer {
    value: SpannedValue,
}

impl Deserializer {
    pub fn from_spanned_value(value: SpannedValue) -> Self {
        Deserializer { value }
    }
}

pub fn from_spanned_value<'a, T>(value: SpannedValue) -> Result<T, ConfigError>
where
    T: Deserialize<'a>,
{
    let deserializer = Deserializer::from_spanned_value(value);
    T::deserialize(deserializer)
}

pub fn from_spanned_value_with_report<'a, T>(
    value: SpannedValue,
) -> Result<(T, crate::ConfigReport), ConfigError>
where
    T: Deserialize<'a>,
{
    CONFIG_REPORT.with(|r| *r.borrow_mut() = crate::ConfigReport::default());
    let deserializer = Deserializer::from_spanned_value(value);
    let t = T::deserialize(deserializer)?;
    let report = CONFIG_REPORT.with(|r| r.borrow().clone());
    Ok((t, report))
}

impl<'de> de::Deserializer<'de> for Deserializer {
    type Error = ConfigError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let span = self.value.span();
        let result = match self.value {
            SpannedValue::Null(_) => visitor.visit_unit(),
            SpannedValue::Bool(b, _) => visitor.visit_bool(b),
            SpannedValue::Integer(i, _) => visitor.visit_i64(i),
            SpannedValue::Float(f, _) => visitor.visit_f64(f),
            SpannedValue::String(s, _) => visitor.visit_string(s),
            SpannedValue::Sequence(seq, _) => visitor.visit_seq(SeqDeserializer {
                iter: seq.into_iter(),
            }),
            SpannedValue::Mapping(map, _) => visitor.visit_map(MapDeserializer {
                iter: map.into_iter(),
                value: None,
            }),
        };

        result.map_err(|mut e| {
            if e.span.is_none() {
                e.span = Some(span);
            }
            e
        })
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if name == SPANNED_VALUE_MAGIC_STRUCT {
            SPANNED_VALUE_EXCHANGE.with(|ex| {
                *ex.borrow_mut() = Some(self.value.clone());
            });
            return visitor.visit_unit();
        }

        if name == SPANNED_MAGIC_STRUCT {
            // We inject the span here!
            let span = self.value.span();
            visitor
                .visit_map(SpannedMapAccess {
                    value: Some(self.value),
                    span: Some(span),
                })
                .map_err(|mut e| {
                    if e.span.is_none() {
                        e.span = Some(span);
                    }
                    e
                })
        } else {
            if let SpannedValue::Mapping(ref map, _) = self.value {
                let mut provided = std::collections::HashSet::new();
                for (k, _) in map {
                    if let SpannedValue::String(s, _) = k {
                        provided.insert(s.clone());
                    }
                }
                CONFIG_REPORT.with(|report| {
                    let mut r = report.borrow_mut();
                    for &field in _fields {
                        if provided.contains(field) {
                            r.overrides.push(field.to_string());
                        } else {
                            r.defaults.push(field.to_string());
                        }
                    }
                });
            }
            self.deserialize_any(visitor)
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if matches!(self.value, SpannedValue::Null(_)) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        use serde::de::IntoDeserializer;
        match self.value {
            SpannedValue::String(s, _) => visitor.visit_enum(s.into_deserializer()),
            SpannedValue::Mapping(mut map, span) => {
                if map.len() == 1 {
                    let (k, v) = map.pop().unwrap();
                    if let SpannedValue::String(s, _) = k {
                        return visitor
                            .visit_enum(crate::de_enum::EnumDeserializer { key: s, value: v });
                    }
                }
                Err(ConfigError {
                    message: "expected a single-key map for enum".into(),
                    span: Some(span),
                })
            }
            _ => Err(ConfigError::custom("expected string or map for enum")),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map identifier ignored_any
    }
}

impl de::Error for ConfigError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        ConfigError {
            message: msg.to_string(),
            span: None,
        }
    }
}

// SpannedMapAccess generates the fake fields for SPANNED_MAGIC_STRUCT
struct SpannedMapAccess {
    value: Option<SpannedValue>,
    span: Option<Span>,
}

impl<'de> MapAccess<'de> for SpannedMapAccess {
    type Error = ConfigError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        if self.span.is_some() {
            seed.deserialize(StringDeserializer(SPANNED_SPAN_FIELD.to_string()))
                .map(Some)
        } else if self.value.is_some() {
            seed.deserialize(StringDeserializer(SPANNED_VALUE_FIELD.to_string()))
                .map(Some)
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        if let Some(span) = self.span.take() {
            let map: std::collections::HashMap<&str, usize> = [
                ("line", span.line),
                ("column", span.column),
                ("offset", span.offset),
                ("length", span.length),
            ]
            .into_iter()
            .collect();
            let de = serde::de::value::MapDeserializer::<_, ConfigError>::new(map.into_iter());
            seed.deserialize(de)
        } else if let Some(value) = self.value.take() {
            seed.deserialize(Deserializer::from_spanned_value(value))
        } else {
            Err(ConfigError::custom("no more fields"))
        }
    }
}

struct StringDeserializer(String);

impl<'de> de::Deserializer<'de> for StringDeserializer {
    type Error = ConfigError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_string(self.0)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        use serde::de::IntoDeserializer;
        visitor.visit_enum(self.0.into_deserializer())
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

struct SeqDeserializer {
    iter: std::vec::IntoIter<SpannedValue>,
}

impl<'de> SeqAccess<'de> for SeqDeserializer {
    type Error = ConfigError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed
                .deserialize(Deserializer::from_spanned_value(value))
                .map(Some),
            None => Ok(None),
        }
    }
}

struct MapDeserializer {
    iter: std::vec::IntoIter<(SpannedValue, SpannedValue)>,
    value: Option<SpannedValue>,
}

impl<'de> MapAccess<'de> for MapDeserializer {
    type Error = ConfigError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((k, v)) => {
                self.value = Some(v);
                seed.deserialize(Deserializer::from_spanned_value(k))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        match self.value.take() {
            Some(value) => seed.deserialize(Deserializer::from_spanned_value(value)),
            None => Err(ConfigError::custom("value is missing")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StrictConfig {
        #[serde(rename = "expected")]
        _expected: String,
    }

    #[test]
    fn unknown_fields_use_the_exact_mapping_key_span() {
        let value = crate::parse_yaml("typo: value\n").unwrap();

        let error = from_spanned_value::<StrictConfig>(value).unwrap_err();

        assert!(error.message.contains("unknown field"));
        assert_eq!(
            error.span,
            Some(Span {
                line: 1,
                column: 1,
                offset: 0,
                length: 4,
            })
        );
    }
}
