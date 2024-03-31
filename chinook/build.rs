use std::{env, fs, path::Path};

use rust_query::{client::Client, schema::generate};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("tables.rs");

    let client = Client::open_in_memory();
    client.execute_batch(&fs::read_to_string("Chinook_Sqlite.sql").unwrap());
    client.execute_batch(&fs::read_to_string("migrate.sql").unwrap());
    let code = generate(client);
    fs::write(dest_path, code).unwrap();

    println!("cargo::rerun-if-changed=Chinook_Sqlite.sql");
    println!("cargo::rerun-if-changed=migrate.sql");
}
