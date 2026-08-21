Paging has various flags, bring to 96 bytes. currently just using page id
need to use dirty flag now that flush is done

made an error module with a big enum in it and use that everywhere so that it has all types.

CHANGE RECORD ID FROM U16 TO U32 => This is a later todo. Requires quite the refactor, especially with byte serialisation
and deserialisation.

For now, the database can have a maximum of 65,536 records.

lru cache uses a free list that owns DLLNodes, which own the page objects. There's a trash vector too.
it stores indices of deleted nodes in the old vector.
am i checking if the node id is in trash during search? no. do i need to? idk. prolly not on just 1 thread.
dll nodes store indices in the nodes vector. addition of nodes first checks if trash is empty.

popping a node from the dll flushes it but this should be done only if it is dirty. haven't added dirty
flag usage yet even though page header includes it.

buffer pool hides APIs for page stuff so that other components need only worry about getting the page and
not whether it is from RAM or ROM. it caches it right after. there's get mut and regular get. that's because
of rust's ownership mechanism, which has been a bit of a nuisance so far for a single threaded project.

db file modified only on flush and allocate so far. page deletion not added. otherwise, it's a buffer of 
[u8;PAGE_SIZE] that is modified. it is all little endian bytes. slots and the header have a serialize method
in their impl too to abstract all the byte management away. Just make an array using to_le_bytes() and then
do copy_from_slice at the needed offset in the buffer from the bytes array.

each record is fixes size : 128 bytes at max size. That doesn't mean each user entry must be 128 bytes.
just that putting 128 bytes solves external fragmentation but usually guarantees internal fragmentation.
each slot is 6 bytes. record id, offset and size. this is where the actual user entered data size is stored.

# B+ tree

AAAAH. between m/2 and m values per node. each node has next too, forms a linked list. this prevents going back
and speeds up traversal. Quite a lot of book-keeping methods, public ones are insertion and deletion, search, etc.

# Index

Index owns a B+ tree as well as a buffer pool. Checks if record exist, tries to get the page and then carries out
the necessary operation.

For writing a record, a next record id is maintained in the index. Size is found from the provided buffer. For finding the page_id, the buffer pool's
cached pages are scanned and the first one that isn't in the trash vector and has free space i.e lower-upper>=134 as each record is 128 bytes and
each slot is 6 bytes and then it returns the page_id. this does not affect the LRU cache order. Then a simple write is made with that page_id after getting
it from the buffer pool.

Recommended to have fixed size records. No varchar or any funny business like that. That way no defragmentation is needed.

BOTH B+ TREE and BUFFER POOL need a serialise and deserialise method to reconstruct and deconstruct them.

Write record and update record find the size from the provided buffer. Should the caller provide the size instead? 
Currently they find it by finding the first null byte. Yeah, I decided to pass the size there in write_record and update_record in index.rs.

# What I learned

- Byte handling
- Persistence
- Ownership
- Corruption tests
- Error handling
- I might be a masochist
