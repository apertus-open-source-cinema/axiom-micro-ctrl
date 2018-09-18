extern crate fuseable;
#[macro_use]
extern crate fuseable_derive;
#[macro_use]
extern crate fuse_mt;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;

use fuseable::{Fuseable, CachedFuseable, Either};
use std::ffi::OsStr;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Component, Components, Path, PathBuf};
use std::collections::{HashMap, BTreeMap};

#[derive(Debug, PartialEq, Serialize, Deserialize, Fuseable)]
#[serde(untagged)]
enum Range {
    MinMax { min: i64, max: i64 },
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Fuseable)]
#[serde(untagged)]
enum Description {
    Simple(String),
    LongAndShort { long: String, short: String },
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Fuseable)]
struct Register {
    address: String,
    width: Option<u8>,
    mask: Option<String>,
    #[serde(flatten)]
    range: Option<Range>,
    description: Option<Description>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Fuseable)]
struct RegisterSet {
    #[serde(flatten)]
    registers: HashMap<String, Register>,
}

fn main() {
    let mut f = File::open("sensors/ar0330/raw.yml").expect("file not found");

    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");

    let sensor: RegisterSet = serde_yaml::from_str(&contents).unwrap();

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
