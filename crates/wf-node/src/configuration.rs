//! Cluster configuration (ADR-019): which members a node's group has and
//! which one it is, from its settings.
//!
//! `FERRICSTORE_DISCOVERY` picks the source:
//!
//! - `none` (the default): a single node.
//! - `static`: `FERRICSTORE_CLUSTER_NODES` lists every member as
//!   `host[:port[:peer_port]]`, comma separated, the same list on every
//!   node; a member's id is its position (from 1). A node is the member
//!   `FERRICSTORE_NODE_ID` names, or the one whose host is its advertised
//!   host.
//! - `dns`: members are the pods of a Kubernetes `StatefulSet` behind the
//!   headless service `FERRICSTORE_DNS_NAME`: `FERRICSTORE_CLUSTER_SIZE`
//!   pods named `<base>-<ordinal>`, reached as
//!   `<base>-<ordinal>.<FERRICSTORE_DNS_NAME>`; a member's id is its ordinal
//!   plus 1, and a node's base and ordinal come from its own pod name.
//!
//! Names stay names: members resolve them on each connection, so a pod that
//! restarts with a new address is found again. Ports default to the node's
//! own client and peer ports.

use std::collections::BTreeMap;

/// Where one member is reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberAddress {
    /// Host name or address.
    pub host: String,
    /// Client (native protocol) port.
    pub port: u16,
    /// Raft port, for the other members.
    pub peer_port: u16,
}

impl MemberAddress {
    /// The Raft address, `host:peer_port`.
    #[must_use]
    pub fn peer(&self) -> String {
        format!("{}:{}", self.host, self.peer_port)
    }
}

/// A node's place in its group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    /// This node's member id.
    pub id: u64,
    /// Every member, this one included.
    pub members: BTreeMap<u64, MemberAddress>,
}

/// What this node knows of itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local {
    /// The name other members and clients reach it by (its pod name in
    /// Kubernetes).
    pub host: String,
    /// Its client port.
    pub port: u16,
    /// Its Raft port.
    pub peer_port: u16,
}

/// Reads a setting by name.
pub type Settings<'a> = &'a dyn Fn(&str) -> Option<String>;

fn required(settings: Settings<'_>, name: &str) -> Result<String, String> {
    settings(name)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required for this discovery"))
}

fn number<T: std::str::FromStr>(name: &str, value: &str) -> Result<T, String> {
    value
        .trim()
        .parse()
        .map_err(|_| format!("{name} needs a number, not {value:?}"))
}

/// The cluster `settings` describe; `None` for a single node.
///
/// # Errors
/// A missing or malformed setting, and a node that is not among its
/// members.
pub fn cluster(settings: Settings<'_>, local: &Local) -> Result<Option<Cluster>, String> {
    let discovery = settings("FERRICSTORE_DISCOVERY").unwrap_or_default();
    match discovery.trim() {
        "" | "none" => Ok(None),
        "static" => static_cluster(settings, local).map(Some),
        "dns" => dns_cluster(settings, local).map(Some),
        other => Err(format!(
            "FERRICSTORE_DISCOVERY {other:?} is not one of none, static, dns"
        )),
    }
}

/// One `host[:port[:peer_port]]` entry.
fn entry(text: &str, local: &Local) -> Result<MemberAddress, String> {
    let name = "FERRICSTORE_CLUSTER_NODES";
    let mut parts = text.trim().split(':');
    let host = parts.next().filter(|host| !host.is_empty());
    let host = host.ok_or_else(|| format!("{name} has an empty entry"))?;
    let port = parts.next().map(|port| number(name, port)).transpose()?;
    let peer_port = parts.next().map(|port| number(name, port)).transpose()?;
    if parts.next().is_some() {
        return Err(format!("{name} entry {text:?} has too many parts"));
    }
    Ok(MemberAddress {
        host: host.to_owned(),
        port: port.unwrap_or(local.port),
        peer_port: peer_port.unwrap_or(local.peer_port),
    })
}

fn static_cluster(settings: Settings<'_>, local: &Local) -> Result<Cluster, String> {
    let nodes = required(settings, "FERRICSTORE_CLUSTER_NODES")?;
    let mut members = BTreeMap::new();
    for (position, text) in nodes.split(',').enumerate() {
        let id = u64::try_from(position).map_err(|_| "too many members".to_owned())?;
        members.insert(id.saturating_add(1), entry(text, local)?);
    }
    let id = match settings("FERRICSTORE_NODE_ID") {
        Some(id) => number("FERRICSTORE_NODE_ID", &id)?,
        None => members
            .iter()
            .find(|(_, member)| member.host == local.host)
            .map(|(id, _)| *id)
            .ok_or_else(|| {
                format!(
                    "this node ({}) is not in FERRICSTORE_CLUSTER_NODES; set FERRICSTORE_ADVERTISE_HOST or FERRICSTORE_NODE_ID",
                    local.host
                )
            })?,
    };
    if !members.contains_key(&id) {
        return Err(format!("member {id} is not in FERRICSTORE_CLUSTER_NODES"));
    }
    Ok(Cluster { id, members })
}

