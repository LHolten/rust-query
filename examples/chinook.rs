use rust_orm::{
    new_query,
    value::{MyFk, MyIden, Value},
    Table,
};

fn main() {
    todo!()
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
    invoice: i64,
}

fn invoice_info() -> Vec<Invoice> {
    new_query(|e, mut q| {
        let i = q.table(InvoiceLine);
        let invoice = q.all(i.invoice_line_id);
        let track = q.all(i.track.name);
        let artist = q.all(i.track.album.artist.name);

        e.all_rows(q)
            .into_iter()
            .map(|row| Invoice {
                track: row.get_string(track),
                artist: row.get_string(artist),
                invoice: row.get_i64(invoice),
            })
            .collect()
    })
}

struct InvoiceLine;

struct InvoiceLineDummy<'a> {
    invoice_line_id: MyIden<'a>,
    invoice_id: MyIden<'a>,
    track: MyFk<'a, Track>,
    unit_price: MyIden<'a>,
    quantity: MyIden<'a>,
}

impl Table for InvoiceLine {
    const NAME: &'static str = "InvoiceLine";

    type Dummy<'names> = InvoiceLineDummy<'names>;

    fn build<'a, F>(mut f: F) -> Self::Dummy<'a>
    where
        F: FnMut(&'static str) -> MyIden<'a>,
    {
        InvoiceLineDummy {
            invoice_line_id: f("InvoiceLineId"),
            invoice_id: f("InvoiceId"),
            track: f("TrackId").fk(),
            unit_price: f("UnitPrice"),
            quantity: f("Quantity"),
        }
    }
}

struct Track;

struct TrackDummy<'a> {
    track_id: MyIden<'a>,
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

    type Dummy<'names> = TrackDummy<'names>;

    fn build<'a, F>(mut f: F) -> Self::Dummy<'a>
    where
        F: FnMut(&'static str) -> MyIden<'a>,
    {
        TrackDummy {
            track_id: f("TrackId"),
            name: f("Name"),
            album: f("AlbumId").fk(),
            media_type_id: f("MediaTypeId"),
            genre_id: f("GenreId"),
            composer: f("Composer"),
            milliseconds: f("Milliseconds"),
            bytes: f("Bytes"),
            unit_price: f("UnitPrice"),
        }
    }
}

struct Album;

struct AlbumDummy<'a> {
    album_id: MyIden<'a>,
    title: MyIden<'a>,
    artist: MyFk<'a, Artist>,
}

impl Table for Album {
    const NAME: &'static str = "Album";

    type Dummy<'names> = AlbumDummy<'names>;

    fn build<'a, F>(mut f: F) -> Self::Dummy<'a>
    where
        F: FnMut(&'static str) -> MyIden<'a>,
    {
        AlbumDummy {
            album_id: f("AlbumId"),
            title: f("Title"),
            artist: f("ArtistId").fk(),
        }
    }
}

struct Artist;

struct ArtistDummy<'a> {
    artist_id: MyIden<'a>,
    name: MyIden<'a>,
}

impl Table for Artist {
    const NAME: &'static str = "Artist";

    type Dummy<'names> = ArtistDummy<'names>;

    fn build<'a, F>(mut f: F) -> Self::Dummy<'a>
    where
        F: FnMut(&'static str) -> MyIden<'a>,
    {
        ArtistDummy {
            artist_id: f("ArtistId"),
            name: f("Name"),
        }
    }
}
