mod chinook_schema;

use std::fmt::Debug;

use chinook_schema::*;
use expect_test::expect_file;
use rust_query::{
    aggregate, Column, Dummy, IntoColumn, IntoDummy, LocalClient, Table, TableRow, Transaction,
    Update,
};

/// requires [PartialEq] to get rid of unused warnings.
fn assert_dbg(val: impl Debug + PartialEq, file_name: &str) {
    let path = format!("chinook_tests/{file_name}.dbg");
    expect_file![path].assert_debug_eq(&val);
}

#[test]
fn test_queries() {
    let mut client = LocalClient::try_new().unwrap();
    let db = migrate(&mut client);
    let mut db = client.transaction_mut(&db);

    let res = invoice_info(&db);
    assert_dbg(&res[..20], "invoice_info");
    let res = playlist_track_count(&db);
    assert_dbg(&res[..], "playlist_track_count");
    let res = avg_album_track_count_for_artist(&db);
    assert_dbg(&res[..20], "avg_album_track_count_for_artist");
    let res = count_reporting(&db);
    assert_dbg(&res[..], "count_reporting");
    let res = list_all_genres(&db);
    assert_dbg(&res[..20], "list_all_genres");
    let res = filtered_track(&db, "Metal", 1000 * 60);
    assert_dbg(&res[..], "filtered_track");
    let res = genre_statistics(&db);
    assert_dbg(&res[..20], "genre_statistics");
    let res = all_customer_spending(&db);
    assert_dbg(&res[..20], "customer_spending");
    let res = get_the_artists(&db);
    assert_dbg(&res[..], "the_artists");
    let res = ten_space_tracks(&db);
    assert_dbg(&res[..], "ten_space_tracks");

    free_reference(&db);

    db.try_insert(Artist { name: "first" }).unwrap();
    let id = db.try_insert(Artist { name: "second" }).unwrap();

    let Err(_) = db.try_update(
        id,
        Artist {
            name: Update::set("first"),
        },
    ) else {
        panic!()
    };
    db.try_update(
        id,
        Artist {
            name: Update::set("other"),
        },
    )
    .unwrap();
    assert_eq!(db.query_one(id.name()), "other");

    let mut db = db.downgrade();
    assert!(db.try_delete(id).unwrap());
}

#[derive(Debug, Dummy, PartialEq)]
struct InvoiceInfo<'a> {
    track: String,
    artist: String,
    ivl_id: TableRow<'a, InvoiceLine>,
}

fn invoice_info<'a>(db: &'a Transaction<Schema>) -> Vec<InvoiceInfo<'a>> {
    db.query(|rows| {
        let ivl = InvoiceLine::join(rows);
        rows.into_vec(InvoiceInfoDummy {
            track: ivl.track().name(),
            artist: ivl.track().album().artist().name(),
            ivl_id: ivl,
        })
    })
}

#[derive(Debug, Dummy, PartialEq)]
struct PlaylistTrackCount {
    playlist: String,
    track_count: i64,
}

fn playlist_track_count(db: &Transaction<Schema>) -> Vec<PlaylistTrackCount> {
    db.query(|rows| {
        let pl = Playlist::join(rows);
        let track_count = aggregate(|rows| {
            let plt = PlaylistTrack::join(rows);
            rows.filter_on(plt.playlist(), &pl);
            rows.count_distinct(plt)
        });

        rows.into_vec(PlaylistTrackCountDummy {
            playlist: pl.name(),
            track_count,
        })
    })
}

fn avg_album_track_count_for_artist(db: &Transaction<Schema>) -> Vec<(String, Option<f64>)> {
    db.query(|rows| {
        let artist = Artist::join(rows);
        let avg_track_count = aggregate(|rows| {
            let album = Album::join(rows);
            rows.filter_on(album.artist(), &artist);

            let track_count = aggregate(|rows| {
                let track = Track::join(rows);
                rows.filter_on(track.album(), album);

                rows.count_distinct(track)
            });
            rows.avg(track_count.as_float())
        });
        rows.into_vec((artist.name(), avg_track_count))
    })
}

fn count_reporting(db: &Transaction<Schema>) -> Vec<(String, i64)> {
    db.query(|rows| {
        let receiver = Employee::join(rows);
        let report_count = aggregate(|rows| {
            let reporter = Employee::join(rows);
            // only count employees that report to someone
            let reports_to = rows.filter_some(reporter.reports_to());
            rows.filter_on(reports_to, &receiver);
            rows.count_distinct(reporter)
        });

        rows.into_vec((receiver.last_name(), report_count))
    })
}

