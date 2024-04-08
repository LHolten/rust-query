#![allow(dead_code)]

mod tables {
    include!(concat!(env!("OUT_DIR"), "/tables.rs"));
}

use rust_query::{client::Client, value::Value};
use tables::{Album, Artist, Employee, InvoiceLine, Playlist, PlaylistTrack, Track};

use crate::tables::{Genre, GenreDummy};

fn main() {
    let client = Client::open_in_memory();
    client.execute_batch(include_str!("../Chinook_Sqlite.sql"));
    client.execute_batch(include_str!("../migrate.sql"));

    client.new_query(|q| {
        q.insert(GenreDummy {
            name: q.select("my cool genre"),
        })
    });

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
    ivl_id: i64,
}

fn invoice_info(client: &Client) -> Vec<InvoiceInfo> {
    client.new_query(|q| {
        let ivl = q.table(InvoiceLine);
        q.into_vec(u32::MAX, |row| InvoiceInfo {
            track: row.get(ivl.track.name),
            artist: row.get(ivl.track.album.artist.name),
            ivl_id: row.get(ivl.id()),
        })
    })
}

#[derive(Debug)]
struct PlaylistTrackCount {
    playlist: String,
    track_count: i64,
}

fn playlist_track_count(client: &Client) -> Vec<PlaylistTrackCount> {
    client.new_query(|q| {
        let pl = q.table(Playlist);
        let track_count = q.query(|q| {
            let plt = q.table(PlaylistTrack);
            q.filter_on(&plt.playlist, &pl);
            q.group().count_distinct(plt)
        });

        q.into_vec(u32::MAX, |row| PlaylistTrackCount {
            playlist: row.get(pl.name),
            track_count: row.get(track_count),
        })
    })
}

fn avg_album_track_count_for_artist(client: &Client) -> Vec<(String, Option<i64>)> {
    client.new_query(|q| {
        let artist = q.table(Artist);
        let avg_track_count = q.query(|q| {
            let album = q.table(Album);
            q.filter_on(&album.artist, &artist);

            let track_count = q.query(|q| {
                let track = q.table(Track);
                q.filter_on(&track.album, album);

                q.group().count_distinct(track)
            });
            q.group().avg(track_count)
        });
        q.into_vec(u32::MAX, |row| {
            (row.get(artist.name), row.get(avg_track_count))
        })
    })
}

fn count_reporting(client: &Client) -> Vec<(String, i64)> {
    client.new_query(|q| {
        let receiver = q.table(Employee);
        let report_count = q.query(|q| {
            let reporter = q.table(Employee);
            // only count employees that report to someone
            let reports_to = q.filter_some(reporter.reports_to);
            q.filter_on(reports_to, &receiver);
            q.group().count_distinct(reporter)
        });

        q.into_vec(u32::MAX, |row| {
            (row.get(receiver.last_name), row.get(report_count))
        })
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
