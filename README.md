# rustdb

Rustdb is a database built from scratch in Rust by Sohum Pathak. The project has 4349 lines of rust code as of now.
It is single threaded but accepts concurrent requests using Tokio's mpsc.

>[!NOTE]
>This README does not go too deep into implementation details.
>For a detailed view of not only the features but also how I decided on architecture and refactors,
>READ THE dev_notes.md. However vulgar they may seem, I documented my thought process there.
>Also read Compromises.md to see by detailed explanation of the tradeoffs in this project.

## Features

- Slotted pages
- Variable sized records with compaction
- A B+ tree for indexing
- A buffer pool for page access via cache or disk
- A page level ARIES like WAL
- A table layer
- A table level ARIES like WAL coupled to the first
- A custom JSON parser
- Accepts concurrent requests but only executes 1 at a time
- A custom query interface using JSON despite relational data

## Dependencies

1) Tokio : Async TCP connection handling
2) Clap : Command line argument parsing

## Query API

The order does not have to be fixed in this format as long as the required parameters are included in the right way.

Each pair is to be specified like "column":"<name>","value":"<value>". All values are entered as strings. My JSON
parser handles the type conversion.

When making a table, along with the table name, a schema is to be provided :
"schema":{"column":"name","type":"VARCHAR","column":"age","type":"u32"}

The data types supported are : UINT32, INT32, FLOAT32, VARCHAR. Varchar represents any string or text or characters.
If you have a boolean, either store it as an number (0 or 1) or as a varchar of true and false.

the task must be given too, as well as the table name. Examples of queries can be found in test.rs.

eg : "task":"insert","table_name":"users"

for operations like dropping a database and creating a new one, the db_name must be passed too, similar to how
table name was passed above.

for operations like select, conditions must be passed.

"conditions":{"column":"age","operator":"ge","value":"25"} => translation : age >= 25

In case you want all rows to be affected, do "conditions":{}

### Operator types (case sensitive) and their meaning :

* e     :   equal
* ne    :   not equal
* g     :   greater
* l     :   lesser
* ge    :   greater than or equal to
* le    :   less that or equal to

Update needs updates. This is just "updates":{"column":"age","value":"21","salary":"0"} basically just the
columns that changed.

### Task Types (case sensitive) and their requirements : 

* insert        :   row, table name
* update        :   conditions, updates, table name
* delete        :   conditions, table name
* select        :   conditions, columns needed, table name
* create_table  :   table name
* delete_table  :   table name
* create_db     :   database name
* drop_db       :   database name

## Page Layout

The database uses fixed-size 8192-byte pages.

Each page contains a 96-byte header followed by the page's slot directory and record data. The header contains:

* Magic number
* Page ID
* Page type
* Page flags
* LSN
* Checksum
* Item count
* Lower and upper boundaries for the slotted page
* Reserved bytes

All page and on-disk metadata is serialized in little-endian format.

The page flags distinguish clean, dirty, and corrupted pages. A corrupted page cannot be flushed or operated on. Checksums are calculated when pages are loaded and recalculated during flushing to detect corruption.

The database originally used fixed-size records, but this was changed to variable-sized records. Records are stored using a slotted-page layout, with free space and deleted slots being tracked so that space can be reused. Compaction is performed when contiguous free space is insufficient for a new record.

## Buffer Pool

The storage engine contains a buffer pool backed by an LRU cache.

The buffer pool abstracts whether a page is currently in memory or needs to be loaded from disk. Pages are cached after being retrieved, and separate mutable and immutable access paths are provided.

The cache maintains a free list and an internal representation of evicted nodes. Pages can be flushed individually or all at once without destroying the cache itself.

Rather than serializing the buffer pool, page metadata is persisted separately. This allows the database to determine which pages exist and how much free space they contain without having to keep the entire buffer pool persistent.

## B+ Tree

The index layer uses a B+ tree for mapping record IDs to their corresponding pages.

The tree maintains the usual node bookkeeping and also links leaf nodes together, allowing sequential traversal through the leaves.

The B+ tree can be serialized and deserialized to persistent storage. It is reconstructed into memory when the database boots and serialized during checkpointing and shutdown.

The current index is an internal storage-engine index rather than a user-created secondary index. User-created indexes are not currently supported.

## Index Layer

The index owns both the B+ tree and the buffer pool.

For a record operation, it determines the relevant page, obtains the page through the buffer pool, and performs the required operation. New record IDs are allocated by the index.

When a suitable page cannot be found in the cache, persistent page metadata is consulted before allocating a new page.

The index is also responsible for the page-level Write-Ahead Log.

## Table Layer

A table is represented by its own persistent file containing table metadata, its schema, and the record IDs belonging to the table.

The table header currently contains:

