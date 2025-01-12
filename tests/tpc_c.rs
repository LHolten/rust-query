#![feature(type_changing_struct_update)]
use std::time;

use rust_query::{migration::schema, FromDummy, IntoColumn, Table, TableRow, TransactionMut};

#[schema]
enum Schema {
    Warehouse {
        name: String,
        street_1: String,
        street_2: String,
        city: String,
        state: String,
        zip: String,
        tax: f64,
        ytd: i64,
    },
    #[unique(warehouse, number)]
    District {
        warehouse: Warehouse,
        number: i64,
        name: String,
        street_1: String,
        street_2: String,
        city: String,
        state: String,
        zip: String,
        tax: f64,
        ytd: i64,
        next_order: i64, // next available order id
    },
    Customer {
        district: District,
        first: String,
        middle: String,
        last: String,
        street_1: String,
        street_2: String,
        city: String,
        state: String,
        zip: String,
        phone: String,
        since: i64,
        credit: String,
        credit_lim: i64,
        discount: f64,
        balance: i64,
        ytd_payment: i64,
        payment_cnt: i64,
        delivery_cnt: i64,
        data: String,
    },
    History {
        customer: Customer,
        district: District,
        date: i64,
        amount: i64,
        data: String,
    },
    #[unique(order)]
    #[no_reference]
    NewOrder { order: Order },
    Order {
        customer: Customer,
        entry_d: i64,
        carrier_id: Option<i64>,
        order_line_cnt: i64, // unnecessary
        all_local: i64,
    },
    #[unique(order, number)]
    OrderLine {
        order: Order,
        number: i64,
        stock: Stock,
        derlivery_d: Option<i64>,
        quantity: i64,
        amount: i64, // total cost of this line
        dist_info: String,
    },
    Item {
        image_id: i64,
        name: String,
        price: i64,
        data: String,
    },
    #[unique(warehouse, item)]
    Stock {
        warehouse: Warehouse,
        item: Item,
        quantity: i64,
        dist_00: String,
        dist_01: String,
        dist_02: String,
        dist_03: String,
        dist_04: String,
        dist_05: String,
        dist_06: String,
        dist_07: String,
        dist_08: String,
        dist_09: String,
        dist_10: String,
        ytd: i64,
        order_cnt: i64,
        remote_cnt: i64,
        data: String,
    },
}
use v0::*;

pub struct NewOrderInput<'a> {
    customer: TableRow<'a, Customer>,
    items: Vec<ItemInput<'a>>,
}

pub struct ItemInput<'a> {
    item: TableRow<'a, Item>,
    supplying_warehouse: TableRow<'a, Warehouse>,
    quantity: i64,
}

pub fn new_order<'a>(
    mut txn: TransactionMut<'a, Schema>,
    input: NewOrderInput<'a>,
) -> OutputData<'a> {
    let district = txn.query_one(input.customer.district());

    #[derive(FromDummy)]
    #[rq(from = District, lt = 't)]
    struct DistrictInfo<'t> {
        warehouse: TableRow<'t, Warehouse>,
        number: i64,
        tax: f64,
        next_order: i64,
    }

    let district_info: DistrictInfo = txn.query_one(district.trivial());

    let warehouse_tax = txn.query_one(district.warehouse().tax());

    txn.find_and_update(District {
        next_order: district_info.next_order + 1,
        ..Table::dummy(district)
    })
    .unwrap();

    #[derive(FromDummy)]
    #[rq(from = Customer)]
    struct CustomerInfo {
        discount: f64,
        last: String,
        credit: String,
    }
    let customer_info: CustomerInfo = txn.query_one(input.customer.trivial());

    let local = input
        .items
        .iter()
        .all(|item| item.supplying_warehouse == district_info.warehouse);

    let entry_d = time::SystemTime::UNIX_EPOCH.elapsed().unwrap().as_millis() as i64;

    let order = txn.insert(Order {
        customer: input.customer,
        entry_d,
        carrier_id: None::<i64>,
        order_line_cnt: input.items.len() as i64,
        all_local: local as i64,
    });
    txn.try_insert(NewOrder { order }).unwrap();

    let mut output_order_lines = vec![];

    for (
        number,
        ItemInput {
            item,
            supplying_warehouse,
            quantity,
        },
    ) in input.items.into_iter().enumerate()
    {
        // TODO: make this a lookup by external item id

        #[derive(FromDummy)]
        #[rq(from = Item)]
        struct ItemInfo {
            price: i64,
            name: String,
            data: String,
        }

        let item_info: ItemInfo = txn.query_one(item.trivial());

        let stock = Stock::unique(supplying_warehouse, item);
        let stock = txn.query_one(stock).unwrap();

        #[derive(FromDummy)]
        struct StockInfo {
            quantity: i64,
            dist_xx: String,
            data: String,
        }
        let stock_info = txn.query_one(StockInfoDummy {
            quantity: stock.quantity(),
            dist_xx: &[
                stock.dist_00(),
                stock.dist_01(),
                stock.dist_02(),
                stock.dist_03(),
                stock.dist_04(),
                stock.dist_05(),
                stock.dist_06(),
                stock.dist_07(),
                stock.dist_08(),
                stock.dist_09(),
                stock.dist_10(),
            ][district_info.number as usize],
            data: stock.data(),
        });

        let new_quantity = if stock_info.quantity >= quantity + 10 {
            stock_info.quantity - quantity
        } else {
            stock_info.quantity - quantity + 91
        };

        let is_remote = supplying_warehouse != district_info.warehouse;
        txn.find_and_update(Stock {
            ytd: stock.ytd().add(quantity),
            quantity: new_quantity,
            order_cnt: stock.order_cnt().add(1),
            remote_cnt: stock.remote_cnt().add(is_remote as i64),
            ..Table::dummy(stock)
        })
        .unwrap();

        let amount = quantity * item_info.price;
        let brand_generic =
            if item_info.data.contains("ORIGINAL") && stock_info.data.contains("ORIGINAL") {
                "B"
            } else {
                "G"
            };

        txn.try_insert(OrderLine {
            order,
            number: number as i64,
            stock,
            derlivery_d: None::<i64>,
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
        order_entry_date: entry_d,
        total_amount: total_amount as i64,
        order_lines: output_order_lines,
    }
}

pub struct OutputData<'t> {
    warehouse: TableRow<'t, Warehouse>,
    district: TableRow<'t, District>,
    customer: TableRow<'t, Customer>,
    order: TableRow<'t, Order>,
    customer_last_name: String,
    customer_credit: String,
    customer_discount: f64,
    warehouse_tax: f64,
    district_tax: f64,
    order_entry_date: i64,
    total_amount: i64,
    // order_line_count: i64,
    order_lines: Vec<OutputLine<'t>>,
}

pub struct OutputLine<'t> {
    supplying_warehouse: TableRow<'t, Warehouse>,
    item: TableRow<'t, Item>,
    item_name: String,
    quantity: i64,
    stock_quantity: i64,
    brand_generic: &'static str,
    item_price: i64,
    amount: i64,
}
