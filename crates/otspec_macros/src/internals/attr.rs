use crate::internals::respan::respan;
use crate::internals::symbol::*;
use crate::internals::Ctxt;
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;

use syn;
use syn::parse::{self, Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::Meta::{List, NameValue, Path};
use syn::NestedMeta::{Lit, Meta};
use syn::Token;

// This module handles parsing of `#[serde(...)]` attributes. The entrypoints
// are `attr::Container::from_ast`, `attr::Variant::from_ast`, and
// `attr::Field::from_ast`. Each returns an instance of the corresponding
// struct. Note that none of them return a Result. Unrecognized, malformed, or
// duplicated attributes result in a span_err but otherwise are ignored. The
// user will see errors simultaneously for all bad attributes in the crate
// rather than just the first.

struct Attr<'c, T> {
    cx: &'c Ctxt,
    name: Symbol,
    tokens: TokenStream,
    value: Option<T>,
}

impl<'c, T> Attr<'c, T> {
    fn none(cx: &'c Ctxt, name: Symbol) -> Self {
        Attr {
            cx,
            name,
            tokens: TokenStream::new(),
            value: None,
        }
    }

    fn set<A: ToTokens>(&mut self, obj: A, value: T) {
        let tokens = obj.into_token_stream();

        if self.value.is_some() {
            self.cx
                .error_spanned_by(tokens, format!("duplicate serde attribute `{}`", self.name));
        } else {
            self.tokens = tokens;
            self.value = Some(value);
        }
    }

    fn set_if_none(&mut self, value: T) {
        if self.value.is_none() {
            self.value = Some(value);
        }
    }

    fn get(self) -> Option<T> {
        self.value
    }

    fn get_with_tokens(self) -> Option<(TokenStream, T)> {
        match self.value {
            Some(v) => Some((self.tokens, v)),
            None => None,
        }
    }
}

struct BoolAttr<'c>(Attr<'c, ()>);

impl<'c> BoolAttr<'c> {
    fn none(cx: &'c Ctxt, name: Symbol) -> Self {
        BoolAttr(Attr::none(cx, name))
    }

    fn set_true<A: ToTokens>(&mut self, obj: A) {
        self.0.set(obj, ());
    }

    fn get(&self) -> bool {
        self.0.value.is_some()
    }
}

/// Represents struct or enum attribute information.
pub struct Container {
    default: Default,
    ser_bound: Option<Vec<syn::WherePredicate>>,
}

impl Container {
    /// Extract out the `#[serde(...)]` attributes from an item.
    pub fn from_ast(cx: &Ctxt, item: &syn::DeriveInput) -> Self {
        let mut default = Attr::none(cx, DEFAULT);
        let mut ser_bound = Attr::none(cx, BOUND);
        let mut de_bound = Attr::none(cx, BOUND);

        for meta_item in item
            .attrs
            .iter()
            .flat_map(|attr| get_serde_meta_items(cx, attr))
            .flatten()
        {
            match &meta_item {
                // Parse `#[serde(default)]`
                Meta(Path(word)) if word == DEFAULT => match &item.data {
                    syn::Data::Struct(syn::DataStruct { fields, .. }) => match fields {
                        syn::Fields::Named(_) => {
                            default.set(word, Default::Default);
                        }
                        syn::Fields::Unnamed(_) | syn::Fields::Unit => cx.error_spanned_by(
                            fields,
                            "#[serde(default)] can only be used on structs with named fields",
                        ),
                    },
                    syn::Data::Enum(syn::DataEnum { enum_token, .. }) => cx.error_spanned_by(
                        enum_token,
                        "#[serde(default)] can only be used on structs with named fields",
                    ),
                    syn::Data::Union(syn::DataUnion { union_token, .. }) => cx.error_spanned_by(
                        union_token,
                        "#[serde(default)] can only be used on structs with named fields",
                    ),
                },

                // Parse `#[serde(default = "...")]`
                Meta(NameValue(m)) if m.path == DEFAULT => {
                    if let Ok(path) = parse_lit_into_expr_path(cx, DEFAULT, &m.lit) {
                        match &item.data {
                            syn::Data::Struct(syn::DataStruct { fields, .. }) => {
                                match fields {
                                    syn::Fields::Named(_) => {
                                        default.set(&m.path, Default::Path(path));
                                    }
                                    syn::Fields::Unnamed(_) | syn::Fields::Unit => cx
                                        .error_spanned_by(
                                            fields,
                                            "#[serde(default = \"...\")] can only be used on structs with named fields",
                                        ),
                                }
                            }
                            syn::Data::Enum(syn::DataEnum { enum_token, .. }) => cx
                                .error_spanned_by(
                                    enum_token,
                                    "#[serde(default = \"...\")] can only be used on structs with named fields",
                                ),
                            syn::Data::Union(syn::DataUnion {
                                union_token, ..
                            }) => cx.error_spanned_by(
                                union_token,
                                "#[serde(default = \"...\")] can only be used on structs with named fields",
                            ),
                        }
                    }
                }

                // Parse `#[serde(bound = "T: SomeBound")]`
                Meta(NameValue(m)) if m.path == BOUND => {
                    if let Ok(where_predicates) = parse_lit_into_where(cx, BOUND, BOUND, &m.lit) {
                        ser_bound.set(&m.path, where_predicates.clone());
                        de_bound.set(&m.path, where_predicates);
                    }
                }

                Meta(meta_item) => {
                    let path = meta_item
                        .path()
                        .into_token_stream()
                        .to_string()
                        .replace(' ', "");
                    cx.error_spanned_by(
                        meta_item.path(),
                        format!("unknown serde container attribute `{}`", path),
                    );
                }

                Lit(lit) => {
                    cx.error_spanned_by(lit, "unexpected literal in serde container attribute");
                }
            }
        }

        let mut is_packed = false;
        for attr in &item.attrs {
            if attr.path.is_ident("repr") {
                let _ = attr.parse_args_with(|input: ParseStream| {
                    while let Some(token) = input.parse()? {
                        if let TokenTree::Ident(ident) = token {
                            is_packed |= ident == "packed";
                        }
                    }
                    Ok(())
                });
            }
        }

        Container {
            default: default.get().unwrap_or(Default::None),
            ser_bound: ser_bound.get(),
        }
    }
    pub fn default(&self) -> &Default {
        &self.default
    }

