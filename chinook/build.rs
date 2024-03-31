use std::{env, fs, path::Path};

use rust_query::schema::generate;

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("tables.rs");

    let code = generate();
    fs::write(dest_path, code).unwrap();
}
