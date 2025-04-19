use super::*;
use rust_query::{FromExpr, TableRow, Transaction, TransactionMut, Update};
use std::time::UNIX_EPOCH;

pub fn generate_payment<'a>(
    txn: &Transaction<'a, Schema>,
    warehouse: TableRow<'a, Warehouse>,
) -> PaymentInput<'a> {
    let mut rng = rand::rng();
    let district = txn
        .query_one(District::unique(warehouse, rng.random_range(1..=10)))
        .unwrap();

    let customer_district = if rng.random_ratio(85, 100) {
        district
    } else {
        // TODO: select a different warehouse here
        txn.query_one(District::unique(warehouse, rng.random_range(1..=10)))
            .unwrap()
    };

    let customer = customer_ident(txn, &mut rng, customer_district);

    PaymentInput {
        district,
        customer,
        amount: rng.random_range(100..=500000),
        date: UNIX_EPOCH.elapsed().unwrap().as_millis() as i64,
    }
}

struct PaymentInput<'a> {
    district: TableRow<'a, District>,
    customer: CustomerIdent<'a>,
    amount: i64,
    date: i64,
}

#[derive(FromExpr)]
#[rust_query(From = Warehouse, From = District)]
struct LocationYtd {
    name: String,
    street_1: String,
    street_2: String,
    city: String,
    state: String,
    zip: String,
    ytd: i64,
}

#[derive(FromExpr)]
#[rust_query(From = Customer)]
struct CustomerInfo {
    first: String,
    middle: String,
    last: String,
    street_1: String,
    street_2: String,
    city: String,
    state: String,
    zip: String,
    phone: String,
    since: i64,
    credit: String,
    credit_lim: i64,
    discount: f64,
    balance: i64,
}

fn payment<'a>(mut txn: TransactionMut<'a, Schema>, input: PaymentInput<'a>) -> PaymentOutput<'a> {
    let district = input.district;
    let warehouse = district.warehouse();
    let warehouse_info = txn.query_one(LocationYtd::from_expr(&warehouse));

    txn.update_ok(
        &warehouse,
        Warehouse {
            ytd: Update::add(input.amount),
            ..Default::default()
        },
    );

    let district_info = txn.query_one(LocationYtd::from_expr(district));

    txn.update_ok(
        district,
        District {
            ytd: Update::add(input.amount),
            ..Default::default()
        },
    );

    let customer = input.customer.lookup_customer(&txn);
    let customer_info: CustomerInfo = txn.query_one(FromExpr::from_expr(customer));

    txn.update_ok(
        customer,
        Customer {
            ytd_payment: Update::add(input.amount),
            payment_cnt: Update::add(1),
            ..Default::default()
        },
    );

    let mut credit_data = None;
    if customer_info.credit == "BC" {
        let data = txn.query_one(customer.data());
        let mut data = format!("{customer:?},{};{data}", input.amount);
        txn.update_ok(
            customer,
            Customer {
                data: Update::set(&data[..500]),
                ..Default::default()
            },
        );
        data.truncate(200);
        credit_data = Some(data);
    }

    let data = format!("{}    {}", warehouse_info.name, district_info.name);
    txn.insert_ok(History {
        customer,
        district,
        date: input.date,
        amount: input.amount,
        data,
    });

    txn.commit();

    PaymentOutput {
        district,
        customer,
        warehouse_info,
        district_info,
        customer_info,
        data: credit_data,
        amount: input.amount,
        date: input.date,
    }
}

struct PaymentOutput<'a> {
    district: TableRow<'a, District>,
    customer: TableRow<'a, Customer>,
    warehouse_info: LocationYtd,
    district_info: LocationYtd,
    customer_info: CustomerInfo,
    data: Option<String>,
    amount: i64,
    date: i64,
}
