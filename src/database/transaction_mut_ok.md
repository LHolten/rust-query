Same as [Self::transaction_mut], but always commits the transaction.

The only exception is that if the closure panics, a rollback is performed.
