-- altered from https://github.com/lerocha/chinook-database
PRAGMA foreign_keys = OFF;

CREATE TABLE [Album2] (
    [AlbumId] INTEGER NOT NULL,
    [Title] TEXT NOT NULL,
    [ArtistId] INTEGER NOT NULL,
    CONSTRAINT [PK_Album] PRIMARY KEY ([AlbumId]),
    FOREIGN KEY ([ArtistId]) REFERENCES [Artist] ([ArtistId]) ON DELETE NO ACTION ON UPDATE NO ACTION
) STRICT;

INSERT INTO
    Album2
SELECT
    *
FROM
    Album;

DROP TABLE Album;

ALTER TABLE
    Album2 RENAME TO Album;

CREATE TABLE [Artist2] (
    [ArtistId] INTEGER NOT NULL,
    [Name] TEXT NOT NULL,
    CONSTRAINT [PK_Artist] PRIMARY KEY ([ArtistId])
) STRICT;

INSERT INTO
    Artist2
SELECT
    *
FROM
    Artist;

DROP TABLE Artist;

ALTER TABLE
    Artist2 RENAME TO Artist;

CREATE TABLE [Customer2] (
    [CustomerId] INTEGER NOT NULL,
    [FirstName] TEXT NOT NULL,
    [LastName] TEXT NOT NULL,
    [Company] TEXT,
    [Address] TEXT NOT NULL,
    [City] TEXT NOT NULL,
    [State] TEXT,
    [Country] TEXT NOT NULL,
    [PostalCode] TEXT,
    [Phone] TEXT,
    [Fax] TEXT,
    [Email] TEXT NOT NULL,
    [SupportRepId] INTEGER NOT NULL,
    CONSTRAINT [PK_Customer] PRIMARY KEY ([CustomerId]),
    FOREIGN KEY ([SupportRepId]) REFERENCES [Employee] ([EmployeeId]) ON DELETE NO ACTION ON UPDATE NO ACTION
) STRICT;

INSERT INTO
    Customer2
SELECT
    *
FROM
    Customer;

DROP TABLE Customer;

ALTER TABLE
    Customer2 RENAME TO Customer;

CREATE TABLE [Employee2] (
    [EmployeeId] INTEGER NOT NULL,
    [LastName] TEXT NOT NULL,
    [FirstName] TEXT NOT NULL,
    [Title] TEXT,
    [ReportsTo] INTEGER,
    [BirthDate] TEXT,
    [HireDate] TEXT,
    [Address] TEXT,
    [City] TEXT,
    [State] TEXT,
    [Country] TEXT,
    [PostalCode] TEXT,
    [Phone] TEXT,
    [Fax] TEXT,
    [Email] TEXT NOT NULL,
    CONSTRAINT [PK_Employee] PRIMARY KEY ([EmployeeId]),
    FOREIGN KEY ([ReportsTo]) REFERENCES [Employee] ([EmployeeId]) ON DELETE NO ACTION ON UPDATE NO ACTION
) STRICT;

INSERT INTO
    Employee2
SELECT
    *
FROM
    Employee;

DROP TABLE Employee;

ALTER TABLE
    Employee2 RENAME TO Employee;

CREATE TABLE [Genre2] (
    [GenreId] INTEGER NOT NULL,
    [Name] TEXT NOT NULL,
    CONSTRAINT [PK_Genre] PRIMARY KEY ([GenreId])
) STRICT;

INSERT INTO
    Genre2
SELECT
    *
FROM
    Genre;

DROP TABLE Genre;

ALTER TABLE
    Genre2 RENAME TO Genre;

CREATE TABLE [Invoice2] (
    [InvoiceId] INTEGER NOT NULL,
    [CustomerId] INTEGER NOT NULL,
    [InvoiceDate] TEXT NOT NULL,
    [BillingAddress] TEXT,
    [BillingCity] TEXT,
    [BillingState] TEXT,
    [BillingCountry] TEXT,
    [BillingPostalCode] TEXT,
    [Total] REAL NOT NULL,
    CONSTRAINT [PK_Invoice] PRIMARY KEY ([InvoiceId]),
    FOREIGN KEY ([CustomerId]) REFERENCES [Customer] ([CustomerId]) ON DELETE NO ACTION ON UPDATE NO ACTION
) STRICT;

INSERT INTO
    Invoice2
SELECT
    *
FROM
    Invoice;

DROP TABLE Invoice;

ALTER TABLE
    Invoice2 RENAME TO Invoice;

