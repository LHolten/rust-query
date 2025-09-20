use std::{
    hint::black_box,
    iter::repeat_n,
    ops::ControlFlow,
    sync::{Arc, Condvar, Mutex, atomic::AtomicU64},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rand::seq::SliceRandom;
use rust_query::{Database, Transaction};

use crate::{
    delivery, new_order, order_status, payment, stock_level,
    v0::{District, Schema, Warehouse},
};

pub(crate) struct Emulate {
    pub db: Arc<Database<Schema>>,
    pub warehouse: i64,
    pub district: i64,
    pub queue: Vec<JoinHandle<()>>,
}

impl Emulate {
    pub fn loop_emulate(mut self) {
        let mut txn_deck = Vec::new();
        while let ControlFlow::Continue(()) = self.emulate(&mut txn_deck) {}
        for thread in self.queue {
            thread.join().unwrap();
        }
    }

    fn emulate(&mut self, txn_deck: &mut Vec<TxnKind>) -> ControlFlow<()> {
        let txn_kind = select_transaction(txn_deck);
        keying_time(txn_kind)?;
        if let TxnKind::Delivery = txn_kind {
            let warehouse = self.warehouse;
            let district = self.district;
            let db = self.db.clone();
            self.queue.push(thread::spawn(move || {
                measure_txn_rt(&db, txn_kind, warehouse, district)
            }));
        } else {
            measure_txn_rt(&self.db, txn_kind, self.warehouse, self.district)
        }
        think_time(txn_kind)?;
        ControlFlow::Continue(())
    }
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

fn measure_txn_rt(db: &Database<Schema>, txn_kind: TxnKind, warehouse: i64, district: i64) {
    let get_warehouse = |txn: &Transaction<Schema>| {
        txn.query_one(Warehouse::unique(warehouse))
            .expect("warehouse exists")
    };
    let get_district = |txn: &Transaction<Schema>| {
        txn.query_one(District::unique(get_warehouse(txn), district))
            .expect("district exists")
    };
    let before = Instant::now();
    match txn_kind {
        TxnKind::NewOrder => {
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
        }
        TxnKind::Payment => db.transaction_mut_ok(|txn| {
            let warehouse = get_warehouse(txn);
            // TODO: need to initialize other warehouses
            black_box(payment::random_payment(txn, warehouse, &[]));
        }),
        TxnKind::OrderStatus => db.transaction(|txn| {
            let warehouse = get_warehouse(txn);
            black_box(order_status::random_order_status(txn, warehouse));
        }),
        TxnKind::Delivery => black_box(delivery::random_delivery(db, warehouse)),
        TxnKind::StockLevel => db.transaction(|txn| {
            let district = get_district(txn);
            black_box(stock_level::random_stock_level(txn, district));
        }),
    }
    let elapsed = before.elapsed();
    match txn_kind {
        TxnKind::NewOrder => STATS.new_order.add_sample(elapsed),
        TxnKind::Payment => STATS.payment.add_sample(elapsed),
        TxnKind::OrderStatus => STATS.order_status.add_sample(elapsed),
        TxnKind::Delivery => STATS.delivery.add_sample(elapsed),
        TxnKind::StockLevel => STATS.stock_level.add_sample(elapsed),
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

struct TxnStats {
    cnt: AtomicU64,
    time_ms: AtomicU64,
    late: AtomicU64,
    max_time: Duration,
}

impl std::fmt::Display for TxnStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cnt = self.cnt.load(std::sync::atomic::Ordering::Acquire);
        let late = self.late.load(std::sync::atomic::Ordering::Acquire);
        let time = Duration::from_millis(self.time_ms.load(std::sync::atomic::Ordering::Acquire));
        write!(
            f,
            "cnt: {cnt}, late: {}%, avg: {}ms",
            late as f64 / cnt as f64 * 100.,
            time.checked_div(cnt as u32)
                .map(|x| x.as_millis())
                .unwrap_or_default()
        )
    }
}

impl TxnStats {
    pub const fn new(max_time: Duration) -> Self {
        Self {
            cnt: AtomicU64::new(0),
            time_ms: AtomicU64::new(0),
            late: AtomicU64::new(0),
            max_time,
        }
    }

    pub fn add_sample(&self, dur: Duration) {
        let time_ms = dur.as_millis() as u64;
        self.cnt.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.time_ms
            .fetch_add(time_ms, std::sync::atomic::Ordering::Relaxed);
        if dur > self.max_time {
            self.late.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

static STATS: Stats = Stats {
    new_order: TxnStats::new(Duration::from_secs(5)),
    payment: TxnStats::new(Duration::from_secs(5)),
    order_status: TxnStats::new(Duration::from_secs(5)),
    delivery: TxnStats::new(Duration::from_secs(80)),
    stock_level: TxnStats::new(Duration::from_secs(20)),
};

struct Stats {
    new_order: TxnStats,
    payment: TxnStats,
    order_status: TxnStats,
    delivery: TxnStats,
    stock_level: TxnStats,
}

pub fn print_stats() {
    println!("new_order:    {}", STATS.new_order);
    println!("delivery:     {}", STATS.delivery);
    println!("order_status: {}", STATS.order_status);
    println!("payment:      {}", STATS.payment);
    println!("stock_level:  {}", STATS.stock_level);
}
