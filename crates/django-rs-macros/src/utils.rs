//! Utility attribute macros.
//!
//! - `#[management_command]` — wraps a function as a management command
//! - `#[middleware]` — wraps a struct as middleware
//! - `#[signal_handler]` — registers a signal handler

use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, ItemImpl};

/// Attributes for `#[management_command(...)]`.
#[derive(Debug, FromMeta)]
pub struct ManagementCommandOpts {
    /// The name of the management command.
    #[darling(default)]
    pub name: Option<String>,

    /// Help text for the command.
    #[darling(default)]
    pub help: Option<String>,
}

/// Generates wrapper code for a management command.
pub fn expand_management_command(opts: ManagementCommandOpts, func: ItemFn) -> TokenStream {
    let fn_name = &func.sig.ident;
    let cmd_name = opts.name.unwrap_or_else(|| fn_name.to_string());
    let help_text = opts
        .help
        .unwrap_or_else(|| format!("Run the {cmd_name} management command"));

    let vis = &func.vis;
    let attrs = &func.attrs;
    let block = &func.block;
    let sig = &func.sig;

    let struct_name = syn::Ident::new(
        &format!(
            "{}Command",
            cmd_name
                .split('_')
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<String>()
        ),
        fn_name.span(),
    );

    quote! {
        /// Generated management command wrapper.
        #vis struct #struct_name;

        impl #struct_name {
            /// The command name.
            pub fn name() -> &'static str {
                #cmd_name
            }

            /// The help text.
            pub fn help() -> &'static str {
                #help_text
            }

            /// Execute the command.
            #(#attrs)*
            #vis #sig
            #block
        }
    }
}

/// Generates wrapper code for middleware.
///
/// This simply re-emits the impl block but adds helper methods
/// for middleware registration.
pub fn expand_middleware(impl_block: ItemImpl) -> TokenStream {
    let self_ty = &impl_block.self_ty;
    let items = &impl_block.items;
    let attrs = &impl_block.attrs;

    quote! {
        #(#attrs)*
        impl #self_ty {
            #(#items)*
        }

        impl #self_ty {
            /// Returns the middleware name for registration.
            pub fn middleware_name() -> &'static str {
                stringify!(#self_ty)
            }
        }
    }
}

/// Attributes for `#[signal_handler(...)]`.
#[derive(Debug, FromMeta)]
pub struct SignalHandlerOpts {
    /// The signal to listen to (e.g., "pre_save", "post_save").
    pub signal: String,
}

/// Generates wrapper code for a signal handler.
pub fn expand_signal_handler(opts: SignalHandlerOpts, func: ItemFn) -> TokenStream {
    let fn_name = &func.sig.ident;
    let signal = &opts.signal;
    let vis = &func.vis;
    let attrs = &func.attrs;
    let sig = &func.sig;
    let block = &func.block;

    let register_fn_name = syn::Ident::new(&format!("register_{fn_name}"), fn_name.span());

    quote! {
        #(#attrs)*
        #vis #sig
        #block

        /// Auto-generated registration function for this signal handler.
        #vis fn #register_fn_name() -> (&'static str, &'static str) {
            (#signal, stringify!(#fn_name))
        }
    }
}
