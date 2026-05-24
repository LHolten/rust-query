use std::collections::BTreeSet;

use crate::{
    Transaction,
    lower::{self, list_writer::Alias},
    migrate::Schema,
    schema::{from_db, from_macro, read::read_schema},
};

pub fn fix_indices<S: Schema>(txn: &Transaction<S>) {
    let schema = read_schema(txn);
    let expected_schema = crate::schema::from_macro::Schema::new::<S>();

    fn check_eq(expected: &from_macro::Table, actual: &from_db::Table) -> bool {
        let expected: BTreeSet<_> = expected.indices.iter().map(|idx| &idx.def).collect();
        let actual: BTreeSet<_> = actual.indices.values().collect();
        expected == actual
    }

    for (&table_name, expected_table) in &expected_schema.tables {
        let table = &schema.tables[table_name];

        if !check_eq(expected_table, table) {
            // Unique constraints that are part of a table definition
            // can not be dropped, so we assume the worst and just recreate
            // the whole table.

            let scope = lower::Scope::default();
            let tmp_name = scope.tmp_table();

            txn.execute(&expected_table.create(lower::JoinableTable::Tmp(tmp_name), "id"));

            let mut columns: Vec<_> = expected_table
                .columns
                .keys()
                .map(|x| Alias(x.as_str()))
                .collect();
            columns.push(Alias("id"));
            let columns = columns
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(", ");

            txn.execute(&format!(
                "INSERT INTO main.{tmp_name} ({columns}) SELECT {columns} FROM main.{}",
                Alias(table_name)
            ));

            txn.execute(&format!("DROP TABLE main.{}", Alias(table_name)));

            txn.execute(&format!(
                "ALTER TABLE main.{tmp_name} RENAME TO {}",
                Alias(table_name)
            ));
            // Add the new non-unique indices
            for sql in expected_table.delayed_indices(table_name) {
                txn.execute(&sql);
            }
        }
    }

    // check that we solved the mismatch
    let schema = read_schema(txn);
    for (name, table) in schema.tables {
        let expected_table = &expected_schema.tables[&*name];
        assert!(check_eq(expected_table, &table));
    }
}
