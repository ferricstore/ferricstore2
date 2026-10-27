//! Checked inline module/function scope; does not expand macros or execute Rust.

use crate::{
    CheckError,
    rust_tokens::{self, Token},
};

pub(crate) struct TestBody {
    tokens: Vec<Token>,
}

#[derive(Default)]
struct Attributes {
    test: bool,
    ignored: bool,
    unsupported_cfg: bool,
}

struct Candidate {
    module: String,
    name: String,
    body: TestBody,
}

pub(crate) fn find_test(
    source: &str,
    module: &str,
    function: &str,
) -> Result<TestBody, CheckError> {
    let tokens = rust_tokens::lex(source)?;
    let mut candidates = Vec::new();
    scan_items(&tokens, "", false, 0, &mut candidates)?;
    let mut matching = candidates
        .into_iter()
        .filter(|candidate| candidate.module == module && candidate.name == function);
    let candidate = matching.next().ok_or_else(|| CheckError::new(format!(
        "scoped Rust test absent, ignored, conditionally unsupported or not #[test]: {module}::{function}")))?;
    if matching.next().is_some() {
        return Err(CheckError::new("ambiguous scoped Rust test identity"));
    }
    Ok(candidate.body)
}

fn close(tokens: &[Token], start: usize, open: &str, end: &str) -> Result<usize, CheckError> {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.text == open {
            depth += 1;
        }
        if token.text == end {
            depth = depth
                .checked_sub(1)
                .ok_or_else(|| CheckError::new("unbalanced Rust delimiters"))?;
            if depth == 0 {
                return Ok(index);
            }
        }
    }
    Err(CheckError::new("unclosed Rust item delimiter"))
}

fn read_attribute(
    tokens: &[Token],
    bracket: usize,
    attributes: &mut Attributes,
) -> Result<usize, CheckError> {
    let end = close(tokens, bracket, "[", "]")?;
    let words: Vec<_> = tokens[bracket + 1..end]
        .iter()
        .map(|token| token.text.as_str())
        .collect();
    if words == ["test"] {
        attributes.test = true;
    }
    if words.first() == Some(&"ignore") {
        attributes.ignored = true;
    }
    if words.first() == Some(&"cfg") && words != ["cfg", "(", "test", ")"] {
        attributes.unsupported_cfg = true;
    }
    if words.first() == Some(&"cfg_attr") {
        attributes.unsupported_cfg = true;
    }
    Ok(end + 1)
}

fn scan_items(
    tokens: &[Token],
    module: &str,
    unsupported_parent: bool,
    depth: usize,
    candidates: &mut Vec<Candidate>,
) -> Result<(), CheckError> {
    if depth > 64 {
        return Err(CheckError::new(
            "scoped Rust test navigation exceeds 64 inline modules",
        ));
    }
    let mut index = 0;
    let mut attributes = Attributes::default();
    let scope_start = candidates.len();
    while let Some(token) = tokens.get(index) {
        if token.text == "#"
            && tokens.get(index + 1).is_some_and(|token| token.text == "!")
            && tokens.get(index + 2).is_some_and(|token| token.text == "[")
        {
            let mut inner = Attributes::default();
            index = read_attribute(tokens, index + 2, &mut inner)?;
            if inner.unsupported_cfg || inner.ignored {
                candidates.truncate(scope_start);
                return Ok(());
            }
            continue;
        }
        if token.text == "#" && tokens.get(index + 1).is_some_and(|token| token.text == "[") {
            index = read_attribute(tokens, index + 1, &mut attributes)?;
            continue;
        }
        if token.text == "mod"
            && let (Some(name), Some(open)) = (tokens.get(index + 1), tokens.get(index + 2))
            && open.text == "{"
        {
            let end = close(tokens, index + 2, "{", "}")?;
            let child = if module.is_empty() {
                name.text.clone()
            } else {
                format!("{module}::{}", name.text)
            };
            scan_items(
                &tokens[index + 3..end],
                &child,
                unsupported_parent || attributes.unsupported_cfg || attributes.ignored,
                depth + 1,
                candidates,
            )?;
            index = end + 1;
            attributes = Attributes::default();
            continue;
        }
        if token.text == "fn" {
            let name = tokens
                .get(index + 1)
                .ok_or_else(|| CheckError::new("unnamed Rust function"))?;
            let open = tokens
                .iter()
                .enumerate()
                .skip(index + 2)
                .find(|(_, token)| token.text == "{" || token.text == ";")
                .ok_or_else(|| CheckError::new("Rust function has no body/terminator"))?
                .0;
            if tokens[open].text != "{" {
                index = open + 1;
                attributes = Attributes::default();
                continue;
            }
            let end = close(tokens, open, "{", "}")?;
            if attributes.test
                && !attributes.ignored
                && !attributes.unsupported_cfg
                && !unsupported_parent
            {
                candidates.push(Candidate {
                    module: module.to_owned(),
                    name: name.text.clone(),
                    body: TestBody {
                        tokens: tokens[open + 1..end].to_vec(),
                    },
                });
            }
            index = end + 1;
            attributes = Attributes::default();
            continue;
        }
        if token.text == "{" {
            index = close(tokens, index, "{", "}")? + 1;
            attributes = Attributes::default();
            continue;
        }
        if token.text == ";" {
            attributes = Attributes::default();
        }
        index += 1;
    }
    Ok(())
}

