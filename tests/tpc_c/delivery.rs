use std::time::UNIX_EPOCH;

use rust_query::{TransactionMut, Update, aggregate};

use super::*;

pub fn generate_delivery<'a>(
    txn: &Transaction<'a, Schema>,
    warehouse: TableRow<'a, Warehouse>,
) -> DeliveryInput<'a> {
    let mut rng = rand::rng();

    DeliveryInput {
        warehouse,
        carrier_id: rng.random_range(1..=10),
        delivery_d: UNIX_EPOCH.elapsed().unwrap().as_millis() as i64,
    }
}

struct DeliveryInput<'a> {
    warehouse: TableRow<'a, Warehouse>,
    carrier_id: i64,
    delivery_d: i64,
}

pub fn delivery<'a>(mut txn: TransactionMut<'a, Schema>, input: DeliveryInput<'a>) {
    let mut new_orders = vec![];
    for district_num in 0..10 {
        let district = txn
            .query_one(District::unique(input.warehouse, district_num))
            .unwrap();

        let Some(first_new_order) = txn.query_one(aggregate(|rows| {
            let new_order = NewOrder::join(rows);
            let order = new_order.order();
            rows.filter_on(order.customer().district(), district);
            rows.min(order.number())
        })) else {
            continue;
        };

        let new_order = txn.query(|rows| {
            let new_order = NewOrder::join(rows);
            let order = new_order.order();
            rows.filter(order.customer().district().eq(district));
            rows.filter(order.number().eq(first_new_order));
            rows.into_vec(new_order).swap_remove(0)
        });
        new_orders.push(new_order);

        txn.update_ok(
            new_order.order(),
            Order {
                carrier_id: Update::set(Some(input.carrier_id)),
                ..Default::default()
            },
        );

        let mut total_amount = 0;
        for (line, amount) in txn.query(|rows| {
            let ol = OrderLine::join(rows);
            rows.filter(ol.order().eq(new_order.order()));
            rows.into_vec((&ol, ol.amount()))
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
            new_order.order().customer(),
            Customer {
                balance: Update::add(total_amount),
                delivery_cnt: Update::add(1),
                ..Default::default()
            },
        );
    }
    let mut txn = txn.downgrade();
    for new_order in new_orders {
        assert!(txn.delete_ok(new_order));
    }

    txn.commit();
}
