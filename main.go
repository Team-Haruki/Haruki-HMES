package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
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
	Addr                 string
	InternalToken        string
	CloudBaseURL         string
	CloudToken           string
	UserAgent            string
	SSEHeartbeatInterval time.Duration
	CloudHTTPTimeout     time.Duration
}

type Event struct {
	EventID             string `json:"event_id"`
	SubscriptionID      string `json:"subscription_id"`
	SubscriptionVersion string `json:"subscription_version,omitempty"`
	PayloadRef          string `json:"payload_ref,omitempty"`
	EmptyResult         bool   `json:"empty_result"`
}

type cloudValidationResponse struct {
	Valid               bool    `json:"valid"`
	SubscriptionID      string  `json:"subscription_id"`
	SubscriptionVersion string  `json:"subscription_version,omitempty"`
	ExpiresAt           int64   `json:"expires_at"`
	PendingEvents       []Event `json:"pending_events"`
}

type closeSubscriptionRequest struct {
	SubscriptionVersion string `json:"subscription_version"`
}

type Server struct {
	cfg    Config
	client *http.Client

	mu         sync.Mutex
	latest     map[string]Event
	sseClients map[string]map[chan Event]struct{}
}

func main() {
	cfg := loadConfig()
	server := NewServer(cfg)
	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", server.handleHealthz)
	mux.HandleFunc("GET /sse", server.handleSSE)
	mux.HandleFunc("POST /internal/events", server.handleInternalEvent)
	mux.HandleFunc("POST /internal/subscriptions/{subscription_id}/close", server.handleCloseSubscription)

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
		latest:     make(map[string]Event),
		sseClients: make(map[string]map[chan Event]struct{}),
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
	event.SubscriptionVersion = strings.TrimSpace(event.SubscriptionVersion)
	event.PayloadRef = strings.TrimSpace(event.PayloadRef)
	if event.EventID == "" || event.SubscriptionID == "" || event.SubscriptionVersion == "" {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": "event_id, subscription_id and subscription_version are required"})
		return
	}
	s.publish(event)
	writeJSON(w, http.StatusOK, map[string]any{"status": "ok"})
}

func (s *Server) handleCloseSubscription(w http.ResponseWriter, r *http.Request) {
	if !s.authorized(r) {
		writeJSON(w, http.StatusUnauthorized, map[string]any{"error": "unauthorized"})
		return
	}
	subscriptionID := strings.TrimSpace(r.PathValue("subscription_id"))
	subscriptionVersion := subscriptionVersionFromQuery(r)
	if subscriptionVersion == "" && r.Body != nil {
		var req closeSubscriptionRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil && !errors.Is(err, io.EOF) {
			writeJSON(w, http.StatusBadRequest, map[string]any{"error": "invalid json"})
			return
		}
		subscriptionVersion = strings.TrimSpace(req.SubscriptionVersion)
	}
	if subscriptionID == "" || subscriptionVersion == "" {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": "subscription_id and subscription_version are required"})
		return
	}
	closed := s.closeSubscription(subscriptionKey(subscriptionID, subscriptionVersion))
	writeJSON(w, http.StatusOK, map[string]any{"status": "ok", "closed_clients": closed})
}

func (s *Server) handleSSE(w http.ResponseWriter, r *http.Request) {
	subscriptionID := strings.TrimSpace(r.URL.Query().Get("subscription_id"))
	subscriptionVersion := subscriptionVersionFromQuery(r)
	token := strings.TrimSpace(r.URL.Query().Get("token"))
	if subscriptionID == "" || subscriptionVersion == "" || token == "" {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": "subscription_id, subscription_version and token are required"})
		return
	}
	flusher, ok := w.(http.Flusher)
	if !ok {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": "streaming is not supported"})
		return
	}

	validation, err := s.validateWithCloud(r.Context(), subscriptionID, subscriptionVersion, token)
	if err != nil {
		log.Printf("cloud validation failed: subscription=%s version=%s err=%v", subscriptionID, subscriptionVersion, err)
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": "cloud validation failed"})
		return
	}
	if !validation.Valid {
		writeJSON(w, http.StatusUnauthorized, map[string]any{"error": "invalid subscription token"})
		return
	}

	key := subscriptionKey(subscriptionID, subscriptionVersion)
	if len(validation.PendingEvents) > 0 {
		s.clearLatest(key)
	}
	eventCh := s.registerSSEClient(key)
	defer s.unregisterSSEClient(key, eventCh)

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("X-Accel-Buffering", "no")

	_, _ = w.Write([]byte(": connected\n\n"))
	flusher.Flush()

	for _, event := range normalizeEvents(validation.PendingEvents, subscriptionID, subscriptionVersion) {
		if !writeSSEEvent(w, flusher, event) {
			return
		}
	}

	heartbeat := s.cfg.SSEHeartbeatInterval
	if heartbeat <= 0 {
		heartbeat = 15 * time.Second
	}
	ticker := time.NewTicker(heartbeat)
	defer ticker.Stop()

	for {
		select {
		case event, ok := <-eventCh:
			if !ok {
				return
			}
			if !writeSSEEvent(w, flusher, event) {
				return
			}
		case <-ticker.C:
			if _, err := w.Write([]byte(": heartbeat\n\n")); err != nil {
				return
			}
			flusher.Flush()
		case <-r.Context().Done():
			return
		}
	}
}

