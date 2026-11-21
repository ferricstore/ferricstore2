//! Starting a governed claim: the explicit options checked as
//! `optional_governance_limit/8` checks them, or the type's running-limit
//! rules, then the claim queued for the schedule thread.

use std::sync::Arc;

use wf_application::commands::queue::{ClaimFlows, ClaimedFlow, Reply, ServiceError};

use super::super::QueueNode;
use super::super::claim;
use super::super::router::Router;
use super::{Governed, run, running};
use crate::supervision::scheduler::Job;

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

impl QueueNode {
    /// Runs a claim naming a limit on the schedule thread, after checking
    /// its options as `optional_governance_limit/8` does.
    pub(in crate::composition) fn governed_claim(
        &self,
        router: &Arc<Router>,
        request: ClaimFlows,
        reply: Reply<Vec<ClaimedFlow>>,
    ) {
        let shards = router.shards.len();
        if !request.governance.given {
            return match running::governed(&router.running, &request, shards) {
                Some(governed) => self.governed_job(router, request, governed, reply),
                None => claim::start(router, request, None, reply),
            };
        }
        let governed = match (&request.governance.scope, request.governance.shard) {
            (Some(scope), Some(shard))
                if usize::try_from(shard).is_ok_and(|shard| shard < shards) =>
            {
                Governed {
                    scope: scope.clone(),
                    shard_id: u64::try_from(shard).unwrap_or(0),
                    strict: false,
                    auto: None,
                }
            }
            (Some(_), Some(_)) => {
                return reply(Err(ServiceError::rejected(
                    "ERR flow governance_shard_id is outside the instance shard range".to_owned(),
                )));
            }
            _ => {
                return reply(Err(ServiceError::rejected(
                    "ERR flow governance_limit_scope and governance_shard_id must be provided together"
                        .to_owned(),
                )));
            }
        };
        self.governed_job(router, request, governed, reply);
    }

    fn governed_job(
        &self,
        router: &Arc<Router>,
        request: ClaimFlows,
        governed: Governed,
        reply: Reply<Vec<ClaimedFlow>>,
    ) {
        let cache = Arc::clone(&self.credit_cache);
        let router = Arc::downgrade(router);
        let job: Job = Box::new(move || {
            let result = router
                .upgrade()
                .map_or(Err(ServiceError::Unavailable), |router| {
                    run(&router, request, &governed, &cache, wall_ms())
                });
            reply(result);
        });
        let Some(jobs) = &self.schedule_jobs else {
            return job();
        };
        if let Err(returned) = jobs.send(job) {
            (returned.0)();
        }
    }
}
