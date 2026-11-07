//! Everything the analysis looks up by name, from the parsed crate: functions and methods (with what
//! their signatures say), modules (their items and `use`s), struct fields' types, and lock declarations.
//! Test code is left out.

use super::attrs;
use super::sig::{callable_generics, is_lock_type, param, tokens, type_name};
use super::modinfo::ModInfo;
use super::{Crate, SourceFile};
use std::collections::{BTreeMap, BTreeSet};
use syn::spanned::Spanned;

pub type FnId = usize;

/// One function or method with a body.
pub struct FnDef<'a> {
    pub name: String,
    pub module: Vec<String>,
    /// The type of the `impl` (or trait) it's in.
    pub self_ty: Option<String>,
    pub file: usize,
    pub line: usize,
    pub is_async: bool,
    pub is_command: bool,
    pub has_receiver: bool,
    /// Its parameters after `self`.
    pub params: Vec<Param>,
    /// Its return type names a lock guard (`Guard`, `MutexGuard`, …): calling it may hand one out.
    pub returns_guard: bool,
    pub body: &'a syn::Block,
}

impl FnDef<'_> {
    /// `hub::Hub::snapshot`, `leads::name_of`.
    pub fn path(&self) -> String {
        let mut segs = self.module.clone();
        segs.extend(self.self_ty.clone());
        segs.push(self.name.clone());
        segs.join("::")
    }

    /// In the `db` module or one of its children: a call to it is a database call.
    pub fn in_db(&self) -> bool {
        self.module.first().is_some_and(|m| m == "db")
    }
}

pub struct Param {
    pub ident: Option<String>,
    /// The type, as tokens.
    pub ty: String,
    /// Its type is callable (`impl Fn…`, `&dyn FnMut…`, a generic bound by `Fn…`).
    pub fn_like: bool,
}

/// A `Mutex`, `RwLock` or once-initialized cell declared as a static or a struct field.
pub struct LockDecl {
    pub file: usize,
    pub line: usize,
    /// `INDEX`, or `Hub.store` for a field.
    pub owner: String,
    /// The identifier a lock call names it by (`INDEX`, `store`, `0`).
    pub ident: String,
    pub ty: String,
}

pub struct Index<'a> {
    pub files: &'a [SourceFile],
    pub fns: Vec<FnDef<'a>>,
    pub modules: BTreeMap<Vec<String>, ModInfo>,
    /// (type, method) → the methods of that name on that type (more than one under different `cfg`s).
    pub methods: BTreeMap<(String, String), Vec<FnId>>,
    /// method name → every type that has one.
    pub method_owners: BTreeMap<String, BTreeSet<String>>,
    /// (struct, field) → the field's type, as tokens.
    pub fields: BTreeMap<(String, String), String>,
    pub locks: Vec<LockDecl>,
}

