use std::time::UNIX_EPOCH;

use rust_query::{aggregate, optional};

use super::*;

pub fn generate_input(warehouse: i64) -> DeliveryInput {
    DeliveryInput {
        warehouse,
        carrier_id: rand::random_range(1..=10),
        delivery_d: UNIX_EPOCH.elapsed().unwrap().as_millis() as i64,
    }
}

pub struct DeliveryInput {
    warehouse: i64,
    carrier_id: i64,
    delivery_d: i64,
}

pub fn delivery(
    txn: &'static mut Transaction<Schema>,
    input: &DeliveryInput,
    district_num: i64,
) -> Option<DeliveryOutput> {
    let district = txn
        .query_one(optional(|row| {
            let warehouse = row.and(Warehouse.number(input.warehouse));
            row.and_then(District.warehouse(warehouse).number(district_num))
        }))
        .unwrap();

    let new_order = txn.query_one(optional(|row| {
        aggregate(|rows| {
            let customer = rows.join(Customer.district(district));
            let order = rows.join(Order.customer(&customer));
            rows.filter_some(NewOrder.order(&order));

            let order_num = row.and(rows.min(&order.number));
            rows.filter(order.number.eq(&order_num));

            let customer_num = row.and(rows.min(&customer.number));
            let customer = row.and(Customer.district(district).number(customer_num));
            let order = row.and(Order.customer(customer).number(order_num));
            row.and_then(NewOrder.order(order))
        })
    }))?;

    let mut order = txn.mutable(&new_order.into_expr().order);
    order.carrier_id = Some(input.carrier_id);
    let order_num = order.number;
    let order = order.into_table_row();

    let mut total_amount = 0;
    for mut line in txn.mutable_vec(OrderLine.order(order)) {
        total_amount += line.amount;
        line.delivery_d = Some(input.delivery_d);
    }

    let mut customer = txn.mutable(&order.into_expr().customer);
    customer.balance += total_amount;
    customer.delivery_cnt += 1;
    drop(customer);

    let txn = txn.downgrade();
    assert!(txn.delete_ok(new_order));

    Some(DeliveryOutput {
        district: district_num,
        order: order_num,
    })
}

#[expect(unused)]
pub struct DeliveryOutput {
    district: i64,
    order: i64,
}
