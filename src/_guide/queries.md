Rust query's primary goal is to make it super easy to write queries!
Having preliminary knowledge about SQL is not required. 
However, we will show the equivalent SQL for each rust-query query since that is useful for those that do know SQL.
All code below assumes that you have a `txn: &Transaction<Chinook>` (read rights on a sqlite db with chinook schema).

## Baby's first query:
```rust
fn paris_customer_emails(txn: &Transaction<Chinook>) -> Vec<String> {
    txn.query(|rows| {
        let customer = rows.join(Customer);
        rows.filter(customer.city.eq("Paris"));
        rows.into_vec(&customer.email)
    })
}
```
<details><summary>The equivalent SQL</summary>

```sql
SELECT customer.email
FROM customer AS customer
WHERE customer.city = 'Paris'
```
</details>

The `Transaction::query` method only executes the closure once.
It gives the closure a `rows: Query`. `Query` dereferences to `Rows`, which provides
`Rows::join` and `Rows::filter`. Those methods mutate `Rows` to keep track of which tables are joined
and what those tables are filtered on. `Query` itself has the `Query::into_vec` method, which makes
actual execution of the query possible.

All other things you see in this query are `Expr` "expressions". For example `customer`, `customer.city`, `customer.email`, `customer.city.eq("Paris")` are all `Expr`. The `Expr` type has a generic to keep track of the type of the value that the expression evaluates to.
