use super::*;
use rust_query::{TableRow, Transaction, aggregate};

pub fn random_stock_level(txn: &Transaction<Schema>, district: TableRow<District>) -> i64 {
    let input = generate_input(district);
    stock_level(txn, input)
}

// returns the minimum quantity
fn generate_input(district: TableRow<District>) -> StockLevelInput {
    StockLevelInput {
        district,
        minimum_quantity: rand::random_range(10..=20),
    }
}

struct StockLevelInput {
    district: TableRow<District>,
    minimum_quantity: i64,
}

fn stock_level(txn: &Transaction<Schema>, input: StockLevelInput) -> i64 {
    let district = input.district.into_expr();

    txn.query_one(aggregate(|rows| {
        let ol = rows.join(OrderLine);
        rows.filter(ol.order.customer.district.eq(&district));
        rows.filter(ol.number.gte(district.next_order.sub(20)));
        rows.filter(ol.stock.quantity.lt(input.minimum_quantity));
        rows.count_distinct(&ol.stock)
    }))
}
