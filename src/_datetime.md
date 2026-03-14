# On the choice of sqlite datetime format

goals:
1. support conversion to and from common rust timestamps without surprises.
2. compatibility with other applications that use sqlite and existing schemas.
3. reduce storage size, without compromising the previous goals.

goal 1 is difficult because of the following causes:
- rounding, (most rust timestamps have nanosecond precision)
- range, sqlite does not support negative years, rust types have different ranges.

sqlite builtin datetime support accepts three basic formats:
- string datetime, extra precision beyond ms is ignored by sqlite
- integer, only support for whole seconds
- float (julian day or unix seconds), extra precision beyond ms is ignored by sqlite.
  floats are only lossless to precision of miliseconds.

The only format that supports storing nanoseconds and can be interpreted by sqlite natively is string datetimes.
Working with strings gives some small issues:
- Datetime strings with a negative year are sorted incorrectly.
- Datetimes that should be equal can differ because of trailing zeros in the fractional seconds.

We can "fix" string datetimes by not allowing them to be negative and not storing trailing 0s in the fractional seconds.
Removing trailing zeros in the fractional seconds also improves storage size a bit when precision is not needed.

# check constraint

Sqlite will round results, even with subsec precision and it is inconsistent:
`'0000-01-01 00:00:00.9979'` -> `'0000-01-01 00:00:00.998'`
`'0000-01-01 00:00:00.9989'` -> `'0000-01-01 00:00:00.999'`
`'0000-01-01 00:00:00.9999'` -> `'0000-01-01 00:00:00.999'`

It looks like the seconds component is never updated.
We can use that for our check constraint..

`"col" IS ltrim(datetime("col"), '-') || rtrim(substr("col", 20, 10), '0')`

This checks the properties that are required for correct sorting and comparisons.
- no negative years (`-` prefix).
- no trailing `0` after the `.`.
- no more than 9 digits after the `.`.

Only the range incompatibility sadness remains:
- jiff -> sqlite: negative years give error
- sqlite -> jiff: errors from `9999-12-30 22:00:00` until `9999-12-31 23:59:59`

The seconds error kind is simultanously better and worse.
It is less likely to happen because another program needs to write the value.
When it happens it is not possible to recover using rust-query alone.
