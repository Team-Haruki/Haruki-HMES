package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

type Config struct {
	Addr             string
	InternalToken    string
	CloudBaseURL     string
	CloudToken       string
	UserAgent        string
	PollTimeout      time.Duration
	CloudHTTPTimeout time.Duration
}

type Event struct {
	EventID        string `json:"event_id"`
	SubscriptionID string `json:"subscription_id"`
	EmptyResult    bool   `json:"empty_result"`
}

type PollResponse struct {
	Events []Event `json:"events"`
}

type cloudValidationResponse struct {
	Valid          bool    `json:"valid"`
	SubscriptionID string  `json:"subscription_id"`
	ExpiresAt      int64   `json:"expires_at"`
	PendingEvents  []Event `json:"pending_events"`
}

type Server struct {
	cfg    Config
	client *http.Client

	mu      sync.Mutex
	queues  map[string][]Event
	waiters map[string][]chan Event
}

func main() {
	cfg := loadConfig()
	server := NewServer(cfg)
	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", server.handleHealthz)
	mux.HandleFunc("GET /poll", server.handlePoll)
	mux.HandleFunc("POST /internal/events", server.handleInternalEvent)

	log.Printf("HMES listening on %s", cfg.Addr)
	if err := http.ListenAndServe(cfg.Addr, mux); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatal(err)
	}
}

func NewServer(cfg Config) *Server {
	return &Server{
		cfg: cfg,
		client: &http.Client{
			Timeout: cfg.CloudHTTPTimeout,
		},
		queues:  make(map[string][]Event),
		waiters: make(map[string][]chan Event),
	}
}

func (s *Server) handleHealthz(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{"status": "ok"})
}

func (s *Server) handleInternalEvent(w http.ResponseWriter, r *http.Request) {
	if !s.authorized(r) {
		writeJSON(w, http.StatusUnauthorized, map[string]any{"error": "unauthorized"})
		return
	}
	var event Event
	if err := json.NewDecoder(r.Body).Decode(&event); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": "invalid json"})
		return
	}
	event.EventID = strings.TrimSpace(event.EventID)
	event.SubscriptionID = strings.TrimSpace(event.SubscriptionID)
	if event.EventID == "" || event.SubscriptionID == "" {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": "event_id and subscription_id are required"})
		return
	}
	s.enqueue(event)
	writeJSON(w, http.StatusOK, map[string]any{"status": "ok"})
}

func (s *Server) handlePoll(w http.ResponseWriter, r *http.Request) {
	subscriptionID := strings.TrimSpace(r.URL.Query().Get("subscription_id"))
	token := strings.TrimSpace(r.URL.Query().Get("token"))
	if subscriptionID == "" || token == "" {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": "subscription_id and token are required"})
		return
	}

	validation, err := s.validateWithCloud(r.Context(), subscriptionID, token)
	if err != nil {
		log.Printf("cloud validation failed: subscription=%s err=%v", subscriptionID, err)
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": "cloud validation failed"})
		return
	}
	if !validation.Valid {
		writeJSON(w, http.StatusUnauthorized, map[string]any{"error": "invalid subscription token"})
		return
	}
	if len(validation.PendingEvents) > 0 {
		s.clearQueue(subscriptionID)
		writeJSON(w, http.StatusOK, PollResponse{Events: validation.PendingEvents})
		return
	}

	if events := s.popQueue(subscriptionID); len(events) > 0 {
		writeJSON(w, http.StatusOK, PollResponse{Events: events})
		return
	}

	event, ok := s.waitForEvent(r.Context(), subscriptionID)
	if !ok {
		writeJSON(w, http.StatusOK, PollResponse{Events: []Event{}})
		return
	}
	writeJSON(w, http.StatusOK, PollResponse{Events: []Event{event}})
}

func (s *Server) validateWithCloud(ctx context.Context, subscriptionID string, token string) (*cloudValidationResponse, error) {
	base := strings.TrimRight(strings.TrimSpace(s.cfg.CloudBaseURL), "/")
	if base == "" {
		return nil, fmt.Errorf("cloud base url is not configured")
	}
	u, err := url.Parse(base + "/internal/subscriptions/mysekai-birthday/validate")
	if err != nil {
		return nil, err
	}
	query := u.Query()
	query.Set("subscription_id", subscriptionID)
	query.Set("token", token)
	u.RawQuery = query.Encode()

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", s.cfg.UserAgent)
	if auth := bearerAuth(s.cfg.CloudToken); auth != "" {
		req.Header.Set("Authorization", auth)
	}
	resp, err := s.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, fmt.Errorf("cloud returned status %d", resp.StatusCode)
	}
	var result cloudValidationResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	return &result, nil
}

