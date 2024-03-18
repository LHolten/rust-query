#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    From,
    Filter,
    Order,
}

// push the query into a sub_query so that all columns are referenceable
pub fn push_down(select: &mut SelectStatement) {
    let inner = select.expr(Expr::col(Asterisk)).take();
    *select = Query::select().from_subquery(inner, NullAlias).take();
}

impl MyDef {
    pub fn into_select(self) -> SelectStatement {
        let mut select = Query::select().from(self.table).take();
        for (col, alias) in self.columns {
            select.expr_as(Expr::col(col), alias);
        }
        select
    }
}

impl MySelect {
    pub fn into_select(self, then: Option<SimpleExpr>) -> SelectStatement {
        let mut select = Query::select();
        let mut stage = Stage::From;
        for op in self.ops {
            match op {
                Operation::From(table) => {
                    // we need to make sure that we are in the [From] stage
                    if stage > Stage::From {
                        push_down(&mut select);
                    }
                    let right = match table {
                        Source::Select(right) => right.into_select(None),
                        Source::Table(right) => right.into_select(),
                    };
                    select.from_subquery(right, NullAlias);
                    stage = Stage::From;
                }
                Operation::Filter(expr) => {
                    if stage > Stage::Filter {
                        push_down(&mut select);
                    }
                    select.and_where(expr);
                    stage = Stage::Filter;
                }
            }
        }
        if let Some(then) = then {
            // we need to push down windows
            if stage > Stage::Filter {
                push_down(&mut select)
            }
            select.expr(then);
        } else {
            select.expr(Expr::col(Asterisk));
        }
        select
    }
}
