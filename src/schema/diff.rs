use std::collections::BTreeMap;

use annotate_snippets::{AnnotationKind, Group, Level, Snippet};

use crate::schema::{from_db, from_macro};

pub enum EntryDiff<A, E> {
    DbOnly(E),
    MacroOnly(A),
    Diff { actual: A, expected: E },
}

pub fn diff_map<A, E>(
    mut actual: BTreeMap<String, A>,
    expected: BTreeMap<String, E>,
) -> BTreeMap<String, EntryDiff<A, E>> {
    let mut out = BTreeMap::new();
    for (key, expected) in expected {
        match actual.remove(&key) {
            Some(actual) => {
                out.insert(key, EntryDiff::Diff { actual, expected });
            }
            None => {
                out.insert(key, EntryDiff::DbOnly(expected));
            }
        }
    }
    for (key, actual) in actual {
        out.insert(key, EntryDiff::MacroOnly(actual));
    }
    out
}

impl from_db::Schema {
    pub fn diff<'a>(
        self,
        from_macro: from_macro::Schema,
        source: &'a str,
        path: &'a str,
    ) -> Vec<Group<'a>> {
        let mut db_only = Vec::new();
        let mut report = Vec::new();

        for (table, diff) in diff_map(from_macro.tables, self.tables) {
            match diff {
                EntryDiff::DbOnly(_) => {
                    db_only.push(Level::ERROR.message(format!("table {table} was not defined")))
                }
                EntryDiff::MacroOnly(val) => {
                    let span = val.span.0..val.span.1;
                    report.push(
                        Level::ERROR
                            .primary_title("database does not have table")
                            .element(
                                Snippet::source(source).path(path).annotation(
                                    AnnotationKind::Primary
                                        .span(span)
                                        .label("table is defined here"),
                                ),
                            ),
                    );
                }
                EntryDiff::Diff { actual, expected } => {
                    let mut db_only = Vec::new();
                    let mut annotations = Vec::new();

                    let span = actual.span.0..actual.span.1;
                    for (col, diff) in diff_map(actual.columns, expected.columns) {
                        match diff {
                            EntryDiff::DbOnly(_) => db_only.push(
                                Level::ERROR.message(format!("column {col} was not defined")),
                            ),
                            EntryDiff::MacroOnly(column) => {
                                let span = column.span.0..column.span.1;
                                annotations.push(
                                    AnnotationKind::Context
                                        .span(span)
                                        .label("this column does not exist in the database"),
                                );
                            }
                            EntryDiff::Diff { actual, expected } => {
                                // TODO: match column
                            }
                        }
                    }

                    if !annotations.is_empty() || !db_only.is_empty() {
                        report.push(
                            Level::ERROR
                                .primary_title("column mismatch for table")
                                .element(
                                    Snippet::source(source)
                                        .path(path)
                                        .annotation(
                                            AnnotationKind::Primary
                                                .span(span)
                                                .label("in this table"),
                                        )
                                        .annotations(annotations),
                                )
                                .elements(db_only),
                        );
                    }
                }
            };
        }
        if !db_only.is_empty() {
            report.push(
                Level::ERROR
                    .no_name()
                    .primary_title("there are undefined tables in the database")
                    .elements(db_only),
            );
        }

        report
    }
}
