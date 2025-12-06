use std::{
    hint::black_box,
    iter::repeat_n,
    ops::ControlFlow,
    sync::{Arc, Condvar, Mutex, atomic::AtomicU64},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rand::seq::SliceRandom;
use rust_query::Database;

use crate::{delivery, new_order, order_status, payment, stock_level, v0::Schema};

pub(crate) struct EmulateWithQueue {
    pub info: Arc<Emulate>,
    pub queue: Vec<JoinHandle<()>>,
}

pub(crate) struct Emulate {
    pub db: Arc<Database<Schema>>,
    pub warehouse: i64,
    pub district: i64,
    pub other_warehouses: Vec<i64>,
}

impl EmulateWithQueue {
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
            let info = self.info.clone();
            self.queue
                .push(thread::spawn(move || info.measure_txn_rt(txn_kind)));
        } else {
            self.info.measure_txn_rt(txn_kind)
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

impl Emulate {
    fn measure_txn_rt(&self, txn_kind: TxnKind) {
        let db = &self.db;
        let stats = match txn_kind {
            TxnKind::NewOrder => &STATS.new_order,
            TxnKind::Payment => &STATS.payment,
            TxnKind::OrderStatus => &STATS.order_status,
            TxnKind::Delivery => &STATS.delivery,
            TxnKind::StockLevel => &STATS.stock_level,
        };
        let before = Instant::now();
        let mut start = None;
        match txn_kind {
            TxnKind::NewOrder => {
                let input = new_order::generate_input(self.warehouse, &self.other_warehouses);
                let _ = black_box(db.transaction_mut(|txn| {
                    start = Some(Instant::now());
                    new_order::new_order(txn, input)
                }));
            }
            TxnKind::Payment => {
                let input = payment::generate_input(self.warehouse, &self.other_warehouses);
                black_box(db.transaction_mut_ok(|txn| {
                    start = Some(Instant::now());
                    payment::payment(txn, input)
                }));
            }
            TxnKind::OrderStatus => {
                let input = order_status::generate_input(self.warehouse);
                black_box(db.transaction(|txn| {
                    start = Some(Instant::now());
                    order_status::order_status(txn, input)
                }));
            }
            TxnKind::Delivery => {
                let input = delivery::generate_input(self.warehouse);
                for district_num in 1..=10 {
                    // use separate transactions to allow other threads to do stuff in between
                    black_box(db.transaction_mut_ok(|txn| {
                        start = Some(Instant::now()); // we only measure the last one
                        delivery::delivery(txn, &input, district_num)
                    }));
                }
            }
            TxnKind::StockLevel => {
                let input = stock_level::generate_input(self.warehouse, self.district);
                black_box(db.transaction(|txn| {
                    start = Some(Instant::now());
                    stock_level::stock_level(txn, input)
                }));
            }
        }
        stats.add_total_time(before.elapsed());
        stats.add_individual_time(start.unwrap().elapsed());
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

pub fn stop_emulation(f: impl FnOnce()) {
    *STOP.should_stop.lock().unwrap() = true;
    STOP.condvar.notify_all();
    f();
    *STOP.should_stop.lock().unwrap() = false;
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
        let cnt = self.cnt();
        let late = self.late.load(std::sync::atomic::Ordering::Relaxed);
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

    pub fn cnt(&self) -> u64 {
        self.cnt.load(std::sync::atomic::Ordering::Relaxed)
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
    pub fn add_individual_time(&self, dur: Duration) {
        self.time_cnt
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.time_us
            .fetch_add(dur.as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn average_time(&self) -> Duration {
        let time = Duration::from_micros(self.time_us.load(std::sync::atomic::Ordering::Acquire));
        let time_cnt = self.time_cnt.load(std::sync::atomic::Ordering::Acquire);
        time.checked_div(time_cnt as u32).unwrap_or_default()
    }

    pub fn reset_ok(&self) -> bool {
        let cnt = self.cnt.load(std::sync::atomic::Ordering::Relaxed);
        let late = self.late.load(std::sync::atomic::Ordering::Relaxed);
        self.cnt.store(0, std::sync::atomic::Ordering::Relaxed);
        self.late.store(0, std::sync::atomic::Ordering::Relaxed);
        self.time_us.store(0, std::sync::atomic::Ordering::Relaxed);
        self.time_cnt.store(0, std::sync::atomic::Ordering::Relaxed);

        // check that at most 10% is late
        late * 10 <= cnt
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

pub fn reset_ok() -> bool {
    STATS.new_order.reset_ok()
        & STATS.delivery.reset_ok()
        & STATS.order_status.reset_ok()
        & STATS.payment.reset_ok()
        & STATS.stock_level.reset_ok()
}

pub fn print_stats(start: Instant) -> bool {
    let new_order_cnt = STATS.new_order.cnt();
    let dur = start.elapsed();

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

    let success = reset_ok();

    if success {
        println!(
            "achieved tpmC: {}",
            new_order_cnt as u128 * Duration::from_secs(60).as_nanos() / dur.as_nanos()
        );
        println!(
            "expected max tpmC: {}",
            (Duration::from_secs(60).as_nanos() as f64 / total_time_for_cycle.as_nanos() as f64)
                as u64
        );
    } else {
        println!("percentage of late transactions is too high, stopping benchmark")
    }

    success
}
