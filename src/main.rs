extern crate fuseable;
#[macro_use]
extern crate fuseable_derive;
extern crate fuse_mt;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_derive_state;
extern crate serde_yaml;
#[macro_use]
extern crate structopt;
#[macro_use]
extern crate lazy_static;
extern crate isomorphism;
extern crate num;

use num::Num;
use isomorphism::BiMap;
use fuseable::{CachedFuseable, Either, Fuseable};
use serde::ser::{Serialize, Serializer, };
use serde::de::{Deserialize, Deserializer};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::fs::File;
use std::io::prelude::*;
use std::sync::Mutex;
use std::path::{Component, Components, Path, PathBuf};
use structopt::StructOpt;
use std::sync::{RwLock, Arc};

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

#[derive(Debug, Serialize, Deserialize, Fuseable)]
#[fuseable(virtual_field(name = "value", read = "&self.read", write = "&self.write", is_dir = "&self.is_dir"))]
struct Register {
    address: String,
    width: Option<u8>,
    mask: Option<String>,
    range: Option<Range>,
    description: Option<Description>,
    #[serde(skip)]
    comm_channel: Option<Arc<Mutex<CommunicationChannel>>>,
}

#[derive(Debug, Serialize, Deserialize, Fuseable)]
struct RegisterSet {
    #[serde(flatten)]
    registers: HashMap<String, Register>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Fuseable)]
#[serde(tag = "mode")]
enum CommunicationChannel {
    #[serde(rename = "i2c-cdev")]
    I2CCdev { bus: u8, address: u8 },
    #[serde(rename = "mmaped-gpio")]
    MMAPGPIO { base: u64 },
}

impl CommunicationChannel {
    fn read(&self, address: String, amount: usize) -> Result<Vec<u8>, ()> {
        println!("read {} bytes @{}", amount, address);
        Err(())
    }
}

#[derive(Debug, Fuseable)]
struct RegisterSetting {
    #[fuseable(skip)]
    channel: Arc<Mutex<CommunicationChannel>>,
    map: RegisterSet,
    functions: HashMap<String, Function>
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
            functions: HashMap<String, Function>
        }

        println!("deserializing setting config");
        let settings = RegisterSettingConfig::deserialize(deserializer)?;
        println!("didi the shit {:#?}", settings);
        let channel = Arc::new(Mutex::new(settings.channel));

        let registers = settings.map.registers.into_iter().map(|(name, reg)| (name, Register { comm_channel: Some(channel.clone()), .. reg })).collect();
        let map = RegisterSet { registers };

        Ok(RegisterSetting {
            channel,
            map,
            functions: settings.functions
        })
    }

}

struct FileOpener {
    path: Mutex<Option<PathBuf>>,
}

impl FileOpener {
    fn set_path(&self, path: PathBuf) {
        *self.path.lock().unwrap() = Some(path);
    }

    fn open(&self, filename: &str) -> std::io::Result<File> {
        let path = match *self.path.lock().unwrap() {
            Some(ref path) => path.with_file_name(filename),
            None => PathBuf::from(filename),
        };

        File::open(path)
    }
}

lazy_static! {
    static ref file_opener: FileOpener = FileOpener {
        path: Mutex::new(None),
    };
}

fn by_path<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    for<'a> T: Deserialize<'a>,
    D: Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;

    let mut f = file_opener.open(&path).expect("file not found");

    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");

    Ok(serde_yaml::from_str(&contents).unwrap())
}

fn bool_false() -> bool {
    false
}

#[derive(Debug, Serialize, Deserialize)]
enum ValueMap {
    Keywords(BiMap<Vec<u8>, String>),
    Floating(HashMap<Vec<u8>, f64>),
    Fixed(HashMap<Vec<u8>, u64>)
}

fn deser_valuemap<'de, D>(deserializer: D) -> Result<Option<ValueMap>, D::Error>
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
                StringOru64Orf64::F64(f) => f.to_string()
            }
        }
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct ValueMapNonMatched {
        #[serde(flatten)]
        map: Option<HashMap<String, StringOru64Orf64>>
    }

    let map = ValueMapNonMatched::deserialize(deserializer)?;

    let map: HashMap<String, String> = if map.map.is_none() {
        return Ok(None);
    } else {
        map.map.unwrap().into_iter().map(|(k, v)| (k, v.to_string())).collect()
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
            (b'0', b'o') => {
                s = &s[2..];
                8
            }
            (b'0'...b'9', _) => 10,
            _ => {
                return Err(());
            }
        };

        BigInt::from_str_radix(s, base).map(|s| s.to_bytes_be().1).map_err(|_| ())
    }

    let (keys, values): (Vec<_>, Vec<_>) = map.into_iter().unzip();

    println!("keys: {:?}", keys);
    println!("values: {:?}", values);

    let keys_as_numbers: Result<Vec<Vec<u8>>, ()> = keys.iter().map(parse_number).collect();
    println!("keys_as_numbers: {:?}", keys_as_numbers);
    let converted_keys = if let Ok(keys) = keys_as_numbers {
        keys
    } else {
        keys.into_iter().map(String::into_bytes).collect()
    };

    // now to the values
    // first try u64, as they are the most specific (numbers without point)
    // then try f64, if nothing matches use String
    let values_as_int: Result<Vec<u64>, ()> = values.iter().map(|s| s.parse::<u64>().map_err(|_| ())).collect();

    fn build_hashmap<K: std::hash::Hash + std::cmp::Eq, V>(keys: Vec<K>, values: Vec<V>) -> HashMap<K, V> {
        keys.into_iter().zip(values.into_iter()).collect()
    }

    if let Ok(converted_values) = values_as_int {
        Ok(Some(ValueMap::Fixed(build_hashmap(converted_keys, converted_values))))
    } else if let Ok(converted_values) = values.iter().map(|s| s.parse::<f64>().map_err(|_| ())).collect() {
        Ok(Some(ValueMap::Floating(build_hashmap(converted_keys, converted_values))))
    } else {
        Ok(Some(ValueMap::Keywords(converted_keys.into_iter().zip(values.into_iter()).collect())))
    }
}


