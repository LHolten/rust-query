use rust_query::{value::Db, Builder, Table};

pub struct InvoiceLine;

pub struct InvoiceLineDummy<'a> {
    pub invoice: Db<'a, Invoice>,
    pub track: Db<'a, Track>,
    pub unit_price: Db<'a, i64>,
    pub quantity: Db<'a, i64>,
}

impl Table for InvoiceLine {
    const NAME: &'static str = "InvoiceLine";
    const ID: &'static str = "InvoiceLineId";

    type Dummy<'names> = InvoiceLineDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        InvoiceLineDummy {
            invoice: f.iden("InvoiceId"),
            track: f.iden("TrackId"),
            unit_price: f.iden("UnitPrice"),
            quantity: f.iden("Quantity"),
        }
    }
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
    const ID: &'static str = "TrackId";

    type Dummy<'names> = TrackDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        TrackDummy {
            name: f.iden("Name"),
            album: f.iden("AlbumId"),
            media_type: f.iden("MediaTypeId"),
            genre: f.iden("GenreId"),
            composer: f.iden("Composer"),
            milliseconds: f.iden("Milliseconds"),
            bytes: f.iden("Bytes"),
            unit_price: f.iden("UnitPrice"),
        }
    }
}

pub struct Album;

pub struct AlbumDummy<'a> {
    pub title: Db<'a, String>,
    pub artist: Db<'a, Artist>,
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

pub struct Artist;

pub struct ArtistDummy<'a> {
    pub name: Db<'a, String>,
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

pub struct Playlist;

pub struct PlaylistDummy<'t> {
    pub name: Db<'t, String>,
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

pub struct PlaylistTrack;

pub struct PlaylistTrackDummy<'t> {
    pub playlist: Db<'t, Playlist>,
    pub track: Db<'t, Track>,
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

pub struct Customer;
pub struct CustomerDummy<'t> {
    first_name: Db<'t, String>,
    last_name: Db<'t, String>,
    company: Db<'t, String>,
    address: Db<'t, String>,
    city: Db<'t, String>,
    state: Db<'t, String>,
    country: Db<'t, String>,
    postal_code: Db<'t, String>,
    phone: Db<'t, String>,
    fax: Db<'t, String>,
    email: Db<'t, String>,
    support_rep: Db<'t, Employee>,
}

impl Table for Customer {
    const NAME: &'static str = "Customer";
    const ID: &'static str = "CustomerId";

    type Dummy<'names> = CustomerDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        CustomerDummy {
            first_name: f.iden("FirstName"),
            last_name: f.iden("LastName"),
            company: f.iden("Company"),
            address: f.iden("Address"),
            city: f.iden("City"),
            state: f.iden("State"),
            country: f.iden("Country"),
            postal_code: f.iden("PostalCode"),
            phone: f.iden("Phone"),
            fax: f.iden("Fax"),
            email: f.iden("Email"),
            support_rep: f.iden("SupportRepId"),
        }
    }
}

pub struct Employee;
pub struct EmployeeDummy<'t> {
    last_name: Db<'t, String>,
    first_name: Db<'t, String>,
    title: Db<'t, String>,
    reports_to: Db<'t, Employee>,
    birth_date: Db<'t, String>,
    hire_date: Db<'t, String>,
    address: Db<'t, String>,
    city: Db<'t, String>,
    state: Db<'t, String>,
    country: Db<'t, String>,
    postal_code: Db<'t, String>,
    phone: Db<'t, String>,
    fax: Db<'t, String>,
    email: Db<'t, String>,
}

impl Table for Employee {
    const NAME: &'static str = "Employee";
    const ID: &'static str = "EmployeeId";

    type Dummy<'names> = EmployeeDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        EmployeeDummy {
            last_name: f.iden("LastName"),
            first_name: f.iden("FirstName"),
            title: f.iden("Title"),
            reports_to: f.iden("ReportsTo"),
            birth_date: f.iden("BirthDate"),
            hire_date: f.iden("HireDate"),
            address: f.iden("Address"),
            city: f.iden("City"),
            state: f.iden("State"),
            country: f.iden("Country"),
            postal_code: f.iden("PostalCode"),
            phone: f.iden("Phone"),
            fax: f.iden("Fax"),
            email: f.iden("Email"),
        }
    }
}

pub struct Genre;
pub struct GenreDummy<'t> {
    name: Db<'t, String>,
}

impl Table for Genre {
    const NAME: &'static str = "Genre";
    const ID: &'static str = "GenreId";

    type Dummy<'names> = GenreDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        GenreDummy {
            name: f.iden("Name"),
        }
    }
}

pub struct Invoice;
pub struct InvoiceDummy<'t> {
    customer: Db<'t, Customer>,
    invoice_date: Db<'t, String>,
    billing_address: Db<'t, String>,
    billing_city: Db<'t, String>,
    billing_state: Db<'t, String>,
    billing_country: Db<'t, String>,
    billing_postal_code: Db<'t, String>,
    total: Db<'t, i64>,
}

impl Table for Invoice {
    const NAME: &'static str = "Invoice";
    const ID: &'static str = "InvoiceId";

    type Dummy<'names> = InvoiceDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        InvoiceDummy {
            customer: f.iden("CustomerId"),
            invoice_date: f.iden("InvoiceDate"),
            billing_address: f.iden("BillingAddress"),
            billing_city: f.iden("BillingCity"),
            billing_state: f.iden("BillingState"),
            billing_country: f.iden("BillingCountry"),
            billing_postal_code: f.iden("BillingPostalCode"),
            total: f.iden("Total"),
        }
    }
}

pub struct MediaType;
pub struct MediaTypeDummy<'t> {
    name: Db<'t, String>,
}

impl Table for MediaType {
    const NAME: &'static str = "MediaType";
    const ID: &'static str = "MediaTypeId";

    type Dummy<'names> = MediaTypeDummy<'names>;

    fn build(f: Builder<'_>) -> Self::Dummy<'_> {
        MediaTypeDummy {
            name: f.iden("Name"),
        }
    }
}
