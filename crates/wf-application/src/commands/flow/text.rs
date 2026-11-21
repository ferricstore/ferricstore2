//! FLOW commands sent as text through `COMMAND_EXEC` (ADR-014 step 9c), as
//! `Ferricstore.Commands.NativeAstParser` reads them: positional arguments,
//! then named options in any case, each value read by the option's kind.
//! Numbers and booleans among the arguments count as their text, as the
//! Elixir server stringifies them. The result is the typed map the typed
//! opcode would carry, read by the same parsers, so both forms decide and
//! answer alike.

mod options;
mod policy;
mod specs;

use super::term::Term;
use super::{FlowCommand, FlowError, claim};
use options::{Options, Spec, general, read, signal};

/// A text command, ready for the service.
#[derive(Debug, PartialEq)]
pub enum Parsed {
    /// A claim, for [`claim::parse`] and the queue service.
    Claim(&'static str, Term),
    /// Any other Flow command.
    Command(FlowCommand),
}

/// An argument as the parser reads it: numbers and booleans as text.
fn word(arg: &Term) -> Term {
    match arg {
        Term::Int(value) => Term::text(&value.to_string()),
        Term::Float(value) => Term::text(&value.to_string()),
        Term::Bool(value) => Term::text(if *value { "true" } else { "false" }),
        other => other.clone(),
    }
}

fn arity(name: &str) -> FlowError {
    FlowError::rejected(format!(
        "ERR wrong number of arguments for '{}' command",
        name.to_ascii_lowercase()
    ))
}

/// The options with the positional arguments in front.
fn with(positional: &[(&str, &Term)], mut options: Options) -> Term {
    let mut entries: Vec<(Vec<u8>, Term)> = positional
        .iter()
        .map(|(key, value)| (key.as_bytes().to_vec(), (*value).clone()))
        .collect();
    entries.append(&mut options.0);
    Term::Map(entries)
}

/// `nest_flow_retry_policy/1`: a retry's own limit and target go under
/// `retry`.
fn nest_retry(options: Options) -> Options {
    let (own, rest): (Vec<_>, Vec<_>) = options.0.into_iter().partition(|(key, _)| {
        key.as_slice() == b"max_retries" || key.as_slice() == b"exhausted_to"
    });
    let mut nested = Options(rest);
    if !own.is_empty() {
        nested.push("retry", Term::Map(own));
    }
    nested
}

/// How a command's options after its positional arguments are read.
#[derive(Clone, Copy)]
enum Grammar {
    General(Spec),
    Read(Spec),
    Signal,
    Retry,
    Policy,
}

/// One text command: its name, positional keys and option grammar.
struct Form {
    name: &'static str,
    positional: &'static [&'static str],
    grammar: Grammar,
}

