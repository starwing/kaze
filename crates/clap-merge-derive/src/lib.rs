extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::Error, parse_macro_input, punctuated::Punctuated, token::Comma,
    Data, DataStruct, DeriveInput, Expr, ExprLit, Fields, FieldsNamed, Lit,
};

#[proc_macro_derive(
    ClapMerge,
    attributes(arg, command, subcommand, clap_merge)
)]
pub fn clap_merge_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    // Generate the implementation
    impl_merge(input)
        .unwrap_or_else(|err| {
            let msg = err.to_string();
            quote! {
                compile_error!(#msg);
            }
        })
        .into()
}

/// Main function to generate ClapMerge implementation
fn impl_merge(
    input: DeriveInput,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let struct_name = &input.ident; // Extract the struct name
    let mut default_body = proc_macro2::TokenStream::new();
    let mut clap_merge_body = proc_macro2::TokenStream::new();
    let mut create_default = true;
    let mut clap = quote! { clap };

    if let Some(attr) = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("clap_merge"))
    {
        attr.parse_nested_meta(|nested| {
            if nested.path.is_ident("no_default") {
                create_default = false;
            }
            if nested.path.is_ident("crate") {
                let value = nested.value()?;
                let value: syn::Path = value.parse()?;
                clap = quote! { #value };
            }
            Ok(())
        })?;
    }

    // Ensure it's a struct
    let fields = retrive_named_fields(&input, &struct_name)?;

    // Iterate over each field in the struct
    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap(); // Field name
        let mut field_id = field_name.to_string(); // Default ID is the field name
        let ty = &field.ty; // Field type

        let mut skip_field = false;
        let mut recursive = false;
        let mut cur_default = None;

        // Parse attributes on the field
        for attr in &field.attrs {
            if attr.path().is_ident("arg") {
                let (id, skip, def) = parse_arg_attrs(attr, &ty, &clap)?;
                if let Some(id) = id {
                    field_id = id;
                }
                skip_field = skip_field || skip;
                cur_default = def.or(cur_default);
            } else if attr.path().is_ident("command") {
                // Parse nested attributes
                let punctuated = attr.parse_args_with(
                    Punctuated::<Expr, Comma>::parse_terminated,
                )?;

                for item in punctuated {
                    if let Expr::Path(p) = &item {
                        if p.path.is_ident("flatten") {
                            recursive = true;
                            skip_field = false;
                        }
                    }
                }
            } else if attr.path().is_ident("subcommand") {
                recursive = true;
                skip_field = false;
            }
        }

        default_body.extend({
            let def = cur_default.unwrap_or_else(|| {
                quote! { Default::default() }
            });
            quote! { #field_name: #def, }
        });

        // Skip the field if `#[arg(skip)]` is present
        if skip_field {
            continue;
        }

        // Generate different logic based on type and attributes
        if recursive {
            // If attribute is `command(fatten)` or `subcommand`
            clap_merge_body.extend(quote! {
                changed = self.#field_name.merge(args) || changed;
            });
        } else if is_option_type(ty) {
            // If the type is Option<T>
            clap_merge_body.extend(quote! {
                if args.value_source(#field_id) == Some(ValueSource::CommandLine) {
                    self.#field_name = args.get_one(#field_id).cloned();
                    changed = true;
                }
            });
        } else {
            // Non-optional types
            clap_merge_body.extend(quote!{
                if args.value_source(#field_id) == Some(ValueSource::CommandLine) {
                    self.#field_name = args.get_one(#field_id).cloned().unwrap();
                    changed = true;
                }
            });
        }
    }

    let default_body = if create_default {
        quote! {
            impl Default for #struct_name {
                fn default() -> Self {
                    Self{ #default_body }
                }
            }
        }
    } else {
        quote! {}
    };

    // Generate the full implementation for the merge method
    Ok(quote! {
        #default_body

        impl ClapMerge for #struct_name {
            fn merge(&mut self, args: &#clap::ArgMatches) -> bool {
                use #clap::parser::ValueSource;
                let mut changed = false;

                #clap_merge_body

                changed
            }
        }
    })
}

