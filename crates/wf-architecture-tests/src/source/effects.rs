//! ADR-010 runtime effect ownership for parsed production source.
//!
//! Filesystem access belongs to approved storage packages, durable sync calls
//! to approved storage modules, and thread creation to approved owner modules.
//! Async code may not make blocking calls; a blocking method name is allowed
//! only when the call is awaited directly (an async API of the same name).
//! Test sources and tooling packages are exempt.

use syn::visit::Visit;

use super::Inspector;

const BLOCKING_METHODS: &[&str] = &["sync_all", "sync_data", "recv", "recv_timeout"];
const DURABLE_SYNC: &[&str] = &["sync_all", "sync_data"];

impl Inspector<'_, '_> {
    fn effect_rules_apply(&self) -> bool {
        self.context.classification.runtime && !self.test
    }

    fn owns(&self, approved: &[String]) -> bool {
        approved.iter().any(|entry| {
            entry.split_once("::").is_some_and(|(package, module)| {
                package == self.context.package
                    && (self.module == module
                        || self
                            .module
                            .strip_prefix(module)
                            .is_some_and(|suffix| suffix.starts_with("::")))
            })
        })
    }

    pub(super) fn effect_path(&mut self, path: &[String]) {
        if !self.effect_rules_apply() {
            return;
        }
        let standard = |module: &str| {
            matches!(path, [namespace, name, ..]
                if matches!(namespace.as_str(), "std" | "core") && name == module)
        };
        let thread_item = |items: &[&str]| {
            matches!(path, [namespace, name, item, ..]
                if namespace == "std" && name == "thread" && items.contains(&item.as_str()))
        };
        if standard("fs")
            && !self
                .context
                .policy
                .filesystem_packages
                .iter()
                .any(|package| package == self.context.package)
        {
            self.report("filesystem effects belong to an approved storage package");
        }
        if thread_item(&["spawn", "Builder", "scope"])
            && !self.owns(&self.context.policy.thread_owner_modules)
        {
            self.report("threads are spawned only by approved runtime owner modules");
        }
        if path
            .last()
            .is_some_and(|last| DURABLE_SYNC.contains(&last.as_str()))
        {
            self.durable_sync();
        }
        if self.in_async && (standard("fs") || standard("net") || thread_item(&["sleep"])) {
            self.report("blocking call inside async code");
        }
    }

    pub(super) fn effect_method(&mut self, call: &syn::ExprMethodCall) {
        if !self.effect_rules_apply() {
            return;
        }
        let method = call.method.to_string();
        if DURABLE_SYNC.contains(&method.as_str()) {
            self.durable_sync();
        }
        if self.in_async
            && BLOCKING_METHODS.contains(&method.as_str())
            && !self.awaited.contains(&std::ptr::from_ref(call).addr())
        {
            self.report("blocking call inside async code");
        }
    }

    /// Visits a function with its body, but not its signature, in async context.
    pub(super) fn visit_function<'ast>(
        &mut self,
        attributes: &'ast [syn::Attribute],
        signature: &'ast syn::Signature,
        body: Option<&'ast syn::Block>,
    ) {
        for attribute in attributes {
            self.visit_attribute(attribute);
        }
        let previous = std::mem::replace(&mut self.in_async, false);
        self.visit_signature(signature);
        self.in_async = signature.asyncness.is_some();
        if let Some(body) = body {
            self.visit_block(body);
        }
        self.in_async = previous;
    }

    pub(super) fn mark_awaited(&mut self, expression: &syn::ExprAwait) {
        if let syn::Expr::MethodCall(call) = expression.base.as_ref() {
            self.awaited.insert(std::ptr::from_ref(call).addr());
        }
    }

    fn durable_sync(&mut self) {
        if !self.owns(&self.context.policy.durable_sync_modules) {
            self.report("durable sync calls belong to approved storage modules");
        }
    }
}
