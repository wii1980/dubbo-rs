use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ImplItem, ItemImpl, ItemTrait, TraitItem};

/// Attribute macro for defining a Dubbo service implementation.
///
/// Parses the impl block and auto-generates service metadata
/// such as method name introspection.
/// # Panics
///
/// Panics if the proc-macro is used on a non-struct type (e.g., primitive, bare trait).
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);

    let struct_name = match &*input.self_ty {
        syn::Type::Path(type_path) => &type_path.path.segments.last().unwrap().ident,
        _ => panic!("#[dubbo_rs_service] must be used on an impl block for a struct"),
    };

    let methods: Vec<_> = input
        .items
        .iter()
        .filter_map(|item| {
            if let ImplItem::Fn(method) = item {
                Some(&method.sig.ident)
            } else {
                None
            }
        })
        .collect();

    let expanded = quote! {
        #input

        impl #struct_name {
            fn __service_methods() -> Vec<&'static str> {
                vec![#(stringify!(#methods)),*]
            }
        }
    };

    TokenStream::from(expanded)
}

/// Attribute macro for generating a Dubbo client proxy from a service trait.
///
/// Given a trait with async methods, generates a client struct that implements
/// the trait by calling `Invoker::invoke()` for each method. Arguments are
/// serialized via `serde_json` and return values are deserialized.
///
/// # Example
///
/// ```ignore
/// #[dubbo_rs_client]
/// pub trait Greeter {
///     async fn say_hello(&self, name: String) -> Result<String, anyhow::Error>;
/// }
/// ```
///
/// Generates `GreeterClient` with `new(invoker)` constructor and `impl Greeter`.
#[proc_macro_attribute]
pub fn client(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemTrait);

    let trait_name = &input.ident;
    let client_name = format_ident!("{trait_name}Client");
    let vis = &input.vis;

    let methods: Vec<_> = input
        .items
        .iter()
        .filter_map(|item| {
            if let TraitItem::Fn(method) = item {
                Some(method)
            } else {
                None
            }
        })
        .collect();

    let client_methods: Vec<_> = methods
        .iter()
        .map(|method| {
            let method_name = &method.sig.ident;
            let method_name_str = method_name.to_string();
            let args: Vec<_> = method
                .sig
                .inputs
                .iter()
                .filter_map(|arg| {
                    if let syn::FnArg::Typed(pat_type) = arg {
                        if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                            if pat_ident.ident != "self" {
                                return Some((pat_ident.ident.clone(), pat_type.ty.clone()));
                            }
                        }
                    }
                    None
                })
                .collect();

            let arg_names: Vec<_> = args.iter().map(|(name, _)| name).collect();
            let arg_types: Vec<_> = args.iter().map(|(_, ty)| ty).collect();
            let return_ty = match &method.sig.output {
                syn::ReturnType::Type(_, ty) => quote! { #ty },
                syn::ReturnType::Default => quote! { () },
            };

            let serialize_args = if args.is_empty() {
                quote! {
                    let arguments: std::vec::Vec<std::vec::Vec<u8>> = std::vec::Vec::new();
                }
            } else {
                let serializations: Vec<_> = args
                    .iter()
                    .map(|(name, _)| {
                        quote! {
                            serde_json::to_vec(&#name)
                                .map_err(|e| anyhow::anyhow!("failed to serialize argument '{}': {}", stringify!(#name), e))?
                        }
                    })
                    .collect();
                quote! {
                    let arguments: std::vec::Vec<std::vec::Vec<u8>> = std::vec![#(#serializations),*];
                }
            };

            quote! {
                async fn #method_name(&self, #( #arg_names: #arg_types ),*) -> #return_ty {
                    let mut ctx = dubbo_rs_protocol::InvocationContext::new(
                        #method_name_str,
                        self.invoker.get_url().clone(),
                    );
                    #serialize_args
                    ctx.arguments = arguments;
                    let result = self.invoker.invoke(&mut ctx).await?;
                    let value = result.value
                        .ok_or_else(|| anyhow::anyhow!("empty response from {}", #method_name_str))?;
                    Ok(serde_json::from_slice(&value)?)
                }
            }
        })
        .collect();

    let expanded = quote! {
        #input

        #vis struct #client_name {
            invoker: Box<dyn dubbo_rs_protocol::Invoker>,
        }

        impl #client_name {
            pub fn new(invoker: Box<dyn dubbo_rs_protocol::Invoker>) -> Self {
                Self { invoker }
            }
        }

        impl #trait_name for #client_name {
            #(#client_methods)*
        }
    };

    TokenStream::from(expanded)
}
