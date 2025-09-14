use std::{
    hint::black_box,
    iter::repeat_n,
    sync::atomic::AtomicU64,
    thread::sleep,
    time::{Duration, Instant},
};

use rand::seq::SliceRandom;
use rust_query::{Database, Transaction};

use crate::{
    delivery, new_order, order_status, payment, stock_level,
    v0::{District, Schema, Warehouse},
};

pub(crate) fn loop_emulate(db: Database<Schema>, warehouse: i64, district: i64) {
    let mut txn_deck = Vec::new();
    // TODO: Need to put this on a thread with a signal to stop
    for _ in 0..100 {
        emulate(&mut txn_deck, &db, warehouse, district);
    }
}

fn emulate(txn_deck: &mut Vec<TxnKind>, db: &Database<Schema>, warehouse: i64, district: i64) {
    let txn_kind = select_transaction(txn_deck);
    keying_time(txn_kind);
    measure_txn_rt(&db, txn_kind, warehouse, district);
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

static ON_TIME: AtomicU64 = AtomicU64::new(0);
static LATE: AtomicU64 = AtomicU64::new(0);

fn measure_txn_rt(db: &Database<Schema>, txn_kind: TxnKind, warehouse: i64, district: i64) {
    let get_warehouse = |txn: &Transaction<Schema>| {
        txn.query_one(Warehouse::unique(warehouse))
            .expect("warehouse exists")
    };
    let get_district = |txn: &Transaction<Schema>| {
        txn.query_one(District::unique(get_warehouse(txn), district))
            .expect("district exists")
    };
    match txn_kind {
        TxnKind::NewOrder => {
            let before = Instant::now();
            let _ = db.transaction_mut(|txn| {
                let warehouse = get_warehouse(txn);
                // TODO: need to initialize other warehouses
                new_order::random_new_order(txn, warehouse, &[])
                    .map(|val| {
                        black_box(val);
                    })
                    .map_err(|val| {
                        black_box(val);
                    })
            });
            let elapsed = before.elapsed();
            if elapsed <= Duration::from_secs(5) {
                ON_TIME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            } else {
                LATE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        TxnKind::Payment => db.transaction_mut_ok(|txn| {
            let warehouse = get_warehouse(txn);
            black_box(payment::random_payment(txn, warehouse, &[]));
        }),
        TxnKind::OrderStatus => db.transaction(|txn| {
            let warehouse = get_warehouse(txn);
            black_box(order_status::random_order_status(txn, warehouse));
        }),
        TxnKind::Delivery => db.transaction_mut_ok(|txn| {
            // TODO: this transaction can be queued.
            let warehouse = get_warehouse(txn);
            black_box(delivery::random_delivery(txn, warehouse));
        }),
        TxnKind::StockLevel => db.transaction(|txn| {
            let district = get_district(txn);
            black_box(stock_level::random_stock_level(txn, district));
        }),
    }
}

fn think_time(txn_kind: TxnKind) {
    let mean_secs = match txn_kind {
        TxnKind::NewOrder | TxnKind::Payment => 12.,
        TxnKind::OrderStatus => 10.,
        TxnKind::Delivery | TxnKind::StockLevel => 5.,
    };
    let secs = -rand::random::<f64>().ln() * mean_secs;
    let secs = secs.min(10. * mean_secs);
    sleep(Duration::from_secs_f64(secs));
}
