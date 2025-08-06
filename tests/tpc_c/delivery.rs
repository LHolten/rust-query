use std::time::UNIX_EPOCH;

use rust_query::{Transaction, Update, aggregate, optional};

use super::*;

pub fn random_delivery(txn: &'static mut Transaction<Schema>, warehouse: TableRow<Warehouse>) {
    delivery(txn, generate_input(warehouse));
}

fn generate_input(warehouse: TableRow<Warehouse>) -> DeliveryInput {
    let mut rng = rand::rng();

    DeliveryInput {
        warehouse,
        carrier_id: rng.random_range(1..=10),
        delivery_d: UNIX_EPOCH.elapsed().unwrap().as_millis() as i64,
    }
}

struct DeliveryInput {
    warehouse: TableRow<Warehouse>,
    carrier_id: i64,
    delivery_d: i64,
}

fn delivery(txn: &'static mut Transaction<Schema>, input: DeliveryInput) {
    let mut new_orders = vec![];
    for district_num in 0..10 {
        let district = txn
            .query_one(District::unique(input.warehouse, district_num))
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
                let customer = row.and(Customer::unique(district, customer_num));
                let order = row.and(Order::unique(customer, order_num));
                let new_order = row.and(NewOrder::unique(order));
                row.then(new_order)
            })
        }));
        let Some(new_order) = new_order else {
            continue;
        };

        new_orders.push(new_order);
        let order = &new_order.into_expr().order;

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
    }
    let txn = txn.downgrade();
    for new_order in new_orders {
        assert!(txn.delete_ok(new_order));
    }
}
