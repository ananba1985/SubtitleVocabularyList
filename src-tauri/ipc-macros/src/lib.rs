//! Keep Tauri invocation and the optional TypeScript export on the same signatures.
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, FnArg, GenericArgument, ItemFn, Path, PathArguments, ReturnType, Type, parse_macro_input,
    parse::{Parse, ParseStream},
};

struct CatalogCommand {
    attrs: Vec<Attribute>,
    path: Path,
}
impl Parse for CatalogCommand {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self { attrs: input.call(Attribute::parse_outer)?, path: input.parse()? })
    }
}

fn last_name(ty: &Type) -> String {
    match ty {
        Type::Path(p) => p.path.segments.last().unwrap().ident.to_string(),
        _ => String::new(),
    }
}

fn untyped_response(ty: &Type) -> bool {
    if last_name(ty) == "Value" {
        return true;
    }
    match ty {
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                args.args
                    .iter()
                    .any(|arg| matches!(arg, GenericArgument::Type(ty) if untyped_response(ty)))
            } else {
                false
            }
        }),
        _ => false,
    }
}

#[proc_macro_attribute]
pub fn command(_: TokenStream, input: TokenStream) -> TokenStream {
    let function = parse_macro_input!(input as ItemFn);
    let name = &function.sig.ident;
    let export = format_ident!("__ipc_{}", name);
    let mut args = Vec::new();
    let mut raw_body = false;
    for arg in &function.sig.inputs {
        let FnArg::Typed(arg) = arg else { continue };
        let ty = &arg.ty;
        let kind = last_name(ty);
        if matches!(kind.as_str(), "AppState" | "State" | "AppHandle") {
            continue;
        }
        if kind == "Request" {
            raw_body = true;
            continue;
        }
        let syn::Pat::Ident(parameter) = &*arg.pat else {
            panic!("IPC parameters must be named")
        };
        let snake = parameter.ident.to_string();
        let mut upper = false;
        let field: String = snake
            .chars()
            .filter_map(|c| {
                if c == '_' {
                    upper = true;
                    None
                } else if upper {
                    upper = false;
                    Some(c.to_ascii_uppercase())
                } else {
                    Some(c)
                }
            })
            .collect();
        let optional = if kind == "Option" { "?" } else { "" };
        args.push(quote! { format!("{}{}: {}", #field, #optional, registry.include::<#ty>()) });
    }
    let mut output = match &function.sig.output {
        ReturnType::Type(_, ty) => (**ty).clone(),
        _ => syn::parse_quote!(()),
    };
    if last_name(&output) == "Result" {
        if let Type::Path(path) = &output {
            if let PathArguments::AngleBracketed(arguments) =
                &path.path.segments.last().unwrap().arguments
            {
                if let GenericArgument::Type(ty) = arguments.args.first().unwrap() {
                    output = ty.clone();
                }
            }
        }
    }
    if untyped_response(&output) {
        return syn::Error::new_spanned(
            &output,
            "Define an explicit DTO for an IPC response instead of Value",
        )
        .to_compile_error()
        .into();
    }
    let unit = matches!(&output, Type::Tuple(tuple) if tuple.elems.is_empty());
    let result = if unit {
        quote! { "void".to_owned() }
    } else {
        quote! { registry.include::<#output>() }
    };
    let arguments = if raw_body {
        quote! { "Uint8Array".to_owned() }
    } else if args.is_empty() {
        quote! { "undefined".to_owned() }
    } else {
        quote! { format!("{{ {} }}", vec![#(#args),*].join("; ")) }
    };
    quote! {
        #[tauri::command]
        #function
        #[cfg(feature = "ipc-types")]
        pub(crate) fn #export(registry: &mut crate::ipc_export::Registry) {
            let args = #arguments;
            let result = #result;
            registry.command(stringify!(#name), args, result);
        }
    }
    .into()
}

#[proc_macro]
pub fn catalog(input: TokenStream) -> TokenStream {
    let paths = parse_macro_input!(input with syn::punctuated::Punctuated::<CatalogCommand, syn::Token![,]>::parse_terminated);
    let commands: Vec<_> = paths.iter().map(|entry| {
        let attrs = &entry.attrs;
        let path = &entry.path;
        quote! { #(#attrs)* #path }
    }).collect();
    let exports: Vec<_> = paths
        .iter()
        .map(|entry| {
            let attrs = &entry.attrs;
            let mut path = entry.path.clone();
            let last = path.segments.last_mut().unwrap();
            last.ident = format_ident!("__ipc_{}", last.ident);
            quote! { #(#attrs)* #path(&mut registry); }
        })
        .collect();
    quote! {
        pub(crate) fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![#(#commands),*]
        }
        #[cfg(feature = "ipc-types")]
        pub(crate) fn export_types() -> String {
            let mut registry = crate::ipc_export::Registry::new();
            #(#exports)*
            registry.include::<crate::error::AppError>();
            registry.include::<crate::tasks::TaskUpdate<'static>>();
            registry.include::<crate::preparation::PreparationProgress>();
            registry.include::<crate::synchronization::SyncResult>();
            registry.finish()
        }
    }.into()
}
