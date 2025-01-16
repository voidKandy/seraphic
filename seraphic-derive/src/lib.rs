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
            let mut from_impls = quote! {};
            let mut into_req_body = quote! {};
            let mut from_req_body = quote! {
                let e:Box<dyn std::error::Error + Send + Sync + 'static> = std::io::Error::other("Could not get Request object").into();
                let mut ret = Err(e);
            };
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
                    if ret.is_err() {
                        match #enum_typ::try_from_request(&req) {
                            Ok(v) => return Ok(Self::#id(v)),
                            Err(e) => ret = Err(e),
                        }
                    }
                };

                from_impls = quote! {
                    #from_impls
                    impl From<#enum_typ> for #ident {
                        fn from(v: #enum_typ) -> Self {
                            Self::#id(v)
                        }
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
                    return ret;
                }
            };

            let output = quote! {
                #from_impls
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
            let mut from_impls = quote! {};
            let mut into_res_body = quote! {};
            let mut from_res_body = quote! {
                let e:Box<dyn std::error::Error + Send + Sync + 'static> = std::io::Error::other("Could not get Response object").into();
                let mut ret = Err(e);
            };
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
                    if ret.is_err() {
                        ret = #enum_typ::try_from_response(&res).map(|maybe_ok|  maybe_ok.map(|ok| Self::#id(ok)));
                    }
                };

                from_impls = quote! {
                    #from_impls
                    impl From<#enum_typ> for #ident {
                        fn from(v: #enum_typ) -> Self {
                            Self::#id(v)
                        }
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
                    return ret;
                }
            };

            let output = quote! {
                #from_impls
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
