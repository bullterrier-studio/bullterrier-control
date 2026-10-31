//! The derives of bullterrier-control, used through its re-exports (`bullterrier_control::Params`,
//! `bullterrier_control::Choice`); their documentation and examples are there. They only write code: every rule a
//! declaration must follow is checked by `ParamTable::new` in the core, the one place those rules live.

use proc_macro::TokenStream;
use proc_macro2::{Literal, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::meta::ParseNestedMeta;
use syn::{Data, DeriveInput, Expr, Fields, Ident, Lit, Type, UnOp, parse_macro_input};

/// `#[derive(Params)]` on a struct with named fields, each `#[param(default = .., unit = .., min = .., max = ..,
/// advanced, cyclic)]` and a doc comment: writes `Default` (from each `default`) and `Params` (the table, built once,
/// and `from_values`). `#[param(rule = path)]` on the struct adds a rule between fields to the table.
#[proc_macro_derive(Params, attributes(param))]
pub fn derive_params(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    params(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// `#[derive(Choice)]` on an enum of unit variants: writes `Choice`, each variant named in snake case (`SawDown` is
/// `saw_down`) unless `#[choice(name = "saw-down")]` names it.
#[proc_macro_derive(Choice, attributes(choice))]
pub fn derive_choice(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    choice(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// One field of a `Params` struct, as its attribute and doc comment declare it.
struct Field {
    ident: Ident,
    ty: Type,
    default: Expr,
    doc: String,
    unit: Option<Ident>,
    min: Option<f64>,
    max: Option<f64>,
    advanced: bool,
    cyclic: bool,
}

fn params(input: &DeriveInput) -> syn::Result<Tokens> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "a Params struct takes no generic parameters",
        ));
    }
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => named.named.iter().map(field).collect::<syn::Result<Vec<_>>>()?,
            Fields::Unit => Vec::new(),
            Fields::Unnamed(_) => {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "a Params struct has named fields",
                ));
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            return Err(syn::Error::new_spanned(&input.ident, "Params is derived for a struct"));
        }
    };
    let rule = struct_rule(&input.attrs)?;
    let name = &input.ident;
    let krate = quote!(::bullterrier_control::params);
    let idents: Vec<&Ident> = fields.iter().map(|field| &field.ident).collect();
    let names: Vec<String> = idents.iter().map(ToString::to_string).collect();
    let types: Vec<&Type> = fields.iter().map(|field| &field.ty).collect();
    let defaults = fields.iter().map(|field| match &field.default {
        // a string literal is the default of a `String` field
        Expr::Lit(literal) if matches!(literal.lit, Lit::Str(_)) => quote!(::std::string::String::from(#literal)),
        other => quote!(#other),
    });
    let declared = fields.iter().map(|field| {
        let Field {
            ident,
            ty,
            doc,
            advanced,
            cyclic,
            ..
        } = field;
        let text = ident.to_string();
        let unit = field.unit.as_ref().map_or_else(
            || quote!(::core::option::Option::None),
            |unit| quote!(::core::option::Option::Some(#krate::ParamUnit::#unit)),
        );
        let min = bound(field.min);
        let max = bound(field.max);
        quote! {
            #krate::Param {
                name: ::std::string::String::from(#text),
                kind: <#ty as #krate::ParamType>::kind(),
                default: #krate::ParamType::to_json(&defaults.#ident),
                doc: ::std::string::String::from(#doc),
                unit: #unit,
                min: #min,
                max: #max,
                advanced: #advanced,
                cyclic: #cyclic,
            }
        }
    });
    // a struct with no field has no defaults to read
    let read_defaults =
        (!fields.is_empty()).then(|| quote!(let defaults = <Self as ::core::default::Default>::default();));
    let with_rule = rule.map(|path| {
        quote! {
            .map(|table| table.with_rule(|values| #path(&<Self as #krate::Params>::from_values(values))))
        }
    });
    Ok(quote! {
        impl ::core::default::Default for #name {
            fn default() -> Self {
                Self { #(#idents: #defaults,)* }
            }
        }

        impl #krate::Params for #name {
            fn table() -> ::core::result::Result<&'static #krate::ParamTable, #krate::ParamError> {
                static TABLE: ::std::sync::OnceLock<::core::result::Result<#krate::ParamTable, #krate::ParamError>> =
                    ::std::sync::OnceLock::new();
                TABLE
                    .get_or_init(|| {
                        #read_defaults
                        #krate::ParamTable::new(::std::vec![#(#declared),*]) #with_rule
                    })
                    .as_ref()
                    .map_err(::core::clone::Clone::clone)
            }

            fn from_values(values: &#krate::Values) -> Self {
                #read_defaults
                Self {
                    #(#idents: values
                        .get(#names)
                        .and_then(<#types as #krate::ParamType>::from_json)
                        .unwrap_or(defaults.#idents),)*
                }
            }
        }
    })
}

/// The path of the rule between fields, `#[param(rule = path)]` on the struct, if there is one.
fn struct_rule(attributes: &[syn::Attribute]) -> syn::Result<Option<syn::Path>> {
    let mut rule = None;
    for attribute in attributes.iter().filter(|attribute| attribute.path().is_ident("param")) {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("rule") {
                once(&meta, &mut rule, meta.value()?.parse::<syn::Path>()?)
            } else {
                Err(meta.error("unknown key: a Params struct takes rule"))
            }
        })?;
    }
    Ok(rule)
}

/// A bound as the code of an `Option<f64>`.
fn bound(value: Option<f64>) -> Tokens {
    match value {
        // a literal is unsigned: a negative bound is the negation of one
        Some(value) if value.is_sign_negative() => {
            let magnitude = Literal::f64_suffixed(-value);
            quote!(::core::option::Option::Some(-#magnitude))
        }
        Some(value) => {
            let magnitude = Literal::f64_suffixed(value);
            quote!(::core::option::Option::Some(#magnitude))
        }
        None => quote!(::core::option::Option::None),
    }
}

/// A field with its `#[param(..)]` read and its doc comment joined.
fn field(field: &syn::Field) -> syn::Result<Field> {
    let Some(ident) = field.ident.clone() else {
        return Err(syn::Error::new_spanned(field, "a Params struct has named fields"));
    };
    let mut default = None;
    let mut unit = None;
    let mut min = None;
    let mut max = None;
    let mut advanced = false;
    let mut cyclic = false;
    for attribute in field
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("param"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") {
                once(&meta, &mut default, meta.value()?.parse::<Expr>()?)
            } else if meta.path.is_ident("unit") {
                once(&meta, &mut unit, meta.value()?.parse::<Ident>()?)
            } else if meta.path.is_ident("min") {
                once(&meta, &mut min, number(&meta)?)
            } else if meta.path.is_ident("max") {
                once(&meta, &mut max, number(&meta)?)
            } else if meta.path.is_ident("advanced") {
                advanced = true;
                Ok(())
            } else if meta.path.is_ident("cyclic") {
                cyclic = true;
                Ok(())
            } else {
                Err(meta.error("unknown key: a param takes default, unit, min, max, advanced and cyclic"))
            }
        })?;
    }
    let Some(default) = default else {
        return Err(syn::Error::new_spanned(
            &ident,
            "every field needs #[param(default = ..)]",
        ));
    };
    Ok(Field {
        ty: field.ty.clone(),
        doc: doc(&field.attrs),
        ident,
        default,
        unit,
        min,
        max,
        advanced,
        cyclic,
    })
}

/// Set `slot` to `value`, unless the key was given already.
fn once<T>(meta: &ParseNestedMeta, slot: &mut Option<T>, value: T) -> syn::Result<()> {
    if slot.is_some() {
        return Err(meta.error("this key is given twice"));
    }
    *slot = Some(value);
    Ok(())
}

/// The number literal after `=`, a minus sign allowed (`-1e6`, `4294967295`, `0.5`).
fn number(meta: &ParseNestedMeta) -> syn::Result<f64> {
    let expr = meta.value()?.parse::<Expr>()?;
    let (negative, literal) = match &expr {
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => (true, &*unary.expr),
        other => (false, other),
    };
    let magnitude = match literal {
        Expr::Lit(literal) => match &literal.lit {
            Lit::Int(int) => int.base10_parse::<f64>()?,
            Lit::Float(float) => float.base10_parse::<f64>()?,
            _ => return Err(syn::Error::new_spanned(&expr, "min and max are number literals")),
        },
        _ => return Err(syn::Error::new_spanned(&expr, "min and max are number literals")),
    };
    if !magnitude.is_finite() {
        return Err(syn::Error::new_spanned(&expr, "min and max are finite numbers"));
    }
    Ok(if negative { -magnitude } else { magnitude })
}

/// The doc comment: its lines trimmed and joined by spaces, a blank line kept as a paragraph break.
fn doc(attributes: &[syn::Attribute]) -> String {
    let lines = attributes.iter().filter_map(|attribute| match &attribute.meta {
        syn::Meta::NameValue(pair) if pair.path.is_ident("doc") => match &pair.value {
            Expr::Lit(literal) => match &literal.lit {
                Lit::Str(text) => Some(text.value()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    });
    let mut paragraphs: Vec<String> = vec![String::new()];
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            paragraphs.push(String::new());
        } else if let Some(last) = paragraphs.last_mut() {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(line);
        }
    }
    paragraphs.retain(|paragraph| !paragraph.is_empty());
    paragraphs.join("\n\n")
}

fn choice(input: &DeriveInput) -> syn::Result<Tokens> {
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(&input.ident, "Choice is derived for an enum"));
    };
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "a Choice enum takes no generic parameters",
        ));
    }
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "a Choice enum has at least one variant",
        ));
    }
    let mut variants = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(variant, "a Choice enum has unit variants only"));
        }
        let mut name = None;
        for attribute in variant
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("choice"))
        {
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    once(&meta, &mut name, meta.value()?.parse::<syn::LitStr>()?.value())
                } else {
                    Err(meta.error("unknown key: a choice takes name"))
                }
            })?;
        }
        let name = name.unwrap_or_else(|| snake_case(&variant.ident.to_string()));
        if names.contains(&name) {
            return Err(syn::Error::new_spanned(
                variant,
                format!("the name {name:?} is given twice"),
            ));
        }
        names.push(name);
        variants.push(format_ident!("{}", variant.ident));
    }
    let name = &input.ident;
    Ok(quote! {
        impl ::bullterrier_control::params::Choice for #name {
            const NAMES: &'static [&'static str] = &[#(#names),*];

            fn from_name(name: &str) -> ::core::option::Option<Self> {
                match name {
                    #(#names => ::core::option::Option::Some(Self::#variants),)*
                    _ => ::core::option::Option::None,
                }
            }

            fn name(&self) -> &'static str {
                match self {
                    #(Self::#variants => #names,)*
                }
            }
        }
    })
}

/// `SawDown` as `saw_down`: an underscore before each capital but the first, all in lower case.
fn snake_case(name: &str) -> String {
    let mut snake = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.extend(character.to_lowercase());
    }
    snake
}
