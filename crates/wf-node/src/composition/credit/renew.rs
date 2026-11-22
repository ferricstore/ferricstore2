//! Renewing a governed Flow's credit lease before its lease is extended
//! (audit G54), as Elixir's `maybe_renew_governance_limit/2`.

use std::sync::{Arc, OnceLock, Weak};

use wf_application::commands::flow::governance::Command as GovernanceCommand;
use wf_application::commands::flow::governance::limit::{self, Configuration, Fields, Op};
use wf_application::commands::flow::mutate::Change;
use wf_application::commands::flow::{FlowCommand, Term};
use wf_application::commands::queue::{Partition, ServiceError};

use crate::supervision::shard::{RenewFirst, Request};

use super::super::router::Router;

/// Renews a governed Flow's credit lease on its limit's shard (audit G54),
/// then sends the extension back to the Flow's shard marked renewed; a
/// refused renewal (the lease lapsed) answers the extension.
pub(crate) fn renew_first(router: Arc<OnceLock<Weak<Router>>>) -> RenewFirst {
    Arc::new(
        move |(credit, ttl_ms, now_ms), mut command, partition, done| {
            let Some(router) = router.get().and_then(Weak::upgrade) else {
                return done(Err(ServiceError::Unavailable));
            };
            if let FlowCommand::Mutate(mutation) = command.as_mut()
                && let Change::ExtendLease { renewed, .. } = &mut mutation.change
            {
                *renewed = true;
            }
            let fields = Fields {
                ttl_ms: Some(ttl_ms),
                now_ms: Some(now_ms),
                ..Fields::default()
            };
            let renewal = limit::Command::internal(
                Op::Renew,
                credit.scope.clone(),
                credit.shard_id,
                fields,
                Configuration::default(),
            );
            let limit_partition = Partition::parse(&credit.scope);
            let resend = Arc::downgrade(&router);
            router.send(
                router.route(&limit_partition),
                Request::Flow {
                    command: Box::new(FlowCommand::Governance(Box::new(GovernanceCommand::Limit(
                        renewal,
                    )))),
                    partition: limit_partition,
                    done: Box::new(move |renewed: Result<Term, ServiceError>| {
                        match (renewed, resend.upgrade()) {
                            (Ok(_), Some(router)) => router.send(
                                router.route(&partition),
                                Request::Flow {
                                    command,
                                    partition,
                                    done,
                                },
                            ),
                            (Ok(_), None) => done(Err(ServiceError::Unavailable)),
                            (Err(error), _) => done(Err(error)),
                        }
                    }),
                },
            );
        },
    )
}
