package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestSSEReturnsPendingEventsFromCloud(t *testing.T) {
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
		if got := r.URL.Query().Get("token"); got != "sse-token" {
			t.Fatalf("token = %q, want sse-token", got)
		}
		if got := r.URL.Query().Get("subscription_version"); got != "v1" {
			t.Fatalf("subscription_version = %q, want v1", got)
		}
		writeJSON(w, http.StatusOK, cloudValidationResponse{
			Valid:               true,
			SubscriptionID:      "1",
			SubscriptionVersion: "v1",
			PendingEvents: []Event{{
				EventID:             "9",
				SubscriptionID:      "1",
				SubscriptionVersion: "v1",
				EmptyResult:         true,
			}},
		})
	}))
	defer cloud.Close()

	server := NewServer(Config{
		CloudBaseURL:         cloud.URL,
		CloudToken:           "cloud-token",
		UserAgent:            "test",
		SSEHeartbeatInterval: time.Hour,
		CloudHTTPTimeout:     time.Second,
	})
	server.publish(Event{EventID: "stale-local", SubscriptionID: "1", SubscriptionVersion: "v1"})

	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
	defer cancel()
	req := httptest.NewRequest(http.MethodGet, "/sse?subscription_id=1&subscription_version=v1&token=sse-token", nil).WithContext(ctx)
	rec := httptest.NewRecorder()
	server.handleSSE(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200, body=%s", rec.Code, rec.Body.String())
	}
	body := rec.Body.String()
	if !strings.Contains(body, "event: birthday_monitor_update") ||
		!strings.Contains(body, `"event_id":"9"`) ||
		!strings.Contains(body, `"empty_result":true`) {
		t.Fatalf("unexpected SSE body: %s", body)
	}
	if event, ok := server.popLatest(subscriptionKey("1", "v1")); ok {
		t.Fatalf("expected local latest to be cleared, got %+v", event)
	}
}

func TestSSEReturnsLatestLocalEvent(t *testing.T) {
	cloud := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if got := r.URL.Query().Get("subscription_id"); got != "1" {
			t.Fatalf("subscription_id = %q, want 1", got)
		}
		if got := r.URL.Query().Get("subscription_version"); got != "v1" {
			t.Fatalf("subscription_version = %q, want v1", got)
		}
		if got := r.URL.Query().Get("token"); got != "sse-token" {
			t.Fatalf("token = %q, want sse-token", got)
		}
		writeJSON(w, http.StatusOK, cloudValidationResponse{
			Valid:               true,
			SubscriptionID:      "1",
			SubscriptionVersion: "v1",
		})
	}))
	defer cloud.Close()

	server := NewServer(Config{
		CloudBaseURL:         cloud.URL,
		UserAgent:            "test",
		SSEHeartbeatInterval: time.Hour,
		CloudHTTPTimeout:     time.Second,
	})
	server.publish(Event{
		EventID:             "first",
		SubscriptionID:      "1",
		SubscriptionVersion: "v1",
	})
	server.publish(Event{
		EventID:             "latest",
		SubscriptionID:      "1",
		SubscriptionVersion: "v1",
		PayloadRef:          "redis-key",
	})

	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
	defer cancel()
	req := httptest.NewRequest(http.MethodGet, "/sse?subscription_id=1&subscription_version=v1&token=sse-token", nil).WithContext(ctx)
	rec := httptest.NewRecorder()
	server.handleSSE(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200, body=%s", rec.Code, rec.Body.String())
	}
	body := rec.Body.String()
	if !strings.Contains(body, "event: birthday_monitor_update") ||
		!strings.Contains(body, `"event_id":"latest"`) ||
		!strings.Contains(body, `"payload_ref":"redis-key"`) {
		t.Fatalf("unexpected SSE body: %s", body)
	}
	if strings.Contains(body, `"event_id":"first"`) {
		t.Fatalf("SSE body should only include latest pending event, got: %s", body)
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

	req = httptest.NewRequest(http.MethodPost, "/internal/events", strings.NewReader(`{"event_id":"11","subscription_id":"2","subscription_version":"v2","payload_ref":"ref"}`))
	req.Header.Set("Authorization", "Bearer secret")
	rec = httptest.NewRecorder()
	server.handleInternalEvent(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200, body=%s", rec.Code, rec.Body.String())
	}
	event, ok := server.popLatest(subscriptionKey("2", "v2"))
	if !ok || event.EventID != "11" || event.PayloadRef != "ref" {
		t.Fatalf("latest event = %+v ok=%v, want event 11", event, ok)
	}
}

func TestCloseSubscriptionRequiresTokenAndClosesClients(t *testing.T) {
	server := NewServer(Config{InternalToken: "secret"})
	key := subscriptionKey("2", "v2")
	ch := server.registerSSEClient(key)
	server.publish(Event{EventID: "pending", SubscriptionID: "2", SubscriptionVersion: "v2"})

	req := httptest.NewRequest(http.MethodPost, "/internal/subscriptions/2/close?subscription_version=v2", nil)
	req.SetPathValue("subscription_id", "2")
	rec := httptest.NewRecorder()
	server.handleCloseSubscription(rec, req)
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want 401", rec.Code)
	}

	req = httptest.NewRequest(http.MethodPost, "/internal/subscriptions/2/close?subscription_version=v2", nil)
	req.SetPathValue("subscription_id", "2")
	req.Header.Set("Authorization", "Bearer secret")
	rec = httptest.NewRecorder()
	server.handleCloseSubscription(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200, body=%s", rec.Code, rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `"closed_clients":1`) {
		t.Fatalf("unexpected close response: %s", rec.Body.String())
	}

	if _, ok := <-ch; ok {
		t.Fatalf("expected channel to be closed without delivering buffered events")
	}
	if event, ok := server.popLatest(key); ok {
		t.Fatalf("expected latest pending to be cleared, got %+v", event)
	}
}
