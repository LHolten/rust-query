use std::time::SystemTime;

use rand::{Rng, rngs::ThreadRng};
use rust_query::{
    Database, IntoExpr, Select, TableRow, Transaction,
    migration::{Config, schema},
};

mod delivery;
mod new_order;
mod order_status;
mod payment;
mod populate;

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

const DB_FILE: &'static str = "tpc.sqlite";
fn main() {
    if std::fs::exists(DB_FILE).unwrap() {
        std::fs::remove_file(DB_FILE).unwrap();
    };

    let db: Database<Schema> = Database::migrator(Config::open(DB_FILE))
        .expect("database should not be too old")
        .finish()
        .expect("database should not be too new");

    db.transaction_mut_ok(|txn| {
        populate::populate(txn, 1);
    });

    let _ = db.transaction_mut_ok(|txn| {
        let warehouse = get_primary_warehouse(txn);
        new_order::random_new_order(txn, warehouse)
            .map(|_| ())
            .map_err(|_| ())
    });

    db.transaction_mut_ok(|txn| {
        let warehouse = get_primary_warehouse(txn);
        delivery::random_delivery(txn, warehouse);
    });

    db.transaction_mut_ok(|txn| {
        let warehouse = get_primary_warehouse(txn);
        payment::random_payment(txn, warehouse);
    });

    db.transaction(|txn| {
        let warehouse = get_primary_warehouse(txn);
        order_status::random_order_status(txn, warehouse);
    });
}

fn get_primary_warehouse(txn: &Transaction<Schema>) -> TableRow<Warehouse> {
    txn.query_one(Warehouse::unique(1))
        .expect("warehouse should exist")
}

trait NuRand {
    fn nurand(&mut self, a: i64, x: i64, y: i64) -> i64;
}
impl NuRand for ThreadRng {
    fn nurand(&mut self, a: i64, x: i64, y: i64) -> i64 {
        // TODO: select C at runtime?
        const C: i64 = 5;
        (((self.random_range(0..=a) | self.random_range(x..=y)) + C) % (y - x + 1)) + x
    }
}

/// `num` must be in range `0..=999`
pub fn random_to_last_name(num: i64) -> String {
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
    Number(TableRow<Customer>),
    Name(TableRow<District>, String),
}

impl CustomerIdent {
    pub fn lookup_customer(self, txn: &Transaction<Schema>) -> TableRow<Customer> {
        match self {
            CustomerIdent::Number(customer) => customer,
            CustomerIdent::Name(district, last_name) => {
                let mut customers = txn.query(|rows| {
                    let customer = rows.join(Customer);
                    rows.filter(customer.district.eq(district));
                    rows.filter(customer.last.eq(last_name));
                    rows.into_vec((&customer.first, &customer))
                });
                customers.sort_by(|a, b| a.0.cmp(&b.0));

                let count = customers.len();
                let id = count.div_ceil(2) - 1;
                customers.swap_remove(id).1
            }
        }
    }
}

fn customer_ident(
    txn: &Transaction<Schema>,
    rng: &mut ThreadRng,
    customer_district: TableRow<District>,
) -> CustomerIdent {
    if rng.random_ratio(60, 100) {
        CustomerIdent::Name(
            customer_district,
            random_to_last_name(rng.nurand(255, 0, 999)),
        )
    } else {
        let customer = txn
            .query_one(Customer::unique(
                customer_district,
                rng.nurand(1023, 1, 3000),
            ))
            .unwrap();
        CustomerIdent::Number(customer)
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
