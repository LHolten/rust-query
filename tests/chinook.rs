mod chinook_schema;

use std::cell::Cell;
use std::sync::{LazyLock, Mutex};
use std::{cell::LazyCell, fmt::Debug};

use chinook_schema::*;
use expect_test::expect_file;
use rust_query::{
    DbClient, Free, FromRow, LatestToken, Snapshot, SnapshotToken, ThreadToken, Value,
};

struct DbShared {
    latest: Mutex<LatestToken<Schema>>,
    snapshot: SnapshotToken<Schema>,
}

static DATABASE: LazyLock<DbShared> = LazyLock::new(|| {
    let client = migrate();
    DbShared {
        latest: Mutex::new(client.latest),
        snapshot: client.snapshot,
    }
});

pub fn db() -> &'static Schema {
    DB.with(|x| **x)
}

thread_local! {
    static TOKEN: Cell<Option<ThreadToken<Schema>>> = Cell::new(None);

    static DB: LazyCell<&'static Schema> = LazyCell::new(|| {
        let token = ThreadToken::acquire().unwrap();
        let (token, schema) = token.schema();
        TOKEN.set(Some(token));
        Box::leak(Box::new(schema))
    });
}

fn assert_dbg(val: impl Debug, file_name: &str) {
    let path = format!("chinook_tests/{file_name}.dbg");
    expect_file![path].assert_debug_eq(&val);
}

#[test]
fn test_queries() {
    LazyLock::force(&DATABASE);
    DB.with(|db| {
        LazyCell::force(&db);
    });
    let mut thread_token = TOKEN.take().unwrap();
    let mut latest_token = DATABASE.latest.lock().unwrap();
    let mut client = latest_token.latest(&mut thread_token);

    let res = invoice_info(&client);
    assert_dbg(&res[..20], "invoice_info");
    let res = playlist_track_count(&client);
    assert_dbg(&res[..], "playlist_track_count");
    let res = avg_album_track_count_for_artist(&client);
    assert_dbg(&res[..20], "avg_album_track_count_for_artist");
    let res = count_reporting(&client);
    assert_dbg(&res[..], "count_reporting");
    let res = list_all_genres(&client);
    assert_dbg(&res[..20], "list_all_genres");
    let res = filtered_track(&client, "Metal", 1000 * 60);
    assert_dbg(&res[..], "filtered_track");
    let res = genre_statistics(&client);
    assert_dbg(&res[..20], "genre_statistics");
    let res = customer_spending(&client);
    assert_dbg(&res[..20], "customer_spending");

    let artist_name = "my cool artist".to_string();
    let id = client.try_insert(ArtistDummy { name: artist_name });
    assert!(id.is_some());
}

#[derive(Debug, FromRow)]
struct InvoiceInfo<'a> {
    track: String,
    artist: String,
    ivl_id: Free<'a, InvoiceLine>,
}

fn invoice_info<'a>(client: &'a Snapshot) -> Vec<InvoiceInfo<'a>> {
    client.exec(|rows| {
        let ivl = rows.join(&db().invoice_line);
        rows.into_vec(InvoiceInfoDummy {
            track: ivl.track().name(),
            artist: ivl.track().album().artist().name(),
            ivl_id: ivl,
        })
    })
}

#[derive(Debug, FromRow)]
struct PlaylistTrackCount {
    playlist: String,
    track_count: i64,
}

fn playlist_track_count(client: &Snapshot) -> Vec<PlaylistTrackCount> {
    client.exec(|rows| {
        let pl = rows.join(&db().playlist);
        let track_count = rows.query(|rows| {
            let plt = rows.join(&db().playlist_track);
            rows.filter_on(plt.playlist(), pl);
            rows.count_distinct(plt)
        });

        rows.into_vec(PlaylistTrackCountDummy {
            playlist: pl.name(),
            track_count,
        })
    })
}

fn avg_album_track_count_for_artist(client: &Snapshot) -> Vec<(String, Option<f64>)> {
    client.exec(|rows| {
        let artist = rows.join(&db().artist);
        let avg_track_count = rows.query(|rows| {
            let album = rows.join(&db().album);
            rows.filter_on(album.artist(), artist);

            let track_count = rows.query(|rows| {
                let track = rows.join(&db().track);
                rows.filter_on(track.album(), album);

                rows.count_distinct(track)
            });
            rows.avg(track_count.as_float())
        });
        rows.into_vec((artist.name(), avg_track_count))
    })
}

fn count_reporting(client: &Snapshot) -> Vec<(String, i64)> {
    client.exec(|rows| {
        let receiver = rows.join(&db().employee);
        let report_count = rows.query(|rows| {
            let reporter = rows.join(&db().employee);
            // only count employees that report to someone
            let reports_to = rows.filter_some(reporter.reports_to());
            rows.filter_on(reports_to, receiver);
            rows.count_distinct(reporter)
        });

        rows.into_vec((receiver.last_name(), report_count))
    })
}

fn list_all_genres(client: &Snapshot) -> Vec<String> {
    client.exec(|rows| {
        let genre = rows.join(&db().genre_new);
        rows.into_vec(genre.name())
    })
}

#[derive(Debug, FromRow)]
struct FilteredTrack {
    track_name: String,
    album_name: String,
    stats: Stats,
}

#[derive(Debug, FromRow)]
struct Stats {
    milis: i64,
}

fn filtered_track(client: &Snapshot, genre: &str, max_milis: i64) -> Vec<FilteredTrack> {
    client.exec(|rows| {
        let track = rows.join(&db().track);
        rows.filter(track.genre().name().eq(genre));
        rows.filter(track.milliseconds().lt(max_milis));
        rows.into_vec(FilteredTrackDummy {
            track_name: track.name(),
            album_name: track.album().title(),
            stats: StatsDummy {
                milis: track.milliseconds(),
            },
        })
    })
}

#[derive(Debug, FromRow)]
struct GenreStats {
    genre_name: String,
    byte_average: Option<f64>,
    milis_average: Option<f64>,
}

fn genre_statistics(client: &Snapshot) -> Vec<GenreStats> {
    client.exec(|rows| {
        let genre = rows.join(&db().genre_new);
        let (bytes, milis) = rows.query(|rows| {
            let track = rows.join(&db().track);
            rows.filter_on(track.genre(), genre);
            (
                rows.avg(track.bytes().as_float()),
                rows.avg(track.milliseconds().as_float()),
            )
        });
        rows.into_vec(GenreStatsDummy {
            genre_name: genre.name(),
            byte_average: bytes,
            milis_average: milis,
        })
    })
}

#[derive(Debug, FromRow)]
struct CustomerSpending {
    customer_name: String,
    total_spending: f64,
}

fn customer_spending(client: &Snapshot) -> Vec<CustomerSpending> {
    client.exec(|rows| {
        let customer = rows.join(&db().customer);
        let total = rows.query(|rows| {
            let invoice = rows.join(&db().invoice);
            rows.filter_on(invoice.customer(), customer);
            rows.sum_float(invoice.total())
        });

        rows.into_vec(CustomerSpendingDummy {
            customer_name: customer.last_name(),
            total_spending: total,
        })
    })
}

fn free_reference(c: Snapshot) {
    let tracks = c.exec(|rows| {
        let track = rows.join(&db().track);
        rows.into_vec(track)
    });

    for track in tracks {
        let name = c.get(track.album().artist().name());
    }
}