fn dns_cluster(settings: Settings<'_>, local: &Local) -> Result<Cluster, String> {
    let service = required(settings, "FERRICSTORE_DNS_NAME")?;
    let size: u64 = number(
        "FERRICSTORE_CLUSTER_SIZE",
        &required(settings, "FERRICSTORE_CLUSTER_SIZE")?,
    )?;
    // The pod name, without any domain: `<base>-<ordinal>`.
    let pod = local.host.split('.').next().unwrap_or_default();
    let (base, ordinal) = pod
        .rsplit_once('-')
        .and_then(|(base, ordinal)| Some((base, ordinal.parse::<u64>().ok()?)))
        .ok_or_else(|| {
            format!("pod name {pod:?} does not end in -<ordinal>, as a StatefulSet's do")
        })?;
    if size == 0 || ordinal >= size {
        return Err(format!(
            "pod ordinal {ordinal} is outside FERRICSTORE_CLUSTER_SIZE {size}"
        ));
    }
    let members = (0..size)
        .map(|index| {
            let member = MemberAddress {
                host: format!("{base}-{index}.{}", service.trim()),
                port: local.port,
                peer_port: local.peer_port,
            };
            (index.saturating_add(1), member)
        })
        .collect();
    Ok(Cluster {
        id: ordinal.saturating_add(1),
        members,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Cluster, Local, MemberAddress, cluster};

    fn local(host: &str) -> Local {
        Local {
            host: host.to_owned(),
            port: 6388,
            peer_port: 7388,
        }
    }

    fn settings(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn member(host: &str, port: u16, peer_port: u16) -> MemberAddress {
        MemberAddress {
            host: host.to_owned(),
            port,
            peer_port,
        }
    }

    #[test]
    fn without_discovery_a_node_is_single() {
        assert_eq!(cluster(&settings(&[]), &local("a")), Ok(None));
        let none = settings(&[("FERRICSTORE_DISCOVERY", "none")]);
        assert_eq!(cluster(&none, &local("a")), Ok(None));
        let unknown = settings(&[("FERRICSTORE_DISCOVERY", "gossip")]);
        assert!(cluster(&unknown, &local("a")).is_err());
    }

    #[test]
    fn a_static_list_numbers_members_by_position() {
        let list = settings(&[
            ("FERRICSTORE_DISCOVERY", "static"),
            ("FERRICSTORE_CLUSTER_NODES", "a, b:6400, c:6401:7401"),
        ]);
        let expected = Cluster {
            id: 2,
            members: BTreeMap::from([
                (1, member("a", 6388, 7388)),
                (2, member("b", 6400, 7388)),
                (3, member("c", 6401, 7401)),
            ]),
        };
        assert_eq!(cluster(&list, &local("b")), Ok(Some(expected)));
        assert!(cluster(&list, &local("z")).is_err(), "not a member");
        let named = settings(&[
            ("FERRICSTORE_DISCOVERY", "static"),
            ("FERRICSTORE_CLUSTER_NODES", "a,b"),
            ("FERRICSTORE_NODE_ID", "1"),
        ]);
        assert_eq!(
            cluster(&named, &local("z")).map(|found| found.map(|cluster| cluster.id)),
            Ok(Some(1))
        );
    }

    #[test]
    fn a_statefulset_s_pods_are_its_members() {
        let pods = settings(&[
            ("FERRICSTORE_DISCOVERY", "dns"),
            ("FERRICSTORE_DNS_NAME", "fs-headless.prod.svc.cluster.local"),
            ("FERRICSTORE_CLUSTER_SIZE", "3"),
        ]);
        let found = cluster(&pods, &local("fs-2")).ok().flatten();
        assert_eq!(found.as_ref().map(|cluster| cluster.id), Some(3));
        let hosts: Vec<String> = found
            .map(|cluster| cluster.members.values().map(MemberAddress::peer).collect())
            .unwrap_or_default();
        assert_eq!(
            hosts,
            vec![
                "fs-0.fs-headless.prod.svc.cluster.local:7388",
                "fs-1.fs-headless.prod.svc.cluster.local:7388",
                "fs-2.fs-headless.prod.svc.cluster.local:7388",
            ]
        );
        assert!(cluster(&pods, &local("fs-3")).is_err(), "beyond the size");
        assert!(cluster(&pods, &local("standalone")).is_err(), "no ordinal");
        let unsized_pods = settings(&[
            ("FERRICSTORE_DISCOVERY", "dns"),
            ("FERRICSTORE_DNS_NAME", "fs-headless"),
        ]);
        assert!(cluster(&unsized_pods, &local("fs-0")).is_err());
    }
}
