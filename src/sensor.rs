use crate::communication_channel::CommunicationChannel;
use crate::serde_util::{bool_false, by_path};
use crate::valuemap::*;
use fuseable::{Either, Fuseable};
use fuseable_derive::Fuseable;
use itertools::izip;
use num::Num;
use parse_num::parse_num_mask;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::*;
use std::collections::HashMap;
use std::iter::FromIterator;
use std::sync::{Arc, Mutex};

#[derive(Debug, Serialize, Deserialize, Fuseable)]
#[serde(untagged)]
enum Range {
    MinMax { min: i64, max: i64 },
}

#[derive(Debug, Serialize, Deserialize, Fuseable)]
#[serde(untagged)]
enum Description {
    Simple(String),
    LongAndShort { long: String, short: String },
}

// #[fuseable(virtual_field(name = "value", read = "self.read_value", write = "self.write_value", is_dir = "self.value_is_dir"))]
#[derive(Debug, Serialize, Deserialize, Fuseable)]
#[fuseable(virtual_field(
    name = "value",
    read = "self.read_value",
    write = "self.write_value",
    is_dir = "false"
))]
struct Register {
    #[fuseable(ro)]
    address: String,
    #[fuseable(ro)]
    width: Option<u8>,
    #[fuseable(ro)]
    mask: Option<String>,
    #[fuseable(ro)]
    #[serde(flatten)]
    range: Option<Range>,
    #[fuseable(ro)]
    #[serde(default, deserialize_with = "by_string_option_num")]
    default: Option<u64>,
    #[fuseable(ro)]
    description: Option<Description>,
    #[serde(skip)]
    #[fuseable(ro)]
    comm_channel: Option<Arc<Mutex<CommunicationChannel>>>,
}

/*
fn by_string<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    for<'a> T: Deserialize<'a>,
    D: Deserializer<'de>,
    T: FromStr,
    <T as FromStr>::Err: std::fmt::Display
{
    let s = String::deserialize(deserializer)?;

    T::from_str(&s).map_err(D::Error::custom)
}

fn by_string_option<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    for<'a> T: Deserialize<'a>,
    D: Deserializer<'de>,
    T: FromStr,
    <T as FromStr>::Err: std::fmt::Display
{
    let s = Option::<String>::deserialize(deserializer)?;

    match s {
        None => Ok(None),
        Some(v) => T::from_str(&v).map(|t| Some(t)).map_err(D::Error::custom)
    }
}
*/

fn by_string_option_num<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    for<'a> T: Deserialize<'a>,
    D: Deserializer<'de>,
    T: Num,
    <T as Num>::FromStrRadixErr: std::fmt::Display,
{
    let s = Option::<String>::deserialize(deserializer)?;

    match s {
        None => Ok(None),
        Some(v) => {
            let v: Vec<_> = v.chars().collect();
            let (base, start) = match (v.get(0), v.get(1)) {
                (Some('0'), Some('b')) => (2, 2),
                (Some('0'), Some('o')) => (8, 2),
                (Some('0'), Some('x')) => (16, 2),
                (Some('0'...'9'), _) => (10, 0),
                (_, _) => panic!("invalid address {:?}", v),
            };

            T::from_str_radix(&String::from_iter(&v[start..]), base)
                .map(|t| Some(t))
                .map_err(D::Error::custom)
        }
    }
}

impl Register {
    fn read_value(
        &self,
        path: &mut Iterator<Item = &str>,
    ) -> Result<Either<Vec<String>, String>, ()> {
        match path.next() {
            Some(_) => Err(()),
            None => {
                let comm_channel = self.comm_channel.clone().unwrap();
                let comm_channel = comm_channel.lock().unwrap();

                if let Some(width) = self.width {
                    comm_channel.read(&self.address, width as usize).map(|v| {
                        Either::Right(
                            "0x".to_string()
                                + &v.iter()
                                    .map(|v| format!("{:X}", v).to_string())
                                    .collect::<String>(),
                        )
                    })
                } else {
                    Err(())
                }
            }
        }
    }

