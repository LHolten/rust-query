Create an immutable [Transaction].
Immutable transactions never need to wait on any other transactions.
The transaction will be executed on a read-only snapshot of the database.

This function will panic if the schema was modified compared to when the [Database] value
was created. This can happen for example by running another instance of your program with
additional migrations.

Note that many systems have a limit on the number of file descriptors that can
exist in a single process. On my machine the soft limit is (1024) by default.
If this limit is reached, it may cause a panic in this method.