* Magic number (`u32`)
* Checksum (`u32`)
* Number of records (`u32`)
* Number of columns (`u16`)
* Next SI number (`u32`)
* LSN (`u64`)

This produces a 26-byte header. Schema information follows the metadata, with column names prefixed by their lengths and data types represented by a `u16` enum.
Each inserted record receives an SI number. The SI is maintained by the table layer rather than being part of the user's declared schema.

The table layer performs scanning, selection, insertion, deletion, and update operations. A scan currently has complexity of approximately `O(R * (N + C))`, where `R` is the number of records, `N` is the number of columns, and `C` is the number of conditions. Inter-column comparisons are not currently supported.

When selecting columns, a hash set is used to avoid repeatedly scanning the requested-column vector.

## Write-Ahead Logging

The storage engine uses an ARIES-like Write-Ahead Log with redo-style recovery rather than implementing complete ARIES.

The WAL stores the information required to reconstruct page state after a crash. Log entries contain an LSN, operation type, page ID, record ID, and record data where applicable. The WAL also maintains metadata containing the latest LSN and the number/size of log entries.

Pages contain their latest LSN in their headers. During reconstruction, the WAL can therefore be examined for operations that occurred after the page's persisted state.

Checkpointing brings pages and the B+ tree up to date and clears the WAL. Recovery is performed during database bootup when uncheckpointed WAL data remains.

The WAL is deliberately simpler than a complete ARIES implementation. Full transaction management, locking, latching, and MVCC are outside the scope of the current version.

## Table WAL

The table layer also maintains WAL state because the table's in-memory record-ID state is separate from the page-level state maintained by the index.

The table WAL records changes to table membership, particularly insertion and deletion. The table and index WALs are synchronized through checkpointing.

This introduces two sources of truth and therefore a small crash window between the corresponding WAL and data operations. The current implementation accepts this limitation rather than attempting to implement a more complicated unified recovery protocol.

If a crash occurs during this window, the most significant possible consequence is an inserted record not being present after recovery. The client is expected to verify the result of an operation if the connection is lost during a crash.

## Database Layer

The `Database` layer connects the table/storage layer to the external interface.

Jobs are submitted through a Tokio MPSC channel. Each job carries a query and a one-shot response channel, allowing the caller that submitted the query to receive its result without requiring a separate response-routing structure.

The database processes queued jobs in FIFO order and executes them one at a time.

The database layer is responsible for:

* Database creation and bootup
* Table creation and deletion
* CRUD execution
* Query parsing
* Schema lookup
* WAL recovery during bootup
* Database shutdown
* Database deletion
* Returning query results and errors

Dropping the currently running database causes the database run loop to terminate through a shutdown/kill path.

## JSON Query Interface

The database does not implement SQL.

Instead, queries are sent as JSON strings over the network. The JSON parser was written specifically for this database and does not construct a general-purpose JSON tree. It lexes the input into tokens and directly constructs the structures required by the database query layer.
This keeps the server-side parser focused on the database's actual query format rather than implementing a general JSON library.

The parser supports operations including:

* Insert
* Select
* Update
* Delete
* Create table
* Delete table
* Create database
* Drop database
* Shutdown
* Get schema

Values are converted to the data types specified by the table schema before being passed into the storage layer.

## Network Layer

The network layer is implemented using Tokio TCP.

Requests use a small binary frame around the JSON payload:

```text
+--------+----------------+-------------------+
| Flag   | Length (u32)   | JSON payload      |
| 1 byte | Big endian     | variable length   |
+--------+----------------+-------------------+
```

Network integers are encoded in big-endian byte order.

The server uses `read_exact` to consume each component of a frame before processing it. Responses use a similar framing scheme consisting of a status byte, a four-byte big-endian body length, and the response body.

The network layer accepts multiple concurrent TCP connections. These connections can submit jobs concurrently, but the database itself executes the jobs sequentially through its queue.

Command-line configuration is provided using `clap`, including the database name, port, and whether a new database should be created.

## Query Results

The server returns structured JSON responses for successful queries and errors.

Successful responses can represent:

* A generic success
* A count of affected records
* A collection of returned rows

Rows contain column names and their corresponding values.

If no columns are specified for a `SELECT`, the table schema is used to construct the complete column list automatically.

## Concurrency Model

The database is intentionally single-threaded.

It can accept concurrent network requests, but requests are placed into a FIFO job queue and only one database operation is executed at a time. Tokio is used for asynchronous networking and job communication rather than for parallel execution of database operations.

The reason for this design is primarily complexity. Implementing MVCC would require transactions, locking/latching, record versioning, and many additional invariants. Those mechanisms are deliberately left out of version 1.

The architecture is therefore **asynchronous-compatible rather than concurrently executing**.

## Current Limitations

