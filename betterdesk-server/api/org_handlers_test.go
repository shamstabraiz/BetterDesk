package api

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"testing"
	"time"

	"github.com/unitronix/betterdesk-server/config"
	"github.com/unitronix/betterdesk-server/db"
	"github.com/unitronix/betterdesk-server/peer"
)

func TestAssignUserToOrgAcceptsNumericOrgID(t *testing.T) {
	cfg := config.DefaultConfig()
	database := testSetupDB(t)
	defer database.Close()

	if err := database.CreateOrganization(&db.Organization{
		ID:        "42",
		Name:      "Acme",
		Slug:      "acme",
		CreatedAt: time.Now().UTC(),
	}); err != nil {
		t.Fatal(err)
	}
	user := &db.User{Username: "alice", PasswordHash: "hash", Role: "operator"}
	if err := database.CreateUser(user); err != nil {
		t.Fatal(err)
	}

	cfg.APIPort = 19887
	srv := New(cfg, database, peer.NewMap(), nil, "test")
	if err := srv.Start(t.Context()); err != nil {
		t.Fatal(err)
	}
	defer srv.Stop()
	time.Sleep(100 * time.Millisecond)

	body := bytes.NewBufferString(`{"org_id":42,"role":"operator"}`)
	req, err := http.NewRequest("POST", fmt.Sprintf("http://127.0.0.1:%d/api/users/%d/organizations", cfg.APIPort, user.ID), body)
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Content-Type", "application/json")
	testAuthReq(req)

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusCreated {
		t.Fatalf("expected 201 Created, got %d", resp.StatusCode)
	}

	var orgUser db.OrgUser
	if err := json.NewDecoder(resp.Body).Decode(&orgUser); err != nil {
		t.Fatal(err)
	}
	if orgUser.OrgID != "42" || orgUser.ServerUserID != user.ID || orgUser.Role != "operator" {
		t.Fatalf("unexpected org user: %+v", orgUser)
	}
}

func TestListUserOrganizationsIncludesMembershipShape(t *testing.T) {
	cfg := config.DefaultConfig()
	database := testSetupDB(t)
	defer database.Close()

	orgs := []db.Organization{
		{ID: "org-a", Name: "Acme", Slug: "acme", CreatedAt: time.Now().UTC()},
		{ID: "org-b", Name: "Beta", Slug: "beta", CreatedAt: time.Now().UTC()},
	}
	for i := range orgs {
		if err := database.CreateOrganization(&orgs[i]); err != nil {
			t.Fatal(err)
		}
	}

	user := &db.User{Username: "alice", PasswordHash: "hash", Role: "operator"}
	if err := database.CreateUser(user); err != nil {
		t.Fatal(err)
	}
	if _, err := database.LinkUserToOrg("org-a", user.ID, "operator"); err != nil {
		t.Fatal(err)
	}
	if _, err := database.LinkUserToOrg("org-b", user.ID, "admin"); err != nil {
		t.Fatal(err)
	}

	cfg.APIPort = 19888
	srv := New(cfg, database, peer.NewMap(), nil, "test")
	if err := srv.Start(t.Context()); err != nil {
		t.Fatal(err)
	}
	defer srv.Stop()
	time.Sleep(100 * time.Millisecond)

	req, err := http.NewRequest("GET", fmt.Sprintf("http://127.0.0.1:%d/api/users/%d/organizations", cfg.APIPort, user.ID), nil)
	if err != nil {
		t.Fatal(err)
	}
	testAuthReq(req)

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("expected 200 OK, got %d", resp.StatusCode)
	}

	var body struct {
		Organizations []struct {
			ID      string `json:"id"`
			OrgID   string `json:"org_id"`
			Name    string `json:"name"`
			OrgName string `json:"org_name"`
			Role    string `json:"role"`
		} `json:"organizations"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}

	if len(body.Organizations) != 2 {
		t.Fatalf("expected 2 organizations, got %d", len(body.Organizations))
	}
	rolesByOrg := map[string]string{}
	for _, org := range body.Organizations {
		if org.ID == "" || org.OrgID == "" || org.Name == "" || org.OrgName == "" {
			t.Fatalf("missing compatibility fields in org response: %+v", org)
		}
		rolesByOrg[org.OrgID] = org.Role
	}
	if rolesByOrg["org-a"] != "operator" || rolesByOrg["org-b"] != "admin" {
		t.Fatalf("unexpected org roles: %+v", rolesByOrg)
	}
}
