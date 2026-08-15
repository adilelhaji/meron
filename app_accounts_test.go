package main

import "testing"

// The UI reads each account's signature override back from account.list, so a
// field missing from the Account struct reads as "setting lost on restart".
func TestAccountListKeepsSignatureOverride(t *testing.T) {
	app, _ := newMailHandlerTestApp(t, sidecarResponsePlan{Result: map[string]any{
		"accounts": []any{map[string]any{
			"id":        "acc",
			"email":     "user@example.com",
			"signature": map[string]any{"mode": "custom", "html": "<p>Ping</p>"},
		}},
	}})

	res, err := app.accountList()
	if err != nil {
		t.Fatal(err)
	}
	accounts := res.(map[string]any)["accounts"].([]Account)
	if len(accounts) != 1 {
		t.Fatalf("accounts = %#v, want one", accounts)
	}
	signature, _ := accounts[0].Signature.(map[string]any)
	if signature["mode"] != "custom" || signature["html"] != "<p>Ping</p>" {
		t.Fatalf("signature = %#v, want the stored override", accounts[0].Signature)
	}
}

func TestAccountSetSaveSentCopyMapsNullableValue(t *testing.T) {
	app, writer := newMailHandlerTestApp(t,
		sidecarResponsePlan{Result: map[string]any{"ok": true}},
		sidecarResponsePlan{Result: map[string]any{"ok": true}},
	)

	if _, err := app.accountSetSaveSentCopy(map[string]any{"id": "acc", "value": true}); err != nil {
		t.Fatal(err)
	}
	if _, err := app.accountSetSaveSentCopy(map[string]any{"id": "acc", "value": nil}); err != nil {
		t.Fatal(err)
	}

	if len(writer.calls) != 2 {
		t.Fatalf("sidecar calls = %#v, want two calls", writer.calls)
	}
	assertCall(t, writer.calls[0], "account.setSaveSentCopy", map[string]any{"account": "acc", "value": true})
	assertCall(t, writer.calls[1], "account.setSaveSentCopy", map[string]any{"account": "acc", "value": nil})
}
