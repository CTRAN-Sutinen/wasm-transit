# cap-gtfs

The feed-document capability. It answers the feed's own question over a
GTFS zip the kernel registered. `load_feed` answers what the zip
contains at a glance: its summary. cap-gtfs answers the feed's identity
(`feed_info`).

The day views, the watchlist, the pickers, and the names join are not
here. They answer over the network document, in cap-board, cap-runcut,
cap-calendar, and cap-directory. The bus has one answer for each
question.

cap-gtfs only parses and gives the identity, over transit-codec's
whole-spec `Feed`. Its charter is closed: parse, export, and primitive
boundary queries. Nothing more goes into GTFS.
