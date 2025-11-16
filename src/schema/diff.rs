use std::collections::BTreeMap;

use annotate_snippets::{AnnotationKind, Group, Level, Snippet};

use crate::schema::{from_db, from_macro};

pub enum EntryDiff<A, E> {
    DbOnly(E),
    MacroOnly(A),
    Diff { from_macro: A, from_db: E },
}

pub fn diff_map<A, E>(
    mut from_macro: BTreeMap<String, A>,
    from_db: BTreeMap<String, E>,
) -> BTreeMap<String, EntryDiff<A, E>> {
    let mut out = BTreeMap::new();
    for (key, from_db) in from_db {
        match from_macro.remove(&key) {
            Some(from_macro) => {
                out.insert(
                    key,
                    EntryDiff::Diff {
                        from_macro,
                        from_db,
                    },
                );
            }
            None => {
                out.insert(key, EntryDiff::DbOnly(from_db));
            }
        }
    }
    for (key, actual) in from_macro {
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
        let mut annotations = Vec::new();
        let mut report = Vec::new();

        for (table, diff) in diff_map(from_macro.tables, self.tables) {
            match diff {
                EntryDiff::DbOnly(_) => db_only.push(table),
                EntryDiff::MacroOnly(val) => {
                    let span = val.span.0..val.span.1;
                    annotations.push(
                        AnnotationKind::Primary
                            .span(span)
                            .label("database does not have this table"),
                    );
                }
                EntryDiff::Diff {
                    from_macro,
                    from_db,
                } => {
                    let mut db_only = Vec::new();
                    let mut annotations = Vec::new();

                    for (col, diff) in diff_map(from_macro.columns, from_db.columns) {
                        match diff {
                            EntryDiff::DbOnly(_) => db_only.push(col),
                            EntryDiff::MacroOnly(column) => {
                                let span = column.span.0..column.span.1;
                                annotations.push(
                                    AnnotationKind::Primary
                                        .span(span)
                                        .label("database does not have this column"),
                                );
                            }
                            EntryDiff::Diff {
                                from_macro,
                                from_db,
                            } => {
                                let span = from_macro.span.0..from_macro.span.1;
                                if from_db.parse_typ() == Ok(from_macro.def.typ)
                                    && from_db.nullable == from_macro.def.nullable
                                    && from_db.fk == from_macro.def.fk
                                {
                                    continue;
                                }
                                annotations.push(
                                    AnnotationKind::Primary.span(span).label(format!(
                                        "database has type {}",
                                        from_db.render_rust()
                                    )),
                                );
                            }
                        }
                    }

                    if !annotations.is_empty() || !db_only.is_empty() {
                        let span = || from_macro.span.0..from_macro.span.1;
                        report.push(
                            Level::ERROR.primary_title("column mismatch").element(
                                Snippet::source(source)
                                    .path(path)
                                    .annotations(db_only.is_empty().then(|| {
                                        AnnotationKind::Context.span(span()).label("in this table")
                                    }))
                                    .annotations(db_only.iter().map(|col| {
                                        AnnotationKind::Primary
                                            .span(span())
                                            .label(format!("database has `{col}` column"))
                                    }))
                                    .annotations(annotations),
                            ),
                        );
                    }
                }
            };
        }

        if !annotations.is_empty() || !db_only.is_empty() {
            let span = || from_macro.span.0..from_macro.span.1;
            report.push(
                Level::ERROR.primary_title("table mismatch").element(
                    Snippet::source(source)
                        .path(path)
                        .annotations(
                            db_only.is_empty().then(|| {
                                AnnotationKind::Context.span(span()).label("in this schema")
                            }),
                        )
                        .annotations(db_only.iter().map(|table| {
                            AnnotationKind::Primary
                                .span(span())
                                .label(format!("database has `{table}` table"))
                        }))
                        .annotations(annotations),
                ),
            );
        }

        report
    }
}
