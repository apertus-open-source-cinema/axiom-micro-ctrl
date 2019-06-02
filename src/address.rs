use std::string::String;
use std::collections::HashMap;
use crate::sensor::Register;
use serde_derive::Serialize;
use lazy_static::lazy_static;
use regex::Regex;
use parse_num::parse_num_padded;
use parse_num::parse_num;
use fuseable_derive::Fuseable;
use fuseable::{Fuseable, Either};

#[derive(Debug, PartialEq, Serialize, Fuseable, Clone)]
pub struct Address {
    pub base: Vec<u8>,
    pub slice_start: u8,
    pub slice_end: u8,
}

impl Address {
    fn parse_internal(str: &str, register_set: Option<&HashMap<String, Register>>, width: Option<u8>) -> Result<Address, ()> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r#"^([^\[\]]+)(\[(?:([^\[\]]+)?:([^\[\]]+)?|([^:\[\]]+))\])?$"#).unwrap();
        }
    
        match RE.captures(str) {
            Some(captures) => {
                // capture 0 is the whole string
                // capture 1 is the base
                let (base, base_reg) = match captures.get(1) {
                    Some(m) => {
                        let m_str = m.as_str();
                        match parse_num_padded(m_str) {
                            Ok(v) => (v, None),
                            Err(_) => {
                                let base = m_str.bytes().collect::<Vec<u8>>();
                                let base_reg = register_set.and_then(|set| set.get(m_str));

                                let base = match base_reg {
                                    Some(reg) => reg.address.base.clone(),
                                    None => base

                                };

                                (base, base_reg)
                            }
                        }
                    }
                    None => {
                        panic!("no base found, lol?");
                    }
                };

                fn parse_slice_num(v: Vec<u8>) -> u8 {
                    if v.len() == 1 {
                        v[0]
                    } else if v.is_empty() {
                        0
                    } else {
                        panic!("sorry slices longer than one u8 not supported (got {:?})", v); 
                    } 
                }

                // capture 5 is the potential single bit slice
                //
                let (slice_start, slice_end) = match captures.get(5) {
                    Some(m) => {
                        let bit = parse_num(m.as_str()).map(parse_slice_num).map_err(|_| ())?;
                        (bit, bit + 1)
                    }
                    None => {
                        // capture 2 is the potential slice
                        // capture 3 is the potential slice start
                        let slice_start = match captures.get(3) {
                            Some(m) => {
                                parse_num(m.as_str()).map(parse_slice_num).map_err(|_| ())?
                            }
                            None => {
                                match base_reg {
                                    Some(r) => {
                                        r.address.slice_start
                                    }
                                    None => {
                                        0
                                    }
                                }
                            }
                        };
    
                        // capture 4 is the potential slice end
                        let slice_end = match captures.get(4) {
                            Some(m) => {
                                parse_num(m.as_str()).map(parse_slice_num).map_err(|_| ())?
                            }
                            None => {
                                match base_reg {
                                    Some(r) => {
                                        r.address.slice_end
                                    }
                                    None => {
                                        match width {
                                            Some(w) => {
                                                // width is in bytes
                                                slice_start + w * 8 - 1
                                            }
                                            None => {
                                                panic!("address did not specify an end of the slice and neither width nor base register are available ({})", str)
                                            }
                                        }
                                    }
                                }
                            }
                        };
                        
                        (slice_start, slice_end)
                    }
                };
    
                Ok(Address {
                    base,
                    slice_start,
                    slice_end
                })
            }
    
            None => {
                panic!("could not parse address {}", str);
            }
        }
    }

    pub fn parse_named(address: &str, regs: &HashMap<String, Register>) -> Result<Address, ()> {
        Address::parse_internal(address, Some(regs), None)
    }

    pub fn parse(address: &str, amount: usize) -> Result<Address, ()> {
        Address::parse_internal(address, None, Some(amount as u8))
    }

    /*
    fn slice_value(&self, value: Vec<u8>) -> Vec<u8> {
    
    }
    
    */

    // TODO(robin): this could suffer from endianess fuckup
    // TODO(robin): to fix this use byteorder crate and specify the byteorder of base
    // byteorder of base should be big endian to match all the other stuff
    pub fn as_u64(&self) -> u64 {
        assert!(self.base.len() < 9, "base should be no longer than 8 bytes");

        let mut base: u64 = 0;

        for byte in self.base.iter().rev() {
            base <<= 8;
            base |= u64::from(*byte);
        }

        base
    }

    pub fn bytes(&self) -> usize {
        let bits = self.slice_end - self.slice_start;
        let extra_byte = if bits % 8 > 0 { 1 } else { 0 };


        (extra_byte + (bits >> 3)) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_address_test() {
        assert_eq!(
            Address::parse(&"0x1234[1]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice_start: 1,
                slice_end: 2,
            })
        );
        assert_eq!(
            Address::parse(&"0x1234[:1]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice_start: 0,
                slice_end: 1
            })
        );
        assert_eq!(
            Address::parse(&"0x1234[1:]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice_start: 1,
                slice_end: 16
            })
        );
        assert_eq!(
            Address::parse(&"0x1234[1:3]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice_start: 1,
                slice_end: 3,
            })
        );
        assert_eq!(
            Address::parse(&"0x1234[0x1:0xa]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice_start: 1,
                slice_end: 10
            })
        );
    }
}
