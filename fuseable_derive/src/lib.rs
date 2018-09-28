#![recursion_limit = "512"]

extern crate proc_macro;
extern crate proc_macro2;
#[macro_use]
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream as TS;
use proc_macro2::{Ident, Span, TokenStream};
use syn::Data::{Enum, Struct};
use syn::{DeriveInput, Field};
use std::fmt::Debug;

#[proc_macro_derive(Fuseable, attributes(fuseable))]
pub fn fuse_derive(input: TS) -> TS {
    let ast = parse_macro_input!(input as DeriveInput);
    let gen = impl_fuseable(&ast);
    gen.into()
}

fn impl_fuseable(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let (is_dir, read, write) = impl_body(ast);

    let ret = quote! {
        impl Fuseable for #name {
            fn is_dir(&self, path: &mut Iterator<Item = &str>) -> Result<bool, ()> {
                #is_dir
            }

            fn read(&self, path: &mut Iterator<Item = &str>) -> Result<Either<Vec<String>, String>, ()> {
                #read
            }

            fn write(&mut self, path: &mut Iterator<Item = &str>, value: Vec<u8>) -> Result<(), ()> {
                #write
            }
        }
    };

    ret
}

#[derive(Default, Debug)]
struct VirtualField {
    name: Option<String>,
    is_dir: Option<String>,
    read: Option<String>,
    write: Option<String>
}

fn impl_body(ast: &syn::DeriveInput) -> (TokenStream, TokenStream, TokenStream) {
    let attrs: Vec<_> = ast.attrs.iter().filter(|a| {
        a.path.segments.iter().map(|s| &s.ident).next().unwrap() == "fuseable"
    }).collect();

    use syn::{
        Meta::{
            List,
            NameValue,
        },
        MetaNameValue,
        MetaList,
        NestedMeta::Meta,
        token::Paren
    };

    let mut virtual_fields = Vec::new();

    fn lit_to_direct_string(lit: &syn::Lit) -> String {
        match lit {
            syn::Lit::Str(str) => {
                str.value()
            }
            _ => {
                panic!("could not convert literal to string: {:?}", lit)
            }
        }
    }

    for attr in &attrs {
        match &attr.interpret_meta() {
            Some(syn::Meta::List(syn::MetaList { nested, ident, paren_token } )) => {
                for nested_meta in nested {
                    match nested_meta {
                        Meta(List(MetaList { nested, ident, paren_token} )) => {
                            if ident == "virtual_field" {
                                let mut virtual_field: VirtualField = Default::default();
                                for nested_meta in nested {
                                    match nested_meta {
                                        Meta(NameValue(MetaNameValue { ident, eq_token, lit })) => {
                                            if ident == "name" {
                                                virtual_field.name = Some(lit_to_direct_string(lit))
                                            } else if ident == "is_dir" {
                                                virtual_field.is_dir = Some(lit_to_direct_string(lit))
                                            } else if ident == "read" {
                                                virtual_field.read = Some(lit_to_direct_string(lit))
                                            } else if ident == "write" {
                                                virtual_field.write = Some(lit_to_direct_string(lit))
                                            }
                                        }
                                        Comma => {}
                                    }
                                }

                                virtual_fields.push(virtual_field);
                            } else {
                                panic!("unhandled meta {:?}", attrs)
                            }
                        },
                        _ => {
                            panic!("unhandled meta {:?}", nested_meta)
                        }
                    }
                }
            }
            _ => {}
        }
    }

    for virtual_field in &virtual_fields {
        println!();
        println!("name: {}", virtual_field.name.clone().unwrap());
        println!("is_dir: {}", virtual_field.is_dir.clone().unwrap());
        println!("read: {}", virtual_field.read.clone().unwrap());
        println!("write: {}", virtual_field.write.clone().unwrap());
    }

// &attr.interpret_meta()

    match ast.data {
        Struct(ref data) => {
            impl_struct(data, &virtual_fields)
        },
        Enum(ref data) => {
            if virtual_fields.len() > 0 {
                panic!("cannot handle virtual fields in enums yet");
            }

            impl_enum(&ast.ident, data)
        },
        _ => unimplemented!(),
    }
}

fn impl_struct(data: &syn::DataStruct, virtual_fields: &Vec<VirtualField>) -> (TokenStream, TokenStream, TokenStream) {
    let (is_dir, read, write) = match data.fields {
        syn::Fields::Named(ref fields) => {
            /*
            let fields_normal: Vec<_> = fields
                .named
                .iter()
                .map(|f| &f.ident)
                .map(|f| quote! { #f })
                .collect();
            let wrapped_fields: Vec<_> = fields
                .named
                .iter()
                .map(|f| &f.ident)
                .map(|f| quote! { &self.#f })
                .collect();
            */

            let prefix = quote! { &self. };

            impl_fields(&fields.named.iter().collect(), &prefix, virtual_fields)
        }
        _ => unimplemented!(),
    };

    (is_dir, read, write)
}

