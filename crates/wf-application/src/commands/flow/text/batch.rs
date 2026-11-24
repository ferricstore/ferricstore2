//! The batch commands and `FLOW.SPAWN_CHILDREN` as text (audit gap G56),
//! as `NativeAstParser`'s `parse_flow_*_many` and
//! `parse_flow_spawn_children/1` read them: positional arguments, options
//! up to `ITEMS` (a value spelled `items` stays a value), then fixed-width
//! items; `MIXED` as the partition (or a spawn's first item) gives each item
//! its own. The result is the typed map the typed opcode carries.

use super::super::FlowError;
use super::super::term::Term;
use super::options::{Options, Spec, general, int, syntax, upper, upper_is};
use super::{arity, nest_retry, specs};

/// A batch command's layout: its positional names, option spec and items.
struct Layout {
    positional: &'static [&'static str],
    spec: Spec,
    items: Items,
}

#[derive(Clone, Copy)]
enum Items {
    Create,
    Terminal,
    Cancel,
    Transition,
    Children,
}

fn layout(name: &str) -> Option<Layout> {
    let (positional, spec, items): (&[&str], Spec, Items) = match name {
        "FLOW.CREATE_MANY" => (&["partition_key"], specs::CREATE_MANY, Items::Create),
        "FLOW.COMPLETE_MANY" => (&["partition_key"], specs::MANY_TERMINAL, Items::Terminal),
        "FLOW.RETRY_MANY" => (&["partition_key"], specs::MANY_RETRY, Items::Terminal),
        "FLOW.FAIL_MANY" => (&["partition_key"], specs::MANY_FAIL, Items::Terminal),
        "FLOW.CANCEL_MANY" => (&["partition_key"], specs::MANY_CANCEL, Items::Cancel),
        "FLOW.TRANSITION_MANY" => (
            &["partition_key", "from_state", "to_state"],
            specs::MANY_TRANSITION,
            Items::Transition,
        ),
        "FLOW.SPAWN_CHILDREN" => (&["id"], specs::SPAWN_CHILDREN, Items::Children),
        _ => return None,
    };
    Some(Layout {
        positional,
        spec,
        items,
    })
}

/// Whether `name` is a text batch form this module reads.
pub(super) fn is_batch(name: &str) -> bool {
    layout(name).is_some()
}

/// `split_items/1`: the option words before `ITEMS` and the item words
/// after it; `ATTRIBUTE`, `ATTRIBUTE_MERGE` and `STATE_META` take two
/// values.
fn split_items(args: &[Term]) -> Result<(&[Term], &[Term]), FlowError> {
    let mut at = 0;
    while let Some(name) = args.get(at) {
        let token = upper(name).ok_or_else(syntax)?;
        if token == "ITEMS" {
            let items = args.get(at.saturating_add(1)..).unwrap_or(&[]);
            if items.is_empty() {
                return Err(syntax());
            }
            return Ok((args.get(..at).unwrap_or(&[]), items));
        }
        let width = if matches!(
            token.as_str(),
            "ATTRIBUTE" | "ATTRIBUTE_MERGE" | "STATE_META"
        ) {
            2
        } else {
            1
        };
        at = at.saturating_add(1).saturating_add(width);
        if at > args.len() {
            return Err(syntax());
        }
    }
    Err(FlowError::rejected(
        "ERR flow items are required".to_owned(),
    ))
}

fn entry(key: &str, value: &Term) -> (Vec<u8>, Term) {
    (key.as_bytes().to_vec(), value.clone())
}

/// A fencing token as its integer, or the word as given for the typed
/// parser to refuse.
fn fence(value: &Term) -> Term {
    int(value).map_or_else(|| value.clone(), Term::Int)
}

