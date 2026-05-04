// Yomie Agent — CDAP device agent for system monitoring, terminal,
// file browser, and clipboard sync.
//
// Usage:
//
//	yomie-agent -server ws://host:21122/cdap -auth api_key -key YOUR_KEY
//	yomie-agent -config /etc/yomie-agent.json
package main

import (
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/unitronix/betterdesk-agent/agent"
)

var version = "1.0.0"

func main() {
	var (
		configFile = flag.String("config", "", "Config file path (JSON)")
		server     = flag.String("server", "", "Gateway WebSocket URL (ws://host:21122/cdap)")
		authMethod = flag.String("auth", "", "Auth method: api_key, device_token, user_password")
		apiKey     = flag.String("key", "", "API key")
		devToken   = flag.String("token", "", "Device enrollment token")
		username   = flag.String("user", "", "Username")
		password   = flag.String("pass", "", "Password")
		deviceID   = flag.String("device-id", "", "Device ID (default: auto)")
		deviceName = flag.String("device-name", "", "Device name (default: hostname)")
		deviceType = flag.String("device-type", "", "Device type (default: os_agent)")
		logLevel   = flag.String("log-level", "", "Log level: debug, info, warning, error")
		dataDir    = flag.String("data-dir", "", "Data directory")
		showVer    = flag.Bool("version", false, "Print version and exit")
	)
	flag.Parse()

	if *showVer {
		fmt.Printf("yomie-agent %s\n", version)
		os.Exit(0)
	}

	cfg, err := agent.LoadConfig(*configFile)
	if err != nil {
		log.Fatalf("Config error: %v", err)
	}

	// CLI flags override config file
	if *server != "" {
		cfg.Server = *server
	}
	if *authMethod != "" {
		cfg.AuthMethod = *authMethod
	}
	if *apiKey != "" {
		cfg.APIKey = *apiKey
	}
	if *devToken != "" {
		cfg.DeviceToken = *devToken
	}
	if *username != "" {
		cfg.Username = *username
	}
	if *password != "" {
		cfg.Password = *password
	}
	if *deviceID != "" {
		cfg.DeviceID = *deviceID
	}
	if *deviceName != "" {
		cfg.DeviceName = *deviceName
	}
	if *deviceType != "" {
		cfg.DeviceType = *deviceType
	}
	if *logLevel != "" {
		cfg.LogLevel = *logLevel
	}
	if *dataDir != "" {
		cfg.DataDir = *dataDir
	}

	if err := cfg.Validate(); err != nil {
		log.Fatalf("Config validation: %v", err)
	}

	log.Printf("[agent] Yomie Agent %s starting (device: %s, type: %s)", version, cfg.DeviceName, cfg.DeviceType)

	a := agent.New(cfg, version)

	// Graceful shutdown on SIGINT/SIGTERM
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-sigCh
		log.Println("[agent] Shutting down...")
		a.Stop()
	}()

	if err := a.Run(); err != nil {
		log.Fatalf("[agent] Fatal: %v", err)
	}
}
