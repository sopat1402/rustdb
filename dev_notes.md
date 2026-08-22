# Page design

Paging has various flags, bring to 96 bytes. currently just using page id
need to use dirty flag now that flush is done

There's a page flags enum with clean, dirty and corrupted. it should be checked on page access.
The things in the page header are : 
pub struct PageHeader{
    pub magic       :   u32,    //To ensure it is of this table
    pub page_id     :   u64,    //Page ID
    pub page_type   :   PageType,    //Meta data, data, free, etc
    pub flags       :   u16,    //corrupted, dirty, clean
    pub lsn         :   u64,    //LSN for the Write Ahead Logs
    pub checksum    :   u32,    //To deal with corruption
    pub item_count  :   u16,    //For slotted pages
    pub lower       :   u16,    //End of slot list -> it grows downwards
    pub upper       :   u16,    //Upper point of the records -> they grow upwards
    pub reserved    :   [u8;62],
}
This makes 96 bytes. The reserved bytes are just for metadata and checksums in case another module
needs them to store metadata.

Page flag is set to clean prior to disk write. Flush and operations refused if flag is corrupted.
If the page flag is clean, flush will just return Ok(()). Operations change page flag to dirty.
Corrupted data errors change page flag to corrupted. The errors from deserialise in the impl of PageHeader return
corrupted errors and mismatch errors for magic and the page type and page flags. That's the bouncer ig so corrupted 
flags come into picture for the actual bytes in the page.


made an error module with a big enum in it and use that everywhere so that it has all types.

- CHANGE RECORD ID FROM U16 TO U32 => This is a later todo. Requires quite the refactor, especially with byte serialisation
and deserialisation.

- For now, the database can have a maximum of 65,536 records.

lru cache uses a free list that owns DLLNodes, which own the page objects. There's a trash vector too.
it stores indices of deleted nodes in the old vector.
am i checking if the node id is in trash during search? no. do i need to? idk. prolly not on just 1 thread. I actually don't at all.
it's an invariant lol. dll nodes store indices in the nodes vector. addition of nodes first checks if trash is empty.

popping a node from the dll flushes it but this should be done only if it is dirty. haven't added dirty
flag usage yet even though page header includes it.

buffer pool hides APIs for page stuff so that other components need only worry about getting the page and
not whether it is from RAM or ROM. it caches it right after. there's get mut and regular get. that's because
of rust's ownership mechanism, which has been a bit of a nuisance so far for a single threaded project.

Added flush_all and flush_page along and renamed the old ones to evict. Now the cache can be flushed without being
deconstructed.

OMGGG I just realised I totally forgot about Box<T>. Eh maybe it was for the best. I did make a pretty cool arena
and trash solution to sidestep pointers altogether.

db file modified only on flush and allocate so far. page deletion not added. otherwise, it's a buffer of 
[u8;PAGE_SIZE] that is modified. it is all little endian bytes. slots and the header have a serialize method
in their impl too to abstract all the byte management away. Just make an array using to_le_bytes() and then
do copy_from_slice at the needed offset in the buffer from the bytes array.

- each record is fixes size : 128 bytes at max size. That doesn't mean each user entry must be 128 bytes.
just that putting 128 bytes solves external fragmentation but usually guarantees internal fragmentation.
each slot is 6 bytes. record id, offset and size. this is where the actual user entered data size is stored.

# B+ tree

AAAAH. between m/2 and m values per node. each node has next too, forms a linked list. this prevents going back
and speeds up traversal. Quite a lot of book-keeping methods, public ones are insertion and deletion, search, etc.
TBH this is just a nasty to implement data structure. Nothing noteworthy here except the serialising and deserialising methods.
The amount of bookkeeping here nearly drove me mad, even though I referred to a literal PAPER on B+ trees. If you're doing this yourself,
spare yourself the trouble and just copy my B+ tree methods except for the byte handling ones to construct and deconstruct the B+ tree from bytes.

serialising and deserialising the tree nearly drove me mad too. I had to study using some different APIs for it and
it was quite hard. had to even ask a guy for reading material and I'm not even his acquaintance I just walked up!!

# Index

Index owns a B+ tree as well as a buffer pool. Checks if record exist, tries to get the page and then carries out
the necessary operation. Buffer pool and B tree capacities kept at 8 and 4 for dev tests.

For writing a record, a next record id is maintained in the index. Size is found from the provided buffer. For finding the page_id, the buffer pool's
cached pages are scanned and the first one that isn't in the trash vector and has free space i.e lower-upper>=134 as each record is 128 bytes and
each slot is 6 bytes and then it returns the page_id. this does not affect the LRU cache order. Then a simple write is made with that page_id after getting
it from the buffer pool.

However, a new page is allocated when the find_free_page search fails. So, the find_free_page needs to be upgraded
to also perhaps search a metadata file that stores pages vs free space.

Recommended to have fixed size records. No varchar or any funny business like that. That way no defragmentation is needed.

BOTH B+ TREE and BUFFER POOL need a serialise and deserialise method to reconstruct and deconstruct them.=>no they don't!!!
lol I can't believe I nearly serialised the fucking buffer pool. That would have been fuckbrained stupid. I just made a pages.page metadata file
and db_dile now has page_metadata as a file object too. page metadata is just id and free space. little endian bytes. 10 of them.

Write record and update record find the size from the provided buffer. Should the caller provide the size instead? 
Currently they find it by finding the first null byte. Yeah, I decided to pass the size there in write_record and update_record in index.rs.

What bothers me more than serialising the buffer pool is : it is absurd to store the buffer pool.
I should instead be maintaining a meta data file of page id as u64 in le bytes and then page.header.upper-page.header.lower so if cache doesn't have free space, it searches the meta data file of the pages.
That way I know what pages are there. Also, I then don't even need to get rid of allocate page in the space over
condition because then find free page only gives space over if even the meta data file says there's no free page. 
This solves page fragmentation.

K so get free page will now search the page metadata too. Upto db_file.size*10/PAGE_SIZE=>number of bytes
10 byte increment. 10 bytes is u64 page id and u16 free space<=8096 bytes. I should probably start using all the
constants I defined and make slot size as 6 bytes a magic constant thing for now.

When there is a page corrupted error, it means the header is fucked or a byte flipped. 
How can replaying a log fixed that. CorruptedDataError maybe. 
But even then, the log file is being emptied regularly so then just when the log is being emptied, if a flush gives corrupted then rebuild the page. 
see, since the page did in fact load, it means it got corrupted in the time from the last flush to now. which will be stored in the wal. But a PageCorrupted error for the magic, type and flags can't be remedied. And a checksum that doesn't 
match means a file I/O error while loading the page or while flushing it earlier. That can't be fixed by just replaying a WAL so I'll do checksum before the WAL.

k I wrote my own crc32 checksum function and then added checksum test on load and compute a checksum when returning
a new page.

# Massive milestone: Persistent indexing test passed


=== PERSISTENT INDEXING TEST ===
write record 1: hello
write record 2: world
write record 3: database
get record 1: hello
get record 2: world
get record 3: database
update record 2: updated
get updated record 2: updated
delete record 2: OK
get deleted record 2: correctly returned RecordAbsent
write record 4 after deletion: OK
get record 4: new record
flushed buffer pool
serialised B+ tree
dropped entire index
record 1 survived restart: hello
deleted record 2 remained deleted after restart
record 3 survived restart: database
record 4 survived restart: new record
write record 5 after restart: OK
record 5 written successfully after restart

=== PERSISTENT INDEXING TEST PASSED ===
