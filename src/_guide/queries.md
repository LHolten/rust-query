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

You can actually use control flow like `if` conditions and `for` loops inside of a query closure.
This is useful if you want an optional condition for example or if you have a whole list of requirements.
Since the closure only runs once, you have to decide stuff for all result rows at once.
For that reason all the other things you see in this query are `Expr` "expressions". 
For example `customer`, `customer.city`, `customer.email`, `customer.city.eq("Paris")` are all `Expr`.


## Expression and values

As seen in the previous section, `rust-query` makes a lot of use of one "expression" like type: `Expr<S, T>`. 
You can think of `Expr<S, T>` as a function `fn(S) -> T`. It maps data in schema `S` to a value `T`.
`Expr` also has a lifetime, which is used for scoping. All you need to know is that `Expr<'static, S, T>` can be
evaluated in any query scope, or with no scope at all (`Transaction::query_one`).
And another useful property is that `Expr` that are created in a query are not static, but can still be used in sub-queries.

Values like `String`, `Option<String>`, `i64`, `TableRow<X>`, `bool` and `jiff::Timestamp` can be converted into an `Expr<'static, S, T>` where `T` is the type of the value.
This conversion is automatic in most places because `rust-query` arguments are often `impl IntoExpr<S, Typ = T>`.
Sometimes you may need to do a manual conversion using `IntoExpr::into_expr` though.


## Multiple results per row and foreign key joins
```rust
#[derive(Select)]
struct InvoiceInfo {
    track: String,
    artist: String,
    ivl_id: TableRow<InvoiceLine>,
}

fn invoice_info(txn: &Transaction<Chinook>) -> Vec<InvoiceInfo> {
    txn.query(|rows| {
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
    name: String,
}

#[derive(FromExpr)]
#[rust_query(From = Track)]
struct TrackInfo {
    name: String,
    genre: TableRow<Genre>,
    album: AlbumInfo,
}

fn track_info(txn: &Transaction<Chinook>) -> Vec<TrackInfo> {
    txn.query(|rows| {
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
The previous section already did this for `genre`, but that was through `FromExpr`. 
You can also do it without the derives:

```rust
fn track_info(txn: &Transaction<Chinook>) {
    let tracks: Vec<TableRow<Track>> = txn.query(|rows| {
        let track = rows.join(Track);
        rows.filter(track.name.starts_with("Symphony"));
        rows.into_vec(track)
    });

    for track in tracks {
        let album = txn.query_one(&track.into_expr().album.name);
        println!("symphony track in album: {album}")
    }
}
```

This example is a bit silly, because you could have just retrieved the album name in the original query.
What it does show is that `TableRow<Track>` is a useful type. 
It references a row in the database by id and it will never point at a different row.
Since `TableRow` is `Copy + Ord` you can put it in `BTreeMap` and sets etc.
This makes it different from `Expr`, since an `Expr<S, TableRow<T>>` can change what row it points to.
And an `Expr` is not `Copy` or `Ord`.

`TableRow` also proves the existence of the row that it points to.
This is what makes it possible for `txn.query_one(&track.into_expr().album.name)` to guarantee that it can evaluate the expression and that it will have exactly one result.
We don't want deletions to invalidate `TableRow`, so that is why `TableRow` can not be moved between transactions.

## Unique constraints

So, since `TableRow` can not be moved across transactions, how do we find a specific row?
For this we have `unique` constraints! If for example you have a `#[unique] pub_id: String` field, then
you can do `MyTable.pub_id("foo")` to get an `Expr<'static, Schema, Option<TableRow<MyTable>>>`.
The unique constraint can not prove that the row still exists, but at least it can prove that there
is at most one.
Learn more about this in the `rust_query::migration::schema` docs.