CREATE TABLE [InvoiceLine2] (
    [InvoiceLineId] INTEGER NOT NULL,
    [InvoiceId] INTEGER NOT NULL,
    [TrackId] INTEGER NOT NULL,
    [UnitPrice] REAL NOT NULL,
    [Quantity] INTEGER NOT NULL,
    CONSTRAINT [PK_InvoiceLine] PRIMARY KEY ([InvoiceLineId]),
    FOREIGN KEY ([InvoiceId]) REFERENCES [Invoice] ([InvoiceId]) ON DELETE NO ACTION ON UPDATE NO ACTION,
    FOREIGN KEY ([TrackId]) REFERENCES [Track] ([TrackId]) ON DELETE NO ACTION ON UPDATE NO ACTION
) STRICT;

INSERT INTO
    InvoiceLine2
SELECT
    *
FROM
    InvoiceLine;

DROP TABLE InvoiceLine;

ALTER TABLE
    InvoiceLine2 RENAME TO InvoiceLine;

CREATE TABLE [MediaType2] (
    [MediaTypeId] INTEGER NOT NULL,
    [Name] TEXT NOT NULL,
    CONSTRAINT [PK_MediaType] PRIMARY KEY ([MediaTypeId])
) STRICT;

INSERT INTO
    MediaType2
SELECT
    *
FROM
    MediaType;

DROP TABLE MediaType;

ALTER TABLE
    MediaType2 RENAME TO MediaType;

CREATE TABLE [Playlist2] (
    [PlaylistId] INTEGER NOT NULL,
    [Name] TEXT NOT NULL,
    CONSTRAINT [PK_Playlist] PRIMARY KEY ([PlaylistId])
) STRICT;

INSERT INTO
    Playlist2
SELECT
    *
FROM
    Playlist;

DROP TABLE Playlist;

ALTER TABLE
    Playlist2 RENAME TO Playlist;

CREATE TABLE [PlaylistTrack2] (
    [PlaylistId] INTEGER NOT NULL,
    [TrackId] INTEGER NOT NULL,
    CONSTRAINT [PK_PlaylistTrack] PRIMARY KEY ([PlaylistId], [TrackId]),
    FOREIGN KEY ([PlaylistId]) REFERENCES [Playlist] ([PlaylistId]) ON DELETE NO ACTION ON UPDATE NO ACTION,
    FOREIGN KEY ([TrackId]) REFERENCES [Track] ([TrackId]) ON DELETE NO ACTION ON UPDATE NO ACTION
) STRICT;

INSERT INTO
    PlaylistTrack2
SELECT
    *
FROM
    PlaylistTrack;

DROP TABLE PlaylistTrack;

ALTER TABLE
    PlaylistTrack2 RENAME TO PlaylistTrack;

CREATE TABLE [Track2] (
    [TrackId] INTEGER NOT NULL,
    [Name] TEXT NOT NULL,
    [AlbumId] INTEGER NOT NULL,
    [MediaTypeId] INTEGER NOT NULL,
    [GenreId] INTEGER NOT NULL,
    [Composer] TEXT,
    [Milliseconds] INTEGER NOT NULL,
    [Bytes] INTEGER NOT NULL,
    [UnitPrice] REAL NOT NULL,
    CONSTRAINT [PK_Track] PRIMARY KEY ([TrackId]),
    FOREIGN KEY ([AlbumId]) REFERENCES [Album] ([AlbumId]) ON DELETE NO ACTION ON UPDATE NO ACTION,
    FOREIGN KEY ([GenreId]) REFERENCES [Genre] ([GenreId]) ON DELETE NO ACTION ON UPDATE NO ACTION,
    FOREIGN KEY ([MediaTypeId]) REFERENCES [MediaType] ([MediaTypeId]) ON DELETE NO ACTION ON UPDATE NO ACTION
) STRICT;

INSERT INTO
    Track2
SELECT
    *
FROM
    Track;

DROP TABLE Track;

ALTER TABLE
    Track2 RENAME TO Track;

PRAGMA foreign_keys = ON;

CREATE INDEX [IFK_AlbumArtistId] ON [Album] ([ArtistId]);

CREATE INDEX [IFK_CustomerSupportRepId] ON [Customer] ([SupportRepId]);

CREATE INDEX [IFK_EmployeeReportsTo] ON [Employee] ([ReportsTo]);

CREATE INDEX [IFK_InvoiceCustomerId] ON [Invoice] ([CustomerId]);

CREATE INDEX [IFK_InvoiceLineInvoiceId] ON [InvoiceLine] ([InvoiceId]);

CREATE INDEX [IFK_InvoiceLineTrackId] ON [InvoiceLine] ([TrackId]);

CREATE INDEX [IFK_PlaylistTrackPlaylistId] ON [PlaylistTrack] ([PlaylistId]);

CREATE INDEX [IFK_PlaylistTrackTrackId] ON [PlaylistTrack] ([TrackId]);

CREATE INDEX [IFK_TrackAlbumId] ON [Track] ([AlbumId]);

CREATE INDEX [IFK_TrackGenreId] ON [Track] ([GenreId]);

CREATE INDEX [IFK_TrackMediaTypeId] ON [Track] ([MediaTypeId]);