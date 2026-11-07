//! Name resolution, best effort: a path or a method call to the crate function it names, or to the
//! outside path it names once `use`s are expanded (`fs::read` → `std::fs::read`), for the effect rules.
//! What it can't follow is said in the inventory's "What this can't see".

use super::index::{FnId, Index};
use super::modinfo::absolute;

/// Where a path leads.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolved {
    /// Crate functions (several only for one name defined under different `cfg`s).
    Crate(Vec<FnId>),
    /// Outside the crate: the path with `use`s expanded (`std::fs::read`).
    Outside(Vec<String>),
    Unknown,
}

/// Where a path is used: its module and the `impl` it's in.
pub struct Scope<'s> {
    pub module: &'s [String],
    pub self_ty: Option<&'s str>,
}

impl Index<'_> {
    pub fn is_type(&self, name: &str) -> bool {
        self.modules.values().any(|m| m.types.contains(name))
    }

    /// The function `segs` names, as written at `scope`.
    pub fn resolve_path(&self, scope: &Scope, segs: &[String]) -> Resolved {
        let Some(first) = segs.first() else { return Resolved::Unknown };
        if first == "Self" {
            return match (scope.self_ty, segs.get(1)) {
                (Some(t), Some(m)) if segs.len() == 2 => self.method_on(t, m),
                _ => Resolved::Unknown,
            };
        }
        let module = self.modules.get(scope.module);
        if segs.len() == 1 {
            if let Some(ids) = module.and_then(|m| m.fns.get(first)) {
                return Resolved::Crate(ids.clone());
            }
        }
        let expanded = match first.as_str() {
            "crate" | "self" | "super" => absolute(scope.module, segs),
            _ => match module.and_then(|m| m.uses.get(first)) {
                Some(full) => full.iter().chain(&segs[1..]).cloned().collect(),
                None if module.is_some_and(|m| m.children.contains(first)) => absolute(scope.module, &[&["self".to_string()], segs].concat()),
                None if segs.len() == 2 && self.is_type(first) => return self.method_on(first, &segs[1]),
                None if segs.len() == 1 => return self.through_globs(scope.module, first),
                None => return Resolved::Outside(segs.to_vec()),
            },
        };
        match expanded.first().map(String::as_str) {
            Some("crate") => self.walk_crate_path(&expanded[1..], 0),
            _ => Resolved::Outside(expanded),
        }
    }

    /// `path` (under the crate root) followed module by module: to a free function, a type's method,
    /// or through a `use` (or `use …::*`) of the module it ends in. `depth` stops re-export loops.
    fn walk_crate_path(&self, path: &[String], depth: usize) -> Resolved {
        let mut module: Vec<String> = vec![];
        for (i, seg) in path.iter().enumerate() {
            let info = self.modules.get(&module);
            let last = i + 1 == path.len();
            if last {
                if let Some(ids) = info.and_then(|m| m.fns.get(seg)) {
                    return Resolved::Crate(ids.clone());
                }
                return self.reexport(&module, seg, depth);
            }
            if info.is_some_and(|m| m.children.contains(seg)) {
                module.push(seg.clone());
                continue;
            }
            if self.is_type(seg) && i + 2 == path.len() {
                return self.method_on(seg, &path[i + 1]);
            }
            if let Some(full) = info.and_then(|m| m.uses.get(seg)).filter(|_| depth < 8) {
                let rest: Vec<String> = full.iter().skip(1).chain(&path[i + 1..]).cloned().collect();
                return if full.first().is_some_and(|f| f == "crate") { self.walk_crate_path(&rest, depth + 1) } else { Resolved::Unknown };
            }
            return Resolved::Unknown;
        }
        Resolved::Unknown
    }

    /// `name` as `module` re-exports it, by name or through a glob.
    fn reexport(&self, module: &[String], name: &str, depth: usize) -> Resolved {
        if depth >= 8 {
            return Resolved::Unknown;
        }
        let Some(info) = self.modules.get(module) else { return Resolved::Unknown };
        if let Some(full) = info.uses.get(name) {
            return match full.first().map(String::as_str) {
                Some("crate") => self.walk_crate_path(&full[1..], depth + 1),
                _ => Resolved::Outside(full.clone()),
            };
        }
        for glob in info.globs.iter().filter(|g| g.first().is_some_and(|f| f == "crate")) {
            let mut path = glob[1..].to_vec();
            path.push(name.to_string());
            if let r @ Resolved::Crate(_) = self.walk_crate_path(&path, depth + 1) {
                return r;
            }
        }
        Resolved::Unknown
    }

    /// A bare name not defined here: through the module's globs, else an outside name (`drop`, `Some`).
    fn through_globs(&self, module: &[String], name: &str) -> Resolved {
        match self.reexport(module, name, 0) {
            Resolved::Unknown => Resolved::Outside(vec![name.to_string()]),
            r => r,
        }
    }

    pub fn method_on(&self, ty: &str, name: &str) -> Resolved {
        match self.methods.get(&(ty.to_string(), name.to_string())) {
            Some(ids) => Resolved::Crate(ids.clone()),
            None => Resolved::Unknown,
        }
    }

    /// A method call on a receiver of unknown type: the crate's method of that name when exactly one
    /// type has one and the name isn't one the standard library's common types also use.
    pub fn method_by_name(&self, name: &str) -> Resolved {
        match self.method_owners.get(name) {
            Some(owners) if owners.len() == 1 && !STD_METHODS.contains(&name) => self.method_on(owners.iter().next().unwrap(), name),
            _ => Resolved::Unknown,
        }
    }

    /// The crate type a type (as tokens) is about: the first crate type with methods it names
    /// (`State<'_, Arc<Hub>>` → `Hub`).
    pub fn type_hint(&self, ty: &str) -> Option<String> {
        ty.split(|c: char| !c.is_alphanumeric() && c != '_').find(|w| self.method_owners.values().any(|o| o.contains(*w))).map(str::to_string)
    }
}

/// Method names the standard library's (and serde_json's, Tauri's) common types have: a call of one on
/// a receiver of unknown type is far more likely theirs than the crate's.
const STD_METHODS: &[&str] = &[
    "all", "and_then", "any", "as_array", "as_bool", "as_f64", "as_i64", "as_mut", "as_object", "as_ref", "as_str", "as_u64", "borrow", "borrow_mut", "chars", "clear", "clone",
    "close", "cmp", "collect", "contains", "contains_key", "count", "default", "drain", "emit", "entry", "eq", "extend", "filter", "filter_map", "find", "first", "flush", "fmt",
    "from", "get", "get_mut", "hash", "insert", "into", "into_iter", "is_empty", "is_none", "is_some", "iter", "iter_mut", "join", "keys", "kill", "last", "len", "lines", "lock",
    "manage", "map", "map_err", "max", "min", "new", "next", "ok", "or_default", "or_insert", "parse", "pop", "push", "read", "recv", "remove", "replace", "retain", "send",
    "set", "sort", "sort_by", "split", "start", "starts_with", "state", "stop", "take", "to_owned", "to_string", "to_vec", "trim", "try_recv", "unwrap", "unwrap_or", "values",
    "wait", "write",
];
