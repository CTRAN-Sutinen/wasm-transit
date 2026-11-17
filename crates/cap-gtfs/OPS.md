# gtfs

The feed document: the feed's own question over a GTFS static zip. The kernel registers the zip by the content hash of its bytes. The kernel's `load_feed` answers what the zip contains at a glance, its summary. This capability answers the feed's identity from `feed_info.txt`: the version and the publisher.

The day views and the directory questions (pickers, the names join) are not here. They answer over the network document, in the board, runcut, calendar, and directory capabilities. There is one question and one answer on the bus.

This capability has no ops that author anything.

## feed_info

The feed's own metadata (version, publisher), when the feed provides `feed_info.txt`. This is the identity question: which feed is this?
