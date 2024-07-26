mod schema;

use std::ops::Deref;
use std::sync::LazyLock;
use std::sync::Mutex;

use rust_query::Just;
use rust_query::{Client, Value};
use schema::migrate;
use schema::v2::*;

static CLIENT: Mutex<Option<Client>> = Mutex::new(None);
static DB: LazyLock<schema::v2::Schema> = LazyLock::new(|| {
    let (client, schema) = migrate();
    CLIENT.lock().unwrap().replace(client);
    schema
});

fn main() {
    let _ = DB.deref();
    let client = CLIENT.lock().unwrap().take().unwrap();

    let artist_name = "my cool artist".to_string();
    client.insert(ArtistDummy2 { name: artist_name });

    // let res = invoice_info(&client);
    // let res = playlist_track_count(&client);
    // let res = avg_album_track_count_for_artist(&client);
    // let res = count_reporting(&client);
    let res = list_all_genres(&client);
    // let res = filtered_track(&client, "Metal", 1000 * 60);
    // let res = genre_statistics(&client);
    // let res = customer_spending(&client);
    println!("{res:#?}")
}

#[derive(Debug)]
struct InvoiceInfo {
    track: String,
    artist: String,
    ivl_id: Just<'static, InvoiceLine>,
}

fn invoice_info(client: &'static Client) -> Vec<InvoiceInfo> {
    client.exec(|q| {
        let ivl = q.table(&DB.invoice_line);
        q.into_vec(|row| InvoiceInfo {
            track: row.get(ivl.track().name()),
            artist: row.get(ivl.track().album().artist().name()),
            ivl_id: row.get(ivl),
        })
    })
}

#[derive(Debug)]
struct PlaylistTrackCount {
    playlist: String,
    track_count: i64,
}

fn playlist_track_count(client: &Client) -> Vec<PlaylistTrackCount> {
    client.exec(|q| {
        let pl = q.table(&DB.playlist);
        let track_count = q.query(|q| {
            let plt = q.table(&DB.playlist_track);
            q.filter_on(plt.playlist(), pl);
            q.count_distinct(plt)
        });

        q.into_vec(|row| PlaylistTrackCount {
            playlist: row.get(pl.name()),
            track_count: row.get(track_count),
        })
    })
}

fn avg_album_track_count_for_artist(client: &Client) -> Vec<(String, Option<i64>)> {
    client.exec(|q| {
        let artist = q.table(&DB.artist);
        let avg_track_count = q.query(|q| {
            let album = q.table(&DB.album);
            q.filter_on(album.artist(), artist);

            let track_count = q.query(|q| {
                let track = q.table(&DB.track);
                q.filter_on(track.album(), album);

                q.count_distinct(track)
            });
            q.avg(track_count)
        });
        q.into_vec(|row| (row.get(artist.name()), row.get(avg_track_count)))
    })
}

fn count_reporting(client: &Client) -> Vec<(String, i64)> {
    client.exec(|q| {
        let receiver = q.table(&DB.employee);
        let report_count = q.query(|q| {
            let reporter = q.table(&DB.employee);
            // only count employees that report to someone
            let reports_to = q.filter_some(reporter.reports_to());
            q.filter_on(reports_to, receiver);
            q.count_distinct(reporter)
        });

        q.into_vec(|row| (row.get(receiver.last_name()), row.get(report_count)))
    })
}

/// Tip: use [rust_query::Query::table] and [rust_query::Query::select]
fn list_all_genres(client: &Client) -> Vec<String> {
    todo!()
}

#[derive(Debug)]
struct FilteredTrack {
    track_name: String,
    album_name: String,
    milis: i64,
}

/// Tip: use [rust_query::Const::new] and [rust_query::Query::filter]
/// Tip2: use implicit joins! something like `track.genre.name`
fn filtered_track(client: &Client, genre: &str, max_milis: i64) -> Vec<FilteredTrack> {
    todo!()
}

#[derive(Debug)]
struct GenreStats {
    genre_name: String,
    byte_average: i64,
    milis_average: i64,
}

/// Tip: use [rust_query::Query::project_on] and [rust_query::Group::avg]
fn genre_statistics(client: &Client) -> Vec<GenreStats> {
    todo!()
}

#[derive(Debug)]
struct CustomerSpending {
    customer_name: String,
    total_spending: f64,
}

/// Tip: use [rust_query::Query::project_on] and [rust_query::Group::sum]
fn customer_spending(client: &Client) -> Vec<CustomerSpending> {
    todo!()
}
