#![feature(inner_deref)]

use crate::sensor::Sensor;
use crate::serde_util::file_opener;
use fuseable::{FuseableWrapper, Fuseable, CachedFuseable};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use structopt::StructOpt;

mod address;
mod communication_channel;
mod sensor;
mod serde_util;
mod valuemap;

/// Basic daemon for controlling the various components of a camera
#[derive(StructOpt, Debug)]
#[structopt(name = "ctrl")]
struct Opt {
    /// Config file describing the camera components and their functionality
    #[structopt(name = "FILE")]
    file: String,
    /// Set all communication channels to mock mode to prevent them from actually doing anything
    #[structopt(short = "m", long = "mock")]
    mock: bool,
}

fn main() {
    let opt = Opt::from_args();

    let mut f = file_opener.open(&opt.file).unwrap();
    file_opener.set_path(PathBuf::from(opt.file));

    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");

    let mut sensor: Sensor = serde_yaml::from_str(&contents).unwrap();
    sensor.mocked(opt.mock);



    // println!("{:#?}", sensor);

    /*
    let boxed_sensor: Box<Fuseable> = Box::new(sensor);
    let cached_sensor = CachedFuseable::new(boxed_sensor, 65535);
    let sensor: Box<Fuseable> = Box::new(cached_sensor);
    */

    let s = FuseableWrapper::new(sensor);
    // let s = CachedFuseable::new(s, 65536);
    // let s: Box<Fuseable> = Box::new(s);
    let fuse_args: Vec<&OsStr> = vec![&OsStr::new("-o"), &OsStr::new("auto_unmount")];
    // let cached_fs: Box<Fuseable> = Box::new(cached_s);
    fuse_mt::mount(fuse_mt::FuseMT::new(s, 1), &".propfs", &fuse_args).unwrap();
}
