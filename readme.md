# essential functionality

- chunk files
- reconstruct files

# how is this used

This library is used to chunk files. simples. If you find it interesting or useful then you're a smarter person than I am. This library is created to facilitate reading and writting files in chunks. It's based around an optimisation for I/O operations where reading a whole is not that fast and chokes your memory. Although it alters your files, but not permanently, and if something does happen, you can easily reverse engineer the operations and figure out how to put your files back to normal :). My end goal for this library is to write it as a http file server, and provide code bindings to interact with the server, like an orm. It would be its own service, self managing with caching and streaming responses with play pause functionality. Using a database is a bottle neck but it would provide an ironclad security of files being reconstructed.

## how can we ensure proper reconstruction

when we split, we're breaking the file apart, its immediately corrupted.
the way we can ensure this, is to:

- hash the complete file. we store that hash and use it for the sorce of truth.
- when we split out chunks, we do add more data, we add a header/push data into the file.
  - that data we push into the file, it stores the chunk file's hash
  - we need to store that hash into a chunk, after we've calculated it, so that when the file is being reconstructed, we can ensure that those hashes match.
  - this is a overhaul security of file, and it might seem like you're doing more operations when you can just:
    1. read a file
    2. and send it line by line
       but when we're dealing with low-speed/bandwidth internet, pushing a whole file would be very difficult to reach
       for big files, if you're reading huge dataset files, and they're not chunked, in order to use them, they'd be read, whole, into memory which is not what's useful. although, idk how this library would solve that problem. when using a dataset, the whole thing needs to be read into memory. maybe we can speed up the reading process by streaming read.

## addition of merkle trees

chunking is a dangerous operation for files. You are tearing files apart and permanently altering them. So in-order to sustain that files are reconstructed properly, we need to create an identity hash to ensure those hashes arent changed. A solution for this problem is to simply hash the original file, then chunk the file, then hash those chunks and keep track of those hashes. Now thats a valid solution, but there's an ineffecientcy with that solution, as that there's no integrity of that solution.
what do merkle tree's unlock?

1. partial verification: instead of having to reconstruct the whole file and then checking the integrity of chunks to see if that chunk still belongs to its original file structure, merkle tree's allow us to verify each chunk independently, all we'd need is the root hash and the chunk's hash, from there we can verify the chunk simply due to the proofing method.
2. incremental updates to file: since our tree is built from leaves to root, leaves are the first place for change to occur, which allows for some optimised on the fly changes to the file itself. hypothetically if you're writting an asset manager for a big company that needs to physically sustain security for thier files (although its quite easy to reverse engineer the trees without encryption for the original file) for usable archival purposes, if for an example the file needed some editing, that could occur on the fly, and that would occur with o(log N) instead of o(n)
3. plug and play for distributed systems: since merkle tree's computation is difinitive semantically, its either true or false, we can utilise distributed systems for calculating hashes for especially large files. lets say for example our file is split into a million chunks or even hundreds of chunks that are huge files themselfs, distributed work can be divided for working out the proof. computer 1 can work out certain branch, and another computer can do another branch, working together to work out to verify the original file.
4. security: when working with data thats not human inspectable, its quite hard to sustain security. merkle tree's can help software avoid i/o writes if the root hash is affected by some change, any modification forces the change of the root hash, which allows better insight at an improved computation rate.

## how do we chunk

this is the order of operations when chunking a file. In the end i have chosen to go with a manifest json file. we will be storing the merkle tree structure in the manifest file.

## how we reconstruct

and this is the order of operation per file reconstruction

1. we start off with a file name
2. then we find chunk number 1-6
3. once we've gotten all 6 of our file chunk names, we then read those chunks 1 by 1.
   1. while reading, we do a chunk hash check, to make sure the chunk isn't damaged
   2. then we sanitise the chunk's header and footer
   3. and append write it as its original file
4. that happens one by one per chunk
5. once all chunks are appended to the original file
6. we do a hash check for the original file
7. if it matches then we're all set.

# naming convention

`[originalFileName]_[ChunkNumber].[FileExtension]`

- `originalFileName` is used to avoid unnessisary data being stored in the database
- `ChunkID` is the short form hash. its present in the file itself as well.
- `ChunkNumber` is the part number of the file chunk division
- `FileExtension` is sustained original is due to not convoluting the database with data thats not nessisary.

# auxilary functionality

- fast fetch
- encryption
- compression
- streaming
