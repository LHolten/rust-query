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
    let stats = match txn_kind {
        TxnKind::NewOrder => &STATS.new_order,
        TxnKind::Payment => &STATS.payment,
        TxnKind::OrderStatus => &STATS.order_status,
        TxnKind::Delivery => &STATS.delivery,
        TxnKind::StockLevel => &STATS.stock_level,
    };
    let before = Instant::now();
    match txn_kind {
        TxnKind::NewOrder => {
            let _ = db.transaction_mut(|txn| {
                let warehouse = get_warehouse(txn);
                // TODO: need to initialize other warehouses
                stats
                    .add_individual_time(|| new_order::random_new_order(txn, warehouse, &[]))
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
            black_box(stats.add_individual_time(|| payment::random_payment(txn, warehouse, &[])));
        }),
        TxnKind::OrderStatus => db.transaction(|txn| {
            let warehouse = get_warehouse(txn);
            black_box(
                stats.add_individual_time(|| order_status::random_order_status(txn, warehouse)),
            );
        }),
        TxnKind::Delivery => {
            let input = delivery::generate_input(warehouse);
            for district_num in 1..=10 {
                // use separate transactions to allow other threads to do stuff in between
                db.transaction_mut_ok(|txn| {
                    // TODO: add output to this function and black_box it
                    stats.add_individual_time(|| delivery::delivery(txn, &input, district_num));
                })
            }
        }
        TxnKind::StockLevel => db.transaction(|txn| {
            let district = get_district(txn);
            black_box(stock_level::random_stock_level(txn, district));
        }),
    }
    let elapsed = before.elapsed();
    stats.add_total_time(elapsed);
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
    late: AtomicU64,
    time_us: AtomicU64,
    time_cnt: AtomicU64,
    max_time: Duration,
}

impl std::fmt::Display for TxnStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cnt = self.cnt.load(std::sync::atomic::Ordering::Acquire);
        let late = self.late.load(std::sync::atomic::Ordering::Acquire);
        write!(
            f,
            "cnt: {cnt}, late: {:.2}%, avg: {}us",
            late as f64 / cnt as f64 * 100.,
            self.average_time().as_micros()
        )
    }
}

impl TxnStats {
    pub const fn new(max_time: Duration) -> Self {
        Self {
            cnt: AtomicU64::new(0),
            late: AtomicU64::new(0),
            time_us: AtomicU64::new(0),
            time_cnt: AtomicU64::new(0),
            max_time,
        }
    }

    /// This is the time that includes beginning the transaction and committing.
    /// For `delivery` it includes all parts of the delivery.
    pub fn add_total_time(&self, dur: Duration) {
        self.cnt.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if dur > self.max_time {
            self.late.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// This is the time after begginging the transaction and before committing.
    /// For `delivery` it includes only one district.
    pub fn add_individual_time<R>(&self, f: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let res = f();
        let dur = start.elapsed();
        self.time_cnt
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.time_us
            .fetch_add(dur.as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
        res
    }

    pub fn average_time(&self) -> Duration {
        let time = Duration::from_micros(self.time_us.load(std::sync::atomic::Ordering::Acquire));
        let time_cnt = self.time_cnt.load(std::sync::atomic::Ordering::Acquire);
        time.checked_div(time_cnt as u32).unwrap_or_default()
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

    // order_status and stock_level are not included because they are read only.
    // the mutable transactions are each run 10 times. (delivery is split in 10)
    let total_time_for_cycle = STATS.new_order.average_time()
        + STATS.payment.average_time()
        + STATS.delivery.average_time();
    println!(
        "expected max tpmC: {}",
        (Duration::from_secs(60).as_nanos() as f64 / total_time_for_cycle.as_nanos() as f64) as u64
    )
}
