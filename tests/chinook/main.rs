mod schema;

use std::fmt::Debug;

use expect_test::expect_file;
use rust_query::{
    Expr, IntoExpr, IntoSelect, LocalClient, Select, TableRow, Transaction, Update, aggregate,
    optional,
};
use schema::*;

fn assert_dbg<T: Debug + PartialOrd>(file_name: &str, f: impl FnOnce() -> Vec<T>) {
    let (mut val, plan) = rust_query::private::get_plan(f);
    let mut val = &mut val[..];
    val.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if val.len() > 20 {
        val = &mut val[..20];
    }
    let path = format!("expect/{file_name}.dbg");
    expect_file![path].assert_debug_eq(&val);
    let path = format!("expect/{file_name}.plan");
    expect_file![path].assert_debug_eq(&plan);
}

#[test]
fn test_queries() {
    let mut client = LocalClient::try_new().unwrap();
    let db = migrate(&mut client);
    let mut db = client.transaction_mut(&db);

    assert_dbg("invoice_info", || invoice_info(&db));
    assert_dbg("playlist_track_count", || playlist_track_count(&db));
    assert_dbg("avg_album_track_count_for_artist", || {
        avg_album_track_count_for_artist(&db)
    });
    assert_dbg("count_reporting", || count_reporting(&db));
    assert_dbg("list_all_genres", || list_all_genres(&db));
    assert_dbg("filtered_track", || filtered_track(&db, "Metal", 1000 * 60));
    assert_dbg("genre_statistics", || genre_statistics(&db));
    assert_dbg("customer_spending", || all_customer_spending(&db));
    assert_dbg("the_artists", || get_the_artists(&db));
    assert_dbg("ten_space_tracks", || ten_space_tracks(&db));
    assert_dbg("high_avg_invoice_total", || high_avg_invoice_total(&db));
    let artist = db.query_one(Artist::unique("U2")).unwrap();
    assert_dbg("artist_details", || vec![artist_details(&db, artist)]);
    assert_eq!(
        customer_spending_by_email(&db, "vstevens@yahoo.com"),
        Some(42.62)
    );
    assert_eq!(customer_spending_by_email(&db, "asdf"), None);

    free_reference(&db);

    db.insert(Artist { name: "first" }).unwrap();
    let id = db.insert(Artist { name: "second" }).unwrap();

    let Err(_) = db.update(
        id,
        Artist {
            name: Update::set("first"),
        },
    ) else {
        panic!()
    };
    db.update(
        id,
        Artist {
            name: Update::set("other"),
        },
    )
    .unwrap();
    assert_eq!(db.query_one(&id.into_expr().name), "other");

    let mut db = db.downgrade();
    assert!(db.delete(id).unwrap());
}

#[derive(Debug, Select, PartialEq, PartialOrd)]
struct InvoiceInfo<'a> {
    track: String,
    artist: String,
    ivl_id: TableRow<'a, InvoiceLine>,
}

fn invoice_info<'a>(db: &'a Transaction<Schema>) -> Vec<InvoiceInfo<'a>> {
    db.query(|rows| {
        let ivl = rows.join(InvoiceLine);
        rows.into_vec(InvoiceInfoSelect {
            track: &ivl.track.name,
            artist: &ivl.track.album.artist.name,
            ivl_id: &ivl,
        })
    })
}

#[derive(Debug, Select, PartialEq, PartialOrd)]
struct PlaylistTrackCount {
    playlist: String,
    track_count: i64,
}

fn playlist_track_count(db: &Transaction<Schema>) -> Vec<PlaylistTrackCount> {
    db.query(|rows| {
        let pl = rows.join(Playlist);
        let track_count = aggregate(|rows| {
            let plt = rows.join(PlaylistTrack);
            rows.filter(&plt.playlist.eq(&pl));
            rows.count_distinct(plt)
        });

        rows.into_vec(PlaylistTrackCountSelect {
            playlist: &pl.name,
            track_count,
        })
    })
}

fn avg_album_track_count_for_artist(db: &Transaction<Schema>) -> Vec<(String, Option<f64>)> {
    db.query(|rows| {
        let artist = rows.join(Artist);
        let avg_track_count = aggregate(|rows| {
            let album = rows.join(Album);
            rows.filter(album.artist.eq(&artist));

            let track_count = aggregate(|rows| {
                let track = rows.join(Track);
                rows.filter(track.album.eq(album));

                rows.count_distinct(track)
            });
            rows.avg(track_count.as_float())
        });
        rows.into_vec((&artist.name, avg_track_count))
    })
}

fn count_reporting(db: &Transaction<Schema>) -> Vec<(String, i64)> {
    db.query(|rows| {
        let receiver = rows.join(Employee);
        let report_count = aggregate(|rows| {
            let reporter = rows.join(Employee);
            // only count employees that report to someone
            let reports_to = rows.filter_some(&reporter.reports_to);
            rows.filter(reports_to.eq(&receiver));
            rows.count_distinct(reporter)
        });

        rows.into_vec((&receiver.last_name, report_count))
    })
}

fn list_all_genres(db: &Transaction<Schema>) -> Vec<String> {
    db.query(|rows| {
        let genre = rows.join(Genre);
        rows.into_vec(&genre.name)
    })
}

#[derive(Debug, Select, PartialEq, PartialOrd)]
struct FilteredTrack {
    track_name: String,
    album_name: String,
    stats: Stats,
}

