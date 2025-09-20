use std::{
    hint::black_box,
    iter::repeat_n,
    ops::ControlFlow,
    sync::{Condvar, Mutex, atomic::AtomicU64},
    time::{Duration, Instant},
};

use rand::seq::SliceRandom;
use rust_query::{Database, Transaction};

use crate::{
    delivery, new_order, order_status, payment, stock_level,
    v0::{District, Schema, Warehouse},
};

pub(crate) fn loop_emulate(db: &Database<Schema>, warehouse: i64, district: i64) {
    let mut txn_deck = Vec::new();
    while let ControlFlow::Continue(()) = emulate(&mut txn_deck, db, warehouse, district) {}
}

fn emulate(
    txn_deck: &mut Vec<TxnKind>,
    db: &Database<Schema>,
    warehouse: i64,
    district: i64,
) -> ControlFlow<()> {
    let txn_kind = select_transaction(txn_deck);
    keying_time(txn_kind)?;
    measure_txn_rt(&db, txn_kind, warehouse, district);
    think_time(txn_kind)?;
    ControlFlow::Continue(())
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

fn keying_time(txn_kind: TxnKind) -> ControlFlow<()> {
    let secs = match txn_kind {
        TxnKind::NewOrder => 18,
        TxnKind::Payment => 3,
        TxnKind::OrderStatus | TxnKind::Delivery | TxnKind::StockLevel => 2,
    };
    sleep_or_break(Duration::from_secs(secs))
}

static ON_TIME: AtomicU64 = AtomicU64::new(0);
static LATE: AtomicU64 = AtomicU64::new(0);

pub fn print_stats() {
    let on_time = ON_TIME.load(std::sync::atomic::Ordering::Acquire);
    let late = LATE.load(std::sync::atomic::Ordering::Acquire);
    println!(
        "{} new orders inserted, {}% late",
        on_time + late,
        late as f64 / (on_time + late) as f64 * 100.
    )
}

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

fn think_time(txn_kind: TxnKind) -> ControlFlow<()> {
    let mean_secs = match txn_kind {
        TxnKind::NewOrder | TxnKind::Payment => 12.,
        TxnKind::OrderStatus => 10.,
        TxnKind::Delivery | TxnKind::StockLevel => 5.,
    };
    let secs = -rand::random::<f64>().ln() * mean_secs;
    let secs = secs.min(10. * mean_secs);
    sleep_or_break(Duration::from_secs_f64(secs))
}

fn sleep_or_break(dur: Duration) -> ControlFlow<()> {
    let (_guard, res) = STOP
        .condvar
        .wait_timeout_while(STOP.should_stop.lock().unwrap(), dur, |should_stop| {
            !*should_stop
        })
        .unwrap();
    if res.timed_out() {
        ControlFlow::Continue(())
    } else {
        ControlFlow::Break(())
    }
}

struct Stop {
    should_stop: Mutex<bool>,
    condvar: Condvar,
}

static STOP: Stop = Stop {
    should_stop: Mutex::new(false),
    condvar: Condvar::new(),
};

pub fn stop_emulation() {
    *STOP.should_stop.lock().unwrap() = true;
    STOP.condvar.notify_all();
}
