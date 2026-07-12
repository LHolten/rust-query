use crate::{
    Transaction,
    lower::{self, list_writer::Alias},
    migrate::Schema,
    schema::{from_db, from_macro, read::read_schema},
};

#[derive(Debug, Clone, Copy)]
pub enum Detail {
    // Unique constraints that are part of a table definition
    // can not be dropped, so we assume the worst and just recreate
    // the whole table.
    Indexes,
    // TODO(someday): Technically we can fix these by manipulating the `sqlite_schema`
    ForeignKeys,
}

pub fn fix_by_copy<S: Schema>(txn: &Transaction<S>, detail: Detail) {
    let schema = read_schema(txn);
    let expected_schema = crate::schema::from_macro::Schema::new::<S>();

    fn apply_detail(old: &mut from_db::Table, goal: &from_macro::Table, detail: Detail) -> bool {
        let mut changed = false;
        macro_rules! foo  {
            ($($a:ident).* = $b:expr) => {
                let _new = $b;
                changed |= $($a).* != _new;
                $($a).* = _new;
            };
        }
        match detail {
            Detail::Indexes => {
                foo!(old.indices = goal.indices.iter().map(|v| v.def.clone()).collect());
            }
            Detail::ForeignKeys => {
                for (name, def) in &mut old.columns {
                    foo!(def.fk = goal.columns[name].def.fk.clone());
                }
            }
        }
        changed
    }

    for (table_name, mut table) in schema.tables {
        let goal = &expected_schema.tables[&table_name.as_str()];

        if apply_detail(&mut table, goal, detail) {
            let scope = lower::Scope::default();
            let tmp_name = scope.tmp_table();

            txn.execute(&table.create(lower::JoinableTable::Tmp(tmp_name)));

            let mut columns: Vec<_> = table.columns.keys().map(|x| Alias(x.as_str())).collect();
            columns.push(Alias("id"));
            let columns = columns
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(", ");

            txn.execute(&format!(
                "INSERT INTO main.{tmp_name} ({columns}) SELECT {columns} FROM main.{}",
                Alias(&table_name)
            ));

            txn.execute(&format!("DROP TABLE main.{}", Alias(&table_name)));

            txn.execute(&format!(
                "ALTER TABLE main.{tmp_name} RENAME TO {}",
                Alias(&table_name)
            ));
            // Add the new non-unique indices
            for sql in table.delayed_indices(&table_name) {
                txn.execute(&sql);
            }
        }
    }

    // check that we solved the mismatch
    let schema = read_schema(txn);
    for (name, mut table) in schema.tables {
        let expected_table = &expected_schema.tables[&*name];
        assert!(!apply_detail(&mut table, expected_table, detail));
    }
}
