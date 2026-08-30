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

# 2) JSON Query Interface

v1 of this database does not implement SQL even though it is a relational database. Queries are made using
JSON in a socket through the network layer. I wrote the JSON parser. It doesn't make a tree. It converts it 
directly to the structs I need. I'm not trying to make a library.

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

My WAL is not perfect either. This is more of an idgaf thing. I spent hours writing a second WAL for tables so
that I didn't have to change the WAL for pages but now there's two WALs I need to handle, 2 sources of truth.
I'll take the L and admit that there's a crash window of a few microseconds (under 10) between the first fsync
and the second fsync. when it goes there, changes can be lost.

Since the failure can only come between wal writes and the table wal state only changes on insert and delete : 
- when scan finds a record absent, it deletes it from the table's record vector. Do I need to make this durable too?
    no. because whenever there's a record absent it will delete it anyways.
- On delete crash, the record id disappears from the table's record array but is still in the tree and pages. 
    It does not matter, since the next_record_id for the index does not change. So that record id will then be a
    ghost one in the pages with nothing that can reference it. Since the odds of this happening are very low, such
    ghost records are not concerns for space wastage.

Assuming that there's a crash in that window and the window is say, 20 microseconds  i.e the time to get a free
page after which the sync is done once the page is acquired, as calculated before if we assume an extra 0.1ms for
other operations, the total time can be taken as 203ms with the network latency factored in. The odds of a database
crashing then are 20μs/203ms which is about 1/10,000 or 0.01%. So, there's a 0.01% chance of a crash occuring
in that window, in which case a user's inserted value won't be in the database. If a database crashes, I suppose
they ought to recheck then. The above assumption holds if crashes were evenly distributed, which they aren't. You get
the idea, it's unlikely.

In case that paragraph is confusing, good. Ignore it. I was just trying to say, a crash in that window is unlikely but
in databases we have to consider possibility and not probability. The durability is faulty but since this architecture
was mostly me making it up as I go along (as my dev notes will prove), I'm still pretty proud of it.
