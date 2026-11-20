//! Routes Flow commands beyond the queue fast path to their shard (ADR-014).

use wf_application::commands::flow::{FlowCommand, FlowService, Term};
use wf_application::commands::queue::{Partition, Reply, ServiceError};

use crate::supervision::shard::Request;

use super::QueueNode;

impl FlowService for QueueNode {
    fn execute(&self, command: FlowCommand, reply: Reply<Term>) {
        let Some(router) = self.router.as_ref() else {
            return reply(Err(ServiceError::Unavailable));
        };
        let (id, explicit) = command.target();
        let partition = Partition::for_new_flow(id, explicit);
        let index = router.route(&partition);
        router.send(
            index,
            Request::Flow {
                command: Box::new(command),
                partition,
                done: reply,
            },
        );
    }
}