func (s *Server) enqueue(event Event) {
	s.mu.Lock()
	defer s.mu.Unlock()
	waiters := s.waiters[event.SubscriptionID]
	if len(waiters) == 0 {
		s.queues[event.SubscriptionID] = append(s.queues[event.SubscriptionID], event)
		return
	}
	delete(s.waiters, event.SubscriptionID)
	for _, waiter := range waiters {
		select {
		case waiter <- event:
		default:
		}
		close(waiter)
	}
}

func (s *Server) clearQueue(subscriptionID string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.queues, subscriptionID)
}

func (s *Server) popQueue(subscriptionID string) []Event {
	s.mu.Lock()
	defer s.mu.Unlock()
	events := s.queues[subscriptionID]
	if len(events) == 0 {
		return nil
	}
	delete(s.queues, subscriptionID)
	return append([]Event(nil), events...)
}

func (s *Server) waitForEvent(ctx context.Context, subscriptionID string) (Event, bool) {
	waiter := make(chan Event, 1)
	s.mu.Lock()
	if events := s.queues[subscriptionID]; len(events) > 0 {
		event := events[0]
		if len(events) == 1 {
			delete(s.queues, subscriptionID)
		} else {
			s.queues[subscriptionID] = events[1:]
		}
		s.mu.Unlock()
		return event, true
	}
	s.waiters[subscriptionID] = append(s.waiters[subscriptionID], waiter)
	s.mu.Unlock()

	timer := time.NewTimer(s.cfg.PollTimeout)
	defer timer.Stop()
	select {
	case event, ok := <-waiter:
		return event, ok
	case <-timer.C:
		s.removeWaiter(subscriptionID, waiter)
		return Event{}, false
	case <-ctx.Done():
		s.removeWaiter(subscriptionID, waiter)
		return Event{}, false
	}
}

func (s *Server) removeWaiter(subscriptionID string, target chan Event) {
	s.mu.Lock()
	defer s.mu.Unlock()
	waiters := s.waiters[subscriptionID]
	for i, waiter := range waiters {
		if waiter == target {
			waiters = append(waiters[:i], waiters[i+1:]...)
			break
		}
	}
	if len(waiters) == 0 {
		delete(s.waiters, subscriptionID)
		return
	}
	s.waiters[subscriptionID] = waiters
}

func (s *Server) authorized(r *http.Request) bool {
	expected := strings.TrimSpace(s.cfg.InternalToken)
	if expected == "" {
		return true
	}
	auth := strings.TrimSpace(r.Header.Get("Authorization"))
	return auth == expected || auth == bearerAuth(expected)
}

func loadConfig() Config {
	host := strings.TrimSpace(os.Getenv("HMES_HOST"))
	port := envInt("HMES_PORT", 7910)
	addr := strings.TrimSpace(os.Getenv("HMES_ADDR"))
	if addr == "" {
		if host == "" {
			host = "0.0.0.0"
		}
		addr = host + ":" + strconv.Itoa(port)
	}
	return Config{
		Addr:             addr,
		InternalToken:    os.Getenv("HMES_INTERNAL_TOKEN"),
		CloudBaseURL:     os.Getenv("HMES_CLOUD_INTERNAL_BASE_URL"),
		CloudToken:       os.Getenv("HMES_CLOUD_INTERNAL_TOKEN"),
		UserAgent:        envString("HMES_USER_AGENT", "Haruki-HMES"),
		PollTimeout:      time.Duration(envInt("HMES_POLL_TIMEOUT_SECONDS", 25)) * time.Second,
		CloudHTTPTimeout: time.Duration(envInt("HMES_CLOUD_TIMEOUT_SECONDS", 5)) * time.Second,
	}
}

func envString(key string, fallback string) string {
	value := strings.TrimSpace(os.Getenv(key))
	if value == "" {
		return fallback
	}
	return value
}

func envInt(key string, fallback int) int {
	value := strings.TrimSpace(os.Getenv(key))
	if value == "" {
		return fallback
	}
	parsed, err := strconv.Atoi(value)
	if err != nil || parsed <= 0 {
		return fallback
	}
	return parsed
}

func bearerAuth(token string) string {
	token = strings.TrimSpace(token)
	if token == "" {
		return ""
	}
	if strings.HasPrefix(strings.ToLower(token), "bearer ") {
		return token
	}
	return "Bearer " + token
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}
