use std::time::UNIX_EPOCH;

use rust_query::{Update, aggregate, optional};

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
    let warehouse = txn.query_one(Warehouse.number(input.warehouse)).unwrap();
    let district = txn
        .query_one(District.warehouse(warehouse).number(district_num))
        .unwrap();

    let new_order = txn.query_one(optional(|row| {
        aggregate(|rows| {
            let new_order = rows.join(NewOrder);
            let order = &new_order.order;
            let customer = &order.customer;
            rows.filter(customer.district.eq(district));

            let order_num = row.and(rows.min(&order.number));
            rows.filter(order.number.eq(&order_num));

            let customer_num = row.and(rows.min(&customer.number));
            let customer = row.and(Customer.district(district).number(customer_num));
            let order = row.and(Order.customer(customer).number(order_num));
            let new_order = row.and(NewOrder.order(order));
            row.then(new_order)
        })
    }));
    let Some(new_order) = new_order else {
        return None;
    };

    let order = &new_order.into_expr().order;
    let order_num = txn.query_one(&order.number);

    txn.update_ok(
        order,
        Order {
            carrier_id: Update::set(Some(input.carrier_id)),
            ..Default::default()
        },
    );

    let mut total_amount = 0;
    for (line, amount) in txn.query(|rows| {
        let ol = rows.join(OrderLine);
        rows.filter(ol.order.eq(order));
        rows.into_vec((&ol, &ol.amount))
    }) {
        total_amount += amount;
        txn.update_ok(
            line,
            OrderLine {
                delivery_d: Update::set(Some(input.delivery_d)),
                ..Default::default()
            },
        );
    }

    txn.update_ok(
        &order.customer,
        Customer {
            balance: Update::add(total_amount),
            delivery_cnt: Update::add(1),
            ..Default::default()
        },
    );

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
