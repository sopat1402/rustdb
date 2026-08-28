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

- CHANGE RECORD ID FROM U16 TO U32 => This is a major todo. Requires quite the refactor, 
    especially with byte serialisation and deserialisation.

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
a new page. Flush recalculates the checksum on its own.

WAL will be a part of Index. WAL file will have the first 8 bytes as the last lsn which is updated on flushes.
That's so that reconstruction knows what the last LSN was since on graceful shutdown, WAL is to be emptied (except
for the 8 byte metadata). If there is still content, then that means there was a crash. A WAL entry will have lsn u64,
task type u16, record id u32, page id u16, data [u8;RECORD_SIZE]. they can be null like in the case of delete, data will
be a null buffer and really, it won't even be checked. page id must be found prior to fsync into the WAL. Reason being,
when clearing the WAL on a checkpoint, I need to know what page each task corresponds to. Also, when an old page is
loaded with a lower LSN, the WAL will be checked if any of the tasks actually correspond to the LSN stored in the page
header. testing the WAL will be hard. I'd need to figure out how to make the database crash in a test because I've
made it pretty secure. The WAL is in the index, so I just need to call index.execute for a task. There do need to
be checkpoint checks for the number of WAL entries. When it reaches the CHECKPOINT_MAX, start clearing the WAL and
making pages mentioned catch up and update their LSN to the last one. Same for the shutdown. Prior to dropping the
index, the WAL must be cleared like that. 

I must first change record_id to u32. LOL record size u16 allows 65535 records. 
65535*134=8781690 bytes. That's 8.8 megabytes. SMH. Imagine a fucking 8.8Mb database. 
I defo need u32. 4,294,967,295 records comes to max 575.5 GB. K I did it. Even removed matches that weren't
needed. could have just used a question mark in some places to propagate the error up.

Changed RECORD_SIZE to 256 bytes. Allows for more data. Ig since fixed size records are being used, I have to 
min max between space wastage due to internal fragmentation and available space to write a record. I cannot do
variable sized records yet as that is quite a deep rabbit hole with defragmentation and I want to first make a
functioning database and database management system. When I do want variable sized records, I just need to edit slotted
page.

# Variable Sized Records

I got rid of RECORD_SIZE. Reading a record yields a vector. Drop it when it is not needed anymore. Writing a record
finds a trash slot with size>=provided size to the function. The leftover space is just left initially.
Maybe later I'll work on making a secondary hole. For now, compaction will happen when there's a space over.
Then a retry. If it fails at that point, then it returns space over. Compaction will empty trash and make a new
buffer, replace the original one. I also need to fix the errors in other
modules due to this change. When update record returns space over, it deleted the record. So then the index needs
to find a new free page and change the B+ tree entry for that record id. Also, free_space and has_space have been
changed to also check trash. Free space returns total free space. Has space takes size and then checks the
contiguous space available and returns true or false based on that.
I'm also recalculating the upper, lower, item count and checksum and marking the page as dirty.
Compaction has been wired in and insertion and reading is working. Only the compaction test needs to be done for
high amount of insertions.

# WAL

10 bytes of log metadata : the last lsn and the length i.e number of entries.
a record will have log size at the start, then lsn, task type (write, update, delete) and then page id,
record id and data, if relevant will be provided in the function as an immutable reference to a vector.
log size is for traversal and deserialisation of a log
for delete, data will be None. It's an Option that is provided so other callers will do Some<&record> in
the function params. log will serialise itself and provide a buffer. After that, the record will be added as bytes.
the size will be updated in the WAL struct to serve as an offset for new writes. The lsn used will be last_lsn+1
and then lsn will be incremented. Along with the flush of the log, the length of the wal that is in the metadata
must be updated too. Option<T> is pretty useful.
On second thought, page id and record id are in fact needed for the write too after the page it is to be written
to is resolved. It is needed for page reconstruction otherwise it could go anywhere.

The find_next_log takes a page id, last lsn and an optional offset. if offset is none it takes it as 10, to skip
the metadata. offset is taken to optimize the search. it is returned with the found log's size added to it if the
log is found. the log and the attached data in an option are also returned. Last lsn is the lsn of the last task done
on that page

Integration with the rest of the code needs fresh eyes. I spent hours doing it, made sphagetti and needed to
roll back to the pre integration state. I'm tempted to use AI but that defeats the purpose of a project and
AI would probably just mess the code up even more.

page.update deletes the record anyways!! I don't need to add a delete log again.
The reconstruct method of the index.rs needs to read the WAL from the first occurrence of the page's id.
Then, it returns an iterator which is passed back and does it again.
The checkpoint function uses get_log_any, which is just any next log. returns an offset iterator as an option for
the next pass. Need to fix ownership in some functions prior to corrupted data error checks. Checkpoint function
will check the page id, get that page, checks its lsn. if the lsn is >=the log lsn, continue. else, commit the
change

Update record needs to check space over post reconstruction too and basically do the space over branch of the
original match. Operations on the page themselves don't need to be redone after reconstruction as the reconstruct
function will just check the last log too. 
- ISSUE : update does reconstruct on corrupted data error, but then uses the reconstruct's error matching.
        it needs to check for spaceover for a reallocation. So, another method must be used to know if there isn't space.
- Resolved : did reconstruct normally with a ?; after the call. then, did read_record on the record. If ok, don't bind to
        the value and just continue. If record absent, it means that the record was deleted due to a space over
        inside the update when reconstructing. then, use the code to put the record elsewhere. Any other error is
        returned.

made allocate_page and page_header :: new and buffer pool allocate page take lsn, assign wal.last_lsn

# WAL test

- fuck I realized I was passing db_file to the b_plus_tree for serialise. I'll be making a bootup function in
    index. nvm I made a db file struct. the wal owns its own file. doesn't need a file object passed to it.

Index was returning size on success. Made it return the record id instead since I abstracted that away. Made
update_record return nothing i.e Ok(()).
The database passes a CRUD test. I have to make index make the WAL checkpoint with a force flag on new index
if the wal isn't empty. I'll do so in the bootup function since the constructor doesn't take &self.

- My checkpoint function was not doing stuff to the B+ tree, as it shouldn't. Checkpointing is normal behavior
    on the page. Instead, I'll make a recover function that actually puts stuff into the tree as well.

Added a recover function. Also shifted the wal write to after shutdown. that's to not hit the checkpoint
function in graceful shutdown. 

Alright so I'm not editing page metadata somewhere and that's causing find_free_page to still thing a certain
page has free space in my update test where I force a spaceover with the WAL. NAHH the issue was an incorrect
free space test in that branch of the buffer pool find_free_page. it was subtracting size_x, which is the free
space read from the file. K also my lru head was not updating head.prev. That was one bug too.

Now somehow, the fucking page I need got deleted from the map? what the fuck how is that possible, who in the
name of god deleted it, it sure as hell wasn't me?

I'm actually going to fucking shoot someone. The bug was that read_page and write_page were using 0 indexed pages
because some mofo FORGOT TO UPDATE THEM to 1 indexed!!
There is an idempotency issue though : update_record, which is called by recover is unconditionally adding
to the WAL. I'll just make an update_record_recovery function that does not add update to the WAL.

I'm debugging again for checkpoint. First, I made the spaceover branch for write do stuff and got rid of the
special update branch and just made it call update_record_recover because that doesn't checkpoint or add unneeded
WAL entries. The overflow test for checkpoint passed. Delete already passed. Now, I'm making my index methods
edit page metadata wherever I forgot. K I fixed it.

Found another bug : recover's write was trying to take a cached free page, which would naturally give
spaceover. Fixed it, standard code from the other branch.
