INSERT INTO
    album (id, title, artist)
SELECT
    AlbumId, Title, ArtistId
FROM
    old.Album;

INSERT INTO
    artist (id, name)
SELECT
    ArtistId, Name
FROM
    old.Artist;

INSERT INTO
    customer (id, first_name, last_name, company, address, city, state, country, postal_code, phone, fax, email, support_rep)
SELECT
    CustomerId, FirstName, LastName, Company, Address, City, State, Country, PostalCode, Phone, Fax, Email, SupportRepId
FROM
    old.Customer;

INSERT INTO
    employee (id, first_name, last_name, title, reports_to, birth_date, hire_date, address, city, state, country, postal_code, phone, fax, email)
SELECT
    EmployeeId, LastName, FirstName, Title, ReportsTo, BirthDate, HireDate, Address, City, State, Country, PostalCode, Phone, Fax, Email
FROM
    old.Employee;

INSERT INTO
    genre (id, name)
SELECT
    GenreId, Name
FROM
    old.Genre;

INSERT INTO
    invoice (id, customer, invoice_date, billing_address, billing_city, billing_state, billing_country, billing_postal_code, total)
SELECT
    InvoiceId, CustomerId, InvoiceDate, BillingAddress, BillingCity, BillingState, BillingCountry, BillingPostalCode, Total
FROM
    old.Invoice;

INSERT INTO
    invoice_line (id, invoice, track, unit_price, quantity)
SELECT
    InvoiceLineId, InvoiceId, TrackId, UnitPrice, Quantity
FROM
    old.InvoiceLine;

INSERT INTO
    media_type (id, name)
SELECT
    MediaTypeId, Name
FROM
    old.MediaType;

INSERT INTO
    playlist (id, name)
SELECT
    PlaylistId, Name
FROM
    old.Playlist;

INSERT INTO
    playlist_track (id, playlist, track)
SELECT
    ROWID, PlaylistId, TrackId
FROM
    old.PlaylistTrack;

INSERT INTO
    track (id, name, album, media_type, genre, composer, milliseconds, bytes, unit_price)
SELECT
    TrackId, Name, AlbumId, MediaTypeId, GenreId, Composer, Milliseconds, Bytes, UnitPrice
FROM
    old.Track;