fn list_all_genres(db: &Transaction<Schema>) -> Vec<String> {
    db.query(|rows| {
        let genre = Genre::join(rows);
        rows.into_vec(genre.name())
    })
}

#[derive(Debug, Dummy, PartialEq)]
struct FilteredTrack {
    track_name: String,
    album_name: String,
    stats: Stats,
}

#[derive(Debug, Dummy, PartialEq)]
struct Stats {
    milis: i64,
}

fn filtered_track(db: &Transaction<Schema>, genre: &str, max_milis: i64) -> Vec<FilteredTrack> {
    db.query(|rows| {
        let track = Track::join(rows);
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

#[derive(Debug, Dummy, PartialEq)]
struct GenreStats {
    genre_name: String,
    byte_average: f64,
    milis_average: f64,
}

fn genre_statistics(db: &Transaction<Schema>) -> Vec<GenreStats> {
    db.query(|rows| {
        let genre = Genre::join(rows);
        let (bytes, milis) = aggregate(|rows| {
            let track = Track::join(rows);
            rows.filter_on(track.genre(), &genre);
            (
                rows.avg(track.bytes().as_float()),
                rows.avg(track.milliseconds().as_float()),
            )
        });
        rows.into_vec(GenreStatsDummy {
            genre_name: genre.name(),
            byte_average: bytes.map_dummy(|x| x.unwrap()),
            milis_average: milis.map_dummy(|x| x.unwrap()),
        })
    })
}

#[derive(Debug, Dummy, PartialEq)]
struct CustomerSpending {
    customer_name: String,
    total_spending: f64,
}

fn all_customer_spending(db: &Transaction<Schema>) -> Vec<CustomerSpending> {
    db.query(|rows| {
        let customer = Customer::join(rows);
        let total = customer_spending(&customer);

        rows.into_vec(CustomerSpendingDummy {
            customer_name: customer.last_name(),
            total_spending: total,
        })
    })
}

fn customer_spending<'t>(
    customer: impl IntoColumn<'t, Schema, Typ = Customer>,
) -> Column<'t, Schema, f64> {
    aggregate(|rows| {
        let invoice = Invoice::join(rows);
        rows.filter_on(invoice.customer(), customer);
        rows.sum(invoice.total())
    })
}

fn customer_spending_by_email(db: &Transaction<Schema>, email: &str) -> Option<f64> {
    let customer = db.query_one(Customer::unique_by_email(email));
    customer.map(|x| db.query_one(customer_spending(&x)))
}

fn free_reference(db: &Transaction<Schema>) {
    let tracks = db.query(|rows| {
        let track = Track::join(rows);
        rows.into_vec(track)
    });

    for track in tracks {
        let _name = db.query_one(track.album().artist().name());
    }
}

#[derive(Dummy)]
struct TrackStats {
    avg_len_milis: Option<f64>,
    max_len_milis: Option<i64>,
    genre_count: i64,
}

#[derive(Dummy)]
struct ArtistDetails {
    name: String,
    album_count: i64,
    track_stats: TrackStats,
}

fn artist_details<'a>(db: &Transaction<'a, Schema>, artist: TableRow<'a, Artist>) -> ArtistDetails {
    db.query_one(ArtistDetailsDummy {
        name: artist.name(),
        album_count: aggregate(|rows| {
            let album = Album::join(rows);
            rows.filter_on(album.artist(), artist);
            rows.count_distinct(album)
        }),
        track_stats: aggregate(|rows| {
            let track = Track::join(rows);
            rows.filter_on(track.album().artist(), artist);
            TrackStatsDummy {
                avg_len_milis: rows.avg(track.milliseconds().as_float()),
                max_len_milis: rows.max(track.milliseconds()),
                genre_count: rows.count_distinct(track.genre()),
            }
        }),
    })
}

fn get_the_artists(db: &Transaction<Schema>) -> Vec<String> {
    db.query(|rows| {
        let artist = Artist::join(rows);
        rows.filter(artist.name().starts_with("The "));
        rows.into_vec(artist.name())
    })
}

fn ten_space_tracks(db: &Transaction<Schema>) -> Vec<String> {
    db.query(|rows| {
        let track = Track::join(rows);
        rows.filter(track.name().like("% % % % % % % % % % %"));
        rows.into_vec(track.name())
    })
}
