use super::*;
use rust_query::{FromExpr, TableRow, Transaction, aggregate};

pub fn generate_order_status<'a>(
    txn: &Transaction<'a, Schema>,
    warehouse: TableRow<'a, Warehouse>,
) -> CustomerIdent<'a> {
    let mut rng = rand::rng();
    let district = txn
        .query_one(District::unique(warehouse, rng.random_range(1..=10)))
        .unwrap();

    customer_ident(txn, &mut rng, district)
}

pub fn order_status<'a>(
    txn: &Transaction<'a, Schema>,
    input: CustomerIdent<'a>,
) -> OrderStatus<'a> {
    let customer = input.lookup_customer(txn);
    let last_order = txn.query(|rows| {
        let order = rows.join(Order);
        rows.filter(order.customer().eq(customer));
        let max_number = rows.filter_some(aggregate(|rows| {
            let order = rows.join(Order);
            rows.filter(order.customer().eq(customer));
            rows.max(order.number())
        }));
        rows.filter(order.number().eq(max_number));
        rows.into_vec(order).swap_remove(0)
    });

    let order_lines_info = txn.query(|rows| {
        let order_line = rows.join(OrderLine);
        rows.filter(order_line.order().eq(last_order));
        rows.into_vec(FromExpr::from_expr(order_line))
    });

    OrderStatus {
        customer_info: txn.query_one(FromExpr::from_expr(customer)),
        order_info: txn.query_one(FromExpr::from_expr(last_order)),
        order_lines_info,
    }
}

struct OrderStatus<'a> {
    customer_info: Customer!(balance, first, middle, last),
    order_info: Order!(number, entry_d, carrier_id),
    order_lines_info: Vec<OrderLineInfo<'a>>,
}

type OrderLineInfo<'a> = OrderLine!(
    stock as Stock!(item<'a>, warehouse<'a>),
    quantity,
    amount,
    delivery_d
);
