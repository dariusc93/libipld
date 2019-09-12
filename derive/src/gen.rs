use proc_macro2::{TokenStream, TokenTree};
use quote::quote;
use syn::{Attribute, Data, Field, Fields};
use synstructure::{BindingInfo, Structure, VariantInfo};

pub enum Attr {
    Name(String),
    Repr(String),
}

impl Attr {
    pub fn from_attr(attr: &Attribute) -> Option<Self> {
        if attr.path.segments[0].ident != "ipld" {
            return None;
        }
        if let TokenTree::Group(group) = attr.tokens.clone().into_iter().next().unwrap() {
            let key = if let TokenTree::Ident(key) = group.stream().into_iter().nth(0).unwrap() {
                key.to_string()
            } else {
                panic!("invalid attr");
            };
            let value =
                if let TokenTree::Literal(value) = group.stream().into_iter().nth(2).unwrap() {
                    let value = value.to_string();
                    value[1..(value.len() - 1)].to_string()
                } else {
                    panic!("invalid attr");
                };
            match key.as_str() {
                "name" => return Some(Self::Name(value)),
                "repr" => return Some(Self::Repr(value)),
                attr => panic!("Unknown attr {}", attr),
            }
        } else {
            panic!("invalid attr");
        }
    }
}

fn field_key(i: usize, field: &Field) -> String {
    for attr in &field.attrs {
        if let Some(Attr::Name(name)) = Attr::from_attr(attr) {
            return name;
        }
    }
    field
        .ident
        .as_ref()
        .map(|ident| ident.to_string())
        .unwrap_or_else(|| i.to_string())
}

pub enum VariantRepr {
    Keyed,
    Kinded,
}

pub enum BindingRepr {
    Map,
    List,
}

impl BindingRepr {
    pub fn from_variant(variant: &VariantInfo) -> Self {
        match variant.ast().fields {
            Fields::Named(_) => Self::Map,
            Fields::Unnamed(_) => Self::List,
            Fields::Unit => Self::List,
        }
    }

    pub fn repr(&self, bindings: &[BindingInfo]) -> TokenStream {
        match self {
            Self::Map => {
                let fields = bindings.iter().enumerate().map(|(i, binding)| {
                    let key = field_key(i, binding.ast());
                    quote!(map.insert(#key.into(), #binding.to_ipld());)
                });
                quote!({
                    let mut map = BTreeMap::new();
                    #(#fields)*
                    IpldRef::OwnedMap(map)
                })
            }
            Self::List => {
                let len = bindings.len();
                let fields = bindings
                    .iter()
                    .map(|binding| quote!(list.push(#binding.to_ipld());));
                quote!({
                    let mut list = Vec::with_capacity(#len);
                    #(#fields)*
                    IpldRef::OwnedList(list)
                })
            }
        }
    }

    pub fn parse(&self, variant: &VariantInfo) -> TokenStream {
        match self {
            Self::Map => {
                let construct = variant.construct(|field, i| {
                    let key = field_key(i, field);
                    quote!({
                        if let Some(ipld) = map.remove(#key) {
                            FromIpld::from_ipld(ipld)?
                        } else {
                            return Err(IpldError::KeyNotFound);
                        }
                    })
                });
                quote! {
                    if let Ipld::Map(ref mut map) = ipld {
                        Ok(#construct)
                    } else {
                        Err(IpldError::NotMap)
                    }
                }
            }
            Self::List => {
                let len = variant.bindings().len();
                let construct = variant.construct(|_field, i| {
                    quote! {
                        FromIpld::from_ipld(list[#i].clone())?
                    }
                });
                quote! {
                    if let Ipld::List(list) = ipld {
                        if list.len() != #len {
                            return Err(IpldError::IndexNotFound);
                        }
                        Ok(#construct)
                    } else {
                        Err(IpldError::NotList)
                    }
                }
            }
        }
    }
}

impl VariantRepr {
    pub fn from_structure(s: &Structure) -> Self {
        match &s.ast().data {
            Data::Struct(_) => Self::Kinded,
            Data::Enum(_) => Self::Keyed,
            Data::Union(_) => panic!("unsupported"),
        }
    }

    pub fn repr(&self, variant: &VariantInfo) -> TokenStream {
        let binding = BindingRepr::from_variant(variant);
        let bindings = binding.repr(variant.bindings());
        match self {
            Self::Keyed => {
                let name = variant.ast().ident.to_string();
                quote! {
                    let mut map = BTreeMap::new();
                    map.insert(#name.into(), #bindings);
                    IpldRef::OwnedMap(map)
                }
            }
            Self::Kinded => quote!(#bindings),
        }
    }

    pub fn parse(&self, variant: &VariantInfo) -> TokenStream {
        let binding = BindingRepr::from_variant(variant);
        let bindings = binding.parse(variant);
        match self {
            Self::Keyed => {
                let name = variant.ast().ident.to_string();
                quote! {
                    if let Some(ipld) = map.get_mut(#name) {
                        return {#bindings};
                    }
                }
            }
            Self::Kinded => bindings,
        }
    }
}

pub fn to_ipld(s: &Structure) -> TokenStream {
    let var_repr = VariantRepr::from_structure(s);
    let body = s.each_variant(|var| var_repr.repr(var));

    quote! {
        fn to_ipld<'a>(&'a self) -> IpldRef<'a> {
            match *self {
                #body
            }
        }
    }
}

pub fn from_ipld(s: &Structure) -> TokenStream {
    let var_repr = VariantRepr::from_structure(s);
    let variants: Vec<TokenStream> = s.variants().iter().map(|var| var_repr.parse(var)).collect();
    let body = match var_repr {
        VariantRepr::Keyed => {
            quote! {
                let map = if let Ipld::Map(ref mut map) = ipld {
                    map
                } else {
                    return Err(IpldError::NotMap);
                };
                #(#variants)*
                Err(IpldError::KeyNotFound)
            }
        }
        VariantRepr::Kinded => quote!(#(#variants)*),
    };

    quote! {
       fn from_ipld(mut ipld: libipld::Ipld) -> Result<Self, IpldError> {
           #body
       }
    }
}