impl<'a> Index<'a> {
    pub fn build(krate: &'a Crate) -> Index<'a> {
        let mut ix = Index { files: &krate.files, fns: vec![], modules: BTreeMap::new(), methods: BTreeMap::new(), method_owners: BTreeMap::new(), fields: BTreeMap::new(), locks: vec![] };
        for (i, f) in krate.files.iter().enumerate() {
            ix.items(i, &f.module, &f.ast.items);
        }
        ix.fns.sort_by_key(|f| (f.file, f.line));
        for (id, f) in ix.fns.iter().enumerate() {
            match &f.self_ty {
                Some(t) => {
                    ix.methods.entry((t.clone(), f.name.clone())).or_default().push(id);
                    ix.method_owners.entry(f.name.clone()).or_default().insert(t.clone());
                }
                None => {
                    ix.modules.entry(f.module.clone()).or_default().fns.entry(f.name.clone()).or_default().push(id);
                }
            }
        }
        ix
    }

    pub fn rel(&self, id: FnId) -> &str {
        &self.files[self.fns[id].file].rel
    }

    /// `hub/mod.rs:289`.
    pub fn at(&self, id: FnId, line: usize) -> String {
        format!("{}:{line}", self.rel(id))
    }

    fn items(&mut self, file: usize, module: &[String], items: &'a [syn::Item]) {
        let info = self.modules.entry(module.to_vec()).or_default();
        info.read_items(module, items);
        for item in items {
            match item {
                syn::Item::Fn(f) if !attrs::is_test(&f.attrs) => self.item_fn(file, module, f),
                syn::Item::Impl(i) if !attrs::is_test(&i.attrs) => self.item_impl(file, module, i),
                syn::Item::Trait(t) if !attrs::is_test(&t.attrs) => self.item_trait(file, module, t),
                syn::Item::Struct(s) if !attrs::is_test(&s.attrs) => self.item_struct(file, s),
                syn::Item::Static(s) if !attrs::is_test(&s.attrs) => self.item_static(file, s),
                syn::Item::Mod(m) if !attrs::is_test(&m.attrs) => {
                    if let Some((_, inner)) = &m.content {
                        let mut child = module.to_vec();
                        child.push(m.ident.to_string());
                        self.items(file, &child, inner);
                    }
                }
                _ => {}
            }
        }
    }

    fn item_fn(&mut self, file: usize, module: &[String], f: &'a syn::ItemFn) {
        self.add_fn(file, module, None, &f.attrs, &f.sig, &f.block);
    }

    fn item_impl(&mut self, file: usize, module: &[String], i: &'a syn::ItemImpl) {
        let Some(ty) = type_name(&i.self_ty) else { return };
        for item in &i.items {
            if let syn::ImplItem::Fn(m) = item {
                if !attrs::is_test(&m.attrs) {
                    self.add_fn(file, module, Some(ty.clone()), &m.attrs, &m.sig, &m.block);
                }
            }
        }
    }

    fn item_trait(&mut self, file: usize, module: &[String], t: &'a syn::ItemTrait) {
        for item in &t.items {
            if let syn::TraitItem::Fn(m) = item {
                if let (Some(block), false) = (&m.default, attrs::is_test(&m.attrs)) {
                    self.add_fn(file, module, Some(t.ident.to_string()), &m.attrs, &m.sig, block);
                }
            }
        }
    }

    fn item_struct(&mut self, file: usize, s: &syn::ItemStruct) {
        let owner = s.ident.to_string();
        for (n, field) in s.fields.iter().enumerate() {
            let ident = field.ident.as_ref().map_or_else(|| n.to_string(), |i| i.to_string());
            let ty = tokens(&field.ty);
            if is_lock_type(&ty) {
                let line = field.ty.span().start().line;
                self.locks.push(LockDecl { file, line, owner: format!("{owner}.{ident}"), ident: ident.clone(), ty: ty.clone() });
            }
            self.fields.insert((owner.clone(), ident), ty);
        }
    }

    fn item_static(&mut self, file: usize, s: &syn::ItemStatic) {
        let ty = tokens(&s.ty);
        if is_lock_type(&ty) {
            let ident = s.ident.to_string();
            self.locks.push(LockDecl { file, line: s.ident.span().start().line, owner: ident.clone(), ident, ty });
        }
    }

    fn add_fn(&mut self, file: usize, module: &[String], self_ty: Option<String>, attrs: &[syn::Attribute], sig: &syn::Signature, body: &'a syn::Block) {
        let callable = callable_generics(&sig.generics);
        let params = sig.inputs.iter().filter_map(|a| match a {
            syn::FnArg::Typed(t) => Some(param(t, &callable)),
            syn::FnArg::Receiver(_) => None,
        });
        let returns = match &sig.output {
            syn::ReturnType::Type(_, t) => tokens(t),
            syn::ReturnType::Default => String::new(),
        };
        self.fns.push(FnDef {
            name: sig.ident.to_string(),
            module: module.to_vec(),
            self_ty,
            file,
            line: sig.fn_token.span.start().line,
            is_async: sig.asyncness.is_some(),
            is_command: attrs::is_command(attrs),
            has_receiver: sig.receiver().is_some(),
            params: params.collect(),
            returns_guard: ["Guard", "Locked"].iter().any(|g| returns.contains(g)),
            body,
        });
        self.nested(file, module, body);
    }

    /// Functions and statics declared inside a function's body: functions are filed under the module
    /// (callable from the body by name), statics are locks like any other.
    fn nested(&mut self, file: usize, module: &[String], body: &'a syn::Block) {
        for stmt in &body.stmts {
            match stmt {
                syn::Stmt::Item(syn::Item::Fn(f)) if !attrs::is_test(&f.attrs) => self.item_fn(file, module, f),
                syn::Stmt::Item(syn::Item::Static(s)) if !attrs::is_test(&s.attrs) => self.item_static(file, s),
                _ => {}
            }
        }
    }
}
