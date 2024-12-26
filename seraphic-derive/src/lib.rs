use core::panic;

use darling::FromDeriveInput;
use proc_macro::{self, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DataEnum, DataStruct, DeriveInput, TypePath};

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
            let response_struct_name = match opts.response {
                Some(res) => format_ident!("{}", res),
                None => format_ident!("{}Response", name_no_suffix),
            };
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
                    let #json_name =  json.get(#id_string).ok_or(#not_exist)?.to_owned();
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
              fn try_from_json(json: &serde_json::Value) -> MainResult<Self> {
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
                fn namespace() -> Namespace {
                     Self::Namespace::try_from_str(#ns_var).unwrap()

                }
            };

            let output = quote! {
                impl RpcResponse for #response_struct_name {}
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

#[proc_macro_derive(RpcRequestWrapper)]
pub fn derive_req_wrapper(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input);
    let DeriveInput { ident, data, .. } = input;
    match data {
        Data::Enum(DataEnum { variants, .. }) => {
            let mut into_rpc_req_body = quote! {};
            let mut from_rpc_req_body = quote! {};
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
                let not_rpc_request = format!("variant {id} does not implement RpcRequest");
                into_rpc_req_body = quote! {
                    #into_rpc_req_body
                    Self::#id(rq) => rq.into_rpc_request(id).expect(#not_rpc_request),
                };

                from_rpc_req_body = quote! {
                    #from_rpc_req_body
                    if let Some(req) = #enum_typ::try_from_request(&req)? {
                        return Ok(Self::#id(req));
                    }
                };
            }

            let into_rpc_req = quote! {
                fn into_rpc_request(self, id: u32) -> socket::Request {
                    match self {
                        #into_rpc_req_body
                    }
                }
            };

            let from_rpc_req = quote! {
                fn try_from_rpc_req(req: socket::Request) -> MainResult<Self> {
                    #from_rpc_req_body
                    Err("Could not get request".into())
                }
            };

            let output = quote! {
                impl RpcRequestWrapper for #ident {
                    #into_rpc_req
                    #from_rpc_req

                }
            };
            output.into()
        }
        _ => {
            panic!("cannot derive this on anything but an enum")
        }
    }
}

#[proc_macro_derive(RpcNamespace)]
pub fn derive_namespace(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input);
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
