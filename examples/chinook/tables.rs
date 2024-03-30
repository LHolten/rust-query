use rust_query::{value::Db, Builder, HasId, Table};

pub struct Album;

pub struct AlbumDummy<'a, const NotNull: bool> {
    pub title: Db<'a, String, NotNull>,
    pub artist: Db<'a, Artist, NotNull>,
}

impl Table for Album {
    const NAME: &'static str = "Album";

    type Dummy<'names, const NotNull: bool> = AlbumDummy<'names, NotNull>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
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

    type Dummy<'names, const NotNull: bool> = ArtistDummy<'names>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
        ArtistDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Artist {
    const ID: &'static str = "ArtistId";
}

pub struct Customer;
pub struct CustomerDummy<'t, const NotNull: bool> {
    pub first_name: Db<'t, String, NotNull>,
    pub last_name: Db<'t, String, NotNull>,
    pub company: Db<'t, String>,
    pub address: Db<'t, String>,
    pub city: Db<'t, String>,
    pub state: Db<'t, String>,
    pub country: Db<'t, String>,
    pub postal_code: Db<'t, String>,
    pub phone: Db<'t, String>,
    pub fax: Db<'t, String>,
    pub email: Db<'t, String, NotNull>,
    pub support_rep: Db<'t, Employee>,
}

impl Table for Customer {
    const NAME: &'static str = "Customer";

    type Dummy<'names, const NotNull: bool> = CustomerDummy<'names, NotNull>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
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
pub struct EmployeeDummy<'t, const NotNull: bool> {
    pub last_name: Db<'t, String, NotNull>,
    pub first_name: Db<'t, String, NotNull>,
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

    type Dummy<'names, const NotNull: bool> = EmployeeDummy<'names, NotNull>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
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

    type Dummy<'names, const NotNull: bool> = GenreDummy<'names>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
        GenreDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Genre {
    const ID: &'static str = "GenreId";
}

pub struct Invoice;
pub struct InvoiceDummy<'t, const NotNull: bool> {
    pub customer: Db<'t, Customer, NotNull>,
    pub invoice_date: Db<'t, String, NotNull>,
    pub billing_address: Db<'t, String>,
    pub billing_city: Db<'t, String>,
    pub billing_state: Db<'t, String>,
    pub billing_country: Db<'t, String>,
    pub billing_postal_code: Db<'t, String>,
    pub total: Db<'t, i64, NotNull>,
}

impl Table for Invoice {
    const NAME: &'static str = "Invoice";

    type Dummy<'names, const NotNull: bool> = InvoiceDummy<'names, NotNull>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
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

pub struct InvoiceLine;

pub struct InvoiceLineDummy<'a, const NotNull: bool> {
    pub invoice: Db<'a, Invoice, NotNull>,
    pub track: Db<'a, Track, NotNull>,
    pub unit_price: Db<'a, i64, NotNull>,
    pub quantity: Db<'a, i64, NotNull>,
}

impl Table for InvoiceLine {
    const NAME: &'static str = "InvoiceLine";

    type Dummy<'names, const NotNull: bool> = InvoiceLineDummy<'names, NotNull>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
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

pub struct MediaType;
pub struct MediaTypeDummy<'t> {
    pub name: Db<'t, String>,
}

impl Table for MediaType {
    const NAME: &'static str = "MediaType";

    type Dummy<'names, const NotNull: bool> = MediaTypeDummy<'names>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
        MediaTypeDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for MediaType {
    const ID: &'static str = "MediaTypeId";
}

pub struct Playlist;

pub struct PlaylistDummy<'t> {
    pub name: Db<'t, String>,
}

impl Table for Playlist {
    const NAME: &'static str = "Playlist";

    type Dummy<'names, const NotNull: bool> = PlaylistDummy<'names>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
        PlaylistDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Playlist {
    const ID: &'static str = "PlaylistId";
}

pub struct PlaylistTrack;

pub struct PlaylistTrackDummy<'t, const NotNull: bool> {
    pub playlist: Db<'t, Playlist, NotNull>,
    pub track: Db<'t, Track, NotNull>,
}

impl Table for PlaylistTrack {
    const NAME: &'static str = "PlaylistTrack";

    type Dummy<'names, const NotNull: bool> = PlaylistTrackDummy<'names, NotNull>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
        PlaylistTrackDummy {
            playlist: f.col("PlaylistId"),
            track: f.col("TrackId"),
        }
    }
}

pub struct Track;

pub struct TrackDummy<'a, const NotNull: bool> {
    pub name: Db<'a, String, NotNull>,
    pub album: Db<'a, Album>,
    pub media_type: Db<'a, MediaType, NotNull>,
    pub genre: Db<'a, Genre>,
    pub composer: Db<'a, String>,
    pub milliseconds: Db<'a, i64, NotNull>,
    pub bytes: Db<'a, i64>,
    pub unit_price: Db<'a, i64, NotNull>,
}

impl Table for Track {
    const NAME: &'static str = "Track";

    type Dummy<'names, const NotNull: bool> = TrackDummy<'names, NotNull>;

    fn build<const NotNull: bool>(f: Builder<'_>) -> Self::Dummy<'_, NotNull> {
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
