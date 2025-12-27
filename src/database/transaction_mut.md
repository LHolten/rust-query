Create a mutable [Transaction].
This operation needs to wait for all other mutable [Transaction]s for this database to be finished.
There is currently no timeout on this operation, so it will wait indefinitly if required.

Whether the transaction is commited depends on the result of the closure.
The transaction is only commited if the closure return [Ok]. In the case that it returns [Err]
or when the closure panics, a rollback is performed.

This function will panic if the schema was modified compared to when the [Database] value
was created. This can happen for example by running another instance of your program with
additional migrations.

Note that many systems have a limit on the number of file descriptors that can
exist in a single process. On my machine the soft limit is (1024) by default.
If this limit is reached, it may cause a panic in this method.
