//! Effect-free module classification for parsed source inspection.

use super::Inspector;

impl Inspector<'_, '_> {
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
