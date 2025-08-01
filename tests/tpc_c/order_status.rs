use super::*;
use rust_query::{FromExpr, TableRow, Transaction, aggregate, optional};

pub fn random_order_status(
    txn: &Transaction<Schema>,
    warehouse: TableRow<Warehouse>,
) -> OrderStatus {
    let input = generate_input(txn, warehouse);
    order_status(txn, input)
}

fn generate_input(txn: &Transaction<Schema>, warehouse: TableRow<Warehouse>) -> CustomerIdent {
    let mut rng = rand::rng();
    let district = txn
        .query_one(District::unique(warehouse, rng.random_range(1..=10)))
        .unwrap();

    customer_ident(txn, &mut rng, district)
}

fn order_status(txn: &Transaction<Schema>, input: CustomerIdent) -> OrderStatus {
    let customer = input.lookup_customer(txn);
    let last_order = txn
        .query_one(optional(|row| {
            aggregate(|rows| {
                let order = rows.join(Order);
                rows.filter(order.customer.eq(customer));
                let max_number = row.and(rows.max(&order.number));
                rows.filter(order.number.eq(&max_number));
                let order = row.and(Order::unique(customer, max_number));
                row.then(order)
            })
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
    stock as Stock!(item, warehouse),
    quantity,
    amount,
    delivery_d
);