This project intentionally has several limitations.

### No SQL

There is no SQL parser or query planner. The network interface accepts JSON queries instead.

### No MVCC

Only one database operation executes at a time. Transactions, MVCC, locks, and latches are not implemented.

### No User-Created Indexes

The storage engine's B+ tree is used internally, but secondary indexes created by users are not currently supported.

As a result, selection relies on scanning. A future version could add additional B+ trees for frequently queried fields.

### No Joins or Aliases

Because there is no SQL parser or query planner, joins and aliases are not currently supported.

### Entire B+ Tree in Memory

The complete B+ tree is deserialized into memory during bootup and serialized during checkpointing/shutdown.

This is intentionally simpler than implementing a fully disk-resident B+ tree. The current design accepts the memory and startup-time cost.

### Simplified WAL

The WAL is not a complete ARIES implementation. There are two WAL states, one associated with table state and one with page/index state, which introduces a small crash window.

The project explicitly treats this as a known limitation rather than hiding it behind a claim of perfect durability.

## Design Philosophy

This database is primarily an implementation and learning project.

The goal is not to reproduce PostgreSQL or another production database feature-for-feature. The goal is to build the pieces of a database system from the storage layer upward and understand how they interact.

The project therefore deliberately contains mechanisms that would normally be replaced or substantially expanded in a production system:

* A custom JSON parser instead of a general JSON library
* A custom page format
* A custom slotted-page implementation
* A custom LRU buffer pool
* A custom B+ tree
* Custom serialization and deserialization
* Custom CRC32 checksum handling
* A custom WAL and recovery system
* A single-threaded execution model
* A deliberately small network protocol

Some of these choices are not optimal for a production database. They are intentional because the purpose of the project is to understand the machinery underneath the abstractions.

## Development Notes

The implementation evolved incrementally from the page layer upward.

The storage engine was built first, followed by the B+ tree and index layer, WAL and recovery, the table layer, query parser, database/job layer, and finally the TCP network layer. Several implementation decisions changed substantially during development, including the transition from fixed-size to variable-sized records and the addition of table-level WAL state.
The development process also deliberately includes testing of failure and recovery paths rather than only testing successful CRUD operations. The WAL was tested through crash/recovery scenarios, and the integration tests exercise the database through the actual TCP interface.

The project has accumulated a number of bugs during development, including serialization offsets, page indexing, free-space accounting, LRU bookkeeping, recovery idempotency, SI-number handling, and table deserialization bounds checks. These were found through progressively deeper integration tests rather than being abstracted away from the storage implementation.

## Testing

The database is tested through end-to-end TCP requests rather than only calling individual storage methods.

The integration tests currently exercise operations such as:

* Creating tables
* Inserting records
* Selecting specific columns
* Selecting all columns
* Updating records
* Deleting records
* Creating databases
* Dropping databases
* Shutting down the server

The tests also verify details such as SI numbers, typed values, record counts, and the state of rows after updates and deletes.

## Future Work

The architecture leaves several obvious areas for future versions.

The most significant planned additions are:

* User-created secondary indexes
* SQL parsing
* Query planning
* Joins
* Aliases
* Transactions
* MVCC
* More sophisticated concurrency
* A more complete recovery system
* Further optimization of scans
* Potential improvements to variable-sized record management

Secondary indexes are likely to provide the largest immediate performance improvement because the current scan implementation evaluates conditions across records directly.

## Known Trade-offs

This project intentionally makes several trade-offs in favor of implementation simplicity.

The most important is that **correctness of the architecture is prioritized over production-grade completeness**.

The database does not claim to provide the durability guarantees of a mature database system. In particular, the small interval between the table WAL synchronization and the page/index operation synchronization is a known limitation.

Likewise, keeping the entire B+ tree in memory makes the implementation considerably simpler but places a practical limit on how large a database can comfortably become.

These limitations are documented intentionally. The project is meant to make the underlying database mechanisms visible rather than hide them behind a production-oriented abstraction.

## Status

The current implementation has:

* Persistent pages
* Slotted pages
* Variable-sized records
* Page checksums
* LRU buffer pooling
* Page metadata
* A B+ tree index
* Persistent B+ tree serialization
* WAL logging
* Checkpointing
* Crash recovery
* Table persistence
* Table WAL
* CRUD operations
* A custom JSON query parser
* A database/job execution layer
* Tokio-based asynchronous TCP networking
* Framed network requests and responses
* Command-line configuration
* End-to-end integration tests

The core database path is now functional from the network request all the way down to persistent storage and back.

## License

MIT License. (See LICENSE)

## Disclaimer

This is a learning/implementation project and is **not intended to be used as a production database**.

The documented limitations are intentional, particularly around concurrency, transaction semantics, indexing, query planning, and crash durability.