pub(crate) fn check_assertion(
    source: &str,
    body: &TestBody,
    line: usize,
    text: &str,
) -> Result<(), CheckError> {
    let actual = line
        .checked_sub(1)
        .and_then(|index| source.lines().nth(index));
    let macro_exists = own_assertion(&body.tokens, line)?;
    if actual.map(str::trim) != Some(text) || !macro_exists {
        return Err(CheckError::new(
            "Rust assertion line/text is absent or outside its scoped test",
        ));
    }
    Ok(())
}

fn own_assertion(tokens: &[Token], line: usize) -> Result<bool, CheckError> {
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        if token.text == "#" && tokens.get(index + 1).is_some_and(|token| token.text == "[") {
            let mut attributes = Attributes::default();
            index = read_attribute(tokens, index + 1, &mut attributes)?;
            if attributes.unsupported_cfg || attributes.ignored {
                return Err(CheckError::new(
                    "Rust assertion body contains an unsupported conditional statement",
                ));
            }
            continue;
        }
        let nested_function = token.text == "fn"
            && tokens.get(index + 1).is_some_and(|name| {
                name.text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            });
        if nested_function || matches!(token.text.as_str(), "impl" | "macro_rules") {
            let open = tokens
                .iter()
                .enumerate()
                .skip(index + 1)
                .find(|(_, token)| token.text == "{" || token.text == ";")
                .ok_or_else(|| CheckError::new("nested Rust item lacks a body/terminator"))?
                .0;
            index = if tokens[open].text == "{" {
                close(tokens, open, "{", "}")? + 1
            } else {
                open + 1
            };
            continue;
        }
        if tokens.get(index + 1).is_some_and(|next| next.text == "!")
            && let Some(open) = tokens.get(index + 2)
            && let Some(end_delimiter) = match open.text.as_str() {
                "(" => Some(")"),
                "[" => Some("]"),
                "{" => Some("}"),
                _ => None,
            }
        {
            if token.line == line
                && matches!(token.text.as_str(), "assert" | "assert_eq" | "assert_ne")
            {
                return Ok(true);
            }
            // All macro arguments are opaque here. Expanding arbitrary macros
            // or deciding whether nested tokens execute requires another gate.
            index = close(tokens, index + 2, &open.text, end_delimiter)? + 1;
            continue;
        }
        index += 1;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{check_assertion, find_test};

    #[test]
    fn inline_scopes_select_the_correct_same_named_test() -> Result<(), Box<dyn Error>> {
        let source = "mod first { #[test] fn same() { assert!(true); } }\nmod second {\n#[test]\nfn same() {\nassert_eq!(1, 1);\n}\n}";
        let body = find_test(source, "second", "same")?;
        check_assertion(source, &body, 5, "assert_eq!(1, 1);")?;
        assert!(
            check_assertion(
                source,
                &body,
                1,
                "mod first { #[test] fn same() { assert!(true); } }"
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn comments_raw_literals_ignored_and_disabled_tests_are_not_counterparts() {
        for source in [
            "// #[test] fn fake() { assert!(true); }",
            "/* outer /* nested */ #[test] fn fake() { assert!(true); } */",
            "const TEXT: &str = r###\"#[test] fn fake() { assert!(true); }\"###;",
            "#[test] #[ignore = \"missing prerequisite\"] fn fake() { assert!(true); }",
            "#[cfg(any())] mod hidden { #[test] fn fake() { assert!(true); } }",
            "fn fake() { assert!(true); }",
            "macro_rules! hidden { () => { #[test] fn fake() { assert!(true); } }; }",
        ] {
            assert!(find_test(source, "", "fake").is_err(), "accepted {source}");
        }
        assert!(
            find_test(
                "#[cfg(any())] mod hidden { #[test] fn fake() {} }",
                "hidden",
                "fake"
            )
            .is_err()
        );
    }

    #[test]
    fn literal_and_comment_assertions_cannot_be_linked() -> Result<(), Box<dyn Error>> {
        let source = "#[test]\nfn real() {\n// assert!(true);\nlet _message = \"assert_eq!(1, 1);\";\nassert_ne!(1, 2);\n}";
        let body = find_test(source, "", "real")?;
        assert!(check_assertion(source, &body, 3, "// assert!(true);").is_err());
        assert!(
            check_assertion(source, &body, 4, "let _message = \"assert_eq!(1, 1);\";").is_err()
        );
        check_assertion(source, &body, 5, "assert_ne!(1, 2);")?;
        Ok(())
    }

    #[test]
    fn nested_function_assertions_do_not_belong_to_the_outer_test() -> Result<(), Box<dyn Error>> {
        let source = "#[test]\nfn outer() {\nfn never_called() {\nassert!(true);\n}\n}";
        let body = find_test(source, "", "outer")?;
        assert!(check_assertion(source, &body, 4, "assert!(true);").is_err());
        Ok(())
    }

    #[test]
    fn excessively_nested_inline_modules_are_rejected_before_recursive_navigation() {
        let source = format!(
            "{}#[test] fn actual() {{}}{}",
            "mod deep {".repeat(100),
            "}".repeat(100)
        );
        assert!(find_test(&source, &vec!["deep"; 100].join("::"), "actual").is_err());
    }

    #[test]
    fn crate_and_module_inner_conditional_attributes_cannot_hide_disabled_tests()
    -> Result<(), Box<dyn Error>> {
        let active = "#![cfg(test)]\n#[test]\nfn actual() { assert_eq!(1, 1); }";
        find_test(active, "", "actual")?;
        for source in [
            "#![cfg(any())]\n#[test]\nfn actual() { assert_eq!(1, 1); }",
            "#![cfg_attr(test, ignore)]\n#[test]\nfn actual() { assert_eq!(1, 1); }",
        ] {
            assert!(
                find_test(source, "", "actual").is_err(),
                "counted disabled crate: {source}"
            );
        }
        let nested = "mod suite {\n#![cfg(any())]\n#[test]\nfn actual() { assert_eq!(1, 1); }\n}";
        assert!(find_test(nested, "suite", "actual").is_err());
        Ok(())
    }

    #[test]
    fn nonassertion_macro_token_trees_cannot_fabricate_assertion_links()
    -> Result<(), Box<dyn Error>> {
        for macro_name in ["stringify", "quote", "format_args"] {
            let source = format!(
                "#[test]\nfn actual() {{\nlet _ = {macro_name}!(\nassert_eq!(1, 2)\n);\nassert_eq!(1, 1);\n}}"
            );
            let body = find_test(&source, "", "actual")?;
            assert!(
                check_assertion(&source, &body, 4, "assert_eq!(1, 2)").is_err(),
                "inert {macro_name}! tokens counted as an assertion"
            );
            check_assertion(&source, &body, 6, "assert_eq!(1, 1);")?;
        }
        let nested =
            "#[test]\nfn actual() {\nassert!(stringify!(\nassert_eq!(1, 2)\n).len() > 0);\n}";
        let body = find_test(nested, "", "actual")?;
        assert!(check_assertion(nested, &body, 4, "assert_eq!(1, 2)").is_err());
        Ok(())
    }

    #[test]
    fn disabled_assertion_statements_cannot_be_source_links() -> Result<(), Box<dyn Error>> {
        let source = "#[test]\nfn actual() {\n#[cfg(any())]\nassert_eq!(1, 2);\n}";
        let body = find_test(source, "", "actual")?;
        assert!(check_assertion(source, &body, 4, "assert_eq!(1, 2);").is_err());
        Ok(())
    }
}
