# delivery2door

Service that watches configured sources for delivery announcements and creates a door rule in the doorbell server (`../doorbell/server`) for the announced time span - so the door opens by itself while a delivery is expected.

It has its own web UI (login, delivery overview, service log) on `httpPort`.

## Architecture

Two abstractions for reading, both extensible by one more variant / match arm:

* **Source** (`src/sources/`) - a place announcements are read from. `mailSource` reads one IMAP
  account, restricted to a single folder. Fetching happens in `spawn_blocking`, since the `imap`
  crate is blocking.
* **Crawler** (`src/crawlers/`) - extracts delivery ids and the delivery window out of a fetched
  item. `regexCrawler` applies the configured `deliveryIdentifier` regex and the two date patterns;
  the `dhl-mail` crawler is just this crawler with DHL patterns.

`src/pollService.rs` glues them together: it queries every source at startup and then every
`queryIntervalSeconds`, runs the source's crawler over the new items and hands what it finds to the
reconciler.

### From an announcement to a door rule

* `src/deliveries/announcementStore.rs` maps a delivery id (the DHL piececode) to the rule that covers
  it, together with the window the doorbell service *knows*. Comparing against that window is what
  keeps the whole thing idempotent - as long as it matches, nothing is sent.
* `src/deliveries/ruleMapper.rs` turns a window into a rule: one day, two times of day. A window
  running past midnight becomes the doorbell's wrap-around form. A window that has already ended, is
  empty or longer than a day is rejected.
* `src/doorbell/ruleApiClient.rs` speaks the doorbell's `/api/rules` (see its readme), with the token
  from the config.
* `src/deliveries/reconciler.rs` decides between create, update and skip:

| Situation                       | What happens                                                   |
| ------------------------------- | -------------------------------------------------------------- |
| delivery id not known yet       | rule created, id remembered                                    |
| known, window unchanged         | nothing is sent                                                |
| known, window changed           | rule moved (the newest announcement wins, even if it is wider) |
| known, but the rule answers 404 | delivery counts as done, never recreated                       |
| window already ended            | no rule; the delivery is closed                                |

The rules are created with `deleteAtEnd`, so the doorbell service removes them itself once their
window is over.

### Fetching only what is new

Every item is fetched and parsed exactly once - forever. Without that, every poll would download the
whole folder again, including all the mail that has nothing to do with deliveries.
`src/deliveries/processedItemStore.rs` remembers two things, and `mailSource` uses both:

1. **`lookBackDays`** becomes an IMAP `SINCE` key - server-side.
2. A **UID high-water mark** per (folder, `UIDVALIDITY`) becomes `UID <highest+1>:*` - server-side.
   It never moves backwards, and it stops below an item that could not be read, so nothing is skipped
   for good.
3. The **`Message-ID`** of the remaining candidates is fetched header-only; only unknown ones get
   their body downloaded. This also covers a `UIDVALIDITY` reset and a mail moved into the folder with
   a low UID, which a high-water mark alone would miss.

In the steady state a poll is a single IMAP search with no hits and no download at all. The
**Re-scan** button on the deliveries page is the only way to make a source deliver its items again.

### Date parsing

The date parsing has three parts:

* `src/util/textNormalize.rs` reduces mail text to a plain sentence - the announcement is torn apart
  by bold markers, line breaks and non-breaking spaces encoded as `=C2=A0`.
* `src/crawlers/datePattern.rs` compiles a readable config pattern with `{Weekday}`, `{DD.MM}` and
  `{HH:MM}` placeholders into a regex. Literal spaces match any whitespace.
* `src/crawlers/deliveryWindow.rs` turns a match into a `from`/`to` window. Its reference time is the
  mail's `Date` header, not the clock: "heute" means the day the mail was *sent*, and that also
  completes a "05.01." with the right year. It is passed in, which is what makes it unit-testable.

Run the tests with `cargo test`.

## Setup

Needs the Rust toolchain plus the native TLS dependencies:

```
sudo apt-get install build-essential libssl-dev pkg-config
```

## Build & run

```
cargo build
cargo run -- --config config/config-test.json [--log-debug]
```

Release build:

```
cargo build --release
./target/release/delivery2door --config config/config-test.json
```

## Configuration