/// The items as typed maps, `width` words each.
fn items(
    kind: Items,
    words: &[Term],
    (mixed, shared): (bool, bool),
) -> Result<Vec<Term>, FlowError> {
    let width = match (kind, mixed, shared) {
        (Items::Create, true, true) | (Items::Create, false, false) | (Items::Cancel, false, _) => {
            2
        }
        (Items::Create, false, true) => 1,
        (Items::Create, true, false)
        | (Items::Terminal | Items::Transition | Items::Children, false, _)
        | (Items::Cancel, true, _) => 3,
        (Items::Terminal | Items::Transition | Items::Children, true, _) => 4,
    };
    if words.len().checked_rem(width) != Some(0) {
        return Err(syntax());
    }
    Ok(words
        .chunks(width)
        .map(|chunk| Term::Map(item(kind, chunk, mixed)))
        .collect())
}

fn item(kind: Items, chunk: &[Term], mixed: bool) -> Vec<(Vec<u8>, Term)> {
    let (id, rest) = chunk
        .split_first()
        .map_or((&Term::Nil, &[][..]), |(id, rest)| (id, rest));
    let mut entries = vec![entry("id", id)];
    let rest = if mixed {
        if let Some((partition, rest)) = rest.split_first() {
            entries.push(entry("partition_key", partition));
            rest
        } else {
            rest
        }
    } else {
        rest
    };
    match (kind, rest) {
        (Items::Create, [payload]) => entries.push(entry("payload", payload)),
        (Items::Terminal, [lease, fencing]) => {
            entries.push(entry("lease_token", lease));
            entries.push(("fencing_token".as_bytes().to_vec(), fence(fencing)));
        }
        (Items::Cancel, [fencing]) => {
            entries.push(("fencing_token".as_bytes().to_vec(), fence(fencing)));
        }
        (Items::Transition, [fencing, lease]) => {
            entries.push(("fencing_token".as_bytes().to_vec(), fence(fencing)));
            if !matches!(lease, Term::Bin(dash) if dash.as_slice() == b"-") {
                entries.push(entry("lease_token", lease));
            }
        }
        (Items::Children, [flow_type, payload]) => {
            entries.push(entry("type", flow_type));
            entries.push(entry("payload", payload));
        }
        _ => {}
    }
    entries
}

/// Reads text batch command `name` into its typed map.
///
/// # Errors
/// The Elixir parser's refusal: the argument count, a malformed option or
/// item list, or the typed command's own refusal later.
pub(super) fn parse(name: &str, args: &[Term]) -> Result<Term, FlowError> {
    let layout = layout(name).ok_or_else(|| arity(name))?;
    let (positional, rest) = args
        .split_at_checked(layout.positional.len())
        .ok_or_else(|| arity(name))?;
    let (option_words, item_words) = split_items(rest)?;
    let mut options: Options = general(option_words, layout.spec)?;
    if name == "FLOW.RETRY_MANY" {
        options = nest_retry(options);
    }
    let first = positional.first().unwrap_or(&Term::Nil);
    let (mixed, item_words) = match layout.items {
        Items::Children => match item_words.split_first() {
            Some((marker, rest)) if upper_is(marker, "MIXED") => (true, rest),
            _ => (false, item_words),
        },
        _ => (upper_is(first, "MIXED"), item_words),
    };
    let shared = options.get("payload_ref").is_some();
    let items = items(layout.items, item_words, (mixed, shared))?;
    let mut entries: Vec<(Vec<u8>, Term)> = Vec::new();
    for (key, value) in layout.positional.iter().zip(positional) {
        let omitted = *key == "partition_key"
            && (upper_is(value, "MIXED")
                || (name == "FLOW.CREATE_MANY" && upper_is(value, "AUTO")));
        if !omitted {
            entries.push(entry(key, value));
        }
    }
    entries.append(&mut options.0);
    let list = if matches!(layout.items, Items::Children) {
        "children"
    } else {
        "items"
    };
    entries.push((list.as_bytes().to_vec(), Term::List(items)));
    Ok(Term::Map(entries))
}
