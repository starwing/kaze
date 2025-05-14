use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, punctuated::Punctuated, Data, DeriveInput, Expr,
    Fields, Lit, Meta, MetaNameValue, Token, Type, TypePath,
};

/// Derive macro for the `DocumentedToml` trait.
///
/// This macro generates an implementation of the `DocumentedToml` trait
/// for structs. It will extract documentation comments from fields and
/// include them in the resulting TOML.
#[proc_macro_derive(DocumentedToml)]
pub fn documented_toml_derive(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    let struct_doc = extract_doc_comment(&input.attrs);

    // Extract the name of the struct
    let name = input.ident;

    // Check if we're dealing with a struct
    let fields = match input.data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => &fields.named,
            _ => panic!(
                "DocumentedToml only supports structs with named fields"
            ),
        },
        _ => {
            panic!("DocumentedToml only supports structs, not enums or unions")
        }
    };

    // Generate code for each field
    let field_tokens = fields.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();

        // Extract doc comment if present
        let doc_comment = extract_doc_comment(&field.attrs);

        // Check for #[serde(with = "mod_name")]
        let serde_with_module = extract_serde_with_module(&field.attrs);

        // Generate field processing code based on field type
        process_field(
            field_name,
            &field_name_str,
            &field.ty,
            &doc_comment,
            serde_with_module,
        )
    });

    let struct_doc = if let Some(doc) = struct_doc {
        quote! {
            table.decor_mut().set_prefix(concat!("\n# ", #doc, "\n"));
        }
    } else {
        quote! {}
    };

    // Generate the implementation
    let result = quote! {
        impl ::documented_toml::DocumentedToml for #name {
            fn as_toml(&self) -> ::documented_toml::toml_edit::Item {
                let mut item = ::documented_toml::toml_edit::table();
                let mut table = item.as_table_mut().unwrap();
                #struct_doc
                #(#field_tokens)*
                item
            }
        }
    };

    result.into()
}

