package cdap

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"log"
	"time"

	"github.com/coder/websocket"
	"github.com/unitronix/betterdesk-server/audit"
	"github.com/unitronix/betterdesk-server/auth"
)

// handleAuth reads the initial "auth" message from the client, validates
// credentials, and returns a DeviceConn on success.
func (g *Gateway) handleAuth(ctx context.Context, conn *websocket.Conn, clientIP string) (*DeviceConn, error) {
	msg, err := readMessage(ctx, conn)
	if err != nil {
		return nil, fmt.Errorf("read auth message: %w", err)
	}
	if msg.Type != "auth" {
		return nil, fmt.Errorf("expected 'auth' message, got '%s'", msg.Type)
	}

	var payload AuthPayload
	if err := json.Unmarshal(msg.Payload, &payload); err != nil {
		return nil, fmt.Errorf("invalid auth payload: %w", err)
	}

	// Rate limit check for this IP
	if g.limiter != nil && !g.limiter.Allow(clientIP) {
		g.auditAction("cdap_auth_ratelimited", clientIP, map[string]string{
			"method": payload.Method,
		})
		return nil, fmt.Errorf("rate limit exceeded")
	}

	var username, role string

	switch payload.Method {
	case "user_password":
		u, r, authErr := g.authUserPassword(payload, clientIP)
		if authErr != nil {
			return nil, authErr
		}
		username, role = u, r

	case "api_key":
		u, r, authErr := g.authAPIKey(payload, clientIP)
		if authErr != nil {
			return nil, authErr
		}
		username, role = u, r

	case "device_token":
		u, r, authErr := g.authDeviceToken(payload, clientIP)
		if authErr != nil {
			return nil, authErr
		}
		username, role = u, r

	default:
		return nil, fmt.Errorf("unsupported auth method: %s", payload.Method)
	}

	// Generate session token
	sessionID, _ := auth.GenerateRandomString(16)

	// Generate JWT for the device
	token, err := g.jwt.Generate(username, role)
	if err != nil {
		return nil, fmt.Errorf("generate token: %w", err)
	}

	dc := &DeviceConn{
		ID:                payload.DeviceID,
		Username:          username,
		Role:              role,
		ClientIP:          clientIP,
		conn:              conn,
		Token:             token,
		TokenExpiry:       time.Now().Add(g.jwt.Expiry()),
		SessionID:         sessionID,
		ConnectedAt:       time.Now(),
		LastHeartbeat:     time.Now(),
		HeartbeatInterval: 15, // default, updated after manifest registration
	}

	// Send auth_result
	result := AuthResult{
		Success:      true,
		Token:        token,
		Role:         role,
		DeviceID:     payload.DeviceID,
		SessionToken: sessionID,
	}
	if err := sendMessage(ctx, conn, "auth_result", result); err != nil {
		return nil, fmt.Errorf("send auth_result: %w", err)
	}

	g.auditAction("cdap_auth_success", clientIP, map[string]string{
		"device_id": payload.DeviceID,
		"username":  username,
		"role":      role,
		"method":    payload.Method,
	})

	log.Printf("[cdap] Authenticated %s (user=%s, role=%s, method=%s, ip=%s)",
		payload.DeviceID, username, role, payload.Method, clientIP)

	return dc, nil
}

// authUserPassword verifies username/password credentials.
func (g *Gateway) authUserPassword(p AuthPayload, clientIP string) (string, string, error) {
	if p.Username == "" || p.Password == "" {
		return "", "", fmt.Errorf("username and password required")
	}

	user, err := g.db.GetUser(p.Username)
	if err != nil || user == nil {
		// Timing-safe: always call VerifyPassword even for non-existent users
		auth.VerifyPassword("dummy:hash", p.Password)
		g.auditAction("cdap_auth_failed", clientIP, map[string]string{
			"username": p.Username,
			"reason":   "invalid credentials",
		})
		return "", "", fmt.Errorf("invalid credentials")
	}

	if !auth.VerifyPassword(user.PasswordHash, p.Password) {
		g.auditAction("cdap_auth_failed", clientIP, map[string]string{
			"username": p.Username,
			"reason":   "invalid password",
		})
		return "", "", fmt.Errorf("invalid credentials")
	}

	// Check 2FA if enabled
	if user.TOTPEnabled {
		if p.TOTPCode == "" {
			return "", "", fmt.Errorf("2fa_required")
		}
		if !auth.ValidateTOTP(user.TOTPSecret, p.TOTPCode) {
			g.auditAction("cdap_auth_failed", clientIP, map[string]string{
				"username": p.Username,
				"reason":   "invalid 2fa code",
			})
			return "", "", fmt.Errorf("invalid 2FA code")
		}
	}

	return user.Username, user.Role, nil
}

// authAPIKey verifies an API key.
func (g *Gateway) authAPIKey(p AuthPayload, clientIP string) (string, string, error) {
	if p.Key == "" {
		return "", "", fmt.Errorf("api key required")
	}

	h := sha256.Sum256([]byte(p.Key))
	keyHash := hex.EncodeToString(h[:])

	apiKey, err := g.db.GetAPIKeyByHash(keyHash)
	if err != nil || apiKey == nil {
		g.auditAction("cdap_auth_failed", clientIP, map[string]string{
			"reason": "invalid api key",
		})
		return "", "", fmt.Errorf("invalid API key")
	}

	// Touch last_used
	g.db.TouchAPIKey(apiKey.ID)

	return fmt.Sprintf("apikey:%s", apiKey.Name), apiKey.Role, nil
}

// authDeviceToken verifies a device enrollment token.
func (g *Gateway) authDeviceToken(p AuthPayload, clientIP string) (string, string, error) {
	if p.Token == "" {
		return "", "", fmt.Errorf("device token required")
	}

	h := sha256.Sum256([]byte(p.Token))
	tokenHash := hex.EncodeToString(h[:])

	dt, err := g.db.ValidateToken(tokenHash)
	if err != nil || dt == nil {
		g.auditAction("cdap_auth_failed", clientIP, map[string]string{
			"reason": "invalid device token",
		})
		return "", "", fmt.Errorf("invalid or expired device token")
	}

	// Bind token to device if not already bound
	if dt.PeerID == "" && p.DeviceID != "" {
		g.db.BindTokenToPeer(tokenHash, p.DeviceID)
	}

	// Increment usage
	g.db.IncrementTokenUse(tokenHash)

	return fmt.Sprintf("token:%s", dt.Name), "operator", nil
}

// auditAction logs a CDAP action to the audit log.
func (g *Gateway) auditAction(action, target string, details map[string]string) {
	if g.auditLog == nil {
		return
	}
	if details == nil {
		details = make(map[string]string)
	}
	details["source"] = "cdap"
	g.auditLog.Log(audit.Action(action), "cdap", target, details)
}

// readMessage reads a single CDAP protocol message from a bare websocket connection.
func readMessage(ctx context.Context, conn *websocket.Conn) (*Message, error) {
	typ, data, err := conn.Read(ctx)
	if err != nil {
		return nil, err
	}
	if typ != websocket.MessageText {
		return nil, fmt.Errorf("expected text frame, got %v", typ)
	}
	var msg Message
	if err := json.Unmarshal(data, &msg); err != nil {
		return nil, fmt.Errorf("invalid JSON: %w", err)
	}
	return &msg, nil
}
