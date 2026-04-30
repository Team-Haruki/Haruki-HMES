package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestPollReturnsPendingEventsFromCloud(t *testing.T) {
	cloud := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/internal/subscriptions/mysekai-birthday/validate" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer cloud-token" {
			t.Fatalf("authorization = %q, want Bearer cloud-token", got)
		}
		if got := r.URL.Query().Get("subscription_id"); got != "1" {
			t.Fatalf("subscription_id = %q, want 1", got)
		}
		if got := r.URL.Query().Get("token"); got != "poll-token" {
			t.Fatalf("token = %q, want poll-token", got)
		}
		writeJSON(w, http.StatusOK, cloudValidationResponse{
			Valid:          true,
			SubscriptionID: "1",
			PendingEvents: []Event{{
				EventID:        "9",
				SubscriptionID: "1",
				EmptyResult:    true,
			}},
		})
	}))
	defer cloud.Close()

	server := NewServer(Config{
		CloudBaseURL:     cloud.URL,
		CloudToken:       "cloud-token",
		UserAgent:        "test",
		PollTimeout:      10 * time.Millisecond,
		CloudHTTPTimeout: time.Second,
	})
	server.enqueue(Event{EventID: "stale-local", SubscriptionID: "1"})

	req := httptest.NewRequest(http.MethodGet, "/poll?subscription_id=1&token=poll-token", nil)
	rec := httptest.NewRecorder()
	server.handlePoll(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200, body=%s", rec.Code, rec.Body.String())
	}
	var resp PollResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if len(resp.Events) != 1 || resp.Events[0].EventID != "9" || !resp.Events[0].EmptyResult {
		t.Fatalf("events = %+v, want pending event 9", resp.Events)
	}
	if queued := server.popQueue("1"); len(queued) != 0 {
		t.Fatalf("expected local queue to be cleared, got %+v", queued)
	}
}

func TestInternalEventRequiresConfiguredToken(t *testing.T) {
	server := NewServer(Config{InternalToken: "secret"})

	req := httptest.NewRequest(http.MethodPost, "/internal/events", nil)
	rec := httptest.NewRecorder()
	server.handleInternalEvent(rec, req)
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want 401", rec.Code)
	}

	req = httptest.NewRequest(http.MethodPost, "/internal/events", strings.NewReader(`{"event_id":"11","subscription_id":"2"}`))
	req.Header.Set("Authorization", "Bearer secret")
	rec = httptest.NewRecorder()
	server.handleInternalEvent(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200, body=%s", rec.Code, rec.Body.String())
	}
	queued := server.popQueue("2")
	if len(queued) != 1 || queued[0].EventID != "11" {
		t.Fatalf("queued events = %+v, want event 11", queued)
	}
}
