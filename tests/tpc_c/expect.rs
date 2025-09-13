use expect_test::expect_file;

pub fn collect_all<R>(f: impl FnMut() -> R) -> R {
    let (res, plans) = rust_query::private::get_plan(f);

    for (sql, plan) in plans {
        let table_name = sql
            .strip_prefix("INSERT INTO \"")
            .unwrap()
            .split_once('"')
            .unwrap()
            .0;

        let path = format!("expect/{table_name}.plan");
        expect_file![path].assert_debug_eq(&plan);
    }

    res
}
