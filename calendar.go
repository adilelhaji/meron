package main

import (
	"errors"
	"strings"
)

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

// calendarCreate adds an event and returns it with the id the server assigned.
//
// Creating an appointment never notifies anyone: inviting people is a
// deliberate act, and doing it as a side effect of writing something in your
// own calendar would mail real people by accident.
func (a *App) calendarCreate(payload map[string]any) (any, error) {
	var req struct {
		AccountID string         `json:"account_id"`
		Event     map[string]any `json:"event"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.create", map[string]any{
		"account": req.AccountID,
		"event":   req.Event,
	})
}

// calendarUpdate applies a changed event. The event carries the change key it
// was read with, which the server checks so an edit made elsewhere is not
// silently overwritten.
func (a *App) calendarUpdate(payload map[string]any) (any, error) {
	var req struct {
		AccountID string         `json:"account_id"`
		Event     map[string]any `json:"event"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.update", map[string]any{
		"account": req.AccountID,
		"event":   req.Event,
	})
}

func (a *App) calendarDelete(payload map[string]any) (any, error) {
	var req struct {
		AccountID string `json:"account_id"`
		EventID   string `json:"event_id"`
		ChangeKey string `json:"change_key"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.delete", map[string]any{
		"account":    req.AccountID,
		"event":      req.EventID,
		"change_key": req.ChangeKey,
	})
}

// calendarCreateCalendar adds a calendar to the account.
func (a *App) calendarCreateCalendar(payload map[string]any) (any, error) {
	var req struct {
		AccountID string `json:"account_id"`
		Name      string `json:"name"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if strings.TrimSpace(req.Name) == "" {
		return nil, errors.New("a calendar needs a name")
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.createCalendar", map[string]any{
		"account": req.AccountID, "name": req.Name,
	})
}

func (a *App) calendarRenameCalendar(payload map[string]any) (any, error) {
	var req struct {
		AccountID  string `json:"account_id"`
		CalendarID string `json:"calendar_id"`
		Name       string `json:"name"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if strings.TrimSpace(req.Name) == "" {
		return nil, errors.New("a calendar needs a name")
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.renameCalendar", map[string]any{
		"account": req.AccountID, "calendar": req.CalendarID, "name": req.Name,
	})
}

// calendarDeleteCalendar removes a calendar and everything on it. The
// confirmation belongs to the UI; by the time this runs the choice is made.
func (a *App) calendarDeleteCalendar(payload map[string]any) (any, error) {
	var req struct {
		AccountID  string `json:"account_id"`
		CalendarID string `json:"calendar_id"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.deleteCalendar", map[string]any{
		"account": req.AccountID, "calendar": req.CalendarID,
	})
}

// calendarSetColor records the colour a calendar is drawn with. Local by
// design: Exchange has no colour other clients agree on.
func (a *App) calendarSetColor(payload map[string]any) (any, error) {
	var req struct {
		AccountID  string `json:"account_id"`
		CalendarID string `json:"calendar_id"`
		Color      string `json:"color"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.setColor", map[string]any{
		"account": req.AccountID, "calendar": req.CalendarID, "color": req.Color,
	})
}

// calendarCreateLocal adds a calendar that lives only in this copy of Meron.
// Nothing syncs it and nothing else holds a copy, which the interface says.
func (a *App) calendarCreateLocal(payload map[string]any) (any, error) {
	var req struct {
		AccountID string `json:"account_id"`
		Name      string `json:"name"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if strings.TrimSpace(req.Name) == "" {
		return nil, errors.New("a calendar needs a name")
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.createLocal", map[string]any{
		"account": req.AccountID, "name": req.Name,
	})
}

// calendarSubscribe follows a published calendar file. The core fetches it
// once before storing, so a URL that is not a calendar fails here rather than
// becoming a subscription that never fills.
func (a *App) calendarSubscribe(payload map[string]any) (any, error) {
	var req struct {
		AccountID string `json:"account_id"`
		Name      string `json:"name"`
		URL       string `json:"url"`
	}
	if err := decode(payload, &req); err != nil {
		return nil, err
	}
	if strings.TrimSpace(req.Name) == "" {
		return nil, errors.New("a calendar needs a name")
	}
	if a.sidecar == nil || !a.sidecar.Started() {
		return nil, a.engineUnavailable()
	}
	return a.sidecar.Call("calendar.subscribe", map[string]any{
		"account": req.AccountID, "name": req.Name, "url": req.URL,
	})
}
