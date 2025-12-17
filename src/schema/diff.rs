use std::{collections::BTreeMap, mem::take};

use annotate_snippets::{AnnotationKind, Group, Level, Snippet};

use crate::schema::{from_db, from_macro};

pub enum EntryDiff<A, E> {
    DbOnly(E),
    MacroOnly(A),
    Diff { from_macro: A, from_db: E },
}

pub fn diff_map<K: Ord, M, D>(
    mut from_macro: BTreeMap<K, M>,
    from_db: BTreeMap<K, D>,
) -> BTreeMap<K, EntryDiff<M, D>> {
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
        schema_version: i64,
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
                    report.extend(from_db.diff(from_macro, source, path, schema_version));
                }
            };
        }

        if !annotations.is_empty() || !db_only.is_empty() {
            let span = || from_macro.span.0..from_macro.span.1;
            let snippet = Snippet::source(source)
                .path(path)
                .annotations(
                    db_only
                        .is_empty()
                        .then(|| AnnotationKind::Context.span(span()).label("in this schema")),
                )
                .annotations(db_only.iter().map(|table| {
                    AnnotationKind::Primary
                        .span(span())
                        .label(format!("database has table `{table}`"))
                }))
                .annotations(annotations);
            report.push(
                Level::ERROR
                    .primary_title(format!("Table mismatch for `#[version({schema_version})]`"))
                    .element(snippet),
            );
        }

        report
    }
}

impl from_db::Table {
    fn diff<'a>(
        self,
        from_macro: from_macro::Table,
        source: &'a str,
        path: &'a str,
        schema_version: i64,
    ) -> Vec<Group<'a>> {
        let mut annotations = Vec::new();
        let mut db_only = Vec::new();

        let span = || from_macro.span.0..from_macro.span.1;

        for (col, diff) in diff_map(from_macro.columns, self.columns) {
            match diff {
                EntryDiff::DbOnly(column) => {
                    db_only.push(AnnotationKind::Primary.span(span()).label(format!(
                        "database has column `{col}: {}`",
                        column.render_rust()
                    )))
                }
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
                    if from_db.parse_typ() == Some(from_macro.def.typ)
                        && from_db.nullable == from_macro.def.nullable
                        && from_db.fk == from_macro.def.fk
                        && from_db.check == from_macro.def.check
                    {
                        continue;
                    }
                    annotations.push(AnnotationKind::Primary.span(span).label(format!(
                        "database column has type `{}`",
                        from_db.render_rust()
                    )));
                }
            }
        }

        let mut out = Vec::new();
        if !annotations.is_empty() || !db_only.is_empty() {
            let context = db_only
                .is_empty()
                .then(|| AnnotationKind::Context.span(span()).label("in this table"));
            let snippet = Snippet::source(source)
                .path(path)
                .annotations(context)
                .annotations(take(&mut annotations))
                .annotations(take(&mut db_only));
            let title = format!("Column mismatch for `#[version({schema_version})]`");
            out.push(Level::ERROR.primary_title(title).element(snippet))
        }

        let macro_indices = from_macro
            .indices
            .into_iter()
            .filter_map(|i| Some((i.def.normalize()?, i.span)))
            .collect();
        let db_indices = self
            .indices
            .into_values()
            .filter_map(|i| Some((i.normalize()?, ())))
            .collect();

        for (unique, diff) in diff_map(macro_indices, db_indices) {
            match diff {
                EntryDiff::DbOnly(()) => {
                    let columns: Vec<_> = unique.columns.iter().map(|s| s.as_str()).collect();
                    db_only.push(
                        AnnotationKind::Primary
                            .span(span())
                            .label(format!("database has `#[unique({})]`", columns.join(", "))),
                    )
                }
                EntryDiff::MacroOnly(span) => {
                    let span = span.0..span.1;
                    annotations.push(
                        AnnotationKind::Primary
                            .span(span)
                            .label("database does not have this unique constraint"),
                    );
                }
                EntryDiff::Diff { .. } => {}
            }
        }

        if !annotations.is_empty() || !db_only.is_empty() {
            let context = db_only
                .is_empty()
                .then(|| AnnotationKind::Context.span(span()).label("in this table"));
            let snippet = Snippet::source(source)
                .path(path)
                .annotations(context)
                .annotations(annotations)
                .annotations(db_only);
            let title = format!("Unique constraint mismatch for `#[version({schema_version})]`");
            out.push(Level::ERROR.primary_title(title).element(snippet));
        }
        out
    }
}