```json
{
    "general": {
        "queryIntervalSeconds": 600,
        "toleranceMinutes": 120,
        "fullDayStartHour": 8,
        "fullDayEndHour": 20,
        "logRetentionDays": 180
    },
    "httpPort": 3002,
    "users": [
        { "username": "...", "password": "..." }
    ],
    "doorbell": {
        "baseUrl": "http://localhost:3001",
        "apiToken": "one of the doorbell's apiTokens"
    },
    "ruleCreation": {
        "namePrefix": "DHL",
        "autoAck": true,
        "singleShot": false,
        "deleteAtEnd": true
    },
    "sources": [
        {
            "type": "mail",
            "server": "mail.example.de",
            "port": 143,
            "user": "...",
            "pass": "...",
            "restrictToFolder": "Inbox/test-delivery",
            "senderMasks": ["@dhl.de"],
            "lookBackDays": 14,
            "crawler": "dhl-mail"
        }
    ],
    "crawlers": {
        "dhl-mail": {
            "deliveryIdentifier": "piececode=([A-Za-z0-9]+)",
            "deliveryDateToday": "heute zwischen {HH:MM} - {HH:MM} Uhr",
            "deliveryDateFuture": "am {Weekday}, den {DD.MM}"
        }
    }
}
```

### general

`toleranceMinutes` (default 120), `fullDayStartHour` (8), `fullDayEndHour` (20) and
`logRetentionDays` (180) may be omitted.

`fullDayStartHour` / `fullDayEndHour` do two jobs. They are the span assumed for a day announced
without a time - and they are the hard bound of *every* window, whatever a mail announces. An
announcement is an unauthenticated mail and could be faked, so a delivery announced for 15:00 - 24:00
opens the door from 13:00 to 20:00, not into the night. During the day the bound costs nothing:
whoever is at home opens for a parcel service anyway. An announcement lying outside the hours
completely is refused with a warning, and no rule is created for it.

`fullDayEndHour` may be 24, which means midnight and leaves the evening unbounded. Startup fails if
the two do not span anything, since that would refuse every announcement.

### users, doorbell, ruleCreation

* `users` - login for the web UI, plain text in the config, same as in the doorbell server.
* `doorbell.apiToken` - has to be one of the tokens in the doorbell's own `apiTokens`. Startup fails if
  it is empty, since reaching the doorbell service is the whole point.
* `ruleCreation` - the whole section may be omitted; the defaults are `autoAck` on, `singleShot` off
  and `deleteAtEnd` on. `singleShot` is deliberately off: the courier may ring twice, and a second
  parcel can arrive in the same window.

### sources

* `restrictToFolder` - only this IMAP folder is searched. Mails are fetched with `BODY.PEEK`, so
  they stay unread.
* `senderMasks` - optional list, becomes an IMAP `FROM` search (substring match per entry, OR-ed).
  A list because one carrier may write from several addresses - Amazon uses `shipment-tracking@` and
  `versandbestaetigung@`. Omit it to search all mails of the folder. DHL changes its local part
  (`noreply@`, `livetracking@`, ...), so the mask is the domain `@dhl.de`; what really decides
  whether a mail is an announcement is the crawler's `deliveryIdentifier`, not the sender.
* `lookBackDays` - default 14. Mails older than this are never looked at; the IMAP server filters by
  it, so they are not even transferred.
* `crawlers` - list of entries of the `crawlers` section, all checked at startup. Every mail of the
  folder is offered to each of them, so one folder can hold the announcements of several carriers -
  only the crawler whose `deliveryIdentifier` matches reports anything. Do **not** configure two
  sources on the same folder instead: they share the record of what has already been processed and
  would skip each other's mails.
* Port `993` connects with TLS, every other port with STARTTLS. A server offering neither is an
  error - the login never falls back to plaintext.

### crawlers

* `namePrefix` - optional, prefixed to the delivery id to name the rule, e.g. `DHL 357820075428` or
  `Amazon 305-3203050-2444367`. It belongs to the crawler because it says which carrier the parcel
  came from; `ruleCreation.namePrefix` is only the fallback.
* `deliveryIdentifier` - a regex. Capture group 1 is the delivery id; patterns without a group use
  the whole match. It also decides *whether* a crawler feels responsible for a mail at all, so it has
  to be specific enough not to fire on another carrier's mail.
