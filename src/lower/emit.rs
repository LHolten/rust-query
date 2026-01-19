use std::{
    collections::{BTreeMap, BTreeSet},
    rc::{self, Rc},
};

use crate::lower::{Expr, Join, Select, Unique};

/// This is pre escaped SQL
struct Emitted(String);

#[derive(Clone, Copy)]
enum TableAlias {
    Forward(usize),
    Unique(usize),
    Join(usize),
}
#[derive(Clone, Copy)]
enum ColAlias {
    Select(usize),
    Forward(usize),
}

struct UniqueOptions {
    alias: TableAlias,
    filter: bool,
    emitted_conds: Vec<Emitted>,
}

struct SelectOptions {
    alias: ColAlias,
    emitted: Emitted,
}

enum TableScope {
    From(Join),
    Extra(),
}

struct EmitSelect {
    /// these are the tables that are aggregated over
    from: BTreeMap<Join, TableAlias>,
    /// These are the tables that are grouped by
    /// These are joined distinct to not influence the aggregate
    forwarded: BTreeMap<Join, usize>,
    unique: BTreeMap<Unique, UniqueOptions>,
    filter: BTreeMap<Rc<Expr>, Emitted>,
    select: BTreeMap<Rc<Expr>, SelectOptions>,
}

impl EmitSelect {
    pub fn new(select: Select) -> Self {
        let mut out = EmitSelect {
            from: select
                .from
                .into_iter()
                .enumerate()
                .map(|(idx, val)| (val, TableAlias::Join(idx)))
                .collect(),
            unique: BTreeMap::new(),
            filter: BTreeMap::new(),
            forwarded: BTreeMap::new(),
            select: BTreeMap::new(),
        };
        for expr in select.filter {
            out.filter(expr);
        }
        out
    }

    pub fn select_alias(&mut self, val: Rc<Expr>) -> ColAlias {
        if let Some(ops) = self.select.get(&val) {
            return ops.alias;
        }
        let emitted = self.emit(&val);
        let alias = ColAlias::Select(self.select.len());
        let options = SelectOptions { emitted, alias };
        assert!(self.select.insert(val, options).is_none());
        alias
    }

    fn emit(&mut self, expr: &Expr) -> Emitted {
        match expr {
            Expr::Constant(_) => todo!(),
            Expr::Parameter(ord_rc) => todo!(),
            Expr::AggrIndex(select, expr) => todo!(),
            Expr::RowIndex(row_like, _) => todo!(),
            Expr::Prefix(_, expr) => todo!(),
            Expr::Infix(expr, _, expr1) => todo!(),
            Expr::Func(_, exprs) => todo!(),
        }
    }

    fn join_alias(&mut self, join: Join) -> TableAlias {
        if let Some(from) = self.from.get(&join) {
            return *from;
        }
        let next = self.forwarded.len();
        let idx = *self.forwarded.entry(join).or_insert(next);
        TableAlias::Forward(idx)
    }

    fn unique_alias(&mut self, unique: Unique) -> TableAlias {
        if let Some(val) = self.unique.get(&unique) {
            return val.alias;
        }
        let emitted_conds: Vec<_> = unique.conds.iter().map(|(_, v)| self.emit(v)).collect();
        // reserve new alias after emitting all conds
        let alias = TableAlias::Unique(self.unique.len());
        let options = UniqueOptions {
            alias,
            filter: false,
            emitted_conds,
        };
        assert!(self.unique.insert(unique, options,).is_none());
        alias
    }

    fn filter(&mut self, expr: Rc<Expr>) {
        if self.filter.contains_key(&expr) {
            return;
        }
        // TODO: if the expr checks that some tableidx is some, we might be able
        // to change joinoptions instead of filtering.
        let emitted = self.emit(&expr);
        assert!(self.filter.insert(expr, emitted).is_none());
    }
}
