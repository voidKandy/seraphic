use core::panic;
use std::str::FromStr;

use darling::FromDeriveInput;
use proc_macro::{self, TokenStream};
use proc_macro2::Span;
use quote::{format_ident, quote, ToTokens};
use syn::{
    braced, bracketed, parse::Parse, parse_macro_input, punctuated::Punctuated, Data, DataEnum,
    DataStruct, DeriveInput, Ident, Path, Token, TypePath,
};

struct WrapperInput {
    trait_name: WrapperTrait,
    enum_name: Ident,
    types: Punctuated<Path, Token![,]>,
}

impl TryFrom<Path> for WrapperTrait {
    type Error = syn::Error;
    fn try_from(value: Path) -> Result<Self, Self::Error> {
        let response_id = format_ident!("ResponseWrapper");
        let request_id = format_ident!("RequestWrapper");
        let last_segment_id = &value.segments.last().unwrap().ident;

        if *last_segment_id == response_id {
            Ok(Self::Response(value))
        } else if *last_segment_id == request_id {
            Ok(Self::Request(value))
        } else {
            Err(syn::Error::new(
                Span::call_site(),
                format!("{last_segment_id:#?} is not a valid trait"),
            ))
        }
    }
}

enum WrapperTrait {
    Request(Path),
    Response(Path),
}

impl syn::parse::Parse for WrapperInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let trait_name = WrapperTrait::try_from(Path::parse(input)?)?;
        let _comma: Token![,] = input.parse()?;
        let enum_name: Ident = input.parse()?;
        let _comma: Token![,] = input.parse()?;
        let content;
        bracketed!(content in input);
        let types = content.parse_terminated(Path::parse, Token![,])?;
        Ok(WrapperInput {
            trait_name,
            enum_name,
            types,
        })
    }
}

#[proc_macro]
pub fn wrapper(input: TokenStream) -> TokenStream {
    let WrapperInput {
        enum_name,
        types,
        trait_name,
    } = parse_macro_input!(input as WrapperInput);

    let mut variants_body = quote! {};
    let mut from_impls = quote! {};
    let mut trait_into_body = quote! {};
    let trait_into_fn = match trait_name {
        WrapperTrait::Request(_) => {
            quote! {
                fn into_req(&self, id: impl ToString) -> seraphic::Request
            }
        }
        WrapperTrait::Response(_) => {
            quote! {
                fn into_res(&self, id: impl ToString) -> seraphic::Response
            }
        }
    };

    let mut trait_from_body = quote! {};
    let trait_from_fn = match trait_name {
        WrapperTrait::Request(_) => {
            quote! {
                fn try_from_req(req: seraphic::Request) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>>
            }
        }
        WrapperTrait::Response(_) => {
            quote! {
                fn try_from_res(res: seraphic::Response) -> Result<Result<serde_::Response, seraphic::error::Error>, Box<(dyn StdError + Send + Sync + 'static)>>
            }
        }
    };

    types.iter().for_each(|ty| {
        // let mut into_body = quote! {};
        // let mut from_body = quote! {};
        let last_segment_id = &ty.segments.last().unwrap().ident;
        let variant_ident = format_ident!(
            "{}{}",
            last_segment_id.to_string().chars().next().unwrap(),
            last_segment_id
                .to_string()
                .chars()
                .skip(1)
                .take_while(|c| c.is_lowercase())
                .collect::<String>()
        );

        let to_be_added = proc_macro2::TokenStream::from_str(&format!(
            "{}({})",
            variant_ident.to_string(),
            ty.to_token_stream().to_string()
        ))
        .unwrap();

        variants_body = quote! {
            #variants_body
            #to_be_added
        };

        let from_impl = quote! {
            impl From<#ty> for #enum_name {
                fn from(v: #ty) -> Self {
                    Self::#variant_ident(v)
                }
            }
        };

        from_impls = quote! {
            #from_impls
            #from_impl
        };
    });

    let trait_path = match trait_name {
        WrapperTrait::Response(p) => p,
        WrapperTrait::Request(p) => p,
    };

    // Generate the enum definition
    let expanded = quote! {
        #[derive(Debug, Clone, #trait_path, PartialEq)]
        pub enum #enum_name {
            #variants_body
        }

        #from_impls


    };

    expanded.into()
}

