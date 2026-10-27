//! Effect-free module classification for parsed source inspection.

use std::collections::{BTreeMap, BTreeSet};

use super::{Inspector, use_paths};

impl Inspector<'_, '_> {
    pub(super) fn begin_metadata_scope<'ast>(
        &mut self,
        items: impl Iterator<Item = &'ast syn::Item>,
        isolated: bool,
    ) -> BTreeMap<String, Vec<String>> {
        let previous = self.aliases.clone();
        let raw = Self::metadata_use_bindings(items);
        let inherited = previous
            .iter()
            .filter(|(name, target)| {
                !isolated && !raw.contains_key(*name) && Self::metadata_iterator(target)
            })
            .map(|(name, target)| (name.clone(), target.clone()))
            .collect();
        self.aliases.retain(|name, target| {
            !Self::metadata_iterator(target) || (!isolated && !raw.contains_key(name))
        });
        self.aliases
            .extend(Self::resolve_metadata_uses(&raw, &inherited));
        previous
    }

    fn metadata_use_bindings<'ast>(
        items: impl Iterator<Item = &'ast syn::Item>,
    ) -> BTreeMap<String, Vec<String>> {
        let mut bindings = BTreeMap::new();
        for item in items {
            if let syn::Item::Use(item) = item {
                let mut paths = Vec::new();
                use_paths(&item.tree, Vec::new(), &mut paths);
                for (path, alias) in paths {
                    if let Some(alias) = alias {
                        bindings.insert(alias, path);
                    }
                }
            }
        }
        bindings
    }

    fn resolve_metadata_uses(
        raw: &BTreeMap<String, Vec<String>>,
        inherited: &BTreeMap<String, Vec<String>>,
    ) -> BTreeMap<String, Vec<String>> {
        let mut known = inherited.clone();
        let mut resolved = BTreeMap::new();
        for _ in 0..raw.len() {
            let mut changed = false;
            for (name, path) in raw {
                if known.contains_key(name) {
                    continue;
                }
                let target = if let Some(prefix) = path.first().and_then(|first| known.get(first)) {
                    let mut target = prefix.clone();
                    target.extend_from_slice(&path[1..]);
                    target
                } else {
                    path.clone()
                };
                if Self::metadata_iterator(&target) {
                    known.insert(name.clone(), target.clone());
                    resolved.insert(name.clone(), target);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        resolved
    }

    pub(super) fn end_metadata_scope(&mut self, previous: BTreeMap<String, Vec<String>>) {
        let touched: BTreeSet<_> = previous
            .iter()
            .chain(&self.aliases)
            .filter(|(_, target)| Self::metadata_iterator(target))
            .map(|(name, _)| name.clone())
            .collect();
        for name in touched {
            if let Some(target) = previous.get(&name) {
                self.aliases.insert(name, target.clone());
            } else {
                self.aliases.remove(&name);
            }
        }
        drop(previous);
    }

    pub(super) fn visit_metadata_block(&mut self, block: &syn::Block) {
        let previous = self.begin_metadata_scope(
            block.stmts.iter().filter_map(|statement| {
                if let syn::Stmt::Item(item) = statement {
                    Some(item)
                } else {
                    None
                }
            }),
            false,
        );
        syn::visit::visit_block(self, block);
        self.end_metadata_scope(previous);
    }

    pub(super) fn resolved_path(&self, written: &[String]) -> Vec<String> {
        if let Some(alias) = written.first().and_then(|first| self.aliases.get(first)) {
            let mut path = alias.clone();
            path.extend_from_slice(&written[1..]);
            path
        } else {
            written.to_vec()
        }
    }

    pub(super) fn metadata_iterator(path: &[String]) -> bool {
        matches!(path, [namespace, module, item]
            if matches!(namespace.as_str(), "std" | "core")
                && module == "array" && item == "IntoIter")
    }

    pub(super) fn standard_path(&mut self, path: &[String], exact_metadata: bool) {
        const PURE_MODULES: &[&str] = &[
            "borrow",
            "cmp",
            "collections",
            "convert",
            "default",
            "error",
            "fmt",
            "hash",
            "iter",
            "marker",
            "mem",
            "num",
            "ops",
            "option",
            "result",
            "slice",
            "str",
            "string",
            "vec",
        ];
        if exact_metadata && Self::metadata_iterator(path) {
            return;
        }
        if path
            .get(1)
            .is_none_or(|module| !PURE_MODULES.contains(&module.as_str()))
        {
            self.report("pure values/domain source references effects or an unclassified standard-library module");
        }
    }

    pub(super) fn pure(&self) -> bool {
        let under = |prefix: &str| {
            self.module
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with("::"))
        };
        let ports_module = matches!(
            self.module.as_str(),
            "log::record_batch"
                | "log::record_batch::intent"
                | "log::record_batch::response"
                | "log::record_batch::atomic_start"
                | "log::record_batch::atomic_start::codec"
                | "log::record_batch::atomic_start::response"
                | "log::record_batch::atomic_start::response::scan"
        ) || [
            "blob",
            "clock",
            "consensus::value",
            "log::value_batch",
            "log::record_batch::phase",
            "log::record_batch::fresh",
            "log::record_batch::independent",
            "log::record_batch::response::independent",
            "state::transaction::independent",
            "state::transaction::atomic_start",
            "log::public_mutation",
            "log::claim",
            "consensus::membership",
            "log::control",
            "log::common",
            "state::control",
            "snapshot::metadata",
        ]
        .into_iter()
        .any(under);
        !self.test
            && (matches!(self.context.package, "wf-types" | "wf-domain" | "wf-wire")
                || (self.context.package == "wf-storage"
                    && (under("state::value") || under("state::key")))
                || (self.context.package == "wf-ports" && ports_module))
    }
}
