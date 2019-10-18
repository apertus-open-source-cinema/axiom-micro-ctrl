use crate::{
    address::Address,
    communication_channel::CommunicationChannel,
    serde_util::{bool_false, by_path},
    valuemap::*,
};
use failure::format_err;
use fuseable::{type_name, Either, FuseableError};
use fuseable_derive::Fuseable;
use itertools::izip;
use num::Num;
use parse_num::parse_num_mask;
use serde::{de::Error, Deserialize, Deserializer};
use serde_derive::*;
use std::{
    collections::HashMap,
    iter::FromIterator,
    sync::{Arc, Mutex},
};

#[derive(Debug, Serialize, Deserialize, Fuseable, Clone)]
#[serde(untagged)]
enum Range {
    MinMax { min: i64, max: i64 },
}

#[derive(Debug, Serialize, Deserialize, Fuseable, Clone)]
#[serde(untagged)]
enum Description {
    Simple(String),
    LongAndShort { long: String, short: String },
}

// #[fuseable(virtual_field(name = "value", read = "self.read_value", write =
// "self.write_value", is_dir = "self.value_is_dir"))]
#[derive(Debug, Serialize, Fuseable, Clone)]
#[fuseable(virtual_field(
    name = "value",
    read = "self.read_value",
    write = "self.write_value",
    is_dir = "false"
))]
pub struct Register {
    #[fuseable(ro)]
    pub address: Address,
    #[fuseable(ro)]
    pub width: Option<u8>,
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

impl<'de> Deserialize<'de> for Register {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        pub struct RegisterStringAddr {
            pub address: String,
            pub width: Option<u8>,
            mask: Option<String>,
            #[serde(flatten)]
            range: Option<Range>,
            #[serde(default, deserialize_with = "by_string_option_num")]
            default: Option<u64>,
            description: Option<Description>,
            #[serde(skip)]
            comm_channel: Option<Arc<Mutex<CommunicationChannel>>>,
        }

        let reg = RegisterStringAddr::deserialize(deserializer)?;

        let address = Address::parse(&reg.address, reg.width.map(|v| v as usize))
            .map_err(|_| D::Error::custom("error parsing address"))?;

        Ok(Register {
            address,
            width: reg.width,
            mask: reg.mask,
            range: reg.range,
            default: reg.default,
            description: reg.description,
            comm_channel: reg.comm_channel,
        })
    }
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
                (Some('0'..='9'), _) => (10, 0),
                (..) => panic!("invalid address {:?}", v),
            };

            T::from_str_radix(&String::from_iter(&v[start..]), base)
                .map(Some)
                .map_err(D::Error::custom)
        }
    }
}

fn to_hex(v: Vec<u8>) -> String {
    if !v.is_empty() {
        "0x".to_string() + &v.iter().map(|v| format!("{:02X}", v).to_string()).collect::<String>()
    } else {
        "".to_string()
    }
}

impl Register {
    fn read_value(
        &self,
        path: &mut dyn Iterator<Item = &str>,
    ) -> fuseable::Result<Either<Vec<String>, String>> {
        match path.next() {
            Some(s) => Err(FuseableError::not_a_directory(type_name(&self), s)),
            None => {
                let comm_channel = self.comm_channel.clone().unwrap();
                let comm_channel = comm_channel.lock().unwrap();

                comm_channel.read_value(&self.address).map(|v| Either::Right(to_hex(v)))
            }
        }
    }

