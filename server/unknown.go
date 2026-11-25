package main

// A write may carry fields that this build does not know. Every
// authored kind keeps them, so a writer on an older build does not drop
// what a newer one wrote. The write path never refuses one. It flags
// the field for a person.

import (
	"log"
	"net/http"
	"strings"
)

const unknownFieldsHeader = "X-Unknown-Fields"

func flagUnknown(w http.ResponseWriter, what, writer string, names []string) {
	if len(names) == 0 {
		return
	}
	w.Header().Set(unknownFieldsHeader, strings.Join(names, ", "))
	log.Printf("receive %s: %s wrote fields this build does not know, kept: %s", what, writer, strings.Join(names, ", "))
}
