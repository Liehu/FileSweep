package server

import (
	"encoding/json"
	"log/slog"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

type WSEvent struct {
	Type      string `json:"type"`
	Payload   any    `json:"payload"`
	Timestamp string `json:"timestamp"`
}

type clientEntry struct {
	writeMu sync.Mutex
}

type Hub struct {
	mu      sync.RWMutex
	clients map[*websocket.Conn]*clientEntry
}

func NewHub() *Hub {
	return &Hub{clients: make(map[*websocket.Conn]*clientEntry)}
}

func (h *Hub) Register(conn *websocket.Conn) {
	h.mu.Lock()
	h.clients[conn] = &clientEntry{}
	h.mu.Unlock()
	slog.Info("WebSocket 客户端已连接", "total", h.count())
}

func (h *Hub) Unregister(conn *websocket.Conn) {
	h.mu.Lock()
	delete(h.clients, conn)
	h.mu.Unlock()
	conn.Close()
	slog.Info("WebSocket 客户端已断开", "total", h.count())
}

func (h *Hub) count() int {
	return len(h.clients)
}

func (h *Hub) Broadcast(eventType string, payload any) {
	event := WSEvent{
		Type:      eventType,
		Payload:   payload,
		Timestamp: time.Now().UTC().Format(time.RFC3339),
	}
	data, err := json.Marshal(event)
	if err != nil {
		slog.Error("序列化事件失败", "error", err)
		return
	}

	h.mu.RLock()
	defer h.mu.RUnlock()

	for conn, entry := range h.clients {
		entry.writeMu.Lock()
		err := conn.WriteMessage(websocket.TextMessage, data)
		entry.writeMu.Unlock()
		if err != nil {
			slog.Warn("发送 WebSocket 消息失败", "error", err)
		}
	}
}
