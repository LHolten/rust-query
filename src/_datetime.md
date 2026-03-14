# On the choice of sqlite datetime format.

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
Unfortunately this format is not very compact, the only optimization we can do is to not store trailing 0s in the fractional seconds.
Another problem is that for datetime strings with a negative year, the sorting order is wrong.
Sorting a mix of datetimes with and without fractional seconds can also give wrong results.

The one remaining problem is that sqlite does not support any operations on datetimes before `0000-01-01 00:00:00`. In practice it seems to be able to handle up to `-4713-11-24 12:00:00`.
This should not be a problem for most applications, but it could cause panics if timstamps are received from untrusted sources.

Also sqlite will round results, even with subsec precision and it is inconsistent:
`'0000-01-01 00:00:00.9979'` -> `'0000-01-01 00:00:00.998'`
`'0000-01-01 00:00:00.9989'` -> `'0000-01-01 00:00:00.999'`
`'0000-01-01 00:00:00.9999'` -> `'0000-01-01 00:00:00.999'`

It looks like the seconds component is never updated.
We can use that for our check constraint..

`"col" IS datetime("col") || rtrim(substr("col", 20), '0')`

This checks the properties that are required for correct sorting and comparisons.
- no negative years (`datetime("col")` would have length 20 instead of 19).
- no trailing `.` or `0` after the `.`.
