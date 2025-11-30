use super::*;
use rand::seq::IndexedRandom;
use rust_query::{FromExpr, Transaction};

pub fn generate_input(warehouse: i64, other: &[i64]) -> PaymentInput {
    let district = rand::random_range(1..=10);

    let (customer_warehouse, customer_district);
    if rand::random_ratio(85, 100) || other.is_empty() {
        (customer_district, customer_warehouse) = (district, warehouse);
    } else {
        customer_warehouse = *other.choose(&mut rand::rng()).expect("other is not empty");
        customer_district = rand::random_range(1..=10);
    };

    PaymentInput {
        warehouse,
        district,
        customer: customer_ident(),
        customer_warehouse,
        customer_district,
        amount: rand::random_range(100..=500000),
        date: SystemTime::now(),
    }
}

pub struct PaymentInput {
    warehouse: i64,
    district: i64,
    customer: CustomerIdent,
    customer_warehouse: i64,
    customer_district: i64,
    amount: i64,
    date: SystemTime,
}

pub fn payment(txn: &mut Transaction<Schema>, input: PaymentInput) -> PaymentOutput {
    let mut warehouse = txn.mutable(Warehouse.number(input.warehouse)).unwrap();
    warehouse.ytd += input.amount;
    let warehouse_name = warehouse.name.clone();
    let warehouse = warehouse.table_row();

    let mut district = txn
        .mutable(District.warehouse(warehouse).number(input.district))
        .unwrap();
    district.ytd += input.amount;
    let district_name = district.name.clone();
    let district = district.table_row();

    let customer: TableRow<Customer> =
        input
            .customer
            .lookup_customer(txn, input.customer_warehouse, input.customer_district);

    let mut customer = txn.mutable(customer);
    customer.ytd_payment += input.amount;
    customer.payment_cnt += 1;

    let mut credit_data = None;
    if customer.credit == "BC" {
        let mut data = format!(
            "{},{},{},{},{},{};{}",
            customer.number,
            input.customer_district,
            input.customer_warehouse,
            input.district,
            input.warehouse,
            input.amount,
            customer.data
        );
        data.truncate(500);
        customer.data = data.clone();
        data.truncate(200);
        credit_data = Some(data);
    }

    let customer_number = customer.number;
    let customer = customer.table_row();

    txn.insert_ok(History {
        customer,
        district,
        date: input.date,
        amount: input.amount,
        data: format!("{}    {}", warehouse_name, district_name),
    });

    PaymentOutput {
        warehouse: input.warehouse,
        district: input.district,
        customer: customer_number,
        customer_district: input.customer_district,
        customer_warehouse: input.customer_warehouse,
        warehouse_info: txn.query_one(LocationYtd::from_expr(warehouse)),
        district_info: txn.query_one(LocationYtd::from_expr(district)),
        customer_info: txn.query_one(CustomerInfo::from_expr(customer)),
        data: credit_data,
        amount: input.amount,
        date: input.date,
    }
}

#[expect(unused)]
#[derive(FromExpr)]
#[rust_query(From = Warehouse, From = District)]
struct LocationYtd {
    #[doc(hidden)]
    name: String,

    street_1: String,
    street_2: String,
    city: String,
    state: String,
    zip: String,
}

#[expect(unused)]
#[derive(FromExpr)]
#[rust_query(From = Customer)]
struct CustomerInfo {
    #[doc(hidden)]
    number: i64,
    #[doc(hidden)]
    data: String,

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

#[expect(unused)]
pub struct PaymentOutput {
    warehouse: i64,
    district: i64,
    customer: i64,
    customer_district: i64,
    customer_warehouse: i64,
    warehouse_info: LocationYtd,
    district_info: LocationYtd,
    customer_info: CustomerInfo,
    data: Option<String>,
    amount: i64,
    date: SystemTime,
}
