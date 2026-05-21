use std::{borrow::Cow, convert::Infallible, fmt::Debug, marker::PhantomData, rc::Rc};

use crate::{
    Table, TableRow,
    db::TableRowInner,
    lower::{self, emit},
};

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
        table: &'static str,
        cols: Vec<(&'static str, Rc<lower::Expr>)>,
        msg: String,
    ) -> Self;
}

impl FromConflict for Infallible {
    fn from_conflict(
        _txn: &rusqlite::Transaction<'_>,
        _table: &'static str,
        _cols: Vec<(&'static str, Rc<lower::Expr>)>,
        _msg: String,
    ) -> Self {
        unreachable!()
    }
}

impl<T: Table<Conflict = Self>> FromConflict for Conflict<T> {
    fn from_conflict(
        _txn: &rusqlite::Transaction<'_>,
        _table: &'static str,
        _cols: Vec<(&'static str, Rc<lower::Expr>)>,
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
        table: &'static str,
        mut cols: Vec<(&'static str, Rc<lower::Expr>)>,
        _msg: String,
    ) -> Self {
        let unique_columns = get_unique_columns::<T>();

        cols.retain(|(name, _val)| unique_columns.contains(&Cow::Borrowed(*name)));
        assert_eq!(cols.len(), unique_columns.len());

        let mut select = Rc::new(lower::Rows::default());
        let join = select.join(lower::JoinableTable::Table(table));

        for (col, val) in cols {
            let table_val = Rc::new(lower::Expr::RowIndex(lower::RowLike::Join(join), col));
            select.filter(Rc::new(lower::Expr::Infix(val, "=", table_val)));
        }

        let select = select.into_vecs();
        let mut info = emit::Select::new(&select);

        let id = Rc::new(lower::Expr::RowIndex(lower::RowLike::Join(join), T::ID));
        info.add_select(&select, &id);
        let info = info.into_vecs(select);

        let mut stmt = emit::Stmt::default();
        info.emit(&mut stmt, false).unwrap();

        let mut cached = txn.prepare_cached(&stmt.sql).unwrap();
        cached
            .query_one(&stmt.params, |row| {
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