    pub fn ser_bound(&self) -> Option<&[syn::WherePredicate]> {
        self.ser_bound.as_ref().map(|vec| &vec[..])
    }
}

/// Represents variant attribute information
pub struct Variant {}

impl Variant {
    pub fn from_ast(_cx: &Ctxt, _variant: &syn::Variant) -> Self {
        Variant {}
    }
}

/// Represents field attribute information
pub struct Field {
    pub offset_base: bool,
    default: Default,
    serialize_with: Option<syn::ExprPath>,
    deserialize_with: Option<syn::ExprPath>,
    ser_bound: Option<Vec<syn::WherePredicate>>,
}

/// Represents the default to use for a field when deserializing.
pub enum Default {
    /// Field must always be specified because it does not have a default.
    None,
    /// The default is given by `std::default::Default::default()`.
    Default,
    /// The default is given by this function.
    Path(syn::ExprPath),
}

impl Field {
    /// Extract out the `#[serde(...)]` attributes from a struct field.
    pub fn from_ast(
        cx: &Ctxt,
        _index: usize,
        field: &syn::Field,
        attrs: Option<&Variant>,
        container_default: &Default,
    ) -> Self {
        let skip_deserializing = BoolAttr::none(cx, SKIP_DESERIALIZING);
        let mut default = Attr::none(cx, DEFAULT);
        let mut serialize_with = Attr::none(cx, SERIALIZE_WITH);
        let mut deserialize_with = Attr::none(cx, DESERIALIZE_WITH);
        let ser_bound = Attr::none(cx, BOUND);
        let mut offset_base = BoolAttr::none(cx, OFFSET_BASE);

        for meta_item in field
            .attrs
            .iter()
            .flat_map(|attr| get_serde_meta_items(cx, attr))
            .flatten()
        {
            match &meta_item {
                // Parse `#[serde(serialize_with = "...")]`
                Meta(NameValue(m)) if m.path == SERIALIZE_WITH => {
                    if let Ok(path) = parse_lit_into_expr_path(cx, SERIALIZE_WITH, &m.lit) {
                        serialize_with.set(&m.path, path);
                    }
                }

                // Parse `#[serde(deserialize_with = "...")]`
                Meta(NameValue(m)) if m.path == DESERIALIZE_WITH => {
                    if let Ok(path) = parse_lit_into_expr_path(cx, DESERIALIZE_WITH, &m.lit) {
                        deserialize_with.set(&m.path, path);
                    }
                }

                // Parse `#[serde(with = "...")]`
                Meta(NameValue(m)) if m.path == WITH => {
                    if let Ok(path) = parse_lit_into_expr_path(cx, WITH, &m.lit) {
                        let ser_path = path.clone();
                        serialize_with.set(&m.path, ser_path);
                        let de_path = path;
                        deserialize_with.set(&m.path, de_path);
                    }
                }

                // Parse `#[serde(offset_base)]`
                Meta(Path(word)) if word == OFFSET_BASE => {
                    offset_base.set_true(word);
                }

                Meta(meta_item) => {
                    let path = meta_item
                        .path()
                        .into_token_stream()
                        .to_string()
                        .replace(' ', "");
                    cx.error_spanned_by(
                        meta_item.path(),
                        format!("unknown serde field attribute `{}`", path),
                    );
                }

                Lit(lit) => {
                    cx.error_spanned_by(lit, "unexpected literal in serde field attribute");
                }
            }
        }

        // Is skip_deserializing, initialize the field to Default::default() unless a
        // different default is specified by `#[serde(default = "...")]` on
        // ourselves or our container (e.g. the struct we are in).
        if let Default::None = *container_default {
            if skip_deserializing.0.value.is_some() {
                default.set_if_none(Default::Default);
            }
        }

        Field {
            offset_base: offset_base.get(),
            default: default.get().unwrap_or(Default::None),
            serialize_with: serialize_with.get(),
            deserialize_with: deserialize_with.get(),
            ser_bound: ser_bound.get(),
        }
    }

