Rust query's primary goal is to make it super easy to write queries!
Having preliminary knowledge about SQL is not required. 
However, we will show the equivalent SQL for each `rust-query` query since that is useful for those that do know SQL.
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

## Multiple results per row and foreign key joins
```rust
#[derive(Select)]
struct InvoiceInfo {
    track: String,
    artist: String,
    ivl_id: TableRow<InvoiceLine>,
}

fn invoice_info(db: &Transaction<Schema>) -> Vec<InvoiceInfo> {
    db.query(|rows| {
        let ivl = rows.join(InvoiceLine);
        rows.into_vec(InvoiceInfoSelect {
            track: &ivl.track.name,
            artist: &ivl.track.album.artist.name,
            ivl_id: &ivl,
        })
    })
}
```
<details><summary>The equivalent SQL</summary>

```sql
SELECT ivl.id, track.name, artist.name 
FROM invoice_line AS ivl
JOIN track ON ivl.track = track.id
JOIN album ON track.album = album.id
JOIN artist ON album.artist = artist.id
```
</details>

The `Select` derive creates a sibling struct with the `Select` postfix. In this case that is
`InvoiceInfoSelect`. This sibling struct has all fields with `Expr` types, to make it possible
to construct it inside a query. The `Query::into_vec` method will then build actual `InvoiceInfo`
rows based on the expressions from the `InvoiceInfoSelect`.

Another interesting bit in this example is the use of foreign key joins like `&ivl.track.album.artist.name`.
`rust-query` allows this kind of implicit joining because it can not affect the number of rows.
For `Option` (nullable) foreign keys, you have to filter first (see a later section).

P.S. You can query nested `Select` derive structs.

## Plain table subset select

Not always do you need complex expressions for the different result columns.
That is why there is another derive that makes it easier.

```rust
#[derive(FromExpr)]
#[rust_query(From = Album)]
struct AlbumInfo {
    name: String
}

#[derive(FromExpr)]
#[rust_query(From = Track)]
struct TrackInfo {
    name: String
    genre: String,
    album: AlbumInfo,
}

fn track_info(db: &Transaction<Schema>) -> Vec<TrackInfo> {
    db.query(|rows| {
        let track = rows.join(Track);
        rows.filter(track.name.starts_with("Symphony"));
        rows.into_vec(TrackInfo::from_expr(track))
    })
}
```
The `FromExpr` derive can be used to get a subset of table columns and it works over foreign keys!
This example also shows a more interesting filter.

## TableRow and query_one

You may wonder what happens when you put a table directly into `rows.into_vec` instead
of specifying one of the fields.

```rust
fn track_info(db: &Transaction<Schema>) -> Vec<TableRow<Track>> {
    let tracks: Vec<TableRow<Track>> = db.query(|rows| {
        let track = rows.join(Track);
        rows.filter(track.name.starts_with("Symphony"));
        rows.into_vec(track)
    })

    for track in tracks {
        db.query_one(track)
    }
}
```
