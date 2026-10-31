//! Effect-free module classification for parsed source inspection.

use std::collections::{BTreeMap, BTreeSet};

use syn::{Expr, Lit, Meta, Token, parse::Parser, punctuated::Punctuated, visit::Visit};

use super::{Inspector, MatchArguments, VectorRepeat, use_paths};

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
            let syn::Item::Use(item) = item else { continue };
            let mut paths = Vec::new();
            use_paths(&item.tree, Vec::new(), &mut paths);
            bindings.extend(
                paths
                    .into_iter()
                    .filter_map(|(path, alias)| alias.map(|alias| (alias, path))),
            );
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
            if !Self::resolve_metadata_pass(raw, &mut known, &mut resolved) {
                break;
            }
        }
        resolved
    }

    fn resolve_metadata_pass(
        raw: &BTreeMap<String, Vec<String>>,
        known: &mut BTreeMap<String, Vec<String>>,
        resolved: &mut BTreeMap<String, Vec<String>>,
    ) -> bool {
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
        changed
    }

    pub(super) fn store_alias(&mut self, alias: String, path: Vec<String>) {
        let resolved = self.resolved_path(&path);
        let target = if Self::metadata_iterator(&resolved) {
            resolved
        } else {
            path
        };
        self.aliases.insert(alias, target);
    }

    pub(super) fn inspect_cfg_attr(&mut self, list: &syn::MetaList) {
        if let Ok(nested) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) {
            for meta in &nested {
                self.lint(meta);
            }
        } else {
            self.report("uninspectable cfg_attr");
        }
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
        // Only the approved immutable frame owner in this neutral port signature.
        if self.context.package == "wf-ports"
            && self.module == "consensus::public_credit"
            && matches!(path, [namespace, module, item]
                if namespace == "std" && module == "sync" && item == "Arc")
        {
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
            "consensus::catalog_publication",
            "consensus::public_credit",
            "log::value_batch",
            "log::record_batch::phase",
            "log::record_batch::fresh",
            "log::record_batch::step_continue",
            "log::record_batch::independent",
            "log::record_batch::response::independent",
            "state::transaction::independent",
            "state::transaction::atomic_start",
            "state::transaction::step_continue",
            "log::public_mutation",
            "log::claim",
            "consensus::membership",
            "log::control",
            "log::common",
            "log::catalog_progress",
            "log::catalog_prune",
            "log::public_credit",
            "log::release",
            "log::model",
            "state::control",
            "state::credit_catalog",
            "state::governance_catalog",
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

impl Inspector<'_, '_> {
    pub(super) fn inspect_expression_macro(&mut self, mac: &syn::Macro, name: Option<&str>) {
        match name {
            Some("format" | "vec" | "write") => self.inspect_list_macro(mac, name),
            Some("matches") => self.inspect_matches_macro(mac),
            _ => {}
        }
    }

    fn inspect_list_macro(&mut self, mac: &syn::Macro, name: Option<&str>) {
        let Ok(arguments) =
            Punctuated::<Expr, Token![,]>::parse_terminated.parse2(mac.tokens.clone())
        else {
            if name == Some("vec") {
                self.inspect_repeat_macro(mac);
            } else {
                self.report("uninspectable approved expression macro");
            }
            return;
        };
        if name == Some("write")
            && !matches!(arguments.iter().nth(1), Some(Expr::Lit(literal))
                if matches!(&literal.lit, Lit::Str(_)))
        {
            self.report("uninspectable approved expression macro");
        }
        for expression in &arguments {
            self.visit_expr(expression);
        }
    }

    fn inspect_repeat_macro(&mut self, mac: &syn::Macro) {
        if let Ok(repeat) = syn::parse2::<VectorRepeat>(mac.tokens.clone()) {
            self.visit_expr(&repeat.item);
            self.visit_expr(&repeat.length);
        } else {
            self.report("uninspectable approved expression macro");
        }
    }

    fn inspect_matches_macro(&mut self, mac: &syn::Macro) {
        if let Ok(arguments) = syn::parse2::<MatchArguments>(mac.tokens.clone()) {
            self.visit_expr(&arguments.item);
            self.visit_pat(&arguments.pattern);
            if let Some(guard) = &arguments.guard {
                self.visit_expr(guard);
            }
        } else {
            self.report("uninspectable approved expression macro");
        }
    }
}
