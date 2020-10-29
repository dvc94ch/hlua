extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro_error::{abort_call_site, emit_warning, proc_macro_error};
use quote::{format_ident, quote};

#[proc_macro_attribute]
#[proc_macro_error]
pub fn lua_module(_args: TokenStream, input: TokenStream) -> TokenStream {
    let module: syn::ItemMod = syn::parse(input).unwrap_or_else(|_| {
        abort_call_site!("`export_lua_module` attribute is only allowed on modules");
    });

    if let syn::Visibility::Public(_) = &module.vis {
    } else {
        abort_call_site!("`export_lua_module` attribute is only allowed on public modules");
    }

    let mut table = Vec::new();
    let mut mod_items = Vec::new();
    let mut init = Vec::new();

    if let Some((_, items)) = module.content.as_ref() {
        for item in items {
            match item {
                syn::Item::Fn(f) => {
                    let fn_ident = &f.sig.ident;
                    let fn_name = fn_ident.to_string();
                    let n_args = f.sig.inputs.len();
                    let wrapper = format_ident!("function{}", n_args);
                    table.push(quote! {
                        table.set(#fn_name, hlua::#wrapper(#fn_ident));
                    });

                    let mut new_f = f.clone();
                    new_f.attrs = f
                        .attrs
                        .iter()
                        .filter(|attr| {
                            if let Some(ident) = attr.path.get_ident() {
                                if ident.to_string() == "init" {
                                    return false;
                                }
                            }
                            true
                        })
                        .cloned()
                        .collect();
                    mod_items.push(quote!(#new_f));

                    if new_f.attrs.len() != f.attrs.len() {
                        init.push(quote!(#fn_ident();));
                    }
                }
                syn::Item::Static(s) => {
                    let ident = &s.ident;
                    let name = ident.to_string();
                    table.push(quote! {
                        table.set(#name, #ident);
                    });
                    mod_items.push(quote!(#s));
                }
                _ => {
                    emit_warning!(
                        "item is neiter a function nor a static and will ",
                        "thus be ignored by `export_lua_module`",
                    );
                }
            }
        }
    }

    let ident = &module.ident;
    let name = ident.to_string();

    (quote! {
        pub mod #ident {
            #(#mod_items)*

            pub fn luaopen(lua: &mut hlua::Lua) {
                let mut table = lua.empty_array(#name);
                #(#table)*
                #(#init)*
            }
        }
    })
    .into()
}
