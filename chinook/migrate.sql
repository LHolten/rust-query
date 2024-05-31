-- altered from https://github.com/lerocha/chinook-database
PRAGMA foreign_keys = OFF;

CREATE TABLE [Album2] (
    [id] INTEGER PRIMARY KEY,
    [title] TEXT NOT NULL,
    [artist] INTEGER NOT NULL,
    FOREIGN KEY ([artist]) REFERENCES [artist] ([id])
) STRICT;

INSERT INTO
    Album2
SELECT
    *
FROM
    Album;

DROP TABLE Album;

ALTER TABLE
    Album2 RENAME TO album;

CREATE TABLE [Artist2] (
    [id] INTEGER PRIMARY KEY,
    [name] TEXT NOT NULL
) STRICT;

INSERT INTO
    Artist2
SELECT
    *
FROM
    Artist;

DROP TABLE Artist;

ALTER TABLE
    Artist2 RENAME TO artist;

CREATE TABLE [Customer2] (
    [id] INTEGER PRIMARY KEY,
    [first_name] TEXT NOT NULL,
    [last_name] TEXT NOT NULL,
    [company] TEXT,
    [address] TEXT NOT NULL,
    [city] TEXT NOT NULL,
    [state] TEXT,
    [country] TEXT NOT NULL,
    [postal_code] TEXT,
    [phone] TEXT,
    [fax] TEXT,
    [email] TEXT NOT NULL,
    [support_rep] INTEGER NOT NULL,
    FOREIGN KEY ([support_rep]) REFERENCES [employee] ([id])
) STRICT;

INSERT INTO
    Customer2
SELECT
    *
FROM
    Customer;

DROP TABLE Customer;

ALTER TABLE
    Customer2 RENAME TO customer;

CREATE TABLE [Employee2] (
    [id] INTEGER PRIMARY KEY,
    [last_name] TEXT NOT NULL,
    [first_name] TEXT NOT NULL,
    [title] TEXT,
    [reports_to] INTEGER,
    [birth_day] TEXT,
    [hire_date] TEXT,
    [address] TEXT,
    [city] TEXT,
    [state] TEXT,
    [country] TEXT,
    [postal_code] TEXT,
    [phone] TEXT,
    [fax] TEXT,
    [email] TEXT NOT NULL,
    FOREIGN KEY ([reports_to]) REFERENCES [employee] ([id])
) STRICT;

INSERT INTO
    Employee2
SELECT
    *
FROM
    Employee;

DROP TABLE Employee;

ALTER TABLE
    Employee2 RENAME TO employee;

CREATE TABLE [Genre2] (
    [id] INTEGER PRIMARY KEY,
    [name] TEXT NOT NULL
) STRICT;

INSERT INTO
    Genre2
SELECT
    *
FROM
    Genre;

DROP TABLE Genre;

ALTER TABLE
    Genre2 RENAME TO genre;

CREATE TABLE [Invoice2] (
    [id] INTEGER PRIMARY KEY,
    [customer] INTEGER NOT NULL,
    [invoice_date] TEXT NOT NULL,
    [billing_address] TEXT,
    [billing_city] TEXT,
    [billing_state] TEXT,
    [billing_country] TEXT,
    [billing_postal_code] TEXT,
    [total] REAL NOT NULL,
    FOREIGN KEY ([customer]) REFERENCES [customer] ([id])
) STRICT;

INSERT INTO
    Invoice2
SELECT
    *
FROM
    Invoice;

DROP TABLE Invoice;

ALTER TABLE
    Invoice2 RENAME TO invoice;

CREATE TABLE [InvoiceLine2] (
    [id] INTEGER PRIMARY KEY,
    [invoice] INTEGER NOT NULL,
    [track] INTEGER NOT NULL,
    [unit_price] REAL NOT NULL,
    [quantity] INTEGER NOT NULL,
    FOREIGN KEY ([invoice]) REFERENCES [invoice] ([id]),
    FOREIGN KEY ([track]) REFERENCES [track] ([id])
) STRICT;

INSERT INTO
    InvoiceLine2
SELECT
    *
FROM
    InvoiceLine;

DROP TABLE InvoiceLine;

ALTER TABLE
    InvoiceLine2 RENAME TO invoice_line;

CREATE TABLE [MediaType2] (
    [id] INTEGER PRIMARY KEY,
    [name] TEXT NOT NULL
) STRICT;

INSERT INTO
    MediaType2
SELECT
    *
FROM
    MediaType;

DROP TABLE MediaType;

ALTER TABLE
    MediaType2 RENAME TO media_type;

CREATE TABLE [Playlist2] (
    [id] INTEGER PRIMARY KEY,
    [name] TEXT NOT NULL
) STRICT;

INSERT INTO
    Playlist2
SELECT
    *
FROM
    Playlist;

DROP TABLE Playlist;

ALTER TABLE
    Playlist2 RENAME TO playlist;

CREATE TABLE [PlaylistTrack2] (
    [id] INTEGER PRIMARY KEY,
    [playlist] INTEGER NOT NULL,
    [track] INTEGER NOT NULL,
    CONSTRAINT [PlaylistTrackUnique] UNIQUE ([playlist], [track]),
    FOREIGN KEY ([playlist]) REFERENCES [playlist] ([id]),
    FOREIGN KEY ([track]) REFERENCES [track] ([id])
) STRICT;

INSERT INTO
    PlaylistTrack2
SELECT
    ROWID,
    PlaylistId,
    TrackId
FROM
    PlaylistTrack;

DROP TABLE PlaylistTrack;

ALTER TABLE
    PlaylistTrack2 RENAME TO playlist_track;

CREATE TABLE [Track2] (
    [id] INTEGER PRIMARY KEY,
    [name] TEXT NOT NULL,
    [album] INTEGER NOT NULL,
    [media_type] INTEGER NOT NULL,
    [genre] INTEGER NOT NULL,
    [composer] TEXT,
    [milliseconds] INTEGER NOT NULL,
    [bytes] INTEGER NOT NULL,
    [unit_price] REAL NOT NULL,
    FOREIGN KEY ([album]) REFERENCES [album] ([id]),
    FOREIGN KEY ([genre]) REFERENCES [genre] ([id]),
    FOREIGN KEY ([media_type]) REFERENCES [media_type] ([id])
) STRICT;

INSERT INTO
    Track2
SELECT
    *
FROM
    Track;

DROP TABLE Track;

ALTER TABLE
    Track2 RENAME TO track;

PRAGMA foreign_keys = ON;