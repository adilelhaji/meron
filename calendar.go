package main

// Calendar bridge methods. These are pass-throughs: the core owns the model,
// the cache and the sync, so there is nothing to reshape on the way past.

// calendarList returns the calendars an account exposes, including the ones
// the user has hidden — the settings panel needs to show those to offer them
// back.
func (a *App) calendarList(payload map[string]any) (any, error) {
	var req struct {
		AccountID string `json:"account_id"`
	}
	_ = decode(payload, &req)
	if a.sidecar == nil || !a.sidecar.Started() {
		return map[string]any{"calendars": []any{}}, nil
	}
	return a.sidecar.Call("calendar.list", map[string]any{"account": req.AccountID})
}

// calendarEvents returns the occurrences overlapping a window, from the cache,
// and refreshes behind the request unless told not to.
//
// `from` and `to` are epoch seconds, and the window is half-open: an event
// ending exactly at `from` belongs to the previous window, not this one.
func (a *App) calendarEvents(payload map[string]any) (any, error) {
	var req struct {
		AccountID string `json:"account_id"`
		From      int64  `json:"from"`
		To        int64  `json:"to"`
		Refresh   *bool  `json:"refresh"`
	}
	_ = decode(payload, &req)
	if a.sidecar == nil || !a.sidecar.Started() {
		return map[string]any{"events": []any{}}, nil
	}
	refresh := true
	if req.Refresh != nil {
		refresh = *req.Refresh
	}
	return a.sidecar.Call("calendar.events", map[string]any{
		"account": req.AccountID,
		"from":    req.From,
		"to":      req.To,
		"refresh": refresh,
	})
}

// calendarSetEnabled shows or hides one calendar. Hiding never forgets it, and
// the choice survives a resync of the calendar list.
func (a *App) calendarSetEnabled(payload map[string]any) (any, error) {
	var req struct {
		AccountID  string `json:"account_id"`
		CalendarID string `json:"calendar_id"`
		Enabled    bool   `json:"enabled"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.setEnabled", map[string]any{
		"account":  req.AccountID,
		"calendar": req.CalendarID,
		"enabled":  req.Enabled,
	})
}
