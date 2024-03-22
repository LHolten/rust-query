#![allow(dead_code)]
use rust_orm::{
    new_query,
    value::{MyFk, MyIden},
    Builder, Table,
};

fn main() {
    invoice_info();
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
        let ivl_id = q.all(ivl.id());
        let track = q.all(ivl.track.name);
        let artist = q.all(ivl.track.album.artist.name);

        e.all_rows(q)
            .map(|row| Invoice {
                track: row.get_string(track),
                artist: row.get_string(artist),
                ivl_id: row.get_i64(ivl_id),
            })
            .collect()
    })
}

struct InvoiceLine;

struct InvoiceLineDummy<'a> {
    invoice_id: MyIden<'a>,
    track: MyFk<'a, Track>,
    unit_price: MyIden<'a>,
    quantity: MyIden<'a>,
}

impl Table for InvoiceLine {
    const NAME: &'static str = "InvoiceLine";
    const ID: &'static str = "InvoiceLineId";

    type Dummy<'names> = InvoiceLineDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        InvoiceLineDummy {
            invoice_id: f.iden("InvoiceId"),
            track: f.fk("TrackId"),
            unit_price: f.iden("UnitPrice"),
            quantity: f.iden("Quantity"),
        }
    }
}

struct Track;

struct TrackDummy<'a> {
    name: MyIden<'a>,
    album: MyFk<'a, Album>,
    media_type_id: MyIden<'a>,
    genre_id: MyIden<'a>,
    composer: MyIden<'a>,
    milliseconds: MyIden<'a>,
    bytes: MyIden<'a>,
    unit_price: MyIden<'a>,
}

impl Table for Track {
    const NAME: &'static str = "Track";
    const ID: &'static str = "TrackId";

    type Dummy<'names> = TrackDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        TrackDummy {
            name: f.iden("Name"),
            album: f.fk("AlbumId"),
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
    title: MyIden<'a>,
    artist: MyFk<'a, Artist>,
}

impl Table for Album {
    const NAME: &'static str = "Album";
    const ID: &'static str = "AlbumId";

    type Dummy<'names> = AlbumDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        AlbumDummy {
            title: f.iden("Title"),
            artist: f.fk("ArtistId"),
        }
    }
}

struct Artist;

struct ArtistDummy<'a> {
    name: MyIden<'a>,
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