// Process a field based on its type
fn process_field(
    field_name: &syn::Ident,
    field_name_str: &str,
    field_type: &Type,
    doc_comment: &Option<String>,
    serde_with_module: Option<String>,
) -> proc_macro2::TokenStream {
    if let Some(module_name) = serde_with_module {
        let module_ident =
            syn::Ident::new(&module_name, proc_macro2::Span::call_site());
        if let Some(doc) = doc_comment {
            let process_value = process_value(doc);
            quote! {
                {
                    let key = ::documented_toml::toml_edit::Key::new(#field_name_str);
                    let value = #module_ident::serialize(&self.#field_name, ::documented_toml::ValueSerializer::new()).unwrap();
                    let value = ::documented_toml::toml_edit::value(value);
                    #process_value
                }
            }
        } else {
            quote! {
                {
                    let value = #module_ident::serialize(&self.#field_name, ::documented_toml::ValueSerializer::new()).unwrap();
                    table.insert(#field_name_str, ::documented_toml::toml_edit::Item::Value(value));
                }
            }
        }
    } else {
        match field_type {
            // Handle Option<T> type
            Type::Path(type_path) if is_option_type(type_path) => {
                process_option_field(field_name, field_name_str, doc_comment)
            }
            // Handle Vec<T> type
            Type::Path(type_path) if is_vec_type(type_path) => {
                process_vec_field(field_name, field_name_str, doc_comment)
            }
            // Handle other types - all types implement DocumentedToml now
            _ => {
                process_standard_field(field_name, field_name_str, doc_comment)
            }
        }
    }
}

// Process fields of Option<T> type
fn process_option_field(
    field_name: &syn::Ident,
    field_name_str: &str,
    doc_comment: &Option<String>,
) -> proc_macro2::TokenStream {
    if let Some(doc) = doc_comment {
        let process_array = process_array(doc);
        let process_table = process_table(doc);
        let process_value = process_value(doc);
        quote! {
            if let Some(ref value) = self.#field_name {
                let key = ::documented_toml::toml_edit::Key::new(#field_name_str);
                match (&&&&::documented_toml::Wrap(value)).as_toml() {
                    ::documented_toml::toml_edit::Item::ArrayOfTables(mut array_value) => {
                        #process_array
                    },
                    ::documented_toml::toml_edit::Item::Table(mut table_value) => {
                        #process_table
                    },
                    value => {
                        #process_value
                    },
                }
            }
        }
    } else {
        quote! {
            if let Some(ref value) = self.#field_name {
                let key = ::documented_toml::toml_edit::Key::new(#field_name_str);
                table.insert(#field_name_str, (&&&&documented_toml::Wrap(value)).as_toml());
            }
        }
    }
}

// Process fields of Vec<T> type
fn process_vec_field(
    field_name: &syn::Ident,
    field_name_str: &str,
    doc_comment: &Option<String>,
) -> proc_macro2::TokenStream {
    if let Some(doc) = doc_comment {
        let process_array = process_array(doc);
        let process_value = process_value(doc);
        quote! {
            {
                let key = ::documented_toml::toml_edit::Key::new(#field_name_str);
                match (&&&&::documented_toml::Wrap(&self.#field_name)).as_toml() {
                    ::documented_toml::toml_edit::Item::ArrayOfTables(mut array_value) => {
                        #process_array
                    },
                    value => {
                        #process_value
                    }
                }
            }
        }
    } else {
        quote! {
            {
                let value_result = (&&&&documented_toml::Wrap(&self.#field_name)).as_toml();
                table.insert(#field_name_str, value_result);
            }
        }
    }
}

// Process standard fields (all types implement DocumentedToml)
fn process_standard_field(
    field_name: &syn::Ident,
    field_name_str: &str,
    doc_comment: &Option<String>,
) -> proc_macro2::TokenStream {
    if let Some(doc) = doc_comment {
        let process_array = process_array(doc);
        let process_table = process_table(doc);
        let process_value = process_value(doc);
        quote! {
            {
                let key = ::documented_toml::toml_edit::Key::new(#field_name_str);
                match (&&&&::documented_toml::Wrap(&self.#field_name)).as_toml() {
                    ::documented_toml::toml_edit::Item::ArrayOfTables(mut array_value) => {
                        #process_array
                    },
                    ::documented_toml::toml_edit::Item::Table(mut table_value) => {
                        #process_table
                    },
                    value => {
                        #process_value
                    }
                }
            }
        }
    } else {
        quote! {
            {
                let key = ::documented_toml::toml_edit::Key::new(#field_name_str);
                table.insert(#field_name_str, (&&&&documented_toml::Wrap(&self.#field_name)).as_toml());
            }
        }
    }
}

fn process_array(doc: &String) -> proc_macro2::TokenStream {
    quote! {
        if array_value.len() == 0 {
            let decor = ::documented_toml::toml_edit::Decor::new(concat!("# ", #doc, "\n"), " ");
            let key = key.with_decor(decor);
            table.insert_formatted(&key, ::documented_toml::toml_edit::Item::None);
        } else {
            array_value.get_mut(0).unwrap().decor_mut().set_prefix(concat!("\n# ", #doc, "\n"));
            table.insert_formatted(&key, ::documented_toml::toml_edit::Item::ArrayOfTables(array_value));
        }
    }
}

fn process_table(doc: &String) -> proc_macro2::TokenStream {
    quote! {
        table_value.decor_mut().set_prefix(concat!("\n# ", #doc, "\n"));
        table.insert_formatted(&key, ::documented_toml::toml_edit::Item::Table(table_value));
    }
}

fn process_value(doc: &String) -> proc_macro2::TokenStream {
    quote! {
        let decor = ::documented_toml::toml_edit::Decor::new(concat!("# ", #doc, "\n"), " ");
        let key = key.with_decor(decor);
        table.insert_formatted(&key, value);
    }
}

// Helper function to extract documentation comments from attributes
fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let mut doc_lines = Vec::new();

    for attr in attrs {
        if attr.meta.path().is_ident("doc") {
            if let Meta::NameValue(MetaNameValue {
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }),
                ..
            }) = &attr.meta
            {
                let doc_line = lit_str.value().trim().to_string();
                doc_lines.push(doc_line);
            }
        }
    }

    if doc_lines.is_empty() {
        None
    } else {
        Some(doc_lines.join("\n# "))
    }
}

// Helper function to extract serde_with module
fn extract_serde_with_module(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("serde") {
            // Use attr.parse_args_with instead of parse_meta for more direct parsing
            // of attribute arguments like `with = "module_name"`
            if let Ok(parsed_args) = attr.parse_args_with(
                Punctuated::<Meta, Token![,]>::parse_terminated,
            ) {
                for meta_item in parsed_args {
                    if let Meta::NameValue(mnv) = meta_item {
                        if mnv.path.is_ident("with") {
                            if let Expr::Lit(expr_lit) = &mnv.value {
                                if let Lit::Str(lit_str) = &expr_lit.lit {
                                    return Some(lit_str.value());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// Check if a type is Option<T>
fn is_option_type(type_path: &TypePath) -> bool {
    if let Some(segment) = type_path.path.segments.last() {
        return segment.ident == "Option";
    }
    false
}

// Check if a type is Vec<T>
fn is_vec_type(type_path: &TypePath) -> bool {
    if let Some(segment) = type_path.path.segments.last() {
        return segment.ident == "Vec";
    }
    false
}