fn impl_enum(name: &Ident, data: &syn::DataEnum) -> (TokenStream, TokenStream, TokenStream) {
    let variants: Vec<_> = data.variants.iter().map(impl_enum_variant).collect();
    let variant_names_read: Vec<_> = data.variants.iter().map(|v| &v.ident).collect();
    let variant_names_is_dir: Vec<_> = variant_names_read.clone();

    let is_dir: Vec<_> = variants.iter().map(|v| &v.0).collect();
    let read: Vec<_> = variants.iter().map(|v| &v.1).collect();
    let write: Vec<_> = variants.iter().map(|v| &v.2).collect();

    let is_dir = quote! {
        use #name::{#(#variant_names_is_dir),*};

        match self {
            #(#is_dir, )*
        }
    };

    let read = quote! {
        use #name::{#(#variant_names_read),*};

        match self {
            #(#read, )*
        }
    };

    let write = quote! {
        Err(())
    };

    (is_dir, read, write)
}

fn impl_enum_variant(variant: &syn::Variant) -> (TokenStream, TokenStream, TokenStream) {
    let name = &variant.ident;

    let (is_dir, read, write) = match variant.fields {
        syn::Fields::Named(ref fields) => {
            let fields: Vec<_> = fields.named.iter().collect();

            if fields.len() == 1 {
                let name = fields[0].clone();
                impl_enum_variant_flatten(&name, false)
            } else {
                impl_enum_variant_namend(&fields)
            }
        }
        syn::Fields::Unnamed(ref fields) => {
            let fields: Vec<_> = fields.unnamed.iter().collect();
            if fields.len() != 1 {
                unimplemented!()
            } else {
                let mut field = fields[0].clone();
                field.ident = Some(Ident::new("value", Span::call_site()));
                impl_enum_variant_flatten(&field, true)
            }
        }
        _ => unimplemented!(),
    };

    let is_dir = quote! {
        #name #is_dir
    };

    let read = quote! {
        #name #read
    };

    let write = quote! {
        #name #write
    };

    (is_dir, read, write)
}

fn impl_enum_variant_flatten(
    name: &syn::Field,
    unnamed: bool
) -> (TokenStream, TokenStream, TokenStream) {
    let name = name.ident.clone().unwrap();
    let wrapped_name = if unnamed {
        quote! {
            ( #name )
        }
    } else {
        quote! {
            { #name }
        }

    };

    let is_dir = quote! {
        #wrapped_name => Fuseable::is_dir(#name, path)
    };

    let read = quote! {
        #wrapped_name => Fuseable::read(#name, path)
    };

    let write = quote! {
        #wrapped_name => Err(())
    };

    (is_dir, read, write)
}

fn impl_enum_variant_namend(fields: &Vec<&syn::Field>) -> (TokenStream, TokenStream, TokenStream) {
    let fields_is_dir: Vec<_> = fields.iter().map(|f| {
        let f = f.ident.clone().unwrap();
        quote! { #f }
    }).collect();
    let fields_read: Vec<_> = fields_is_dir.clone();

    let (fields_impl_is_dir, fields_impl_read, fields_impl_write) =
        impl_fields(&fields, &quote! {}, Vec::new());

    let is_dir = quote! {
        { #(#fields_is_dir),* } => {
            #fields_impl_is_dir
        }
    };

    let read = quote! {
        { #(#fields_read),* } => {
            #fields_impl_read
        }
    };

    let write = quote! {
        => {
            Err(())
        }
    };

    (is_dir, read, write)
}

struct ParsedField {
    ident: Ident,
    skip: bool,
}

fn parse_field(field: &&syn::Field) -> ParsedField {
    let ident = field.ident.clone().unwrap();

    let attrs: Vec<_> = field.attrs.iter().filter(|a| {
        a.path.segments.iter().map(|s| &s.ident).next().unwrap() == "fuseable"
    }).collect();

    let mut skip = false;

    for attr in attrs {
        match &attr.interpret_meta() {
            Some(syn::Meta::List(syn::MetaList { nested, ident, paren_token } )) => {
                match nested.iter().next() {
                    Some(syn::NestedMeta::Meta(syn::Meta::Word(ident))) => {
                        skip = ident == "skip"
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }

    ParsedField {
        ident: ident,
        skip: skip,
    }
}

fn impl_fields(
    fields: &Vec<&Field>,
    prefix: &TokenStream,
    virtual_fields: &Vec<VirtualField>
) -> (TokenStream, TokenStream, TokenStream) {
    let fields: Vec<_> = fields.iter().map(parse_field).filter(|f| !f.skip).map(|f| f.ident.clone()).collect();
    let wrapped_fields: Vec<_> = fields.iter().map(|f| quote!{ #prefix #f }).collect();
    let fields2 = fields.clone();
    let fields3 = fields.clone();
    let wrapped_fields2 = wrapped_fields.clone();

    let read = quote! {
        match path.next() {
            Some(ref name) => {
                match name.as_ref() {
                    #(stringify!(#fields) => Fuseable::read(#wrapped_fields, path), )*
                    _ => Err(())
                }
            }
            None => Ok(Either::Left(vec![#(stringify!(#fields2).to_string()),*]))
        }
    };

    let is_dir = quote! {
        match path.next() {
            Some(ref name) => {
                match name.as_ref() {
                    #(stringify!(#fields3) => Fuseable::is_dir(#wrapped_fields2, path), )*
                    _ => Err(())
                }
            }
            None => Ok(true)
        }
    };

    let write = quote! { Err(()) };

    (is_dir, read, write)
}