    pub fn serialize_with(&self) -> Option<&syn::ExprPath> {
        self.serialize_with.as_ref()
    }

    pub fn deserialize_with(&self) -> Option<&syn::ExprPath> {
        self.deserialize_with.as_ref()
    }

    pub fn ser_bound(&self) -> Option<&[syn::WherePredicate]> {
        self.ser_bound.as_ref().map(|vec| &vec[..])
    }
}

pub fn get_serde_meta_items(cx: &Ctxt, attr: &syn::Attribute) -> Result<Vec<syn::NestedMeta>, ()> {
    if attr.path != SERDE {
        return Ok(Vec::new());
    }

    match attr.parse_meta() {
        Ok(List(meta)) => Ok(meta.nested.into_iter().collect()),
        Ok(other) => {
            cx.error_spanned_by(other, "expected #[serde(...)]");
            Err(())
        }
        Err(err) => {
            cx.syn_error(err);
            Err(())
        }
    }
}

fn get_lit_str<'a>(cx: &Ctxt, attr_name: Symbol, lit: &'a syn::Lit) -> Result<&'a syn::LitStr, ()> {
    get_lit_str2(cx, attr_name, attr_name, lit)
}

fn get_lit_str2<'a>(
    cx: &Ctxt,
    attr_name: Symbol,
    meta_item_name: Symbol,
    lit: &'a syn::Lit,
) -> Result<&'a syn::LitStr, ()> {
    if let syn::Lit::Str(lit) = lit {
        Ok(lit)
    } else {
        cx.error_spanned_by(
            lit,
            format!(
                "expected serde {} attribute to be a string: `{} = \"...\"`",
                attr_name, meta_item_name
            ),
        );
        Err(())
    }
}

fn parse_lit_into_expr_path(
    cx: &Ctxt,
    attr_name: Symbol,
    lit: &syn::Lit,
) -> Result<syn::ExprPath, ()> {
    let string = get_lit_str(cx, attr_name, lit)?;
    parse_lit_str(string).map_err(|_| {
        cx.error_spanned_by(lit, format!("failed to parse path: {:?}", string.value()))
    })
}

fn parse_lit_into_where(
    cx: &Ctxt,
    attr_name: Symbol,
    meta_item_name: Symbol,
    lit: &syn::Lit,
) -> Result<Vec<syn::WherePredicate>, ()> {
    let string = get_lit_str2(cx, attr_name, meta_item_name, lit)?;
    if string.value().is_empty() {
        return Ok(Vec::new());
    }

    let where_string = syn::LitStr::new(&format!("where {}", string.value()), string.span());

    parse_lit_str::<syn::WhereClause>(&where_string)
        .map(|wh| wh.predicates.into_iter().collect())
        .map_err(|err| cx.error_spanned_by(lit, err))
}

fn parse_lit_str<T>(s: &syn::LitStr) -> parse::Result<T>
where
    T: Parse,
{
    let tokens = spanned_tokens(s)?;
    syn::parse2(tokens)
}

fn spanned_tokens(s: &syn::LitStr) -> parse::Result<TokenStream> {
    let stream = syn::parse_str(&s.value())?;
    Ok(respan(stream, s.span()))
}
