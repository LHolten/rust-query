use super::*;
use rand::seq::IndexedRandom;
use rust_query::{FromExpr, Transaction, Update, optional};

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

type WarehouseInfo = WithId<Warehouse, LocationYtd>;
type DistrictInfo = WithId<District, LocationYtd>;

pub fn payment(txn: &mut Transaction<Schema>, input: PaymentInput) -> PaymentOutput {
    let (warehouse, district) = txn
        .query_one(optional(|row| {
            let warehouse = row.and(Warehouse.number(input.warehouse));
            let district = row.and(District.warehouse(&warehouse).number(input.district));
            row.then_select((
                WarehouseInfo::from_expr(warehouse),
                DistrictInfo::from_expr(district),
            ))
        }))
        .unwrap();

    txn.update_ok(
        warehouse.row,
        Warehouse {
            ytd: Update::add(input.amount),
            ..Default::default()
        },
    );

    txn.update_ok(
        district.row,
        District {
            ytd: Update::add(input.amount),
            ..Default::default()
        },
    );

    let customer: WithId<Customer, CustomerInfo> =
        input
            .customer
            .lookup_customer(txn, input.customer_warehouse, input.customer_district);

    txn.update_ok(
        customer.row,
        Customer {
            ytd_payment: Update::add(input.amount),
            payment_cnt: Update::add(1),
            ..Default::default()
        },
    );

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
        txn.update_ok(
            customer.row,
            Customer {
                data: Update::set(&data),
                ..Default::default()
            },
        );
        data.truncate(200);
        credit_data = Some(data);
    }

    txn.insert_ok(History {
        customer: customer.row,
        district: district.row,
        date: input.date,
        amount: input.amount,
        data: format!("{}    {}", warehouse.name, district.name),
    });

    PaymentOutput {
        warehouse: input.warehouse,
        district: input.district,
        customer: customer.number,
        customer_district: input.customer_district,
        customer_warehouse: input.customer_warehouse,
        warehouse_info: warehouse.info,
        district_info: district.info,
        customer_info: customer.info,
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
