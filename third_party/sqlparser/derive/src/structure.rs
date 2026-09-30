// Licensed under Apache-2.0. Copyright Darmok contributors.
//! Generate exhaustive, non-recursive access to every owned AST field.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, GenericParam};

pub(crate) fn derive_structure(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    for parameter in &mut generics.params {
        if let GenericParam::Type(parameter) = parameter {
            parameter
                .bounds
                .push(syn::parse_quote!(sqlparser::ast::AstNode));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let arms = |owned: bool| match &input.data {
        Data::Struct(data) => arm(quote!(Self), &data.fields, owned),
        Data::Enum(data) => {
            let arms = data.variants.iter().map(|variant| {
                let name = &variant.ident;
                arm(quote!(Self::#name), &variant.fields, owned)
            });
            quote!(#(#arms),*)
        }
        Data::Union(_) => panic!("AST ownership cannot contain unions"),
    };
    let borrowed = arms(false);
    let owned = arms(true);
    quote! {
        impl #impl_generics sqlparser::ast::AstNode for #name #ty_generics #where_clause {
            fn children<'a>(&'a self, children: &mut dyn FnMut(&'a dyn sqlparser::ast::AstNode)) {
                match self { #borrowed }
            }
            fn dismantle(self: sqlparser::ast::NodeBox<Self>, pending: &mut sqlparser::ast::NodeQueue) {
                match *self { #owned }
            }
        }
    }
}

fn arm(path: TokenStream, fields: &Fields, owned: bool) -> TokenStream {
    let names: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            field
                .ident
                .clone()
                .unwrap_or_else(|| format_ident!("field_{index}"))
        })
        .collect();
    let pattern = match fields {
        Fields::Named(_) => quote!(#path { #(#names),* }),
        Fields::Unnamed(_) => quote!(#path ( #(#names),* )),
        Fields::Unit => path,
    };
    let visits = names.iter().map(|name| {
        if owned {
            quote!(sqlparser::ast::enqueue_node(#name, pending);)
        } else {
            quote!(children(#name);)
        }
    });
    quote!(#pattern => { #(#visits)* })
}
