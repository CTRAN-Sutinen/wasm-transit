package logs

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

// The reads: each kind's document at the head of its log's fold.

func (l *Log) Seq() int64 {
	l.fold.mu.Lock()
	defer l.fold.mu.Unlock()
	return l.fold.seq
}

// Hash is the hash of the kind's document in the fold, or "".
func (l *Log) Hash(storeKind string) string {
	l.fold.mu.Lock()
	defer l.fold.mu.Unlock()
	return l.docs[storeKind].hash
}

// Current answers the kind's document at the head. While the bucket does
// not answer, it answers the fold this process holds, stale.
func (ls *Logs) Current(l *Log, storeKind string) (doc []byte, head store.Head, err error) {
	stale, err := ls.refreshLog(l.fold)
	if err != nil {
		return nil, store.Head{Name: l.name}, err
	}
	l.fold.mu.Lock()
	defer l.fold.mu.Unlock()
	held := l.docs[storeKind]
	return held.doc, store.Head{Name: l.name, Hash: held.hash, Seq: l.fold.seq, AsOfMs: l.fold.asOfMs, Stale: stale}, nil
}

type logDoc struct {
	hash string
	doc  []byte
}

// Caller holds the fold's lock.
func (ls *Logs) assembleKept(l *Log, records map[string]json.RawMessage) error {
	ctx := context.Background()
	l.docs = map[string]logDoc{}
	for _, k := range l.kinds {
		if len(k.Records) == 0 || k.Module == "" {
			continue
		}
		// A kind whose merge is LWW always has a document: an empty log's is the
		// empty document.
		raw, ok := records[k.Records[0].Kind]
		own := ownRecord(records, k.StoreKind)
		if (!ok || len(raw) <= 2) && own == nil && k.Merge != "lww" {
			continue
		}
		if !ok {
			raw = json.RawMessage("{}")
		}
		req := map[string]any{"op": "log_document", "kind": k.StoreKind, "records": raw}
		if own != nil {
			req["document"] = own
		}
		var out struct {
			DocumentHash string `json:"document_hash"`
			Document     string `json:"document"`
		}
		// The long-lived instance holds each head from the moment it is
		// assembled, so a write registers none, and a refresh carries them.
		long := ls.long()
		err := long.Sequence(ctx, func(inst *wasmhost.Instance) error {
			if err := inst.As(k.Module).Invoke(ctx, req, &out); err != nil {
				return err
			}
			if k.Load == nil || !inst.Documents(l.receive)[strings.TrimPrefix(k.Load.Op, "load_")] {
				return nil
			}
			_, err := long.Head(ctx, inst, l.name+"/"+k.StoreKind, out.DocumentHash, k.Load.Code, []byte(out.Document))
			return err
		})
		if err != nil {
			return fmt.Errorf("the %s document: %w", k.StoreKind, err)
		}
		l.docs[k.StoreKind] = logDoc{out.DocumentHash, []byte(out.Document)}
	}
	return nil
}

func (ls *Logs) Head(storeKind string) (hash string, doc []byte, found bool, err error) {
	for _, l := range ls.all() {
		for _, k := range l.kinds {
			if k.StoreKind != storeKind {
				continue
			}
			if _, err := ls.refreshLog(l.fold); err != nil {
				return "", nil, false, err
			}
			l.fold.mu.Lock()
			defer l.fold.mu.Unlock()
			held := l.docs[storeKind]
			return held.hash, held.doc, held.hash != "", nil
		}
	}
	return "", nil, false, nil
}

func (ls *Logs) Doc(hash string) (kind string, doc []byte, ok bool) {
	if hash == "" {
		return "", nil, false
	}
	for _, l := range ls.all() {
		l.fold.mu.Lock()
		for k, held := range l.docs {
			if held.hash == hash {
				l.fold.mu.Unlock()
				return k, held.doc, true
			}
		}
		l.fold.mu.Unlock()
	}
	return "", nil, false
}

func (ls *Logs) Pins() (pins map[string]string, seqs map[string]int64) {
	pins, seqs = map[string]string{}, map[string]int64{}
	for _, name := range ls.Names() {
		l := ls.Get(name)
		if _, err := ls.refreshLog(l.fold); err != nil {
			continue
		}
		l.fold.mu.Lock()
		for _, k := range l.kinds {
			if held := l.docs[k.StoreKind]; held.hash != "" {
				pins[k.StoreKind+"_hash"] = held.hash
			}
		}
		if l.fold.seq > 0 {
			seqs[l.name] = l.fold.seq
		}
		l.fold.mu.Unlock()
	}
	return pins, seqs
}

func ownRecord(records map[string]json.RawMessage, kind string) json.RawMessage {
	var byID map[string]json.RawMessage
	if raw, ok := records[kind]; ok {
		json.Unmarshal(raw, &byID)
	}
	return byID["document"]
}

func (ls *Logs) Of(storeKind string) *Log {
	for _, l := range ls.all() {
		for _, k := range l.kinds {
			if k.StoreKind == storeKind {
				return l
			}
		}
	}
	return nil
}

func (l *Log) ruleOf(storeKind string) string {
	for _, k := range l.kinds {
		if k.StoreKind == storeKind {
			return k.Rule()
		}
	}
	return storeKind
}

func (l *Log) LoadCode(storeKind string) uint32 {
	for _, k := range l.kinds {
		if k.StoreKind == storeKind && k.Load != nil {
			return k.Load.Code
		}
	}
	return 0
}

func (ls *Logs) Seq(storeKind string) int64 {
	l := ls.Of(storeKind)
	if l == nil {
		return 0
	}
	if _, err := ls.refreshLog(l.fold); err != nil {
		return 0
	}
	l.fold.mu.Lock()
	defer l.fold.mu.Unlock()
	if l.docs[storeKind].hash == "" {
		return 0
	}
	return l.fold.seq
}
