use super::*;
use rand::seq::IndexedRandom;
use rust_query::{Transaction, Update, optional};
use std::time::SystemTime;

pub fn generate_input(warehouse: i64, other: &[i64]) -> NewOrderInput {
    let district = rand::random_range(1..=10);
    let customer = Nu::CustomerId.rand();
    let item_count = rand::random_range(5..=15);
    let rbk = rand::random_ratio(1, 100);

    let mut items = vec![];
    for i in 1..=item_count {
        let mut item_number = Nu::ItemId.rand();
        if rbk && i == item_count {
            // emulate input error
            item_number = -1
        };

        items.push(ItemInput {
            item_number,
            supplying_warehouse: if rand::random_ratio(99, 100) || other.is_empty() {
                warehouse
            } else {
                *other.choose(&mut rand::rng()).expect("other is not empty")
            },
            quantity: rand::random_range(1..=10),
        });
    }

    NewOrderInput {
        warehouse,
        district,
        customer,
        items,
        entry_date: SystemTime::now(),
    }
}

pub struct NewOrderInput {
    warehouse: i64,
    district: i64,
    customer: i64,
    items: Vec<ItemInput>,
    entry_date: SystemTime,
}

struct ItemInput {
    item_number: i64,
    supplying_warehouse: i64,
    quantity: i64,
}

pub fn new_order(
    txn: &mut Transaction<Schema>,
    input: NewOrderInput,
) -> Result<OutputData, OutputData> {
    let customer = txn
        .query_one(optional(|row| {
            let warehouse = row.and(Warehouse.number(input.warehouse));
            let district = row.and(District.warehouse(warehouse).number(input.district));
            row.and_then(Customer.district(district).number(input.customer))
        }))
        .unwrap()
        .load(txn);
    let district = customer.district.load(txn);
    let warehouse = district.warehouse.load(txn);

    txn.update_ok(
        district.id,
        District {
            next_order: Update::add(1),
            ..Default::default()
        },
    );

    let local = input
        .items
        .iter()
        .all(|item| item.supplying_warehouse == input.warehouse);

    let order = txn
        .insert(Order {
            number: district.next_order,
            customer: customer.id,
            entry_d: input.entry_date,
            carrier_id: None::<i64>,
            all_local: local as i64,
            order_line_cnt: input.items.len() as i64,
        })
        .unwrap();
    txn.insert(NewOrder { order }).unwrap();

    let mut output_order_lines = vec![];

    let mut input_valid = true;

    for (
        number,
        ItemInput {
            item_number,
            supplying_warehouse,
            quantity,
        },
    ) in input.items.into_iter().enumerate()
    {
        let Some(item) = txn.query_one(Item.number(item_number)) else {
            input_valid = false;
            continue;
        };

        #[derive(Select)]
        struct StockInfo {
            row: TableRow<Stock>,
            quantity: i64,
            dist_xx: String,
            data: String,
        }

        let stock = txn
            .query_one(optional(|row| {
                let supplying_warehouse = row.and(Warehouse.number(supplying_warehouse));
                let stock = row.and(Stock.warehouse(supplying_warehouse).item(item));
                row.then(StockInfoSelect {
                    row: &stock,
                    quantity: &stock.quantity,
                    dist_xx: &[
                        &stock.dist_00,
                        &stock.dist_01,
                        &stock.dist_02,
                        &stock.dist_03,
                        &stock.dist_04,
                        &stock.dist_05,
                        &stock.dist_06,
                        &stock.dist_07,
                        &stock.dist_08,
                        &stock.dist_09,
                        &stock.dist_10,
                    ][input.district as usize],
                    data: &stock.data,
                })
            }))
            .unwrap();

        let new_quantity = if stock.quantity >= quantity + 10 {
            stock.quantity - quantity
        } else {
            stock.quantity - quantity + 91
        };

        let is_remote = supplying_warehouse != input.warehouse;
        txn.update_ok(
            stock.row,
            Stock {
                ytd: Update::add(quantity),
                quantity: Update::set(new_quantity),
                order_cnt: Update::add(1),
                remote_cnt: Update::add(is_remote as i64),
                ..Default::default()
            },
        );

        let item = item.load(txn);
        let amount = quantity * item.price;
        let brand_generic = if item.data.contains("ORIGINAL") && stock.data.contains("ORIGINAL") {
            "B"
        } else {
            "G"
        };

        txn.insert(OrderLine {
            order,
            number: number as i64,
            stock: stock.row,
            delivery_d: None::<i64>,
            quantity,
            amount,
            dist_info: stock.dist_xx,
        })
        .unwrap();

        output_order_lines.push(OutputLine {
            supplying_warehouse,
            item: item_number,
            item_name: item.name.clone(),
            quantity,
            stock_quantity: stock.quantity,
            brand_generic,
            item_price: item.price,
            amount,
        });
    }

    let total_amount = output_order_lines.iter().map(|x| x.amount).sum::<i64>() as f64
        * (1. - customer.discount)
        * (1. + warehouse.tax + district.tax);

    let out = OutputData {
        warehouse: input.warehouse,
        district: input.district,
        customer: input.customer,
        order: district.next_order,
        customer_last_name: customer.last.clone(),
        customer_credit: customer.credit.clone(),
        customer_discount: customer.discount,
        warehouse_tax: warehouse.tax,
        district_tax: district.tax,
        order_entry_date: input.entry_date,
        total_amount: total_amount as i64,
        order_lines: output_order_lines,
        input_valid,
    };

    if input_valid { Ok(out) } else { Err(out) }
}

#[expect(unused)]
pub struct OutputData {
    warehouse: i64,
    district: i64,
    customer: i64,
    order: i64,
    customer_last_name: String,
    customer_credit: String,
    customer_discount: f64,
    warehouse_tax: f64,
    district_tax: f64,
    order_entry_date: SystemTime,
    total_amount: i64,
    // order_line_count: i64,
    order_lines: Vec<OutputLine>,
    input_valid: bool,
}

#[expect(unused)]
pub struct OutputLine {
    supplying_warehouse: i64,
    item: i64,
    item_name: String,
    quantity: i64,
    stock_quantity: i64,
    brand_generic: &'static str,
    item_price: i64,
    amount: i64,
}
