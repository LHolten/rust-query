use std::{borrow::Cow, convert::Infallible, fmt::Debug, marker::PhantomData};

use sea_query::{ExprTrait, SelectStatement, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use crate::{Table, TableRow, db::TableRowInner};

/// Error type that is used by [crate::Transaction::insert] and [crate::Mutable::unique] when
/// there are at least two unique constraints.
///
/// The source of the error is the message received from sqlite. It contains the column
/// names that were conflicted.
pub struct Conflict<T: Table> {
    _p: PhantomData<T>,
    msg: Box<dyn std::error::Error>,
}

#[cfg_attr(test, mutants::skip)]
impl<T: Table> Debug for Conflict<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conflict")
            .field("table", &T::NAME)
            .field("msg", &self.msg)
            .finish()
    }
}

impl<T: Table> std::fmt::Display for Conflict<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Conflict in table `{}`", T::NAME)
    }
}

impl<T: Table> std::error::Error for Conflict<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.msg)
    }
}

impl<T: Table<Conflict = Self>> std::fmt::Display for TableRow<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let unique_columns = get_unique_columns::<T>().join(", ");
        write!(
            f,
            "Row exists in table `{}`, with unique constraint on ({})",
            T::NAME,
            unique_columns
        )
    }
}

impl<T: Table<Conflict = Self>> std::error::Error for TableRow<T> {}

pub(crate) trait FromConflict {
    fn from_conflict(
        txn: &rusqlite::Transaction<'_>,
        table: sea_query::DynIden,
        cols: Vec<(&'static str, sea_query::Expr)>,
        msg: String,
    ) -> Self;
}

impl FromConflict for Infallible {
    fn from_conflict(
        _txn: &rusqlite::Transaction<'_>,
        _table: sea_query::DynIden,
        _cols: Vec<(&'static str, sea_query::Expr)>,
        _msg: String,
    ) -> Self {
        unreachable!()
    }
}

impl<T: Table<Conflict = Self>> FromConflict for Conflict<T> {
    fn from_conflict(
        _txn: &rusqlite::Transaction<'_>,
        _table: sea_query::DynIden,
        _cols: Vec<(&'static str, sea_query::Expr)>,
        msg: String,
    ) -> Self {
        Self {
            _p: PhantomData,
            msg: msg.into(),
        }
    }
}

pub(crate) fn get_unique_columns<T: Table<Conflict = TableRow<T>>>() -> Vec<Cow<'static, str>> {
    // TODO: optimize to const
    let schema = crate::schema::from_macro::Table::new::<T>();
    let [index] = schema
        .indices
        .into_iter()
        .filter(|x| x.def.unique)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    index.def.columns
}

impl<T: Table<Conflict = Self>> FromConflict for TableRow<T> {
    fn from_conflict(
        txn: &rusqlite::Transaction<'_>,
        table: sea_query::DynIden,
        mut cols: Vec<(&'static str, sea_query::Expr)>,
        _msg: String,
    ) -> Self {
        let unique_columns = get_unique_columns::<T>();

        cols.retain(|(name, _val)| unique_columns.contains(&Cow::Borrowed(*name)));
        assert_eq!(cols.len(), unique_columns.len());

        let mut select = SelectStatement::new()
            .from(("main", table.clone()))
            .column((table.clone(), T::ID))
            .take();

        for (col, val) in cols {
            select.cond_where(val.equals((table.clone(), col)));
        }

        let (query, args) = select.build_rusqlite(SqliteQueryBuilder);

        let mut stmt = txn.prepare_cached(&query).unwrap();
        stmt.query_one(&*args.as_params(), |row| {
            Ok(Self {
                _local: PhantomData,
                inner: TableRowInner {
                    _p: PhantomData,
                    idx: row.get(0)?,
                },
            })
        })
        .unwrap()
    }
}
