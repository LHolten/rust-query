#![allow(dead_code)]
mod tables;

use rust_query::new_query;
use tables::{Employee, InvoiceLine, PlaylistTrack, Track};

fn main() {
    let res = avg_album_track_count_for_artist();
    println!("{res:#?}")
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
    new_query(|e, q| {
        let ivl = q.table(InvoiceLine);
        q.all(&ivl);
        let ivl = q.any(&ivl);

        e.into_vec(q, |row| InvoiceInfo {
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
    new_query(|e, q| {
        let plt = q.flat_table(PlaylistTrack);
        q.all(&plt.playlist);
        let count = q.count_distinct(&plt.track);

        e.into_vec(q, |row| PlaylistTrackCount {
            playlist: row.get(q.any(&plt.playlist).name),
            track_count: row.get(count),
        })
    })
}

fn avg_album_track_count_for_artist() -> Vec<(String, i64)> {
    new_query(|e, q| {
        let (album, track_count) = q.query(|q| {
            let track = q.table(Track);
            q.all(&track.album);
            (q.any(&track.album), q.count_distinct(&track))
        });
        q.all(&album.artist);
        e.into_vec(q, |row| {
            (
                row.get(q.any(&album.artist).name),
                row.get(q.avg(track_count)),
            )
        })
    })
}

fn count_reporting() -> Vec<(String, i64)> {
    new_query(|e, q| {
        let reporter = q.table(Employee);
        q.all(&reporter.reports_to);
        e.into_vec(q, |row| {
            (
                row.get(q.any(&reporter.reports_to).last_name),
                row.get(q.count_distinct(&reporter)),
            )
        })
    })
}
