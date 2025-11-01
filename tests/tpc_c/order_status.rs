use super::*;
use rust_query::{FromExpr, TableRow, Transaction, aggregate, optional};

pub fn generate_input(warehouse: i64) -> OrderStatusInput {
    OrderStatusInput {
        warehouse,
        district: rand::random_range(1..=10),
        customer: customer_ident(),
    }
}

pub struct OrderStatusInput {
    warehouse: i64,
    district: i64,
    customer: CustomerIdent,
}

pub fn order_status(txn: &Transaction<Schema>, input: OrderStatusInput) -> OrderStatus {
    let customer: TableRow<_> =
        input
            .customer
            .lookup_customer(txn, input.warehouse, input.district);
    let last_order = txn
        .query_one(optional(|row| {
            let max_number = row.and(aggregate(|rows| {
                let order = rows.join(Order.customer(customer));
                rows.max(&order.number)
            }));
            row.and_then(Order.customer(customer).number(max_number))
        }))
        .unwrap();

    let order_lines_info = txn.query(|rows| {
        let order_line = rows.join(OrderLine);
        rows.filter(order_line.order.eq(last_order));
        rows.into_vec(FromExpr::from_expr(order_line))
    });

    OrderStatus {
        customer_info: txn.query_one(FromExpr::from_expr(customer)),
        order_info: txn.query_one(FromExpr::from_expr(last_order)),
        order_lines_info,
    }
}

#[expect(unused)]
pub struct OrderStatus {
    customer_info: Customer!(balance, first, middle, last),
    order_info: Order!(number, entry_d, carrier_id),
    order_lines_info: Vec<OrderLineInfo>,
}

type OrderLineInfo = OrderLine!(
    stock as Stock!(item as Item!(number), warehouse as Warehouse!(number)),
    quantity,
    amount,
    delivery_d
);
