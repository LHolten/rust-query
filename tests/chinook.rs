mod chinook_schema;

use std::fmt::Debug;
use std::ops::Deref;
use std::sync::LazyLock;
use std::sync::Mutex;

use chinook_schema::*;
use expect_test::expect_file;
use rust_query::Free;
use rust_query::FromRow;
use rust_query::{Client, Value};

static CLIENT: Mutex<Option<Client>> = Mutex::new(None);
static DB: LazyLock<Schema> = LazyLock::new(|| {
    let (client, schema) = migrate();
    CLIENT.lock().unwrap().replace(client);
    schema
});

fn assert_dbg(val: impl Debug, file_name: &str) {
    let path = format!("chinook_tests/{file_name}.dbg");
    expect_file![path].assert_debug_eq(&val);
}

#[test]
fn test_queries() {
    let _ = DB.deref();
    let client = CLIENT.lock().unwrap().take().unwrap();

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

fn invoice_info(client: &Client) -> Vec<InvoiceInfo> {
    client.exec(|rows| {
        let ivl = rows.join(&DB.invoice_line);
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

fn playlist_track_count(client: &Client) -> Vec<PlaylistTrackCount> {
    client.exec(|rows| {
        let pl = rows.join(&DB.playlist);
        let track_count = rows.query(|rows| {
            let plt = rows.join(&DB.playlist_track);
            rows.filter_on(plt.playlist(), pl);
            rows.count_distinct(plt)
        });

        rows.into_vec(PlaylistTrackCountDummy {
            playlist: pl.name(),
            track_count,
        })
    })
}

fn avg_album_track_count_for_artist(client: &Client) -> Vec<(String, Option<f64>)> {
    client.exec(|rows| {
        let artist = rows.join(&DB.artist);
        let avg_track_count = rows.query(|rows| {
            let album = rows.join(&DB.album);
            rows.filter_on(album.artist(), artist);

            let track_count = rows.query(|rows| {
                let track = rows.join(&DB.track);
                rows.filter_on(track.album(), album);

                rows.count_distinct(track)
            });
            rows.avg(track_count.as_float())
        });
        rows.into_vec((artist.name(), avg_track_count))
    })
}

fn count_reporting(client: &Client) -> Vec<(String, i64)> {
    client.exec(|rows| {
        let receiver = rows.join(&DB.employee);
        let report_count = rows.query(|rows| {
            let reporter = rows.join(&DB.employee);
            // only count employees that report to someone
            let reports_to = rows.filter_some(reporter.reports_to());
            rows.filter_on(reports_to, receiver);
            rows.count_distinct(reporter)
        });

        rows.into_vec((receiver.last_name(), report_count))
    })
}

fn list_all_genres(client: &Client) -> Vec<String> {
    client.exec(|rows| {
        let genre = rows.join(&DB.genre_new);
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

fn filtered_track(client: &Client, genre: &str, max_milis: i64) -> Vec<FilteredTrack> {
    client.exec(|rows| {
        let track = rows.join(&DB.track);
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

fn genre_statistics(client: &Client) -> Vec<GenreStats> {
    client.exec(|rows| {
        let genre = rows.join(&DB.genre_new);
        let (bytes, milis) = rows.query(|rows| {
            let track = rows.join(&DB.track);
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

fn customer_spending(client: &Client) -> Vec<CustomerSpending> {
    client.exec(|rows| {
        let customer = rows.join(&DB.customer);
        let total = rows.query(|rows| {
            let invoice = rows.join(&DB.invoice);
            rows.filter_on(invoice.customer(), customer);
            rows.sum_float(invoice.total())
        });

        rows.into_vec(CustomerSpendingDummy {
            customer_name: customer.last_name(),
            total_spending: total,
        })
    })
}

fn free_reference(c: Client) {
    let tracks = c.exec(|rows| {
        let track = rows.join(&DB.track);
        rows.into_vec(track)
    });

    for track in tracks {
        let name = c.get(track.album().artist().name());
    }
}
