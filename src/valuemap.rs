use isomorphism::BiMap;
use num::Num;
use serde::*;
use serde_derive::*;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub enum ValueMap {
    Keywords(BiMap<Vec<u8>, String>),
    Floating(HashMap<Vec<u8>, f64>),
    Fixed(HashMap<Vec<u8>, u64>),
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
    use num::bigint::BigInt;

    pub fn byte<S: AsRef<[u8]> + ?Sized>(s: &S, idx: usize) -> u8 {
        let s = s.as_ref();
        if idx < s.len() {
            s[idx]
        } else {
            0
        }
    }

    fn parse_number(s: &String) -> Result<Vec<u8>, ()> {
        let mut s: &str = s;
        let base = match (byte(s, 0), byte(s, 1)) {
            (b'0', b'x') => {
                s = &s[2..];
                16
            }
            (b'0', b'o') => {
                s = &s[2..];
                8
            }
            (b'0', b'b') => {
                s = &s[2..];
                2
            }
            (b'0'...b'9', _) => 10,
            _ => {
                return Err(());
            }
        };

        BigInt::from_str_radix(s, base)
            .map(|s| s.to_bytes_be().1)
            .map_err(|_| ())
    }

    let (keys, values): (Vec<_>, Vec<_>) = map.into_iter().unzip();

    //    println!("keys: {:?}", keys);
    //    println!("values: {:?}", values);

    let keys_as_numbers: Result<Vec<Vec<u8>>, ()> = keys.iter().map(parse_number).collect();
    //    println!("keys_as_numbers: {:?}", keys_as_numbers);
    let converted_keys = if let Ok(keys) = keys_as_numbers {
        keys
    } else {
        keys.into_iter().map(String::into_bytes).collect()
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
