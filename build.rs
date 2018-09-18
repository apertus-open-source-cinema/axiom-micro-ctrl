#[macro_use]
extern crate quote;
#[macro_use]
extern crate fuseable_derive;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
extern crate fuse_mt;
extern crate fuseable;
extern crate proc_macro2;
extern crate phf_codegen;

use proc_macro2::{TokenStream, Ident, Span};
use std::env;
use fuseable::{Fuseable, Either};
use fuse_mt::*;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::prelude::*;
use std::path::{Component, Components, Path, PathBuf};
use std::ops::{Deref, DerefMut};
use std::borrow::Cow;
use std::sync::RwLock;
use std::io::{BufWriter, Write};

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

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct RegisterSet {
    #[serde(flatten)]
    registers: BTreeMap<String, Register>,
}

fn generate_description(desc: &Option<Description>) -> TokenStream {
    if let Some(desc) = desc {
        let desc = match desc {
            Description::Simple(s) => quote! { Description::Simple(#s) },
            Description::LongAndShort {long, short} => quote! { Description::LongAndShort { long: #long, short: #short } },
        };
        quote! { Some(#desc) }
    } else {
        quote! { None }
    }
}

fn generate_range(range: &Option<Range>) -> TokenStream {
    if let Some(range) = range {
        let range = match range {
            Range::MinMax {min, max} => quote! { Range::MinMax { min: #min, max: #max } }
        };
        quote! { Some(#range) }
    } else {
        quote! { None }
    }
}

fn generate_option<T: quote::ToTokens>(option: &Option<T>) -> TokenStream {
    match option {
        Some(v) => quote! { Some(#v) },
        None => quote! { None }
    }
}

fn generate_register(reg: (&String, &Register)) -> (TokenStream, TokenStream) {
    let (name, reg) = reg;
    let name = Ident::new(name, Span::call_site());

    let register_fields = quote! {
        #name: Register
    };

    let address = &reg.address;
    let width = generate_option(&reg.width);
    let mask = generate_option(&reg.mask);
    let range = generate_range(&reg.range);
    let description = generate_description(&reg.description);

    let register_init = quote! {
        #name: Register {
            address: #address,
            width: #width,
            mask: #mask,
            range: #range,
            description: #description,
        }
    };

    (register_fields, register_init)
}

fn main() {
    let mut f = File::open("sensors/ar0330/raw.yml").expect("file not found");

    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");

    let sensor: RegisterSet = serde_yaml::from_str(&contents).unwrap();
    
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("sensor.rs");
    let mut f = File::create(&dest_path).unwrap();

    let registers: Vec<(TokenStream, TokenStream)> = sensor.registers.iter().map(generate_register).collect();
    let register_fields = registers.iter().map(|f| &f.0).collect::<Vec<_>>();
    let register_inits = registers.iter().map(|f| &f.1).collect::<Vec<_>>();

    let sensor_struct = quote! {
        #[derive(Debug, PartialEq, Fuseable)]
        enum Range {
            MinMax { min: i64, max: i64 },
        }

        #[derive(Debug, PartialEq, Fuseable)]
        enum Description {
            Simple(&'static str),
            LongAndShort { long: &'static str, short: &'static str },
        }

        #[derive(Debug, PartialEq, Fuseable)]
        struct Register {
            address: &'static str,
            width: Option<u8>,
            mask: Option<&'static str>,
            range: Option<Range>,
            description: Option<Description>,
        }

        #[derive(PartialEq, Debug, Fuseable)]
        struct RegisterSet {
            #(#register_fields),*
        }

        #[derive(PartialEq, Debug, Fuseable)]
        struct Sensor {
            registers: RegisterSet
        }

        const sensor: Sensor = Sensor { registers: RegisterSet { #(#register_inits),* } };
    };


    f.write_all(sensor_struct.to_string().as_bytes());

    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("codegen.rs");
    let mut file = BufWriter::new(File::create(&path).unwrap());

    write!(&mut file, "static KEYWORDS: phf::Map<&'static str, Keyword> = ").unwrap();
    phf_codegen::Map::new()
        .entry("loop", "Keyword::Loop")
        .entry("continue", "Keyword::Continue")
        .entry("break", "Keyword::Break")
        .entry("fn", "Keyword::Fn")
        .entry("extern", "Keyword::Extern")
        .build(&mut file)
        .unwrap();
    write!(&mut file, ";\n").unwrap();
}
