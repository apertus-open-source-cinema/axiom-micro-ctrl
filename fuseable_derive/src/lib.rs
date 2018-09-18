#![recursion_limit="512"]

extern crate proc_macro;
extern crate proc_macro2;
#[macro_use]
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro2::{TokenStream, Ident, Span};
use proc_macro::TokenStream as TS;
use syn::DeriveInput;
use syn::Data::{Struct, Enum};


#[proc_macro_derive(Fuseable)]
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

fn impl_body(ast: &syn::DeriveInput) -> (TokenStream, TokenStream, TokenStream) {
    match ast.data {
        Struct(ref data) => impl_struct(data),
        Enum(ref data) => impl_enum(&ast.ident, data),
        _ => unimplemented!()
    }
}

fn impl_struct(data: &syn::DataStruct) -> (TokenStream, TokenStream, TokenStream) {
    let (is_dir, read, write) = match data.fields {
        syn::Fields::Named(ref fields) => {
            let fields_normal: Vec<_> = fields.named.iter().map(|f| &f.ident).map(|f| quote! { #f }).collect();
            let wrapped_fields: Vec<_> = fields.named.iter().map(|f| &f.ident).map(|f| quote! { &self.#f }).collect();

            impl_fields(&fields_normal, &wrapped_fields)
        },
        _ => unimplemented!() 
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
            let fields: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
            
            if fields.len() == 1 {
                let name = fields[0].clone().unwrap();
                let wrapped_name = quote! {
                    { #name }
                };

                impl_enum_variant_flatten(&name, wrapped_name)
            } else {
                impl_enum_variant_namend(&fields.into_iter().map(|i| i.clone().unwrap()).collect())
            }
        }
        syn::Fields::Unnamed(ref fields) => {
            let fields: Vec<_> = fields.unnamed.iter().collect();
            if fields.len() != 1 {
                unimplemented!() 
            } else {
                let name = Ident::new("value", Span::call_site());

                let wrapped_name = quote! {
                    ( #name )
                };

                impl_enum_variant_flatten(&name, wrapped_name)
            }
        }
        _ => unimplemented!()
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

fn impl_enum_variant_flatten(name: &syn::Ident, wrapped_name: TokenStream) -> (TokenStream, TokenStream, TokenStream) {
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

fn impl_enum_variant_namend(fields: &Vec<syn::Ident>) -> (TokenStream, TokenStream, TokenStream) {
    let fields_is_dir: Vec<_> = fields.iter().map(|f| quote! { #f }).collect();
    let fields_read: Vec<_> = fields_is_dir.clone();

    let (fields_impl_is_dir, fields_impl_read, fields_impl_write) = impl_fields(&fields_is_dir, &fields_is_dir);

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

fn impl_fields(fields: &Vec<TokenStream>, wrapped_fields: &Vec<TokenStream>) -> (TokenStream, TokenStream, TokenStream) {
    let fields2 = fields.clone();

    let read = quote! {
        match path.next() {
            Some(name) => {
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
            Some(name) => {
                match name.as_ref() {
                    #(stringify!(#fields) => Fuseable::is_dir(#wrapped_fields, path), )*
                    _ => Err(())
                }
            }
            None => Ok(true)
        }
    };

    let write = quote! { Err(()) };

    (is_dir, read, write)
}
