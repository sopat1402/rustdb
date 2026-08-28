This document will detail and explain compromises and limitations in this database

# 1) Single threaded

This database does not have MVCC in v1. It is too complex for now but maybe later I will add it.
MVCC would need transactions, locks, latches, versioning in records and a plethora of invariants I'd have
to keep track of while trying to figure the rest of the code out. However, my database can accept concurrent requests.
I put in a job queue so concurrent requests will go there but only 1 will be executed at a time.

If we consider each operation to take 0.2ms, which is pessimistic since cached pages can bring it down to about 0.01ms
and there are 1024 pages that can be cached in the storage engine.

The tenth job would have to wait 2ms for completion. The worst case round trip for a database query is 200ms i.e
the worst case (so I am considering) network latency. In case the server has a database on the same LAN, it drops to
50ms (approx). But let's assume a remote database. The total time comes to 202 ms.

Now, let's assume with 8 worker threads the time gets divided by 7 and not 8 to account for mutexes and latches.
The total time is now 200.285 ms. That's around a 0.5% improvement. Granted, this delay builds up but the key
here is that a bulk of the delay is due to network latency. So for high workloads, MVCC is definitely needed for
concurrency but until then, my database is async compatible.

# 2) No Query Language

v1 of this database does not implement SQL even though it is a relational database. Queries are made using
JSON in a socket through the network layer.

# 3) No joins, aliases or indices

Since there is no SQL parser or query planner, joins and aliases in queries are not possbible
An index here refers not to the one that maps record id to page id but rather one that the user creates and
requests for the id to be mapped to a specific field and hence creates a new tree that does occupy more memory
but if a lot of selection occurs on that specific field, lookup and scanning can be made faster with it.

Index creation will probably be the first thing to be added in v2.

# 4) Complete deserialisation of the B+ tree

Normally, a B+ tree is not fully deserialised as it would take memory for a massive database and have a significant
bootup time for deserialisation of the tree from a persistent format. This database however, deserialises the whole
tree and then serialises it at shutdown. Crash persistence is not a concern even though the tree is ram resident
because on every checkpoint the tree is serialised. When there's a crash, the WAL won't be flushed and both the
page state and tree will be brought up to speed and the tree and pages will be flushed/serialised.

2 million records can come to about 100Mb of RAM tops. That's basically to account for the overhead of internal
nodes and child vectors in addition to 12 byte leaf nodes.

# 5) ARIES like WAL with redo recovery

Complete ARIES based on that 68 page paper is too complex and may even be overkill. Don't use this as super 
critical infrastructure, I suppose, haha.
