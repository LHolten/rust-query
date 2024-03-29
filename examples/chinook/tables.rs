use rust_query::{value::Db, Builder, HasId, Table};

pub struct InvoiceLine;

pub struct InvoiceLineDummy {
    pub invoice: Db<Invoice>,
    pub track: Db<Track>,
    pub unit_price: Db<i64>,
    pub quantity: Db<i64>,
}

impl Table for InvoiceLine {
    const NAME: &'static str = "InvoiceLine";

    type Dummy = InvoiceLineDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
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

pub struct TrackDummy {
    pub name: Db<String>,
    pub album: Db<Album>,
    pub media_type: Db<MediaType>,
    pub genre: Db<Genre>,
    pub composer: Db<String>,
    pub milliseconds: Db<i64>,
    pub bytes: Db<i64>,
    pub unit_price: Db<i64>,
}

impl Table for Track {
    const NAME: &'static str = "Track";

    type Dummy = TrackDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
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

pub struct AlbumDummy {
    pub title: Db<String>,
    pub artist: Db<Artist>,
}

impl Table for Album {
    const NAME: &'static str = "Album";

    type Dummy = AlbumDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
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

pub struct ArtistDummy {
    pub name: Db<String>,
}

impl Table for Artist {
    const NAME: &'static str = "Artist";

    type Dummy = ArtistDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
        ArtistDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Artist {
    const ID: &'static str = "ArtistId";
}

pub struct Playlist;

pub struct PlaylistDummy {
    pub name: Db<String>,
}

impl Table for Playlist {
    const NAME: &'static str = "Playlist";

    type Dummy = PlaylistDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
        PlaylistDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Playlist {
    const ID: &'static str = "PlaylistId";
}

pub struct PlaylistTrack;

pub struct PlaylistTrackDummy {
    pub playlist: Db<Playlist>,
    pub track: Db<Track>,
}

impl Table for PlaylistTrack {
    const NAME: &'static str = "PlaylistTrack";

    type Dummy = PlaylistTrackDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
        PlaylistTrackDummy {
            playlist: f.col("PlaylistId"),
            track: f.col("TrackId"),
        }
    }
}

pub struct Customer;
pub struct CustomerDummy {
    pub first_name: Db<String>,
    pub last_name: Db<String>,
    pub company: Db<String>,
    pub address: Db<String>,
    pub city: Db<String>,
    pub state: Db<String>,
    pub country: Db<String>,
    pub postal_code: Db<String>,
    pub phone: Db<String>,
    pub fax: Db<String>,
    pub email: Db<String>,
    pub support_rep: Db<Employee>,
}

impl Table for Customer {
    const NAME: &'static str = "Customer";

    type Dummy = CustomerDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
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
pub struct EmployeeDummy {
    pub last_name: Db<String>,
    pub first_name: Db<String>,
    pub title: Db<String>,
    pub reports_to: Db<Employee>,
    pub birth_date: Db<String>,
    pub hire_date: Db<String>,
    pub address: Db<String>,
    pub city: Db<String>,
    pub state: Db<String>,
    pub country: Db<String>,
    pub postal_code: Db<String>,
    pub phone: Db<String>,
    pub fax: Db<String>,
    pub email: Db<String>,
}

impl Table for Employee {
    const NAME: &'static str = "Employee";

    type Dummy = EmployeeDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
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
pub struct GenreDummy {
    pub name: Db<String>,
}

impl Table for Genre {
    const NAME: &'static str = "Genre";

    type Dummy = GenreDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
        GenreDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for Genre {
    const ID: &'static str = "GenreId";
}

pub struct Invoice;
pub struct InvoiceDummy {
    pub customer: Db<Customer>,
    pub invoice_date: Db<String>,
    pub billing_address: Db<String>,
    pub billing_city: Db<String>,
    pub billing_state: Db<String>,
    pub billing_country: Db<String>,
    pub billing_postal_code: Db<String>,
    pub total: Db<i64>,
}

impl Table for Invoice {
    const NAME: &'static str = "Invoice";

    type Dummy = InvoiceDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
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
pub struct MediaTypeDummy {
    pub name: Db<String>,
}

impl Table for MediaType {
    const NAME: &'static str = "MediaType";

    type Dummy = MediaTypeDummy;

    fn build(f: Builder<'_>) -> Self::Dummy {
        MediaTypeDummy {
            name: f.col("Name"),
        }
    }
}

impl HasId for MediaType {
    const ID: &'static str = "MediaTypeId";
}
