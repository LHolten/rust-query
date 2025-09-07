use std::array;

use rust_query::{TableRow, Transaction, UnixEpoch};

use crate::{
    NuRand, random_to_last_name,
    v0::{Customer, District, Item, Schema, Stock, Warehouse},
};

/// String of alphanumeric characters
fn a_string(min_len: usize, max_len: usize) -> String {
    todo!()
}

/// String of numbers
fn n_string(min_len: usize, max_len: usize) -> String {
    todo!()
}

pub fn zip_code() -> String {
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
    let items = Box::new(array::from_fn(|number| {
        txn.insert(Item {
            number: number as i64,
            image_id: rand::random_range(1..=10_000),
            name: a_string(14, 24),
            price: rand::random_range(1 * 100..=100 * 100),
            data: data(),
        })
        .expect("number is unique")
    }));

    for number in 0..warehouse_cnt as i64 {
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

pub fn populate_warehouse(
    txn: &mut Transaction<Schema>,
    warehouse: TableRow<Warehouse>,
    items: &[TableRow<Item>; 100_000],
) {
    for item in items {
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
        .expect("warehouse + item is unique");
    }

    for number in 0..10 {
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

        populate_district(txn, district);
    }
}

fn populate_district(txn: &mut Transaction<Schema>, district: TableRow<District>) {
    for number in 0..3000 {
        txn.insert(Customer {
            district,
            number,
            first: a_string(8, 16),
            middle: "OE",
            last: if number < 1000 {
                random_to_last_name(number)
            } else {
                let mut rng = rand::rng();
                // TODO: choose different constant C
                random_to_last_name(rng.nurand(255, 0, 999))
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
    }
}
