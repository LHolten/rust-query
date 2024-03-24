#![allow(dead_code)]
mod tables;

use rust_query::new_query;
use tables::{Employee, InvoiceLine, PlaylistTrack, Track};

fn main() {
    let res = count_reporting();
    println!("{res:#?}")
}

// -- 13. Provide a query that includes the purchased track name AND artist name with each invoice line item.
// select i.*, t.name as 'track', ar.name as 'artist'
// from invoiceline as i
// 	join track as t on i.trackid = t.trackid
// 	join album as al on al.albumid = t.albumid
// 	join artist as ar on ar.artistid = al.artistid
struct Invoice {
    track: String,
    artist: String,
    ivl_id: i64,
}

fn invoice_info() -> Vec<Invoice> {
    new_query(|e, mut q| {
        let ivl = q.table(InvoiceLine);
        let ivl = q.all(&ivl);

        e.into_vec(q, |row| Invoice {
            track: row.get(ivl.track.name),
            artist: row.get(ivl.track.album.artist.name),
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

fn playlist_track_count() -> Vec<PlaylistTrackCount> {
    new_query(|e, mut q| {
        let plt = q.table(PlaylistTrack);
        let pl = q.all(&plt.playlist);
        let mut q = q.into_groups();
        let count = q.count_distinct(&plt.track);

        e.into_vec(q, |row| PlaylistTrackCount {
            playlist: row.get(pl.name),
            track_count: row.get(count),
        })
    })
}

fn avg_album_track_count_for_artist() -> Vec<(String, i64)> {
    new_query(|e, mut q| {
        let (album, track_count) = q.query(|mut q| {
            let track = q.table(Track);
            let album = q.all(&track.album);
            let mut q = q.into_groups();
            let track_count = q.count_distinct(&track);
            (album, track_count)
        });
        let artist = q.all(&album.artist);
        let mut q = q.into_groups();
        let avg_album_track_count = q.avg(track_count);
        e.into_vec(q, |row| {
            (row.get(artist.name), row.get(avg_album_track_count))
        })
    })
}

fn count_reporting() -> Vec<(String, i64)> {
    new_query(|e, mut q| {
        let reporter = q.table(Employee);
        let reports_to = q.all(&reporter.reports_to);
        let mut q = q.into_groups();
        let report_count = q.count_distinct(&reporter);
        e.into_vec(q, |row| {
            (row.get(reports_to.last_name), row.get(report_count))
        })
    })
}
