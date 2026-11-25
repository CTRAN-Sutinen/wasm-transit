package wasmhost

import (
	"encoding/json"
	"errors"
)

// ModuleError is the error a module answers, `{"message", "code"?, ...}`
// (docs/architecture/bus.md, "Answers"). A host acts on Code, never on Message,
// which is text for a person. Raw keeps the object as the module wrote it,
// fields and all.
type ModuleError struct {
	Message string
	Code    string
	Raw     json.RawMessage
}

func (e *ModuleError) UnmarshalJSON(b []byte) error {
	var fields struct {
		Message string `json:"message"`
		Code    string `json:"code"`
	}
	if err := json.Unmarshal(b, &fields); err != nil {
		return err
	}
	*e = ModuleError{Message: fields.Message, Code: fields.Code, Raw: append(json.RawMessage(nil), b...)}
	return nil
}

func (e ModuleError) MarshalJSON() ([]byte, error) {
	if len(e.Raw) > 0 {
		return e.Raw, nil
	}
	return json.Marshal(map[string]string{"message": e.Message})
}

// Refused is a call that the module answered with an error.
type Refused struct{ ModuleError }

func (r *Refused) Error() string { return "op failed: " + r.Message }

// CodedRefusal finds a module error that carries a code in err's chain.
func CodedRefusal(err error) (ModuleError, bool) {
	var r *Refused
	if errors.As(err, &r) && r.Code != "" {
		return r.ModuleError, true
	}
	return ModuleError{}, false
}
