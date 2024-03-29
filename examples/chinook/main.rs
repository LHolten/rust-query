#![allow(dead_code)]
mod tables;

use std::marker::PhantomData;

use rust_query::{new_query, value::Value, Group, Query};
use tables::{Employee, InvoiceLine, PlaylistTrack, Track};

fn main() {
    // let res = invoice_info();
    // let res = playlist_track_count();
    // let res = avg_album_track_count_for_artist();
    // let res = count_reporting();
    // println!("{res:#?}")
}

// -- 13. Provide a query that includes the purchased track name AND artist name with each invoice line item.
// select i.*, t.name as 'track', ar.name as 'artist'
// from invoiceline as i
// 	join track as t on i.trackid = t.trackid
// 	join album as al on al.albumid = t.albumid
// 	join artist as ar on ar.artistid = al.artistid
struct InvoiceInfo {
    track: String,
    artist: String,
    ivl_id: i64,
}

fn invoice_info() -> Vec<InvoiceInfo> {
    new_query(|e, q, g| {
        let ivl = q.table(InvoiceLine);

        e.into_vec(q, |row| InvoiceInfo {
            track: row.get(&ivl.track.name),
            artist: row.get(&ivl.track.album.artist.name),
            ivl_id: row.get(ivl.id()),
        })
    })
}

// -- 15. Provide a query that shows the total number of tracks in each playlist. The Playlist name should be include on the resultant table.
// select *, count(trackid) as '# of tracks'
// from playlisttrack, playlist
// on playlisttrack.playlistid = playlist.playlistid
// group by playlist.playlistid
#[derive(Debug)]
struct PlaylistTrackCount {
    playlist: String,
    track_count: i64,
}

// fn playlist_track_count() -> Vec<PlaylistTrackCount> {
//     new_query(|e, q| {
//         let plt = q.flat_table(PlaylistTrack);
//         let pl = q.group_old(&plt.playlist);

//         e.into_vec(q, |row| PlaylistTrackCount {
//             playlist: row.get(&pl.name),
//             track_count: row.get(q.count_distinct(&plt.track)),
//         })
//     })
// }

// fn avg_album_track_count_for_artist() -> Vec<(String, i64)> {
//     new_query(|e, q| {
//         let (album, track_count) = q.query(|q| {
//             let track = q.table(Track);
//             let album = q.group_old(&track.album);
//             (album, q.count_distinct(track))
//         });
//         let artist = q.group_old(&album.artist);
//         e.into_vec(q, |row| {
//             (row.get(&artist.name), row.get(q.avg(track_count)))
//         })
//     })
// }

// fn count_reporting() -> Vec<(String, i64)> {
//     new_query(|e, q| {
//         let reporter = q.table(Employee);
//         let receiver = q.group_old(&reporter.reports_to);
//         e.into_vec(q, |row| {
//             (
//                 row.get(&receiver.last_name),
//                 row.get(q.count_distinct(reporter)),
//             )
//         })
//     })
// }

fn playlist_track_count() -> Vec<PlaylistTrackCount> {
    new_query(|e, q, g| {
        let plt = q.flat_table(PlaylistTrack);
        let pl = g.group(q, &plt.playlist);

        e.into_vec(q, |row| PlaylistTrackCount {
            playlist: row.get(&pl.name),
            track_count: row.get(pl.count_distinct(&plt.track)),
        })
    })
}

fn avg_album_track_count_for_artist() -> Vec<(String, i64)> {
    new_query(|e, q, g| {
        let (album, track_count) = q.query(|q, g| {
            let track = q.table(Track);
            let album = g.group(q, &track.album);
            (**album, album.count_distinct(track))
        });
        let artist = g.group(q, &album.artist);
        e.into_vec(q, |row| {
            (row.get(&artist.name), row.get(artist.avg(track_count)))
        })
    })
}

fn count_reporting() -> Vec<(String, i64)> {
    new_query(|e, q, g| {
        let reporter = q.table(Employee);
        let receiver = g.group(q, &reporter.reports_to);
        e.into_vec(q, |row| {
            (
                row.get(&receiver.last_name),
                row.get(receiver.count_distinct(reporter)),
            )
        })
    })
}
