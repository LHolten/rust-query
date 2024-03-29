use rust_query::{value::Db, Builder, HasId, Table};

pub struct InvoiceLine;

pub struct InvoiceLineDummy<'a> {
    pub invoice: Db<'a, Invoice>,
    pub track: Db<'a, Track>,
    pub unit_price: Db<'a, i64>,
    pub quantity: Db<'a, i64>,
}

impl Table for InvoiceLine {
    const NAME: &'static str = "InvoiceLine";

    type Dummy<'names> = InvoiceLineDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        InvoiceLineDummy {
            invoice: f.col("InvoiceId"),
            track: f.col("TrackId"),
            unit_price: f.col("UnitPrice"),
            quantity: f.col("Quantity"),
        }
    }
}

impl HasId for InvoiceLine {
    const ID: &'static str = "InvoiceLineId";
}

pub struct Track;

pub struct TrackDummy<'a> {
    pub name: Db<'a, String>,
    pub album: Db<'a, Album>,
    pub media_type: Db<'a, MediaType>,
    pub genre: Db<'a, Genre>,
    pub composer: Db<'a, String>,
    pub milliseconds: Db<'a, i64>,
    pub bytes: Db<'a, i64>,
    pub unit_price: Db<'a, i64>,
}

impl Table for Track {
    const NAME: &'static str = "Track";

    type Dummy<'names> = TrackDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        TrackDummy {
            name: f.col("Name"),
            album: f.col("AlbumId"),
            media_type: f.col("MediaTypeId"),
            genre: f.col("GenreId"),
            composer: f.col("Composer"),
            milliseconds: f.col("Milliseconds"),
            bytes: f.col("Bytes"),
            unit_price: f.col("UnitPrice"),
        }
    }
}

impl HasId for Track {
    const ID: &'static str = "TrackId";
}

pub struct Album;

pub struct AlbumDummy<'a> {
    pub title: Db<'a, String>,
    pub artist: Db<'a, Artist>,
}

impl Table for Album {
    const NAME: &'static str = "Album";

    type Dummy<'names> = AlbumDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        AlbumDummy {
            title: f.col("Title"),
            artist: f.col("ArtistId"),
        }
    }
}

impl HasId for Album {
    const ID: &'static str = "AlbumId";
}

pub struct Artist;

pub struct ArtistDummy<'a> {
    pub name: Db<'a, String>,
}

impl Table for Artist {
    const NAME: &'static str = "Artist";

    type Dummy<'names> = ArtistDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        ArtistDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Artist {
    const ID: &'static str = "ArtistId";
}

pub struct Playlist;

pub struct PlaylistDummy<'t> {
    pub name: Db<'t, String>,
}

impl Table for Playlist {
    const NAME: &'static str = "Playlist";

    type Dummy<'names> = PlaylistDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        PlaylistDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Playlist {
    const ID: &'static str = "PlaylistId";
}

pub struct PlaylistTrack;

pub struct PlaylistTrackDummy<'t> {
    pub playlist: Db<'t, Playlist>,
    pub track: Db<'t, Track>,
}

impl Table for PlaylistTrack {
    const NAME: &'static str = "PlaylistTrack";

    type Dummy<'names> = PlaylistTrackDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        PlaylistTrackDummy {
            playlist: f.col("PlaylistId"),
            track: f.col("TrackId"),
        }
    }
}

pub struct Customer;
pub struct CustomerDummy<'t> {
    pub first_name: Db<'t, String>,
    pub last_name: Db<'t, String>,
    pub company: Db<'t, String>,
    pub address: Db<'t, String>,
    pub city: Db<'t, String>,
    pub state: Db<'t, String>,
    pub country: Db<'t, String>,
    pub postal_code: Db<'t, String>,
    pub phone: Db<'t, String>,
    pub fax: Db<'t, String>,
    pub email: Db<'t, String>,
    pub support_rep: Db<'t, Employee>,
}

impl Table for Customer {
    const NAME: &'static str = "Customer";

    type Dummy<'names> = CustomerDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        CustomerDummy {
            first_name: f.col("FirstName"),
            last_name: f.col("LastName"),
            company: f.col("Company"),
            address: f.col("Address"),
            city: f.col("City"),
            state: f.col("State"),
            country: f.col("Country"),
            postal_code: f.col("PostalCode"),
            phone: f.col("Phone"),
            fax: f.col("Fax"),
            email: f.col("Email"),
            support_rep: f.col("SupportRepId"),
        }
    }
}

impl HasId for Customer {
    const ID: &'static str = "CustomerId";
}

pub struct Employee;
pub struct EmployeeDummy<'t> {
    pub last_name: Db<'t, String>,
    pub first_name: Db<'t, String>,
    pub title: Db<'t, String>,
    pub reports_to: Db<'t, Employee>,
    pub birth_date: Db<'t, String>,
    pub hire_date: Db<'t, String>,
    pub address: Db<'t, String>,
    pub city: Db<'t, String>,
    pub state: Db<'t, String>,
    pub country: Db<'t, String>,
    pub postal_code: Db<'t, String>,
    pub phone: Db<'t, String>,
    pub fax: Db<'t, String>,
    pub email: Db<'t, String>,
}

impl Table for Employee {
    const NAME: &'static str = "Employee";

    type Dummy<'names> = EmployeeDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        EmployeeDummy {
            last_name: f.col("LastName"),
            first_name: f.col("FirstName"),
            title: f.col("Title"),
            reports_to: f.col("ReportsTo"),
            birth_date: f.col("BirthDate"),
            hire_date: f.col("HireDate"),
            address: f.col("Address"),
            city: f.col("City"),
            state: f.col("State"),
            country: f.col("Country"),
            postal_code: f.col("PostalCode"),
            phone: f.col("Phone"),
            fax: f.col("Fax"),
            email: f.col("Email"),
        }
    }
}

impl HasId for Employee {
    const ID: &'static str = "EmployeeId";
}

pub struct Genre;
pub struct GenreDummy<'t> {
    pub name: Db<'t, String>,
}

impl Table for Genre {
    const NAME: &'static str = "Genre";

    type Dummy<'names> = GenreDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        GenreDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Genre {
    const ID: &'static str = "GenreId";
}

pub struct Invoice;
pub struct InvoiceDummy<'t> {
    pub customer: Db<'t, Customer>,
    pub invoice_date: Db<'t, String>,
    pub billing_address: Db<'t, String>,
    pub billing_city: Db<'t, String>,
    pub billing_state: Db<'t, String>,
    pub billing_country: Db<'t, String>,
    pub billing_postal_code: Db<'t, String>,
    pub total: Db<'t, i64>,
}

impl Table for Invoice {
    const NAME: &'static str = "Invoice";

    type Dummy<'names> = InvoiceDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        InvoiceDummy {
            customer: f.col("CustomerId"),
            invoice_date: f.col("InvoiceDate"),
            billing_address: f.col("BillingAddress"),
            billing_city: f.col("BillingCity"),
            billing_state: f.col("BillingState"),
            billing_country: f.col("BillingCountry"),
            billing_postal_code: f.col("BillingPostalCode"),
            total: f.col("Total"),
        }
    }
}

impl HasId for Invoice {
    const ID: &'static str = "InvoiceId";
}

pub struct MediaType;
pub struct MediaTypeDummy<'t> {
    pub name: Db<'t, String>,
}

impl Table for MediaType {
    const NAME: &'static str = "MediaType";

    type Dummy<'names> = MediaTypeDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        MediaTypeDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for MediaType {
    const ID: &'static str = "MediaTypeId";
}
