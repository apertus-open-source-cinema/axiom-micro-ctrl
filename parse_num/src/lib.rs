use num::Num;
use num::{bigint::Sign, BigInt};
use std::iter::FromIterator;

type ParseError = String;

fn get_negative_radix_start(s: &[char]) -> Result<(bool, u32, usize), ParseError> {
    let mut s = s;
    let negative = match s.get(0) {
        Some('-') => {
            s = &s[1..];
            true
        }
        _ => false,
    };

    match (s.get(0), s.get(1)) {
        (Some('0'), Some('x')) => Ok((negative, 16, 2)),
        (Some('0'), Some('o')) => Ok((negative, 8, 2)),
        (Some('0'), Some('b')) => Ok((negative, 2, 2)),
        (Some('0'...'9'), _) => Ok((negative, 10, 0)),
        _ => return Err("could not determine radix".to_owned()),
    }
}

pub fn parse_num_mask<T: ToString>(s: T) -> Result<(Option<Vec<u8>>, Vec<u8>), ParseError> {
    let string: Vec<_> = s.to_string().chars().collect();
    let string = &string[..];

    if !string.contains(&'z') {
        let num = parse_num(s)?;

        return Ok((None, num));
    }

    let (negative, radix, start) = get_negative_radix_start(string)?;
    if radix.count_ones() != 1 {
        return Err("invald radix, expected radix of from 2^n for masked numbers".to_owned());
    }

    let string = &string[start..];

    // TODO(robin): not really that nice at the moment, because it doesn't work for radix > 256
    // (maybe that is ok?)
    let mask_element = match BigInt::new(Sign::Plus, vec![radix - 1])
        .to_str_radix(radix)
        .chars()
        .next()
    {
        Some(elem) => elem,
        None => return Err(format!("internal mask element error for radix {}", radix)),
    };

    let mask_string: Vec<_> = string
        .iter()
        .cloned()
        .map(|c| if c == 'z' { '0' } else { mask_element })
        .collect();
    let string: Vec<_> = string
        .iter()
        .cloned()
        .map(|c| if c == 'z' { '0' } else { c })
        .collect();

    for c in &string[..] {
        if !c.is_digit(radix) {
            return Err(format!("invalid digit {} for radix {}", c, radix));
        }
    }

    let mask = str_to_vec_radix_negative(&String::from_iter(mask_string), radix, negative)?;
    let value = str_to_vec_radix_negative(&String::from_iter(string), radix, negative)?;

    Ok((Some(mask), value))
}

fn str_to_vec_radix_negative(s: &str, radix: u32, negative: bool) -> Result<Vec<u8>, ParseError> {
    BigInt::from_str_radix(s, radix)
        .map(|v| if negative { -v } else { v })
        .map(|v| v.to_signed_bytes_be())
        .map_err(|e| e.to_string())
}

pub fn parse_num<T: ToString>(string: T) -> Result<Vec<u8>, ParseError> {
    let string: Vec<_> = string.to_string().chars().collect();
    let mut string = &string[..];

    let (negative, radix, start) = get_negative_radix_start(string)?;
    let string = &string[start..];

    for c in string {
        if !c.is_digit(radix) {
            return Err(format!("invalid digit {} for radix {}", c, radix));
        }
    }

    str_to_vec_radix_negative(&String::from_iter(string), radix, negative)
}

#[cfg(test)]
mod tests {
    use crate::{parse_num, parse_num_mask};

    #[test]
    fn basic_number_parsing() {
        assert_eq!(parse_num("2"), Ok(vec![2]));
        assert_eq!(parse_num("0x2"), Ok(vec![0x2]));
        assert_eq!(parse_num("0b10"), Ok(vec![0b10]));
        assert_eq!(parse_num("0o2"), Ok(vec![0o2]));
    }

    #[test]
    fn masked_number_parsing() {
        assert_eq!(
            parse_num_mask("2z"),
            Err("invald radix, expected radix of from 2^n for masked numbers".to_owned())
        );
        assert_eq!(parse_num_mask("0x2"), Ok((None, vec![0x2])));
        assert_eq!(parse_num_mask("0b10"), Ok((None, vec![0b10])));
        assert_eq!(parse_num_mask("0o2"), Ok((None, vec![0o2])));
    }

    #[test]
    fn masked_number_parsing_with_masks() {
        assert_eq!(parse_num_mask("0xz2"), Ok((Some(vec![0b1111]), vec![0x2])));
        assert_eq!(
            parse_num_mask("0b1z0"),
            Ok((Some(vec![0b101]), vec![0b100]))
        );
        assert_eq!(
            parse_num_mask("0o2z"),
            Ok((Some(vec![0b111000]), vec![0o20]))
        );
    }
}
