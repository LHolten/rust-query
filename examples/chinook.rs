#![allow(dead_code)]
use rust_orm::{new_query, value::Db, Builder, Table};

fn main() {
    avg_album_track_count_for_artist();
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

        e.all_rows(q)
            .map(|row| Invoice {
                track: row.get(ivl.track.name),
                artist: row.get(ivl.track.album.artist.name),
                ivl_id: row.get(ivl.id()),
            })
            .collect()
    })
}

// -- 15. Provide a query that shows the total number of tracks in each playlist. The Playlist name should be include on the resultant table.
// select *, count(trackid) as '# of tracks'
// from playlisttrack, playlist
// on playlisttrack.playlistid = playlist.playlistid
// group by playlist.playlistid
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

        e.all_rows2(q)
            .map(|row| PlaylistTrackCount {
                playlist: row.get(pl.name),
                track_count: row.get(count),
            })
            .collect()
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
        e.all_rows2(q)
            .map(|row| (row.get(artist.name), row.get(avg_album_track_count)))
            .collect()
    })
}

struct InvoiceLine;

struct InvoiceLineDummy<'a> {
    invoice_id: Db<'a, i64>,
    track: Db<'a, Track>,
    unit_price: Db<'a, i64>,
    quantity: Db<'a, i64>,
}

impl Table for InvoiceLine {
    const NAME: &'static str = "InvoiceLine";
    const ID: &'static str = "InvoiceLineId";

    type Dummy<'names> = InvoiceLineDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        InvoiceLineDummy {
            invoice_id: f.iden("InvoiceId"),
            track: f.iden("TrackId"),
            unit_price: f.iden("UnitPrice"),
            quantity: f.iden("Quantity"),
        }
    }
}

struct Track;

struct TrackDummy<'a> {
    name: Db<'a, String>,
    album: Db<'a, Album>,
    media_type_id: Db<'a, i64>,
    genre_id: Db<'a, String>,
    composer: Db<'a, String>,
    milliseconds: Db<'a, i64>,
    bytes: Db<'a, i64>,
    unit_price: Db<'a, i64>,
}

impl Table for Track {
    const NAME: &'static str = "Track";
    const ID: &'static str = "TrackId";

    type Dummy<'names> = TrackDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        TrackDummy {
            name: f.iden("Name"),
            album: f.iden("AlbumId"),
            media_type_id: f.iden("MediaTypeId"),
            genre_id: f.iden("GenreId"),
            composer: f.iden("Composer"),
            milliseconds: f.iden("Milliseconds"),
            bytes: f.iden("Bytes"),
            unit_price: f.iden("UnitPrice"),
        }
    }
}

struct Album;

struct AlbumDummy<'a> {
    title: Db<'a, String>,
    artist: Db<'a, Artist>,
}

impl Table for Album {
    const NAME: &'static str = "Album";
    const ID: &'static str = "AlbumId";

    type Dummy<'names> = AlbumDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        AlbumDummy {
            title: f.iden("Title"),
            artist: f.iden("ArtistId"),
        }
    }
}

struct Artist;

struct ArtistDummy<'a> {
    name: Db<'a, String>,
}

impl Table for Artist {
    const NAME: &'static str = "Artist";
    const ID: &'static str = "ArtistId";

    type Dummy<'names> = ArtistDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        ArtistDummy {
            name: f.iden("Name"),
        }
    }
}

struct Playlist;

struct PlaylistDummy<'t> {
    name: Db<'t, String>,
}

impl Table for Playlist {
    const NAME: &'static str = "Playlist";
    const ID: &'static str = "PlaylistId";

    type Dummy<'names> = PlaylistDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        PlaylistDummy {
            name: f.iden("Name"),
        }
    }
}

struct PlaylistTrack;

struct PlaylistTrackDummy<'t> {
    playlist: Db<'t, Playlist>,
    track: Db<'t, Track>,
}

impl Table for PlaylistTrack {
    const NAME: &'static str = "PlaylistTrack";
    const ID: &'static str = ""; //TODO: figure out how to fix this

    type Dummy<'names> = PlaylistTrackDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        PlaylistTrackDummy {
            playlist: f.iden("PlaylistId"),
            track: f.iden("TrackId"),
        }
    }
}
