package imports

import (
	"encoding/json"
	"strings"
	"testing"

	"rustwasm/server/store"
)

func TestADocumentStoredInAnotherVersionIsDerivedAgain(t *testing.T) {
	st := store.New(store.NewMemory())
	feed := "sha256:" + strings.Repeat("a", 64)
	doc := []byte(`{"kind":"network","schema_version":1}`)
	CommitDerivation(st, DerivedNetworkPointer, []string{feed}, "cap-network@sha256:"+strings.Repeat("b", 64), json.RawMessage(`{}`),
		derivedDoc{tag: "network", storeKind: "network_document", version: 1, body: doc, hash: store.HashOf(doc)})
	if _, _, ok := StoredDerivation(st, DerivedNetworkPointer, []string{feed}, "network"); ok {
		t.Fatal("a network stored in version 1 is not read by a build that writes version 2")
	}
	if _, _, ok := StoredDerivation(st, DerivedNetworkPointer, []string{"sha256:" + strings.Repeat("c", 64)}, "network"); ok {
		t.Fatal("a derivation of other inputs is not read")
	}
}
