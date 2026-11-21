//! Governance records on the shard their name routes to (ADR-014 step 15):
//! a command on one record is decided from its stored form, which is
//! stored or removed and logged as is; a listing reads one kind.

use wf_application::commands::flow::governance::{Change, Command, Kind, Listed};
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::workflow::Outcome;

use super::super::command::Logged;
use super::ShardState;
use super::flow::Executed;

impl ShardState {
    /// Decides a command on one governance record.
    pub(crate) fn governance(&mut self, wall: u64, command: &Command) -> Executed {
        let Some((kind, name)) = command.record() else {
            return (Err(ServiceError::Unavailable), None);
        };
        let key = (kind, name.to_vec());
        let (reply, change) = command.decide(self.governance.get(&key).map(Vec::as_slice), wall);
        let Some(change) = change else {
            return (reply, None);
        };
        let value = match change {
            Change::Put(bytes) => Some(bytes),
            Change::Remove => None,
        };
        let logged = Logged::Governance {
            partition: Partition::parse(&key.1),
            kind: kind.tag(),
            name: key.1.clone(),
            value: value.clone(),
        };
        self.store_governance(key, value);
        (reply, Some(logged))
    }

    /// This shard's part of a listing.
    pub(crate) fn governance_select(&self, command: &Command) -> Listed {
        let Some(kind) = command.listed() else {
            return Vec::new();
        };
        command.select(
            self.governance
                .range((kind, Vec::new())..)
                .take_while(|((held, _), _)| *held == kind)
                .map(|(_, value)| value.as_slice()),
        )
    }

    fn store_governance(&mut self, key: (Kind, Vec<u8>), value: Option<Vec<u8>>) {
        match value {
            Some(bytes) => {
                self.governance.insert(key, bytes);
            }
            None => {
                self.governance.remove(&key);
            }
        }
    }

    /// Restores a logged or imaged governance record.
    pub(crate) fn restore_governance(
        &mut self,
        tag: u8,
        name: &[u8],
        value: Option<Vec<u8>>,
    ) -> Result<Outcome, ServiceError> {
        let kind = Kind::from_tag(tag).ok_or(ServiceError::Unavailable)?;
        self.store_governance((kind, name.to_vec()), value);
        Ok(Outcome::Changed)
    }

    /// Every governance record, for a snapshot image: tag, name, form.
    pub(crate) fn governance_records(&self) -> impl Iterator<Item = (u8, &[u8], &[u8])> {
        self.governance
            .iter()
            .map(|((kind, name), value)| (kind.tag(), name.as_slice(), value.as_slice()))
    }
}
