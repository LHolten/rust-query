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

pub fn order_status(txn: &Transaction<Schema>, input: OrderStatusInput) -> output::OrderStatus {
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
        let order_line = rows.join(OrderLine.order(last_order));
        rows.into_vec(FromExpr::from_expr(order_line))
    });

    output::OrderStatus {
        customer_info: txn.query_one(FromExpr::from_expr(customer)),
        order_info: txn.query_one(FromExpr::from_expr(last_order)),
        order_lines_info,
    }
}

#[expect(unused)]
mod output {
    use super::*;
    use rust_query::FromExpr;

    pub struct OrderStatus {
        pub customer_info: CustomerInfo,
        pub order_info: OrderInfo,
        pub order_lines_info: Vec<OrderLineInfo>,
    }

    #[derive(FromExpr)]
    #[rust_query(From = Customer)]
    pub struct CustomerInfo {
        pub balance: i64,
        pub first: String,
        pub middle: String,
        pub last: String,
    }

    #[derive(FromExpr)]
    #[rust_query(From = Order)]
    pub struct OrderInfo {
        pub number: i64,
        pub entry_d: i64,
        pub carrier_id: Option<i64>,
    }

    #[derive(FromExpr)]
    #[rust_query(From = Item, From = Warehouse)]
    pub struct Number {
        pub number: i64,
    }

    #[derive(FromExpr)]
    #[rust_query(From = Stock)]
    pub struct StockInfo {
        pub item: Number,
        pub warehouse: Number,
    }

    #[derive(FromExpr)]
    #[rust_query(From = OrderLine)]
    pub struct OrderLineInfo {
        pub stock: StockInfo,
        pub quantity: i64,
        pub amount: i64,
        pub delivery_d: Option<i64>,
    }
}