    fn write_value(&self, path: &mut Iterator<Item = &str>, value: Vec<u8>) -> Result<(), ()> {
        match path.next() {
            Some(_) => Err(()),
            None => {
                let comm_channel = self.comm_channel.clone().unwrap();
                let comm_channel = comm_channel.lock().unwrap();

                if let Some(width) = self.width {
                    let (mask, mut value) =
                        parse_num_mask(String::from_utf8_lossy(&value)).map_err(|_| ())?;

                    if value.len() > width as usize {
                        return Err(());
                    }

                    let value = match mask {
                        Some(mut mask) => {
                            // TODO(robin): this currently interprets a too short value, as if the
                            // missing part should not be assigned and the old value (that is
                            // already in the register) be kept
                            // it is unclear if this is the wanted / intuitive behaviour, or if the
                            // opposite is the case (note this applies only if a mask is specified,
                            // maybe we only want to allow masks, when their width matches the
                            // expected width
                            while mask.len() < width as usize {
                                mask.push(0);
                            }

                            while value.len() < width as usize {
                                value.push(0);
                            }

                            let current_value = comm_channel.read(&self.address, width as usize)?;

                            izip!(mask, value, current_value)
                                .map(|(m, val, cur)| (val & m) | (cur & !m))
                                .collect()
                        }
                        None => value,
                    };

                    comm_channel.write(&self.address, value)
                } else {
                    Err(())
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Fuseable)]
struct RegisterSet {
    #[serde(flatten)]
    registers: HashMap<String, Register>,
}

#[derive(Debug, Fuseable)]
struct RegisterSetting {
    #[fuseable(skip)]
    channel: Arc<Mutex<CommunicationChannel>>,
    map: RegisterSet,
    #[fuseable(ro)]
    functions: HashMap<String, Function>,
}

impl<'de> Deserialize<'de> for RegisterSetting {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Serialize, Deserialize)]
        struct RegisterSettingConfig {
            channel: CommunicationChannel,
            #[serde(deserialize_with = "by_path")]
            map: RegisterSet,
            #[serde(deserialize_with = "by_path")]
            functions: HashMap<String, Function>,
        }

        //        println!("deserializing setting config");
        let settings = RegisterSettingConfig::deserialize(deserializer)?;
        //        println!("didi the shit {:#?}", settings);
        let channel = Arc::new(Mutex::new(settings.channel));

        let registers = settings
            .map
            .registers
            .into_iter()
            .map(|(name, reg)| {
                (
                    name,
                    Register {
                        comm_channel: Some(channel.clone()),
                        ..reg
                    },
                )
            })
            .collect();
        let map = RegisterSet { registers };

        Ok(RegisterSetting {
            channel,
            map,
            functions: settings.functions,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Fuseable)]
#[fuseable(virtual_field(
    name = "value",
    read = "self.read_value",
    write = "self.write_value",
    is_dir = "false"
))]
struct Function {
    #[fuseable(ro)]
    addr: String,
    #[fuseable(ro)]
    desc: Option<Description>,
    #[fuseable(skip)]
    #[serde(default, deserialize_with = "deser_valuemap")]
    map: Option<ValueMap>,
    #[serde(default = "bool_false")]
    #[fuseable(ro)]
    writable: bool,
    #[fuseable(ro)]
    default: Option<u64>,
}

impl Function {
    fn read_value(
        &self,
        path: &mut Iterator<Item = &str>,
    ) -> Result<Either<Vec<String>, String>, ()> {
        println!("reading of {}", self.addr);
        Err(())
    }

    fn write_value(&self, path: &mut Iterator<Item = &str>, value: Vec<u8>) -> Result<(), ()> {
        println!("writing to {}", self.addr);
        Err(())
    }
}

#[derive(Debug, Deserialize, Fuseable)]
pub struct Sensor {
    #[fuseable(ro)]
    model: String,
    registers: HashMap<String, RegisterSetting>,
}