    fn write_value(
        &self,
        path: &mut dyn Iterator<Item = &str>,
        value: Vec<u8>,
    ) -> fuseable::Result<()> {
        match path.next() {
            Some(s) => Err(FuseableError::not_a_directory(type_name(&self), s)),
            None => {
                let comm_channel = self.comm_channel.clone().unwrap();
                let comm_channel = comm_channel.lock().unwrap();

                println!("writing");

                if let Some(width) = self.width {
                    let (mask, mut value) = parse_num_mask(String::from_utf8_lossy(&value))?;

                    if value.len() > width as usize {
                        return Err(format_err!("value {:?} to write was longer ({}) than register {:?} with width of {}", value, value.len(), self, width));
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

                            // TODO(robin): this also needs to account for little endian vs big
                            // endian for value 0x12345678 at 0x0,
                            // little endian has 0x78 is stored at 0x0, 0x56 is stored at 0x1 and so
                            // on big endian has 0x12 stored at 0x0,
                            // 0x34 stored at 0x1 and so on
                            // need to define internal byte order =>
                            // little endian -- not so intuitive
                            // big endian -- would be more efficient and more intuitive
                            while mask.len() < width as usize {
                                mask.push(0);
                            }

                            while value.len() < width as usize {
                                value.push(0);
                            }

                            let current_value = comm_channel.read_value(&self.address)?;

                            izip!(mask, value, current_value)
                                .map(|(m, val, cur)| (val & m) | (cur & !m))
                                .collect()
                        }
                        None => value,
                    };

                    comm_channel.write_value(&self.address, value)
                } else {
                    Err(format_err!("the register written to {:?} did not specify a width, don't know what to do", self))
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
    #[fuseable(ro)]
    channel: Arc<Mutex<CommunicationChannel>>,
    map: RegisterSet,
    functions: HashMap<String, Function>,
}

impl<'de> Deserialize<'de> for RegisterSetting {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct FunctionStringAddr {
            addr: String,
            desc: Option<Description>,
            #[serde(default, deserialize_with = "deser_valuemap")]
            map: Option<ValueMap>,
            #[serde(default = "bool_false")]
            writable: bool,
            default: Option<u64>,
            #[serde(skip)]
            channel: Option<Arc<Mutex<CommunicationChannel>>>,
        }

        #[derive(Debug, Deserialize)]
        struct RegisterSettingConfig {
            channel: CommunicationChannel,
            #[serde(deserialize_with = "by_path")]
            map: RegisterSet,
            #[serde(deserialize_with = "by_path")]
            functions: HashMap<String, FunctionStringAddr>,
        }

        let settings = RegisterSettingConfig::deserialize(deserializer)?;
        let channel = Arc::new(Mutex::new(settings.channel));

        let registers = settings
            .map
            .registers
            .into_iter()
            .map(|(name, reg)| (name, Register { comm_channel: Some(channel.clone()), ..reg }))
            .collect::<HashMap<_, _>>();

        let map = RegisterSet { registers: registers.clone() };

        let functions = settings
            .functions
            .into_iter()
            .map(|(name, func)| {
                let addr = Address::parse_named(&func.addr, &registers).map_err(|_| {
                    D::Error::custom(format!(
                        "could not parse the address of this function ({})",
                        func.addr
                    ))
                })?;

                Ok((
                    name,
                    Function {
                        channel: Some(channel.clone()),
                        addr,
                        desc: func.desc,
                        map: func.map,
                        default: func.default,
                        writable: func.writable,
                    },
                ))
            })
            .collect::<Result<HashMap<String, Function>, _>>()?;

        Ok(RegisterSetting { channel, map, functions })
    }
}

#[derive(Debug, Serialize, Fuseable)]
#[fuseable(virtual_field(
    name = "value",
    read = "self.read_value",
    write = "self.write_value",
    is_dir = "false"
))]
struct Function {
    #[fuseable(ro)]
    addr: Address,
    #[fuseable(ro)]
    desc: Option<Description>,
    // #[fuseable(skip)]
    #[serde(default, deserialize_with = "deser_valuemap")]
    map: Option<ValueMap>,
    #[serde(default = "bool_false")]
    #[fuseable(ro)]
    writable: bool,
    #[fuseable(ro)]
    default: Option<u64>,
    #[serde(skip)]
    #[fuseable(ro)]
    channel: Option<Arc<Mutex<CommunicationChannel>>>,
}

impl Function {
    fn read_value(
        &self,
        path: &mut dyn Iterator<Item = &str>,
    ) -> fuseable::Result<Either<Vec<String>, String>> {
        match path.next() {
            Some(s) => Err(FuseableError::not_a_directory(type_name(&self), s)),
            None => {
                let channel = self
                    .channel.as_ref() // .unwrap()
                    .ok_or_else(|| {
                        format_err!("tried to read, but had no communication channel of function {:?}", self)
                    })?
                    .lock()
                    .unwrap();
                let value = channel.read_value(&self.addr)?;

                match &self.map {
                    Some(map) => map.lookup(value).map(Either::Right),
                    None => Ok(Either::Right(to_hex(value))),
                }
            }
        }
    }

    fn write_value(
        &self,
        path: &mut dyn Iterator<Item = &str>,
        value: Vec<u8>,
    ) -> fuseable::Result<()> {
        match path.next() {
            Some(s) => Err(FuseableError::not_a_directory(type_name(&self), s)),
            None => {
                let channel = self
                    .channel.as_ref()
                    .ok_or_else(|| {
                        format_err!("tried to write, but had no communication channel of function {:?}", self)
                    })?
                    .lock()
                    .unwrap();

                let value = match &self.map {
                    Some(map) => map.encode(String::from_utf8(value)?)?,
                    None => value,
                };

                println!("encoded value: {:?}", value);

                channel.write_value(&self.addr, value)
            }
        }
    }
}

#[derive(Debug, Deserialize, Fuseable)]
pub struct Sensor {
    #[fuseable(ro)]
    model: String,
    registers: HashMap<String, RegisterSetting>,
}

impl Sensor {
    pub fn mocked(&mut self, mock: bool) {
        for rs in self.registers.values_mut() {
            rs.channel.lock().unwrap().mock_mode(mock);
        }
    }
}
