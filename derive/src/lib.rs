extern crate proc_macro;

use proc_macro2::TokenStream;
use proc_macro_error::{abort_call_site, proc_macro_error};
use quote::{format_ident, quote};
use std::collections::HashMap;

#[derive(Clone, Debug)]
enum Value {
    Table(Table),
    Value(TokenStream),
}

#[derive(Clone, Debug, Default)]
struct Table {
    entries: HashMap<String, Value>,
}

impl Table {
    fn new() -> Self {
        Self::default()
    }

    fn add_table(&mut self, name: String, table: Table) {
        self.entries.insert(name, Value::Table(table));
    }

    fn add_static(&mut self, item: &syn::ItemStatic) {
        let ident = &item.ident;
        let name = ident.to_string();
        self.entries.insert(name, Value::Value(quote!(#ident)));
    }

    fn add_fn(&mut self, item: &syn::ItemFn) {
        let ident = &item.sig.ident;
        let name = ident.to_string();
        let n_args = item.sig.inputs.len();
        self.insert(name, n_args, quote!(#ident));
    }

    fn insert(&mut self, name: String, n_args: usize, tokens: TokenStream) {
        let wrapper = format_ident!("function{}", n_args);
        self.entries
            .insert(name, Value::Value(quote!(hlua::#wrapper(#tokens))));
    }

    fn into_tokens(self, ident: &syn::Ident) -> TokenStream {
        self.entries
            .into_iter()
            .map(|(k, v)| match v {
                Value::Table(table) => {
                    if table.entries.is_empty() {
                        quote!()
                    } else {
                        let child = format_ident!("{}", k);
                        let tokens = table.into_tokens(&child);
                        quote! {
                            {
                                let mut #child = #ident.empty_array(#k);
                                #tokens
                            }
                        }
                    }
                }
                Value::Value(tokens) => quote!(#ident.set(#k, #tokens);),
            })
            .collect()
    }

    fn table_mut(&mut self, name: String) -> &mut Table {
        let value = self
            .entries
            .entry(name)
            .or_insert_with(|| Value::Table(Table::new()));
        if let Value::Table(ref mut table) = value {
            table
        } else {
            panic!("internal error")
        }
    }
}

struct Object {
    self_ty: Box<syn::Type>,
    constructors: Table,
    metatable: Table,
}

impl Object {
    fn new(self_ty: Box<syn::Type>) -> Self {
        Self {
            self_ty,
            constructors: Table::new(),
            metatable: Table::new(),
        }
    }

    fn add_method(&mut self, item: &syn::ImplItemMethod, meta: Option<String>) {
        let ty = &self.self_ty;
        let ident = &item.sig.ident;
        let name = ident.to_string();
        let n_args = item.sig.inputs.len();
        match &item.sig.inputs[0] {
            syn::FnArg::Receiver(_) => {
                let args_decl = item.sig.inputs.iter().skip(1).collect::<Vec<_>>();
                let args = args_decl.iter().map(|arg| {
                    if let syn::FnArg::Typed(pat) = arg {
                        &pat.pat
                    } else {
                        unreachable!()
                    }
                });
                let f = quote!(|o: &mut #ty, #(#args_decl),*| o.#ident(#(#args),*));
                if let Some(meta) = meta {
                    self.metatable.insert(meta, n_args, f)
                } else {
                    let index = self.metatable.table_mut("__index".to_string());
                    index.insert(name, n_args, f);
                }
            }
            _ => {
                let f = quote!(#ty::#ident);
                self.constructors.insert(name, n_args, f);
            }
        };
    }
}

#[derive(Default)]
struct Module {
    ident: String,
    objects: Vec<Object>,
    table: Table,
}

impl Module {
    fn new(ident: String) -> Self {
        Self {
            ident,
            objects: Vec::new(),
            table: Table::new(),
        }
    }

    fn add_static(&mut self, item: &syn::ItemStatic) {
        self.table.add_static(item);
    }

    fn add_fn(&mut self, item: &syn::ItemFn) {
        self.table.add_fn(item);
    }

    fn add_object(&mut self, obj: Object) {
        self.objects.push(obj);
    }

    fn into_tokens(mut self) -> TokenStream {
        let objects: TokenStream = self
            .objects
            .iter()
            .map(|obj| {
                let ident = &obj.self_ty;
                let tokens = obj
                    .metatable
                    .clone()
                    .into_tokens(&format_ident!("metatable"));
                quote! {
                    hlua::implement_lua_push!(#ident, |mut metatable| {
                        #tokens
                    });
                    hlua::implement_lua_read!(#ident);
                }
            })
            .collect();
        for obj in self.objects {
            for (name, cons) in obj.constructors.entries {
                self.table.entries.insert(name, cons);
            }
        }
        let mut module = Table::new();
        module.add_table(self.ident, self.table);
        let module = module.into_tokens(&format_ident!("lua"));
        quote! {
            #objects

            pub fn load<'a, L: hlua::AsMutLua<'a>>(mut lua: hlua::LuaTable<L>) {
                #module
            }
        }
    }
}

#[proc_macro_attribute]
#[proc_macro_error]
pub fn lua(
    _args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let mut item_mod: syn::ItemMod = syn::parse(input).unwrap_or_else(|_| {
        abort_call_site!("`lua_module` attribute is only allowed on modules");
    });

    if let syn::Visibility::Public(_) = &item_mod.vis {
    } else {
        abort_call_site!("`lua_module` attribute is only allowed on public modules");
    }

    let mut module = Module::new(item_mod.ident.to_string());
    let mut mod_items = Vec::new();

    if let Some((_, items)) = item_mod.content.as_mut() {
        for item in items {
            match item {
                syn::Item::Fn(f) => module.add_fn(f),
                syn::Item::Static(s) => module.add_static(s),
                syn::Item::Impl(i) => {
                    let mut obj = Object::new(i.self_ty.clone());
                    for item in &mut i.items {
                        if let syn::ImplItem::Method(item) = item {
                            let mut new_attrs = Vec::with_capacity(item.attrs.len());
                            let mut metatable = None;
                            for attr in &item.attrs {
                                if attr.path.is_ident("lua") {
                                    if let Ok(syn::Meta::List(meta)) = attr.parse_meta() {
                                        if let Some(syn::NestedMeta::Meta(syn::Meta::NameValue(
                                            nv,
                                        ))) = meta.nested.first()
                                        {
                                            if nv.path.is_ident("meta") {
                                                if let syn::Lit::Str(value) = &nv.lit {
                                                    metatable = Some(value.value());
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                    abort_call_site!("unrecognized attribute");
                                } else {
                                    new_attrs.push(attr.clone());
                                }
                            }
                            item.attrs = new_attrs;
                            obj.add_method(&item, metatable);
                        }
                    }
                    module.add_object(obj);
                }
                _ => {}
            };
            mod_items.push(item.clone());
        }
    }

    let ident = &item_mod.ident;
    let lua_module = module.into_tokens();

    (quote! {
        pub mod #ident {
            #(#mod_items)*

            #lua_module
        }
    })
    .into()
}
