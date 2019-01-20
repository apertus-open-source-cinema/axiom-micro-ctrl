use crate::address::Address;
use derivative::*;
use fuseable::{Either, Fuseable};
use fuseable_derive::*;
use i2cdev::{core::I2CDevice, linux::LinuxI2CDevice};
use memmap::{MmapMut, MmapOptions};
use serde::*;
use serde_derive::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::sync::RwLock;

#[derive(Derivative, Serialize, Deserialize, Fuseable)]
#[derivative(Debug, PartialEq = "feature_allow_slow_enum")]
#[serde(tag = "mode")]
pub enum CommunicationChannel {
    #[serde(rename = "i2c-cdev")]
    I2CCdev {
        bus: u8,
        address: u8,
        #[fuseable(skip)]
        #[serde(skip)]
        #[derivative(Debug = "ignore", PartialEq = "ignore")]
        dev: RwLock<Option<LinuxI2CDevice>>,
    },
    #[serde(rename = "mmaped-gpio")]
    MMAPGPIO {
        base: u64,
        len: u64,
        #[fuseable(skip)]
        #[serde(skip)]
        #[derivative(Debug = "ignore", PartialEq = "ignore")]
        dev: RwLock<Option<MmapMut>>,
    },
}

impl CommunicationChannel {
    pub fn read(&self, address: &String, amount: usize) -> Result<Vec<u8>, ()> {
        let addr = Address::parse(address, amount)?;

        //        println!("read {} bytes @{:#?}", amount, address);

        match self {
            CommunicationChannel::I2CCdev { bus, address, dev } => {
                self.with_i2c_dev(bus, address, dev, |i2c_dev| {
                    i2c_dev.write(&addr.base).map_err(|_| ())?;
                    let mut ret = vec![0; amount];
                    i2c_dev.read(&mut ret).map_err(|_| ())?;
                    Ok(ret)
                })
            }
            CommunicationChannel::MMAPGPIO { base, len, dev } => {
                let offset = addr.as_u64() as usize;
                self.with_mmap(base, len, dev, |mmap_dev| {
                    mmap_dev
                        .get(offset..(offset + addr.size))
                        .map(|v| v.to_vec())
                        .ok_or(())
                })
            }
        }
    }

    fn with_mmap<F, T>(
        &self,
        base: &u64,
        len: &u64,
        dev: &RwLock<Option<MmapMut>>,
        func: F,
    ) -> Result<T, ()>
    where
        F: FnOnce(&mut MmapMut) -> Result<T, ()>,
    {
        let mut dev = dev.write().map_err(|_| ())?;

        if dev.is_none() {
            unsafe {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open("/dev/mem")
                    .map_err(|_| ())?;

                *dev = Some(
                    MmapOptions::new()
                        .len(*len as usize)
                        .offset(*base)
                        .map_mut(&file)
                        .map_err(|_| ())?,
                );
            }

            println!("opened device");
        } else {
            println!("had cached device");
        }

        match *dev {
            None => Err(()),
            Some(ref mut dev) => func(dev),
        }
    }

    fn with_i2c_dev<F, T>(
        &self,
        bus: &u8,
        address: &u8,
        dev: &RwLock<Option<LinuxI2CDevice>>,
        func: F,
    ) -> Result<T, ()>
    where
        F: FnOnce(&mut LinuxI2CDevice) -> Result<T, ()>,
    {
        let mut dev = dev.write().map_err(|_| ())?;

        if dev.is_none() {
            *dev = Some(
                LinuxI2CDevice::new(format!("/dev/i2c-{}", bus), *address as u16)
                    .map_err(|_| ())?,
            );
            println!("opened device");
        } else {
            println!("had cached device");
        }

        match *dev {
            None => Err(()),
            Some(ref mut dev) => func(dev),
        }
    }

    pub fn write(&self, address: &String, value: Vec<u8>) -> Result<(), ()> {
        // println!("writing {:?} to @{}", value, address);
        let addr = Address::parse(address, value.len())?;

        match self {
            CommunicationChannel::I2CCdev { bus, address, dev } => {
                self.with_i2c_dev(bus, address, dev, |i2c_dev| {
                    i2c_dev.write(&addr.base).map_err(|_| ())?;
                    i2c_dev.write(&value).map_err(|_| ())
                })
            }
            CommunicationChannel::MMAPGPIO { base, len, dev } => {
                let offset = addr.as_u64() as usize;
                self.with_mmap(base, len, dev, |mmap_dev| {
                    let mut i = 0;
                    for byte in value {
                        mmap_dev[offset + i] = byte;
                        i += 1;
                    }
                    Ok(())
                })
            }
        }
    }
}
