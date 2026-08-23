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

use crate::de::Deserializer;
use crate::{ConfigError, SpannedValue};
use serde::de::{
    DeserializeSeed, Deserializer as _, EnumAccess, IntoDeserializer, VariantAccess, Visitor,
};

pub struct EnumDeserializer {
    pub key: String,
    pub value: SpannedValue,
}

impl<'de> EnumAccess<'de> for EnumDeserializer {
    type Error = ConfigError;
    type Variant = VariantDeserializer;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let val = seed.deserialize(self.key.into_deserializer())?;
        Ok((val, VariantDeserializer(self.value)))
    }
}

pub struct VariantDeserializer(pub SpannedValue);

impl<'de> VariantAccess<'de> for VariantDeserializer {
    type Error = ConfigError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(Deserializer::from_spanned_value(self.0))
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Deserializer::from_spanned_value(self.0).deserialize_seq(visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Deserializer::from_spanned_value(self.0).deserialize_map(visitor)
    }
}
