#![allow(dead_code)]

mod tables {
    include!(concat!(env!("OUT_DIR"), "/tables.rs"));
}

use rust_query::client::Client;
use tables::{Employee, InvoiceLine, PlaylistTrack, Track};

fn main() {
    let client = Client::open_in_memory();
    client.execute_batch(include_str!("../Chinook_Sqlite.sql"));
    client.execute_batch(include_str!("../migrate.sql"));

    // let res = invoice_info(&client);
    // let res = playlist_track_count(&client);
    let res = avg_album_track_count_for_artist(&client);
    // let res = count_reporting(&client);
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
            track: row.get(q.select(ivl.track.name)),
            artist: row.get(q.select(ivl.track.album.artist.name)),
            ivl_id: row.get(q.select(ivl.id())),
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
        let plt = q.flat_table(PlaylistTrack);
        let pl = q.project_on(&plt.playlist);
        pl.into_vec(u32::MAX, |row| PlaylistTrackCount {
            playlist: row.get(pl.select().name),
            track_count: row.get(pl.count_distinct(&plt.track)),
        })
    })
}

fn avg_album_track_count_for_artist(client: &Client) -> Vec<(String, Option<i64>)> {
    client.new_query(|q| {
        let (album, track_count) = q.query(|q| {
            let track = q.table(Track);
            let album = q.project_on(&track.album);
            (album.select(), album.count_distinct(&track))
        });
        let artist = q.project_on(&album.artist);
        artist.into_vec(u32::MAX, |row| {
            (
                row.get(artist.select().name),
                row.get(artist.avg(track_count)),
            )
        })
    })
}

fn count_reporting(client: &Client) -> Vec<(String, i64)> {
    client.new_query(|q| {
        let reporter = q.table(Employee);
        // only count employees that report to someone
        let receiver = q.unwrap(reporter.reports_to);
        let receiver = q.project_on(&receiver);
        receiver.into_vec(u32::MAX, |row| {
            (
                row.get(receiver.select().last_name),
                row.get(receiver.count_distinct(&reporter)),
            )
        })
    })
}
