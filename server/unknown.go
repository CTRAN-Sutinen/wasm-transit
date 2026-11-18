package main

// The fields a write carries that this build does not know (D-167).
// Every authored kind keeps them: a writer on an older build must not
// drop what a newer one wrote. The receive path never refuses one. It
// flags it for a person: the module that checks the write names each
// field (`unknown_fields` in its answer), and the server passes the
// names back on the write's response and logs them with the writer.

import (
	"log"
	"net/http"
	"strings"
)

// unknownFieldsHeader names, on a write's response, the fields the
// written document carries that this build does not know.
const unknownFieldsHeader = "X-Unknown-Fields"

// flagUnknown sets the header and logs the write when it carried
// fields this build does not know. Nothing when it carried none.
func flagUnknown(w http.ResponseWriter, what, writer string, names []string) {
	if len(names) == 0 {
		return
	}
	w.Header().Set(unknownFieldsHeader, strings.Join(names, ", "))
	log.Printf("receive %s: %s wrote fields this build does not know, kept: %s", what, writer, strings.Join(names, ", "))
}
