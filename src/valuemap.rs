use fuseable::{Either, Fuseable};
use fuseable_derive::Fuseable;
use isomorphism::BiMap;
use parse_num::{parse_num, parse_num_padded, ParseError};
use serde::*;
use serde_derive::*;
use std::collections::HashMap;
use std::str::FromStr;
use std::cmp::Ordering;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

#[derive(Debug, Serialize, Deserialize, Hash, Eq, PartialEq, Clone)]
pub enum Value {
    Value(Vec<u8>),
    Any,
}

impl ToString for Value {
    fn to_string(&self) -> String {
        match self {
            Value::Value(v) => {
                "0x".to_string()
                    + &v.iter()
                        .map(|v| format!("{:02X}", v).to_string())
                        .collect::<String>()
            }
            Value::Any => "Any".to_string()
        }
    }
}

impl FromStr for Value {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Any" | "any" => Ok(Value::Any),
            _ => {
                println!("got {}", s);
                let v = parse_num_padded(s).map(Value::Value);
                println!("parsed {:?}", v);
                v
            }
        }
    }
}


#[derive(Debug, Serialize, Deserialize, Fuseable)]
pub enum ValueMap {
    Keywords(BiMap<Value, String>),
    Floating(HashMap<Value, f64>),
    Fixed(HashMap<Value, u64>),
}

impl ValueMap {
    pub fn lookup(&self, v: Vec<u8>) -> Result<String, ()> {
        match self {
            ValueMap::Keywords(map) => {
                match map.get_left(&Value::Value(v)) {
                    Some(v) => Ok(v.clone()),
                    None => {
                        match map.get_left(&Value::Any) {
                            Some(v) => Ok(v.clone()),
                            None => Err(())
                        }
                    }
                }
            },
            ValueMap::Floating(map) => {
                match map.get(&Value::Value(v)) {
                    Some(v) => Ok(v.to_string()),
                    None => {
                        match map.get(&Value::Any) {
                            Some(v) => Ok(v.to_string()),
                            None => Err(())
                        }
                    }
                }
            },
            ValueMap::Fixed(map) => {
                match map.get(&Value::Value(v)) {
                    Some(v) => Ok(v.to_string()),
                    None => {
                        match map.get(&Value::Any) {
                            Some(v) => Ok(v.to_string()),
                            None => Err(())
                        }
                    }
                }
            },
        }
    }

    pub fn encode(&self, s: String) -> Result<Vec<u8>, ()> {
        match self {
            ValueMap::Keywords(map) => {
                let v = map.get_right(&s).ok_or(())?;
                match v {
                    Value::Value(v) => Ok(v.clone()),
                    _ => {
                        let mut potential_value = vec![0u8];
                        loop {
                            match potential_value.last().unwrap() {
                                255 => {
                                    potential_value.push(1);
                                    let last_pos = potential_value.len() - 2;
                                    potential_value[last_pos] = 0;
                                }
                                _ => {
                                    let last_pos = potential_value.len() - 1;
                                    potential_value[last_pos] = potential_value.last().unwrap() + 1;
                                }
                            }

                            if map.get_left(&Value::Value(potential_value.clone())).is_none() {
                                break
                            }
                        }

                        Ok(potential_value)
                    }
                }
            },
            ValueMap::Floating(map) => {
                let wanted_value: f64 = s.parse().map_err(|_| ())?;

                let (v, _) = map.iter().min_by(|(_, v1), (_, v2)| {
                    if (wanted_value - **v1).abs() < (wanted_value - **v2).abs() {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }).ok_or(())?;

                match v {
                    Value::Value(v) => Ok(v.clone()),
                    _ => {
                        let mut potential_value = vec![0u8];
                        loop {
                            match potential_value.last().unwrap() {
                                255 => {
                                    potential_value.push(1);
                                    let last_pos = potential_value.len() - 2;
                                    potential_value[last_pos] = 0;
                                }
                                _ => {
                                    let last_pos = potential_value.len() - 1;
                                    potential_value[last_pos] = potential_value.last().unwrap() + 1;
                                }
                            }

                            if map.get(&Value::Value(potential_value.clone())).is_none() {
                                break
                            }
                        }

                        Ok(potential_value)
                    }
                }
            },
            ValueMap::Fixed(map) => {
                let wanted_value: u64 = Cursor::new(parse_num(s).map_err(|_| ())?).read_u64::<BigEndian>().map_err(|_| ())?;


                let (v, _) = map.iter().find(|(_, v)| {
                    **v == wanted_value
                }).ok_or(())?;

                match v {
                    Value::Value(v) => Ok(v.clone()),
                    _ => {
                        let mut potential_value = vec![0u8];
                        loop {
                            match potential_value.last().unwrap() {
                                255 => {
                                    potential_value.push(1);
                                    let last_pos = potential_value.len() - 2;
                                    potential_value[last_pos] = 0;
                                }
                                _ => {
                                    let last_pos = potential_value.len() - 1;
                                    potential_value[last_pos] = potential_value.last().unwrap() + 1;
                                }
                            }

                            if map.get(&Value::Value(potential_value.clone())).is_none() {
                                break
                            }
                        }

                        Ok(potential_value)
                    }
                }
            },
        }
    }
}

pub fn deser_valuemap<'de, D>(deserializer: D) -> Result<Option<ValueMap>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(untagged)]
    enum StringOru64Orf64 {
        String(String),
        U64(u64),
        F64(f64),
    }

