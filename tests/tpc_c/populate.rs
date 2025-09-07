use std::iter::{self, zip};

use rand::seq::{IndexedRandom, IteratorRandom, SliceRandom};
use rust_query::{TableRow, Transaction, UnixEpoch};

use crate::{
    Nu, random_to_last_name,
    v0::{Customer, District, History, Item, NewOrder, Order, OrderLine, Schema, Stock, Warehouse},
};

/// String of alphanumeric characters
fn a_string(min_len: usize, max_len: usize) -> String {
    iter::repeat_with(|| {
        ('a'..='z')
            .chain('A'..='Z')
            .chain('0'..='9')
            .choose(&mut rand::rng())
            .unwrap()
    })
    .take(rand::random_range(min_len..=max_len))
    .collect()
}

/// String of numbers
fn n_string(min_len: usize, max_len: usize) -> String {
    iter::repeat_with(|| ('0'..='9').choose(&mut rand::rng()).unwrap())
        .take(rand::random_range(min_len..=max_len))
        .collect()
}

fn zip_code() -> String {
    n_string(4, 4) + "11111"
}

fn data() -> String {
    let mut data = a_string(26, 50);
    if rand::random_ratio(10, 100) {
        let start = rand::random_range(0..data.len() - 8);
        data.replace_range(start..start + 8, "ORIGINAL");
    }
    data
}

pub fn populate(txn: &mut Transaction<Schema>, warehouse_cnt: usize) {
    let items: Box<[_]> = (1..=100_000)
        .map(|number| {
            txn.insert(Item {
                number: number as i64,
                image_id: rand::random_range(1..=10_000),
                name: a_string(14, 24),
                price: rand::random_range(1 * 100..=100 * 100),
                data: data(),
            })
            .expect("number is unique")
        })
        .collect();

    for number in 1..=warehouse_cnt as i64 {
        let warehouse = txn
            .insert(Warehouse {
                number,
                name: a_string(6, 10),
                street_1: a_string(10, 20),
                street_2: a_string(10, 20),
                city: a_string(10, 20),
                state: a_string(2, 2),
                zip: zip_code(),
                tax: rand::random_range(0.0..=0.2),
                ytd: 300_000 * 100,
            })
            .expect("number is unique");
        populate_warehouse(txn, warehouse, &items);
    }
}

fn populate_warehouse(
    txn: &mut Transaction<Schema>,
    warehouse: TableRow<Warehouse>,
    items: &[TableRow<Item>],
) {
    let stock: Box<_> = items
        .iter()
        .map(|item| {
            txn.insert(Stock {
                warehouse,
                item,
                quantity: rand::random_range(10..=100),
                dist_00: a_string(24, 24),
                dist_01: a_string(24, 24),
                dist_02: a_string(24, 24),
                dist_03: a_string(24, 24),
                dist_04: a_string(24, 24),
                dist_05: a_string(24, 24),
                dist_06: a_string(24, 24),
                dist_07: a_string(24, 24),
                dist_08: a_string(24, 24),
                dist_09: a_string(24, 24),
                dist_10: a_string(24, 24),
                ytd: 0,
                order_cnt: 0,
                remote_cnt: 0,
                data: data(),
            })
            .expect("warehouse + item is unique")
        })
        .collect();

    for number in 1..=10 {
        let district = txn
            .insert(District {
                warehouse,
                number,
                name: a_string(6, 10),
                street_1: a_string(10, 20),
                street_2: a_string(10, 20),
                city: a_string(10, 20),
                state: a_string(2, 2),
                zip: zip_code(),
                tax: rand::random_range(0.0..=0.2),
                ytd: 30_000 * 100,
                next_order: 3001,
            })
            .expect("warehouse + number is unique");

        populate_district(txn, district, &stock);
    }
}

fn populate_district(
    txn: &mut Transaction<Schema>,
    district: TableRow<District>,
    stock: &[TableRow<Stock>],
) {
    let mut customers = vec![];
    for number in 1..=3000 {
        let customer = txn
            .insert(Customer {
                district,
                number,
                first: a_string(8, 16),
                middle: "OE",
                last: if number < 1001 {
                    random_to_last_name(number - 1)
                } else {
                    random_to_last_name(Nu::LastNameLoad.rand())
                },
                street_1: a_string(10, 20),
                street_2: a_string(10, 20),
                city: a_string(10, 20),
                state: a_string(2, 2),
                zip: zip_code(),
                phone: n_string(16, 16),
                since: UnixEpoch,
                credit: if rand::random_ratio(10, 100) {
                    "BC"
                } else {
                    "GC"
                },
                credit_lim: 50_000 * 100,
                discount: rand::random_range(0.0..=0.5),
                balance: -10 * 100,
                ytd_payment: 10 * 100,
                payment_cnt: 1,
                delivery_cnt: 0,
                data: a_string(300, 500),
            })
            .expect("district + customer is unique");

        txn.insert_ok(History {
            customer,
            district,
            date: UnixEpoch,
            amount: 10 * 100,
            data: a_string(12, 24),
        });
        customers.push(customer);
    }

    customers.shuffle(&mut rand::rng());

    for (order_number, customer) in zip(1.., customers) {
        let delivered = order_number < 2101;

        let order_line_cnt = rand::random_range(5..=15);
        let order = txn
            .insert(Order {
                customer,
                number: order_number as i64,
                entry_d: UnixEpoch,
                carrier_id: delivered.then_some(rand::random_range(1..=10)),
                order_line_cnt,
                all_local: 1,
            })
            .expect("customer + number is unique");

        for line_number in 1..=order_line_cnt {
            txn.insert(OrderLine {
                order,
                number: line_number,
                stock: stock
                    .choose(&mut rand::rng())
                    .expect("stock array is not empty"),
                delivery_d: delivered.then_some(UnixEpoch),
                quantity: 5,
                amount: if delivered {
                    0
                } else {
                    rand::random_range(1..=999999)
                },
                dist_info: a_string(24, 24),
            })
            .expect("order + number is unique");
        }

        if !delivered {
            txn.insert(NewOrder { order }).expect("order is unique");
        }
    }
}
