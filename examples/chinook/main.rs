#![allow(dead_code)]
mod tables;

use rust_query::new_query;
use tables::{Employee, InvoiceLine, PlaylistTrack, Track};

fn main() {
    // let res = invoice_info();
    // let res = playlist_track_count();
    let res = avg_album_track_count_for_artist();
    // let res = count_reporting();
    println!("{res:#?}")
}

// -- 13. Provide a query that includes the purchased track name AND artist name with each invoice line item.
// select i.*, t.name as 'track', ar.name as 'artist'
// from invoiceline as i
// 	join track as t on i.trackid = t.trackid
// 	join album as al on al.albumid = t.albumid
// 	join artist as ar on ar.artistid = al.artistid
#[derive(Debug)]
struct InvoiceInfo {
    track: String,
    artist: String,
    ivl_id: i64,
}

fn invoice_info() -> Vec<InvoiceInfo> {
    new_query(|q| {
        let ivl = q.table(InvoiceLine);

        q.into_vec(|row| InvoiceInfo {
            track: row.get(q.select(ivl.track.name)),
            artist: row.get(q.select(ivl.track.album.artist.name)),
            ivl_id: row.get(q.select(ivl.id())),
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

fn playlist_track_count() -> Vec<PlaylistTrackCount> {
    new_query(|q| {
        let plt = q.flat_table(PlaylistTrack);
        let pl = q.project_on(&plt.playlist);
        pl.into_vec(|row| PlaylistTrackCount {
            playlist: row.get(pl.select().name),
            track_count: row.get(pl.count_distinct(&plt.track)),
        })
    })
}

fn avg_album_track_count_for_artist() -> Vec<(String, Option<i64>)> {
    new_query(|q| {
        let (album, track_count) = q.query(|q| {
            let track = q.table(Track);
            let album = q.project_on(&track.album);
            (album.select(), album.count_distinct(&track))
        });
        let artist = q.project_on(&album.artist);
        artist.into_vec(|row| {
            (
                row.get(artist.select().name),
                row.get(artist.avg(track_count)),
            )
        })
    })
}

fn count_reporting() -> Vec<(String, i64)> {
    new_query(|q| {
        let reporter = q.table(Employee);
        // only count employees that report to someone
        let receiver = q.unwrap(&reporter.reports_to);
        let receiver = q.project_on(&receiver);
        receiver.into_vec(|row| {
            (
                row.get(receiver.select().last_name),
                row.get(receiver.count_distinct(&reporter)),
            )
        })
    })
}
