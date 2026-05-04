package events

import (
	"testing"
	"time"
)

func TestSubscribeAndPublish(t *testing.T) {
	bus := NewBus()

	sub := bus.Subscribe("") // all events
	defer bus.Unsubscribe(sub)

	bus.Publish(Event{
		Type: EventPeerOnline,
		Data: map[string]string{"id": "ABC123"},
	})

	select {
	case evt := <-sub.Ch:
		if evt.Type != EventPeerOnline {
			t.Errorf("expected peer_online, got %s", evt.Type)
		}
		if evt.Data["id"] != "ABC123" {
			t.Errorf("expected id ABC123, got %s", evt.Data["id"])
		}
		if evt.Timestamp.IsZero() {
			t.Error("timestamp should be set")
		}
	case <-time.After(time.Second):
		t.Fatal("timeout waiting for event")
	}
}

func TestFilteredSubscription(t *testing.T) {
	bus := NewBus()

	subBans := bus.Subscribe(EventPeerBanned)
	subAll := bus.Subscribe("")
	defer bus.Unsubscribe(subBans)
	defer bus.Unsubscribe(subAll)

	bus.Publish(Event{Type: EventPeerOnline, Data: map[string]string{"id": "A"}})
	bus.Publish(Event{Type: EventPeerBanned, Data: map[string]string{"id": "B"}})

	// subAll should receive both
	select {
	case evt := <-subAll.Ch:
		if evt.Type != EventPeerOnline {
			t.Errorf("expected peer_online, got %s", evt.Type)
		}
	case <-time.After(time.Second):
		t.Fatal("timeout")
	}
	select {
	case evt := <-subAll.Ch:
		if evt.Type != EventPeerBanned {
			t.Errorf("expected peer_banned, got %s", evt.Type)
		}
	case <-time.After(time.Second):
		t.Fatal("timeout")
	}

	// subBans should only receive the ban event
	select {
	case evt := <-subBans.Ch:
		if evt.Type != EventPeerBanned {
			t.Errorf("expected peer_banned, got %s", evt.Type)
		}
	case <-time.After(time.Second):
		t.Fatal("timeout")
	}
}

func TestUnsubscribe(t *testing.T) {
	bus := NewBus()

	sub := bus.Subscribe("")
	if bus.Count() != 1 {
		t.Fatalf("expected 1 subscriber, got %d", bus.Count())
	}

	bus.Unsubscribe(sub)
	if bus.Count() != 0 {
		t.Fatalf("expected 0 subscribers, got %d", bus.Count())
	}

	// Channel should be closed
	_, ok := <-sub.Ch
	if ok {
		t.Error("expected channel to be closed")
	}
}

func TestSlowSubscriberDropsEvents(t *testing.T) {
	bus := NewBus()

	sub := bus.Subscribe("")
	defer bus.Unsubscribe(sub)

	// Fill the buffer (64 capacity)
	for i := 0; i < 100; i++ {
		bus.Publish(Event{Type: EventServerStats})
	}

	// Should have 64 events in channel (rest dropped)
	if len(sub.Ch) != 64 {
		t.Errorf("expected 64 buffered events, got %d", len(sub.Ch))
	}
}

func TestMarshalEvent(t *testing.T) {
	evt := Event{
		Type:      EventPeerOnline,
		Timestamp: time.Now(),
		Data:      map[string]string{"id": "TEST"},
	}
	data := MarshalEvent(evt)
	if len(data) == 0 {
		t.Error("expected non-empty JSON")
	}
}
