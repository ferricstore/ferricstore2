# ADR-020: Member roles: voters and replicas

Status: accepted (2026-11-28). Solo decision at the user's request ("we have
more nodes in cluster but we don't want all to vote", then "do 1 and 2":
roles at start, then changing them while the group runs); no independent
review is claimed.

## Context

Every listed member of a group votes (ADR-019). A write commits once a
majority of voters hold it, and any voter may be elected, so each member
added for read capacity (any member answers reads, ADR-018 phase 4) also
makes every write wait for one more acknowledgement and elections slower.
The Elixir server gives each node a role, `FERRICSTORE_CLUSTER_ROLE`
`voter`, `replica` or `readonly`, the last two both a non-voting WARaft
participant, and changes them with `CLUSTER.PROMOTE <node>` and
`CLUSTER.DEMOTE <node>`; `CLUSTER.ROLE` answers the configured role, even
after a promotion. OpenRaft keeps non-voting members, learners, in a
group's membership, which its log and snapshots carry (`codec` writes
every member next to the voter sets).

## Decision

- **Two roles.** A voter votes, may lead and counts toward every quorum. A
  replica is an OpenRaft learner: it takes and applies every committed
  entry, answers reads as any follower does (ADR-018) and refuses writes
  with `REROUTE`, but never votes, never stands for election and counts
  toward no quorum. Three voters and any number of replicas commit a write
  on two acknowledgements. Elixir's `readonly` was the same membership as
  `replica`, so there is one name.
- **The setting is the role a member joins with.**
  `FERRICSTORE_CLUSTER_ROLE` (`--cluster-role`) is `voter` (the default) or
  `replica`; anything else is refused at start. Once a member is in its
  group's membership, the membership decides, and only `CLUSTER.PROMOTE`
  and `CLUSTER.DEMOTE` change it: a demoted member restarted with the
  setting still `voter` stays a replica.
- **Formation.** `Status` (peer kind 6) also answers whether the member
  joins as a voter. A new group is formed by its lowest voter, with the
  voters alone, once every member answered (ADR-019); a list without a
  voter never forms, and its members never report ready.
- **The leader adds the replicas.** A leader keeps every listed member in
  the membership: one missing from it (a replica, since the voters formed
  the group) is added as a learner. It looks every 200 ms (not on every
  change of its Raft metrics, which change with every append), so a
  replica that starts late, or a leader elected before it added one, still
  adds it. The member list stays fixed for the
  group's life (its id is the list's hash, ADR-019), so nothing removes a
  member.
- **`CLUSTER.PROMOTE <node>` and `CLUSTER.DEMOTE <node>`** make a replica a
  voter and a voter a replica, one member at a time: OpenRaft then needs no
  joint configuration, since every majority before the change meets every
  majority after it. `<node>` is the name the slot table names
  (`ferricstore2-<id>`); any other name is refused before anything is
  proposed (Elixir's validation case). Any member takes them: a member that
  does not lead asks the leader (peer kind 9: the member and the role), as
  it asks for a read index, and asks again while no leader is known or the
  one it asked no longer leads or does not answer (an election under way). Promoting a voter or demoting a replica changes
  nothing and answers `OK`. Demoting the last voter is refused, since a
  group without one can never elect a leader. A change while another is
  still being committed, before a new leader committed an entry of its own
  term, or of a replica the leader has not added yet, is tried again for up
  to five seconds, then refused with a reason to try again. The SDKs send
  these as opcodes `0x0309` and `0x030A` with `{"args": [node]}`;
  `COMMAND_EXEC` takes them as text too.
- **A demoted leader hands leadership on without losing a round.**
  OpenRaft steps down only a leader removed from the membership; one kept
  as a learner leads on, so a demoted leader would stay a leader that does
  not vote. Its shards stop taking writes (`REROUTE`) as soon as it no
  longer votes, it leads on until its demotion and every entry it appended
  are committed, and 200 ms after that it hands leadership to the most
  up-to-date voter. Handing it on at once lost the rounds its shards had
  already decided, which stops a member until it is restarted (ADR-018;
  found by `a_leader_demoted_under_writes_loses_no_round`).
- **`CLUSTER.ROLE`** (`0x030B`) answers the role this member holds in its
  group's membership as it knows it, not the setting, so it says `voter`
  after a promotion; before the member is in the membership (a group not
  formed yet, a replica not added yet) it answers the setting. A node
  without a group answers `voter`, as Elixir's single node does.
- **A promoted replica counts at once.** Promotion does not wait for the
  replica to catch up: with three voters, promoting a replica that lags
  makes four, of which three must acknowledge, and the up-to-date three
  still commit. A replica whose data was lost and is still catching up
  does not vote once promoted until it has caught up (ADR-019).

## Consequences

A cluster can add read capacity without making writes or elections slower.
Kubernetes gives every pod of one StatefulSet the same settings, so there a
pod's role is set from its ordinal by its start command, or changed with
`CLUSTER.DEMOTE` once the group runs. Not decided here: adding members not
in the list and removing members, which change the list and so the group's
id, and moving leadership on request (Elixir's `CLUSTER.FAILOVER`).
