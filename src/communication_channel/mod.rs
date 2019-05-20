use crate::address::Address;
use core::fmt::Debug;
use derivative::*;
use fuseable::{Either, Fuseable};
use fuseable_derive::*;
use i2cdev::{core::I2CDevice, linux::LinuxI2CDevice};
use memmap::{MmapMut, MmapOptions};
use paste;
use serde::*;
use serde_derive::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::sync::RwLock;

pub type CommunicationChannel = Box<CommChannel>;

pub trait CommChannel: Debug + Fuseable {
    fn read_value_real(&self, address: &Address) -> Result<Vec<u8>, ()>;
    fn write_value_real(&self, address: &Address, value: Vec<u8>) -> Result<(), ()>;

    fn read_value_mock(&self, address: &Address) -> Result<Vec<u8>, ()> {
        println!("MOCK READ {:?} bits at {:?} by {:?}", address.slice.1 - address.slice.1, address, self);
        Ok(vec![])
    }

    fn write_value_mock(&self, address: &Address, value: Vec<u8>) -> Result<(), ()> {
        println!("MOCK WRITE {:?} to {:?} by {:?}", value, address, self);
        Ok(())
    }

    fn mock_mode(&mut self, mock: bool);
    fn get_mock_mode(&self) -> bool;

    fn read_value(&self, address: &Address) -> Result<Vec<u8>, ()> {
        if self.get_mock_mode() {
            self.read_value_mock(&address)
        } else {
            self.read_value_real(&address)
        }
    }

    fn write_value(&self, address: &Address, value: Vec<u8>) -> Result<(), ()> {
        if self.get_mock_mode() {
            self.write_value_mock(&address, value)
        } else {
            self.write_value_real(&address, value)
        }
    }
}

#[derive(Derivative, Serialize, Deserialize, Fuseable)]
#[derivative(Debug, PartialEq)]
struct I2CCdev {
    bus: u8,
    address: u8,
    #[fuseable(skip)]
    #[serde(skip)]
    #[derivative(Debug = "ignore", PartialEq = "ignore")]
    dev: RwLock<Option<LinuxI2CDevice>>,
    #[fuseable(ro)]
    #[serde(skip)]
    mock: bool,
}

#[derive(Derivative, Serialize, Deserialize, Fuseable)]
#[derivative(Debug, PartialEq)]
struct MMAPGPIO {
    base: u64,
    len: u64,
    #[fuseable(skip)]
    #[serde(skip)]
    #[derivative(Debug = "ignore", PartialEq = "ignore")]
    dev: RwLock<Option<MmapMut>>,
    #[fuseable(ro)]
    #[serde(skip)]
    mock: bool,
}

impl I2CCdev {
    fn init(&self) -> Result<LinuxI2CDevice, ()> {
        LinuxI2CDevice::new(format!("/dev/i2c-{}", self.bus), self.address as u16).map_err(|_| ())
    }
}

impl MMAPGPIO {
    fn init(&self) -> Result<MmapMut, ()> {
        unsafe {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open("/dev/mem")
                .map_err(|_| ())?;

            MmapOptions::new()
                .len(self.len as usize)
                .offset(self.base)
                .map_mut(&file)
                .map_err(|_| ())
        }
    }
}

impl CommChannel for I2CCdev {
    fn read_value_real(&self, address: &Address) -> Result<Vec<u8>, ()> {
        with_dev(
            &self.dev,
            |i2c_dev| {
                i2c_dev.write(&address.base).map_err(|_| ())?;
                let mut ret = vec![0; address.bytes()];
                i2c_dev.read(&mut ret).map_err(|_| ())?;
                Ok(ret)
            },
            || self.init(),
        )
    }

    fn write_value_real(&self, address: &Address, value: Vec<u8>) -> Result<(), ()> {
        with_dev(
            &self.dev,
            |i2c_dev| {
                i2c_dev.write(&address.base).map_err(|_| ())?;
                i2c_dev.write(&value).map_err(|_| ())
            },
            || self.init(),
        )
    }

    fn mock_mode(&mut self, mock: bool) {
        self.mock = mock;
    }

    fn get_mock_mode(&self) -> bool {
        self.mock
    }
}

impl CommChannel for MMAPGPIO {
    fn read_value_real(&self, address: &Address) -> Result<Vec<u8>, ()> {
        let offset = address.as_u64() as usize;

        with_dev(
            &self.dev,
            |mmap_dev| {
                mmap_dev
                    .get(offset..(offset + address.bytes()))
                    .map(|v| v.to_vec())
                    .ok_or(())
            },
            || self.init(),
        )
    }

    fn write_value_real(&self, address: &Address, value: Vec<u8>) -> Result<(), ()> {
        let offset = address.as_u64() as usize;

        with_dev(
            &self.dev,
            |mmap_dev| {
                let mut i = 0;
                for byte in value {
                    mmap_dev[offset + i] = byte;
                    i += 1;
                }
                Ok(())
            },
            || self.init(),
        )
    }

    fn mock_mode(&mut self, mock: bool) {
        self.mock = mock;
    }

    fn get_mock_mode(&self) -> bool {
        self.mock
    }
}

fn with_dev<D, F, I, T>(dev: &RwLock<Option<D>>, func: F, init: I) -> Result<T, ()>
where
    F: FnOnce(&mut D) -> Result<T, ()>,
    I: FnOnce() -> Result<D, ()>,
{
    let mut dev = dev.write().map_err(|_| ())?;

    if dev.is_none() {
        *dev = Some(init()?);

        println!("opened device");
    } else {
        println!("had cached device");
    }

    match *dev {
        None => Err(()),
        Some(ref mut dev) => func(dev),
    }
}

macro_rules! comm_channel_config {
    ( $($struct:ident => $tag:tt),* ) => {
        paste::item!{
            #[derive(Debug, PartialEq, Serialize, Deserialize, Fuseable)]
            #[serde(tag = "mode")]
            enum CommChannelConfig {
                $(
                    #[serde(rename = $tag)]
                    [<$struct Channel___>] {
                        #[serde(flatten)]
                        channel: $struct,
                    },
                )*
            }

            impl CommChannelConfig {
                fn to_comm_channel(self) -> Box<CommChannel> {
                    match self {
                        $(
                            CommChannelConfig::[<$struct Channel___>] { channel } => { Box::new(channel) },
                        )*
                    }
                }
            }
        }
    }
}

comm_channel_config!(I2CCdev => "i2c-cdev", MMAPGPIO => "mmaped-gpio");

impl<'de> Deserialize<'de> for Box<CommChannel> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let config = CommChannelConfig::deserialize(deserializer)?;
        Ok(config.to_comm_channel())
    }
}
