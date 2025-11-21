//! You can run this benchmark as `cargo bench -- {num_warehouses}`.
//! It will run with increasingly more warehouses and users.
//! The benchmark stops when any of the transaction types has more than 10% late.
//! At that point you can read the previous number of `new_order` transactions executed
//! divided by the number of minutes (default 2) as the approximate tpmC.
//! Note that for a real measurement the performance needs to be measured for several hours.

use std::{
    env::args,
    ops::RangeInclusive,
    sync::{Arc, OnceLock},
    thread,
    time::{Duration, SystemTime},
};

use rust_query::{
    Database, FromExpr, IntoExpr, IntoSelect, Select, Table, TableRow, Transaction, aggregate,
    migration::{Config, schema},
    optional,
};

mod delivery;
mod emulated_user;
mod expect;
mod new_order;
mod order_status;
mod payment;
mod populate;
mod stock_level;

#[schema(Schema)]
pub mod vN {
    #[unique(number)]
    pub struct Warehouse {
        pub number: i64,
        pub name: String,
        pub street_1: String,
        pub street_2: String,
        pub city: String,
        pub state: String,
        pub zip: String,
        pub tax: f64,
        // stored multiplied by 100
        pub ytd: i64,
    }
    #[unique(warehouse, number)]
    pub struct District {
        pub warehouse: Warehouse,
        pub number: i64,
        pub name: String,
        pub street_1: String,
        pub street_2: String,
        pub city: String,
        pub state: String,
        pub zip: String,
        pub tax: f64,
        // stored multiplied by 100
        pub ytd: i64,
        pub next_order: i64, // next available order id
    }
    #[unique(district, number)]
    pub struct Customer {
        pub district: District,
        pub number: i64,
        pub first: String,
        pub middle: String,
        pub last: String,
        pub street_1: String,
        pub street_2: String,
        pub city: String,
        pub state: String,
        pub zip: String,
        pub phone: String,
        pub since: i64,
        pub credit: String,
        // stored multiplied by 100
        pub credit_lim: i64,
        pub discount: f64,
        // stored multiplied by 100
        pub balance: i64,
        // stored multiplied by 100
        pub ytd_payment: i64,
        pub payment_cnt: i64,
        pub delivery_cnt: i64,
        pub data: String,
    }
    pub struct History {
        pub customer: Customer,
        pub district: District,
        pub date: i64,
        // stored multiplied by 100
        pub amount: i64,
        pub data: String,
    }
    #[no_reference]
    pub struct NewOrder {
        #[unique]
        pub order: Order,
    }
    #[unique(customer, number)]
    pub struct Order {
        pub customer: Customer,
        pub number: i64,
        pub entry_d: i64,
        pub carrier_id: Option<i64>,
        pub order_line_cnt: i64,
        pub all_local: i64,
    }
    #[unique(order, number)]
    pub struct OrderLine {
        pub order: Order,
        pub number: i64,
        pub stock: Stock,
        pub delivery_d: Option<i64>,
        pub quantity: i64,
        // stored multiplied by 100
        pub amount: i64, // total cost of this line
        pub dist_info: String,
    }
    #[unique(number)]
    pub struct Item {
        pub number: i64,
        pub image_id: i64,
        pub name: String,
        // stored multiplied by 100
        pub price: i64,
        pub data: String,
    }
    #[unique(warehouse, item)]
    pub struct Stock {
        pub warehouse: Warehouse,
        pub item: Item,
        pub quantity: i64,
        pub dist_00: String,
        pub dist_01: String,
        pub dist_02: String,
        pub dist_03: String,
        pub dist_04: String,
        pub dist_05: String,
        pub dist_06: String,
        pub dist_07: String,
        pub dist_08: String,
        pub dist_09: String,
        pub dist_10: String,
        // stored multiplied by 100
        pub ytd: i64,
        pub order_cnt: i64,
        pub remote_cnt: i64,
        pub data: String,
    }
}
use v0::*;

use crate::emulated_user::{Emulate, EmulateWithQueue, print_stats, reset_ok, stop_emulation};

const DB_FILE: &'static str = "tpc.sqlite";

fn main() {
    // every warehouse is ~70MB
    let warehouse_cnt = args()
        .skip(1) // skip binary name
        .filter(|x| x != "--bench")
        .next()
        .map(|x| x.parse().unwrap())
        .unwrap_or(50);

    let mut config = Config::open(DB_FILE);
    config.foreign_keys = rust_query::migration::ForeignKeys::Rust;
    let db = Database::new(config);
    let db = Arc::new(db);

    for warehouse_cnt in (warehouse_cnt..).step_by(10) {
        println!("testing with {warehouse_cnt} warehouses");
        if !test_cnt(db.clone(), warehouse_cnt) {
            return;
        }
    }
}