const fn form(name: &'static str, positional: &'static [&'static str], grammar: Grammar) -> Form {
    Form {
        name,
        positional,
        grammar,
    }
}

const FORMS: &[Form] = &[
    form("FLOW.CREATE", &["id"], Grammar::General(specs::CREATE)),
    form(
        "FLOW.VALUE.PUT",
        &["value"],
        Grammar::General(specs::VALUE_PUT),
    ),
    form("FLOW.SIGNAL", &["id"], Grammar::Signal),
    form("FLOW.GET", &["id"], Grammar::Read(specs::PARTITION)),
    form("FLOW.POLICY.SET", &["type"], Grammar::Policy),
    form(
        "FLOW.POLICY.GET",
        &["type"],
        Grammar::General(specs::POLICY_GET),
    ),
    form("FLOW.CLAIM_DUE", &["type"], Grammar::Read(specs::CLAIM_DUE)),
    form("FLOW.RECLAIM", &["type"], Grammar::Read(specs::CLAIM_DUE)),
    form(
        "FLOW.EXTEND_LEASE",
        &["id", "lease_token"],
        Grammar::General(specs::EXTEND_LEASE),
    ),
    form(
        "FLOW.COMPLETE",
        &["id", "lease_token"],
        Grammar::General(specs::TERMINAL),
    ),
    form(
        "FLOW.TRANSITION",
        &["id", "from_state", "to_state"],
        Grammar::General(specs::TRANSITION),
    ),
    form("FLOW.RETRY", &["id", "lease_token"], Grammar::Retry),
    form(
        "FLOW.FAIL",
        &["id", "lease_token"],
        Grammar::General(specs::FAIL),
    ),
    form("FLOW.CANCEL", &["id"], Grammar::General(specs::CANCEL)),
    form("FLOW.REWIND", &["id"], Grammar::General(specs::REWIND)),
    form("FLOW.HISTORY", &["id"], Grammar::General(specs::HISTORY)),
    form(
        "FLOW.ATTRIBUTES",
        &["type"],
        Grammar::General(specs::INDEX_QUERY),
    ),
    form(
        "FLOW.ATTRIBUTE_VALUES",
        &["type", "attribute"],
        Grammar::General(specs::INDEX_QUERY),
    ),
    form("FLOW.STATS", &["type"], Grammar::General(specs::STATS)),
    form("FLOW.INFO", &["type"], Grammar::General(specs::PARTITION)),
    form(
        "FLOW.RETENTION_CLEANUP",
        &[],
        Grammar::General(specs::RETENTION_CLEANUP),
    ),
];

/// Text forms with item lists, not read by this server yet.
const NOT_YET: [&str; 8] = [
    "FLOW.CREATE_MANY",
    "FLOW.COMPLETE_MANY",
    "FLOW.RETRY_MANY",
    "FLOW.FAIL_MANY",
    "FLOW.CANCEL_MANY",
    "FLOW.TRANSITION_MANY",
    "FLOW.SPAWN_CHILDREN",
    "FLOW.QUERY",
];

/// Reads one FLOW text command.
///
/// # Errors
/// The Elixir server's refusal for an unknown command, a wrong argument
/// count, an invalid option, or the typed command's own refusal.
pub fn parse(name: &[u8], args: &[Term]) -> Result<Parsed, FlowError> {
    parse_with_clock(name, args, true)
}

/// [`parse`], keeping the client's `NOW` only when `client_clock` is set
/// (ADR-015).
///
/// # Errors
/// As [`parse`].
pub fn parse_with_clock(
    name: &[u8],
    args: &[Term],
    client_clock: bool,
) -> Result<Parsed, FlowError> {
    let name = String::from_utf8_lossy(name).to_ascii_uppercase();
    if NOT_YET.contains(&name.as_str()) {
        return Err(FlowError::rejected(format!(
            "ERR {name} as a text command is not supported by this server yet"
        )));
    }
    if name == "FLOW.QUERY.INDEXES" {
        let args: Vec<Term> = args.iter().map(word).collect();
        return super::query::indexes::parse(&args)
            .map(|request| Parsed::Command(FlowCommand::QueryIndexes(request)));
    }
    let Some(form) = FORMS.iter().find(|form| form.name == name) else {
        return Err(FlowError::rejected(format!(
            "ERR unknown command '{}', with args beginning with: ",
            name.to_ascii_lowercase()
        )));
    };
    let args: Vec<Term> = args.iter().map(word).collect();
    let (positional, rest) = args
        .split_at_checked(form.positional.len())
        .ok_or_else(|| arity(&name))?;
    let options = match form.grammar {
        Grammar::General(spec) => general(rest, spec)?,
        Grammar::Read(spec) => read(rest, spec)?,
        Grammar::Signal => signal(rest)?,
        Grammar::Retry => nest_retry(general(rest, specs::RETRY)?),
        Grammar::Policy => policy::options(rest)?,
    };
    let named: Vec<(&str, &Term)> = form.positional.iter().copied().zip(positional).collect();
    let map = with(&named, options);
    let map = if client_clock {
        map
    } else {
        super::without_client_time(&map)
    };
    match form.name {
        "FLOW.POLICY.SET" => super::policy::parse_set_text(&map)
            .map(|set| Parsed::Command(FlowCommand::PolicySet(Box::new(set)))),
        "FLOW.CLAIM_DUE" | "FLOW.RECLAIM" => {
            claim::parse(form.name, &map)?;
            Ok(Parsed::Claim(form.name, map))
        }
        typed => super::parse(typed, &map).map(Parsed::Command),
    }
}
