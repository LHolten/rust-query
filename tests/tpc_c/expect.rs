pub fn collect_all<R>(f: impl FnMut() -> R) -> R {
    let (res, plans) = rust_query::private::get_plan(f);

    for (sql, plan) in plans {
        assert!(sql.starts_with("INSERT INTO"));

        expect_test::expect![[r#"
            QUERY PLAN [
                SCAN CONSTANT ROW,
            ]
        "#]]
        .assert_debug_eq(&plan);
    }

    res
}
