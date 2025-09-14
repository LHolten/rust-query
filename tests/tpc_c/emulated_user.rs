use std::{hint::black_box, iter::repeat_n, thread::sleep, time::Duration};

use rand::seq::SliceRandom;
use rust_query::Database;

use crate::{delivery, get_primary_warehouse, new_order, order_status, payment, v0::Schema};

fn emulate(txn_deck: &mut Vec<TxnKind>, db: Database<Schema>) {
    let txn_kind = select_transaction(txn_deck);
    keying_time(txn_kind);
    measure_txn_rt(&db, txn_kind);
    think_time(txn_kind);
}

#[derive(Clone, Copy)]
enum TxnKind {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
}

fn select_transaction(txn_deck: &mut Vec<TxnKind>) -> TxnKind {
    if txn_deck.is_empty() {
        txn_deck.extend(repeat_n(TxnKind::NewOrder, 10));
        txn_deck.extend(repeat_n(TxnKind::Payment, 10));
        txn_deck.push(TxnKind::OrderStatus);
        txn_deck.push(TxnKind::Delivery);
        txn_deck.push(TxnKind::StockLevel);
        txn_deck.shuffle(&mut rand::rng());
    }
    txn_deck.pop().unwrap()
}

fn keying_time(txn_kind: TxnKind) {
    let secs = match txn_kind {
        TxnKind::NewOrder => 18,
        TxnKind::Payment => 3,
        TxnKind::OrderStatus | TxnKind::Delivery | TxnKind::StockLevel => 2,
    };
    sleep(Duration::from_secs(secs))
}

fn measure_txn_rt(db: &Database<Schema>, txn_kind: TxnKind) {
    match txn_kind {
        TxnKind::NewOrder => {
            let _ = db.transaction_mut(|txn| {
                let warehouse = get_primary_warehouse(txn);
                new_order::random_new_order(txn, warehouse, &[])
                    .map(|val| {
                        black_box(val);
                    })
                    .map_err(|val| {
                        black_box(val);
                    })
            });
        }
        TxnKind::Payment => db.transaction_mut_ok(|txn| {
            let warehouse = get_primary_warehouse(txn);
            black_box(payment::random_payment(txn, warehouse, &[]));
        }),
        TxnKind::OrderStatus => db.transaction(|txn| {
            let warehouse = get_primary_warehouse(txn);
            black_box(order_status::random_order_status(txn, warehouse));
        }),
        TxnKind::Delivery => db.transaction_mut_ok(|txn| {
            let warehouse = get_primary_warehouse(txn);
            black_box(delivery::random_delivery(txn, warehouse));
        }),
        TxnKind::StockLevel => todo!(),
    }
}

fn think_time(txn_kind: TxnKind) {
    let mean_secs = match txn_kind {
        TxnKind::NewOrder | TxnKind::Payment => 12.,
        TxnKind::OrderStatus => 10.,
        TxnKind::Delivery | TxnKind::StockLevel => 5.,
    };
    let secs = -rand::random::<f64>().ln() * mean_secs;
    sleep(Duration::from_secs_f64(secs));
}
