//! Effect-free module classification for parsed source inspection.

use super::Inspector;

impl Inspector<'_, '_> {
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