#[derive(Debug, Select, PartialEq, PartialOrd)]
struct Stats {
    milis: i64,
}

fn filtered_track(db: &Transaction<Schema>, genre: &str, max_milis: i64) -> Vec<FilteredTrack> {
    db.query(|rows| {
        let track = rows.join(Track);
        rows.filter(track.genre.name.eq(genre));
        rows.filter(track.milliseconds.lt(max_milis));
        rows.into_vec(FilteredTrackSelect {
            track_name: &track.name,
            album_name: &track.album.title,
            stats: StatsSelect {
                milis: &track.milliseconds,
            },
        })
    })
}

#[derive(Debug, Select, PartialEq, PartialOrd)]
struct GenreStats {
    genre_name: String,
    byte_average: f64,
    milis_average: f64,
}

fn genre_statistics(db: &Transaction<Schema>) -> Vec<GenreStats> {
    db.query(|rows| {
        let genre = rows.join(Genre);
        let (bytes, milis) = aggregate(|rows| {
            let track = rows.join(Track);
            rows.filter(&track.genre.eq(&genre));
            (
                rows.avg(track.bytes.as_float()),
                rows.avg(track.milliseconds.as_float()),
            )
        });
        rows.into_vec(GenreStatsSelect {
            genre_name: &genre.name,
            byte_average: bytes.into_select().map(|x| x.unwrap()),
            milis_average: milis.into_select().map(|x| x.unwrap()),
        })
    })
}

#[derive(Debug, Select, PartialEq, PartialOrd)]
struct HighInvoiceInfo {
    customer_name: String,
    avg_spend: f64,
    high_avg_spend: f64,
}

fn high_avg_invoice_total(db: &Transaction<Schema>) -> Vec<HighInvoiceInfo> {
    db.query(|q_rows| {
        let customer = q_rows.join(Customer);
        aggregate(|rows| {
            let invoice = rows.join(Invoice);
            rows.filter(invoice.customer.eq(&customer));
            let avg = q_rows.filter_some(rows.avg(&invoice.total));
            rows.filter(invoice.total.gt(&avg));
            let high_avg = q_rows.filter_some(rows.avg(&invoice.total));
            q_rows.into_vec(HighInvoiceInfoSelect {
                customer_name: &customer.last_name,
                avg_spend: avg,
                high_avg_spend: high_avg,
            })
        })
    })
}

#[derive(Debug, Select, PartialEq, PartialOrd)]
struct CustomerSpending {
    customer_name: String,
    total_spending: f64,
}

fn all_customer_spending(db: &Transaction<Schema>) -> Vec<CustomerSpending> {
    db.query(|rows| {
        let customer = rows.join(Customer);
        let total = customer_spending(&customer);

        rows.into_vec(CustomerSpendingSelect {
            customer_name: &customer.last_name,
            total_spending: total,
        })
    })
}

fn customer_spending<'t>(
    customer: impl IntoExpr<'t, Schema, Typ = Customer>,
) -> Expr<'t, Schema, f64> {
    let customer = customer.into_expr();
    aggregate(|rows| {
        let invoice = rows.join(Invoice);
        rows.filter(invoice.customer.eq(customer));
        rows.sum(&invoice.total)
    })
}

fn customer_spending_by_email(db: &Transaction<Schema>, email: &str) -> Option<f64> {
    db.query_one(optional(|row| {
        let customer = row.and(Customer::unique_by_email(email));
        row.then(customer_spending(customer))
    }))
}

fn free_reference(db: &Transaction<Schema>) {
    let tracks = db.query(|rows| {
        let track = rows.join(Track);
        rows.into_vec(track)
    });

    for track in tracks {
        let _name = db.query_one(&track.into_expr().album.artist.name);
    }
}

#[derive(Select, PartialEq, PartialOrd, Debug)]
struct TrackStats {
    avg_len_milis: Option<f64>,
    max_len_milis: Option<i64>,
    genre_count: i64,
}

#[derive(Select, PartialEq, PartialOrd, Debug)]
struct ArtistDetails {
    name: String,
    album_count: i64,
    track_stats: TrackStats,
}

fn artist_details<'a>(db: &Transaction<'a, Schema>, artist: TableRow<'a, Artist>) -> ArtistDetails {
    db.query_one(ArtistDetailsSelect {
        name: &artist.into_expr().name,
        album_count: aggregate(|rows| {
            let album = rows.join(Album);
            rows.filter(album.artist.eq(artist));
            rows.count_distinct(album)
        }),
        track_stats: aggregate(|rows| {
            let track = rows.join(Track);
            rows.filter(track.album.artist.eq(artist));
            TrackStatsSelect {
                avg_len_milis: rows.avg(track.milliseconds.as_float()),
                max_len_milis: rows.max(&track.milliseconds),
                genre_count: rows.count_distinct(&track.genre),
            }
        }),
    })
}

fn get_the_artists(db: &Transaction<Schema>) -> Vec<String> {
    db.query(|rows| {
        let artist = rows.join(Artist);
        rows.filter(artist.name.starts_with("The "));
        rows.into_vec(&artist.name)
    })
}

fn ten_space_tracks(db: &Transaction<Schema>) -> Vec<String> {
    db.query(|rows| {
        let track = rows.join(Track);
        rows.filter(track.name.like("% % % % % % % % % % %"));
        rows.into_vec(&track.name)
    })
}
