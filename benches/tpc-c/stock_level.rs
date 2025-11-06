use super::*;
use rust_query::{Transaction, aggregate};

pub fn generate_input(warehouse: i64, district: i64) -> StockLevelInput {
    StockLevelInput {
        warehouse,
        district,
        minimum_quantity: rand::random_range(10..=20),
    }
}

pub struct StockLevelInput {
    warehouse: i64,
    district: i64,
    minimum_quantity: i64,
}

pub fn stock_level(txn: &Transaction<Schema>, input: StockLevelInput) -> i64 {
    txn.query_one(aggregate(|rows| {
        let ol = rows.join(OrderLine);
        let district = &ol.order.customer.district;
        rows.filter(district.warehouse.number.eq(input.warehouse));
        rows.filter(district.number.eq(input.district));

        rows.filter(ol.number.gte(district.next_order.sub(20)));
        rows.filter(ol.stock.quantity.lt(input.minimum_quantity));
        rows.count_distinct(&ol.stock)
    }))
}
