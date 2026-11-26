package logs

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"strings"

	"rustwasm/server/store"
)

type SeedFile struct {
	Flag, Path string
}

// Seed gives each empty log its first entry at startup, from its
// kinds' old pointers, else a file in `files` (the dev store's -edits).
// The writer is migrate:<the first pointer read>, or migrate:-<flag>.
// The pointers are never written again.
func (ls *Logs) Seed(files map[string]SeedFile) {
	for _, name := range ls.Names() {
		l := ls.Get(name)
		head, err := ls.store.LogHeadOf(name)
		if err != nil || head.Seq > 0 || len(head.Gaps) > 0 {
			if err != nil {
				log.Printf("seed the %s log: %v", name, err)
			}
			continue
		}
		heads := map[string][]byte{}
		var note, hashes []string
		writer := ""
		for _, k := range l.kinds {
			for _, pointer := range k.Pointers {
				if strings.HasSuffix(pointer, "*") {
					continue
				}
				ptr, found, err := ls.store.Head(pointer)
				if err != nil || !found {
					continue
				}
				doc, _, err := ls.store.GetBlob(ptr.Hash)
				if err != nil {
					log.Printf("seed the %s log: the %s pointer's head %s: %v", name, pointer, ptr.Hash, err)
					return
				}
				heads[k.StoreKind] = doc
				note = append(note, pointer+" "+ptr.Hash)
				hashes = append(hashes, ptr.Hash)
				if writer == "" {
					writer = "migrate:" + pointer
				}
			}
			if f, ok := files[k.StoreKind]; ok && f.Path != "" && heads[k.StoreKind] == nil {
				doc, err := os.ReadFile(f.Path)
				if err != nil || len(doc) == 0 {
					continue
				}
				heads[k.StoreKind] = doc
				note = append(note, "-"+f.Flag+" "+store.HashOf(doc))
				hashes = append(hashes, store.HashOf(doc))
				if writer == "" {
					writer = "migrate:-" + f.Flag
				}
			}
		}
		if len(heads) == 0 {
			continue
		}
		for kind, doc := range heads {
			if fwd, migrated, err := ls.forward(kind, doc); err == nil && migrated {
				heads[kind] = fwd
			}
		}
		op, _ := json.Marshal(map[string]any{"cap": l.receive, "op": "migrate"})
		w := Write{Writer: writer, Op: "migrate", Note: strings.Join(note, ", ")}
		if len(hashes) == 1 {
			w.ArgsHash = hashes[0]
		}
		seq, _, _, refusal, err := ls.Receive(context.Background(), nil, l, w, Call{Ops: []json.RawMessage{op}, Heads: heads})
		if err == nil && refusal != nil {
			err = fmt.Errorf("%v", refusal.Body)
		}
		if err != nil {
			log.Printf("seed the %s log: %v", name, err)
			continue
		}
		log.Printf("seed the %s log: entry %d from %s", name, seq, w.Note)
	}
}
