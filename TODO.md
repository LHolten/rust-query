TODO:
- Use Migration concrete types?
- attribute FromColumn type mismatches
- make only Column impl IntoDummy

- ~~Figure out if it is possible to add Insert concrete type~~
- ~~Make dummy type concrete~~
- impl From<T> where T: IntoColumn for Column
- Add transaction lifetime to aggregate?