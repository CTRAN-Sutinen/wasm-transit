package store

import (
	"encoding/json"
	"fmt"
)

type RevertReport struct {
	Pointer string     `json:"pointer"`
	From    string     `json:"from"`
	To      string     `json:"to"`
	Dropped []LogEntry `json:"dropped"`
	// References name a dropped commit's document by its hash (D-233), for
	// a person to judge. Absent when no check ran.
	References []json.RawMessage `json:"references,omitempty"`
}

// Revert is the emergency procedure for a bad release (docs/FORMATS.md,
// "Rolling a version back"). The move is a new commit of that head's
// document, so the log keeps every commit. A dropped commit's document
// stays readable under its hash, for a person to enter again. A pointer
// already at to does not move.
func Revert(s *Store, name, to, writer string, atMs int64) (RevertReport, error) {
	report := RevertReport{Pointer: name, To: to, Dropped: []LogEntry{}}
	head, found, err := s.Head(name)
	if err != nil {
		return report, err
	}
	if !found {
		return report, fmt.Errorf("no pointer %q", name)
	}
	report.From = head.Hash
	entries, _, err := s.Log(name, "")
	if err != nil {
		return report, err
	}
	at := -1
	for i, entry := range entries {
		if entry.To == to {
			at = i
		}
	}
	if at < 0 {
		return report, fmt.Errorf("%s was never the head of %q", to, name)
	}
	if head.Hash == to {
		return report, nil
	}
	data, _, err := s.GetBlob(to)
	if err != nil {
		return report, err
	}
	was := entries[at]
	env := Envelope{Kind: was.Kind, SchemaVersion: was.SchemaVersion, ProducedBy: was.ProducedBy}
	if _, _, err := s.CAS(name, head.Hash, data, env, to, writer, atMs); err != nil {
		return report, err
	}
	report.Dropped = append(report.Dropped, entries[at+1:]...)
	return report, nil
}
