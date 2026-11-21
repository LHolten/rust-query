Updates are always done using the `Mutable` type that you get from calling `TransactionScope::mutable`.

If what you have is a `&mut Transaction`, then the first thing you need to do is enter a new transaction scope
with `Transaction::scope`. That will give you the `TransactionScope` that you need.

The only reason that `&mut Transaction` exists at all is to allow downgrading with `Transaction::downgrade`.
That gives a `TransactionWeak` that can be used to delete rows.

Usage of the `Mutable` type is really straigtforward:

```rust
fn add_balance(txn: &TransactionScope, user: TableRow<User>, amount: u32) {
    txn.mutable(user).balance += amount;
}
```
```sql
UPDATE user
SET balance = balance + $2
WHERE id = $1
```

## Updating columns that are part of unique constraints

This is only slightly more complicated than other updates.

```rust
fn update_email(txn: &TransactionScope, user: TableRow<User>, email: &str) {
    match txn.mutable(user).unique(|user| {
        user.email = email.to_owned();
    }) {
        Ok(()) => println!("success"),
        Err(conflict_user) => {
            let name = &txn.lazy(conflict_user).name;
            panic!("conflict: user {name} already has email {email}")'
        }
    }
}
```
