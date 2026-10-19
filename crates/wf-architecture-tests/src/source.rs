use std::collections::{BTreeMap, BTreeSet};

use syn::{
    Attribute, Expr, Lit, Meta, Token, UseTree, Visibility,
    parse::{Parse, ParseStream, Parser},
    punctuated::Punctuated,
    visit::Visit,
};

use crate::CheckResult;
use crate::policy::{Package, Policy};

mod purity;

pub(crate) struct Context<'a> {
    pub(crate) package: &'a str,
    pub(crate) module: &'a str,
    pub(crate) root_file: bool,
    pub(crate) test_file: bool,
    pub(crate) classification: &'a Package,
    pub(crate) policy: &'a Policy,
    pub(crate) dependency_names: &'a BTreeMap<String, String>,
}

pub(crate) fn module_name(relative: &str) -> String {
    let module = relative
        .strip_prefix("src/")
        .unwrap_or(relative)
        .trim_end_matches(".rs")
        .trim_end_matches("/mod")
        .replace('/', "::");
    if module == "lib" {
        String::new()
    } else {
        module
    }
}

pub(crate) fn check(source: &str, context: &Context<'_>) -> CheckResult<Vec<String>> {
    let syntax = syn::parse_file(source)?;
    let mut visitor = Inspector {
        context,
        module: context.module.into(),
        aliases: BTreeMap::new(),
        violations: Vec::new(),
        test: context.test_file,
        in_function: false,
    };
    if let Some(finding) =
        crate::size::check(source, context.classification.runtime, context.test_file)
    {
        visitor.report(&finding);
    }
    if context.root_file && !syntax.attrs.iter().any(forbids_unsafe) {
        visitor.report("compiler target must declare #![forbid(unsafe_code)]");
    }
    if context.classification.runtime {
        if source
            .split("archunit:")
            .skip(1)
            .any(|suffix| suffix.trim_start().starts_with("ignore"))
        {
            visitor.report("unreviewed archunit ignore directive");
        }
        if context.module.is_empty() {
            let modules: BTreeSet<_> = syntax
                .items
                .iter()
                .filter_map(|item| {
                    if let syn::Item::Mod(module) = item {
                        Some(module.ident.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            let classified: BTreeSet<_> = context.classification.modules.keys().cloned().collect();
            if modules != classified {
                visitor.report("root modules differ from complete module classification");
            }
        } else if !context.test_file
            && !context
                .classification
                .modules
                .contains_key(context.module.split("::").next().unwrap_or(""))
        {
            visitor.report("unclassified production module");
        }
    }
    visitor.visit_file(&syntax);
    Ok(visitor.violations)
}

fn forbids_unsafe(attribute: &Attribute) -> bool {
    attribute.path().is_ident("forbid")
        && attribute
            .parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
            .is_ok_and(|paths| paths.iter().any(|path| path.is_ident("unsafe_code")))
}

struct Inspector<'a, 'b> {
    context: &'a Context<'b>,
    module: String,
    aliases: BTreeMap<String, Vec<String>>,
    violations: Vec<String>,
    test: bool,
    in_function: bool,
}

impl Inspector<'_, '_> {
    fn report(&mut self, message: &str) {
        self.violations.push(message.to_owned());
    }

    fn path(&mut self, written: &[String]) {
        let Some(first) = written.first() else { return };
        let path = if let Some(alias) = self.aliases.get(first) {
            let mut path = alias.clone();
            path.extend_from_slice(&written[1..]);
            path
        } else {
            written.to_vec()
        };
        let Some(first) = path.first() else { return };
        let canonical = self
            .context
            .dependency_names
            .get(first)
            .cloned()
            .unwrap_or_else(|| first.replace('_', "-"));
        if self.context.classification.runtime
            && !self.test
            && self
                .context
                .policy
                .packages
                .get(&canonical)
                .is_some_and(|package| !package.runtime)
        {
            self.report("runtime source references M0-only tooling");
        }
        if self.pure() && matches!(first.as_str(), "std" | "core" | "alloc") {
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
            if path
                .get(1)
                .is_none_or(|module| !PURE_MODULES.contains(&module.as_str()))
            {
                self.report("pure values/domain source references effects or an unclassified standard-library module");
            }
        }
        if self.context.package == "wf-wire"
            && canonical == "wf-domain"
            && path.get(1) != Some(&self.context.policy.wire_domain_module)
        {
            self.report("wire may reference only wf_domain::value");
        }
        if self.context.package == "wf-worker" {
            if canonical == "wf-ports"
                && path.get(1) != Some(&self.context.policy.worker_port_module)
            {
                self.report("worker may reference only wf_ports::client");
            }
            if canonical == "wf-wire"
                && !self
                    .context
                    .policy
                    .worker_wire_modules
                    .iter()
                    .any(|approved| {
                        self.module == *approved
                            || self.module.starts_with(&format!("{approved}::"))
                    })
            {
                self.report("worker wire access requires a private client transport module");
            }
        }
        self.facade_path(first, &path, &canonical);
    }

    fn facade_path(&mut self, first: &str, path: &[String], canonical: &str) {
        if self.context.package == "wf-domain"
            && self.module.split("::").next()
                == Some(self.context.policy.wire_domain_module.as_str())
            && (first == "super"
                || (first == "crate"
                    && path.get(1) != Some(&self.context.policy.wire_domain_module)))
        {
            self.report("approved domain value module cannot expose other domain modules");
        }
        if self.context.package == "wf-ports"
            && self.module.split("::").next()
                == Some(self.context.policy.worker_port_module.as_str())
            && (first == "super"
                || (first == "crate"
                    && path.get(1) != Some(&self.context.policy.worker_port_module))
                || (canonical == "wf-ports"
                    && path.get(1) != Some(&self.context.policy.worker_port_module))
                || (canonical == "wf-domain"
                    && path.get(1) != Some(&self.context.policy.wire_domain_module)))
        {
            self.report(
                "client port facade cannot expose server capabilities or non-value domain modules",
            );
        }
    }

    fn lint(&mut self, meta: &Meta) {
        let Meta::List(list) = meta else { return };
        if list.path.is_ident("cfg_attr") {
            if let Ok(nested) =
                list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            {
                for meta in &nested {
                    self.lint(meta);
                }
            } else {
                self.report("uninspectable cfg_attr");
            }
        }
        if !list.path.is_ident("allow") && !list.path.is_ident("expect") {
            return;
        }
        let Ok(entries) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        else {
            self.report("uninspectable lint suppression");
            return;
        };
        let mut reason = false;
        let mut lints = Vec::new();
        for entry in &entries {
            match entry {
                Meta::NameValue(value) if value.path.is_ident("reason") => {
                    reason = matches!(&value.value, Expr::Lit(literal) if matches!(&literal.lit, Lit::Str(text) if text.value().trim().len() >= 12));
                }
                Meta::Path(path) => {
                    lints.push(
                        path.segments
                            .iter()
                            .map(|segment| segment.ident.to_string())
                            .collect::<Vec<_>>(),
                    );
                }
                _ => self.report("unclassified lint suppression syntax"),
            }
        }
        let narrow = lints.len() == 1
            && lints.iter().all(|lint| {
                lint.len() == 2
                    && lint[0] == "clippy"
                    && ![
                        "all",
                        "pedantic",
                        "restriction",
                        "style",
                        "correctness",
                        "complexity",
                        "perf",
                        "suspicious",
                        "nursery",
                    ]
                    .contains(&lint[1].as_str())
            });
        if !self.test || !self.in_function || !reason || !narrow {
            self.report("lint exceptions require one specific Clippy lint on a test function with a meaningful reason");
        }
    }
}

impl<'ast> Visit<'ast> for Inspector<'_, '_> {
    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        self.lint(&attribute.meta);
        let approved = match (self.context.package, self.module.as_str()) {
            ("wf-storage", "tests::durable_log::main::disk_backend") => {
                Some("../state_store/faults/backend.rs")
            }
            ("wf-node", "tests::snapshot_recovery::main::fixture_data") => {
                Some("../recovery/fixtures/shared.rs")
            }
            ("wf-node", "tests::snapshot_recovery::main::disk_backend") => {
                Some("../../../wf-storage/tests/state_store/faults/backend.rs")
            }
            _ => None,
        };
        let audited = self.context.test_file
            && matches!((&attribute.meta, approved),
            (Meta::NameValue(value), Some(path)) if matches!(&value.value,
                Expr::Lit(literal) if matches!(&literal.lit, Lit::Str(text) if text.value() == path)));
        if attribute.path().is_ident("path") && self.context.classification.runtime && !audited {
            self.report("module path redirection requires an audit gate");
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.path(
            &path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>(),
        );
        syn::visit::visit_path(self, path);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut paths = Vec::new();
        use_paths(&item.tree, Vec::new(), &mut paths);
        for (path, alias) in paths {
            self.path(&path);
            if self.context.package == "wf-worker"
                && !matches!(item.vis, Visibility::Inherited)
                && (path.first().is_some_and(|first| first == "client")
                    || (path
                        .first()
                        .is_some_and(|first| matches!(first.as_str(), "crate" | "self"))
                        && path.get(1).is_some_and(|second| second == "client")))
            {
                self.report("private worker transport modules cannot be re-exported");
            }
            if let Some(alias) = alias {
                self.aliases.insert(alias, path);
            }
        }
        syn::visit::visit_item_use(self, item);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        let path = vec![item.ident.to_string()];
        self.path(&path);
        if let Some((_, alias)) = &item.rename {
            self.aliases.insert(alias.to_string(), path);
        }
        syn::visit::visit_item_extern_crate(self, item);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if self.context.classification.runtime
            && self.module.is_empty()
            && let Some(public) = self
                .context
                .classification
                .modules
                .get(&item.ident.to_string())
            && *public != matches!(item.vis, Visibility::Public(_))
        {
            self.report("module visibility differs from approved facade");
        }
        let previous_module = self.module.clone();
        let previous_test = self.test;
        self.module = if previous_module.is_empty() {
            item.ident.to_string()
        } else {
            format!("{previous_module}::{}", item.ident)
        };
        self.test |= item.attrs.iter().any(|attribute| {
            attribute.path().is_ident("cfg")
                && matches!(&attribute.meta, Meta::List(list) if list.tokens.to_string() == "test")
        });
        if self.context.package == "wf-worker"
            && (self.module == "client" || self.module.starts_with("client::"))
            && !matches!(item.vis, Visibility::Inherited)
        {
            self.report("worker client transport modules must remain private");
        }
        syn::visit::visit_item_mod(self, item);
        self.module = previous_module;
        self.test = previous_test;
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous_test = self.test;
        let previous_function = self.in_function;
        self.test |= item
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("test"));
        self.in_function = true;
        syn::visit::visit_item_fn(self, item);
        self.test = previous_test;
        self.in_function = previous_function;
    }

    fn visit_signature(&mut self, signature: &'ast syn::Signature) {
        if signature.unsafety.is_some() {
            self.report("first-party unsafe function");
        }
        if self.pure() && signature.asyncness.is_some() {
            self.report("pure values/domain source declares async behavior");
        }
        syn::visit::visit_signature(self, signature);
    }

    fn visit_expr_unsafe(&mut self, expression: &'ast syn::ExprUnsafe) {
        self.report("first-party unsafe block");
        syn::visit::visit_expr_unsafe(self, expression);
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if item.unsafety.is_some() {
            self.report("first-party unsafe impl");
        }
        syn::visit::visit_item_impl(self, item);
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        if item.unsafety.is_some() {
            self.report("first-party unsafe trait");
        }
        syn::visit::visit_item_trait(self, item);
    }
    fn visit_item_foreign_mod(&mut self, item: &'ast syn::ItemForeignMod) {
        self.report("first-party foreign/unsafe boundary");
        syn::visit::visit_item_foreign_mod(self, item);
    }
    fn visit_type_bare_fn(&mut self, item: &'ast syn::TypeBareFn) {
        if item.unsafety.is_some() {
            self.report("first-party unsafe function type");
        }
        syn::visit::visit_type_bare_fn(self, item);
    }
    fn visit_expr_async(&mut self, expression: &'ast syn::ExprAsync) {
        if self.pure() {
            self.report("pure values/domain source contains async behavior");
        }
        syn::visit::visit_expr_async(self, expression);
    }
    fn visit_expr_await(&mut self, expression: &'ast syn::ExprAwait) {
        if self.pure() {
            self.report("pure values/domain source awaits effects");
        }
        syn::visit::visit_expr_await(self, expression);
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if self.pure() {
            self.report("pure values/domain source declares global state");
        }
        syn::visit::visit_item_static(self, item);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let name = mac
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        if self.context.classification.runtime
            && !self.test
            && name
                .as_deref()
                .is_none_or(|name| !["vec", "format", "write", "matches"].contains(&name))
        {
            self.report("unreviewed runtime macro/generated code requires an audit gate");
        }
        if self.context.classification.runtime && !self.test {
            self.inspect_expression_macro(mac, name.as_deref());
        }
        syn::visit::visit_macro(self, mac);
    }
}

impl Inspector<'_, '_> {
    fn inspect_expression_macro(&mut self, mac: &syn::Macro, name: Option<&str>) {
        match name {
            Some("format" | "vec" | "write") => {
                if let Ok(arguments) =
                    Punctuated::<Expr, Token![,]>::parse_terminated.parse2(mac.tokens.clone())
                {
                    if name == Some("write")
                        && !matches!(arguments.iter().nth(1), Some(Expr::Lit(literal))
                            if matches!(&literal.lit, Lit::Str(_)))
                    {
                        self.report("uninspectable approved expression macro");
                    }
                    for expression in &arguments {
                        self.visit_expr(expression);
                    }
                } else if name == Some("vec") {
                    if let Ok(repeat) = syn::parse2::<VectorRepeat>(mac.tokens.clone()) {
                        self.visit_expr(&repeat.item);
                        self.visit_expr(&repeat.length);
                    } else {
                        self.report("uninspectable approved expression macro");
                    }
                } else {
                    self.report("uninspectable approved expression macro");
                }
            }
            Some("matches") => {
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
            _ => {}
        }
    }
}

struct VectorRepeat {
    item: Expr,
    length: Expr,
}

impl Parse for VectorRepeat {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let item = input.parse()?;
        input.parse::<Token![;]>()?;
        let length = input.parse()?;
        Ok(Self { item, length })
    }
}

struct MatchArguments {
    item: Expr,
    pattern: syn::Pat,
    guard: Option<Expr>,
}

impl Parse for MatchArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let item = input.parse()?;
        input.parse::<Token![,]>()?;
        let pattern = syn::Pat::parse_multi_with_leading_vert(input)?;
        let guard = if input.peek(Token![if]) {
            input.parse::<Token![if]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        Ok(Self {
            item,
            pattern,
            guard,
        })
    }
}

fn use_paths(tree: &UseTree, prefix: Vec<String>, result: &mut Vec<(Vec<String>, Option<String>)>) {
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(path.ident.to_string());
            use_paths(&path.tree, prefix, result);
        }
        UseTree::Name(name) => {
            let mut path = prefix;
            if name.ident != "self" {
                path.push(name.ident.to_string());
            }
            result.push((path, Some(name.ident.to_string())));
        }
        UseTree::Rename(rename) => {
            let mut path = prefix;
            if rename.ident != "self" {
                path.push(rename.ident.to_string());
            }
            result.push((path, Some(rename.rename.to_string())));
        }
        UseTree::Glob(_) => result.push((prefix, None)),
        UseTree::Group(group) => {
            for item in &group.items {
                use_paths(item, prefix.clone(), result);
            }
        }
    }
}