fn test_cnt(db: Arc<Database<Schema>>, warehouse_cnt: i64) -> bool {
    db.transaction_mut_ok(|txn| {
        let warehouses_exist = txn.query_one(aggregate(|rows| {
            let warehouse = rows.join(Warehouse);
            rows.max(&warehouse.number).unwrap_or(0)
        }));
        expect::collect_all(|| {
            populate::populate(txn, warehouses_exist..warehouse_cnt);
        });
    });
    println!("initialization complete");

    let mut threads = vec![];
    for warehouse in 1..=warehouse_cnt {
        for district in 1..=10 {
            let db = db.clone();
            threads.push(thread::spawn(move || {
                EmulateWithQueue {
                    info: Arc::new(Emulate {
                        db,
                        warehouse,
                        district,
                        other_warehouses: (1..=warehouse_cnt).filter(|x| x != &warehouse).collect(),
                    }),
                    queue: vec![],
                }
                .loop_emulate();
            }));
        }
    }

    // warmup
    let duration = Duration::from_secs(30);
    thread::sleep(duration);
    println!("warmup complete");

    reset_ok();

    let duration = Duration::from_secs(90);
    thread::sleep(duration);

    println!("benchmark complete");
    stop_emulation();
    for thread in threads {
        thread.join().unwrap();
    }

    print_stats(duration)
}

enum Nu {
    LastNameLoad,
    LastNameRun,
    CustomerId,
    ItemId,
}

struct NuStats {
    a: i64,
    range: RangeInclusive<i64>,
    c: i64,
}

impl Nu {
    fn stats(self) -> NuStats {
        static C_LOAD: OnceLock<i64> = OnceLock::new();
        static C_RUN: OnceLock<i64> = OnceLock::new();
        static C_CUSTOMER: OnceLock<i64> = OnceLock::new();
        static C_ITEM: OnceLock<i64> = OnceLock::new();
        let (a, range, c) = match self {
            Nu::LastNameLoad => (255, 0..=999, &C_LOAD),
            Nu::LastNameRun => (255, 0..=999, &C_RUN),
            Nu::CustomerId => (1023, 1..=3000, &C_CUSTOMER),
            Nu::ItemId => (8191, 1..=100_000, &C_ITEM),
        };

        let c = match self {
            Nu::LastNameRun => *c.get_or_init(|| {
                let c_load = Nu::LastNameLoad.stats().c;
                loop {
                    let candidate = rand::random_range(0..=a);
                    let c_diff = c_load.abs_diff(candidate);
                    if (65..=119).contains(&c_diff) && c_diff != 96 && c_diff != 112 {
                        break candidate;
                    }
                }
            }),
            _ => *c.get_or_init(|| rand::random_range(0..=a)),
        };
        NuStats { a, range, c }
    }

    fn rand(self) -> i64 {
        let NuStats { a, range, c } = self.stats();
        let (x, y) = (*range.start(), *range.end());

        (((rand::random_range(0..=a) | rand::random_range(x..=y)) + c) % (y - x + 1)) + x
    }
}

/// `num` must be in range `0..=999`
fn random_to_last_name(num: i64) -> String {
    assert!((0..=999).contains(&num));

    let mut out = String::new();
    for position in [100, 10, 1] {
        let digit = (num / position) % 10;
        out.push_str(
            [
                "BAR", "OUGHT", "ABLE", "PRI", "PRES", "ESE", "ANTI", "CALLY", "ATION", "EING",
            ][digit as usize],
        );
    }
    out
}

enum CustomerIdent {
    Number(i64),
    Name(String),
}

impl CustomerIdent {
    fn lookup_customer<O: FromExpr<Schema, Customer>>(
        self,
        txn: &Transaction<Schema>,
        warehouse: i64,
        district: i64,
    ) -> O {
        match self {
            CustomerIdent::Number(customer) => txn
                .query_one(optional(|row| {
                    let warehouse = row.and(Warehouse.number(warehouse));
                    let district = row.and(District.warehouse(warehouse).number(district));
                    Option::<O>::from_expr(
                        row.and_then(Customer.district(district).number(customer)),
                    )
                }))
                .unwrap(),
            CustomerIdent::Name(last_name) => {
                let mut customers = txn.query(|rows| {
                    let warehouse = rows.filter_some(Warehouse.number(warehouse));
                    let district = rows.filter_some(District.warehouse(warehouse).number(district));

                    let customer = rows.join(Customer.district(district));
                    rows.filter(customer.last.eq(last_name));
                    rows.into_vec((&customer.first, FromExpr::from_expr(&customer)))
                });
                customers.sort_by(|a, b| a.0.cmp(&b.0));

                let count = customers.len();
                let id = count.div_ceil(2) - 1;
                customers.swap_remove(id).1
            }
        }
    }
}

fn customer_ident() -> CustomerIdent {
    if rand::random_ratio(60, 100) {
        CustomerIdent::Name(random_to_last_name(Nu::LastNameRun.rand()))
    } else {
        CustomerIdent::Number(Nu::CustomerId.rand())
    }
}

impl<'column> IntoExpr<'column, Schema> for SystemTime {
    type Typ = i64;

    fn into_expr(self) -> rust_query::Expr<'column, Schema, Self::Typ> {
        let millis = self
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        (millis as i64).into_expr()
    }
}

struct WithId<T: Table, F: FromExpr<Schema, T>> {
    info: F,
    row: TableRow<T>,
}

impl<T: Table, F: FromExpr<Schema, T>> std::ops::Deref for WithId<T, F> {
    type Target = F;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}
impl<T: Table<Schema = Schema>, F: FromExpr<Schema, T>> FromExpr<Schema, T> for WithId<T, F> {
    fn from_expr<'columns>(
        col: impl IntoExpr<'columns, Schema, Typ = T>,
    ) -> Select<'columns, Schema, Self> {
        let col = col.into_expr();
        (&col, F::from_expr(&col))
            .into_select()
            .map(|(row, info)| Self { info, row })
    }
}
