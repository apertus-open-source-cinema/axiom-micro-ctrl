use num::Num;
use std::iter::FromIterator;
use std::string::String;
use std::collections::HashMap;
use crate::sensor::Register;

#[derive(Debug, PartialEq)]
pub struct Address {
    pub base: Vec<u8>,
    pub slice: (u8, u8),
//    pub size: usize,
}


impl Address {
    pub fn parse_named(address: &String, regs: &HashMap<String, Register>) -> Result<Address, ()> {
        let mut split = address.split('[');
        match split.next() {
            Some(name) => {
                match regs.get(name) {
                    Some(reg) => {
                        match split.next() {
                            Some(idx_part) => {
                                Address::parse(&(reg.address.clone() + "[" + idx_part), reg.width.ok_or(())? as usize)
                            }
                            None => {
                                Address::parse(&reg.address, reg.width.ok_or(())? as usize)
                            }
                        }
                    }
                    None => Err(())
                }
            }
            None => Err(())
        }
    }

    pub fn parse(address: &String, amount: usize) -> Result<Address, ()> {
        let address: Vec<char> = address.chars().filter(|c| !c.is_whitespace()).collect();

        let mut idx = 0;

        let negative = match address[idx] {
            '-' => {
                idx += 1;
                true
            }
            _ => false,
        };

        fn parse_number(idx: usize, s: &Vec<char>) -> (usize, usize) {
            match (s.get(idx), s.get(idx + 1)) {
                (Some('0'), Some('b')) => (2, idx + 2),
                (Some('0'), Some('o')) => (8, idx + 2),
                (Some('0'), Some('x')) => (16, idx + 2),
                (Some('0'...'9'), _) => (10, idx),
                (_, _) => panic!("invalid address {:?}", s),
            }
        }

        fn get_number(
            base: usize,
            idx: usize,
            s: &Vec<char>,
            delim: char,
        ) -> (Vec<char>, bool, usize) {
            let mut found_delim = false;
            let mut chars = Vec::new();
            let mut idx = idx;

            //            println!("parsing number, base: {}, idx: {}, s: {:?}, delim: {}", base, idx, s, delim);

            loop {
                match s.get(idx) {
                    Some(c) => {
                        //                        println!("found digit: {}", c);
                        if *c == delim {
                            found_delim = true;
                            break;
                        } else {
                            //                            println!("found digit: {}", c);
                            if c.is_digit(base as u32) {
                                //                                println!("it is a valid digit :)");
                                chars.push(c.clone())
                            } else {
                                //                                println!("it is NOT a valid digit :(");
                                break;
                            }
                        }
                    }
                    None => {
                        break;
                    }
                }

                idx += 1;
            }

            (chars, found_delim, idx)
        }

        let (base, start) = parse_number(idx, &address);
        let (base_chars, has_slice, mut idx) = get_number(base, start, &address, '[');
        //        let base = usize::from_str_radix(&String::from_iter(base_chars), base as u32).unwrap();
        use num::bigint::BigInt;
        //        println!("{:?}", base_chars);
        let base = BigInt::from_str_radix(&String::from_iter(base_chars), base as u32)
            .map(|s| if negative { -s } else { s })
            .map(|s| s.to_bytes_be().1)
            .unwrap();

        idx += 1;

        let mut slice_start = 0u8;
        let mut slice_end = (amount * 8) as u8;

        if has_slice {
            //            println!("has_slice");

            let found_colon;
            match address.get(idx) {
                Some(':') => {
                    //                    println!("slice starts with colon");
                    found_colon = true;
                    slice_start = 0;
                }
                _ => {
                    //                    println!("slice has number before colon");
                    let (base, start) = parse_number(idx, &address);
                    let (start_chars, found_colon_here, end_idx) =
                        get_number(base, start, &address, ':');
                    found_colon = found_colon_here;
                    slice_start =
                        u8::from_str_radix(&String::from_iter(start_chars), base as u32).unwrap();
                    idx = end_idx;
                }
            }

            match address.get(idx) {
                Some(':') => {
                    idx += 1;

                    match address.get(idx) {
                        Some(']') => {}
                        _ => {
                            let (base, start) = parse_number(idx, &address);
                            let (end_chars, found_end, _end_idx) =
                                get_number(base, start, &address, ']');
                            assert!(found_end, "invalid slice, missing ]");
                            slice_end =
                                u8::from_str_radix(&String::from_iter(end_chars), base as u32)
                                    .unwrap();
                        }
                    }
                }

                Some(']') => {
                    assert!(!found_colon, "found colon when already at ]");
                    slice_end = slice_start + 1;
                }
                _ => assert!(false, "invalid address to be parsed"),
            }
        }

        // println!("amount: {}, slice_end: {}, slice_end >> 3: {}", amount, slice_end, slice_end >> 3);
        assert!(amount >= ((slice_end - slice_start) >> 3) as usize);

        Ok(Address {
            base,
            slice: (slice_start, slice_end),
//            size: amount,
        })
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
            base |= (*byte as u64);
        }

        base
    }

    pub fn bytes(&self) -> usize {
        let bits = self.slice.1 - self.slice.0;
        let extra_byte = if bits % 8 > 0 { 1 } else { 0 };


        (extra_byte + bits >> 3) as usize
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
                slice: (1, 2),
                size: 2
            })
        );
        assert_eq!(
            Address::parse(&"0x1234[:1]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice: (0, 1),
                size: 2
            })
        );
        assert_eq!(
            Address::parse(&"0x1234[1:]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice: (1, 16),
                size: 2
            })
        );
        assert_eq!(
            Address::parse(&"0x1234[1:3]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice: (1, 3),
                size: 2
            })
        );
        assert_eq!(
            Address::parse(&"0x1234[0x1:0xa]".to_string(), 2),
            Ok(Address {
                base: vec![0x12, 0x34],
                slice: (1, 10),
                size: 2
            })
        );
    }
}
