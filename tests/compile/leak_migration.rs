use rust_query::{
    LocalClient,
    migration::{Config, Migrate, schema},
};

#[schema]
#[version(0..=1)]
enum Schema {
    MyTable {
        #[version(1..)]
        col: i64,
    },
}

fn migrate<'t>(client: &mut LocalClient) {
    let m = client.migrator(Config::open_in_memory()).unwrap();

    let mut sneaky = None;
    m.migrate(|_| v1::update::Schema {
        my_table: Migrate::<v1::MyTable>::all(|prev| {
            sneaky = Some(prev);
            todo!()
        }),
    });
}

fn main() {}