// https://github.com/imbolc/rust-derive-macro-guide
#[derive(FromDeriveInput, Default)]
#[darling(default, attributes(rpc_request))]
struct Opts {
    // formatted "type:variant"
    namespace: String,
    response: Option<String>,
}

#[proc_macro_derive(RpcRequest, attributes(rpc_request))]
pub fn derive_rpc_req(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input);
    let opts = Opts::from_derive_input(&input).expect("Wrong options");
    let DeriveInput { ident, data, .. } = input;
    match data {
        syn::Data::Struct(DataStruct { fields, .. }) => {
            let name = format!("{ident}");
            let name_no_suffix = name
                .strip_suffix("Request")
                .expect("make sure to put 'Request' at the end of your struct name");
            // let struct_name = format_ident!("{}", name_no_suffix);
            let first_char = name_no_suffix
                .chars()
                .next()
                .unwrap()
                .to_owned()
                .to_lowercase();
            let method = format!("{first_char}{}", &name_no_suffix[1..]);

            let mut from_json_body = quote! {};
            let mut create_self_body = quote! {};

            for f in fields {
                let id = f.ident.unwrap();
                let json_name = format_ident!("{}_json", id);
                let id_string = format!("{id}");
                let not_exist = format!("field '{id_string}' does not exist");
                let not_deserialize = format!("field '{id_string}' does not implement deserialize");
                from_json_body = quote! {
                    #from_json_body
                    let #json_name = json.get(#id_string).ok_or(#not_exist)?.to_owned();
                    let #id = serde_json::from_value(#json_name).map_err(|_|#not_deserialize)?;
                };

                create_self_body = quote! {
                    #create_self_body
                    #id,
                }
            }

            let create_self = quote! {
                Ok(Self {
                    #create_self_body
                })
            };

            let from_json = quote! {
              fn try_from_json(json: &serde_json::Value) -> std::result::Result<Self,Box<dyn std::error::Error + Send + Sync + 'static>> {
                    #from_json_body
                    #create_self
              }
            };

            let method_name = quote! {
                fn method()-> &'static str {
                    #method
                }
            };

            let ns = opts.namespace;
            let (ns_type, ns_var) = ns
                .split_once(':')
                .expect("expected namespace attribute to have a ':'");

            let ns_type_id = format_ident!("{ns_type}");
            let namespace = quote! {
                fn namespace() -> Self::Namespace {
                     Self::Namespace::try_from_str(#ns_var).unwrap()

                }
            };

            let (response_struct_name, should_impl) = match opts.response {
                //if a response struct is passed in opt, it is assumed it alrady implements needed
                //trait
                Some(res) => (format_ident!("{}", res), false),
                None => (format_ident!("{}Response", name_no_suffix), true),
            };

            let mut output = quote! {};
            if should_impl {
                output = quote! {
                    impl RpcResponse for #response_struct_name {}
                }
            }
            output = quote! {
                #output
                impl RpcRequest for #ident {
                    type Response = #response_struct_name;
                    type Namespace = #ns_type_id;
                    #from_json
                    #method_name
                    #namespace
                }
            };

            output.into()
        }
        _ => {
            panic!("cannot derive this on anything but a struct")
        }
    }
}

#[proc_macro_derive(RequestWrapper)]
pub fn derive_req_wrapper(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input);
    let DeriveInput { ident, data, .. } = input;
    match data {
        Data::Enum(DataEnum { variants, .. }) => {
            let mut into_req_body = quote! {};
            let mut from_req_body = quote! {};
            for v in variants {
                let id = v.ident;
                let enum_typ = match v.fields {
                    syn::Fields::Unnamed(t) => match t.unnamed.iter().next().cloned().unwrap().ty {
                        syn::Type::Path(TypePath { path, .. }) => {
                            path.segments.iter().next().unwrap().ident.clone()
                        }
                        other => panic!("Expected type path as unnamed variant, got: {other:#?}"),
                    },
                    _ => panic!("only unnamed struct variants supported"),
                };
                let not_request = format!("variant {id} does not implement RpcRequest");

                into_req_body = quote! {
                    #into_req_body
                    Self::#id(r) => r.into_request(id).expect(#not_request),
                };

                from_req_body = quote! {
                    #from_req_body
                    if let Some(r) = #enum_typ::try_from_request(&req)? {
                        return Ok(Self::#id(r));
                    }
                };
            }

            let into_req = quote! {
                fn into_req(&self, id: impl ToString) -> seraphic::Request {
                    match self {
                        #into_req_body
                    }
                }
            };

            let from_req = quote! {
                fn try_from_req(req: seraphic::Request) -> std::result::Result<Self,Box<dyn std::error::Error + Send + Sync + 'static>> {
                    #from_req_body
                    Err("Could not get request".into())
                }
            };

            let output = quote! {
                impl seraphic::RequestWrapper for #ident {
                    #into_req
                    #from_req

                }
            };
            output.into()
        }
        _ => {
            panic!("cannot derive this on anything but an enum")
        }
    }
}