func (s *Server) validateWithCloud(ctx context.Context, subscriptionID string, subscriptionVersion string, token string) (*cloudValidationResponse, error) {
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
	if subscriptionVersion != "" {
		query.Set("subscription_version", subscriptionVersion)
	}
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

func (s *Server) publish(event Event) {
	key := event.key()
	s.mu.Lock()
	defer s.mu.Unlock()
	clients := s.sseClients[key]
	if len(clients) == 0 {
		s.latest[key] = event
		return
	}
	for client := range clients {
		sendLatest(client, event)
	}
}

func (s *Server) clearLatest(key string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.latest, key)
}

func (s *Server) popLatest(key string) (Event, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	event, ok := s.latest[key]
	if ok {
		delete(s.latest, key)
	}
	return event, ok
}

func (s *Server) registerSSEClient(key string) chan Event {
	ch := make(chan Event, 1)
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.sseClients[key] == nil {
		s.sseClients[key] = make(map[chan Event]struct{})
	}
	s.sseClients[key][ch] = struct{}{}
	if event, ok := s.latest[key]; ok {
		delete(s.latest, key)
		sendLatest(ch, event)
	}
	return ch
}

func (s *Server) unregisterSSEClient(key string, ch chan Event) {
	s.mu.Lock()
	defer s.mu.Unlock()
	clients := s.sseClients[key]
	if len(clients) == 0 {
		return
	}
	delete(clients, ch)
	close(ch)
	if len(clients) == 0 {
		delete(s.sseClients, key)
	}
}

func (s *Server) closeSubscription(key string) int {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.latest, key)
	clients := s.sseClients[key]
	if len(clients) == 0 {
		return 0
	}
	delete(s.sseClients, key)
	for ch := range clients {
		drainEventChannel(ch)
		close(ch)
	}
	return len(clients)
}

func (s *Server) authorized(r *http.Request) bool {
	expected := strings.TrimSpace(s.cfg.InternalToken)
	if expected == "" {
		return true
	}
	auth := strings.TrimSpace(r.Header.Get("Authorization"))
	return auth == expected || auth == bearerAuth(expected)
}

func (e Event) key() string {
	return subscriptionKey(e.SubscriptionID, e.SubscriptionVersion)
}

func subscriptionKey(subscriptionID string, subscriptionVersion string) string {
	subscriptionID = strings.TrimSpace(subscriptionID)
	subscriptionVersion = strings.TrimSpace(subscriptionVersion)
	if subscriptionVersion == "" {
		return subscriptionID
	}
	return subscriptionID + ":" + subscriptionVersion
}

func subscriptionVersionFromQuery(r *http.Request) string {
	version := strings.TrimSpace(r.URL.Query().Get("subscription_version"))
	if version == "" {
		version = strings.TrimSpace(r.URL.Query().Get("version"))
	}
	return version
}

func normalizeEvents(events []Event, subscriptionID string, subscriptionVersion string) []Event {
	if len(events) == 0 {
		return nil
	}
	result := make([]Event, 0, len(events))
	for _, event := range events {
		event.EventID = strings.TrimSpace(event.EventID)
		if event.SubscriptionID == "" {
			event.SubscriptionID = strings.TrimSpace(subscriptionID)
		}
		if event.SubscriptionVersion == "" {
			event.SubscriptionVersion = strings.TrimSpace(subscriptionVersion)
		}
		event.PayloadRef = strings.TrimSpace(event.PayloadRef)
		result = append(result, event)
	}
	return result
}

func sendLatest(ch chan Event, event Event) {
	select {
	case ch <- event:
		return
	default:
	}
	select {
	case <-ch:
	default:
	}
	select {
	case ch <- event:
	default:
	}
}

func drainEventChannel(ch chan Event) {
	for {
		select {
		case <-ch:
			continue
		default:
			return
		}
	}
}

func writeSSEEvent(w http.ResponseWriter, flusher http.Flusher, event Event) bool {
	data, err := json.Marshal(event)
	if err != nil {
		return false
	}
	if _, err := fmt.Fprintf(w, "event: birthday_monitor_update\ndata: %s\n\n", data); err != nil {
		return false
	}
	flusher.Flush()
	return true
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
		Addr:                 addr,
		InternalToken:        os.Getenv("HMES_INTERNAL_TOKEN"),
		CloudBaseURL:         os.Getenv("HMES_CLOUD_INTERNAL_BASE_URL"),
		CloudToken:           os.Getenv("HMES_CLOUD_INTERNAL_TOKEN"),
		UserAgent:            envString("HMES_USER_AGENT", "Haruki-HMES"),
		SSEHeartbeatInterval: time.Duration(envInt("HMES_SSE_HEARTBEAT_SECONDS", 15)) * time.Second,
		CloudHTTPTimeout:     time.Duration(envInt("HMES_CLOUD_TIMEOUT_SECONDS", 5)) * time.Second,
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
