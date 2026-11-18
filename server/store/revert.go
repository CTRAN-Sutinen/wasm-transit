package store

import "fmt"

// RevertReport is what a revert did: the pointer moved from one head
// back to an older one, and the commits that move dropped.
type RevertReport struct {
	Pointer string     `json:"pointer"`
	From    string     `json:"from"`
	To      string     `json:"to"`
	Dropped []LogEntry `json:"dropped"`
}

// Revert moves pointer name back to to, a head its log holds. It is the
// emergency procedure for a bad release (docs/FORMATS.md, "Rolling a
// version back"), which an operator runs by hand. The move is a new
// commit of that head's document, under that head's envelope, so the
// log keeps every commit. The report lists the commits after that head:
// the ones the move dropped. Each names its writer and its document,
// which stays readable under its hash, for a person to enter again.
// A pointer already at to does not move.
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