fn retrive_named_fields(
    input: &DeriveInput,
    struct_name: &syn::Ident,
) -> Result<Punctuated<syn::Field, Comma>, Error> {
    match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(FieldsNamed { named: fields, .. }),
            ..
        }) => Ok(fields.clone()),
        _ => Err(Error::new_spanned(
            struct_name,
            "ClapMerge can only be derived for structs has named fields",
        )),
    }
}

fn parse_arg_attrs(
    attr: &syn::Attribute,
    ty: &syn::Type,
    clap_path: &proc_macro2::TokenStream,
) -> Result<(Option<String>, bool, Option<proc_macro2::TokenStream>), syn::Error>
{
    let mut field_id = None;
    let mut skip_field = false;
    let mut value_parser = None;
    let mut default_body = None;

    let punctuated =
        attr.parse_args_with(Punctuated::<Expr, Comma>::parse_terminated)?;

    for item in punctuated {
        if let Expr::Assign(assign) = item {
            // process key = value
            if let Expr::Path(path) = assign.left.as_ref() {
                if path.path.is_ident("id") {
                    // process id
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }) = assign.right.as_ref()
                    {
                        field_id = Some(lit_str.value());
                    } else {
                        return Err(Error::new_spanned(
                            assign.right,
                            "id must be a string literal",
                        ));
                    }
                } else if path.path.is_ident("value_parser") {
                    // process value_parser
                    value_parser = Some(*assign.right);
                } else if path.path.is_ident("default_value_t") {
                    let default_value = &assign.right;
                    default_body = Some(quote! { #default_value });
                } else if path.path.is_ident("default_value")
                    || path.path.is_ident("default_missing_value")
                    || path.path.is_ident("default_value_os")
                {
                    default_body = Some(make_default(
                        &assign.right,
                        value_parser.as_ref().map(|vp| quote! { #vp }),
                        ty,
                        clap_path,
                    ));
                }
            }
        } else if let Expr::Path(path) = item {
            // process `#[arg(skip)]`
            if path.path.is_ident("skip") {
                skip_field = true;
            }
        }
    }
    return Ok((field_id, skip_field, default_body));
}

/// Creates a default value for a field based on the type and value parser
fn make_default(
    default_value: &Expr,
    value_parser: Option<proc_macro2::TokenStream>,
    ty: &syn::Type,
    clap_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let inner_type = get_option_inner_type(ty);
    let value_parser = value_parser
        .unwrap_or(quote! { #clap_path::value_parser!(#inner_type) });
    let to_option =
        is_option_type(ty).then(|| quote! { .map(std::option::Option::Some) });
    quote! {{
        use #clap_path::{Arg, Command, Error, error::ErrorKind};
        const dummy_id: &str = "__clap_derive_dummy__";
        Command::new("__clap_derive_tmp__")
            .arg(Arg::new(dummy_id)
                .default_value(#default_value)
                .value_parser(#value_parser))
            .try_get_matches()
            .and_then(|m| m.get_one::<#inner_type>(dummy_id)
                .cloned().ok_or_else(||
                    Error::new(ErrorKind::InvalidValue)))
            #to_option
            .unwrap_or_else(|_|
                panic!("Invalid clap default value `{:?}` for type `{}`",
                    #default_value, stringify!(#ty)))
    }}
}

// check if the type is `Option<T>`
fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        type_path
            .path
            .segments
            .last()
            .map_or(false, |seg| seg.ident == "Option")
    } else {
        false
    }
}

fn get_option_inner_type(ty: &syn::Type) -> proc_macro2::TokenStream {
    if is_option_type(&ty) {
        if let syn::Type::Path(type_path) = &ty {
            if let syn::PathArguments::AngleBracketed(args) =
                &type_path.path.segments.last().unwrap().arguments
            {
                let args = args.args.clone();
                return quote! { #args };
            }
        }
    }
    quote! { #ty }
}
