package main

import (
	"log"
	"time"
)

// today reads the clock in the agency's zone, so a server in a UTC
// container answers the agency's service day. With no zone known yet,
// it uses the host's local date. -today pins it on a test server.
func (s *server) today() string {
	if s.fixedToday != "" {
		return s.fixedToday
	}
	now := time.Now()
	if s.now != nil {
		now = s.now()
	}
	if loc := s.agencyZone(); loc != nil {
		now = now.In(loc)
	}
	return now.Format("2006-01-02")
}

// writeDate is today() for the reference rules of a log write.
// It never loads the feed, because the feed's own import writes the
// stops log while Load holds its lock.
func (s *server) writeDate() string {
	if s.fixedToday != "" {
		return s.fixedToday
	}
	now := time.Now()
	if s.now != nil {
		now = s.now()
	}
	zone := ""
	if s.feeder != nil {
		zone, _ = s.feeder.zone.Load().(string)
	}
	if zone == "" {
		s.zoneMu.Lock()
		zone = s.zone
		s.zoneMu.Unlock()
	}
	if loc, err := time.LoadLocation(zone); zone != "" && err == nil {
		now = now.In(loc)
	}
	return now.Format("2006-01-02")
}

// agencyZone is the published feed's zone, else the last published
// pick's. It is nil when neither is known or the tz database lacks the
// name.
func (s *server) agencyZone() *time.Location {
	zone := ""
	if s.feeder != nil {
		if feed, err := s.feeder.Load(); err == nil {
			zone = feed.timezone
		}
	}
	if zone == "" {
		s.zoneMu.Lock()
		zone = s.zone
		s.zoneMu.Unlock()
	}
	if zone == "" {
		return nil
	}
	loc, err := time.LoadLocation(zone)
	if err != nil {
		log.Printf("agency timezone %q: %v (counting today in the host's zone)", zone, err)
		return nil
	}
	return loc
}