* `deliveryDateToday` / `deliveryDateRelativeDay` / `deliveryDateFuture` - all optional; a crawler
  without any of them reports `window unknown`. They are **not** regexes but literal text with
  placeholders:

  | Placeholder     | Matches                          | Example    |
  | --------------- | -------------------------------- | ---------- |
  | `{Weekday}`     | a German weekday name            | `Mittwoch` |
  | `{RelativeDay}` | a day named relative to the mail | `morgen`   |
  | `{DD.MM}`       | day and month                    | `12.08`    |
  | `{HH:MM}`       | a time of day                    | `13:45`    |

  A run of spaces in the pattern matches any whitespace, so line breaks inside the announcement do
  not break the match. Every other character is literal - `.` and `(` need no escaping.

  Keep a pattern as short as the announcement allows. The normalization removes the mail's own
  formatting, but not HTML markup - DHL's newer mails put HTML in the `text/plain` part as well, so
  the announcement reads `heute zwischen 13:55 - 15:25 Uhr</strong> durch Ihre Paketzustellkraft`.
  Every word beyond what identifies the announcement is one more place for a tag to break the match.

  They are checked most-precise-first, and the first one that matches decides:

  1. `deliveryDateToday` needs two `{HH:MM}`; the window is those two times on the day the mail was
     **sent**, widened by `toleranceMinutes` on both sides.
  2. `deliveryDateRelativeDay` needs a `{RelativeDay}` (`heute`, `morgen`, `uebermorgen`); the window
     is the whole of that day, counted from the send day.
  3. `deliveryDateFuture` needs a `{DD.MM}`; the window is the whole of that day, and the year follows
     from the send day. A `{Weekday}` in the pattern is cross-checked against the date and the match is
     rejected if the two disagree.

  "The whole of a day" is `fullDayStartHour` to `fullDayEndHour`, without tolerance - it is wide
  enough already. Whichever of the three matched, the window is narrowed to those same hours of the
  day it starts on afterwards, so the tolerance can never push a rule into the night.

  A `{HH:MM}` of `24:00` is read as the end of the day.

### Supported carriers

Both come out of the same `regexCrawler`, only the config differs:

| Crawler       | Senders                                                        | Id                               | Announcements                                                     |
| ------------- | -------------------------------------------------------------- | -------------------------------- | ----------------------------------------------------------------- |
| `dhl-mail`    | `@dhl.de`                                                      | `piececode=...`                  | `heute zwischen 13:45 - 15:15 Uhr ...`, `am Mittwoch, den 12.08.` |
| `amazon-mail` | `shipment-tracking@amazon.de`, `versandbestaetigung@amazon.de` | `Bestellnr. 305-3203050-2444367` | `Zustellung heute 15:15 - 18:15`, `Ankunft morgen`                |

Amazon sends the shipping confirmation with `Ankunft morgen` first and the `In Zustellung` mail with
the precise window on the delivery day - the second one moves the rule that the first one created.

Note that all searching happens in the MIME parts decoded to text, not in the raw mail source -
`piececode=3D357820075428` in a raw link would otherwise yield the delivery id `3D357820075428`,
since `=3D` is an encoded "=". The raw source is only used for documents that cannot be decoded.

## Web UI

On `http://localhost:<httpPort>`, login with one of the configured `users`:

* **Main page** - sources, last and next poll, how many deliveries are active, how many mails have
  been looked at.
* **Deliveries** - one row per delivery id with its window, state and rule id, and below it the mails
  that were looked at, including the ones with no delivery id in them. Per source a **Re-scan**
  button, which forgets its processed items, its read position and its deliveries so the next poll
  reads the folder again from `lookBackDays`. Existing rules are left alone - they delete themselves.
* **Service log** - the persistent event log (`data/serviceLog.sqlite`), same layout as the doorbell's
  server log.

## Data directory

Created on the first start, `.gitignore`d:

* `data/deliveries.sqlite` - processed items, source positions and the delivery-to-rule mapping
* `data/serviceLog.sqlite` - the service log
* `data/sessions.json` - web UI sessions

```
sqlite3 -header -column data/deliveries.sqlite "SELECT * FROM deliveryAnnouncements;"
sqlite3 -header -column data/deliveries.sqlite "SELECT * FROM sourceState;"
```

## Build and deploy for Raspberry Pi

Mirrors the doorbell server's `scripts/`, minus the broker and the tunnel - this service only reaches
out as a client (IMAP to the provider, HTTP to the doorbell service).

* `scripts/prepare-raspi.sh` once: Rust, build tools, `sqlite3`, the directories, and the systemd
  service. Skips rustup if the Pi already has it from the doorbell setup.
* `scripts/build-and-deploy-raspi.sh` for every deployment: transfers `src/`, `static/`, `templates/`,
  `Cargo.toml`, `Cargo.lock`, `build.rs` and `config/config-release.json`, builds on the Pi and
  restarts the service. `data/` is never touched, so the databases survive.
* `scripts/prepare-service-raspi.sh` to (re-)install just the systemd unit.
* `scripts/terminate-service-sigterm.sh` to stop a locally running instance the way systemd would.

The unit is ordered `After=doorbell.service` and waits 15 s before the first poll, so the doorbell's
HTTP port is open by the time the first rule is created.

```
sudo systemctl {start|stop|restart|status} delivery2door.service
journalctl -u delivery2door.service -f
```