    impl StringOru64Orf64 {
        fn to_string(&self) -> String {
            match self {
                StringOru64Orf64::String(s) => s.clone(),
                StringOru64Orf64::U64(i) => i.to_string(),
                StringOru64Orf64::F64(f) => f.to_string(),
            }
        }
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct ValueMapNonMatched {
        #[serde(flatten)]
        map: Option<HashMap<String, StringOru64Orf64>>,
    }

    let map = ValueMapNonMatched::deserialize(deserializer)?;

    let map: HashMap<String, String> = if map.map.is_none() {
        return Ok(None);
    } else {
        map.map
            .unwrap()
            .into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect()
    };

    // At first the map is split into it's key's and and it's values
    // then first try if the keys are a number (base 2, 8, 10  or 16), if this is the case the number gets parsed and converted to bytes
    // if the key cannot be parsed as a number the string is converted to bytes
    // A special role has the _ character, it is a valid number and represents any number not yet used

    let (keys, values): (Vec<_>, Vec<_>) = map.into_iter().unzip();

    let keys_as_numbers: Result<Vec<Value>, _> = keys
        .iter()
        .map(|k| {
            if k.trim() == "_" {
                Ok(Value::Any)
            } else {
                parse_num_padded(k).map(Value::Value)
            }
        })
        .collect();

    let converted_keys = if let Ok(keys) = keys_as_numbers {
        keys
    } else {
        keys.into_iter()
            .map(|k| Value::Value(String::into_bytes(k)))
            .collect()
    };

    // now to the values
    // first try u64, as they are the most specific (numbers without point)
    // then try f64, if nothing matches use String
    let values_as_int: Result<Vec<u64>, ()> = values
        .iter()
        .map(|s| s.parse::<u64>().map_err(|_| ()))
        .collect();

    fn build_hashmap<K: std::hash::Hash + std::cmp::Eq, V>(
        keys: Vec<K>,
        values: Vec<V>,
    ) -> HashMap<K, V> {
        keys.into_iter().zip(values.into_iter()).collect()
    }

    if let Ok(converted_values) = values_as_int {
        Ok(Some(ValueMap::Fixed(build_hashmap(
            converted_keys,
            converted_values,
        ))))
    } else if let Ok(converted_values) = values
        .iter()
        .map(|s| s.parse::<f64>().map_err(|_| ()))
        .collect()
    {
        Ok(Some(ValueMap::Floating(build_hashmap(
            converted_keys,
            converted_values,
        ))))
    } else {
        Ok(Some(ValueMap::Keywords(
            converted_keys.into_iter().zip(values.into_iter()).collect(),
        )))
    }
}