#[derive(Debug, Serialize, Deserialize, Fuseable)]
struct Function {
    addr: String,
    desc: Option<Description>,
    #[fuseable(skip)]
    #[serde(default, deserialize_with = "deser_valuemap")]
    map: Option<ValueMap>,
    #[serde(default = "bool_false")]
    writable: bool
}

#[derive(Debug, Deserialize, Fuseable)]
struct Sensor {
    model: String,
    registers: HashMap<String, RegisterSetting>,
}

/// Basic daemon for controlling the various components of a camera
#[derive(StructOpt, Debug)]
#[structopt(name = "ctrl")]
struct Opt {
    /// Config file describing the camera components and their functionality
    #[structopt(name = "FILE")]
    file: String,
}

fn main() {
    let opt = Opt::from_args();

    let mut f = file_opener.open(&opt.file).unwrap();
    file_opener.set_path(PathBuf::from(opt.file));

    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");


    let sensor: Sensor = serde_yaml::from_str(&contents).unwrap();
    println!("{:#?}", sensor);

    let s: Box<Fuseable> = Box::new(sensor);
    // let s = CachedFuseable::new(s, 65536);
    // let s: Box<Fuseable> = Box::new(s);
    let fuse_args: Vec<&OsStr> = vec![&OsStr::new("-o"), &OsStr::new("auto_unmount")];
    // let cached_fs: Box<Fuseable> = Box::new(cached_s);
    fuse_mt::mount(fuse_mt::FuseMT::new(s, 1), &".propfs", &fuse_args).unwrap();

    /*
    let mut f = File::open("sensors/ar0330/raw.yml").expect("file not found");

    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");

    let sensor: RegisterSet = serde_yaml::from_str(&contents).unwrap();
    // println!("{:#?}", sensor);
    //

    let s = Sensor {
        registers: sensor.registers,
    };

    println!("{:#?}", s.read(&mut vec![].into_iter()));
    println!(
        "{:#?}",
        s.read(&mut vec!["registers"].into_iter())
    );
    println!(
        "{:#?}",
        s.read(&mut vec!["registers", "analog_gain",].into_iter())
    );
    println!(
        "{:#?}",
        s.read(
            &mut vec![
                "registers",
                "analog_gain",
                "address",
            ].into_iter()
        )
    );
    println!(
        "{:#?}",
        s.read(
            &mut vec![
                "registers",
                "analog_gain",
                "width",
            ].into_iter()
        )
    );
    println!(
        "{:#?}",
        s.read(
            &mut vec![
                "registers",
                "analog_gain",
                "description",
            ].into_iter()
        )
    );
    println!(
        "{:#?}",
        s.read(
            &mut vec![
                "registers",
                "chip_version_reg",
                "description",
            ].into_iter()
        )
    );
    println!(
        "{:#?}",
        s.read(
            &mut vec![
                "registers",
                "chip_version_reg",
                "description",
                "long",
            ].into_iter()
        )
    );
    println!(
        "{:#?}",
        s.read(
            &mut vec![
                "registers",
                "chip_version_reg",
                "description",
                "short",
            ].into_iter()
        )
    );
    // println!("{:?}", sensor.read_reg("reset"));
    
    /*
    trait A {};
    trait B {};

    impl A for B {};

    fn t<T: A>(test: &&T) {};


    struct T;
    impl A for T {};
    impl B for T {};

    let tt = T{};
    t(&tt as &B);
    */

    /*
    println!(
        "{:#?}",
        cached_s.read(
            &mut vec![
                "registers".to_string(),
                "chip_version_reg".to_string(),
                "description".to_string(),
                "short".to_string(),
            ].into_iter()
        )
    );
    */

    let test = vec!["a", "b", "c", "d", "e"];

    fn tester(t: &mut Iterator<Item = &str>) {
        {
            let a = t.into_iter().collect::<Vec<_>>().concat();
            println!("{}", a);
        }

        {
            let b = t.into_iter().collect::<Vec<_>>().concat();
            println!("{}", b);
        }
    }
    
    tester(&mut test.into_iter());

    let s: Box<Fuseable> = Box::new(s);
    // let cached_s = CachedFuseable::new(s, 65536);
    let fuse_args: Vec<&OsStr> = vec![&OsStr::new("-o"), &OsStr::new("auto_unmount")];
    // let cached_fs: Box<Fuseable> = Box::new(cached_s);
    fuse_mt::mount(fuse_mt::FuseMT::new(s, 1), &".propfs", &fuse_args).unwrap();


    /*
    for reg in sensor.registers.iter() {
        println!("{:?}", reg);
    }
    */

    //    println!("{:?}", sensor);
    */
}
