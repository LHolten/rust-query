use super::*;
use rust_query::{FromExpr, TableRow, Transaction, TransactionMut, Update, optional};
use std::time::SystemTime;

pub fn random_new_order(txn: TransactionMut<Schema>, warehouse: TableRow<Warehouse>) -> OutputData {
    let input = generate_input(&txn, warehouse);
    new_order(txn, input)
}

fn generate_input(txn: &Transaction<Schema>, warehouse: TableRow<Warehouse>) -> NewOrderInput {
    let mut rng = rand::rng();
    let district = txn
        .query_one(District::unique(warehouse, rng.random_range(1..=10)))
        .unwrap();
    let customer = txn
        .query_one(Customer::unique(district, rng.nurand(1023, 1, 3000)))
        .unwrap();
    let item_count = rng.random_range(5..=15);
    let rbk = rng.random_ratio(1, 100);

    let mut items = vec![];
    for i in 1..=item_count {
        let mut item_number = rng.nurand(8191, 1, 100000);
        if rbk && i == item_count {
            // emulate input error
            item_number = -1
        };

        items.push(ItemInput {
            item_number,
            // TODO: support remote warehouses in case there are multiple
            supplying_warehouse: warehouse,
            quantity: rng.random_range(1..=10),
        });
    }

    NewOrderInput {
        customer,
        items,
        entry_date: SystemTime::now(),
    }
}

struct NewOrderInput {
    customer: TableRow<Customer>,
    items: Vec<ItemInput>,
    entry_date: SystemTime,
}

struct ItemInput {
    item_number: i64,
    supplying_warehouse: TableRow<Warehouse>,
    quantity: i64,
}

fn new_order(mut txn: TransactionMut<Schema>, input: NewOrderInput) -> OutputData {
    let district = txn.query_one(&input.customer.into_expr().district);

    let district_info: District!(warehouse, number, tax, next_order) =
        txn.query_one(FromExpr::from_expr(district));

    let warehouse_tax = txn.query_one(&district.into_expr().warehouse.tax);

    txn.update_ok(
        district,
        District {
            next_order: Update::add(1),
            ..Default::default()
        },
    );

    let customer_info: Customer!(discount, last, credit) =
        txn.query_one(FromExpr::from_expr(input.customer));

    let local = input
        .items
        .iter()
        .all(|item| item.supplying_warehouse == district_info.warehouse);

    let order = txn
        .insert(Order {
            number: district_info.next_order,
            customer: input.customer,
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
        let Some((item, item_info)): Option<(_, Item!(price, name, data))> =
            txn.query_one(optional(|row| {
                let item = row.and(Item::unique(item_number));
                row.then((&item, FromExpr::from_expr(&item)))
            }))
        else {
            input_valid = false;
            continue;
        };

        let stock = Stock::unique(supplying_warehouse, item);
        let stock = txn.query_one(stock).unwrap();
        let stock_expr = stock.into_expr();

        #[derive(Select)]
        struct StockInfo {
            quantity: i64,
            dist_xx: String,
            data: String,
        }
        let stock_info = txn.query_one(StockInfoSelect {
            quantity: &stock_expr.quantity,
            dist_xx: &[
                &stock_expr.dist_00,
                &stock_expr.dist_01,
                &stock_expr.dist_02,
                &stock_expr.dist_03,
                &stock_expr.dist_04,
                &stock_expr.dist_05,
                &stock_expr.dist_06,
                &stock_expr.dist_07,
                &stock_expr.dist_08,
                &stock_expr.dist_09,
                &stock_expr.dist_10,
            ][district_info.number as usize],
            data: &stock_expr.data,
        });

        let new_quantity = if stock_info.quantity >= quantity + 10 {
            stock_info.quantity - quantity
        } else {
            stock_info.quantity - quantity + 91
        };

        let is_remote = supplying_warehouse != district_info.warehouse;
        txn.update_ok(
            stock,
            Stock {
                ytd: Update::add(quantity),
                quantity: Update::set(new_quantity),
                order_cnt: Update::add(1),
                remote_cnt: Update::add(is_remote as i64),
                ..Default::default()
            },
        );

        let amount = quantity * item_info.price;
        let brand_generic =
            if item_info.data.contains("ORIGINAL") && stock_info.data.contains("ORIGINAL") {
                "B"
            } else {
                "G"
            };

        txn.insert(OrderLine {
            order,
            number: number as i64,
            stock,
            delivery_d: None::<i64>,
            quantity,
            amount,
            dist_info: stock_info.dist_xx,
        })
        .unwrap();

        output_order_lines.push(OutputLine {
            supplying_warehouse,
            item,
            item_name: item_info.name,
            quantity,
            stock_quantity: stock_info.quantity,
            brand_generic,
            item_price: item_info.price,
            amount,
        });
    }

    let total_amount = output_order_lines.iter().map(|x| x.amount).sum::<i64>() as f64
        * (1. - customer_info.discount)
        * (1. + warehouse_tax + district_info.tax);

    if input_valid {
        txn.commit();
    } else {
        // rollback if there are input errors
        drop(txn);
    }

    OutputData {
        warehouse: district_info.warehouse,
        district,
        customer: input.customer,
        order,
        customer_last_name: customer_info.last,
        customer_credit: customer_info.credit,
        customer_discount: customer_info.discount,
        warehouse_tax,
        district_tax: district_info.tax,
        order_entry_date: input.entry_date,
        total_amount: total_amount as i64,
        order_lines: output_order_lines,
        input_valid,
    }
}

#[expect(unused)]
pub struct OutputData {
    warehouse: TableRow<Warehouse>,
    district: TableRow<District>,
    customer: TableRow<Customer>,
    order: TableRow<Order>,
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
    supplying_warehouse: TableRow<Warehouse>,
    item: TableRow<Item>,
    item_name: String,
    quantity: i64,
    stock_quantity: i64,
    brand_generic: &'static str,
    item_price: i64,
    amount: i64,
}