#[proc_macro_derive(ResponseWrapper)]
pub fn derive_res_wrapper(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input);
    let DeriveInput { ident, data, .. } = input;
    match data {
        Data::Enum(DataEnum { variants, .. }) => {
            let mut into_res_body = quote! {};
            let mut from_res_body = quote! {};
            for v in variants {
                let id = v.ident;
                let enum_typ = match v.fields {
                    syn::Fields::Unnamed(t) => match t.unnamed.iter().next().cloned().unwrap().ty {
                        syn::Type::Path(TypePath { path, .. }) => {
                            path.segments.iter().next().unwrap().ident.clone()
                        }
                        other => panic!("Expected type path as unnamed variant, got: {other:#?}"),
                    },
                    _ => panic!("only unnamed struct variants supported"),
                };
                let not_res = format!("variant {id} does not implement RpcResponse");

                into_res_body = quote! {
                    #into_res_body
                    Self::#id(r) => r.into_response(id).expect(#not_res),
                };

                from_res_body = quote! {
                    #from_res_body
                    match #enum_typ::try_from_response(&res)? {
                        Ok(r) => {Ok(Ok(Self::#id(r)))}
                        Err(err) => {Ok(Err(err))}

                    }
                };
            }

            let into_res = quote! {
                fn into_res(&self, id: impl ToString) -> seraphic::Response {
                    match self {
                        #into_res_body
                    }
                }
            };

            let from_res = quote! {
                fn try_from_res(res: seraphic::Response) -> std::result::Result<std::result::Result<Self, seraphic::error::Error>, Box<dyn std::error::Error + Send + Sync + 'static>> {
                    #from_res_body
                }
            };

            let output = quote! {
                impl ResponseWrapper for #ident {
                    #into_res
                    #from_res

                }
            };
            output.into()
        }
        _ => {
            panic!("cannot derive this on anything but an enum")
        }
    }
}

#[derive(FromDeriveInput, Default)]
#[darling(default, attributes(namespace))]
struct NamespaceOpts {
    separator: Option<String>,
}

#[proc_macro_derive(RpcNamespace, attributes(namespace))]
pub fn derive_namespace(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input);
    let opts = NamespaceOpts::from_derive_input(&input).expect("Wrong options");
    let separator = opts.separator.unwrap_or("_".to_string());
    let separator = quote! {const SEPARATOR: &str = #separator;};

    let DeriveInput { ident, data, .. } = input;
    match data {
        Data::Enum(DataEnum { variants, .. }) => {
            let mut from_str_body = quote! {};
            let mut as_ref_body = quote! {};
            let mut my_str_consts = quote! {};
            for v in variants {
                let id = v.ident;
                let id_str = format!("{id}");
                let const_id = format_ident!("{}", id_str.to_uppercase());
                let const_val = id_str.to_lowercase();
                my_str_consts = quote! {
                    #my_str_consts
                    const #const_id: &str = #const_val;
                };
                from_str_body = quote! {
                    #from_str_body
                    Self::#const_id => Some(Self::#id),
                };
                as_ref_body = quote! {
                    #as_ref_body
                    Self::#id => Self::#const_id,
                };
            }

            let as_str = quote! {
                fn as_str(&self)-> &str {
                    match self {
                        #as_ref_body
                    }
                }
            };

            let try_from = quote! {
                fn try_from_str(str: &str) -> Option<Self> {
                    match str {
                        #from_str_body
                        o => None,
                    }
                }
            };

            let output = quote! {
                impl #ident {
                    #my_str_consts
                }
                impl RpcNamespace for #ident {
                 #separator
                    #as_str
                    #try_from
                }
            };

            output.into()
        }
        _ => {
            panic!("cannot derive this on anything but an enum")
        }
    }
}
