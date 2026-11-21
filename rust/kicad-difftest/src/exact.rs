// SPDX-License-Identifier: GPL-3.0-or-later
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::collections::BTreeMap;
use std::fmt;

/// JSON semantics, except numbers retain their complete original token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exact {
    Object(BTreeMap<String, Exact>),
    Array(Vec<Exact>),
    String(String),
    Scalar(String),
}

struct Object(BTreeMap<String, Box<RawValue>>);

impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = Object;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object with unique keys")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Object, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!("duplicate key {key}")));
                    }
                }
                Ok(Object(values))
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}

pub fn parse(text: &str) -> Result<Exact, String> {
    let raw: Box<RawValue> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    parse_raw(&raw, 0)
}

fn parse_raw(raw: &RawValue, depth: usize) -> Result<Exact, String> {
    if depth > 128 {
        return Err("JSON nesting exceeds 128".into());
    }
    let text = raw.get();
    match text.as_bytes().first() {
        Some(b'{') => {
            let Object(values) = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let values = values
                .into_iter()
                .map(|(k, v)| Ok((k, parse_raw(&v, depth + 1)?)))
                .collect::<Result<_, String>>()?;
            Ok(Exact::Object(values))
        }
        Some(b'[') => {
            let values: Vec<Box<RawValue>> =
                serde_json::from_str(text).map_err(|e| e.to_string())?;
            Ok(Exact::Array(
                values
                    .iter()
                    .map(|v| parse_raw(v, depth + 1))
                    .collect::<Result<_, _>>()?,
            ))
        }
        Some(b'"') => Ok(Exact::String(
            serde_json::from_str(text).map_err(|e| e.to_string())?,
        )),
        _ => Ok(Exact::Scalar(text.to_owned())),
    }
}

impl Exact {
    pub fn object(&self) -> Result<&BTreeMap<String, Exact>, String> {
        match self {
            Self::Object(value) => Ok(value),
            _ => Err("expected JSON object".into()),
        }
    }

    pub fn string(&self) -> Result<&str, String> {
        match self {
            Self::String(value) if !value.is_empty() => Ok(value),
            _ => Err("expected nonempty JSON string".into()),
        }
    }
}
